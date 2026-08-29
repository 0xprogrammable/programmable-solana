//! Creates one permanent stored-authorization tombstone in Draft state.

use anchor_lang::prelude::*;
use generic_effect_private_wire::{
    decode_core_control_instruction_exact, CoreControlInstructionCandidateV0,
    InitializeStoredAuthorizationArgsCandidateV0,
};

use crate::{
    constants::{EXPERIMENTAL_MAJOR, STORED_AUTHORIZATION_SEED},
    error::CoreError,
    runtime::{
        authenticate_actor_invocation, create_core_pda_account_exact,
        require_actor_invocation_privileges, AuthenticatedActorInvocation, RequestedPrivilege,
    },
    state::{
        initialize_stored_authorization_draft_exact, IntentIdentityCandidateV0,
        StoredAuthorizationCandidateV0,
    },
};

pub const INITIALIZE_STORED_AUTHORIZATION_ACCOUNT_COUNT: usize = 5;

const PAYER_INDEX: usize = 0;
const ACTOR_INDEX: usize = 1;
const AUTHORIZATION_INDEX: usize = 2;
const SYSTEM_PROGRAM_INDEX: usize = 3;
const INSTRUCTIONS_SYSVAR_INDEX: usize = 4;

/// Initializes the exact final PDA as an incomplete Draft. The account is
/// never closed or reused; later chunk writes may only fill currently empty
/// immutable row positions.
pub fn handle_initialize_stored_authorization(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    complete_instruction_data: &[u8],
) -> Result<()> {
    require_eq!(
        accounts.len(),
        INITIALIZE_STORED_AUTHORIZATION_ACCOUNT_COUNT,
        CoreError::AccountSegmentLengthMismatch
    );
    let args = match decode_core_control_instruction_exact(complete_instruction_data)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?
    {
        CoreControlInstructionCandidateV0::InitializeStoredAuthorization(args) => args,
        _ => return err!(CoreError::InvalidWireEncoding),
    };

    let (expected_authorization, authorization_bump) =
        StoredAuthorizationCandidateV0::address(program_id, &args.intent_digest);
    let authenticated = authenticate_actor_invocation(
        program_id,
        accounts,
        complete_instruction_data,
        &accounts[INSTRUCTIONS_SYSVAR_INDEX],
        ACTOR_INDEX,
        &[expected_authorization],
    )?;
    let payer_actor_alias = accounts[PAYER_INDEX].key == accounts[ACTOR_INDEX].key;
    require_canonical_initialize_aliasing(accounts, &authenticated)?;
    let expected_privileges = [
        RequestedPrivilege {
            key: *accounts[PAYER_INDEX].key,
            signer: true,
            writable: true,
        },
        RequestedPrivilege {
            key: *accounts[ACTOR_INDEX].key,
            signer: true,
            // The Instructions sysvar exposes global effective privileges,
            // not the original per-position AccountMeta flags. When one
            // transaction-root wallet occupies both positions, the payer's
            // writable privilege is therefore visible at both occurrences.
            writable: payer_actor_alias,
        },
        RequestedPrivilege {
            key: *accounts[AUTHORIZATION_INDEX].key,
            signer: false,
            writable: true,
        },
        RequestedPrivilege {
            key: *accounts[SYSTEM_PROGRAM_INDEX].key,
            signer: false,
            writable: false,
        },
        RequestedPrivilege {
            key: *accounts[INSTRUCTIONS_SYSVAR_INDEX].key,
            signer: false,
            writable: false,
        },
    ];
    require_actor_invocation_privileges(&authenticated, accounts, &expected_privileges)?;
    require_keys_eq!(
        *accounts[AUTHORIZATION_INDEX].key,
        expected_authorization,
        CoreError::AuthorizationIdentityMismatch
    );
    require_keys_eq!(
        *accounts[ACTOR_INDEX].key,
        Pubkey::new_from_array(args.identity.actor),
        CoreError::AuthorizationIdentityMismatch
    );

    let identity = local_identity(program_id, &args);
    identity.validate(program_id)?;
    require!(
        identity.intent_digest == args.intent_digest,
        CoreError::AuthorizationIdentityMismatch
    );

    let bump_seed = [authorization_bump];
    let signer_seeds = [
        STORED_AUTHORIZATION_SEED,
        identity.intent_digest.as_ref(),
        bump_seed.as_ref(),
    ];
    create_core_pda_account_exact(
        program_id,
        &accounts[PAYER_INDEX],
        &accounts[AUTHORIZATION_INDEX],
        &accounts[SYSTEM_PROGRAM_INDEX],
        StoredAuthorizationCandidateV0::SPACE,
        &signer_seeds,
    )?;
    initialize_stored_authorization_draft_exact(
        &accounts[AUTHORIZATION_INDEX],
        program_id,
        accounts[ACTOR_INDEX].key,
        &identity,
        args.term_count,
        args.constraint_count,
    )
}

fn local_identity(
    program_id: &Pubkey,
    args: &InitializeStoredAuthorizationArgsCandidateV0,
) -> IntentIdentityCandidateV0 {
    IntentIdentityCandidateV0 {
        experimental_major: EXPERIMENTAL_MAJOR,
        core_program: *program_id,
        actor: Pubkey::new_from_array(args.identity.actor),
        authorization_nonce: args.identity.authorization_nonce,
        market_binding_digest: args.market_binding_digest,
        loader_state_snapshot_digest: args.engine_loader_state_snapshot_digest,
        fee_policy_digest: args.fee_policy_digest,
        engine_terms_commitment: args.identity.engine_terms_commitment,
        core_terms_root: args.core_terms_root,
        reserved_digest: [0; 32],
        expires_at_slot_exclusive: args.identity.expires_at_slot_exclusive,
        max_fills: args.maximum_successful_fills,
        intent_digest: args.intent_digest,
    }
}

fn require_canonical_initialize_aliasing(
    accounts: &[AccountInfo<'_>],
    authenticated: &AuthenticatedActorInvocation,
) -> Result<()> {
    for right in 0..accounts.len() {
        for left in 0..right {
            if accounts[left].key == accounts[right].key {
                require!(
                    left == PAYER_INDEX && right == ACTOR_INDEX,
                    CoreError::DuplicateAccountIdentityDrift
                );
            }
        }
    }
    if accounts[PAYER_INDEX].key == accounts[ACTOR_INDEX].key {
        require!(
            matches!(
                authenticated,
                AuthenticatedActorInvocation::TransactionRoot(_)
            ),
            CoreError::DuplicateAccountIdentityDrift
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use generic_effect_private_wire::{InlineIntentIdentityRowCandidateV0, WIRE_VERSION};

    #[test]
    fn local_identity_preserves_every_signed_field() {
        let program_id = Pubkey::new_from_array([1; 32]);
        let args = InitializeStoredAuthorizationArgsCandidateV0 {
            wire_version: WIRE_VERSION,
            term_count: 2,
            constraint_count: 1,
            flags: 0,
            maximum_successful_fills: 3,
            identity: InlineIntentIdentityRowCandidateV0 {
                actor: [4; 32],
                engine_terms_commitment: [5; 32],
                authorization_nonce: 6,
                expires_at_slot_exclusive: 7,
            },
            market_binding_digest: [8; 32],
            engine_loader_state_snapshot_digest: [9; 32],
            fee_policy_digest: [10; 32],
            intent_capability_terms_root: [11; 32],
            credit_constraints_root: [12; 32],
            core_terms_root: [13; 32],
            intent_digest: [14; 32],
        };
        let identity = local_identity(&program_id, &args);
        assert_eq!(identity.core_program, program_id);
        assert_eq!(identity.actor.to_bytes(), args.identity.actor);
        assert_eq!(
            identity.authorization_nonce,
            args.identity.authorization_nonce
        );
        assert_eq!(identity.market_binding_digest, args.market_binding_digest);
        assert_eq!(
            identity.loader_state_snapshot_digest,
            args.engine_loader_state_snapshot_digest
        );
        assert_eq!(identity.fee_policy_digest, args.fee_policy_digest);
        assert_eq!(
            identity.engine_terms_commitment,
            args.identity.engine_terms_commitment
        );
        assert_eq!(identity.core_terms_root, args.core_terms_root);
        assert_eq!(identity.reserved_digest, [0; 32]);
        assert_eq!(
            identity.expires_at_slot_exclusive,
            args.identity.expires_at_slot_exclusive
        );
        assert_eq!(identity.max_fills, args.maximum_successful_fills);
        assert_eq!(identity.intent_digest, args.intent_digest);
    }
}
