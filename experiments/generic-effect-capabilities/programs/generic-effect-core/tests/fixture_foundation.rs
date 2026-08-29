mod common;

use anchor_lang::solana_program::system_instruction;
use common::{
    advance_one_slot, assert_controlled_resource_headroom, build_core_execute_instruction,
    compile_v0_transaction, deploy_fixed_id_mutable_program, extend_mutable_program,
    fixture_keypair, install_lookup_table, loader_v3_test_vm, measure_execution,
    prepare_upgrade_buffer, read_program_data_state, send_legacy_failure, snapshot_accounts,
    upgrade_instruction, upgrade_mutable_program, CoreExecuteAccountClosure, SbfArtifacts,
    LOADER_EXTEND_BYTES, LOADER_V3_PROGRAM_DATA_METADATA_LEN, SBPF_V0_DEPLOYMENT_OVERRIDE_LABEL,
};
use litesvm::LiteSVM;
use litesvm_cpi_tree::CpiTreeExt;
use solana_clock::Clock;
use solana_loader_v3_interface::state::UpgradeableLoaderState;
use solana_message::AccountMeta;
use solana_native_token::LAMPORTS_PER_SOL;
use solana_signer::Signer;

#[test]
fn final_wire_gate_lengths_are_fixture_invariants() {
    use generic_effect_private_wire::{
        AUTHORIZATION_CAPABILITY_STATE_ROW_LEN, AUTHORIZATION_FEE_STATE_ROW_LEN,
        CLASSIC_SPL_ENDPOINT_STATE_ROW_LEN, DOMAIN_DESCRIPTOR_ROW_LEN,
        INLINE_INTENT_IDENTITY_ROW_LEN, INTENT_SET_ROW_LEN, OBSERVED_PROTECTED_DELTA_ROW_LEN,
    };

    assert_eq!(INTENT_SET_ROW_LEN, 32);
    assert_eq!(AUTHORIZATION_CAPABILITY_STATE_ROW_LEN, 88);
    assert_eq!(AUTHORIZATION_FEE_STATE_ROW_LEN, 80);
    assert_eq!(INLINE_INTENT_IDENTITY_ROW_LEN, 80);
    assert_eq!(DOMAIN_DESCRIPTOR_ROW_LEN, 304);
    assert_eq!(CLASSIC_SPL_ENDPOINT_STATE_ROW_LEN, 224);
    assert_eq!(OBSERVED_PROTECTED_DELTA_ROW_LEN, 40);
}

#[test]
fn canonical_builder_freezes_prefix_and_segment_order() {
    use generic_effect_private_wire::{
        compute_payload_digest, ExecuteEnvelopeCandidateV0, ExecuteEnvelopeHeaderCandidateV0,
        CORE_EXECUTE_EFFECT_DISCRIMINATOR, WIRE_VERSION,
    };

    let configuration = fixture_keypair(150).pubkey();
    let market = fixture_keypair(151).pubkey();
    let fee_policy = fixture_keypair(152).pubkey();
    let engine_program = fixture_keypair(153).pubkey();
    let callback_authority = fixture_keypair(154).pubkey();
    let loader_policy = fixture_keypair(155).pubkey();
    let protected_profile = fixture_keypair(156).pubkey();
    let opaque = fixture_keypair(157).pubkey();
    let envelope = ExecuteEnvelopeCandidateV0 {
        header: ExecuteEnvelopeHeaderCandidateV0 {
            wire_version: WIRE_VERSION,
            loader_policy_account_count: 1,
            domain_control_account_count: 0,
            authorization_account_count: 0,
            protected_profile_account_count: 1,
            fee_control_account_count: 0,
            settlement_capability_count: 0,
            opaque_capability_count: 1,
            domain_count: 0,
            intent_count: 0,
            inline_intent_row_count: 0,
            asset_count: 0,
            fee_shard_count: 0,
            authorization_snapshot_row_count: 0,
            maximum_engine_moves: 0,
            flags: 0,
            payload_len: 0,
            expires_at_slot_exclusive: 0,
            expected_engine_sequence: 0,
            intent_set_digest: [0; 32],
            domain_set_digest: [0; 32],
            protected_execution_root: [0; 32],
            expected_opaque_capability_root: [0; 32],
            fee_policy_digest: [0; 32],
            expected_engine_loader_state_snapshot_digest: [0; 32],
            payload_digest: compute_payload_digest(&[]).unwrap(),
        },
        domain_controls: vec![],
        authorization_snapshots: vec![],
        inline_intent_identities: vec![],
        fee_shards: vec![],
        settlement_capabilities: vec![],
        payload: vec![],
    };
    let closure = CoreExecuteAccountClosure {
        configuration,
        market,
        fee_policy,
        engine_program,
        callback_authority,
        loader_policy: vec![loader_policy],
        domain_controls: vec![],
        authorization_controls: vec![],
        protected_profile: vec![protected_profile],
        fee_controls: vec![],
        settlement: vec![],
        opaque: vec![AccountMeta::new(opaque, false)],
    };

    let instruction = build_core_execute_instruction(&envelope, &closure).unwrap();
    assert_eq!(instruction.program_id, programmable_generic_effect_core::ID);
    assert_eq!(instruction.data[..8], CORE_EXECUTE_EFFECT_DISCRIMINATOR);
    assert_eq!(
        instruction
            .accounts
            .iter()
            .map(|meta| (meta.pubkey, meta.is_signer, meta.is_writable))
            .collect::<Vec<_>>(),
        vec![
            (configuration, false, false),
            (market, false, false),
            (fee_policy, false, false),
            (engine_program, false, false),
            (callback_authority, false, false),
            (solana_sdk_ids::sysvar::instructions::id(), false, false),
            (loader_policy, false, false),
            (protected_profile, false, false),
            (opaque, false, true),
        ]
    );

    let mut short_closure = closure;
    short_closure.protected_profile.clear();
    let error = build_core_execute_instruction(&envelope, &short_closure).unwrap_err();
    assert!(error.contains("protected-profile closure length mismatch"));
}

#[test]
fn exact_five_sbf_artifacts_preload_the_four_distinct_program_ids() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    assert_ne!(artifacts.engine, artifacts.replacement_engine);
    assert_eq!(effect_engine_probe::ID, replacement_effect_engine_probe::ID);

    let mut svm = LiteSVM::new();
    artifacts.install_cached_programs(&mut svm);
    for program_id in [
        programmable_generic_effect_core::ID,
        effect_engine_probe::ID,
        hostile_router_probe::ID,
        callback_capability_probe::ID,
    ] {
        let account = svm
            .get_account(&program_id)
            .unwrap_or_else(|| panic!("cached program {program_id} is absent"));
        assert!(account.executable, "cached program {program_id}");
    }
}

#[test]
fn real_v0_lookup_table_is_warmed_resolved_and_executed() {
    let mut svm = LiteSVM::new();
    let payer = fixture_keypair(180);
    let destination = fixture_keypair(181).pubkey();
    svm.airdrop(&payer.pubkey(), 10 * LAMPORTS_PER_SOL)
        .expect("fund v0 payer");
    svm.airdrop(&destination, LAMPORTS_PER_SOL)
        .expect("install v0 destination");

    let transfer = system_instruction::transfer(&payer.pubkey(), &destination, 1);
    let table = install_lookup_table(&mut svm, &payer, vec![destination]);
    let (transaction, message_resources) =
        compile_v0_transaction(&svm, &payer, std::slice::from_ref(&transfer), &[table])
            .expect("compile measured v0 transaction");
    assert!(!message_resources.loaded_writable_keys.is_empty());
    assert!(message_resources
        .loaded_writable_keys
        .contains(&destination));
    assert_eq!(
        message_resources.unique_locks,
        message_resources.resolved_unique_keys.len()
    );
    assert!(message_resources.loaded_account_data_bytes > 0);

    let metadata = svm.send_transaction(transaction).unwrap_or_else(|failure| {
        panic!(
            "real v0 transaction failed: {:?}\n{}\n{}",
            failure.err,
            failure.meta.pretty_logs(),
            failure.meta.pretty_cpi_tree(),
        )
    });
    let execution = measure_execution(&metadata, &[transfer]);
    assert_eq!(execution.instruction_trace_len, 1);
    assert_eq!(execution.maximum_stack_height, 1);
    assert!(execution.compute_units > 0);
    assert!(message_resources.packet_bytes <= 1_232);
    assert_controlled_resource_headroom("v0-alt-foundation", &message_resources, &execution);
}

#[test]
fn fixed_id_loader_v3_deploy_same_slot_rejection_extend_and_upgrade_are_real() {
    assert!(SBPF_V0_DEPLOYMENT_OVERRIDE_LABEL.contains("SBPFv0"));
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before loader-v3 integration tests");
    let mut svm = loader_v3_test_vm();
    let payer = fixture_keypair(190);
    svm.airdrop(&payer.pubkey(), 50_000 * LAMPORTS_PER_SOL)
        .expect("fund loader payer");

    let deployed = deploy_fixed_id_mutable_program(
        &mut svm,
        &payer,
        effect_engine_probe::ID,
        &artifacts.engine,
        artifacts.engine.len(),
        191,
    );
    assert_eq!(
        deployed.deployment_slot,
        svm.get_sysvar::<Clock>().slot,
        "DeployWithMaxDataLen must record the landing slot"
    );
    let replacement_buffer =
        prepare_upgrade_buffer(&mut svm, &payer, &artifacts.replacement_engine, 192);

    let protected_before_same_slot_failure = snapshot_accounts(
        &svm,
        &[
            deployed.program_id,
            deployed.program_data,
            replacement_buffer,
        ],
    );
    let same_slot_upgrade = upgrade_instruction(&deployed, replacement_buffer, &payer);
    let failure = send_legacy_failure(&mut svm, &payer, &[same_slot_upgrade], &[]);
    assert!(failure
        .meta
        .logs
        .iter()
        .any(|line| line.contains("deployed in this block")));
    assert_eq!(
        snapshot_accounts(
            &svm,
            &[
                deployed.program_id,
                deployed.program_data,
                replacement_buffer
            ]
        ),
        protected_before_same_slot_failure,
        "same-slot loader rejection must roll back Program, ProgramData, and Buffer"
    );

    advance_one_slot(&mut svm);
    let extended = extend_mutable_program(&mut svm, &payer, &deployed, LOADER_EXTEND_BYTES);
    assert_eq!(
        extended.program_data_len,
        deployed.program_data_len + usize::try_from(LOADER_EXTEND_BYTES).unwrap()
    );
    assert_eq!(extended.deployment_slot, svm.get_sysvar::<Clock>().slot);

    advance_one_slot(&mut svm);
    let upgraded = upgrade_mutable_program(
        &mut svm,
        &payer,
        &extended,
        replacement_buffer,
        &artifacts.replacement_engine,
    );
    assert_eq!(upgraded.program_id, deployed.program_id);
    assert_eq!(upgraded.program_data, deployed.program_data);
    assert_eq!(upgraded.deployment_slot, svm.get_sysvar::<Clock>().slot);
    assert!(upgraded.deployment_slot > extended.deployment_slot);

    match read_program_data_state(&svm, &upgraded.program_data) {
        UpgradeableLoaderState::ProgramData {
            slot,
            upgrade_authority_address,
        } => {
            assert_eq!(slot, upgraded.deployment_slot);
            assert_eq!(upgrade_authority_address, Some(payer.pubkey()));
        }
        other => panic!("unexpected final ProgramData state: {other:?}"),
    }
    let account = svm
        .get_account(&upgraded.program_data)
        .expect("final ProgramData account");
    assert_eq!(
        &account.data[LOADER_V3_PROGRAM_DATA_METADATA_LEN
            ..LOADER_V3_PROGRAM_DATA_METADATA_LEN + artifacts.replacement_engine.len()],
        artifacts.replacement_engine.as_slice()
    );
}
