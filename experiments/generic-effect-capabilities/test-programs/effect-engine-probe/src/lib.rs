//! Disposable configurable engine for the generic-effect capability experiment.
//!
//! The transition entrypoint deliberately uses the raw Solana instruction
//! bytes. Its callee account prefix is exactly one read-only Core PDA signer;
//! every following account is an ordered opaque capability. Engine-owned state
//! is selected by a payload bitmap, so zero, one, or many state accounts may
//! appear at arbitrary positions without changing that fixed prefix.

use anchor_lang::solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program::set_return_data,
    program_error::ProgramError, pubkey::Pubkey,
};
use generic_effect_private_wire::{
    compute_opaque_capability_root, decode_engine_request, derive_callback_authority,
    encode_effect_receipt, EffectReceiptCandidateV0, EngineAssetRowCandidateV0,
    EngineContextRowCandidateV0, EngineRequestCandidateV0, MoveCandidateV0,
    OpaqueCapabilityDescriptorCandidateV0, DISPOSABLE_ENGINE_PROGRAM_ID, EFFECT_RECEIPT_MAGIC,
    MAX_ENGINE_MOVES, PHASE_TRANSITION, RIGHT_CREDIT, RIGHT_DEBIT, WIRE_VERSION,
};

pub mod helper;
pub mod plan;
pub mod reference;
pub mod reference_state;
pub mod state;

use helper::{invoke_descendant_setter, invoke_increment};
use plan::{
    EnginePlan, PlanBody, PlannedMove, RECEIPT_ACCEPT, RECEIPT_DESCENDANT_SETTER,
    RECEIPT_LATE_FAILURE, RECEIPT_MISSING, RECEIPT_NONZERO_FLAGS, RECEIPT_OVERSIZED_MOVE_COUNT,
    RECEIPT_TRAILING_BYTE, RECEIPT_TRUNCATED, RECEIPT_WRONG_CAPABILITY_ROOT,
    RECEIPT_WRONG_INTENT_SET, RECEIPT_WRONG_MAGIC, RECEIPT_WRONG_PHASE,
    RECEIPT_WRONG_REQUEST_DIGEST, RECEIPT_WRONG_VERSION,
};
use state::mutate_state_account;

anchor_lang::declare_id!("3qbR1eZRqXUWroWKKYhbDmR3FfqTHfqSU8zZSxtANzYh");

#[cfg(not(feature = "no-entrypoint"))]
anchor_lang::solana_program::entrypoint!(process_instruction);

pub const PRIMARY_ENGINE_SUPPLIED_DIGEST_MARKER: [u8; 32] = [0xa1; 32];
const RECEIPT_MAGIC_OFFSET: usize = 0;
const RECEIPT_VERSION_OFFSET: usize = 8;
const RECEIPT_PHASE_OFFSET: usize = 9;
const RECEIPT_MOVE_COUNT_OFFSET: usize = 10;
const RECEIPT_FLAGS_OFFSET: usize = 11;
const RECEIPT_REQUEST_DIGEST_OFFSET: usize = 12;
const RECEIPT_INTENT_SET_OFFSET: usize = 44;
const RECEIPT_PROTECTED_ROOT_OFFSET: usize = 76;

pub type EngineResult<T> = Result<T, ProgramError>;

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineError {
    InvalidProgramId = 0,
    InvalidRequestEncoding = 1,
    InvalidAccountCount = 2,
    InvalidCallbackPrivileges = 3,
    InvalidCallbackAuthority = 4,
    CallbackOpaqueAlias = 5,
    OpaqueSignerForbidden = 6,
    OpaqueCapabilityRootMismatch = 7,
    InvalidPlanLength = 8,
    InvalidPlanVersion = 9,
    InvalidReceiptMode = 10,
    TooManyMoves = 11,
    InvalidPlanFlags = 12,
    InvalidOpaquePosition = 13,
    IncompleteHelperClosure = 14,
    AliasedHelperClosure = 15,
    InvalidPlanKind = 16,
    ZeroFanoutAmount = 17,
    InvalidFanoutContext = 18,
    InsufficientFanoutPairs = 19,
    InvalidEngineState = 20,
    InvalidEngineStateCapability = 21,
    StateSequenceMismatch = 22,
    AccountBorrowFailed = 23,
    ArithmeticOverflow = 24,
    InvalidHelperProgram = 25,
    InvalidHelperState = 26,
    HelperInvocationFailed = 27,
    ReceiptEncodingFailed = 28,
    EvidenceDigestFailed = 29,
    DeliberateLateFailure = 30,
    InvalidSemanticPlan = 31,
    InvalidSemanticContext = 32,
    InvalidReferenceState = 33,
    InvalidReferenceStateCapability = 34,
}

pub const ENGINE_ERROR_BASE: u32 = 7_000;

pub const fn engine_error(error: EngineError) -> ProgramError {
    ProgramError::Custom(ENGINE_ERROR_BASE + error as u32)
}

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    instruction_data: &[u8],
) -> ProgramResult {
    if *program_id != ID || *program_id != DISPOSABLE_ENGINE_PROGRAM_ID {
        return Err(engine_error(EngineError::InvalidProgramId));
    }

    let request = decode_engine_request(instruction_data)
        .map_err(|_| engine_error(EngineError::InvalidRequestEncoding))?;
    let request_digest = request
        .digest()
        .map_err(|_| engine_error(EngineError::InvalidRequestEncoding))?;
    let (callback_authority, opaque_accounts) = accounts
        .split_first()
        .ok_or_else(|| engine_error(EngineError::InvalidAccountCount))?;
    if opaque_accounts.len() != usize::from(request.header.opaque_capability_count)
        || accounts.len() != 1 + usize::from(request.header.opaque_capability_count)
    {
        return Err(engine_error(EngineError::InvalidAccountCount));
    }

    authenticate_callback(callback_authority, opaque_accounts, &request)?;
    authenticate_opaque_root(opaque_accounts, request.header.opaque_capability_root)?;

    let plan = EnginePlan::decode_exact(request.payload())?;
    if plan.move_count > request.header.maximum_engine_moves {
        return Err(engine_error(EngineError::TooManyMoves));
    }
    reference::validate_reference_state_positions(&plan, opaque_accounts.len())?;
    let moves = materialize_moves_with_reference_context(
        &plan,
        &request.assets,
        &request.contexts,
        opaque_accounts,
    )?;
    // This checksum is opaque engine evidence, not protected accounting. Keep
    // it bounded without preventing Core from receiving deliberately huge
    // graphs in its independent u64/u128 boundary tests.
    let mutation_amount = moves.iter().fold(0_u64, |total, movement| {
        total.saturating_add(movement.amount)
    });
    let engine_sequence = match reference::mutate_reference_states(
        &plan.body,
        opaque_accounts,
        request_digest,
        &moves,
    )? {
        Some(sequence) => sequence,
        None => mutate_selected_states(
            opaque_accounts,
            plan.state_position_bitmap,
            request_digest,
            moves.len(),
            mutation_amount,
        )?,
    };

    let helper_amount = mutation_amount.max(1);
    if plan.receipt_mode != RECEIPT_DESCENDANT_SETTER && plan.has_helper() {
        invoke_increment(callback_authority, opaque_accounts, &plan, helper_amount)?;
    }

    if plan.receipt_mode == RECEIPT_LATE_FAILURE {
        return Err(engine_error(EngineError::DeliberateLateFailure));
    }

    let engine_supplied_evidence_digest = primary_engine_supplied_evidence_digest(engine_sequence);
    let receipt = EffectReceiptCandidateV0 {
        magic: EFFECT_RECEIPT_MAGIC,
        wire_version: WIRE_VERSION,
        phase: PHASE_TRANSITION,
        flags: 0,
        request_digest,
        intent_set_digest: request.header.intent_set_digest,
        protected_execution_root: request.header.protected_execution_root,
        engine_sequence,
        engine_supplied_evidence_digest,
        moves,
    };

    emit_receipt(
        plan.receipt_mode,
        callback_authority,
        opaque_accounts,
        &plan,
        receipt,
    )
}

/// Opaque engine-local evidence. Core performs the canonical attested binding
/// exactly once after it authenticates the receipt setter.
pub fn primary_engine_supplied_evidence_digest(engine_sequence: u64) -> [u8; 32] {
    let mut digest = PRIMARY_ENGINE_SUPPLIED_DIGEST_MARKER;
    digest[24..].copy_from_slice(&engine_sequence.to_le_bytes());
    digest
}

fn authenticate_callback(
    callback_authority: &AccountInfo<'_>,
    opaque_accounts: &[AccountInfo<'_>],
    request: &EngineRequestCandidateV0,
) -> EngineResult<()> {
    if !callback_authority.is_signer
        || callback_authority.is_writable
        || callback_authority.executable
    {
        return Err(engine_error(EngineError::InvalidCallbackPrivileges));
    }
    let expected = derive_callback_authority(request)
        .map_err(|_| engine_error(EngineError::InvalidCallbackAuthority))?
        .0;
    if *callback_authority.key != expected {
        return Err(engine_error(EngineError::InvalidCallbackAuthority));
    }
    for opaque in opaque_accounts {
        if opaque.is_signer {
            return Err(engine_error(EngineError::OpaqueSignerForbidden));
        }
        if opaque.key == callback_authority.key {
            return Err(engine_error(EngineError::CallbackOpaqueAlias));
        }
    }
    Ok(())
}

fn authenticate_opaque_root(
    opaque_accounts: &[AccountInfo<'_>],
    expected_root: [u8; 32],
) -> EngineResult<()> {
    let descriptors: Vec<_> = opaque_accounts
        .iter()
        .enumerate()
        .map(
            |(position, account)| OpaqueCapabilityDescriptorCandidateV0 {
                position: position as u8,
                key: account.key.to_bytes(),
                owner: account.owner.to_bytes(),
                executable: account.executable,
                effective_signer: account.is_signer,
                effective_writable: account.is_writable,
            },
        )
        .collect();
    let observed = compute_opaque_capability_root(&descriptors)
        .map_err(|_| engine_error(EngineError::OpaqueCapabilityRootMismatch))?;
    if observed != expected_root {
        return Err(engine_error(EngineError::OpaqueCapabilityRootMismatch));
    }
    Ok(())
}

pub fn materialize_moves(
    plan: &EnginePlan,
    contexts: &[EngineContextRowCandidateV0],
) -> EngineResult<Vec<MoveCandidateV0>> {
    materialize_moves_with_reference_context(plan, &[], contexts, &[])
}

pub fn materialize_moves_with_reference_context(
    plan: &EnginePlan,
    assets: &[EngineAssetRowCandidateV0],
    contexts: &[EngineContextRowCandidateV0],
    opaque_accounts: &[AccountInfo<'_>],
) -> EngineResult<Vec<MoveCandidateV0>> {
    match &plan.body {
        PlanBody::Explicit(moves) => Ok(moves.iter().copied().map(move_from_plan).collect()),
        PlanBody::ContextFanout { unit_amount } => {
            if plan.move_count == 0 {
                return Ok(Vec::new());
            }
            let sources: Vec<_> = contexts
                .iter()
                .filter(|row| {
                    row.rights_bits & RIGHT_DEBIT != 0 && row.rights_bits & RIGHT_CREDIT == 0
                })
                .collect();
            let destinations: Vec<_> = contexts
                .iter()
                .filter(|row| {
                    row.rights_bits & RIGHT_CREDIT != 0 && row.rights_bits & RIGHT_DEBIT == 0
                })
                .collect();
            if sources.is_empty() || destinations.is_empty() {
                return Err(engine_error(EngineError::InvalidFanoutContext));
            }

            let mut moves = Vec::with_capacity(usize::from(plan.move_count));
            for source in sources {
                for destination in &destinations {
                    if source.asset_index == destination.asset_index
                        && source.settlement_capability_index
                            != destination.settlement_capability_index
                    {
                        moves.push(MoveCandidateV0 {
                            source_capability_index: source.settlement_capability_index,
                            destination_capability_index: destination.settlement_capability_index,
                            amount: *unit_amount,
                        });
                        if moves.len() == usize::from(plan.move_count) {
                            return Ok(moves);
                        }
                    }
                }
            }
            Err(engine_error(EngineError::InsufficientFanoutPairs))
        }
        PlanBody::WeightedAllocation(_)
        | PlanBody::ConstantProduct(_)
        | PlanBody::PartialAuction(_)
        | PlanBody::BatchClearing(_)
        | PlanBody::InventoryDistribution(_) => {
            reference::materialize_reference_moves(&plan.body, assets, contexts, opaque_accounts)
        }
    }
}

fn move_from_plan(planned: PlannedMove) -> MoveCandidateV0 {
    MoveCandidateV0 {
        source_capability_index: planned.source_capability_index,
        destination_capability_index: planned.destination_capability_index,
        amount: planned.amount,
    }
}

fn mutate_selected_states(
    opaque_accounts: &[AccountInfo<'_>],
    bitmap: u8,
    request_digest: [u8; 32],
    move_count: usize,
    mutation_amount: u64,
) -> EngineResult<u64> {
    let mut observed_sequence = None;
    for position in 0..8_u8 {
        if bitmap & (1_u8 << position) == 0 {
            continue;
        }
        let account = opaque_accounts
            .get(usize::from(position))
            .ok_or_else(|| engine_error(EngineError::InvalidOpaquePosition))?;
        let next_sequence =
            mutate_state_account(account, request_digest, move_count, mutation_amount)?;
        if observed_sequence.is_some_and(|sequence| sequence != next_sequence) {
            return Err(engine_error(EngineError::StateSequenceMismatch));
        }
        observed_sequence = Some(next_sequence);
    }
    Ok(observed_sequence.unwrap_or(0))
}

fn emit_receipt<'info>(
    mode: u8,
    callback_authority: &AccountInfo<'info>,
    opaque_accounts: &[AccountInfo<'info>],
    plan: &EnginePlan,
    receipt: EffectReceiptCandidateV0,
) -> EngineResult<()> {
    if mode == RECEIPT_MISSING {
        return Ok(());
    }
    let mut encoded = encode_effect_receipt(&receipt)
        .map_err(|_| engine_error(EngineError::ReceiptEncodingFailed))?;

    match mode {
        RECEIPT_ACCEPT => {}
        RECEIPT_TRUNCATED => {
            encoded.pop();
        }
        RECEIPT_TRAILING_BYTE => encoded.push(0),
        RECEIPT_WRONG_MAGIC => encoded[RECEIPT_MAGIC_OFFSET] ^= 1,
        RECEIPT_WRONG_VERSION => encoded[RECEIPT_VERSION_OFFSET] ^= 1,
        RECEIPT_WRONG_PHASE => encoded[RECEIPT_PHASE_OFFSET] ^= 1,
        RECEIPT_WRONG_REQUEST_DIGEST => encoded[RECEIPT_REQUEST_DIGEST_OFFSET] ^= 1,
        RECEIPT_WRONG_INTENT_SET => encoded[RECEIPT_INTENT_SET_OFFSET] ^= 1,
        RECEIPT_WRONG_CAPABILITY_ROOT => encoded[RECEIPT_PROTECTED_ROOT_OFFSET] ^= 1,
        RECEIPT_NONZERO_FLAGS => encoded[RECEIPT_FLAGS_OFFSET] = 1,
        RECEIPT_OVERSIZED_MOVE_COUNT => {
            encoded[RECEIPT_MOVE_COUNT_OFFSET] = (MAX_ENGINE_MOVES + 1) as u8;
        }
        RECEIPT_DESCENDANT_SETTER => {
            return invoke_descendant_setter(callback_authority, opaque_accounts, plan, encoded);
        }
        RECEIPT_MISSING | RECEIPT_LATE_FAILURE => {
            return Err(engine_error(EngineError::InvalidReceiptMode));
        }
        _ => return Err(engine_error(EngineError::InvalidReceiptMode)),
    }

    // This must be the engine's final operation after every nested CPI because
    // the runtime has one transaction return-data slot.
    set_return_data(&encoded);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use generic_effect_private_wire::{
        compute_engine_attested_evidence_digest, EngineAttestedEvidenceDigestInputs,
        EngineContextRowCandidateV0, FEE_CLASS_NONE, NONE_INDEX,
    };

    fn context(index: u8, asset: u8, rights: u16) -> EngineContextRowCandidateV0 {
        EngineContextRowCandidateV0 {
            settlement_capability_index: index,
            asset_index: asset,
            domain_index_or_none: NONE_INDEX,
            authorization_slot_or_none: NONE_INDEX,
            rights_bits: rights,
            fee_class: FEE_CLASS_NONE,
            context_flags: 0,
            endpoint_key: [index; 32],
            observed_before: 0,
            accounted_before_or_zero: 0,
            remaining_maximum_engine_debit: u64::MAX,
            remaining_maximum_total_debit: u64::MAX,
            remaining_minimum_credit: 0,
            remaining_maximum_protocol_fee: 0,
        }
    }

    #[test]
    fn program_id_matches_wire_identity() {
        assert_eq!(ID, DISPOSABLE_ENGINE_PROGRAM_ID);
    }

    #[test]
    fn receipt_carries_raw_engine_evidence_not_the_attested_digest() {
        let sequence = 9_u64;
        let supplied = primary_engine_supplied_evidence_digest(sequence);
        assert_eq!(
            &supplied[..24],
            &PRIMARY_ENGINE_SUPPLIED_DIGEST_MARKER[..24]
        );
        assert_eq!(&supplied[24..], &sequence.to_le_bytes());

        let engine_program = ID.to_bytes();
        let attested =
            compute_engine_attested_evidence_digest(EngineAttestedEvidenceDigestInputs {
                engine_program: &engine_program,
                engine_interface_id: &[1; 32],
                engine_instance_id: &[2; 32],
                request_digest: &[3; 32],
                engine_supplied_digest: &supplied,
            })
            .unwrap();
        assert_ne!(supplied, attested);
    }

    #[test]
    fn fanout_is_pair_sorted_and_asset_local() {
        let payload = plan::encode_context_fanout_plan(
            RECEIPT_ACCEPT,
            0,
            generic_effect_private_wire::NONE_INDEX,
            generic_effect_private_wire::NONE_INDEX,
            3,
            7,
        )
        .unwrap();
        let plan = EnginePlan::decode_exact(&payload).unwrap();
        let contexts = vec![
            context(0, 0, RIGHT_DEBIT),
            context(1, 1, RIGHT_DEBIT),
            context(2, 0, RIGHT_CREDIT),
            context(3, 0, RIGHT_CREDIT),
            context(4, 1, RIGHT_CREDIT),
        ];
        assert_eq!(
            materialize_moves(&plan, &contexts).unwrap(),
            vec![
                MoveCandidateV0 {
                    source_capability_index: 0,
                    destination_capability_index: 2,
                    amount: 7,
                },
                MoveCandidateV0 {
                    source_capability_index: 0,
                    destination_capability_index: 3,
                    amount: 7,
                },
                MoveCandidateV0 {
                    source_capability_index: 1,
                    destination_capability_index: 4,
                    amount: 7,
                },
            ]
        );
    }

    #[test]
    fn explicit_plan_preserves_hostile_move_bytes_for_core_validation() {
        let planned = PlannedMove {
            source_capability_index: 7,
            destination_capability_index: 7,
            amount: 0,
        };
        let payload = plan::encode_explicit_plan(
            RECEIPT_ACCEPT,
            0,
            generic_effect_private_wire::NONE_INDEX,
            generic_effect_private_wire::NONE_INDEX,
            &[planned],
        )
        .unwrap();
        let plan = EnginePlan::decode_exact(&payload).unwrap();
        assert_eq!(
            materialize_moves(&plan, &[]).unwrap(),
            vec![move_from_plan(planned)]
        );
    }
}
