//! Smaller replacement ELF for real upgradeable-loader drift tests.
//!
//! This program deliberately declares the exact same disposable program ID and
//! consumes the same raw private request wire as the primary engine. It returns
//! the same protected Move plan for stateless, helper-free requests but commits
//! a distinct evidence marker. That makes a real loader upgrade observable
//! without inventing a second logical engine identity.

use anchor_lang::solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program::set_return_data,
    program_error::ProgramError, pubkey::Pubkey,
};
use generic_effect_engine_probe::{
    materialize_moves,
    plan::{EnginePlan, RECEIPT_ACCEPT},
};
use generic_effect_private_wire::{
    compute_opaque_capability_root, decode_engine_request, derive_callback_authority,
    encode_effect_receipt, EffectReceiptCandidateV0, OpaqueCapabilityDescriptorCandidateV0,
    DISPOSABLE_ENGINE_PROGRAM_ID, EFFECT_RECEIPT_MAGIC, PHASE_TRANSITION, WIRE_VERSION,
};

anchor_lang::declare_id!("3qbR1eZRqXUWroWKKYhbDmR3FfqTHfqSU8zZSxtANzYh");

#[cfg(not(feature = "no-entrypoint"))]
anchor_lang::solana_program::entrypoint!(process_instruction);

pub const REPLACEMENT_ENGINE_SUPPLIED_DIGEST_MARKER: [u8; 32] = [0xb2; 32];
pub const ENGINE_ERROR_BASE: u32 = 7_100;

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplacementError {
    InvalidProgramId = 0,
    InvalidRequest = 1,
    InvalidAccountCount = 2,
    InvalidCallback = 3,
    InvalidOpaqueClosure = 4,
    UnsupportedStatefulFixture = 5,
    ReceiptEncodingFailed = 6,
    EvidenceDigestFailed = 7,
}

const fn replacement_error(error: ReplacementError) -> ProgramError {
    ProgramError::Custom(ENGINE_ERROR_BASE + error as u32)
}

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    instruction_data: &[u8],
) -> ProgramResult {
    if *program_id != ID || *program_id != DISPOSABLE_ENGINE_PROGRAM_ID {
        return Err(replacement_error(ReplacementError::InvalidProgramId));
    }
    let request = decode_engine_request(instruction_data)
        .map_err(|_| replacement_error(ReplacementError::InvalidRequest))?;
    let (callback, opaque) = accounts
        .split_first()
        .ok_or_else(|| replacement_error(ReplacementError::InvalidAccountCount))?;
    if opaque.len() != usize::from(request.header.opaque_capability_count)
        || accounts.len() != 1 + opaque.len()
    {
        return Err(replacement_error(ReplacementError::InvalidAccountCount));
    }
    let expected_callback = derive_callback_authority(&request)
        .map_err(|_| replacement_error(ReplacementError::InvalidCallback))?
        .0;
    if !callback.is_signer
        || callback.is_writable
        || callback.executable
        || *callback.key != expected_callback
    {
        return Err(replacement_error(ReplacementError::InvalidCallback));
    }

    let mut descriptors = Vec::with_capacity(opaque.len());
    for (position, account) in opaque.iter().enumerate() {
        if account.is_signer || account.key == callback.key {
            return Err(replacement_error(ReplacementError::InvalidOpaqueClosure));
        }
        descriptors.push(OpaqueCapabilityDescriptorCandidateV0 {
            position: position as u8,
            key: account.key.to_bytes(),
            owner: account.owner.to_bytes(),
            executable: account.executable,
            effective_signer: account.is_signer,
            effective_writable: account.is_writable,
        });
    }
    let observed_root = compute_opaque_capability_root(&descriptors)
        .map_err(|_| replacement_error(ReplacementError::InvalidOpaqueClosure))?;
    if observed_root != request.header.opaque_capability_root {
        return Err(replacement_error(ReplacementError::InvalidOpaqueClosure));
    }

    let plan = EnginePlan::decode_exact(request.payload())
        .map_err(|_| replacement_error(ReplacementError::InvalidRequest))?;
    if plan.receipt_mode != RECEIPT_ACCEPT || plan.state_position_bitmap != 0 || plan.has_helper() {
        return Err(replacement_error(
            ReplacementError::UnsupportedStatefulFixture,
        ));
    }
    if plan.move_count > request.header.maximum_engine_moves {
        return Err(replacement_error(ReplacementError::InvalidRequest));
    }
    let moves = materialize_moves(&plan, &request.contexts)
        .map_err(|_| replacement_error(ReplacementError::InvalidRequest))?;
    let request_digest = request
        .digest()
        .map_err(|_| replacement_error(ReplacementError::InvalidRequest))?;
    let receipt = EffectReceiptCandidateV0 {
        magic: EFFECT_RECEIPT_MAGIC,
        wire_version: WIRE_VERSION,
        phase: PHASE_TRANSITION,
        flags: 0,
        request_digest,
        intent_set_digest: request.header.intent_set_digest,
        protected_execution_root: request.header.protected_execution_root,
        engine_sequence: 0,
        engine_supplied_evidence_digest: REPLACEMENT_ENGINE_SUPPLIED_DIGEST_MARKER,
        moves,
    };
    let encoded = encode_effect_receipt(&receipt)
        .map_err(|_| replacement_error(ReplacementError::ReceiptEncodingFailed))?;
    set_return_data(&encoded);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use generic_effect_private_wire::{
        compute_engine_attested_evidence_digest, EngineAttestedEvidenceDigestInputs,
    };

    #[test]
    fn replacement_keeps_the_exact_engine_identity() {
        assert_eq!(ID, generic_effect_engine_probe::ID);
        assert_eq!(ID, DISPOSABLE_ENGINE_PROGRAM_ID);
    }

    #[test]
    fn replacement_marker_is_observably_distinct() {
        assert_ne!(
            REPLACEMENT_ENGINE_SUPPLIED_DIGEST_MARKER,
            generic_effect_engine_probe::PRIMARY_ENGINE_SUPPLIED_DIGEST_MARKER,
        );
    }

    #[test]
    fn receipt_marker_is_raw_and_core_can_bind_it_once() {
        let engine_program = ID.to_bytes();
        let interface = [1; 32];
        let instance = [2; 32];
        let request = [3; 32];
        let replacement =
            compute_engine_attested_evidence_digest(EngineAttestedEvidenceDigestInputs {
                engine_program: &engine_program,
                engine_interface_id: &interface,
                engine_instance_id: &instance,
                request_digest: &request,
                engine_supplied_digest: &REPLACEMENT_ENGINE_SUPPLIED_DIGEST_MARKER,
            })
            .unwrap();
        let primary = compute_engine_attested_evidence_digest(EngineAttestedEvidenceDigestInputs {
            engine_program: &engine_program,
            engine_interface_id: &interface,
            engine_instance_id: &instance,
            request_digest: &request,
            engine_supplied_digest:
                &generic_effect_engine_probe::PRIMARY_ENGINE_SUPPLIED_DIGEST_MARKER,
        })
        .unwrap();
        assert_ne!(REPLACEMENT_ENGINE_SUPPLIED_DIGEST_MARKER, replacement);
        assert_ne!(replacement, primary);
    }
}
