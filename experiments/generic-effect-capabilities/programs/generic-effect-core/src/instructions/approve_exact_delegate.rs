//! Top-level actor-authorized approval of one source-specific spend PDA.

use anchor_lang::{prelude::*, solana_program::program::invoke};
use anchor_spl::token::spl_token;

use crate::{
    authorization::derive_exact_spend_authority,
    error::CoreError,
    runtime::{
        authenticate_actor_invocation, require_actor_invocation_privileges, RequestedPrivilege,
    },
    token_settlement::{load_classic_spl_endpoint, load_classic_spl_mint},
};

pub const APPROVE_EXACT_DELEGATE_ACCOUNT_COUNT: usize = 6;

const ACTOR_INDEX: usize = 0;
const SOURCE_INDEX: usize = 1;
const MINT_INDEX: usize = 2;
const SPEND_AUTHORITY_INDEX: usize = 3;
const TOKEN_PROGRAM_INDEX: usize = 4;
const INSTRUCTIONS_SYSVAR_INDEX: usize = 5;

/// Approves exactly `amount` to the PDA derived from the immutable intent and
/// this source account. The approval alone is never execution authorization:
/// settlement recomputes the intent, requires this exact source relation and
/// accepts success only after the allowance is consumed completely.
pub fn handle_approve_exact_delegate(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    complete_instruction_data: &[u8],
    intent_digest: [u8; 32],
    amount: u64,
) -> Result<()> {
    require_eq!(
        accounts.len(),
        APPROVE_EXACT_DELEGATE_ACCOUNT_COUNT,
        CoreError::AccountSegmentLengthMismatch
    );
    require!(
        intent_digest != [0; 32] && amount != 0,
        CoreError::InvalidWireEncoding
    );

    let authenticated = authenticate_actor_invocation(
        program_id,
        accounts,
        complete_instruction_data,
        &accounts[INSTRUCTIONS_SYSVAR_INDEX],
        ACTOR_INDEX,
        &[*accounts[SPEND_AUTHORITY_INDEX].key],
    )?;
    let expected = [
        RequestedPrivilege {
            key: *accounts[ACTOR_INDEX].key,
            signer: true,
            writable: false,
        },
        RequestedPrivilege {
            key: *accounts[SOURCE_INDEX].key,
            signer: false,
            writable: true,
        },
        RequestedPrivilege {
            key: *accounts[MINT_INDEX].key,
            signer: false,
            writable: false,
        },
        RequestedPrivilege {
            key: *accounts[SPEND_AUTHORITY_INDEX].key,
            signer: false,
            writable: false,
        },
        RequestedPrivilege {
            key: *accounts[TOKEN_PROGRAM_INDEX].key,
            signer: false,
            writable: false,
        },
        RequestedPrivilege {
            key: *accounts[INSTRUCTIONS_SYSVAR_INDEX].key,
            signer: false,
            writable: false,
        },
    ];
    require_actor_invocation_privileges(&authenticated, accounts, &expected)?;

    for (position, account) in accounts.iter().enumerate() {
        require!(
            accounts[..position]
                .iter()
                .all(|earlier| earlier.key != account.key),
            CoreError::DuplicateAccountIdentityDrift
        );
    }
    require_keys_eq!(
        *accounts[TOKEN_PROGRAM_INDEX].key,
        spl_token::ID,
        CoreError::MoveAssetProfileMismatch
    );
    require!(
        accounts[ACTOR_INDEX].is_signer
            && !accounts[ACTOR_INDEX].is_writable
            && !accounts[ACTOR_INDEX].executable
            && accounts[SOURCE_INDEX].is_writable
            && !accounts[SOURCE_INDEX].is_signer
            && !accounts[SOURCE_INDEX].executable
            && !accounts[MINT_INDEX].is_signer
            && !accounts[MINT_INDEX].is_writable
            && !accounts[MINT_INDEX].executable
            && !accounts[INSTRUCTIONS_SYSVAR_INDEX].is_signer
            && !accounts[INSTRUCTIONS_SYSVAR_INDEX].is_writable
            && !accounts[INSTRUCTIONS_SYSVAR_INDEX].executable,
        CoreError::DirectAuthorizationNotTransactionRoot
    );
    require!(
        accounts[TOKEN_PROGRAM_INDEX].executable
            && !accounts[TOKEN_PROGRAM_INDEX].is_signer
            && !accounts[TOKEN_PROGRAM_INDEX].is_writable,
        CoreError::MoveAssetProfileMismatch
    );
    require!(
        !accounts[SPEND_AUTHORITY_INDEX].executable
            && !accounts[SPEND_AUTHORITY_INDEX].is_signer
            && !accounts[SPEND_AUTHORITY_INDEX].is_writable,
        CoreError::ExactDelegateConsumptionMismatch
    );

    let mint = load_classic_spl_mint(&accounts[MINT_INDEX])?;
    let source_before = load_classic_spl_endpoint(&accounts[SOURCE_INDEX])?;
    require_keys_eq!(
        source_before.mint,
        mint.key,
        CoreError::MoveAssetProfileMismatch
    );
    require_keys_eq!(
        source_before.authority,
        *accounts[ACTOR_INDEX].key,
        CoreError::AuthorizationIdentityMismatch
    );
    let (expected_spend_authority, _) =
        derive_exact_spend_authority(program_id, &intent_digest, accounts[SOURCE_INDEX].key)?;
    require_keys_eq!(
        *accounts[SPEND_AUTHORITY_INDEX].key,
        expected_spend_authority,
        CoreError::ExactDelegateConsumptionMismatch
    );

    let approve = spl_token::instruction::approve_checked(
        &spl_token::ID,
        accounts[SOURCE_INDEX].key,
        accounts[MINT_INDEX].key,
        accounts[SPEND_AUTHORITY_INDEX].key,
        accounts[ACTOR_INDEX].key,
        &[],
        amount,
        mint.decimals,
    )
    .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    invoke(
        &approve,
        &[
            accounts[SOURCE_INDEX].clone(),
            accounts[MINT_INDEX].clone(),
            accounts[SPEND_AUTHORITY_INDEX].clone(),
            accounts[ACTOR_INDEX].clone(),
            accounts[TOKEN_PROGRAM_INDEX].clone(),
        ],
    )?;

    let source_after = load_classic_spl_endpoint(&accounts[SOURCE_INDEX])?;
    require!(
        source_after.delegate == Some(expected_spend_authority)
            && source_after.delegated_amount == amount,
        CoreError::ExactDelegateConsumptionMismatch
    );
    Ok(())
}
