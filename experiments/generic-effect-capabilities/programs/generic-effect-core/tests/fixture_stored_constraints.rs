mod common;

use anchor_lang::AccountSerialize;
use common::{
    fixture_keypair, must_send_legacy, read_anchor_account, read_program_data_state,
    request_heap_frame_instruction, send_legacy_failure, set_compute_unit_limit_instruction,
    ReferenceAssetSpec, ReferenceCapabilityKind, ReferenceCapabilitySpec,
    ReferenceCreditConstraintSpec, ReferenceFixtureCompiler, ReferenceFixtureSpec,
    ReferenceIntentSpec, ReferencePlanSpec, SbfArtifacts, CONTROLLED_COMPUTE_UNIT_LIMIT,
    CONTROLLED_HEAP_FRAME_BYTES,
};
use effect_engine_probe::plan::{PlannedMove, RECEIPT_ACCEPT};
use generic_effect_private_wire::{
    compute_fee_policy_digest, compute_intent_capability_terms_root,
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
use solana_message::{AccountMeta, Instruction};
use solana_native_token::LAMPORTS_PER_SOL;
use solana_signer::Signer;
use solana_transaction::{InstructionError, TransactionError};

use programmable_generic_effect_core::{
    constants::{EXPERIMENTAL_MAJOR, ROUND_FLOOR},
    error::CoreError,
    state::{StoredAuthorizationCandidateV0, StoredAuthorizationLifecycle},
};

const FEE_NUMERATOR: u64 = 3;
const FEE_DENOMINATOR: u64 = 1_000;

#[test]
fn ninety_nine_debit_prefix_rejects_zero_credit_then_accepts_exactly_one() {
    let artifacts = artifacts();
    let mut fixture = ReferenceFixtureCompiler::new(
        &artifacts,
        ReferenceFixtureSpec {
            label: "stored-prefix-99-to-1",
            assets: vec![ReferenceAssetSpec { decimals: 6 }],
            intents: vec![ReferenceIntentSpec {
                maximum_successful_fills: 3,
            }],
            capabilities: vec![intent_debit(9_900), domain_credit(), exact_credit(1)],
            credit_constraints: vec![ReferenceCreditConstraintSpec {
                authorization_slot: 0,
                credit_local_term_index: 1,
                debit_local_term_indices: vec![0],
                minimum_credit_numerator: 1,
                nonzero_debit_denominator: 99,
                terminal_absolute_minimum: 0,
            }],
            opaque: vec![],
            plan: explicit(&[(0, 1, 99)]),
            receipt_mode: RECEIPT_ACCEPT,
        },
    );

    let before = fixture.rollback_snapshot();
    let failure = send_reference_failure(&mut fixture);
    assert_eq!(
        failure.err,
        TransactionError::InstructionError(
            2,
            InstructionError::Custom(
                anchor_lang::error::ERROR_CODE_OFFSET
                    + CoreError::CapabilityMinimumCreditNotMet as u32,
            ),
        )
    );
    assert!(program_invoked(&failure.meta.logs, effect_engine_probe::ID));
    assert_eq!(fixture.rollback_snapshot(), before);
    let rejected = fixture.stored_state(0);
    assert_eq!(rejected.lifecycle, StoredAuthorizationLifecycle::ACTIVE);
    assert_eq!(rejected.fill_sequence, 0);
    assert_eq!(rejected.capabilities[0].cumulative_engine_debit, 0);
    assert_eq!(rejected.capabilities[1].cumulative_credit, 0);

    fixture.set_plan(explicit(&[(0, 1, 98), (0, 2, 1)]), RECEIPT_ACCEPT);
    fixture.send_success();

    let accepted = fixture.stored_state(0);
    assert_eq!(accepted.lifecycle, StoredAuthorizationLifecycle::ACTIVE);
    assert_eq!(accepted.fill_sequence, 1);
    assert_eq!(accepted.pending_execution_digest, [0; 32]);
    assert_eq!(accepted.capabilities[0].cumulative_engine_debit, 99);
    assert_eq!(accepted.capabilities[0].cumulative_fee_debit, 0);
    assert_eq!(accepted.capabilities[0].remaining_total_debit, 9_830);
    assert_eq!(accepted.capabilities[1].cumulative_credit, 1);
    assert_eq!(fixture.endpoint_balance(0), 9_830);
    assert_eq!(fixture.endpoint_balance(1), 98);
    assert_eq!(fixture.endpoint_balance(2), 1);
    assert_eq!(fixture.domain_accounted(0), 98);
    assert_eq!(fixture.fee_vault_balance(0), 0);
    assert_eq!(fixture.fee_liability(0), 0);
}

#[test]
fn economic_terminal_enforces_absolute_minimum_after_prefix_already_passes() {
    let artifacts = artifacts();
    let mut fixture = ReferenceFixtureCompiler::new(
        &artifacts,
        ReferenceFixtureSpec {
            label: "stored-terminal-absolute-minimum",
            assets: vec![ReferenceAssetSpec { decimals: 6 }],
            intents: vec![ReferenceIntentSpec {
                maximum_successful_fills: 5,
            }],
            capabilities: vec![intent_debit(1_000), domain_credit(), exact_credit(1)],
            credit_constraints: vec![ReferenceCreditConstraintSpec {
                authorization_slot: 0,
                credit_local_term_index: 1,
                debit_local_term_indices: vec![0],
                minimum_credit_numerator: 1,
                nonzero_debit_denominator: 1_000,
                terminal_absolute_minimum: 9,
            }],
            opaque: vec![],
            plan: explicit(&[(0, 1, 999), (0, 2, 1)]),
            receipt_mode: RECEIPT_ACCEPT,
        },
    );

    let before = fixture.rollback_snapshot();
    let failure = send_reference_failure(&mut fixture);
    assert_eq!(
        failure.err,
        TransactionError::InstructionError(
            2,
            InstructionError::Custom(
                anchor_lang::error::ERROR_CODE_OFFSET
                    + CoreError::CapabilityMinimumCreditNotMet as u32,
            ),
        )
    );
    assert!(program_invoked(&failure.meta.logs, effect_engine_probe::ID));
    assert_eq!(fixture.rollback_snapshot(), before);

    fixture.set_plan(explicit(&[(0, 1, 990), (0, 2, 10)]), RECEIPT_ACCEPT);
    fixture.send_success();

    let terminal = fixture.stored_state(0);
    assert_eq!(terminal.lifecycle, StoredAuthorizationLifecycle::CONSUMED);
    assert_eq!(terminal.fill_sequence, 1);
    assert_eq!(terminal.pending_execution_digest, [0; 32]);
    assert_eq!(terminal.capabilities[0].cumulative_engine_debit, 1_000);
    assert_eq!(terminal.capabilities[0].cumulative_fee_debit, 3);
    assert_eq!(terminal.capabilities[0].remaining_total_debit, 0);
    assert_eq!(terminal.capabilities[1].cumulative_credit, 10);
    assert_eq!(fixture.endpoint_balance(0), 0);
    assert_eq!(fixture.endpoint_balance(1), 990);
    assert_eq!(fixture.endpoint_balance(2), 10);
    assert_eq!(fixture.domain_accounted(0), 990);
    assert_eq!(fixture.fee_vault_balance(0), 3);
    assert_eq!(fixture.fee_liability(0), 3);
}

#[test]
fn credit_only_authorization_consumes_only_at_max_fills_and_cannot_reenter() {
    let artifacts = artifacts();
    let mut fixture = ReferenceFixtureCompiler::new(
        &artifacts,
        ReferenceFixtureSpec {
            label: "stored-credit-only-fill-limit",
            assets: vec![ReferenceAssetSpec { decimals: 6 }],
            intents: vec![ReferenceIntentSpec {
                maximum_successful_fills: 2,
            }],
            capabilities: vec![
                ReferenceCapabilitySpec {
                    asset_index: 0,
                    initial_balance: 2,
                    kind: ReferenceCapabilityKind::DomainDebit {
                        maximum_engine_debit: 2,
                        accounted_before: 2,
                    },
                },
                exact_credit(2),
            ],
            credit_constraints: vec![],
            opaque: vec![],
            plan: explicit(&[(0, 1, 1)]),
            receipt_mode: RECEIPT_ACCEPT,
        },
    );

    fixture.send_success();
    let first = fixture.stored_state(0);
    assert_eq!(first.lifecycle, StoredAuthorizationLifecycle::ACTIVE);
    assert_eq!(first.fill_sequence, 1);
    assert_eq!(first.capabilities[0].remaining_total_debit, 0);
    assert_eq!(first.capabilities[0].cumulative_credit, 1);
    assert_eq!(fixture.domain_accounted(0), 1);
    assert_eq!(fixture.endpoint_balance(0), 1);
    assert_eq!(fixture.endpoint_balance(1), 1);

    fixture.set_plan(explicit(&[(0, 1, 1)]), RECEIPT_ACCEPT);
    let terminal_instruction = fixture.base.instruction.clone();
    fixture.send_success();
    let terminal = fixture.stored_state(0);
    assert_eq!(terminal.lifecycle, StoredAuthorizationLifecycle::CONSUMED);
    assert_eq!(terminal.fill_sequence, 2);
    assert_eq!(terminal.pending_execution_digest, [0; 32]);
    assert_eq!(terminal.capabilities[0].remaining_total_debit, 0);
    assert_eq!(terminal.capabilities[0].cumulative_credit, 2);
    assert_eq!(fixture.domain_accounted(0), 0);
    assert_eq!(fixture.endpoint_balance(0), 0);
    assert_eq!(fixture.endpoint_balance(1), 2);

    let tombstone = fixture.rollback_snapshot();
    let (transaction, _) = fixture.base.compile_custom_v0(terminal_instruction);
    let reentry = fixture
        .base
        .svm
        .send_transaction(transaction)
        .expect_err("consumed credit-only authorization reentered");
    assert_eq!(
        reentry.err,
        TransactionError::InstructionError(
            2,
            InstructionError::Custom(
                anchor_lang::error::ERROR_CODE_OFFSET + CoreError::AuthorizationUnavailable as u32,
            ),
        )
    );
    assert!(!program_invoked(
        &reentry.meta.logs,
        effect_engine_probe::ID
    ));
    assert_eq!(fixture.rollback_snapshot(), tombstone);
}

#[test]
fn replacement_is_one_shot_and_the_same_transition_cannot_reenter() {
    let artifacts = artifacts();
    let mut svm = LiteSVM::new();
    artifacts.install_cached_programs(&mut svm);
    let payer = fixture_keypair(71);
    let actor = fixture_keypair(72);
    svm.airdrop(&payer.pubkey(), 100 * LAMPORTS_PER_SOL)
        .expect("fund stored replacement payer");
    svm.airdrop(&actor.pubkey(), LAMPORTS_PER_SOL)
        .expect("install stored replacement actor");

    let old = initialize_complete_draft(&mut svm, &payer, &actor, 73);
    let activate_old = stored_control_instruction(
        actor.pubkey(),
        old,
        CoreControlInstructionCandidateV0::ActivateStoredAuthorization,
    );
    must_send_legacy(
        &mut svm,
        &payer,
        &[activate_old],
        &[&actor],
        "activate old stored replacement authorization",
    );
    let replacement = initialize_complete_draft(&mut svm, &payer, &actor, 83);

    let replace = replace_instruction(actor.pubkey(), old, replacement);
    let replace_transaction = [
        set_compute_unit_limit_instruction(CONTROLLED_COMPUTE_UNIT_LIMIT),
        request_heap_frame_instruction(CONTROLLED_HEAP_FRAME_BYTES),
        replace,
    ];

    // LiteSVM cannot interleave a second top-level transaction inside Core's
    // engine CPI. Install the exact valid Executing state produced by Core's
    // public host oracle, then prove the on-chain replacement handler rejects
    // that mid-execution lifecycle without changing either tombstone.
    let active_old_account = svm
        .get_account(&old)
        .expect("active old authorization exists");
    let mut executing: StoredAuthorizationCandidateV0 = read_anchor_account(&svm, &old);
    executing
        .reserve_execution(
            &programmable_generic_effect_core::ID,
            &old,
            svm.get_sysvar::<Clock>().slot,
            0,
            [0x5a; 32],
        )
        .expect("construct valid executing replacement guard state");
    let mut executing_account = active_old_account.clone();
    let mut executing_data = Vec::with_capacity(StoredAuthorizationCandidateV0::SPACE);
    executing
        .try_serialize(&mut executing_data)
        .expect("serialize valid executing replacement guard state");
    executing_data.resize(StoredAuthorizationCandidateV0::SPACE, 0);
    executing_account.data = executing_data;
    svm.set_account(old, executing_account)
        .expect("install valid executing replacement guard state");
    let executing_old = svm
        .get_account(&old)
        .expect("executing old authorization exists");
    let draft_replacement = svm
        .get_account(&replacement)
        .expect("draft replacement exists");
    let mid_execution = send_legacy_failure(&mut svm, &payer, &replace_transaction, &[&actor]);
    assert_eq!(
        mid_execution.err,
        TransactionError::InstructionError(
            2,
            InstructionError::Custom(
                anchor_lang::error::ERROR_CODE_OFFSET + CoreError::AuthorizationUnavailable as u32,
            ),
        )
    );
    assert_eq!(
        svm.get_account(&old).expect("executing state survives"),
        executing_old
    );
    assert_eq!(
        svm.get_account(&replacement)
            .expect("draft replacement survives"),
        draft_replacement
    );

    svm.set_account(old, active_old_account)
        .expect("restore active state after isolated reentry guard proof");
    svm.expire_blockhash();
    must_send_legacy(
        &mut svm,
        &payer,
        &replace_transaction,
        &[&actor],
        "replace active authorization with complete draft",
    );
    let cancelled: StoredAuthorizationCandidateV0 = read_anchor_account(&svm, &old);
    let active: StoredAuthorizationCandidateV0 = read_anchor_account(&svm, &replacement);
    assert_eq!(cancelled.lifecycle, StoredAuthorizationLifecycle::CANCELLED);
    assert_eq!(active.lifecycle, StoredAuthorizationLifecycle::ACTIVE);
    assert_eq!(cancelled.identity.actor, actor.pubkey());
    assert_eq!(active.identity.actor, actor.pubkey());

    let old_tombstone = svm.get_account(&old).expect("old tombstone exists");
    let new_active = svm
        .get_account(&replacement)
        .expect("replacement authorization exists");
    svm.expire_blockhash();
    let repeated = send_legacy_failure(&mut svm, &payer, &replace_transaction, &[&actor]);
    assert_eq!(
        repeated.err,
        TransactionError::InstructionError(
            2,
            InstructionError::Custom(
                anchor_lang::error::ERROR_CODE_OFFSET + CoreError::AuthorizationUnavailable as u32,
            ),
        )
    );
    assert_eq!(
        svm.get_account(&old).expect("old tombstone survives"),
        old_tombstone
    );
    assert_eq!(
        svm.get_account(&replacement)
            .expect("replacement active state survives"),
        new_active
    );
}

fn artifacts() -> SbfArtifacts {
    SbfArtifacts::load_exact().expect("run ./scripts/build-sbf.sh before exact-SBF tests")
}

fn intent_debit(maximum_engine_debit: u64) -> ReferenceCapabilitySpec {
    ReferenceCapabilitySpec {
        asset_index: 0,
        initial_balance: maximum_engine_debit + protocol_fee(maximum_engine_debit),
        kind: ReferenceCapabilityKind::IntentDebit {
            authorization_slot: 0,
            maximum_engine_debit,
        },
    }
}

fn domain_credit() -> ReferenceCapabilitySpec {
    ReferenceCapabilitySpec {
        asset_index: 0,
        initial_balance: 0,
        kind: ReferenceCapabilityKind::DomainCredit {
            accounted_before: 0,
        },
    }
}

fn exact_credit(minimum_credit: u64) -> ReferenceCapabilitySpec {
    ReferenceCapabilitySpec {
        asset_index: 0,
        initial_balance: 0,
        kind: ReferenceCapabilityKind::ExactCredit {
            authorization_slot: 0,
            minimum_credit,
        },
    }
}

fn protocol_fee(amount: u64) -> u64 {
    amount * FEE_NUMERATOR / FEE_DENOMINATOR
}

fn explicit(moves: &[(u8, u8, u64)]) -> ReferencePlanSpec {
    ReferencePlanSpec::Explicit(
        moves
            .iter()
            .map(|(source, destination, amount)| PlannedMove {
                source_capability_index: *source,
                destination_capability_index: *destination,
                amount: *amount,
            })
            .collect(),
    )
}

fn send_reference_failure(
    fixture: &mut ReferenceFixtureCompiler,
) -> litesvm::types::FailedTransactionMetadata {
    let (transaction, _) = fixture.compile_v0();
    fixture
        .base
        .svm
        .send_transaction(transaction)
        .expect_err("invalid stored constraint execution unexpectedly succeeded")
}

fn program_invoked(logs: &[String], program: anchor_lang::prelude::Pubkey) -> bool {
    let prefix = format!("Program {program} invoke [");
    logs.iter().any(|line| line.starts_with(&prefix))
}

fn initialize_complete_draft(
    svm: &mut LiteSVM,
    payer: &solana_keypair::Keypair,
    actor: &solana_keypair::Keypair,
    semantic_tag: u8,
) -> anchor_lang::prelude::Pubkey {
    let (initialize, authorization, term) =
        initialize_instruction(svm, payer.pubkey(), actor.pubkey(), semantic_tag);
    must_send_legacy(
        svm,
        payer,
        &[initialize],
        &[actor],
        "initialize stored replacement draft",
    );
    let write = stored_control_instruction(
        actor.pubkey(),
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
        svm,
        payer,
        &[write],
        &[actor],
        "write stored replacement draft terms",
    );
    let draft: StoredAuthorizationCandidateV0 = read_anchor_account(svm, &authorization);
    assert_eq!(draft.lifecycle, StoredAuthorizationLifecycle::DRAFT);
    assert_eq!(draft.term_bitmap, 1);
    authorization
}

fn initialize_instruction(
    svm: &LiteSVM,
    payer: anchor_lang::prelude::Pubkey,
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
        .expect("derive replacement admission-policy digest");
    let loader_snapshot = EngineLoaderStateSnapshotCandidateV0 {
        engine_program: engine_program.to_bytes(),
        loader_program: loader_program.to_bytes(),
        program_data_or_zero: program_data.to_bytes(),
        observed_programdata_slot: captured_slot,
        observed_controller_or_zero: [0; 32],
    };
    let engine_loader_state_snapshot_digest = loader_snapshot
        .digest()
        .expect("derive replacement loader-state digest");
    let fee_policy = EngineFeePolicyRowCandidateV0 {
        wire_version: WIRE_VERSION,
        rounding_mode: ROUND_FLOOR,
        flags: 0,
        revision: 1,
        rate_numerator: FEE_NUMERATOR,
        nonzero_denominator: FEE_DENOMINATOR,
    };
    let fee_policy_digest = compute_fee_policy_digest(&core_program_bytes, &fee_policy)
        .expect("derive replacement fee-policy digest");
    let protected_profile_digest = [semantic_tag; 32];
    let market_binding = MarketBindingRowCandidateV0 {
        core_program: core_program_bytes,
        core_experimental_major: EXPERIMENTAL_MAJOR,
        market_descriptor_key: fixture_keypair(semantic_tag.wrapping_add(1))
            .pubkey()
            .to_bytes(),
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
        .expect("derive replacement market-binding digest");
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
        asset_binding_digest: asset_binding
            .digest()
            .expect("derive replacement asset-binding digest"),
        required_domain_descriptor_digest_or_zero: [0; 32],
        maximum_engine_debit: 100,
        maximum_total_debit: 101,
        minimum_credit: 0,
        maximum_protocol_fee: 1,
    };
    let intent_capability_terms_root =
        compute_intent_capability_terms_root(&[term]).expect("derive replacement capability root");
    let credit_constraints_root = compute_intent_credit_constraints_root(&[])
        .expect("derive replacement empty constraint root");
    let maximum_successful_fills = 2;
    let core_terms_root = compute_intent_core_terms_root(IntentCoreTermsDigestInputs {
        maximum_successful_fills,
        capability_terms_root: &intent_capability_terms_root,
        credit_constraints_root: &credit_constraints_root,
    })
    .expect("derive replacement Core terms root");
    let identity = InlineIntentIdentityRowCandidateV0 {
        actor: actor.to_bytes(),
        engine_terms_commitment: [semantic_tag.wrapping_add(8); 32],
        authorization_nonce: u64::from(semantic_tag),
        expires_at_slot_exclusive: svm.get_sysvar::<Clock>().slot + 1_000,
    };
    let intent_digest = compute_intent_digest(IntentDigestInputs {
        core_program: &core_program_bytes,
        market_binding_digest: &market_binding_digest,
        loader_state_snapshot_digest: &engine_loader_state_snapshot_digest,
        fee_policy_digest: &fee_policy_digest,
        identity: &identity,
        core_terms_root: &core_terms_root,
    })
    .expect("derive replacement intent digest");
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
    let authorization = StoredAuthorizationCandidateV0::address(&core_program, &intent_digest).0;
    (
        Instruction {
            program_id: core_program,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(actor, true),
                AccountMeta::new(authorization, false),
                AccountMeta::new_readonly(solana_sdk_ids::system_program::id(), false),
                AccountMeta::new_readonly(solana_sdk_ids::sysvar::instructions::id(), false),
            ],
            data: CoreControlInstructionCandidateV0::InitializeStoredAuthorization(args)
                .encode()
                .expect("encode stored replacement initializer"),
        },
        authorization,
        term,
    )
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
        data: control.encode().expect("encode stored replacement control"),
    }
}

fn replace_instruction(
    actor: anchor_lang::prelude::Pubkey,
    old: anchor_lang::prelude::Pubkey,
    replacement: anchor_lang::prelude::Pubkey,
) -> Instruction {
    Instruction {
        program_id: programmable_generic_effect_core::ID,
        accounts: vec![
            AccountMeta::new_readonly(actor, true),
            AccountMeta::new(old, false),
            AccountMeta::new(replacement, false),
            AccountMeta::new_readonly(solana_sdk_ids::sysvar::instructions::id(), false),
        ],
        data: CoreControlInstructionCandidateV0::ReplaceStoredAuthorization
            .encode()
            .expect("encode exact stored replacement"),
    }
}
