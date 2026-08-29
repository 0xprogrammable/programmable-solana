//! Loader-aware engine admission and exact execution-snapshot validation.

use anchor_lang::prelude::*;
use generic_effect_private_wire::{
    EngineAdmissionPolicyCandidateV0 as WireEngineAdmissionPolicyCandidateV0,
    EngineLoaderStateSnapshotCandidateV0 as WireEngineLoaderStateSnapshotCandidateV0,
    ENGINE_ADMISSION_POLICY_LEN, ENGINE_LOADER_STATE_SNAPSHOT_LEN,
};

use crate::{
    account_segments::EffectivePrivilege,
    constants::{
        POLICY_IMMUTABLE_DEPLOYMENT, POLICY_MUTABLE_CONTROLLER_RISK,
        POLICY_PINNED_MUTABLE_DEPLOYMENT, UPGRADEABLE_LOADER_PROGRAM_BYTES,
        UPGRADEABLE_LOADER_PROGRAM_DATA_METADATA_BYTES, UPGRADEABLE_LOADER_PROGRAM_DATA_TAG,
        UPGRADEABLE_LOADER_PROGRAM_TAG,
    },
    error::CoreError,
    state::ImmutableEngineReleaseCandidateV0,
};

pub const ENGINE_ADMISSION_POLICY_ENCODED_BYTES: usize = ENGINE_ADMISSION_POLICY_LEN;
pub const ENGINE_LOADER_STATE_SNAPSHOT_ENCODED_BYTES: usize = ENGINE_LOADER_STATE_SNAPSHOT_LEN;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EngineAdmissionPolicyKind {
    ImmutableDeployment,
    PinnedMutableDeployment,
    MutableControllerRisk,
}

impl EngineAdmissionPolicyKind {
    pub fn decode(value: u8) -> Result<Self> {
        match value {
            POLICY_IMMUTABLE_DEPLOYMENT => Ok(Self::ImmutableDeployment),
            POLICY_PINNED_MUTABLE_DEPLOYMENT => Ok(Self::PinnedMutableDeployment),
            POLICY_MUTABLE_CONTROLLER_RISK => Ok(Self::MutableControllerRisk),
            _ => err!(CoreError::UnsupportedEngineAdmissionPolicy),
        }
    }

    pub fn encode(self) -> u8 {
        match self {
            Self::ImmutableDeployment => POLICY_IMMUTABLE_DEPLOYMENT,
            Self::PinnedMutableDeployment => POLICY_PINNED_MUTABLE_DEPLOYMENT,
            Self::MutableControllerRisk => POLICY_MUTABLE_CONTROLLER_RISK,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EngineAdmissionPolicyCandidateV0 {
    pub policy_kind: u8,
    pub engine_program: Pubkey,
    pub loader_program: Pubkey,
    pub program_data_or_zero: Pubkey,
    pub expected_controller_or_zero: Pubkey,
    pub captured_programdata_slot_or_zero: u64,
}

impl EngineAdmissionPolicyCandidateV0 {
    fn as_wire(self) -> WireEngineAdmissionPolicyCandidateV0 {
        WireEngineAdmissionPolicyCandidateV0 {
            policy_kind: self.policy_kind,
            engine_program: self.engine_program.to_bytes(),
            loader_program: self.loader_program.to_bytes(),
            program_data_or_zero: self.program_data_or_zero.to_bytes(),
            expected_controller_or_zero: self.expected_controller_or_zero.to_bytes(),
            captured_programdata_slot_or_zero: self.captured_programdata_slot_or_zero,
        }
    }

    pub fn encode(self) -> Result<[u8; ENGINE_ADMISSION_POLICY_ENCODED_BYTES]> {
        self.as_wire()
            .encode()
            .map_err(|_| error!(CoreError::InvalidWireEncoding))
    }

    pub fn digest(self) -> Result<[u8; 32]> {
        self.as_wire()
            .digest()
            .map_err(|_| error!(CoreError::InvalidWireEncoding))
    }

    pub fn validate_shape(self) -> Result<EngineAdmissionPolicyKind> {
        self.as_wire()
            .validate()
            .map_err(|_| error!(CoreError::InvalidEngineAdmissionPolicy))?;
        let kind = EngineAdmissionPolicyKind::decode(self.policy_kind)?;
        Ok(kind)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EngineLoaderStateSnapshotCandidateV0 {
    pub engine_program: Pubkey,
    pub loader_program: Pubkey,
    pub program_data_or_zero: Pubkey,
    pub observed_programdata_slot: u64,
    pub observed_controller_or_zero: Pubkey,
}

impl EngineLoaderStateSnapshotCandidateV0 {
    fn as_wire(self) -> WireEngineLoaderStateSnapshotCandidateV0 {
        WireEngineLoaderStateSnapshotCandidateV0 {
            engine_program: self.engine_program.to_bytes(),
            loader_program: self.loader_program.to_bytes(),
            program_data_or_zero: self.program_data_or_zero.to_bytes(),
            observed_programdata_slot: self.observed_programdata_slot,
            observed_controller_or_zero: self.observed_controller_or_zero.to_bytes(),
        }
    }

    pub fn encode(self) -> Result<[u8; ENGINE_LOADER_STATE_SNAPSHOT_ENCODED_BYTES]> {
        self.as_wire()
            .encode()
            .map_err(|_| error!(CoreError::InvalidWireEncoding))
    }

    pub fn digest(self) -> Result<[u8; 32]> {
        self.as_wire()
            .digest()
            .map_err(|_| error!(CoreError::InvalidWireEncoding))
    }
}

pub struct LoaderAccountView<'a> {
    pub privilege: EffectivePrivilege,
    pub data: &'a [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParsedProgramData {
    pub programdata_slot: u64,
    pub controller: Option<Pubkey>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedEngineIdentity {
    pub policy_kind: EngineAdmissionPolicyKind,
    pub admission_policy_digest: [u8; 32],
    pub loader_state_snapshot: EngineLoaderStateSnapshotCandidateV0,
    pub loader_state_snapshot_digest: [u8; 32],
}

pub(crate) fn validate_loader_v3_identity(
    policy: EngineAdmissionPolicyCandidateV0,
    engine_program: &LoaderAccountView<'_>,
    program_data: Option<&LoaderAccountView<'_>>,
    current_slot: u64,
    experimental_core_program: &Pubkey,
) -> Result<ValidatedEngineIdentity> {
    let kind = policy.validate_shape()?;
    let loader_v3 = anchor_lang::solana_program::bpf_loader_upgradeable::ID;

    require_keys_eq!(
        policy.loader_program,
        loader_v3,
        CoreError::UnsupportedEngineLoader
    );
    require_keys_neq!(
        policy.engine_program,
        *experimental_core_program,
        CoreError::InvalidEngineAdmissionPolicy
    );
    require_keys_eq!(
        engine_program.privilege.key,
        policy.engine_program,
        CoreError::EngineAdmissionPolicyMismatch
    );
    require_keys_eq!(
        engine_program.privilege.owner,
        loader_v3,
        CoreError::UnsupportedEngineLoader
    );
    require!(
        engine_program.privilege.executable,
        CoreError::EngineProgramNotExecutable
    );
    let (canonical_program_data, _) =
        Pubkey::find_program_address(&[policy.engine_program.as_ref()], &loader_v3);
    require_keys_eq!(
        policy.program_data_or_zero,
        canonical_program_data,
        CoreError::LoaderProgramDataRelationMismatch
    );
    require!(
        !engine_program.privilege.writable,
        CoreError::WritableLoaderIdentityAccount
    );
    require!(
        !engine_program.privilege.signer,
        CoreError::SignerLoaderIdentityAccount
    );

    let related_program_data = parse_loader_v3_program(engine_program.data)?;
    require_keys_eq!(
        related_program_data,
        policy.program_data_or_zero,
        CoreError::LoaderProgramDataRelationMismatch
    );
    let observed = match kind {
        EngineAdmissionPolicyKind::ImmutableDeployment => {
            // Ordinary immutable execution intentionally omits ProgramData.
            // Authority None makes loader-v3 mutation routes, including
            // ExtendProgram, unavailable. The canonical Core-owned release
            // record is required by the public execution wrapper below.
            require!(
                program_data.is_none(),
                CoreError::InvalidEngineAdmissionPolicy
            );
            require!(
                current_slot > policy.captured_programdata_slot_or_zero,
                CoreError::SameSlotEngineObservation
            );
            ParsedProgramData {
                programdata_slot: policy.captured_programdata_slot_or_zero,
                controller: None,
            }
        }
        EngineAdmissionPolicyKind::PinnedMutableDeployment => {
            let observed = validate_current_program_data(
                program_data.ok_or(CoreError::MalformedLoaderProgramDataState)?,
                &policy,
                current_slot,
            )?;
            require!(
                observed.controller == Some(policy.expected_controller_or_zero)
                    && observed.programdata_slot == policy.captured_programdata_slot_or_zero,
                CoreError::EngineAdmissionPolicyMismatch
            );
            observed
        }
        EngineAdmissionPolicyKind::MutableControllerRisk => {
            let observed = validate_current_program_data(
                program_data.ok_or(CoreError::MalformedLoaderProgramDataState)?,
                &policy,
                current_slot,
            )?;
            require!(
                observed.controller == Some(policy.expected_controller_or_zero),
                CoreError::EngineAdmissionPolicyMismatch
            );
            observed
        }
    };

    let loader_state_snapshot = EngineLoaderStateSnapshotCandidateV0 {
        engine_program: policy.engine_program,
        loader_program: policy.loader_program,
        program_data_or_zero: policy.program_data_or_zero,
        observed_programdata_slot: observed.programdata_slot,
        observed_controller_or_zero: observed.controller.unwrap_or_default(),
    };
    let admission_policy_digest = policy.digest()?;
    let loader_state_snapshot_digest = loader_state_snapshot.digest()?;

    Ok(ValidatedEngineIdentity {
        policy_kind: kind,
        admission_policy_digest,
        loader_state_snapshot,
        loader_state_snapshot_digest,
    })
}

pub enum LoaderPolicyClosure<'a> {
    ImmutableRelease {
        account_key: Pubkey,
        privilege: EffectivePrivilege,
        release: &'a ImmutableEngineReleaseCandidateV0,
    },
    CurrentProgramData(&'a LoaderAccountView<'a>),
}

/// Canonical market-bound ordinary-execution gate for the single loader-policy
/// account. Immutable execution resolves the market policy through the exact
/// Core-owned release record. Mutable-controller execution derives the policy
/// from the current canonical ProgramData account rather than accepting a
/// caller-declared controller or slot. The rejected pinned-mutable policy can
/// therefore never be selected by this gate.
pub fn validate_loader_policy_closure_for_market_execution(
    expected_engine_program: &Pubkey,
    expected_admission_policy_digest: &[u8; 32],
    expected_loader_state_snapshot_digest: &[u8; 32],
    engine_program: &LoaderAccountView<'_>,
    closure: LoaderPolicyClosure<'_>,
    current_slot: u64,
    experimental_core_program: &Pubkey,
) -> Result<ValidatedEngineIdentity> {
    require_keys_eq!(
        engine_program.privilege.key,
        *expected_engine_program,
        CoreError::EngineAdmissionPolicyMismatch
    );

    let (policy, closure) = match closure {
        LoaderPolicyClosure::ImmutableRelease {
            account_key,
            privilege,
            release,
        } => (
            EngineAdmissionPolicyCandidateV0 {
                policy_kind: POLICY_IMMUTABLE_DEPLOYMENT,
                engine_program: release.engine_program,
                loader_program: release.loader_program,
                program_data_or_zero: release.canonical_program_data,
                expected_controller_or_zero: Pubkey::default(),
                captured_programdata_slot_or_zero: release.captured_programdata_slot,
            },
            LoaderPolicyClosure::ImmutableRelease {
                account_key,
                privilege,
                release,
            },
        ),
        LoaderPolicyClosure::CurrentProgramData(program_data) => {
            let loader_v3 = anchor_lang::solana_program::bpf_loader_upgradeable::ID;
            let (canonical_program_data, _) =
                Pubkey::find_program_address(&[expected_engine_program.as_ref()], &loader_v3);
            require_keys_eq!(
                program_data.privilege.key,
                canonical_program_data,
                CoreError::LoaderProgramDataRelationMismatch
            );
            require_keys_eq!(
                program_data.privilege.owner,
                loader_v3,
                CoreError::UnsupportedEngineLoader
            );
            require!(
                !program_data.privilege.executable,
                CoreError::MalformedLoaderProgramDataState
            );
            require!(
                !program_data.privilege.writable,
                CoreError::WritableLoaderIdentityAccount
            );
            require!(
                !program_data.privilege.signer,
                CoreError::SignerLoaderIdentityAccount
            );
            let observed = parse_loader_v3_program_data(program_data.data)?;
            let controller = observed
                .controller
                .ok_or(CoreError::EngineAdmissionPolicyMismatch)?;
            (
                EngineAdmissionPolicyCandidateV0 {
                    policy_kind: POLICY_MUTABLE_CONTROLLER_RISK,
                    engine_program: *expected_engine_program,
                    loader_program: loader_v3,
                    program_data_or_zero: canonical_program_data,
                    expected_controller_or_zero: controller,
                    captured_programdata_slot_or_zero: 0,
                },
                LoaderPolicyClosure::CurrentProgramData(program_data),
            )
        }
    };

    require!(
        policy.digest()? == *expected_admission_policy_digest,
        CoreError::EngineAdmissionPolicyMismatch
    );
    let validated = validate_engine_identity_for_execution(
        policy,
        engine_program,
        closure,
        current_slot,
        experimental_core_program,
    )?;
    require_expected_snapshot(&validated, expected_loader_state_snapshot_digest)?;
    Ok(validated)
}

/// Typed lower-level loader-policy gate. Callers handling a market execution
/// should use `validate_loader_policy_closure_for_market_execution`, which also
/// binds the reconstructed policy and exact execution snapshot.
pub fn validate_engine_identity_for_execution(
    policy: EngineAdmissionPolicyCandidateV0,
    engine_program: &LoaderAccountView<'_>,
    closure: LoaderPolicyClosure<'_>,
    current_slot: u64,
    experimental_core_program: &Pubkey,
) -> Result<ValidatedEngineIdentity> {
    let kind = policy.validate_shape()?;
    match (kind, closure) {
        (
            EngineAdmissionPolicyKind::ImmutableDeployment,
            LoaderPolicyClosure::ImmutableRelease {
                account_key,
                privilege,
                release,
            },
        ) => {
            require_keys_eq!(
                privilege.key,
                account_key,
                CoreError::EngineAdmissionPolicyMismatch
            );
            require_keys_eq!(
                privilege.owner,
                *experimental_core_program,
                CoreError::EngineAdmissionPolicyMismatch
            );
            require!(
                !privilege.executable && !privilege.signer && !privilege.writable,
                CoreError::WritableLoaderIdentityAccount
            );
            release.validate(experimental_core_program, &account_key)?;
            require_keys_eq!(
                release.engine_program,
                policy.engine_program,
                CoreError::EngineAdmissionPolicyMismatch
            );
            require_keys_eq!(
                release.loader_program,
                policy.loader_program,
                CoreError::EngineAdmissionPolicyMismatch
            );
            require_keys_eq!(
                release.canonical_program_data,
                policy.program_data_or_zero,
                CoreError::EngineAdmissionPolicyMismatch
            );
            require_eq!(
                release.captured_programdata_slot,
                policy.captured_programdata_slot_or_zero,
                CoreError::EngineAdmissionPolicyMismatch
            );
            require!(
                release.engine_admission_policy_digest == policy.digest()?,
                CoreError::EngineAdmissionPolicyMismatch
            );
            let identity = validate_loader_v3_identity(
                policy,
                engine_program,
                None,
                current_slot,
                experimental_core_program,
            )?;
            require!(
                release.loader_state_snapshot_digest == identity.loader_state_snapshot_digest,
                CoreError::EngineLoaderStateSnapshotMismatch
            );
            Ok(identity)
        }
        (
            EngineAdmissionPolicyKind::MutableControllerRisk,
            LoaderPolicyClosure::CurrentProgramData(program_data),
        ) => validate_loader_v3_identity(
            policy,
            engine_program,
            Some(program_data),
            current_slot,
            experimental_core_program,
        ),
        (
            EngineAdmissionPolicyKind::PinnedMutableDeployment,
            LoaderPolicyClosure::CurrentProgramData(_),
        ) => err!(CoreError::UnsupportedEngineAdmissionPolicy),
        _ => err!(CoreError::EngineAdmissionPolicyMismatch),
    }
}

fn validate_current_program_data(
    program_data: &LoaderAccountView<'_>,
    policy: &EngineAdmissionPolicyCandidateV0,
    current_slot: u64,
) -> Result<ParsedProgramData> {
    let loader_v3 = anchor_lang::solana_program::bpf_loader_upgradeable::ID;
    require_keys_eq!(
        program_data.privilege.key,
        policy.program_data_or_zero,
        CoreError::LoaderProgramDataRelationMismatch
    );
    require_keys_eq!(
        program_data.privilege.owner,
        loader_v3,
        CoreError::UnsupportedEngineLoader
    );
    require!(
        !program_data.privilege.executable,
        CoreError::MalformedLoaderProgramDataState
    );
    require!(
        !program_data.privilege.writable,
        CoreError::WritableLoaderIdentityAccount
    );
    require!(
        !program_data.privilege.signer,
        CoreError::SignerLoaderIdentityAccount
    );
    let observed = parse_loader_v3_program_data(program_data.data)?;
    require!(
        current_slot > observed.programdata_slot,
        CoreError::SameSlotEngineObservation
    );
    Ok(observed)
}

/// One-time immutable release capture. This validates the current ProgramData
/// relation and removed authority before a Core-owned release record stores
/// the resulting policy. Ordinary execution then uses the policy without
/// loading ProgramData.
pub fn validate_immutable_release_capture(
    policy: EngineAdmissionPolicyCandidateV0,
    engine_program: &LoaderAccountView<'_>,
    program_data: &LoaderAccountView<'_>,
    current_slot: u64,
    experimental_core_program: &Pubkey,
) -> Result<ValidatedEngineIdentity> {
    require!(
        policy.validate_shape()? == EngineAdmissionPolicyKind::ImmutableDeployment,
        CoreError::InvalidEngineAdmissionPolicy
    );
    let loader_v3 = anchor_lang::solana_program::bpf_loader_upgradeable::ID;
    require_keys_eq!(
        policy.loader_program,
        loader_v3,
        CoreError::UnsupportedEngineLoader
    );
    require_keys_neq!(
        policy.engine_program,
        *experimental_core_program,
        CoreError::InvalidEngineAdmissionPolicy
    );
    require_keys_eq!(
        engine_program.privilege.key,
        policy.engine_program,
        CoreError::EngineAdmissionPolicyMismatch
    );
    require_keys_eq!(
        engine_program.privilege.owner,
        loader_v3,
        CoreError::UnsupportedEngineLoader
    );
    require!(
        engine_program.privilege.executable,
        CoreError::EngineProgramNotExecutable
    );
    require!(
        !engine_program.privilege.writable,
        CoreError::WritableLoaderIdentityAccount
    );
    require!(
        !engine_program.privilege.signer,
        CoreError::SignerLoaderIdentityAccount
    );
    let embedded = parse_loader_v3_program(engine_program.data)?;
    let (canonical, _) =
        Pubkey::find_program_address(&[policy.engine_program.as_ref()], &loader_v3);
    require_keys_eq!(
        embedded,
        canonical,
        CoreError::LoaderProgramDataRelationMismatch
    );
    require_keys_eq!(
        policy.program_data_or_zero,
        canonical,
        CoreError::LoaderProgramDataRelationMismatch
    );
    let observed = validate_current_program_data(program_data, &policy, current_slot)?;
    require!(
        observed.controller.is_none(),
        CoreError::EngineAdmissionPolicyMismatch
    );
    require_eq!(
        observed.programdata_slot,
        policy.captured_programdata_slot_or_zero,
        CoreError::EngineAdmissionPolicyMismatch
    );
    let loader_state_snapshot = EngineLoaderStateSnapshotCandidateV0 {
        engine_program: policy.engine_program,
        loader_program: policy.loader_program,
        program_data_or_zero: policy.program_data_or_zero,
        observed_programdata_slot: observed.programdata_slot,
        observed_controller_or_zero: Pubkey::default(),
    };
    Ok(ValidatedEngineIdentity {
        policy_kind: EngineAdmissionPolicyKind::ImmutableDeployment,
        admission_policy_digest: policy.digest()?,
        loader_state_snapshot,
        loader_state_snapshot_digest: loader_state_snapshot.digest()?,
    })
}

pub fn require_expected_snapshot(
    observed: &ValidatedEngineIdentity,
    expected_digest: &[u8; 32],
) -> Result<()> {
    require!(
        observed.loader_state_snapshot_digest == *expected_digest,
        CoreError::EngineLoaderStateSnapshotMismatch
    );
    Ok(())
}

pub fn parse_loader_v3_program(data: &[u8]) -> Result<Pubkey> {
    require!(
        data.len() >= UPGRADEABLE_LOADER_PROGRAM_BYTES,
        CoreError::MalformedLoaderProgramState
    );
    let tag = u32::from_le_bytes(
        data[0..4]
            .try_into()
            .map_err(|_| CoreError::MalformedLoaderProgramState)?,
    );
    require_eq!(
        tag,
        UPGRADEABLE_LOADER_PROGRAM_TAG,
        CoreError::MalformedLoaderProgramState
    );
    Ok(Pubkey::new_from_array(
        data[4..36]
            .try_into()
            .map_err(|_| CoreError::MalformedLoaderProgramState)?,
    ))
}

pub fn parse_loader_v3_program_data(data: &[u8]) -> Result<ParsedProgramData> {
    require!(
        data.len() > UPGRADEABLE_LOADER_PROGRAM_DATA_METADATA_BYTES,
        CoreError::MalformedLoaderProgramDataState
    );
    let tag = u32::from_le_bytes(
        data[0..4]
            .try_into()
            .map_err(|_| CoreError::MalformedLoaderProgramDataState)?,
    );
    require_eq!(
        tag,
        UPGRADEABLE_LOADER_PROGRAM_DATA_TAG,
        CoreError::MalformedLoaderProgramDataState
    );
    let programdata_slot = u64::from_le_bytes(
        data[4..12]
            .try_into()
            .map_err(|_| CoreError::MalformedLoaderProgramDataState)?,
    );
    let controller = match data[12] {
        // The official SetAuthority(None) serialization writes the None tag
        // but may retain the prior key bytes in the fixed 45-byte metadata
        // region. Those bytes are semantically unreachable and must not make a
        // legitimately authority-removed program unparsable. Fork finality is
        // an external release gate, not an on-chain loader-state claim.
        0 => None,
        1 => Some(Pubkey::new_from_array(
            data[13..45]
                .try_into()
                .map_err(|_| CoreError::MalformedLoaderProgramDataState)?,
        )),
        _ => return err!(CoreError::MalformedLoaderProgramDataState),
    };
    Ok(ParsedProgramData {
        programdata_slot,
        controller,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{UPGRADEABLE_LOADER_BUFFER_TAG, WIRE_VERSION_V0};

    fn program_state(program_data: Pubkey) -> [u8; 36] {
        let mut data = [0u8; 36];
        data[0..4].copy_from_slice(&UPGRADEABLE_LOADER_PROGRAM_TAG.to_le_bytes());
        data[4..36].copy_from_slice(program_data.as_ref());
        data
    }

    fn program_data_state(slot: u64, controller: Option<Pubkey>) -> [u8; 46] {
        let mut data = [0u8; 46];
        data[0..4].copy_from_slice(&UPGRADEABLE_LOADER_PROGRAM_DATA_TAG.to_le_bytes());
        data[4..12].copy_from_slice(&slot.to_le_bytes());
        if let Some(controller) = controller {
            data[12] = 1;
            data[13..45].copy_from_slice(controller.as_ref());
        }
        data
    }

    fn privilege(key: Pubkey, owner: Pubkey, executable: bool) -> EffectivePrivilege {
        EffectivePrivilege {
            key,
            owner,
            executable,
            signer: false,
            writable: false,
        }
    }

    #[test]
    fn policy_and_snapshot_lengths_are_exact() {
        let policy = EngineAdmissionPolicyCandidateV0 {
            policy_kind: POLICY_IMMUTABLE_DEPLOYMENT,
            engine_program: Pubkey::new_from_array([1; 32]),
            loader_program: anchor_lang::solana_program::bpf_loader_upgradeable::ID,
            program_data_or_zero: Pubkey::new_from_array([3; 32]),
            expected_controller_or_zero: Pubkey::default(),
            captured_programdata_slot_or_zero: 9,
        };
        assert_eq!(policy.encode().unwrap().len(), 144);
        let snapshot = EngineLoaderStateSnapshotCandidateV0 {
            engine_program: policy.engine_program,
            loader_program: policy.loader_program,
            program_data_or_zero: policy.program_data_or_zero,
            observed_programdata_slot: 9,
            observed_controller_or_zero: Pubkey::default(),
        };
        assert_eq!(snapshot.encode().unwrap().len(), 136);
        assert_ne!(policy.digest().unwrap(), snapshot.digest().unwrap());
    }

    #[test]
    fn loader_v3_requires_later_slot_and_exact_controller_policy() {
        let loader = anchor_lang::solana_program::bpf_loader_upgradeable::ID;
        let engine = Pubkey::new_unique();
        let (program_data_key, _) = Pubkey::find_program_address(&[engine.as_ref()], &loader);
        let controller = Pubkey::new_unique();
        let core = Pubkey::new_unique();
        let program_bytes = program_state(program_data_key);
        let program_data_bytes = program_data_state(10, Some(controller));
        let program = LoaderAccountView {
            privilege: privilege(engine, loader, true),
            data: &program_bytes,
        };
        let program_data = LoaderAccountView {
            privilege: privilege(program_data_key, loader, false),
            data: &program_data_bytes,
        };
        let policy = EngineAdmissionPolicyCandidateV0 {
            policy_kind: POLICY_MUTABLE_CONTROLLER_RISK,
            engine_program: engine,
            loader_program: loader,
            program_data_or_zero: program_data_key,
            expected_controller_or_zero: controller,
            captured_programdata_slot_or_zero: 0,
        };
        assert!(
            validate_loader_v3_identity(policy, &program, Some(&program_data), 10, &core).is_err()
        );
        let identity =
            validate_loader_v3_identity(policy, &program, Some(&program_data), 11, &core).unwrap();
        assert_eq!(identity.loader_state_snapshot.observed_programdata_slot, 10);
        assert_eq!(
            identity.loader_state_snapshot.observed_controller_or_zero,
            controller
        );
    }

    #[test]
    fn market_loader_gate_derives_mutable_policy_and_rejects_pinned_or_privileged_state() {
        let loader = anchor_lang::solana_program::bpf_loader_upgradeable::ID;
        let engine = Pubkey::new_unique();
        let (program_data_key, _) = Pubkey::find_program_address(&[engine.as_ref()], &loader);
        let controller = Pubkey::new_unique();
        let core = Pubkey::new_unique();
        let program_bytes = program_state(program_data_key);
        let program_data_bytes = program_data_state(10, Some(controller));
        let program = LoaderAccountView {
            privilege: privilege(engine, loader, true),
            data: &program_bytes,
        };
        let program_data = LoaderAccountView {
            privilege: privilege(program_data_key, loader, false),
            data: &program_data_bytes,
        };
        let mutable_policy = EngineAdmissionPolicyCandidateV0 {
            policy_kind: POLICY_MUTABLE_CONTROLLER_RISK,
            engine_program: engine,
            loader_program: loader,
            program_data_or_zero: program_data_key,
            expected_controller_or_zero: controller,
            captured_programdata_slot_or_zero: 0,
        };
        let snapshot = EngineLoaderStateSnapshotCandidateV0 {
            engine_program: engine,
            loader_program: loader,
            program_data_or_zero: program_data_key,
            observed_programdata_slot: 10,
            observed_controller_or_zero: controller,
        };

        let validated = validate_loader_policy_closure_for_market_execution(
            &engine,
            &mutable_policy.digest().unwrap(),
            &snapshot.digest().unwrap(),
            &program,
            LoaderPolicyClosure::CurrentProgramData(&program_data),
            11,
            &core,
        )
        .unwrap();
        assert_eq!(
            validated.policy_kind,
            EngineAdmissionPolicyKind::MutableControllerRisk
        );

        let pinned_policy = EngineAdmissionPolicyCandidateV0 {
            policy_kind: POLICY_PINNED_MUTABLE_DEPLOYMENT,
            captured_programdata_slot_or_zero: 10,
            ..mutable_policy
        };
        assert!(validate_loader_policy_closure_for_market_execution(
            &engine,
            &pinned_policy.digest().unwrap(),
            &snapshot.digest().unwrap(),
            &program,
            LoaderPolicyClosure::CurrentProgramData(&program_data),
            11,
            &core,
        )
        .is_err());

        let writable_program_data = LoaderAccountView {
            privilege: EffectivePrivilege {
                writable: true,
                ..program_data.privilege
            },
            data: &program_data_bytes,
        };
        assert!(validate_loader_policy_closure_for_market_execution(
            &engine,
            &mutable_policy.digest().unwrap(),
            &snapshot.digest().unwrap(),
            &program,
            LoaderPolicyClosure::CurrentProgramData(&writable_program_data),
            11,
            &core,
        )
        .is_err());
        assert!(validate_loader_policy_closure_for_market_execution(
            &engine,
            &mutable_policy.digest().unwrap(),
            &snapshot.digest().unwrap(),
            &program,
            LoaderPolicyClosure::CurrentProgramData(&program_data),
            10,
            &core,
        )
        .is_err());
    }

    #[test]
    fn market_loader_gate_accepts_only_the_exact_immutable_release_evidence() {
        let loader = anchor_lang::solana_program::bpf_loader_upgradeable::ID;
        let engine = Pubkey::new_unique();
        let core = Pubkey::new_unique();
        let (program_data_key, _) = Pubkey::find_program_address(&[engine.as_ref()], &loader);
        let program_bytes = program_state(program_data_key);
        let program = LoaderAccountView {
            privilege: privilege(engine, loader, true),
            data: &program_bytes,
        };
        let policy = EngineAdmissionPolicyCandidateV0 {
            policy_kind: POLICY_IMMUTABLE_DEPLOYMENT,
            engine_program: engine,
            loader_program: loader,
            program_data_or_zero: program_data_key,
            expected_controller_or_zero: Pubkey::default(),
            captured_programdata_slot_or_zero: 7,
        };
        let snapshot = EngineLoaderStateSnapshotCandidateV0 {
            engine_program: engine,
            loader_program: loader,
            program_data_or_zero: program_data_key,
            observed_programdata_slot: 7,
            observed_controller_or_zero: Pubkey::default(),
        };
        let (release_key, bump) = ImmutableEngineReleaseCandidateV0::address(&core, &engine);
        let mut release = ImmutableEngineReleaseCandidateV0 {
            wire_version: WIRE_VERSION_V0,
            bump,
            reserved: [0; 6],
            engine_program: engine,
            loader_program: loader,
            canonical_program_data: program_data_key,
            captured_programdata_slot: 7,
            observed_controller_or_zero: Pubkey::default(),
            captured_programdata_data_len: 46,
            engine_admission_policy_digest: policy.digest().unwrap(),
            loader_state_snapshot_digest: snapshot.digest().unwrap(),
            release_observation_digest: [0; 32],
        };
        release.release_observation_digest =
            release.derive_observation_digest_for_core(&core).unwrap();
        let release_privilege = privilege(release_key, core, false);

        validate_loader_policy_closure_for_market_execution(
            &engine,
            &policy.digest().unwrap(),
            &snapshot.digest().unwrap(),
            &program,
            LoaderPolicyClosure::ImmutableRelease {
                account_key: release_key,
                privilege: release_privilege,
                release: &release,
            },
            8,
            &core,
        )
        .unwrap();

        let mut poisoned = release;
        poisoned.captured_programdata_data_len += 1;
        assert!(validate_loader_policy_closure_for_market_execution(
            &engine,
            &policy.digest().unwrap(),
            &snapshot.digest().unwrap(),
            &program,
            LoaderPolicyClosure::ImmutableRelease {
                account_key: release_key,
                privilege: release_privilege,
                release: &poisoned,
            },
            8,
            &core,
        )
        .is_err());
    }

    #[test]
    fn authority_removed_program_data_ignores_stale_former_controller_bytes() {
        let former_controller = Pubkey::new_unique();
        let mut data = program_data_state(7, Some(former_controller));
        data[12] = 0;
        let parsed = parse_loader_v3_program_data(&data).unwrap();
        assert_eq!(parsed.programdata_slot, 7);
        assert_eq!(parsed.controller, None);
    }

    #[test]
    fn oversized_program_account_decodes_canonical_prefix() {
        let program_data = Pubkey::new_unique();
        let mut oversized = program_state(program_data).to_vec();
        oversized.extend_from_slice(&[0xa5; 64]);
        assert_eq!(parse_loader_v3_program(&oversized).unwrap(), program_data);
        assert!(parse_loader_v3_program(&oversized[..35]).is_err());
    }

    #[test]
    fn loader_parsers_reject_wrong_tags_options_and_empty_elf() {
        let program_data = Pubkey::new_unique();
        let mut program = program_state(program_data);
        program[0..4].copy_from_slice(&UPGRADEABLE_LOADER_BUFFER_TAG.to_le_bytes());
        assert!(parse_loader_v3_program(&program).is_err());

        let mut data = program_data_state(0, None);
        data[0..4].copy_from_slice(&UPGRADEABLE_LOADER_PROGRAM_TAG.to_le_bytes());
        assert!(parse_loader_v3_program_data(&data).is_err());

        let mut malformed_option = program_data_state(0, None);
        malformed_option[12] = 2;
        assert!(parse_loader_v3_program_data(&malformed_option).is_err());
        assert!(parse_loader_v3_program_data(&malformed_option[..45]).is_err());
    }
}
