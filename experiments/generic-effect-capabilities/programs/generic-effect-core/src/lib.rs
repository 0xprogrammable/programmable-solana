//! Disposable Core for the private generic-effect capability experiment.
//!
//! This crate deliberately exposes no stable ABI or IDL. The raw entrypoint
//! consumes only the experiment-local canonical wire.

pub mod account_segments;
pub mod authorization;
pub mod capabilities;
pub mod constants;
pub mod engine_identity;
pub mod error;
pub mod events;
pub mod fees;
pub mod heap;
pub mod instructions;
pub mod moves;
pub mod runtime;
pub mod state;
pub mod token_settlement;

use crate::{constants::INSTRUCTIONS_SYSVAR_ACCOUNT_INDEX, error::CoreError};
use anchor_lang::solana_program::{
    account_info::AccountInfo, entrypoint, entrypoint::ProgramResult, program_error::ProgramError,
    pubkey::Pubkey,
};
use generic_effect_private_wire::{
    decode_core_control_instruction_exact, CoreControlInstructionCandidateV0,
    CORE_EXECUTE_EFFECT_DISCRIMINATOR,
};

anchor_lang::declare_id!("3mg7sM6RFEBHiiFotFNfvteH1WdFcc9cujKuPaqZdfDz");

#[cfg(all(
    target_os = "solana",
    not(feature = "no-entrypoint"),
    not(feature = "custom-heap")
))]
compile_error!("the Core SBF entrypoint requires its controlled custom heap");

#[cfg(not(feature = "no-entrypoint"))]
entrypoint!(process_instruction);

/// Raw private dispatch. Every reachable control ABI is decoded exactly by the
/// canonical Wire crate before any account or state mutation occurs.
pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    instruction_data: &[u8],
) -> ProgramResult {
    if instruction_data.starts_with(&CORE_EXECUTE_EFFECT_DISCRIMINATOR) {
        let instructions_sysvar =
            accounts
                .get(INSTRUCTIONS_SYSVAR_ACCOUNT_INDEX)
                .ok_or(ProgramError::Custom(
                    anchor_lang::error::ERROR_CODE_OFFSET
                        + CoreError::AccountSegmentLengthMismatch as u32,
                ))?;
        heap::require_controlled_heap_frame(instructions_sysvar).map_err(ProgramError::from)?;
        return instructions::execute_effect_full::handle_execute_effect_full(
            program_id,
            accounts,
            instruction_data,
        )
        .map(|_| ())
        .map_err(Into::into);
    }
    let control = decode_core_control_instruction_exact(instruction_data)
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    match control {
        CoreControlInstructionCandidateV0::CaptureImmutableEngineRelease(policy) => {
            let policy = engine_identity::EngineAdmissionPolicyCandidateV0 {
                policy_kind: policy.policy_kind,
                engine_program: Pubkey::new_from_array(policy.engine_program),
                loader_program: Pubkey::new_from_array(policy.loader_program),
                program_data_or_zero: Pubkey::new_from_array(policy.program_data_or_zero),
                expected_controller_or_zero: Pubkey::new_from_array(
                    policy.expected_controller_or_zero,
                ),
                captured_programdata_slot_or_zero: policy.captured_programdata_slot_or_zero,
            };
            instructions::capture_immutable_release::handle_capture_immutable_release(
                program_id,
                accounts,
                instruction_data,
                policy,
            )
            .map_err(Into::into)
        }
        CoreControlInstructionCandidateV0::ApproveExactDelegate(args) => {
            instructions::approve_exact_delegate::handle_approve_exact_delegate(
                program_id,
                accounts,
                instruction_data,
                args.intent_digest,
                args.amount,
            )
            .map_err(Into::into)
        }
        CoreControlInstructionCandidateV0::InitializeStoredAuthorization(_) => {
            instructions::initialize_stored_authorization::handle_initialize_stored_authorization(
                program_id,
                accounts,
                instruction_data,
            )
            .map_err(Into::into)
        }
        CoreControlInstructionCandidateV0::WriteStoredAuthorizationChunk(_) => {
            instructions::write_stored_authorization::handle_write_stored_authorization(
                program_id,
                accounts,
                instruction_data,
            )
            .map_err(Into::into)
        }
        CoreControlInstructionCandidateV0::ActivateStoredAuthorization => {
            instructions::activate_stored_authorization::handle_activate_stored_authorization(
                program_id,
                accounts,
                instruction_data,
            )
            .map_err(Into::into)
        }
        CoreControlInstructionCandidateV0::ReplaceStoredAuthorization => {
            instructions::replace_stored_authorization::handle_replace_stored_authorization(
                program_id,
                accounts,
                instruction_data,
            )
            .map_err(Into::into)
        }
        CoreControlInstructionCandidateV0::CancelStoredAuthorization => {
            instructions::cancel_stored_authorization::handle_cancel_stored_authorization(
                program_id,
                accounts,
                instruction_data,
            )
            .map_err(Into::into)
        }
    }
}
