mod common;

use anchor_lang::InstructionData;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use generic_effect_private_wire::{
    EffectReceiptCandidateV0, MoveCandidateV0, EFFECT_RECEIPT_MAGIC, PHASE_TRANSITION, WIRE_VERSION,
};
use litesvm_cpi_tree::CpiTreeExt;
use solana_message::{AccountMeta, Instruction};

use common::{
    decode_core_evidence_events, decode_execution_evidence, DirectFixture, SbfArtifacts,
    DIRECT_DEFAULT_AMOUNT,
};

#[test]
fn root_direct_events_bind_the_independent_expected_receipt() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = DirectFixture::accepted(&artifacts, DIRECT_DEFAULT_AMOUNT);
    let (transaction, _) = fixture.compile_v0();
    let metadata = fixture
        .svm
        .send_transaction(transaction)
        .unwrap_or_else(|failure| {
            panic!(
                "root direct evidence fixture failed: {:?}\n{}\n{}",
                failure.err,
                failure.meta.pretty_logs(),
                failure.meta.pretty_cpi_tree(),
            )
        });

    let expected_receipt = expected_primary_receipt(&fixture);
    let evidence = decode_execution_evidence(
        &metadata,
        effect_engine_probe::ID,
        &fixture.engine_request,
        &expected_receipt,
    )
    .expect("decode exact root evidence");
    assert!(!evidence.core_verified.routed);
    assert_eq!(evidence.core_verified.move_count, 1);
    assert_eq!(expected_receipt.moves.len(), 1);
    assert_eq!(expected_receipt.moves[0].source_capability_index, 0);
    assert_eq!(expected_receipt.moves[0].destination_capability_index, 1);
    assert_eq!(expected_receipt.moves[0].amount, DIRECT_DEFAULT_AMOUNT);
}

#[test]
fn routed_exact_delegate_events_bind_the_independent_expected_receipt() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let direct = DirectFixture::accepted(&artifacts, DIRECT_DEFAULT_AMOUNT);
    let delegated_amount = direct
        .transfer_amount
        .checked_add(direct.protocol_fee)
        .expect("delegate amount remains bounded");
    let mut fixture = direct.into_exact_delegate(delegated_amount);
    let router = forward_exact_router_instruction(&fixture.instruction);
    let (transaction, _) = fixture.compile_custom_v0(router);
    let metadata = fixture
        .svm
        .send_transaction(transaction)
        .unwrap_or_else(|failure| {
            panic!(
                "routed exact-delegate evidence fixture failed: {:?}\n{}\n{}",
                failure.err,
                failure.meta.pretty_logs(),
                failure.meta.pretty_cpi_tree(),
            )
        });

    let expected_receipt = expected_primary_receipt(&fixture);
    let evidence = decode_execution_evidence(
        &metadata,
        effect_engine_probe::ID,
        &fixture.engine_request,
        &expected_receipt,
    )
    .expect("decode exact routed evidence");
    assert!(evidence.core_verified.routed);
    assert_eq!(evidence.core_verified.move_count, 1);
    assert_eq!(expected_receipt.moves.len(), 1);
    assert_eq!(expected_receipt.moves[0].amount, DIRECT_DEFAULT_AMOUNT);
}

#[test]
fn structured_decoder_rejects_wrong_expected_receipt_trailing_and_wrong_frame_data() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = DirectFixture::accepted(&artifacts, DIRECT_DEFAULT_AMOUNT);
    let (transaction, _) = fixture.compile_v0();
    let metadata = fixture
        .svm
        .send_transaction(transaction)
        .expect("produce one accepted evidence transaction");

    let expected_receipt = expected_primary_receipt(&fixture);
    let mut wrong_expected_receipt = expected_receipt.clone();
    wrong_expected_receipt.request_digest[0] ^= 1;
    assert!(decode_execution_evidence(
        &metadata,
        effect_engine_probe::ID,
        &fixture.engine_request,
        &wrong_expected_receipt,
    )
    .err()
    .expect("wrong expected receipt must fail")
    .contains("does not exactly bind"));

    let mut invalid_expected_receipt = expected_receipt;
    invalid_expected_receipt.flags = 1;
    assert!(decode_execution_evidence(
        &metadata,
        effect_engine_probe::ID,
        &fixture.engine_request,
        &invalid_expected_receipt,
    )
    .err()
    .expect("invalid expected receipt must fail")
    .contains("invalid expected engine receipt"));

    let event_index = metadata
        .logs
        .iter()
        .position(|line| line.starts_with("Program data: "))
        .expect("accepted Core execution emitted structured event data");
    let event_line = metadata.logs[event_index].clone();
    let encoded = event_line
        .strip_prefix("Program data: ")
        .expect("event log prefix");
    let mut event_bytes = STANDARD.decode(encoded).expect("decode accepted event");

    let wrong_frame_logs = vec![
        format!("Program {} invoke [1]", effect_engine_probe::ID),
        event_line,
        format!("Program {} success", effect_engine_probe::ID),
    ];
    assert!(decode_core_evidence_events(&wrong_frame_logs)
        .err()
        .expect("wrong-frame evidence must fail")
        .contains("wrong frame"));

    event_bytes.push(0);
    let mut trailing_event_logs = metadata.logs.clone();
    trailing_event_logs[event_index] = format!("Program data: {}", STANDARD.encode(event_bytes));
    assert!(decode_core_evidence_events(&trailing_event_logs)
        .err()
        .expect("trailing event data must fail")
        .contains("trailing bytes"));

    let mut unknown_event_logs = metadata.logs;
    unknown_event_logs[event_index] = format!("Program data: {}", STANDARD.encode([0xA5_u8; 8]));
    assert!(decode_core_evidence_events(&unknown_event_logs)
        .err()
        .expect("unknown Core program-data must fail")
        .contains("unknown Core program-data frame"));
}

fn expected_primary_receipt(fixture: &DirectFixture) -> EffectReceiptCandidateV0 {
    let engine_sequence = fixture.envelope.header.expected_engine_sequence;
    EffectReceiptCandidateV0 {
        magic: EFFECT_RECEIPT_MAGIC,
        wire_version: WIRE_VERSION,
        phase: PHASE_TRANSITION,
        flags: 0,
        request_digest: fixture
            .engine_request
            .digest()
            .expect("fixture engine request digest"),
        intent_set_digest: fixture.engine_request.header.intent_set_digest,
        protected_execution_root: fixture.engine_request.header.protected_execution_root,
        engine_sequence,
        engine_supplied_evidence_digest:
            effect_engine_probe::primary_engine_supplied_evidence_digest(engine_sequence),
        moves: vec![MoveCandidateV0 {
            source_capability_index: 0,
            destination_capability_index: 1,
            amount: fixture.transfer_amount,
        }],
    }
}

fn forward_exact_router_instruction(core: &Instruction) -> Instruction {
    let core_account_count =
        u8::try_from(core.accounts.len()).expect("bounded Core closure for router fixture");
    let mut accounts = Vec::with_capacity(core.accounts.len() + 1);
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
