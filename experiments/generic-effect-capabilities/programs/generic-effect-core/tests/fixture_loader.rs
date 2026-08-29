mod common;

use anchor_lang::prelude::Pubkey;
use common::{
    advance_one_slot, deploy_fixed_id_mutable_program, extend_mutable_program, fixture_keypair,
    loader_v3_test_vm, must_send_legacy, prepare_upgrade_buffer, read_anchor_account,
    read_program_data_state, send_legacy_failure, snapshot_accounts, upgrade_mutable_program,
    DirectFixture, MutableProgramDeployment, SbfArtifacts, LOADER_EXTEND_BYTES,
};
use generic_effect_private_wire::{
    CoreControlInstructionCandidateV0, EngineAdmissionPolicyCandidateV0, ENGINE_POLICY_IMMUTABLE,
};
use litesvm::LiteSVM;
use solana_loader_v3_interface::{
    get_program_data_address, instruction as loader_v3_instruction, state::UpgradeableLoaderState,
};
use solana_message::{AccountMeta, Instruction};
use solana_native_token::LAMPORTS_PER_SOL;
use solana_signer::Signer;
use solana_transaction::{InstructionError, TransactionError};

use programmable_generic_effect_core::{
    error::CoreError, state::ImmutableEngineReleaseCandidateV0,
};

#[test]
fn mutable_controller_state_only_execution_uses_current_programdata_snapshot() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = DirectFixture::state_only_mutable(&artifacts);
    assert_eq!(fixture.loader_policy_account, fixture.engine_program_data);
    assert_eq!(fixture.engine_controller, Some(fixture.payer.pubkey()));
    assert!(fixture.svm.get_sysvar::<solana_clock::Clock>().slot > fixture.engine_programdata_slot);
    let protected = fixture.rollback_state_addresses();
    let before = snapshot_accounts(&fixture.svm, &protected);
    let (transaction, _) = fixture.compile_v0();
    let metadata = fixture
        .svm
        .send_transaction(transaction)
        .unwrap_or_else(|failure| {
            panic!(
                "mutable-controller state-only execution failed: {:?}\n{}",
                failure.err,
                failure.meta.pretty_logs()
            )
        });
    assert!(program_invoked(&metadata.logs, effect_engine_probe::ID));
    assert_eq!(snapshot_accounts(&fixture.svm, &protected), before);
}

#[test]
fn mutable_controller_same_slot_and_pinned_policy_are_rejected_before_engine() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");

    let mut same_slot = DirectFixture::state_only_mutable_same_slot(&artifacts);
    let deployment = MutableProgramDeployment {
        program_id: effect_engine_probe::ID,
        program_data: same_slot.engine_program_data,
        deployment_slot: same_slot.engine_programdata_slot,
        program_data_len: same_slot
            .svm
            .get_account(&same_slot.engine_program_data)
            .expect("same-slot mutable ProgramData exists")
            .data
            .len(),
    };
    let predicted_slot = same_slot.prepare_next_slot_mutable_loader_snapshot();
    let (same_slot_transaction, _) = same_slot.compile_v0();
    assert_eq!(
        same_slot.svm.get_sysvar::<solana_clock::Clock>().slot,
        predicted_slot,
        "ALT warming must land on the pre-committed loader slot"
    );
    let extended = extend_mutable_program(
        &mut same_slot.svm,
        &same_slot.payer,
        &deployment,
        LOADER_EXTEND_BYTES,
    );
    assert_eq!(
        extended.deployment_slot, predicted_slot,
        "real loader-v3 mutation did not land in the pre-committed slot"
    );
    let protected = same_slot.rollback_state_addresses();
    let before = snapshot_accounts(&same_slot.svm, &protected);
    let failure = same_slot
        .svm
        .send_transaction(same_slot_transaction)
        .expect_err("same-slot mutable snapshot unexpectedly executed");
    assert_eq!(
        failure.err,
        TransactionError::InstructionError(
            2,
            core_instruction_error(CoreError::SameSlotEngineObservation),
        ),
        "same-slot fixture must reach Core's strict earlier-slot gate"
    );
    assert!(
        !program_invoked(&failure.meta.logs, effect_engine_probe::ID),
        "same-slot mutable snapshot crossed the Engine boundary:\n{}",
        failure.meta.pretty_logs()
    );
    assert_eq!(
        snapshot_accounts(&same_slot.svm, &protected),
        before,
        "same-slot mutable snapshot changed protected state"
    );

    let mut pinned = DirectFixture::state_only_pinned_mutable(&artifacts);
    let protected = pinned.rollback_state_addresses();
    let before = snapshot_accounts(&pinned.svm, &protected);
    let (pinned_transaction, _) = pinned.compile_v0();
    let failure = pinned
        .svm
        .send_transaction(pinned_transaction)
        .expect_err("pinned mutable policy unexpectedly executed");
    assert!(
        !program_invoked(&failure.meta.logs, effect_engine_probe::ID),
        "pinned mutable policy crossed the Engine boundary:\n{}",
        failure.meta.pretty_logs()
    );
    assert_eq!(
        snapshot_accounts(&pinned.svm, &protected),
        before,
        "pinned mutable policy changed protected state"
    );
}

#[test]
fn mutable_controller_change_invalidates_the_bound_market_policy() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = DirectFixture::state_only_mutable(&artifacts);
    let replacement_controller = fixture_keypair(214);
    let change = loader_v3_instruction::set_upgrade_authority(
        &effect_engine_probe::ID,
        &fixture.payer.pubkey(),
        Some(&replacement_controller.pubkey()),
    );
    must_send_legacy(
        &mut fixture.svm,
        &fixture.payer,
        &[change],
        &[],
        "real loader-v3 controller change",
    );
    match read_program_data_state(&fixture.svm, &fixture.engine_program_data) {
        UpgradeableLoaderState::ProgramData {
            upgrade_authority_address,
            ..
        } => assert_eq!(
            upgrade_authority_address,
            Some(replacement_controller.pubkey())
        ),
        other => panic!("controller change produced unexpected ProgramData: {other:?}"),
    }
    let protected = fixture.rollback_state_addresses();
    let before = snapshot_accounts(&fixture.svm, &protected);
    let (transaction, _) = fixture.compile_v0();
    let failure = fixture
        .svm
        .send_transaction(transaction)
        .expect_err("changed mutable controller matched the old market policy");
    assert!(!program_invoked(
        &failure.meta.logs,
        effect_engine_probe::ID
    ));
    assert_eq!(snapshot_accounts(&fixture.svm, &protected), before);
}

#[test]
fn mutable_extend_and_upgrade_invalidate_old_snapshot_then_fresh_rebind_executes() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    for mutation in ["extend", "upgrade"] {
        let mut fixture = DirectFixture::state_only_mutable(&artifacts);
        let old_snapshot = fixture
            .envelope
            .header
            .expected_engine_loader_state_snapshot_digest;
        let deployment = MutableProgramDeployment {
            program_id: effect_engine_probe::ID,
            program_data: fixture.engine_program_data,
            deployment_slot: fixture.engine_programdata_slot,
            program_data_len: fixture
                .svm
                .get_account(&fixture.engine_program_data)
                .expect("mutable ProgramData exists")
                .data
                .len(),
        };
        let changed = if mutation == "extend" {
            extend_mutable_program(
                &mut fixture.svm,
                &fixture.payer,
                &deployment,
                LOADER_EXTEND_BYTES,
            )
        } else {
            let buffer = prepare_upgrade_buffer(
                &mut fixture.svm,
                &fixture.payer,
                &artifacts.replacement_engine,
                215,
            );
            upgrade_mutable_program(
                &mut fixture.svm,
                &fixture.payer,
                &deployment,
                buffer,
                &artifacts.replacement_engine,
            )
        };
        assert!(
            changed.deployment_slot > fixture.engine_programdata_slot,
            "real loader-v3 {mutation} did not advance ProgramData.slot"
        );
        advance_one_slot(&mut fixture.svm);

        let protected = fixture.rollback_state_addresses();
        let before = snapshot_accounts(&fixture.svm, &protected);
        let (old_transaction, _) = fixture.compile_v0();
        let old_failure = match fixture.svm.send_transaction(old_transaction) {
            Ok(_) => panic!("old snapshot survived mutable {mutation}"),
            Err(failure) => failure,
        };
        assert!(
            !program_invoked(&old_failure.meta.logs, effect_engine_probe::ID),
            "old {mutation} snapshot crossed the Engine boundary"
        );
        assert_eq!(snapshot_accounts(&fixture.svm, &protected), before);

        fixture.refresh_mutable_loader_snapshot();
        assert_ne!(
            fixture
                .envelope
                .header
                .expected_engine_loader_state_snapshot_digest,
            old_snapshot,
            "fresh {mutation} snapshot digest did not change"
        );
        let (fresh_transaction, _) = fixture.compile_v0();
        let metadata = fixture
            .svm
            .send_transaction(fresh_transaction)
            .unwrap_or_else(|failure| {
                panic!(
                    "fresh snapshot after mutable {mutation} failed: {:?}\n{}",
                    failure.err,
                    failure.meta.pretty_logs()
                )
            });
        assert!(program_invoked(&metadata.logs, effect_engine_probe::ID));
    }
}

#[test]
fn virtual_loader_and_core_identities_cannot_enter_the_opaque_tail() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let core_program_data = get_program_data_address(&programmable_generic_effect_core::ID);
    for identity in ["engine-programdata", "core-program", "core-programdata"] {
        for writable in [false, true] {
            let mut fixture = DirectFixture::state_only(&artifacts);
            let alias = match identity {
                "engine-programdata" => fixture.engine_program_data,
                "core-program" => programmable_generic_effect_core::ID,
                "core-programdata" => core_program_data,
                _ => unreachable!(),
            };
            fixture.append_opaque_account(alias, writable);
            let mut protected = fixture.rollback_state_addresses().to_vec();
            protected.push(alias);
            let before = snapshot_accounts(&fixture.svm, &protected);
            let (transaction, _) = fixture.compile_v0();
            let failure = fixture
                .svm
                .send_transaction(transaction)
                .expect_err("virtual protected identity reached the opaque Engine tail");
            assert!(program_invoked(
                &failure.meta.logs,
                programmable_generic_effect_core::ID
            ));
            assert!(
                !program_invoked(&failure.meta.logs, effect_engine_probe::ID),
                "{identity} writable={writable} reached Engine:\n{}",
                failure.meta.pretty_logs()
            );
            assert_eq!(
                snapshot_accounts(&fixture.svm, &protected),
                before,
                "{identity} writable={writable} changed protected state"
            );
        }
    }
}

#[test]
fn core_capture_is_real_exact_idempotent_and_loader_union_safe() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut svm = LiteSVM::new();
    artifacts.install_cached_programs(&mut svm);
    let payer = fixture_keypair(210);
    svm.airdrop(&payer.pubkey(), 100 * LAMPORTS_PER_SOL)
        .expect("fund immutable-release capture payer");

    let (capture, release, program_data, captured_slot) = immutable_capture(&svm, payer.pubkey());
    advance_one_slot(&mut svm);
    let metadata = must_send_legacy(
        &mut svm,
        &payer,
        std::slice::from_ref(&capture),
        &[],
        "capture exact immutable Engine release",
    );
    assert!(metadata.logs.iter().any(|line| line.starts_with(&format!(
        "Program {} invoke",
        programmable_generic_effect_core::ID
    ))));

    let observed: ImmutableEngineReleaseCandidateV0 = read_anchor_account(&svm, &release);
    observed
        .validate(&programmable_generic_effect_core::ID, &release)
        .expect("captured release validates against its exact Core PDA");
    assert_eq!(observed.engine_program, effect_engine_probe::ID);
    assert_eq!(observed.canonical_program_data, program_data);
    assert_eq!(observed.captured_programdata_slot, captured_slot);
    assert_eq!(observed.observed_controller_or_zero, Pubkey::default());

    let before_idempotent = snapshot_accounts(&svm, &[release]);
    svm.expire_blockhash();
    must_send_legacy(
        &mut svm,
        &payer,
        std::slice::from_ref(&capture),
        &[],
        "repeat exact immutable Engine release capture",
    );
    assert_eq!(snapshot_accounts(&svm, &[release]), before_idempotent);

    let mut tainted_svm = LiteSVM::new();
    artifacts.install_cached_programs(&mut tainted_svm);
    tainted_svm
        .airdrop(&payer.pubkey(), 100 * LAMPORTS_PER_SOL)
        .expect("fund loader-union rejection payer");
    let (tainted_capture, tainted_release, tainted_program_data, _) =
        immutable_capture(&tainted_svm, payer.pubkey());
    advance_one_slot(&mut tainted_svm);
    let unrelated_later_write = anchor_lang::solana_program::system_instruction::transfer(
        &payer.pubkey(),
        &tainted_program_data,
        0,
    );
    let failure = send_legacy_failure(
        &mut tainted_svm,
        &payer,
        &[tainted_capture, unrelated_later_write],
        &[],
    );
    assert!(failure.meta.logs.iter().any(|line| {
        line.starts_with(&format!(
            "Program {} invoke",
            programmable_generic_effect_core::ID
        ))
    }));
    assert!(tainted_svm.get_account(&tainted_release).is_none());
}

#[test]
fn core_capture_rejects_programdata_observation_in_its_landing_slot() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut svm = LiteSVM::new();
    artifacts.install_cached_programs(&mut svm);
    let payer = fixture_keypair(211);
    svm.airdrop(&payer.pubkey(), 100 * LAMPORTS_PER_SOL)
        .expect("fund same-slot rejection payer");
    let (capture, release, _, captured_slot) = immutable_capture(&svm, payer.pubkey());
    assert_eq!(
        svm.get_sysvar::<solana_clock::Clock>().slot,
        captured_slot,
        "fixture must actually exercise the same-slot capture boundary"
    );
    let failure = send_legacy_failure(&mut svm, &payer, &[capture], &[]);
    assert!(failure.meta.logs.iter().any(|line| {
        line.starts_with(&format!(
            "Program {} invoke",
            programmable_generic_effect_core::ID
        ))
    }));
    assert!(svm.get_account(&release).is_none());
}

#[test]
fn core_capture_rejects_mutable_programdata_forged_as_immutable() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut svm = loader_v3_test_vm();
    svm.add_program(programmable_generic_effect_core::ID, &artifacts.core)
        .expect("load exact Core SBF into loader fixture");
    let payer = fixture_keypair(212);
    svm.airdrop(&payer.pubkey(), 50_000 * LAMPORTS_PER_SOL)
        .expect("fund mutable loader capture rejection payer");
    let deployed = deploy_fixed_id_mutable_program(
        &mut svm,
        &payer,
        effect_engine_probe::ID,
        &artifacts.engine,
        artifacts.engine.len(),
        213,
    );
    advance_one_slot(&mut svm);
    let (capture, release) = immutable_capture_instruction(
        payer.pubkey(),
        deployed.program_id,
        deployed.program_data,
        deployed.deployment_slot,
    );
    let failure = send_legacy_failure(&mut svm, &payer, &[capture], &[]);
    assert!(failure.meta.logs.iter().any(|line| {
        line.starts_with(&format!(
            "Program {} invoke",
            programmable_generic_effect_core::ID
        ))
    }));
    assert!(svm.get_account(&release).is_none());
}

fn immutable_capture(svm: &LiteSVM, payer: Pubkey) -> (Instruction, Pubkey, Pubkey, u64) {
    let engine_program = effect_engine_probe::ID;
    let program_data = get_program_data_address(&engine_program);
    let captured_slot = match read_program_data_state(svm, &program_data) {
        UpgradeableLoaderState::ProgramData {
            slot,
            upgrade_authority_address,
        } => {
            assert_eq!(
                upgrade_authority_address, None,
                "cached exact-SBF engine must be authority-removed"
            );
            slot
        }
        other => panic!("cached engine has unexpected ProgramData state: {other:?}"),
    };
    let (instruction, release) =
        immutable_capture_instruction(payer, engine_program, program_data, captured_slot);
    (instruction, release, program_data, captured_slot)
}

fn immutable_capture_instruction(
    payer: Pubkey,
    engine_program: Pubkey,
    program_data: Pubkey,
    captured_slot: u64,
) -> (Instruction, Pubkey) {
    let (release, _) = ImmutableEngineReleaseCandidateV0::address(
        &programmable_generic_effect_core::ID,
        &engine_program,
    );
    let policy = EngineAdmissionPolicyCandidateV0 {
        policy_kind: ENGINE_POLICY_IMMUTABLE,
        engine_program: engine_program.to_bytes(),
        loader_program: solana_sdk_ids::bpf_loader_upgradeable::id().to_bytes(),
        program_data_or_zero: program_data.to_bytes(),
        expected_controller_or_zero: [0; 32],
        captured_programdata_slot_or_zero: captured_slot,
    };
    let data = CoreControlInstructionCandidateV0::CaptureImmutableEngineRelease(policy)
        .encode()
        .expect("encode canonical immutable-release capture control");
    let instruction = Instruction {
        program_id: programmable_generic_effect_core::ID,
        accounts: vec![
            AccountMeta::new(payer, true),
            AccountMeta::new(release, false),
            AccountMeta::new_readonly(engine_program, false),
            AccountMeta::new_readonly(program_data, false),
            AccountMeta::new_readonly(solana_sdk_ids::system_program::id(), false),
            AccountMeta::new_readonly(solana_sdk_ids::sysvar::instructions::id(), false),
        ],
        data,
    };
    (instruction, release)
}

fn program_invoked(logs: &[String], program_id: Pubkey) -> bool {
    let prefix = format!("Program {program_id} invoke");
    logs.iter().any(|line| line.starts_with(&prefix))
}

fn core_instruction_error(error: CoreError) -> InstructionError {
    InstructionError::Custom(anchor_lang::error::ERROR_CODE_OFFSET + error as u32)
}
