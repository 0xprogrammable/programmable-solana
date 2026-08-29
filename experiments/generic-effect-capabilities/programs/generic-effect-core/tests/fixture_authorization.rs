mod common;

use anchor_lang::InstructionData;
use common::{
    fixture_keypair, must_send_legacy, read_anchor_account, read_program_data_state,
    send_legacy_failure, SbfArtifacts,
};
use generic_effect_private_wire::{
    compute_asset_set_digest, compute_fee_policy_digest, compute_intent_capability_terms_root,
    compute_intent_core_terms_root, compute_intent_credit_constraints_root, compute_intent_digest,
    AssetBindingRowCandidateV0, CoreControlInstructionCandidateV0,
    EngineAdmissionPolicyCandidateV0, EngineFeePolicyRowCandidateV0,
    EngineLoaderStateSnapshotCandidateV0, InitializeStoredAuthorizationArgsCandidateV0,
    InlineIntentIdentityRowCandidateV0, IntentCapabilityTermRowCandidateV0,
    IntentCoreTermsDigestInputs, IntentDigestInputs, MarketBindingRowCandidateV0,
    StoredAuthorizationChunkCandidateV0, StoredAuthorizationChunkHeaderCandidateV0,
    StoredAuthorizationChunkRowsCandidateV0, AUTHORITY_INTENT_FUNDED, ENGINE_POLICY_IMMUTABLE,
    FEE_CLASS_GROSS_DEBIT_RATE, INTENT_CAPABILITY_TERM_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT,
    INTENT_CAPABILITY_TERM_FLAG_FEE_FUNDING, RIGHT_DEBIT, STORED_AUTHORIZATION_CHUNK_KIND_TERM,
    WIRE_VERSION,
};
use litesvm::LiteSVM;
use solana_clock::Clock;
use solana_loader_v3_interface::{get_program_data_address, state::UpgradeableLoaderState};
use solana_message::{AccountMeta, Instruction, Message};
use solana_native_token::LAMPORTS_PER_SOL;
use solana_signer::Signer;
use solana_transaction::{InstructionError, TransactionError};

use programmable_generic_effect_core::{
    constants::{EXPERIMENTAL_MAJOR, ROUND_FLOOR},
    state::{StoredAuthorizationCandidateV0, StoredAuthorizationLifecycle},
};

#[test]
fn transaction_root_payer_actor_alias_uses_effective_writable_signer_union() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut svm = LiteSVM::new();
    artifacts.install_cached_programs(&mut svm);
    let wallet = fixture_keypair(220);
    svm.airdrop(&wallet.pubkey(), 100 * LAMPORTS_PER_SOL)
        .expect("fund duplicate payer/actor wallet");

    let (instruction, authorization, term) = initialize_instruction(&svm, wallet.pubkey(), 221);
    assert_eq!(
        instruction.accounts[0].pubkey,
        instruction.accounts[1].pubkey
    );
    assert!(instruction.accounts[0].is_writable);
    assert!(!instruction.accounts[1].is_writable);

    let message = Message::new(std::slice::from_ref(&instruction), Some(&wallet.pubkey()));
    let compiled_accounts = &message.instructions[0].accounts;
    assert_eq!(
        compiled_accounts[0], compiled_accounts[1],
        "duplicate instruction positions resolve to one message key"
    );
    assert_eq!(
        compiled_accounts[0], 0,
        "the shared actor key is the transaction fee payer"
    );
    assert_eq!(
        message.header.num_readonly_signed_accounts, 0,
        "the one global signer key is writable; original per-position SRO is not observable"
    );

    must_send_legacy(
        &mut svm,
        &wallet,
        &[instruction],
        &[],
        "initialize stored authorization with payer/actor alias",
    );
    let state: StoredAuthorizationCandidateV0 = read_anchor_account(&svm, &authorization);
    assert_eq!(state.lifecycle, StoredAuthorizationLifecycle::DRAFT);
    assert_eq!(state.identity.actor, wallet.pubkey());
    assert_eq!(state.term_count, 1);
    assert_eq!(state.constraint_count, 0);

    let sponsor = fixture_keypair(227);
    svm.airdrop(&sponsor.pubkey(), 100 * LAMPORTS_PER_SOL)
        .expect("fund staged-control sponsor");
    let write = stored_control_instruction(
        wallet.pubkey(),
        authorization,
        CoreControlInstructionCandidateV0::WriteStoredAuthorizationChunk(
            StoredAuthorizationChunkCandidateV0 {
                header: StoredAuthorizationChunkHeaderCandidateV0 {
                    wire_version: WIRE_VERSION,
                    chunk_kind: STORED_AUTHORIZATION_CHUNK_KIND_TERM,
                    start_index: 0,
                    row_count: 1,
                },
                rows: StoredAuthorizationChunkRowsCandidateV0::Terms(vec![term]),
            },
        ),
    );
    must_send_legacy(
        &mut svm,
        &sponsor,
        std::slice::from_ref(&write),
        &[&wallet],
        "write exact stored-authorization term chunk",
    );
    let written: StoredAuthorizationCandidateV0 = read_anchor_account(&svm, &authorization);
    assert_eq!(written.lifecycle, StoredAuthorizationLifecycle::DRAFT);
    assert_eq!(written.term_bitmap, 1);

    let before_duplicate_write = svm
        .get_account(&authorization)
        .expect("written authorization exists");
    send_legacy_failure(&mut svm, &sponsor, &[write], &[&wallet]);
    assert_eq!(
        svm.get_account(&authorization)
            .expect("authorization survives rejected overwrite"),
        before_duplicate_write,
        "immutable staged rows cannot be overwritten"
    );

    let activate = stored_control_instruction(
        wallet.pubkey(),
        authorization,
        CoreControlInstructionCandidateV0::ActivateStoredAuthorization,
    );
    must_send_legacy(
        &mut svm,
        &sponsor,
        &[activate],
        &[&wallet],
        "activate complete stored authorization",
    );
    let active: StoredAuthorizationCandidateV0 = read_anchor_account(&svm, &authorization);
    assert_eq!(active.lifecycle, StoredAuthorizationLifecycle::ACTIVE);
    assert_eq!(active.fill_sequence, 0);
    assert_eq!(active.capabilities[0].remaining_total_debit, 101);
}

#[test]
fn distinct_transaction_root_actor_rejects_unrequested_writable_privilege() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut svm = LiteSVM::new();
    artifacts.install_cached_programs(&mut svm);
    let payer = fixture_keypair(222);
    let actor = fixture_keypair(223);
    svm.airdrop(&payer.pubkey(), 100 * LAMPORTS_PER_SOL)
        .expect("fund distinct payer");
    svm.airdrop(&actor.pubkey(), LAMPORTS_PER_SOL)
        .expect("install distinct actor account");

    let (mut instruction, authorization, _) = initialize_instruction(&svm, actor.pubkey(), 224);
    instruction.accounts[0] = AccountMeta::new(payer.pubkey(), true);
    instruction.accounts[1] = AccountMeta::new(actor.pubkey(), true);
    let failure = send_legacy_failure(&mut svm, &payer, &[instruction], &[&actor]);
    assert!(failure.meta.logs.iter().any(|line| {
        line.starts_with(&format!(
            "Program {} invoke",
            programmable_generic_effect_core::ID
        ))
    }));
    assert!(svm.get_account(&authorization).is_none());
}

#[test]
fn incomplete_stored_authorization_cannot_activate_and_rolls_back_exactly() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut svm = LiteSVM::new();
    artifacts.install_cached_programs(&mut svm);
    let actor = fixture_keypair(228);
    let sponsor = fixture_keypair(229);
    svm.airdrop(&actor.pubkey(), 100 * LAMPORTS_PER_SOL)
        .expect("fund incomplete stored actor");
    svm.airdrop(&sponsor.pubkey(), 100 * LAMPORTS_PER_SOL)
        .expect("fund incomplete activation sponsor");
    let (initialize, authorization, _) = initialize_instruction(&svm, actor.pubkey(), 230);
    must_send_legacy(
        &mut svm,
        &actor,
        &[initialize],
        &[],
        "initialize incomplete stored authorization",
    );
    let before = svm
        .get_account(&authorization)
        .expect("incomplete Draft exists");
    let activate = stored_control_instruction(
        actor.pubkey(),
        authorization,
        CoreControlInstructionCandidateV0::ActivateStoredAuthorization,
    );
    send_legacy_failure(&mut svm, &sponsor, &[activate], &[&actor]);
    assert_eq!(
        svm.get_account(&authorization)
            .expect("incomplete Draft survives failed activation"),
        before
    );
}

#[test]
fn program_actor_payer_alias_is_rejected_even_with_valid_invoke_signed_privilege() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut svm = LiteSVM::new();
    artifacts.install_cached_programs(&mut svm);
    let outer_payer = fixture_keypair(225);
    svm.airdrop(&outer_payer.pubkey(), 100 * LAMPORTS_PER_SOL)
        .expect("fund signed-router outer payer");
    let program_actor = hostile_router_probe::router_program_actor_address().0;
    svm.airdrop(&program_actor, 10 * LAMPORTS_PER_SOL)
        .expect("fund router-owned program actor PDA");

    let (core_initialize, authorization, _) = initialize_instruction(&svm, program_actor, 226);
    let router = Instruction {
        program_id: hostile_router_probe::ID,
        accounts: vec![
            AccountMeta::new_readonly(programmable_generic_effect_core::ID, false),
            AccountMeta::new(program_actor, false),
            AccountMeta::new(program_actor, false),
            AccountMeta::new(authorization, false),
            AccountMeta::new_readonly(solana_sdk_ids::system_program::id(), false),
            AccountMeta::new_readonly(solana_sdk_ids::sysvar::instructions::id(), false),
        ],
        data: hostile_router_probe::instruction::Route {
            args: hostile_router_probe::RouteProbeArgs {
                core_account_count: 5,
                mode: hostile_router_probe::RouterMode::ForwardInitWithSignedActorAlias,
                core_instruction_data: core_initialize.data,
            },
        }
        .data(),
    };
    let actor_before = svm
        .get_account(&program_actor)
        .expect("funded router actor exists");
    let failure = send_legacy_failure(&mut svm, &outer_payer, &[router], &[]);
    assert!(failure
        .meta
        .logs
        .iter()
        .any(|line| { line.starts_with(&format!("Program {} invoke", hostile_router_probe::ID)) }));
    assert!(failure.meta.logs.iter().any(|line| {
        line.starts_with(&format!(
            "Program {} invoke",
            programmable_generic_effect_core::ID
        ))
    }));
    assert_eq!(
        failure.err,
        TransactionError::InstructionError(0, InstructionError::Custom(6_006)),
        "Core must reject the ProgramActor payer alias with DuplicateAccountIdentityDrift (0x1776)"
    );
    assert!(svm.get_account(&authorization).is_none());
    assert_eq!(
        svm.get_account(&program_actor)
            .expect("router actor survives failed transaction"),
        actor_before,
        "rejected Core account creation must roll back every lamport and data change"
    );
}

#[test]
fn distinct_program_actor_initializes_through_one_exact_signed_forward() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut svm = LiteSVM::new();
    artifacts.install_cached_programs(&mut svm);
    let outer_payer = fixture_keypair(231);
    svm.airdrop(&outer_payer.pubkey(), 100 * LAMPORTS_PER_SOL)
        .expect("fund signed-router outer payer");
    let program_actor = hostile_router_probe::router_program_actor_address().0;
    svm.airdrop(&program_actor, LAMPORTS_PER_SOL)
        .expect("install router-owned program actor PDA");

    let (mut core_initialize, authorization, _) = initialize_instruction(&svm, program_actor, 232);
    core_initialize.accounts[0] = AccountMeta::new(outer_payer.pubkey(), true);
    core_initialize.accounts[1] = AccountMeta::new_readonly(program_actor, false);
    let router = Instruction {
        program_id: hostile_router_probe::ID,
        accounts: std::iter::once(AccountMeta::new_readonly(
            programmable_generic_effect_core::ID,
            false,
        ))
        .chain(core_initialize.accounts.iter().cloned())
        .collect(),
        data: hostile_router_probe::instruction::Route {
            args: hostile_router_probe::RouteProbeArgs {
                core_account_count: 5,
                mode: hostile_router_probe::RouterMode::ForwardExactOnceWithSignedProgramActor,
                core_instruction_data: core_initialize.data,
            },
        }
        .data(),
    };

    must_send_legacy(
        &mut svm,
        &outer_payer,
        &[router],
        &[],
        "initialize with one distinct signed ProgramActor forward",
    );
    let state: StoredAuthorizationCandidateV0 = read_anchor_account(&svm, &authorization);
    assert_eq!(state.lifecycle, StoredAuthorizationLifecycle::DRAFT);
    assert_eq!(state.identity.actor, program_actor);
    assert_eq!(state.term_count, 1);
    assert_eq!(state.constraint_count, 0);
}

fn initialize_instruction(
    svm: &LiteSVM,
    actor: anchor_lang::prelude::Pubkey,
    semantic_tag: u8,
) -> (
    Instruction,
    anchor_lang::prelude::Pubkey,
    IntentCapabilityTermRowCandidateV0,
) {
    let core_program = programmable_generic_effect_core::ID;
    let core_program_bytes = core_program.to_bytes();
    let engine_program = effect_engine_probe::ID;
    let loader_program = solana_sdk_ids::bpf_loader_upgradeable::id();
    let program_data = get_program_data_address(&engine_program);
    let captured_slot = match read_program_data_state(svm, &program_data) {
        UpgradeableLoaderState::ProgramData {
            slot,
            upgrade_authority_address,
        } => {
            assert_eq!(upgrade_authority_address, None);
            slot
        }
        other => panic!("cached engine has unexpected ProgramData state: {other:?}"),
    };
    let admission_policy = EngineAdmissionPolicyCandidateV0 {
        policy_kind: ENGINE_POLICY_IMMUTABLE,
        engine_program: engine_program.to_bytes(),
        loader_program: loader_program.to_bytes(),
        program_data_or_zero: program_data.to_bytes(),
        expected_controller_or_zero: [0; 32],
        captured_programdata_slot_or_zero: captured_slot,
    };
    let engine_admission_policy_digest = admission_policy
        .digest()
        .expect("derive immutable admission-policy digest");
    let loader_snapshot = EngineLoaderStateSnapshotCandidateV0 {
        engine_program: engine_program.to_bytes(),
        loader_program: loader_program.to_bytes(),
        program_data_or_zero: program_data.to_bytes(),
        observed_programdata_slot: captured_slot,
        observed_controller_or_zero: [0; 32],
    };
    let engine_loader_state_snapshot_digest = loader_snapshot
        .digest()
        .expect("derive immutable loader-state digest");
    let fee_policy = EngineFeePolicyRowCandidateV0 {
        wire_version: WIRE_VERSION,
        rounding_mode: ROUND_FLOOR,
        flags: 0,
        revision: 1,
        rate_numerator: 3,
        nonzero_denominator: 1_000,
    };
    let fee_policy_digest = compute_fee_policy_digest(&core_program_bytes, &fee_policy)
        .expect("derive nonzero fee-policy digest");
    let protected_profile_digest = [semantic_tag; 32];
    let market_descriptor_key = fixture_keypair(semantic_tag.wrapping_add(1)).pubkey();
    let market_binding = MarketBindingRowCandidateV0 {
        core_program: core_program_bytes,
        core_experimental_major: EXPERIMENTAL_MAJOR,
        market_descriptor_key: market_descriptor_key.to_bytes(),
        market_descriptor_revision: 1,
        engine_program: engine_program.to_bytes(),
        engine_interface_id: [semantic_tag.wrapping_add(2); 32],
        engine_instance_id: [semantic_tag.wrapping_add(3); 32],
        engine_admission_policy_digest,
        domain_admission_profile_digest: [semantic_tag.wrapping_add(4); 32],
        protected_profile_digest,
        fee_policy_digest,
        opaque_schema_digest: [semantic_tag.wrapping_add(5); 32],
    };
    let market_binding_digest = market_binding
        .digest()
        .expect("derive canonical market-binding digest");
    let asset_binding = AssetBindingRowCandidateV0 {
        wire_version: WIRE_VERSION,
        flags: 0,
        decimals: 6,
        reserved: 0,
        asset_identity: fixture_keypair(semantic_tag.wrapping_add(6))
            .pubkey()
            .to_bytes(),
        asset_program: litesvm_token::TOKEN_ID.to_bytes(),
        settlement_profile_digest: protected_profile_digest,
    };
    compute_asset_set_digest(&[asset_binding]).expect("derive canonical asset-set digest");
    let asset_binding_digest = asset_binding
        .digest()
        .expect("derive canonical asset-binding digest");
    let term = IntentCapabilityTermRowCandidateV0 {
        intent_local_term_index: 0,
        authority_class: AUTHORITY_INTENT_FUNDED,
        fee_class: FEE_CLASS_GROSS_DEBIT_RATE,
        flags: INTENT_CAPABILITY_TERM_FLAG_FEE_FUNDING
            | INTENT_CAPABILITY_TERM_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT,
        rights_bits: RIGHT_DEBIT,
        endpoint_key: fixture_keypair(semantic_tag.wrapping_add(7))
            .pubkey()
            .to_bytes(),
        asset_binding_digest,
        required_domain_descriptor_digest_or_zero: [0; 32],
        maximum_engine_debit: 100,
        maximum_total_debit: 101,
        minimum_credit: 0,
        maximum_protocol_fee: 1,
    };
    let intent_capability_terms_root =
        compute_intent_capability_terms_root(&[term]).expect("derive one-term capability root");
    let credit_constraints_root =
        compute_intent_credit_constraints_root(&[]).expect("derive empty constraint root");
    let maximum_successful_fills = 2;
    let core_terms_root = compute_intent_core_terms_root(IntentCoreTermsDigestInputs {
        maximum_successful_fills,
        capability_terms_root: &intent_capability_terms_root,
        credit_constraints_root: &credit_constraints_root,
    })
    .expect("derive stored Core-terms root");
    let current_slot = svm.get_sysvar::<Clock>().slot;
    let identity = InlineIntentIdentityRowCandidateV0 {
        actor: actor.to_bytes(),
        engine_terms_commitment: [semantic_tag.wrapping_add(8); 32],
        authorization_nonce: u64::from(semantic_tag),
        expires_at_slot_exclusive: current_slot + 1_000,
    };
    let intent_digest = compute_intent_digest(IntentDigestInputs {
        core_program: &core_program_bytes,
        market_binding_digest: &market_binding_digest,
        loader_state_snapshot_digest: &engine_loader_state_snapshot_digest,
        fee_policy_digest: &fee_policy_digest,
        identity: &identity,
        core_terms_root: &core_terms_root,
    })
    .expect("derive stored intent digest");
    let args = InitializeStoredAuthorizationArgsCandidateV0 {
        wire_version: WIRE_VERSION,
        term_count: 1,
        constraint_count: 0,
        flags: 0,
        maximum_successful_fills,
        identity,
        market_binding_digest,
        engine_loader_state_snapshot_digest,
        fee_policy_digest,
        intent_capability_terms_root,
        credit_constraints_root,
        core_terms_root,
        intent_digest,
    };
    let data = CoreControlInstructionCandidateV0::InitializeStoredAuthorization(args)
        .encode()
        .expect("encode canonical stored-authorization initializer");
    let authorization = StoredAuthorizationCandidateV0::address(&core_program, &intent_digest).0;
    let instruction = Instruction {
        program_id: core_program,
        accounts: vec![
            AccountMeta::new(actor, true),
            AccountMeta::new_readonly(actor, true),
            AccountMeta::new(authorization, false),
            AccountMeta::new_readonly(solana_sdk_ids::system_program::id(), false),
            AccountMeta::new_readonly(solana_sdk_ids::sysvar::instructions::id(), false),
        ],
        data,
    };
    (instruction, authorization, term)
}

fn stored_control_instruction(
    actor: anchor_lang::prelude::Pubkey,
    authorization: anchor_lang::prelude::Pubkey,
    control: CoreControlInstructionCandidateV0,
) -> Instruction {
    Instruction {
        program_id: programmable_generic_effect_core::ID,
        accounts: vec![
            AccountMeta::new_readonly(actor, true),
            AccountMeta::new(authorization, false),
            AccountMeta::new_readonly(solana_sdk_ids::sysvar::instructions::id(), false),
        ],
        data: control
            .encode()
            .expect("encode canonical stored-authorization control"),
    }
}
