//! One-time capture of an authority-removed loader-v3 engine observation.
//!
//! The onchain gate proves the exact relation, no controller and a strict
//! later-slot observation. Fork finality remains separate release evidence for
//! clients and indexers; this handler makes no finalization claim.

use anchor_lang::{prelude::*, solana_program::system_program};

use crate::{
    account_segments::{
        load_all_top_level_account_metas, require_readonly_non_signer, snapshot_and_union,
    },
    engine_identity::{
        validate_immutable_release_capture, EngineAdmissionPolicyCandidateV0, LoaderAccountView,
    },
    error::CoreError,
    runtime::{
        authenticate_exact_top_level_call, create_core_pda_account_exact,
        require_effective_top_level_privileges, RequestedPrivilege,
    },
    state::{
        deserialize_account_exact, serialize_account_exact, ImmutableEngineReleaseCandidateV0,
    },
};

pub const CAPTURE_IMMUTABLE_RELEASE_ACCOUNT_COUNT: usize = 6;

const PAYER_INDEX: usize = 0;
const RELEASE_INDEX: usize = 1;
const ENGINE_PROGRAM_INDEX: usize = 2;
const PROGRAM_DATA_INDEX: usize = 3;
const SYSTEM_PROGRAM_INDEX: usize = 4;
const INSTRUCTIONS_SYSVAR_INDEX: usize = 5;

/// Captures an authority-removed loader-v3 observation or accepts an existing
/// record only when its complete canonical state is exactly equal.
///
/// Account order is private and disposable:
/// payer, release PDA, engine Program, ProgramData, System program,
/// Instructions sysvar.
pub fn handle_capture_immutable_release(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    complete_instruction_data: &[u8],
    policy: EngineAdmissionPolicyCandidateV0,
) -> Result<()> {
    require_eq!(
        accounts.len(),
        CAPTURE_IMMUTABLE_RELEASE_ACCOUNT_COUNT,
        CoreError::AccountSegmentLengthMismatch
    );
    let authenticated = authenticate_exact_top_level_call(
        program_id,
        accounts,
        complete_instruction_data,
        &accounts[INSTRUCTIONS_SYSVAR_INDEX],
    )?;
    let expected_effective = [
        RequestedPrivilege {
            key: *accounts[PAYER_INDEX].key,
            signer: true,
            writable: true,
        },
        RequestedPrivilege {
            key: *accounts[RELEASE_INDEX].key,
            signer: false,
            writable: true,
        },
        RequestedPrivilege {
            key: *accounts[ENGINE_PROGRAM_INDEX].key,
            signer: false,
            writable: false,
        },
        RequestedPrivilege {
            key: *accounts[PROGRAM_DATA_INDEX].key,
            signer: false,
            writable: false,
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
    require_effective_top_level_privileges(&authenticated, &expected_effective)?;

    for (position, account) in accounts.iter().enumerate() {
        require!(
            accounts[..position]
                .iter()
                .all(|earlier| earlier.key != account.key),
            CoreError::DuplicateAccountIdentityDrift
        );
    }
    let outer_effective = snapshot_and_union(accounts, &authenticated.effective_accounts)?;
    let all_transaction_metas =
        load_all_top_level_account_metas(&accounts[INSTRUCTIONS_SYSVAR_INDEX])?;
    let loader_effective = snapshot_and_union(accounts, &all_transaction_metas)?;
    require_readonly_non_signer(&loader_effective[ENGINE_PROGRAM_INDEX])?;
    require_readonly_non_signer(&loader_effective[PROGRAM_DATA_INDEX])?;
    require_readonly_non_signer(&outer_effective[SYSTEM_PROGRAM_INDEX])?;
    require_readonly_non_signer(&outer_effective[INSTRUCTIONS_SYSVAR_INDEX])?;
    require_keys_eq!(
        *accounts[SYSTEM_PROGRAM_INDEX].key,
        system_program::ID,
        CoreError::InvalidWireEncoding
    );
    require!(
        accounts[SYSTEM_PROGRAM_INDEX].executable,
        CoreError::InvalidWireEncoding
    );

    let (release_key, release_bump) =
        ImmutableEngineReleaseCandidateV0::address(program_id, &policy.engine_program);
    require_keys_eq!(
        *accounts[RELEASE_INDEX].key,
        release_key,
        CoreError::EngineAdmissionPolicyMismatch
    );

    let current_slot = Clock::get()?.slot;
    let program_data_len = accounts[PROGRAM_DATA_INDEX].data_len();
    let validated = {
        let engine_data = accounts[ENGINE_PROGRAM_INDEX]
            .try_borrow_data()
            .map_err(|_| error!(CoreError::MalformedLoaderProgramState))?;
        let program_data = accounts[PROGRAM_DATA_INDEX]
            .try_borrow_data()
            .map_err(|_| error!(CoreError::MalformedLoaderProgramDataState))?;
        let engine_view = LoaderAccountView {
            privilege: loader_effective[ENGINE_PROGRAM_INDEX],
            data: &engine_data,
        };
        let program_data_view = LoaderAccountView {
            privilege: loader_effective[PROGRAM_DATA_INDEX],
            data: &program_data,
        };
        validate_immutable_release_capture(
            policy,
            &engine_view,
            &program_data_view,
            current_slot,
            program_id,
        )?
    };

    let mut expected = ImmutableEngineReleaseCandidateV0 {
        wire_version: crate::constants::WIRE_VERSION_V0,
        bump: release_bump,
        reserved: [0; 6],
        engine_program: policy.engine_program,
        loader_program: policy.loader_program,
        canonical_program_data: policy.program_data_or_zero,
        captured_programdata_slot: validated.loader_state_snapshot.observed_programdata_slot,
        observed_controller_or_zero: Pubkey::default(),
        captured_programdata_data_len: u64::try_from(program_data_len)
            .map_err(|_| CoreError::ArithmeticOverflow)?,
        engine_admission_policy_digest: validated.admission_policy_digest,
        loader_state_snapshot_digest: validated.loader_state_snapshot_digest,
        release_observation_digest: [0; 32],
    };
    expected.release_observation_digest =
        expected.derive_observation_digest_for_core(program_id)?;
    expected.validate(program_id, accounts[RELEASE_INDEX].key)?;

    if *accounts[RELEASE_INDEX].owner == *program_id {
        let data = accounts[RELEASE_INDEX]
            .try_borrow_data()
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        let existing: ImmutableEngineReleaseCandidateV0 =
            deserialize_account_exact(&data, ImmutableEngineReleaseCandidateV0::SPACE)?;
        existing.validate(program_id, accounts[RELEASE_INDEX].key)?;
        existing.require_exact_existing(&expected)?;
        return Ok(());
    }

    let bump_seed = [release_bump];
    let signer_seeds = [
        crate::constants::IMMUTABLE_RELEASE_SEED,
        policy.engine_program.as_ref(),
        bump_seed.as_ref(),
    ];
    create_core_pda_account_exact(
        program_id,
        &accounts[PAYER_INDEX],
        &accounts[RELEASE_INDEX],
        &accounts[SYSTEM_PROGRAM_INDEX],
        ImmutableEngineReleaseCandidateV0::SPACE,
        &signer_seeds,
    )?;
    let mut data = accounts[RELEASE_INDEX]
        .try_borrow_mut_data()
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    serialize_account_exact(
        &expected,
        &mut data,
        ImmutableEngineReleaseCandidateV0::SPACE,
    )
}
