//! Atomically replaces one actor's Active authorization with its complete Draft.

use anchor_lang::prelude::*;
use generic_effect_private_wire::{
    decode_core_control_instruction_exact, CoreControlInstructionCandidateV0,
};

use crate::{
    error::CoreError,
    runtime::{
        authenticate_actor_invocation, require_actor_invocation_privileges, RequestedPrivilege,
    },
    state::replace_stored_authorization_same_actor_exact,
};

pub const REPLACE_STORED_AUTHORIZATION_ACCOUNT_COUNT: usize = 4;

const ACTOR_INDEX: usize = 0;
const OLD_AUTHORIZATION_INDEX: usize = 1;
const NEW_AUTHORIZATION_INDEX: usize = 2;
const INSTRUCTIONS_SYSVAR_INDEX: usize = 3;

pub fn handle_replace_stored_authorization(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    complete_instruction_data: &[u8],
) -> Result<()> {
    require_eq!(
        accounts.len(),
        REPLACE_STORED_AUTHORIZATION_ACCOUNT_COUNT,
        CoreError::AccountSegmentLengthMismatch
    );
    match decode_core_control_instruction_exact(complete_instruction_data)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?
    {
        CoreControlInstructionCandidateV0::ReplaceStoredAuthorization => {}
        _ => return err!(CoreError::InvalidWireEncoding),
    }

    let forbidden_authorities = [
        *accounts[OLD_AUTHORIZATION_INDEX].key,
        *accounts[NEW_AUTHORIZATION_INDEX].key,
    ];
    let actor_proof = authenticate_actor_invocation(
        program_id,
        accounts,
        complete_instruction_data,
        &accounts[INSTRUCTIONS_SYSVAR_INDEX],
        ACTOR_INDEX,
        &forbidden_authorities,
    )?;
    let expected_privileges = [
        RequestedPrivilege {
            key: *accounts[ACTOR_INDEX].key,
            signer: true,
            writable: false,
        },
        RequestedPrivilege {
            key: *accounts[OLD_AUTHORIZATION_INDEX].key,
            signer: false,
            writable: true,
        },
        RequestedPrivilege {
            key: *accounts[NEW_AUTHORIZATION_INDEX].key,
            signer: false,
            writable: true,
        },
        RequestedPrivilege {
            key: *accounts[INSTRUCTIONS_SYSVAR_INDEX].key,
            signer: false,
            writable: false,
        },
    ];
    require_actor_invocation_privileges(&actor_proof, accounts, &expected_privileges)?;
    require_all_keys_distinct(accounts)?;

    replace_stored_authorization_same_actor_exact(
        &accounts[OLD_AUTHORIZATION_INDEX],
        &accounts[NEW_AUTHORIZATION_INDEX],
        program_id,
        accounts[ACTOR_INDEX].key,
    )
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
