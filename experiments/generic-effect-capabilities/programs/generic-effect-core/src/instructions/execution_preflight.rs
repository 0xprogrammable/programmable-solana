//! Shared preflight guardrails for the canonical generic-effect execution path.

use anchor_lang::prelude::*;
use generic_effect_private_wire::WIRE_VERSION;

use crate::{
    account_segments::{require_readonly_non_signer, require_writable_non_signer, AccountSegments},
    constants::{
        CALLBACK_ACCOUNT_INDEX, CONFIG_ACCOUNT_INDEX, ENGINE_PROGRAM_ACCOUNT_INDEX,
        EXPERIMENTAL_MAJOR, FEE_POLICY_ACCOUNT_INDEX, INSTRUCTIONS_SYSVAR_ACCOUNT_INDEX,
        MARKET_ACCOUNT_INDEX,
    },
    engine_identity::{
        validate_loader_policy_closure_for_market_execution, LoaderAccountView,
        LoaderPolicyClosure, ValidatedEngineIdentity,
    },
    error::CoreError,
    runtime::RequestedPrivilege,
    state::{
        deserialize_account_exact, CoreConfigurationCandidateV0, ImmutableEngineReleaseCandidateV0,
        MarketDescriptorCandidateV0,
    },
};

pub(super) fn core_derived_authority_denylist(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    segments: AccountSegments,
    envelope: &generic_effect_private_wire::ExecuteEnvelopeCandidateV0,
    direct_actor_positions: &[usize],
) -> Vec<Pubkey> {
    let mut denied = Vec::new();
    let mut add = |key: Pubkey| {
        if !denied.contains(&key) {
            denied.push(key);
        }
    };
    add(*accounts[CALLBACK_ACCOUNT_INDEX].key);
    for account in &accounts[segments.loader_policy.start..segments.loader_policy.end] {
        add(*account.key);
    }
    for account in &accounts[segments.domain_controls.start..segments.domain_controls.end] {
        add(*account.key);
    }
    for (position, account) in accounts
        [segments.authorization_controls.start..segments.authorization_controls.end]
        .iter()
        .enumerate()
        .map(|(offset, account)| (segments.authorization_controls.start + offset, account))
    {
        if !direct_actor_positions.contains(&position) {
            add(*account.key);
        }
    }
    for account in &accounts[segments.fee_controls.start..segments.fee_controls.end] {
        add(*account.key);
    }
    for account in accounts {
        if account.owner == program_id {
            add(*account.key);
        }
    }
    // Keep the envelope dependency explicit: future Core-derived control
    // segments must update this list before their rows become executable.
    debug_assert_eq!(
        envelope.header.authorization_account_count,
        segments.authorization_controls.len() as u8
    );
    denied
}

pub(super) fn opaque_protected_keys(
    accounts: &[AccountInfo<'_>],
    segments: AccountSegments,
    program_id: &Pubkey,
    validated_engine: &ValidatedEngineIdentity,
) -> Vec<Pubkey> {
    let mut protected = accounts[..segments.opaque.start]
        .iter()
        .map(|account| *account.key)
        .collect::<Vec<_>>();
    let loader_v3 = anchor_lang::solana_program::bpf_loader_upgradeable::ID;
    let (core_program_data, _) = Pubkey::find_program_address(&[program_id.as_ref()], &loader_v3);
    for virtual_key in [
        *program_id,
        validated_engine.loader_state_snapshot.program_data_or_zero,
        core_program_data,
    ] {
        if !protected.contains(&virtual_key) {
            protected.push(virtual_key);
        }
    }
    protected
}

pub(super) fn requested_privileges(
    accounts: &[AccountInfo<'_>],
    segments: AccountSegments,
    envelope: &generic_effect_private_wire::ExecuteEnvelopeCandidateV0,
) -> Result<Vec<RequestedPrivilege>> {
    let mut expected = accounts
        .iter()
        .map(|account| RequestedPrivilege {
            key: *account.key,
            signer: false,
            writable: false,
        })
        .collect::<Vec<_>>();
    for snapshot in &envelope.authorization_snapshots {
        let offset = snapshot.authorization_control_offset_or_none;
        if offset == generic_effect_private_wire::NONE_INDEX {
            continue;
        }
        let position = segments.authorization_controls.start + usize::from(offset);
        let privilege = expected
            .get_mut(position)
            .ok_or(CoreError::InvalidAuthorizationSlots)?;
        match snapshot.witness_kind {
            generic_effect_private_wire::WITNESS_DIRECT_ACTOR => privilege.signer = true,
            generic_effect_private_wire::WITNESS_STORED_AUTHORIZATION => {
                privilege.writable = true;
            }
            _ => return err!(CoreError::UnsupportedAuthorizationWitness),
        }
    }
    for row in &envelope.domain_controls {
        let position = segments.domain_controls.start + usize::from(row.accounting_control_offset);
        expected
            .get_mut(position)
            .ok_or(CoreError::InvalidSettlementDomain)?
            .writable = true;
    }
    for row in &envelope.fee_shards {
        let position = segments.fee_controls.start + usize::from(row.liability_control_offset);
        expected
            .get_mut(position)
            .ok_or(CoreError::InvalidSettlementFeeShard)?
            .writable = true;
    }
    for privilege in &mut expected[segments.settlement.start..segments.settlement.end] {
        privilege.writable = true;
    }
    for (privilege, account) in expected[segments.opaque.start..segments.opaque.end]
        .iter_mut()
        .zip(&accounts[segments.opaque.start..segments.opaque.end])
    {
        privilege.writable = account.is_writable;
    }
    Ok(expected)
}

pub(super) fn validate_account_closure(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    effective: &[crate::account_segments::EffectivePrivilege],
    segments: AccountSegments,
    envelope: &generic_effect_private_wire::ExecuteEnvelopeCandidateV0,
) -> Result<()> {
    require_eq!(
        accounts.len(),
        effective.len(),
        CoreError::AccountSegmentLengthMismatch
    );
    for (position, account) in accounts[..segments.opaque.start].iter().enumerate() {
        require!(
            accounts[..position]
                .iter()
                .all(|earlier| earlier.key != account.key),
            CoreError::DuplicateAccountIdentityDrift
        );
    }
    for account in &accounts[segments.opaque.start..segments.opaque.end] {
        require!(
            accounts[..segments.opaque.start]
                .iter()
                .all(|protected| protected.key != account.key),
            CoreError::OpaqueProtectedAlias
        );
    }
    for position in [
        CONFIG_ACCOUNT_INDEX,
        MARKET_ACCOUNT_INDEX,
        FEE_POLICY_ACCOUNT_INDEX,
        CALLBACK_ACCOUNT_INDEX,
        INSTRUCTIONS_SYSVAR_ACCOUNT_INDEX,
        segments.loader_policy.start,
    ] {
        require_readonly_non_signer(&effective[position])?;
    }
    require_readonly_non_signer(&effective[ENGINE_PROGRAM_ACCOUNT_INDEX])?;
    require!(
        effective[ENGINE_PROGRAM_ACCOUNT_INDEX].executable,
        CoreError::EngineProgramNotExecutable
    );
    for snapshot in &envelope.authorization_snapshots {
        let offset = snapshot.authorization_control_offset_or_none;
        if offset == generic_effect_private_wire::NONE_INDEX {
            continue;
        }
        let privilege = effective
            .get(segments.authorization_controls.start + usize::from(offset))
            .ok_or(CoreError::InvalidAuthorizationSlots)?;
        match snapshot.witness_kind {
            generic_effect_private_wire::WITNESS_DIRECT_ACTOR => require!(
                privilege.signer && !privilege.writable && !privilege.executable,
                CoreError::DirectAuthorizationNotTransactionRoot
            ),
            generic_effect_private_wire::WITNESS_STORED_AUTHORIZATION => {
                require_writable_non_signer(privilege)?
            }
            _ => return err!(CoreError::UnsupportedAuthorizationWitness),
        }
    }
    for declaration in &envelope.settlement_capabilities {
        if declaration.spend_authority_control_offset_or_none
            == generic_effect_private_wire::NONE_INDEX
        {
            continue;
        }
        let privilege = effective
            .get(
                segments.authorization_controls.start
                    + usize::from(declaration.spend_authority_control_offset_or_none),
            )
            .ok_or(CoreError::InvalidAuthorizationSlots)?;
        require_readonly_non_signer(privilege)?;
    }
    for row in &envelope.domain_controls {
        require_readonly_non_signer(
            &effective[segments.domain_controls.start + usize::from(row.descriptor_control_offset)],
        )?;
        if row.admission_control_offset_or_none != generic_effect_private_wire::NONE_INDEX {
            require_readonly_non_signer(
                &effective[segments.domain_controls.start
                    + usize::from(row.admission_control_offset_or_none)],
            )?;
        }
        require_writable_non_signer(
            &effective[segments.domain_controls.start + usize::from(row.accounting_control_offset)],
        )?;
    }
    for row in &envelope.fee_shards {
        require_readonly_non_signer(
            &effective[segments.fee_controls.start + usize::from(row.descriptor_control_offset)],
        )?;
        require_writable_non_signer(
            &effective[segments.fee_controls.start + usize::from(row.liability_control_offset)],
        )?;
    }
    for privilege in &effective[segments.protected_profile.start..segments.protected_profile.end] {
        require_readonly_non_signer(privilege)?;
    }
    for privilege in &effective[segments.settlement.start..segments.settlement.end] {
        require_writable_non_signer(privilege)?;
    }
    require_keys_neq!(
        *accounts[ENGINE_PROGRAM_ACCOUNT_INDEX].key,
        *program_id,
        CoreError::InvalidEngineAdmissionPolicy
    );
    Ok(())
}

pub(super) fn validate_configuration(config: &CoreConfigurationCandidateV0) -> Result<()> {
    require_eq!(
        config.wire_version,
        WIRE_VERSION,
        CoreError::InvalidWireEncoding
    );
    require_eq!(
        config.experimental_major,
        EXPERIMENTAL_MAJOR,
        CoreError::InvalidWireEncoding
    );
    require!(
        config.reserved.iter().all(|byte| *byte == 0),
        CoreError::InvalidWireEncoding
    );
    require!(
        config.classic_spl_profile_digest != [0; 32]
            && config.supported_engine_interface_digest != [0; 32]
            && config.fee_policy_root != [0; 32],
        CoreError::InvalidWireEncoding
    );
    Ok(())
}

pub(super) fn validate_loader_policy_closure(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    effective: &[crate::account_segments::EffectivePrivilege],
    segments: AccountSegments,
    market: &MarketDescriptorCandidateV0,
    expected_loader_state_snapshot_digest: &[u8; 32],
    current_slot: u64,
) -> Result<ValidatedEngineIdentity> {
    let closure_position = segments.loader_policy.start;
    let closure_privilege = effective[closure_position];
    let engine_data = accounts[ENGINE_PROGRAM_ACCOUNT_INDEX]
        .try_borrow_data()
        .map_err(|_| error!(CoreError::MalformedLoaderProgramState))?;
    let engine_view = LoaderAccountView {
        privilege: effective[ENGINE_PROGRAM_ACCOUNT_INDEX],
        data: &engine_data,
    };
    if closure_privilege.owner == *program_id {
        let release = load_core_account::<ImmutableEngineReleaseCandidateV0>(
            program_id,
            &accounts[closure_position],
            ImmutableEngineReleaseCandidateV0::SPACE,
        )?;
        return validate_loader_policy_closure_for_market_execution(
            &market.engine_program,
            &market.engine_admission_policy_digest,
            expected_loader_state_snapshot_digest,
            &engine_view,
            LoaderPolicyClosure::ImmutableRelease {
                account_key: *accounts[closure_position].key,
                privilege: closure_privilege,
                release: &release,
            },
            current_slot,
            program_id,
        );
    }

    let loader_v3 = anchor_lang::solana_program::bpf_loader_upgradeable::ID;
    require_keys_eq!(
        closure_privilege.owner,
        loader_v3,
        CoreError::UnsupportedEngineLoader
    );
    let program_data = accounts[closure_position]
        .try_borrow_data()
        .map_err(|_| error!(CoreError::MalformedLoaderProgramDataState))?;
    let program_data_view = LoaderAccountView {
        privilege: closure_privilege,
        data: &program_data,
    };
    validate_loader_policy_closure_for_market_execution(
        &market.engine_program,
        &market.engine_admission_policy_digest,
        expected_loader_state_snapshot_digest,
        &engine_view,
        LoaderPolicyClosure::CurrentProgramData(&program_data_view),
        current_slot,
        program_id,
    )
}

pub(super) fn load_core_account<T>(
    program_id: &Pubkey,
    account: &AccountInfo<'_>,
    expected_space: usize,
) -> Result<T>
where
    T: AccountDeserialize + AnchorDeserialize + Discriminator,
{
    require_keys_eq!(*account.owner, *program_id, CoreError::InvalidWireEncoding);
    require!(!account.executable, CoreError::InvalidWireEncoding);
    let data = account
        .try_borrow_data()
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    deserialize_account_exact(&data, expected_space)
}
