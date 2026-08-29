//! Writes one exact, non-overlapping immutable Draft chunk.

use anchor_lang::prelude::*;
use generic_effect_private_wire::{
    decode_core_control_instruction_exact, CoreControlInstructionCandidateV0,
    CreditConstraintRowCandidateV0, IntentCapabilityTermRowCandidateV0,
    StoredAuthorizationChunkRowsCandidateV0,
};

use crate::{
    error::CoreError,
    runtime::{
        authenticate_actor_invocation, require_actor_invocation_privileges, RequestedPrivilege,
    },
    state::{
        write_stored_authorization_constraint_chunk_exact,
        write_stored_authorization_term_chunk_exact, StoredCreditConstraintCandidateV0,
        StoredIntentCapabilityTermCandidateV0,
    },
};

pub const WRITE_STORED_AUTHORIZATION_ACCOUNT_COUNT: usize = 3;

const ACTOR_INDEX: usize = 0;
const AUTHORIZATION_INDEX: usize = 1;
const INSTRUCTIONS_SYSVAR_INDEX: usize = 2;

pub fn handle_write_stored_authorization(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    complete_instruction_data: &[u8],
) -> Result<()> {
    require_eq!(
        accounts.len(),
        WRITE_STORED_AUTHORIZATION_ACCOUNT_COUNT,
        CoreError::AccountSegmentLengthMismatch
    );
    let chunk = match decode_core_control_instruction_exact(complete_instruction_data)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?
    {
        CoreControlInstructionCandidateV0::WriteStoredAuthorizationChunk(chunk) => chunk,
        _ => return err!(CoreError::InvalidWireEncoding),
    };

    let authenticated = authenticate_actor_invocation(
        program_id,
        accounts,
        complete_instruction_data,
        &accounts[INSTRUCTIONS_SYSVAR_INDEX],
        ACTOR_INDEX,
        &[*accounts[AUTHORIZATION_INDEX].key],
    )?;
    let expected_privileges = [
        RequestedPrivilege {
            key: *accounts[ACTOR_INDEX].key,
            signer: true,
            writable: false,
        },
        RequestedPrivilege {
            key: *accounts[AUTHORIZATION_INDEX].key,
            signer: false,
            writable: true,
        },
        RequestedPrivilege {
            key: *accounts[INSTRUCTIONS_SYSVAR_INDEX].key,
            signer: false,
            writable: false,
        },
    ];
    require_actor_invocation_privileges(&authenticated, accounts, &expected_privileges)?;
    require_all_keys_distinct(accounts)?;

    match chunk.rows {
        StoredAuthorizationChunkRowsCandidateV0::Terms(rows) => {
            let stored = rows.iter().map(stored_term).collect::<Vec<_>>();
            write_stored_authorization_term_chunk_exact(
                &accounts[AUTHORIZATION_INDEX],
                program_id,
                accounts[ACTOR_INDEX].key,
                chunk.header.start_index,
                &stored,
            )
        }
        StoredAuthorizationChunkRowsCandidateV0::Constraints(rows) => {
            let stored = rows.iter().map(stored_constraint).collect::<Vec<_>>();
            write_stored_authorization_constraint_chunk_exact(
                &accounts[AUTHORIZATION_INDEX],
                program_id,
                accounts[ACTOR_INDEX].key,
                chunk.header.start_index,
                &stored,
            )
        }
    }
}

fn stored_term(row: &IntentCapabilityTermRowCandidateV0) -> StoredIntentCapabilityTermCandidateV0 {
    StoredIntentCapabilityTermCandidateV0 {
        intent_local_term_index: row.intent_local_term_index,
        authority_class: row.authority_class,
        fee_class: row.fee_class,
        flags: row.flags,
        rights_bits: row.rights_bits,
        reserved: [0; 2],
        endpoint_key: Pubkey::new_from_array(row.endpoint_key),
        asset_binding_digest: row.asset_binding_digest,
        required_domain_descriptor_digest_or_zero: row.required_domain_descriptor_digest_or_zero,
        maximum_engine_debit: row.maximum_engine_debit,
        maximum_total_debit: row.maximum_total_debit,
        minimum_credit: row.minimum_credit,
        maximum_protocol_fee: row.maximum_protocol_fee,
    }
}

fn stored_constraint(row: &CreditConstraintRowCandidateV0) -> StoredCreditConstraintCandidateV0 {
    StoredCreditConstraintCandidateV0 {
        constraint_index: row.constraint_index,
        credit_local_term_index: row.credit_local_term_index,
        flags: row.flags,
        reserved: [0; 3],
        debit_source_bitmap: row.debit_source_bitmap,
        debit_group_root: row.debit_group_root,
        minimum_credit_numerator: row.minimum_credit_numerator,
        nonzero_debit_denominator: row.nonzero_debit_denominator,
        terminal_absolute_minimum: row.terminal_absolute_minimum,
    }
}

fn require_all_keys_distinct(accounts: &[AccountInfo<'_>]) -> Result<()> {
    for (position, account) in accounts.iter().enumerate() {
        require!(
            accounts[..position]
                .iter()
                .all(|earlier| earlier.key != account.key),
            CoreError::DuplicateAccountIdentityDrift
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use generic_effect_private_wire::{
        compute_intent_debit_group_root, AUTHORITY_EXACT_EXTERNAL_CREDIT, FEE_CLASS_NONE,
        RIGHT_CREDIT, RIGHT_EXACT_EXTERNAL_RECIPIENT,
    };

    #[test]
    fn wire_rows_convert_without_semantic_or_reserved_byte_drift() {
        let term = IntentCapabilityTermRowCandidateV0 {
            intent_local_term_index: 0,
            authority_class: AUTHORITY_EXACT_EXTERNAL_CREDIT,
            fee_class: FEE_CLASS_NONE,
            flags: 0,
            rights_bits: RIGHT_CREDIT | RIGHT_EXACT_EXTERNAL_RECIPIENT,
            endpoint_key: [1; 32],
            asset_binding_digest: [2; 32],
            required_domain_descriptor_digest_or_zero: [0; 32],
            maximum_engine_debit: 0,
            maximum_total_debit: 0,
            minimum_credit: 3,
            maximum_protocol_fee: 0,
        };
        assert_eq!(stored_term(&term).wire_row().unwrap(), term);

        let constraint = CreditConstraintRowCandidateV0 {
            constraint_index: 0,
            credit_local_term_index: 0,
            flags: 0,
            debit_source_bitmap: 1 << 1,
            debit_group_root: compute_intent_debit_group_root(&[1]).unwrap(),
            minimum_credit_numerator: 1,
            nonzero_debit_denominator: 2,
            terminal_absolute_minimum: 3,
        };
        assert_eq!(
            stored_constraint(&constraint).wire_row().unwrap(),
            constraint
        );
    }
}
