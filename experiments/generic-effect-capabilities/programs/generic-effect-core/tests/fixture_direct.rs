mod common;

use anchor_lang::{solana_program::system_instruction, InstructionData};
use common::{
    assert_controlled_resource_headroom, cpi_program_paths, decode_requested_heap_frame,
    decode_set_compute_unit_limit, measure_execution, read_anchor_account,
    request_heap_frame_instruction, set_compute_unit_limit_instruction, snapshot_accounts,
    token_balance, token_state, DirectFixture, SbfArtifacts, CONTROLLED_COMPUTE_UNIT_LIMIT,
    CONTROLLED_HEAP_FRAME_BYTES, DIRECT_DEFAULT_AMOUNT, DIRECT_SOURCE_BALANCE,
};
use generic_effect_private_wire::{CORE_EXECUTE_EFFECT_DISCRIMINATOR, EXECUTE_ENVELOPE_HEADER_LEN};
use litesvm_cpi_tree::CpiTreeExt;
use solana_message::{AccountMeta, Instruction};
use solana_signer::Signer;
use solana_transaction::{InstructionError, TransactionError};

use programmable_generic_effect_core::{error::CoreError, state::FeeLiabilityLedgerCandidateV0};

#[test]
fn one_transaction_root_direct_may_accept_a_canonical_state_only_effect() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = DirectFixture::state_only(&artifacts);

    let decoded_plan =
        effect_engine_probe::plan::EnginePlan::decode_exact(fixture.engine_request.payload())
            .expect("engine sees the canonical state-only payload");
    assert_eq!(decoded_plan.move_count, 0);
    assert_eq!(fixture.transfer_amount, 0);
    assert_eq!(fixture.protocol_fee, 0);
    assert!(fixture.maximum_engine_debit > 0);
    assert!(fixture.maximum_protocol_fee > 0);
    assert_eq!(
        token_balance(&fixture.svm, &fixture.source),
        DIRECT_SOURCE_BALANCE
    );
    assert_eq!(token_balance(&fixture.svm, &fixture.destination), 0);
    assert_eq!(token_balance(&fixture.svm, &fixture.fee_vault), 0);
    let protected = fixture.rollback_state_addresses();
    let before = snapshot_accounts(&fixture.svm, &protected);

    let (transaction, message_resources) = fixture.compile_v0();
    let metadata = fixture
        .svm
        .send_transaction(transaction)
        .unwrap_or_else(|failure| {
            panic!(
                "exact transaction-root DIRECT state-only execution failed: {:?}\n{}\n{}",
                failure.err,
                failure.meta.pretty_logs(),
                failure.meta.pretty_cpi_tree(),
            )
        });
    assert_eq!(
        snapshot_accounts(&fixture.svm, &protected),
        before,
        "state-only success must not create a settlement or derived fee"
    );
    let paths = cpi_program_paths(&metadata);
    assert!(paths.contains(&vec![
        programmable_generic_effect_core::ID,
        effect_engine_probe::ID,
    ]));
    assert!(!paths.contains(&vec![
        programmable_generic_effect_core::ID,
        litesvm_token::TOKEN_ID,
    ]));
    let compute_limit = set_compute_unit_limit_instruction(CONTROLLED_COMPUTE_UNIT_LIMIT);
    let heap_frame = request_heap_frame_instruction(CONTROLLED_HEAP_FRAME_BYTES);
    let execution = measure_execution(
        &metadata,
        &[compute_limit, heap_frame, fixture.instruction.clone()],
    );
    assert_controlled_resource_headroom("direct-state-only", &message_resources, &execution);
}

#[test]
fn accepted_direct_payload_is_forwarded_exactly_and_settled() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = DirectFixture::accepted(&artifacts, DIRECT_DEFAULT_AMOUNT);

    assert_eq!(fixture.envelope.payload, fixture.engine_request.payload);
    let decoded_plan =
        effect_engine_probe::plan::EnginePlan::decode_exact(fixture.engine_request.payload())
            .expect("engine sees the exact bounded Core payload");
    assert_eq!(decoded_plan.move_count, 1);
    assert_eq!(
        token_balance(&fixture.svm, &fixture.source),
        DIRECT_SOURCE_BALANCE
    );
    assert_eq!(token_balance(&fixture.svm, &fixture.destination), 0);
    assert_eq!(token_balance(&fixture.svm, &fixture.fee_vault), 0);
    assert!(fixture
        .svm
        .get_account(&fixture.callback_authority)
        .is_none());

    let immutable_before = snapshot_accounts(&fixture.svm, &fixture.immutable_state_addresses());
    let instruction = fixture.instruction.clone();
    let (transaction, message_resources) = fixture.compile_v0();
    assert!(message_resources
        .static_keys
        .contains(&fixture.actor.pubkey()));
    assert!(!message_resources
        .loaded_writable_keys
        .contains(&fixture.actor.pubkey()));
    let metadata = fixture
        .svm
        .send_transaction(transaction)
        .unwrap_or_else(|failure| {
            panic!(
                "exact DIRECT v0 transaction failed: {:?}\n{}\n{}",
                failure.err,
                failure.meta.pretty_logs(),
                failure.meta.pretty_cpi_tree(),
            )
        });

    assert_eq!(
        token_balance(&fixture.svm, &fixture.source),
        DIRECT_SOURCE_BALANCE - DIRECT_DEFAULT_AMOUNT - fixture.protocol_fee
    );
    assert_eq!(
        token_balance(&fixture.svm, &fixture.destination),
        DIRECT_DEFAULT_AMOUNT
    );
    assert_eq!(
        token_balance(&fixture.svm, &fixture.fee_vault),
        fixture.protocol_fee
    );
    let liability: FeeLiabilityLedgerCandidateV0 =
        read_anchor_account(&fixture.svm, &fixture.fee_liability);
    assert_eq!(liability.liability, u128::from(fixture.protocol_fee));
    assert_eq!(
        snapshot_accounts(&fixture.svm, &fixture.immutable_state_addresses()),
        immutable_before,
        "execution must not mutate configuration, market, fee policy, or immutable release"
    );
    assert!(fixture
        .svm
        .get_account(&fixture.callback_authority)
        .is_none());

    let paths = cpi_program_paths(&metadata);
    assert!(paths.contains(&vec![
        programmable_generic_effect_core::ID,
        effect_engine_probe::ID,
    ]));
    assert!(paths.contains(&vec![
        programmable_generic_effect_core::ID,
        litesvm_token::TOKEN_ID,
    ]));
    assert!(
        !paths.contains(&vec![
            programmable_generic_effect_core::ID,
            effect_engine_probe::ID,
            litesvm_token::TOKEN_ID,
        ]),
        "engine and settlement are sibling Core CPIs, not a leaked nested authority path"
    );

    let heap_frame = request_heap_frame_instruction(CONTROLLED_HEAP_FRAME_BYTES);
    let compute_limit = set_compute_unit_limit_instruction(CONTROLLED_COMPUTE_UNIT_LIMIT);
    assert_eq!(
        decode_set_compute_unit_limit(&compute_limit),
        Some(CONTROLLED_COMPUTE_UNIT_LIMIT)
    );
    assert_eq!(
        decode_requested_heap_frame(&heap_frame),
        Some(CONTROLLED_HEAP_FRAME_BYTES)
    );
    eprintln!("RESOURCE direct-one-move: requested_heap_frame={CONTROLLED_HEAP_FRAME_BYTES}");
    let execution = measure_execution(&metadata, &[compute_limit, heap_frame, instruction]);
    assert!(execution.instruction_trace_len >= 4);
    assert!(execution.maximum_stack_height >= 2);
    assert_controlled_resource_headroom("direct-one-move", &message_resources, &execution);
}

#[test]
fn execute_heap_contract_rejects_missing_and_undersized_frames_before_decoding() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let cases = [
        ("missing", None),
        ("default-32k", Some(32 * 1_024)),
        ("former-64k", Some(64 * 1_024)),
        ("undersized-128k", Some(128 * 1_024)),
    ];

    for (label, requested_bytes) in cases {
        let mut fixture = DirectFixture::accepted(&artifacts, DIRECT_DEFAULT_AMOUNT);
        let invalid_core = invalid_execute_instruction(&fixture.instruction);
        let mut instructions = Vec::with_capacity(usize::from(requested_bytes.is_some()) + 1);
        if let Some(bytes) = requested_bytes {
            instructions.push(request_heap_frame_instruction(bytes));
        }
        let core_index = instructions.len();
        instructions.push(invalid_core);
        let (transaction, _) = fixture.compile_raw_v0_instructions(instructions, true);
        let failure = fixture
            .svm
            .send_transaction(transaction)
            .expect_err("missing or undersized execute heap unexpectedly passed");
        assert_eq!(
            failure.err,
            TransactionError::InstructionError(
                u8::try_from(core_index).expect("bounded fixture instruction index"),
                core_instruction_error(CoreError::ControlledHeapFrameRequired),
            ),
            "heap contract case {label}"
        );
        assert!(
            failure
                .meta
                .logs
                .iter()
                .all(|line| !line.contains("memory allocation failed")),
            "heap contract case {label} must fail explicitly before allocation pressure"
        );
    }
}

#[test]
fn execute_heap_contract_accepts_the_authenticated_frame_in_any_top_level_order() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");

    for heap_first in [true, false] {
        let mut fixture = DirectFixture::accepted(&artifacts, DIRECT_DEFAULT_AMOUNT);
        let invalid_core = invalid_execute_instruction(&fixture.instruction);
        let heap = request_heap_frame_instruction(CONTROLLED_HEAP_FRAME_BYTES);
        let (instructions, core_index) = if heap_first {
            (vec![heap, invalid_core], 1)
        } else {
            (vec![invalid_core, heap], 0)
        };
        let (transaction, _) = fixture.compile_raw_v0_instructions(instructions, true);
        let failure = fixture
            .svm
            .send_transaction(transaction)
            .expect_err("invalid execute wire unexpectedly passed");
        assert_eq!(
            failure.err,
            TransactionError::InstructionError(
                core_index,
                core_instruction_error(CoreError::InvalidWireEncoding),
            ),
            "the transaction-wide heap request must be accepted regardless of order"
        );
    }
}

#[test]
fn routed_execute_authenticates_a_later_transaction_heap_request() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = DirectFixture::accepted(&artifacts, DIRECT_DEFAULT_AMOUNT);
    let invalid_core = invalid_execute_instruction(&fixture.instruction);
    let router = forward_exact_router_instruction(&invalid_core);
    let heap = request_heap_frame_instruction(CONTROLLED_HEAP_FRAME_BYTES);
    let (transaction, _) = fixture.compile_raw_v0_instructions(vec![router, heap], true);
    let failure = fixture
        .svm
        .send_transaction(transaction)
        .expect_err("invalid routed execute wire unexpectedly passed");
    assert_eq!(
        failure.err,
        TransactionError::InstructionError(
            0,
            core_instruction_error(CoreError::InvalidWireEncoding),
        )
    );
    assert!(program_invoked(
        &failure.meta.logs,
        programmable_generic_effect_core::ID
    ));
}

#[test]
fn short_execute_account_prefix_fails_without_panicking() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = DirectFixture::accepted(&artifacts, DIRECT_DEFAULT_AMOUNT);
    let mut short_core = fixture.instruction.clone();
    short_core.accounts.truncate(5);
    let heap = request_heap_frame_instruction(CONTROLLED_HEAP_FRAME_BYTES);
    let (transaction, _) = fixture.compile_raw_v0_instructions(vec![heap, short_core], false);
    let failure = fixture
        .svm
        .send_transaction(transaction)
        .expect_err("short execute account prefix unexpectedly passed");
    assert_eq!(
        failure.err,
        TransactionError::InstructionError(
            1,
            core_instruction_error(CoreError::AccountSegmentLengthMismatch),
        )
    );
    assert!(
        failure
            .meta
            .logs
            .iter()
            .all(|line| !line.contains("panicked")),
        "short account closure must return a typed error rather than indexing past the slice"
    );
}

#[test]
fn exact_delegate_settles_directly_and_consumes_the_delegate_exactly() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let direct = DirectFixture::accepted(&artifacts, DIRECT_DEFAULT_AMOUNT);
    let delegated_amount = DIRECT_DEFAULT_AMOUNT
        .checked_add(direct.protocol_fee)
        .expect("engine debit plus protocol fee fits u64");
    let mut fixture = direct.into_exact_delegate(delegated_amount);
    let spend_authority = fixture
        .spend_authority
        .expect("exact-delegate fixture exposes its derived PDA");
    let source_before = token_state(&fixture.svm, &fixture.source);
    assert_eq!(source_before.delegate, Some(spend_authority).into());
    assert_eq!(source_before.delegated_amount, delegated_amount);

    let (transaction, _) = fixture.compile_v0();
    let metadata = fixture
        .svm
        .send_transaction(transaction)
        .unwrap_or_else(|failure| {
            panic!(
                "exact one-shot delegate failed at transaction root: {:?}\n{}\n{}",
                failure.err,
                failure.meta.pretty_logs(),
                failure.meta.pretty_cpi_tree(),
            )
        });

    assert_eq!(
        token_balance(&fixture.svm, &fixture.source),
        DIRECT_SOURCE_BALANCE - delegated_amount
    );
    assert_eq!(
        token_balance(&fixture.svm, &fixture.destination),
        DIRECT_DEFAULT_AMOUNT
    );
    assert_eq!(
        token_balance(&fixture.svm, &fixture.fee_vault),
        fixture.protocol_fee
    );
    let source_after = token_state(&fixture.svm, &fixture.source);
    assert_eq!(source_after.delegate, None.into());
    assert_eq!(source_after.delegated_amount, 0);
    let paths = cpi_program_paths(&metadata);
    assert!(paths.contains(&vec![
        programmable_generic_effect_core::ID,
        effect_engine_probe::ID,
    ]));
    assert!(paths.contains(&vec![
        programmable_generic_effect_core::ID,
        litesvm_token::TOKEN_ID,
    ]));
}

#[test]
fn routed_exact_delegate_settles_without_transaction_root_actor_authority() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let direct = DirectFixture::accepted(&artifacts, DIRECT_DEFAULT_AMOUNT);
    let delegated_amount = DIRECT_DEFAULT_AMOUNT
        .checked_add(direct.protocol_fee)
        .expect("engine debit plus protocol fee fits u64");
    let mut fixture = direct.into_exact_delegate(delegated_amount);
    let router = forward_exact_router_instruction(&fixture.instruction);
    let (transaction, _) = fixture.compile_custom_v0(router);
    let metadata = fixture
        .svm
        .send_transaction(transaction)
        .unwrap_or_else(|failure| {
            panic!(
                "router failed to forward an exact one-shot delegate: {:?}\n{}\n{}",
                failure.err,
                failure.meta.pretty_logs(),
                failure.meta.pretty_cpi_tree(),
            )
        });

    assert_eq!(
        token_balance(&fixture.svm, &fixture.source),
        DIRECT_SOURCE_BALANCE - delegated_amount
    );
    assert_eq!(
        token_balance(&fixture.svm, &fixture.destination),
        DIRECT_DEFAULT_AMOUNT
    );
    assert_eq!(
        token_balance(&fixture.svm, &fixture.fee_vault),
        fixture.protocol_fee
    );
    let source_after = token_state(&fixture.svm, &fixture.source);
    assert_eq!(source_after.delegate, None.into());
    assert_eq!(source_after.delegated_amount, 0);
    let paths = cpi_program_paths(&metadata);
    assert!(paths.contains(&vec![
        hostile_router_probe::ID,
        programmable_generic_effect_core::ID,
        effect_engine_probe::ID,
    ]));
    assert!(paths.contains(&vec![
        hostile_router_probe::ID,
        programmable_generic_effect_core::ID,
        litesvm_token::TOKEN_ID,
    ]));
}

#[test]
fn exact_delegate_cannot_use_the_direct_state_only_exception() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = DirectFixture::state_only(&artifacts).into_exact_delegate(1);
    let protected = fixture.rollback_state_addresses();
    let before = snapshot_accounts(&fixture.svm, &protected);
    let (transaction, _) = fixture.compile_v0();
    let failure = fixture
        .svm
        .send_transaction(transaction)
        .expect_err("exact delegate incorrectly accepted a zero-effect fill");

    assert!(program_invoked(&failure.meta.logs, effect_engine_probe::ID));
    assert!(!program_invoked(
        &failure.meta.logs,
        litesvm_token::TOKEN_ID
    ));
    assert_eq!(
        snapshot_accounts(&fixture.svm, &protected),
        before,
        "rejected zero-effect exact delegate changed protected prestate"
    );
}

#[test]
fn mutated_truncated_trailing_or_digest_changed_outer_payload_fails_before_engine() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");

    for case in 0..4 {
        let mut fixture = DirectFixture::accepted(&artifacts, DIRECT_DEFAULT_AMOUNT);
        let label = match case {
            0 => {
                *fixture
                    .instruction
                    .data
                    .last_mut()
                    .expect("direct payload is nonempty") ^= 1;
                "mutated-payload"
            }
            1 => {
                fixture.instruction.data.pop();
                "truncated-payload"
            }
            2 => {
                fixture.instruction.data.push(0);
                "trailing-payload"
            }
            3 => {
                let digest_offset = 8 + EXECUTE_ENVELOPE_HEADER_LEN - 32;
                fixture.instruction.data[digest_offset] ^= 1;
                "changed-payload-digest"
            }
            _ => unreachable!(),
        };
        let protected = fixture.rollback_state_addresses();
        let before = snapshot_accounts(&fixture.svm, &protected);
        let (transaction, _) = fixture.compile_v0();
        let failure = fixture
            .svm
            .send_transaction(transaction)
            .expect_err("malformed outer payload unexpectedly reached settlement");
        assert!(
            !program_invoked(&failure.meta.logs, effect_engine_probe::ID),
            "{label} reached the untrusted engine:\n{}",
            failure.meta.pretty_logs()
        );
        assert_eq!(
            snapshot_accounts(&fixture.svm, &protected),
            before,
            "{label} changed protected state"
        );
    }
}

#[test]
fn malformed_or_late_engine_receipt_rolls_back_before_token_settlement() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let cases = [
        (effect_engine_probe::plan::RECEIPT_TRUNCATED, "truncated"),
        (effect_engine_probe::plan::RECEIPT_TRAILING_BYTE, "trailing"),
        (
            effect_engine_probe::plan::RECEIPT_WRONG_REQUEST_DIGEST,
            "wrong-request-digest",
        ),
        (
            effect_engine_probe::plan::RECEIPT_LATE_FAILURE,
            "engine-late-failure",
        ),
    ];

    for (mode, label) in cases {
        let mut fixture = DirectFixture::with_receipt_mode(&artifacts, DIRECT_DEFAULT_AMOUNT, mode);
        let protected = fixture.rollback_state_addresses();
        let before = snapshot_accounts(&fixture.svm, &protected);
        let (transaction, _) = fixture.compile_v0();
        let failure = fixture
            .svm
            .send_transaction(transaction)
            .expect_err("malformed engine result unexpectedly reached settlement");
        assert!(
            program_invoked(&failure.meta.logs, effect_engine_probe::ID),
            "{label} did not exercise the engine boundary:\n{}",
            failure.meta.pretty_logs()
        );
        assert!(
            !program_invoked(&failure.meta.logs, litesvm_token::TOKEN_ID),
            "{label} reached token settlement:\n{}",
            failure.meta.pretty_logs()
        );
        assert_eq!(
            snapshot_accounts(&fixture.svm, &protected),
            before,
            "{label} failed to roll back protected state"
        );
    }
}

#[test]
fn direct_actor_route_is_rejected_at_transaction_root_before_engine() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = DirectFixture::accepted(&artifacts, DIRECT_DEFAULT_AMOUNT);
    let core_instruction_data = fixture.instruction.data.clone();
    let core_account_count =
        u8::try_from(fixture.instruction.accounts.len()).expect("bounded Core closure");
    let mut router_accounts = Vec::with_capacity(1 + fixture.instruction.accounts.len());
    router_accounts.push(AccountMeta::new_readonly(
        programmable_generic_effect_core::ID,
        false,
    ));
    router_accounts.extend(fixture.instruction.accounts.iter().cloned());
    let router = Instruction {
        program_id: hostile_router_probe::ID,
        accounts: router_accounts,
        data: hostile_router_probe::instruction::Route {
            args: hostile_router_probe::RouteProbeArgs {
                core_account_count,
                mode: hostile_router_probe::RouterMode::ForwardExactOnce,
                core_instruction_data,
            },
        }
        .data(),
    };
    let protected = fixture.rollback_state_addresses();
    let before = snapshot_accounts(&fixture.svm, &protected);
    let (transaction, _) = fixture.compile_custom_v0(router);
    let failure = fixture
        .svm
        .send_transaction(transaction)
        .expect_err("router laundered a DIRECT transaction-root authorization");
    assert!(program_invoked(
        &failure.meta.logs,
        hostile_router_probe::ID
    ));
    assert!(program_invoked(
        &failure.meta.logs,
        programmable_generic_effect_core::ID
    ));
    assert!(!program_invoked(
        &failure.meta.logs,
        effect_engine_probe::ID
    ));
    assert_eq!(snapshot_accounts(&fixture.svm, &protected), before);
}

#[test]
fn routed_direct_state_only_effect_is_rejected_before_engine() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = DirectFixture::state_only(&artifacts);
    let core_instruction_data = fixture.instruction.data.clone();
    let core_account_count =
        u8::try_from(fixture.instruction.accounts.len()).expect("bounded Core closure");
    let mut router_accounts = Vec::with_capacity(1 + fixture.instruction.accounts.len());
    router_accounts.push(AccountMeta::new_readonly(
        programmable_generic_effect_core::ID,
        false,
    ));
    router_accounts.extend(fixture.instruction.accounts.iter().cloned());
    let router = Instruction {
        program_id: hostile_router_probe::ID,
        accounts: router_accounts,
        data: hostile_router_probe::instruction::Route {
            args: hostile_router_probe::RouteProbeArgs {
                core_account_count,
                mode: hostile_router_probe::RouterMode::ForwardExactOnce,
                core_instruction_data,
            },
        }
        .data(),
    };
    let protected = fixture.rollback_state_addresses();
    let before = snapshot_accounts(&fixture.svm, &protected);
    let (transaction, _) = fixture.compile_custom_v0(router);
    let failure = fixture
        .svm
        .send_transaction(transaction)
        .expect_err("router laundered the DIRECT-only state-effect exception");
    assert!(program_invoked(
        &failure.meta.logs,
        hostile_router_probe::ID
    ));
    assert!(program_invoked(
        &failure.meta.logs,
        programmable_generic_effect_core::ID
    ));
    assert!(!program_invoked(
        &failure.meta.logs,
        effect_engine_probe::ID
    ));
    assert_eq!(snapshot_accounts(&fixture.svm, &protected), before);
}

#[test]
fn unrelated_top_level_writable_alias_taints_global_privileges_and_is_rejected() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = DirectFixture::accepted(&artifacts, DIRECT_DEFAULT_AMOUNT);
    let unrelated_writable_alias =
        system_instruction::transfer(&fixture.payer.pubkey(), &fixture.mint, 0);
    assert!(unrelated_writable_alias
        .accounts
        .iter()
        .any(|meta| meta.pubkey == fixture.mint && meta.is_writable));
    let protected = fixture.rollback_state_addresses();
    let protected_before = snapshot_accounts(&fixture.svm, &protected);
    let mint_before = snapshot_accounts(&fixture.svm, &[fixture.mint]);
    let core = fixture.instruction.clone();
    let (transaction, message_resources) =
        fixture.compile_custom_v0_instructions(vec![unrelated_writable_alias, core]);
    // Legacy/v0 CompiledInstruction rows retain account indexes, not original
    // per-instruction privilege bits. The runtime and Instructions sysvar
    // therefore expose this sibling upgrade as a message-global writable
    // privilege. Rejecting it is a necessary consequence of exact
    // transaction-root authentication, not an incidental mint restriction.
    assert!(
        message_resources
            .loaded_writable_keys
            .contains(&fixture.mint),
        "v0 compilation must expose the sibling-induced global writable union"
    );
    assert!(!message_resources
        .loaded_readonly_keys
        .contains(&fixture.mint));
    let failure = fixture
        .svm
        .send_transaction(transaction)
        .expect_err("sibling-induced global writable privilege was accepted");

    assert_eq!(
        failure.err,
        TransactionError::InstructionError(3, InstructionError::Custom(6_057)),
        "Core must fail closed because Solana does not preserve the original per-instruction readonly flag"
    );
    assert!(program_invoked(
        &failure.meta.logs,
        programmable_generic_effect_core::ID
    ));
    assert!(!program_invoked(
        &failure.meta.logs,
        effect_engine_probe::ID
    ));
    assert_eq!(
        snapshot_accounts(&fixture.svm, &protected),
        protected_before,
        "rejected global privilege escalation must roll back every protected account"
    );
    assert_eq!(
        snapshot_accounts(&fixture.svm, &[fixture.mint]),
        mint_before
    );
}

#[test]
fn unrelated_top_level_writable_loader_alias_is_rejected_transaction_wide() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = DirectFixture::accepted(&artifacts, DIRECT_DEFAULT_AMOUNT);
    let unrelated_writable_alias =
        system_instruction::transfer(&fixture.payer.pubkey(), &fixture.loader_policy_account, 0);
    assert!(unrelated_writable_alias
        .accounts
        .iter()
        .any(|meta| meta.pubkey == fixture.loader_policy_account && meta.is_writable));
    let protected = fixture.rollback_state_addresses();
    let before = snapshot_accounts(&fixture.svm, &protected);
    let core = fixture.instruction.clone();
    let (transaction, _) =
        fixture.compile_custom_v0_instructions(vec![unrelated_writable_alias, core]);
    let failure = fixture
        .svm
        .send_transaction(transaction)
        .expect_err("transaction-wide loader alias was not rejected");
    assert!(program_invoked(
        &failure.meta.logs,
        programmable_generic_effect_core::ID
    ));
    assert!(!program_invoked(
        &failure.meta.logs,
        effect_engine_probe::ID
    ));
    assert_eq!(snapshot_accounts(&fixture.svm, &protected), before);
}

fn program_invoked(logs: &[String], program_id: anchor_lang::prelude::Pubkey) -> bool {
    let prefix = format!("Program {program_id} invoke");
    logs.iter().any(|line| line.starts_with(&prefix))
}

fn invalid_execute_instruction(core: &Instruction) -> Instruction {
    let mut invalid = core.clone();
    invalid
        .data
        .truncate(CORE_EXECUTE_EFFECT_DISCRIMINATOR.len());
    invalid
}

fn core_instruction_error(error: CoreError) -> InstructionError {
    InstructionError::Custom(anchor_lang::error::ERROR_CODE_OFFSET + error as u32)
}

fn forward_exact_router_instruction(core: &Instruction) -> Instruction {
    let core_account_count =
        u8::try_from(core.accounts.len()).expect("bounded Core closure for router fixture");
    let mut accounts = Vec::with_capacity(1 + core.accounts.len());
    accounts.push(AccountMeta::new_readonly(
        programmable_generic_effect_core::ID,
        false,
    ));
    accounts.extend(core.accounts.iter().cloned());
    Instruction {
        program_id: hostile_router_probe::ID,
        accounts,
        data: hostile_router_probe::instruction::Route {
            args: hostile_router_probe::RouteProbeArgs {
                core_account_count,
                mode: hostile_router_probe::RouterMode::ForwardExactOnce,
                core_instruction_data: core.data.clone(),
            },
        }
        .data(),
    }
}
