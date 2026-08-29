//! Private account state for the disposable experiment.
//!
//! These layouts deliberately remain local to this nested workspace.

use anchor_lang::prelude::*;
use generic_effect_private_wire::{
    compute_authorization_capability_state_root, compute_authorization_fee_state_root,
    compute_domain_admission_address_digest, compute_exact_engine_instance_policy_digest,
    compute_exact_fee_recipient_policy_digest, compute_fee_shard_descriptor_digest,
    compute_immutable_engine_release_observation_digest, compute_intent_capability_terms_root,
    compute_intent_core_terms_root, compute_intent_credit_constraints_root, compute_intent_digest,
    AuthorizationCapabilityStateRowCandidateV0, AuthorizationFeeStateRowCandidateV0,
    CreditConstraintRowCandidateV0, DomainAdmissionCandidateV0, DomainDescriptorRowCandidateV0,
    FeeShardDescriptorRowCandidateV0, ImmutableEngineReleaseObservationCandidateV0,
    InlineIntentIdentityRowCandidateV0, IntentCapabilityTermRowCandidateV0,
    IntentCoreTermsDigestInputs, IntentDigestInputs, StoredAuthorizationHeaderCandidateV0,
    INTENT_CAPABILITY_TERM_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT,
    INTENT_CAPABILITY_TERM_FLAG_FEE_FUNDING,
};

use crate::{
    constants::{
        AUTHORITY_EXACT_EXTERNAL_CREDIT, AUTHORITY_INTENT_FUNDED_DEBIT, DOMAIN_ACCOUNTING_SEED,
        DOMAIN_ADMISSION_SEED, FEE_CLASS_GROSS_DEBIT_RATE, FEE_CLASS_NONE, FEE_LIABILITY_SEED,
        FEE_SHARD_DESCRIPTOR_SEED, IMMUTABLE_RELEASE_SEED, MAX_ASSETS, MAX_SETTLEMENT_CAPABILITIES,
        POLICY_IMMUTABLE_DEPLOYMENT, RIGHT_CREDIT, RIGHT_DEBIT, RIGHT_EXACT_EXTERNAL_RECIPIENT,
        STORED_AUTHORIZATION_SEED, WIRE_VERSION_V0,
    },
    engine_identity::{EngineAdmissionPolicyCandidateV0, EngineLoaderStateSnapshotCandidateV0},
    error::CoreError,
};

pub const MAX_STORED_INTENT_TERMS: usize = MAX_SETTLEMENT_CAPABILITIES;
pub const MAX_STORED_CREDIT_CONSTRAINTS: usize = MAX_SETTLEMENT_CAPABILITIES;
pub const MAX_STORED_FEE_STATES: usize = MAX_SETTLEMENT_CAPABILITIES;

pub const STORED_AUTHORIZATION_HEADER_OFFSET: usize = 8;
pub const STORED_AUTHORIZATION_IDENTITY_OFFSET: usize = STORED_AUTHORIZATION_HEADER_OFFSET
    + generic_effect_private_wire::STORED_AUTHORIZATION_HEADER_LEN;
pub const STORED_AUTHORIZATION_PENDING_OFFSET: usize =
    STORED_AUTHORIZATION_IDENTITY_OFFSET + IntentIdentityCandidateV0::ENCODED_LEN;
pub const STORED_AUTHORIZATION_TERMS_OFFSET: usize = STORED_AUTHORIZATION_PENDING_OFFSET + 32;
pub const STORED_AUTHORIZATION_CONSTRAINTS_OFFSET: usize = STORED_AUTHORIZATION_TERMS_OFFSET
    + MAX_STORED_INTENT_TERMS * StoredIntentCapabilityTermCandidateV0::ENCODED_LEN;
pub const STORED_AUTHORIZATION_CAPABILITIES_OFFSET: usize = STORED_AUTHORIZATION_CONSTRAINTS_OFFSET
    + MAX_STORED_CREDIT_CONSTRAINTS * StoredCreditConstraintCandidateV0::ENCODED_LEN;
pub const STORED_AUTHORIZATION_FEES_OFFSET: usize = STORED_AUTHORIZATION_CAPABILITIES_OFFSET
    + MAX_SETTLEMENT_CAPABILITIES * AuthorizationCapabilityStateCandidateV0::ENCODED_LEN;
pub const STORED_AUTHORIZATION_ACCOUNT_DISCRIMINATOR: [u8; 8] =
    [0x99, 0xcb, 0xce, 0x98, 0xc7, 0xc8, 0x83, 0x67];

fn exact_prefix_bitmap(count: usize) -> Result<u16> {
    require!(
        count <= u16::BITS as usize,
        CoreError::ExperimentLimitExceeded
    );
    if count == 0 {
        Ok(0)
    } else {
        u16::try_from((1_u32 << count) - 1).map_err(|_| error!(CoreError::ExperimentLimitExceeded))
    }
}

#[account]
#[derive(Debug, Eq, PartialEq)]
pub struct ImmutableEngineReleaseCandidateV0 {
    pub wire_version: u8,
    pub bump: u8,
    pub reserved: [u8; 6],
    pub engine_program: Pubkey,
    pub loader_program: Pubkey,
    pub canonical_program_data: Pubkey,
    /// Captured loader-v3 ProgramData last-modified slot. This is release
    /// observation evidence, not a deployment or ELF-content identity claim.
    pub captured_programdata_slot: u64,
    /// Must be all-zero: the capture observed authority None.
    pub observed_controller_or_zero: Pubkey,
    pub captured_programdata_data_len: u64,
    pub engine_admission_policy_digest: [u8; 32],
    pub loader_state_snapshot_digest: [u8; 32],
    pub release_observation_digest: [u8; 32],
}

impl ImmutableEngineReleaseCandidateV0 {
    pub const DATA_LEN: usize = 248;
    pub const SPACE: usize = 8 + Self::DATA_LEN;

    pub fn address(core_program: &Pubkey, engine_program: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[IMMUTABLE_RELEASE_SEED, engine_program.as_ref()],
            core_program,
        )
    }

    pub fn observation_row(&self) -> ImmutableEngineReleaseObservationCandidateV0 {
        ImmutableEngineReleaseObservationCandidateV0 {
            engine_program: self.engine_program.to_bytes(),
            loader_program: self.loader_program.to_bytes(),
            canonical_program_data: self.canonical_program_data.to_bytes(),
            captured_programdata_slot: self.captured_programdata_slot,
            observed_controller_or_zero: self.observed_controller_or_zero.to_bytes(),
            captured_programdata_data_len: self.captured_programdata_data_len,
            engine_admission_policy_digest: self.engine_admission_policy_digest,
            loader_state_snapshot_digest: self.loader_state_snapshot_digest,
        }
    }

    pub fn derive_observation_digest_for_core(&self, core_program: &Pubkey) -> Result<[u8; 32]> {
        compute_immutable_engine_release_observation_digest(
            &core_program.to_bytes(),
            &self.observation_row(),
        )
        .map_err(|_| error!(CoreError::InvalidWireEncoding))
    }

    pub fn derive_observation_digest(&self) -> Result<[u8; 32]> {
        self.derive_observation_digest_for_core(&crate::ID)
    }

    pub fn validate(&self, core_program: &Pubkey, account_key: &Pubkey) -> Result<()> {
        require_eq!(
            self.wire_version,
            WIRE_VERSION_V0,
            CoreError::InvalidWireEncoding
        );
        require!(
            self.reserved.iter().all(|byte| *byte == 0),
            CoreError::InvalidWireEncoding
        );
        require!(
            self.observed_controller_or_zero == Pubkey::default(),
            CoreError::EngineAdmissionPolicyMismatch
        );
        require!(
            self.captured_programdata_data_len > 45,
            CoreError::MalformedLoaderProgramDataState
        );
        let loader_v3 = anchor_lang::solana_program::bpf_loader_upgradeable::ID;
        require_keys_eq!(
            self.loader_program,
            loader_v3,
            CoreError::UnsupportedEngineLoader
        );
        let (canonical_program_data, _) =
            Pubkey::find_program_address(&[self.engine_program.as_ref()], &loader_v3);
        require_keys_eq!(
            self.canonical_program_data,
            canonical_program_data,
            CoreError::LoaderProgramDataRelationMismatch
        );
        let (expected, bump) = Self::address(core_program, &self.engine_program);
        require_keys_eq!(
            *account_key,
            expected,
            CoreError::EngineAdmissionPolicyMismatch
        );
        require_eq!(self.bump, bump, CoreError::EngineAdmissionPolicyMismatch);
        let policy = EngineAdmissionPolicyCandidateV0 {
            policy_kind: POLICY_IMMUTABLE_DEPLOYMENT,
            engine_program: self.engine_program,
            loader_program: self.loader_program,
            program_data_or_zero: self.canonical_program_data,
            expected_controller_or_zero: Pubkey::default(),
            captured_programdata_slot_or_zero: self.captured_programdata_slot,
        };
        require!(
            self.engine_admission_policy_digest == policy.digest()?,
            CoreError::EngineAdmissionPolicyMismatch
        );
        let snapshot = EngineLoaderStateSnapshotCandidateV0 {
            engine_program: self.engine_program,
            loader_program: self.loader_program,
            program_data_or_zero: self.canonical_program_data,
            observed_programdata_slot: self.captured_programdata_slot,
            observed_controller_or_zero: Pubkey::default(),
        };
        require!(
            self.loader_state_snapshot_digest == snapshot.digest()?,
            CoreError::EngineLoaderStateSnapshotMismatch
        );
        require!(
            self.release_observation_digest
                == self.derive_observation_digest_for_core(core_program)?,
            CoreError::EngineAdmissionPolicyMismatch
        );
        Ok(())
    }

    /// Idempotent creation accepts an already-existing canonical record only
    /// when every stored byte-level fact is identical to the expected capture.
    pub fn require_exact_existing(&self, expected: &Self) -> Result<()> {
        require!(self == expected, CoreError::EngineAdmissionPolicyMismatch);
        Ok(())
    }
}

#[account]
#[derive(Debug)]
pub struct CoreConfigurationCandidateV0 {
    pub wire_version: u8,
    pub experimental_major: u32,
    pub bump: u8,
    pub reserved: [u8; 2],
    pub classic_spl_profile_digest: [u8; 32],
    pub supported_engine_interface_digest: [u8; 32],
    pub fee_policy_root: [u8; 32],
}

impl CoreConfigurationCandidateV0 {
    pub const DATA_LEN: usize = 104;
    pub const SPACE: usize = 8 + Self::DATA_LEN;
}

#[account]
#[derive(Debug)]
pub struct MarketDescriptorCandidateV0 {
    pub wire_version: u8,
    pub experimental_major: u32,
    pub bump: u8,
    pub reserved: [u8; 2],
    pub market_binding_digest: [u8; 32],
    pub market_descriptor_revision: u64,
    pub engine_program: Pubkey,
    pub engine_interface_id: [u8; 32],
    pub engine_instance_id: [u8; 32],
    pub engine_admission_policy_digest: [u8; 32],
    pub protected_profile_digest: [u8; 32],
    pub domain_admission_profile_digest: [u8; 32],
    pub fee_policy_digest: [u8; 32],
    pub fee_policy_revision: u64,
    pub opaque_schema_digest: [u8; 32],
}

impl MarketDescriptorCandidateV0 {
    pub const DATA_LEN: usize = 312;
    pub const SPACE: usize = 8 + Self::DATA_LEN;

    pub fn binding_row(
        &self,
        core_program: &Pubkey,
        market_descriptor_key: &Pubkey,
    ) -> Result<generic_effect_private_wire::MarketBindingRowCandidateV0> {
        require_eq!(
            self.wire_version,
            WIRE_VERSION_V0,
            CoreError::InvalidWireEncoding
        );
        require_eq!(
            self.experimental_major,
            crate::constants::EXPERIMENTAL_MAJOR,
            CoreError::InvalidWireEncoding
        );
        require!(
            self.reserved.iter().all(|byte| *byte == 0),
            CoreError::InvalidWireEncoding
        );
        require!(
            self.engine_instance_id != [0; 32]
                && self.domain_admission_profile_digest != [0; 32]
                && self.opaque_schema_digest != [0; 32],
            CoreError::InvalidWireEncoding
        );
        let row = generic_effect_private_wire::MarketBindingRowCandidateV0 {
            core_program: core_program.to_bytes(),
            core_experimental_major: self.experimental_major,
            market_descriptor_key: market_descriptor_key.to_bytes(),
            market_descriptor_revision: self.market_descriptor_revision,
            engine_program: self.engine_program.to_bytes(),
            engine_interface_id: self.engine_interface_id,
            engine_instance_id: self.engine_instance_id,
            engine_admission_policy_digest: self.engine_admission_policy_digest,
            domain_admission_profile_digest: self.domain_admission_profile_digest,
            protected_profile_digest: self.protected_profile_digest,
            fee_policy_digest: self.fee_policy_digest,
            opaque_schema_digest: self.opaque_schema_digest,
        };
        require!(
            row.digest()
                .map_err(|_| error!(CoreError::InvalidWireEncoding))?
                == self.market_binding_digest,
            CoreError::InvalidWireEncoding
        );
        Ok(row)
    }

    /// Derives the only engine-instance policy supported by the closed-domain
    /// profile from authenticated market facts. The opaque instance identity
    /// is never accepted directly as a policy digest.
    pub fn exact_engine_instance_policy_digest(
        &self,
        core_program: &Pubkey,
        market_descriptor_key: &Pubkey,
    ) -> Result<[u8; 32]> {
        self.binding_row(core_program, market_descriptor_key)?;
        compute_exact_engine_instance_policy_digest(
            &core_program.to_bytes(),
            &self.engine_program.to_bytes(),
            &self.engine_interface_id,
            &self.engine_instance_id,
        )
        .map_err(|_| error!(CoreError::InvalidWireEncoding))
    }
}

#[account]
#[derive(Debug)]
pub struct DomainDescriptorAccountCandidateV0 {
    pub wire_version: u8,
    pub rule_kind: u8,
    pub reserved: [u8; 6],
    pub controller_program: Pubkey,
    pub controller_identity: Pubkey,
    pub domain_revision: u64,
    pub namespace_or_instance: [u8; 32],
    pub custody_profile_digest: [u8; 32],
    pub asset_profile_digest: [u8; 32],
    pub accounting_profile_digest: [u8; 32],
    pub exit_class_digest: [u8; 32],
    pub admission_rule_digest: [u8; 32],
    pub protected_profile_digest: [u8; 32],
}

impl DomainDescriptorAccountCandidateV0 {
    pub const DATA_LEN: usize = 304;
    pub const SPACE: usize = 8 + Self::DATA_LEN;

    pub fn row(&self) -> DomainDescriptorRowCandidateV0 {
        DomainDescriptorRowCandidateV0 {
            wire_version: self.wire_version,
            rule_kind: self.rule_kind,
            controller_program: self.controller_program.to_bytes(),
            controller_identity: self.controller_identity.to_bytes(),
            domain_revision: self.domain_revision,
            namespace_or_instance: self.namespace_or_instance,
            custody_profile_digest: self.custody_profile_digest,
            asset_profile_digest: self.asset_profile_digest,
            accounting_profile_digest: self.accounting_profile_digest,
            exit_class_digest: self.exit_class_digest,
            admission_rule_digest: self.admission_rule_digest,
            protected_profile_digest: self.protected_profile_digest,
        }
    }

    pub fn digest(&self, core_program: &Pubkey) -> Result<[u8; 32]> {
        require!(
            self.reserved.iter().all(|byte| *byte == 0),
            CoreError::InvalidWireEncoding
        );
        require!(
            self.custody_profile_digest != [0; 32]
                && self.asset_profile_digest != [0; 32]
                && self.exit_class_digest != [0; 32],
            CoreError::InvalidSettlementDomain
        );
        self.row()
            .digest(&core_program.to_bytes())
            .map_err(|_| error!(CoreError::InvalidWireEncoding))
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DomainAccountingAssetSlotCandidateV0 {
    /// Stable descriptor-local position, never an execution-envelope asset
    /// index. Envelope indices are assigned from sorted asset-binding digests.
    pub domain_asset_slot: u8,
    pub reserved: [u8; 7],
    pub asset_identity: Pubkey,
    pub asset_program: Pubkey,
    pub settlement_profile_digest: [u8; 32],
    pub accounted_amount: u128,
}

#[account]
#[derive(Debug)]
pub struct DomainAccountingCandidateV0 {
    pub wire_version: u8,
    pub asset_count: u8,
    pub bump: u8,
    pub reserved: [u8; 5],
    pub domain_descriptor: Pubkey,
    pub domain_revision: u64,
    pub assets: [DomainAccountingAssetSlotCandidateV0; MAX_ASSETS],
}

impl DomainAccountingCandidateV0 {
    pub const DATA_LEN: usize = 1_008;
    pub const SPACE: usize = 8 + Self::DATA_LEN;

    pub fn address(core_program: &Pubkey, domain_descriptor_key: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[DOMAIN_ACCOUNTING_SEED, domain_descriptor_key.as_ref()],
            core_program,
        )
    }

    pub fn validate(&self) -> Result<()> {
        require_eq!(
            self.wire_version,
            WIRE_VERSION_V0,
            CoreError::InvalidWireEncoding
        );
        require!(
            self.reserved.iter().all(|byte| *byte == 0),
            CoreError::InvalidWireEncoding
        );
        let count = usize::from(self.asset_count);
        require!(count <= MAX_ASSETS, CoreError::ExperimentLimitExceeded);
        for (position, asset) in self.assets[..count].iter().enumerate() {
            require_eq!(
                usize::from(asset.domain_asset_slot),
                position,
                CoreError::InvalidSettlementDomain
            );
            require!(
                asset.reserved.iter().all(|byte| *byte == 0),
                CoreError::InvalidWireEncoding
            );
            require!(
                asset.asset_identity != Pubkey::default()
                    && asset.asset_program != Pubkey::default()
                    && asset.settlement_profile_digest != [0; 32],
                CoreError::InvalidSettlementDomain
            );
            if position != 0 {
                let previous = &self.assets[position - 1];
                require!(
                    (
                        previous.asset_identity,
                        previous.asset_program,
                        previous.settlement_profile_digest,
                    ) < (
                        asset.asset_identity,
                        asset.asset_program,
                        asset.settlement_profile_digest,
                    ),
                    CoreError::InvalidSettlementDomain
                );
            }
        }
        require!(
            self.assets[count..]
                .iter()
                .all(|asset| *asset == DomainAccountingAssetSlotCandidateV0::default()),
            CoreError::InvalidWireEncoding
        );
        Ok(())
    }

    /// Authenticates the stable descriptor-key PDA and the exact descriptor
    /// revision whose local accounting it carries.
    pub fn validate_authenticated(
        &self,
        core_program: &Pubkey,
        account_key: &Pubkey,
        domain_descriptor_key: &Pubkey,
        domain_revision: u64,
    ) -> Result<()> {
        self.validate()?;
        require_keys_eq!(
            self.domain_descriptor,
            *domain_descriptor_key,
            CoreError::InvalidSettlementDomain
        );
        require_eq!(
            self.domain_revision,
            domain_revision,
            CoreError::InvalidSettlementDomain
        );
        let (expected_key, expected_bump) = Self::address(core_program, domain_descriptor_key);
        require_keys_eq!(
            *account_key,
            expected_key,
            CoreError::InvalidSettlementDomain
        );
        require_eq!(self.bump, expected_bump, CoreError::InvalidSettlementDomain);
        Ok(())
    }
}

#[account]
#[derive(Debug)]
pub struct DomainAdmissionAccountCandidateV0 {
    pub wire_version: u8,
    pub reserved: [u8; 7],
    pub domain_descriptor: [u8; 32],
    pub domain_revision: u64,
    pub market: [u8; 32],
    pub engine_program: [u8; 32],
    pub engine_interface_id: [u8; 32],
    pub engine_instance_policy_digest: [u8; 32],
    pub engine_admission_policy_digest: [u8; 32],
    pub settlement_profile_digest: [u8; 32],
    pub admission_rule_digest: [u8; 32],
    pub active_from_slot: u64,
    pub expires_at_slot_or_zero: u64,
    pub revoked_at_slot_or_zero: u64,
}

impl DomainAdmissionAccountCandidateV0 {
    pub const DATA_LEN: usize = 296;
    pub const SPACE: usize = 8 + Self::DATA_LEN;

    pub fn wire_row(&self) -> Result<DomainAdmissionCandidateV0> {
        require!(
            self.reserved.iter().all(|byte| *byte == 0),
            CoreError::InvalidWireEncoding
        );
        let row = DomainAdmissionCandidateV0 {
            wire_version: self.wire_version,
            domain_descriptor: self.domain_descriptor,
            domain_revision: self.domain_revision,
            market: self.market,
            engine_program: self.engine_program,
            engine_interface_id: self.engine_interface_id,
            engine_instance_policy_digest: self.engine_instance_policy_digest,
            engine_admission_policy_digest: self.engine_admission_policy_digest,
            settlement_profile_digest: self.settlement_profile_digest,
            admission_rule_digest: self.admission_rule_digest,
            active_from_slot: self.active_from_slot,
            expires_at_slot_or_zero: self.expires_at_slot_or_zero,
            revoked_at_slot_or_zero: self.revoked_at_slot_or_zero,
        };
        row.encode()
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        Ok(row)
    }

    pub fn address(
        core_program: &Pubkey,
        row: &DomainAdmissionCandidateV0,
    ) -> Result<(Pubkey, u8)> {
        let address_digest = compute_domain_admission_address_digest(row)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        Ok(Pubkey::find_program_address(
            &[DOMAIN_ADMISSION_SEED, &address_digest],
            core_program,
        ))
    }

    pub fn validate_activity(&self, current_slot: u64) -> Result<()> {
        require!(
            self.expires_at_slot_or_zero == 0
                || self.active_from_slot < self.expires_at_slot_or_zero,
            CoreError::InvalidSettlementDomain
        );
        require!(
            self.active_from_slot <= current_slot
                && (self.expires_at_slot_or_zero == 0
                    || current_slot < self.expires_at_slot_or_zero)
                && self.revoked_at_slot_or_zero == 0,
            CoreError::InvalidSettlementDomain
        );
        Ok(())
    }

    /// Validates the complete closed-domain relation against the exact
    /// descriptor and authenticated market. The engine-instance policy is
    /// always derived from the market's typed facts, never caller supplied.
    #[allow(clippy::too_many_arguments)]
    pub fn validate_authenticated(
        &self,
        core_program: &Pubkey,
        account_key: &Pubkey,
        domain_descriptor_key: &Pubkey,
        domain_descriptor: &DomainDescriptorAccountCandidateV0,
        market_descriptor_key: &Pubkey,
        market: &MarketDescriptorCandidateV0,
        settlement_profile_digest: &[u8; 32],
        current_slot: u64,
    ) -> Result<[u8; 32]> {
        let row = self.wire_row()?;
        let domain_descriptor_digest = domain_descriptor.digest(core_program)?;
        require!(
            domain_descriptor_digest != [0; 32]
                && domain_descriptor.rule_kind == generic_effect_private_wire::DOMAIN_RULE_CLOSED,
            CoreError::InvalidSettlementDomain
        );
        let market_row = market.binding_row(core_program, market_descriptor_key)?;
        let engine_instance_policy_digest =
            market.exact_engine_instance_policy_digest(core_program, market_descriptor_key)?;
        require!(
            row.domain_descriptor == domain_descriptor_key.to_bytes()
                && row.domain_revision == domain_descriptor.domain_revision
                && row.market == market_descriptor_key.to_bytes()
                && row.engine_program == market_row.engine_program
                && row.engine_interface_id == market_row.engine_interface_id
                && row.engine_instance_policy_digest == engine_instance_policy_digest
                && row.engine_admission_policy_digest == market_row.engine_admission_policy_digest
                && row.settlement_profile_digest == *settlement_profile_digest
                && domain_descriptor.protected_profile_digest == *settlement_profile_digest
                && market.protected_profile_digest == *settlement_profile_digest
                && row.admission_rule_digest == domain_descriptor.admission_rule_digest,
            CoreError::InvalidSettlementDomain
        );
        self.validate_activity(current_slot)?;
        let (expected_key, _) = Self::address(core_program, &row)?;
        require_keys_eq!(
            *account_key,
            expected_key,
            CoreError::InvalidSettlementDomain
        );
        row.digest()
            .map_err(|_| error!(CoreError::InvalidWireEncoding))
    }
}

#[account]
#[derive(Debug)]
pub struct FeePolicyCandidateV0 {
    pub wire_version: u8,
    pub rounding_mode: u8,
    pub reserved: [u8; 6],
    pub policy_digest: [u8; 32],
    pub revision: u64,
    pub rate: u64,
    pub denominator: u64,
    /// Fixed envelope fees are disabled in the accepted spike profile.
    pub fixed_fee_disabled: u64,
}

impl FeePolicyCandidateV0 {
    pub const DATA_LEN: usize = 72;
    pub const SPACE: usize = 8 + Self::DATA_LEN;

    pub fn engine_row(&self) -> Result<generic_effect_private_wire::EngineFeePolicyRowCandidateV0> {
        require_eq!(
            self.wire_version,
            WIRE_VERSION_V0,
            CoreError::InvalidWireEncoding
        );
        require!(
            self.reserved.iter().all(|byte| *byte == 0),
            CoreError::InvalidWireEncoding
        );
        require!(
            matches!(
                self.rounding_mode,
                crate::constants::ROUND_FLOOR | crate::constants::ROUND_CEILING
            ),
            CoreError::UnsupportedFeeRounding
        );
        require!(self.denominator != 0, CoreError::ZeroFeeDenominator);
        require_eq!(self.fixed_fee_disabled, 0, CoreError::InvalidWireEncoding);
        let row = generic_effect_private_wire::EngineFeePolicyRowCandidateV0 {
            wire_version: self.wire_version,
            rounding_mode: self.rounding_mode,
            flags: 0,
            revision: self.revision,
            rate_numerator: self.rate,
            nonzero_denominator: self.denominator,
        };
        row.validate()
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        Ok(row)
    }

    pub fn validate_digest(&self, core_program: &Pubkey) -> Result<()> {
        let row = self.engine_row()?;
        let digest =
            generic_effect_private_wire::compute_fee_policy_digest(&core_program.to_bytes(), &row)
                .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        require!(digest == self.policy_digest, CoreError::InvalidWireEncoding);
        Ok(())
    }
}

#[account]
#[derive(Debug)]
pub struct FeeShardDescriptorCandidateV0 {
    pub wire_version: u8,
    pub shard_index: u8,
    pub bump: u8,
    pub reserved: [u8; 5],
    pub descriptor_digest: [u8; 32],
    /// Partitions the writable fee state by market and prevents a protocol-wide
    /// liability hot account.
    pub market_binding_digest: [u8; 32],
    pub fee_policy_digest: [u8; 32],
    pub fee_policy_revision: u64,
    pub asset_identity: Pubkey,
    pub asset_program: Pubkey,
    pub settlement_profile_digest: [u8; 32],
    pub vault: Pubkey,
    pub liability_ledger: Pubkey,
    pub recipient_policy_digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FeeShardAuthenticationExpectedV0 {
    pub shard_index: u8,
    pub asset_identity: Pubkey,
    pub asset_program: Pubkey,
    pub settlement_profile_digest: [u8; 32],
    pub vault: Pubkey,
}

impl FeeShardDescriptorCandidateV0 {
    pub const DATA_LEN: usize = 304;
    pub const SPACE: usize = 8 + Self::DATA_LEN;

    pub fn address(
        core_program: &Pubkey,
        market_binding_digest: &[u8; 32],
        shard_index: u8,
    ) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[
                FEE_SHARD_DESCRIPTOR_SEED,
                market_binding_digest,
                &[shard_index],
            ],
            core_program,
        )
    }

    pub fn wire_row(&self) -> Result<FeeShardDescriptorRowCandidateV0> {
        require!(
            self.reserved.iter().all(|byte| *byte == 0),
            CoreError::InvalidWireEncoding
        );
        let row = FeeShardDescriptorRowCandidateV0 {
            wire_version: self.wire_version,
            shard_index: self.shard_index,
            market_binding_digest: self.market_binding_digest,
            fee_policy_digest: self.fee_policy_digest,
            fee_policy_revision: self.fee_policy_revision,
            asset_identity: self.asset_identity.to_bytes(),
            asset_program: self.asset_program.to_bytes(),
            settlement_profile_digest: self.settlement_profile_digest,
            vault: self.vault.to_bytes(),
            liability_ledger: self.liability_ledger.to_bytes(),
            recipient_policy_digest: self.recipient_policy_digest,
        };
        row.encode()
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        Ok(row)
    }

    pub fn derive_descriptor_digest(&self, core_program: &Pubkey) -> Result<[u8; 32]> {
        compute_fee_shard_descriptor_digest(&core_program.to_bytes(), &self.wire_row()?)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))
    }

    /// Authenticates the complete fee-shard closure against the selected
    /// market, expected asset/vault facts, and the canonical Core liability
    /// account. No caller-provided digest can substitute for these checks.
    #[allow(clippy::too_many_arguments)]
    pub fn validate_authenticated(
        &self,
        core_program: &Pubkey,
        account_key: &Pubkey,
        market_descriptor_key: &Pubkey,
        market: &MarketDescriptorCandidateV0,
        liability_account_key: &Pubkey,
        liability: &FeeLiabilityLedgerCandidateV0,
        expected: &FeeShardAuthenticationExpectedV0,
    ) -> Result<FeeShardDescriptorRowCandidateV0> {
        market.binding_row(core_program, market_descriptor_key)?;
        let row = self.wire_row()?;
        require!(
            row.shard_index == expected.shard_index
                && row.market_binding_digest == market.market_binding_digest
                && row.fee_policy_digest == market.fee_policy_digest
                && row.fee_policy_revision == market.fee_policy_revision
                && row.asset_identity == expected.asset_identity.to_bytes()
                && row.asset_program == expected.asset_program.to_bytes()
                && row.settlement_profile_digest == expected.settlement_profile_digest
                && row.vault == expected.vault.to_bytes()
                && row.liability_ledger == liability_account_key.to_bytes(),
            CoreError::InvalidSettlementFeeShard
        );
        let expected_recipient_policy_digest = compute_exact_fee_recipient_policy_digest(
            &core_program.to_bytes(),
            &row.market_binding_digest,
            &row.vault,
            &row.asset_identity,
            &row.asset_program,
            &row.settlement_profile_digest,
        )
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        require!(
            row.recipient_policy_digest == expected_recipient_policy_digest,
            CoreError::InvalidSettlementFeeShard
        );
        require!(
            self.descriptor_digest == self.derive_descriptor_digest(core_program)?,
            CoreError::InvalidSettlementFeeShard
        );
        self.validate_partition(core_program, account_key, &market.market_binding_digest)?;
        liability.validate_partition(
            core_program,
            liability_account_key,
            self,
            account_key,
            &market.market_binding_digest,
        )?;
        Ok(row)
    }

    pub fn validate_partition(
        &self,
        core_program: &Pubkey,
        account_key: &Pubkey,
        market_binding_digest: &[u8; 32],
    ) -> Result<()> {
        require_eq!(
            self.wire_version,
            WIRE_VERSION_V0,
            CoreError::InvalidWireEncoding
        );
        require!(
            self.reserved.iter().all(|byte| *byte == 0),
            CoreError::InvalidWireEncoding
        );
        require!(
            self.market_binding_digest == *market_binding_digest,
            CoreError::InvalidSettlementFeeShard
        );
        let (expected, bump) = Self::address(core_program, market_binding_digest, self.shard_index);
        require_keys_eq!(*account_key, expected, CoreError::InvalidSettlementFeeShard);
        require_eq!(self.bump, bump, CoreError::InvalidSettlementFeeShard);
        let (expected_liability, _) = FeeLiabilityLedgerCandidateV0::address(
            core_program,
            account_key,
            market_binding_digest,
        );
        require_keys_eq!(
            self.liability_ledger,
            expected_liability,
            CoreError::InvalidSettlementFeeShard
        );
        Ok(())
    }
}

#[account]
#[derive(Debug)]
pub struct FeeLiabilityLedgerCandidateV0 {
    pub wire_version: u8,
    pub shard_index: u8,
    pub bump: u8,
    pub reserved: [u8; 5],
    pub descriptor: Pubkey,
    pub market_binding_digest: [u8; 32],
    pub asset_identity: Pubkey,
    pub settlement_profile_digest: [u8; 32],
    pub liability: u128,
}

impl FeeLiabilityLedgerCandidateV0 {
    pub const DATA_LEN: usize = 152;
    pub const SPACE: usize = 8 + Self::DATA_LEN;

    pub fn address(
        core_program: &Pubkey,
        descriptor: &Pubkey,
        market_binding_digest: &[u8; 32],
    ) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[
                FEE_LIABILITY_SEED,
                descriptor.as_ref(),
                market_binding_digest,
            ],
            core_program,
        )
    }

    pub fn validate_partition(
        &self,
        core_program: &Pubkey,
        account_key: &Pubkey,
        descriptor: &FeeShardDescriptorCandidateV0,
        descriptor_key: &Pubkey,
        market_binding_digest: &[u8; 32],
    ) -> Result<()> {
        require_eq!(
            self.wire_version,
            WIRE_VERSION_V0,
            CoreError::InvalidWireEncoding
        );
        require!(
            self.reserved.iter().all(|byte| *byte == 0),
            CoreError::InvalidWireEncoding
        );
        require_keys_eq!(
            self.descriptor,
            *descriptor_key,
            CoreError::InvalidSettlementFeeShard
        );
        require_eq!(
            self.shard_index,
            descriptor.shard_index,
            CoreError::InvalidSettlementFeeShard
        );
        require!(
            self.market_binding_digest == *market_binding_digest
                && descriptor.market_binding_digest == *market_binding_digest,
            CoreError::InvalidSettlementFeeShard
        );
        require_keys_eq!(
            self.asset_identity,
            descriptor.asset_identity,
            CoreError::InvalidSettlementFeeShard
        );
        require!(
            self.settlement_profile_digest == descriptor.settlement_profile_digest,
            CoreError::InvalidSettlementFeeShard
        );
        let (expected, bump) = Self::address(core_program, descriptor_key, market_binding_digest);
        require_keys_eq!(*account_key, expected, CoreError::InvalidSettlementFeeShard);
        require_eq!(self.bump, bump, CoreError::InvalidSettlementFeeShard);
        Ok(())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IntentIdentityCandidateV0 {
    pub experimental_major: u32,
    pub core_program: Pubkey,
    pub actor: Pubkey,
    pub authorization_nonce: u64,
    pub market_binding_digest: [u8; 32],
    pub loader_state_snapshot_digest: [u8; 32],
    pub fee_policy_digest: [u8; 32],
    pub engine_terms_commitment: [u8; 32],
    pub core_terms_root: [u8; 32],
    pub reserved_digest: [u8; 32],
    pub expires_at_slot_exclusive: u64,
    pub max_fills: u32,
    pub intent_digest: [u8; 32],
}

impl IntentIdentityCandidateV0 {
    pub const ENCODED_LEN: usize = 312;
    const MAX_FILLS_STORAGE_OFFSET: usize = 276;
    const INTENT_DIGEST_STORAGE_OFFSET: usize = 280;

    pub fn encode_storage_exact(&self) -> [u8; Self::ENCODED_LEN] {
        let mut output = [0_u8; Self::ENCODED_LEN];
        output[0..4].copy_from_slice(&self.experimental_major.to_le_bytes());
        output[4..36].copy_from_slice(self.core_program.as_ref());
        output[36..68].copy_from_slice(self.actor.as_ref());
        output[68..76].copy_from_slice(&self.authorization_nonce.to_le_bytes());
        output[76..108].copy_from_slice(&self.market_binding_digest);
        output[108..140].copy_from_slice(&self.loader_state_snapshot_digest);
        output[140..172].copy_from_slice(&self.fee_policy_digest);
        output[172..204].copy_from_slice(&self.engine_terms_commitment);
        output[204..236].copy_from_slice(&self.core_terms_root);
        output[236..268].copy_from_slice(&self.reserved_digest);
        output[268..276].copy_from_slice(&self.expires_at_slot_exclusive.to_le_bytes());
        output[276..280].copy_from_slice(&self.max_fills.to_le_bytes());
        output[280..312].copy_from_slice(&self.intent_digest);
        output
    }

    pub fn decode_storage_exact(data: &[u8]) -> Result<Self> {
        require_eq!(
            data.len(),
            Self::ENCODED_LEN,
            CoreError::InvalidWireEncoding
        );
        let array = |start: usize| -> Result<[u8; 32]> {
            data[start..start + 32]
                .try_into()
                .map_err(|_| error!(CoreError::InvalidWireEncoding))
        };
        Ok(Self {
            experimental_major: u32::from_le_bytes(
                data[0..4]
                    .try_into()
                    .map_err(|_| error!(CoreError::InvalidWireEncoding))?,
            ),
            core_program: Pubkey::new_from_array(array(4)?),
            actor: Pubkey::new_from_array(array(36)?),
            authorization_nonce: u64::from_le_bytes(
                data[68..76]
                    .try_into()
                    .map_err(|_| error!(CoreError::InvalidWireEncoding))?,
            ),
            market_binding_digest: array(76)?,
            loader_state_snapshot_digest: array(108)?,
            fee_policy_digest: array(140)?,
            engine_terms_commitment: array(172)?,
            core_terms_root: array(204)?,
            reserved_digest: array(236)?,
            expires_at_slot_exclusive: u64::from_le_bytes(
                data[268..276]
                    .try_into()
                    .map_err(|_| error!(CoreError::InvalidWireEncoding))?,
            ),
            max_fills: u32::from_le_bytes(
                data[276..280]
                    .try_into()
                    .map_err(|_| error!(CoreError::InvalidWireEncoding))?,
            ),
            intent_digest: array(280)?,
        })
    }

    pub fn compute_intent_digest(&self, core_program: &Pubkey) -> Result<[u8; 32]> {
        require_eq!(
            self.experimental_major,
            crate::constants::EXPERIMENTAL_MAJOR,
            CoreError::AuthorizationIdentityMismatch
        );
        require_keys_eq!(
            self.core_program,
            *core_program,
            CoreError::AuthorizationIdentityMismatch
        );
        let identity = self.inline_identity();
        compute_intent_digest(IntentDigestInputs {
            core_program: &core_program.to_bytes(),
            market_binding_digest: &self.market_binding_digest,
            loader_state_snapshot_digest: &self.loader_state_snapshot_digest,
            fee_policy_digest: &self.fee_policy_digest,
            identity: &identity,
            core_terms_root: &self.core_terms_root,
        })
        .map_err(|_| error!(CoreError::AuthorizationIdentityMismatch))
    }

    pub fn inline_identity(&self) -> InlineIntentIdentityRowCandidateV0 {
        InlineIntentIdentityRowCandidateV0 {
            actor: self.actor.to_bytes(),
            engine_terms_commitment: self.engine_terms_commitment,
            authorization_nonce: self.authorization_nonce,
            expires_at_slot_exclusive: self.expires_at_slot_exclusive,
        }
    }

    pub fn validate(&self, core_program: &Pubkey) -> Result<()> {
        require!(
            self.core_program != Pubkey::default()
                && self.actor != Pubkey::default()
                && self.max_fills != 0
                && self.engine_terms_commitment != [0; 32]
                && self.core_terms_root != [0; 32]
                && self.fee_policy_digest != [0; 32]
                && self.reserved_digest == [0; 32],
            CoreError::AuthorizationIdentityMismatch
        );
        require!(
            self.intent_digest == self.compute_intent_digest(core_program)?,
            CoreError::AuthorizationIdentityMismatch
        );
        Ok(())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StoredIntentCapabilityTermCandidateV0 {
    pub intent_local_term_index: u8,
    pub authority_class: u8,
    pub fee_class: u8,
    pub flags: u8,
    pub rights_bits: u16,
    pub reserved: [u8; 2],
    pub endpoint_key: Pubkey,
    pub asset_binding_digest: [u8; 32],
    pub required_domain_descriptor_digest_or_zero: [u8; 32],
    pub maximum_engine_debit: u64,
    pub maximum_total_debit: u64,
    pub minimum_credit: u64,
    pub maximum_protocol_fee: u64,
}

impl StoredIntentCapabilityTermCandidateV0 {
    pub const ENCODED_LEN: usize = 136;

    pub fn wire_row(&self) -> Result<IntentCapabilityTermRowCandidateV0> {
        require!(
            self.reserved.iter().all(|byte| *byte == 0),
            CoreError::InvalidWireEncoding
        );
        let row = IntentCapabilityTermRowCandidateV0 {
            intent_local_term_index: self.intent_local_term_index,
            authority_class: self.authority_class,
            fee_class: self.fee_class,
            flags: self.flags,
            rights_bits: self.rights_bits,
            endpoint_key: self.endpoint_key.to_bytes(),
            asset_binding_digest: self.asset_binding_digest,
            required_domain_descriptor_digest_or_zero: self
                .required_domain_descriptor_digest_or_zero,
            maximum_engine_debit: self.maximum_engine_debit,
            maximum_total_debit: self.maximum_total_debit,
            minimum_credit: self.minimum_credit,
            maximum_protocol_fee: self.maximum_protocol_fee,
        };
        row.encode()
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        Ok(row)
    }

    pub fn from_wire_row(row: IntentCapabilityTermRowCandidateV0) -> Self {
        Self {
            intent_local_term_index: row.intent_local_term_index,
            authority_class: row.authority_class,
            fee_class: row.fee_class,
            flags: row.flags,
            rights_bits: row.rights_bits,
            reserved: [0; 2],
            endpoint_key: Pubkey::new_from_array(row.endpoint_key),
            asset_binding_digest: row.asset_binding_digest,
            required_domain_descriptor_digest_or_zero: row
                .required_domain_descriptor_digest_or_zero,
            maximum_engine_debit: row.maximum_engine_debit,
            maximum_total_debit: row.maximum_total_debit,
            minimum_credit: row.minimum_credit,
            maximum_protocol_fee: row.maximum_protocol_fee,
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StoredCreditConstraintCandidateV0 {
    pub constraint_index: u8,
    pub credit_local_term_index: u8,
    pub flags: u8,
    pub reserved: [u8; 3],
    pub debit_source_bitmap: u16,
    pub debit_group_root: [u8; 32],
    pub minimum_credit_numerator: u64,
    pub nonzero_debit_denominator: u64,
    pub terminal_absolute_minimum: u64,
}

impl StoredCreditConstraintCandidateV0 {
    pub const ENCODED_LEN: usize = 64;

    pub fn wire_row(&self) -> Result<CreditConstraintRowCandidateV0> {
        require!(
            self.reserved.iter().all(|byte| *byte == 0),
            CoreError::InvalidWireEncoding
        );
        let row = CreditConstraintRowCandidateV0 {
            constraint_index: self.constraint_index,
            credit_local_term_index: self.credit_local_term_index,
            flags: self.flags,
            debit_source_bitmap: self.debit_source_bitmap,
            debit_group_root: self.debit_group_root,
            minimum_credit_numerator: self.minimum_credit_numerator,
            nonzero_debit_denominator: self.nonzero_debit_denominator,
            terminal_absolute_minimum: self.terminal_absolute_minimum,
        };
        row.encode()
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        Ok(row)
    }

    pub fn from_wire_row(row: CreditConstraintRowCandidateV0) -> Self {
        Self {
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
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AuthorizationCapabilityStateCandidateV0 {
    pub local_term_index: u8,
    /// Frozen zero byte. Credit relationships live only in constraint bitmaps.
    pub reserved_0: u8,
    pub flags: u8,
    pub reserved: [u8; 5],
    pub initial_maximum_engine_debit: u64,
    pub initial_minimum_credit: u64,
    pub initial_maximum_total_debit: u64,
    pub remaining_total_debit: u64,
    pub cumulative_engine_debit: u128,
    pub cumulative_fee_debit: u128,
    pub cumulative_credit: u128,
}

impl AuthorizationCapabilityStateCandidateV0 {
    pub const ENCODED_LEN: usize = 88;

    pub fn wire_row(&self) -> Result<AuthorizationCapabilityStateRowCandidateV0> {
        require!(
            self.reserved.iter().all(|byte| *byte == 0),
            CoreError::InvalidWireEncoding
        );
        let row = AuthorizationCapabilityStateRowCandidateV0 {
            local_term_index: self.local_term_index,
            reserved_0: self.reserved_0,
            flags: self.flags,
            initial_maximum_engine_debit: self.initial_maximum_engine_debit,
            initial_minimum_credit: self.initial_minimum_credit,
            initial_maximum_total_debit: self.initial_maximum_total_debit,
            remaining_total_debit: self.remaining_total_debit,
            cumulative_engine_debit: self.cumulative_engine_debit,
            cumulative_fee_debit: self.cumulative_fee_debit,
            cumulative_credit: self.cumulative_credit,
        };
        row.validate()
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        Ok(row)
    }

    pub fn from_wire_row(row: AuthorizationCapabilityStateRowCandidateV0) -> Self {
        Self {
            local_term_index: row.local_term_index,
            reserved_0: row.reserved_0,
            flags: row.flags,
            reserved: [0; 5],
            initial_maximum_engine_debit: row.initial_maximum_engine_debit,
            initial_minimum_credit: row.initial_minimum_credit,
            initial_maximum_total_debit: row.initial_maximum_total_debit,
            remaining_total_debit: row.remaining_total_debit,
            cumulative_engine_debit: row.cumulative_engine_debit,
            cumulative_fee_debit: row.cumulative_fee_debit,
            cumulative_credit: row.cumulative_credit,
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AuthorizationFeeStateCandidateV0 {
    pub rounding_group_digest: [u8; 32],
    pub funding_local_term_index: u8,
    pub fee_class: u8,
    pub flags: u8,
    pub reserved: [u8; 5],
    pub cumulative_basis: u128,
    pub cumulative_assessed_fee: u128,
    pub maximum_fee: u64,
}

impl AuthorizationFeeStateCandidateV0 {
    pub const ENCODED_LEN: usize = 80;

    pub fn wire_row(&self) -> Result<AuthorizationFeeStateRowCandidateV0> {
        require!(
            self.reserved.iter().all(|byte| *byte == 0),
            CoreError::InvalidWireEncoding
        );
        let row = AuthorizationFeeStateRowCandidateV0 {
            rounding_group_digest: self.rounding_group_digest,
            funding_local_term_index: self.funding_local_term_index,
            fee_class: self.fee_class,
            flags: self.flags,
            cumulative_basis: self.cumulative_basis,
            cumulative_assessed_fee: self.cumulative_assessed_fee,
            maximum_fee: self.maximum_fee,
        };
        row.validate()
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        Ok(row)
    }

    pub fn from_wire_row(row: AuthorizationFeeStateRowCandidateV0) -> Self {
        Self {
            rounding_group_digest: row.rounding_group_digest,
            funding_local_term_index: row.funding_local_term_index,
            fee_class: row.fee_class,
            flags: row.flags,
            reserved: [0; 5],
            cumulative_basis: row.cumulative_basis,
            cumulative_assessed_fee: row.cumulative_assessed_fee,
            maximum_fee: row.maximum_fee,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoredAuthorizationLifecycle {
    Draft,
    Active,
    Executing,
    Consumed,
    Cancelled,
}

impl StoredAuthorizationLifecycle {
    pub const DRAFT: u8 = 0;
    pub const ACTIVE: u8 = 1;
    pub const EXECUTING: u8 = 2;
    pub const CONSUMED: u8 = 3;
    pub const CANCELLED: u8 = 4;

    pub fn decode(value: u8) -> Result<Self> {
        match value {
            Self::DRAFT => Ok(Self::Draft),
            Self::ACTIVE => Ok(Self::Active),
            Self::EXECUTING => Ok(Self::Executing),
            Self::CONSUMED => Ok(Self::Consumed),
            Self::CANCELLED => Ok(Self::Cancelled),
            _ => err!(CoreError::AuthorizationUnavailable),
        }
    }

    pub fn encode(self) -> u8 {
        match self {
            Self::Draft => Self::DRAFT,
            Self::Active => Self::ACTIVE,
            Self::Executing => Self::EXECUTING,
            Self::Consumed => Self::CONSUMED,
            Self::Cancelled => Self::CANCELLED,
        }
    }
}

/// Host-side full-value oracle for the frozen storage layout. Solana builds do
/// not derive an account deserializer for this 4,776-byte value; executable
/// paths use `StoredAuthorizationCompactCandidateV0` and fixed-range codecs.
#[cfg_attr(not(target_os = "solana"), account)]
#[derive(Debug)]
pub struct StoredAuthorizationCandidateV0 {
    pub wire_version: u8,
    pub lifecycle: u8,
    pub bump: u8,
    pub term_count: u8,
    pub constraint_count: u8,
    pub fee_state_count: u8,
    pub flags: u8,
    pub reserved: u8,
    pub term_bitmap: u16,
    pub constraint_bitmap: u16,
    pub fill_sequence: u32,
    pub identity: IntentIdentityCandidateV0,
    pub pending_execution_digest: [u8; 32],
    pub immutable_terms: [StoredIntentCapabilityTermCandidateV0; MAX_STORED_INTENT_TERMS],
    pub credit_constraints: [StoredCreditConstraintCandidateV0; MAX_STORED_CREDIT_CONSTRAINTS],
    pub capabilities: [AuthorizationCapabilityStateCandidateV0; MAX_SETTLEMENT_CAPABILITIES],
    pub fee_states: [AuthorizationFeeStateCandidateV0; MAX_STORED_FEE_STATES],
}

impl StoredAuthorizationCandidateV0 {
    pub const HEADER_LEN: usize = generic_effect_private_wire::STORED_AUTHORIZATION_HEADER_LEN;
    pub const DATA_LEN: usize = Self::HEADER_LEN
        + IntentIdentityCandidateV0::ENCODED_LEN
        + 32
        + MAX_STORED_INTENT_TERMS * StoredIntentCapabilityTermCandidateV0::ENCODED_LEN
        + MAX_STORED_CREDIT_CONSTRAINTS * StoredCreditConstraintCandidateV0::ENCODED_LEN
        + MAX_SETTLEMENT_CAPABILITIES * AuthorizationCapabilityStateCandidateV0::ENCODED_LEN
        + MAX_STORED_FEE_STATES * AuthorizationFeeStateCandidateV0::ENCODED_LEN;
    pub const SPACE: usize = 8 + Self::DATA_LEN;

    /// Creates the immutable identity tombstone before any variable-sized rows
    /// are uploaded. A Draft can only progress to Active; its PDA can never be
    /// closed and recreated as a different authorization.
    #[cfg(not(target_os = "solana"))]
    pub fn initialize_draft(
        core_program: &Pubkey,
        actor: &Pubkey,
        identity: IntentIdentityCandidateV0,
        term_count: u8,
        constraint_count: u8,
    ) -> Result<Self> {
        require_keys_eq!(
            identity.actor,
            *actor,
            CoreError::AuthorizationIdentityMismatch
        );
        identity.validate(core_program)?;
        require!(
            term_count != 0
                && usize::from(term_count) <= MAX_STORED_INTENT_TERMS
                && usize::from(constraint_count) <= MAX_STORED_CREDIT_CONSTRAINTS,
            CoreError::ExperimentLimitExceeded
        );
        let (account_key, bump) = Self::address(core_program, &identity.intent_digest);
        let state = Self {
            wire_version: WIRE_VERSION_V0,
            lifecycle: StoredAuthorizationLifecycle::DRAFT,
            bump,
            term_count,
            constraint_count,
            fee_state_count: 0,
            flags: 0,
            reserved: 0,
            term_bitmap: 0,
            constraint_bitmap: 0,
            fill_sequence: 0,
            identity,
            pending_execution_digest: [0; 32],
            immutable_terms: [StoredIntentCapabilityTermCandidateV0::default();
                MAX_STORED_INTENT_TERMS],
            credit_constraints: [StoredCreditConstraintCandidateV0::default();
                MAX_STORED_CREDIT_CONSTRAINTS],
            capabilities: [AuthorizationCapabilityStateCandidateV0::default();
                MAX_SETTLEMENT_CAPABILITIES],
            fee_states: [AuthorizationFeeStateCandidateV0::default(); MAX_STORED_FEE_STATES],
        };
        require!(
            state.validate_account(core_program, &account_key)?
                == StoredAuthorizationLifecycle::Draft,
            CoreError::AuthorizationUnavailable
        );
        Ok(state)
    }

    /// Adds a canonical, non-overlapping run of immutable local terms.
    #[cfg(not(target_os = "solana"))]
    pub fn write_term_chunk(
        &mut self,
        core_program: &Pubkey,
        account_key: &Pubkey,
        start_index: u8,
        rows: &[StoredIntentCapabilityTermCandidateV0],
    ) -> Result<()> {
        require!(
            self.validate_account(core_program, account_key)?
                == StoredAuthorizationLifecycle::Draft,
            CoreError::AuthorizationUnavailable
        );
        require!(!rows.is_empty(), CoreError::InvalidWireEncoding);
        let start = usize::from(start_index);
        let end = start
            .checked_add(rows.len())
            .ok_or(CoreError::ArithmeticOverflow)?;
        require!(
            end <= usize::from(self.term_count),
            CoreError::ExperimentLimitExceeded
        );
        for (offset, row) in rows.iter().enumerate() {
            let index = start + offset;
            require_eq!(
                usize::from(row.intent_local_term_index),
                index,
                CoreError::InvalidWireEncoding
            );
            require_eq!(
                self.term_bitmap & (1_u16 << index),
                0,
                CoreError::InvalidWireEncoding
            );
            row.wire_row()?;
        }
        for (offset, row) in rows.iter().copied().enumerate() {
            let index = start + offset;
            self.immutable_terms[index] = row;
            self.term_bitmap |= 1_u16 << index;
        }
        self.validate_account(core_program, account_key)?;
        Ok(())
    }

    /// Adds a canonical, non-overlapping run of immutable credit constraints.
    #[cfg(not(target_os = "solana"))]
    pub fn write_constraint_chunk(
        &mut self,
        core_program: &Pubkey,
        account_key: &Pubkey,
        start_index: u8,
        rows: &[StoredCreditConstraintCandidateV0],
    ) -> Result<()> {
        require!(
            self.validate_account(core_program, account_key)?
                == StoredAuthorizationLifecycle::Draft,
            CoreError::AuthorizationUnavailable
        );
        require!(!rows.is_empty(), CoreError::InvalidWireEncoding);
        let start = usize::from(start_index);
        let end = start
            .checked_add(rows.len())
            .ok_or(CoreError::ArithmeticOverflow)?;
        require!(
            end <= usize::from(self.constraint_count),
            CoreError::ExperimentLimitExceeded
        );
        for (offset, row) in rows.iter().enumerate() {
            let index = start + offset;
            require_eq!(
                usize::from(row.constraint_index),
                index,
                CoreError::InvalidWireEncoding
            );
            require_eq!(
                self.constraint_bitmap & (1_u16 << index),
                0,
                CoreError::InvalidWireEncoding
            );
            row.wire_row()?;
        }
        for (offset, row) in rows.iter().copied().enumerate() {
            let index = start + offset;
            self.credit_constraints[index] = row;
            self.constraint_bitmap |= 1_u16 << index;
        }
        self.validate_account(core_program, account_key)?;
        Ok(())
    }

    /// Freezes the complete Draft into an executable authorization. Mutable
    /// capability state is derived, never caller supplied; fee rows start empty
    /// and are inserted lazily from authenticated fee assessments.
    #[cfg(not(target_os = "solana"))]
    pub fn activate(&mut self, core_program: &Pubkey, account_key: &Pubkey) -> Result<()> {
        require!(
            self.validate_account(core_program, account_key)?
                == StoredAuthorizationLifecycle::Draft,
            CoreError::AuthorizationUnavailable
        );
        let term_count = usize::from(self.term_count);
        let constraint_count = usize::from(self.constraint_count);
        require_eq!(
            self.term_bitmap,
            exact_prefix_bitmap(term_count)?,
            CoreError::InvalidWireEncoding
        );
        require_eq!(
            self.constraint_bitmap,
            exact_prefix_bitmap(constraint_count)?,
            CoreError::InvalidWireEncoding
        );
        let term_rows = self.immutable_terms[..term_count]
            .iter()
            .map(StoredIntentCapabilityTermCandidateV0::wire_row)
            .collect::<Result<Vec<_>>>()?;
        let constraint_rows = self.credit_constraints[..constraint_count]
            .iter()
            .map(StoredCreditConstraintCandidateV0::wire_row)
            .collect::<Result<Vec<_>>>()?;
        Self::validate_core_terms(&self.identity, &term_rows, &constraint_rows)?;

        let mut capabilities =
            [AuthorizationCapabilityStateCandidateV0::default(); MAX_SETTLEMENT_CAPABILITIES];
        for (index, term) in term_rows.iter().enumerate() {
            capabilities[index] = AuthorizationCapabilityStateCandidateV0 {
                local_term_index: term.intent_local_term_index,
                reserved_0: 0,
                flags: term.flags,
                reserved: [0; 5],
                initial_maximum_engine_debit: term.maximum_engine_debit,
                initial_minimum_credit: term.minimum_credit,
                initial_maximum_total_debit: term.maximum_total_debit,
                remaining_total_debit: term.maximum_total_debit,
                cumulative_engine_debit: 0,
                cumulative_fee_debit: 0,
                cumulative_credit: 0,
            };
        }
        let capability_rows = capabilities[..term_count]
            .iter()
            .map(AuthorizationCapabilityStateCandidateV0::wire_row)
            .collect::<Result<Vec<_>>>()?;
        Self::validate_immutable_to_mutable_mapping_rows(
            &term_rows,
            &constraint_rows,
            &capability_rows,
            &[],
        )?;

        self.capabilities = capabilities;
        self.fee_states = [AuthorizationFeeStateCandidateV0::default(); MAX_STORED_FEE_STATES];
        self.fee_state_count = 0;
        self.lifecycle = StoredAuthorizationLifecycle::ACTIVE;
        self.validate_account(core_program, account_key)?;
        Ok(())
    }

    pub fn address(core_program: &Pubkey, intent_digest: &[u8; 32]) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[STORED_AUTHORIZATION_SEED, intent_digest], core_program)
    }

    #[cfg(not(target_os = "solana"))]
    pub fn header_row(&self) -> Result<StoredAuthorizationHeaderCandidateV0> {
        let header = StoredAuthorizationHeaderCandidateV0 {
            wire_version: self.wire_version,
            lifecycle: self.lifecycle,
            bump: self.bump,
            term_count: self.term_count,
            constraint_count: self.constraint_count,
            fee_state_count: self.fee_state_count,
            flags: self.flags,
            reserved: self.reserved,
            term_written_bitmap: self.term_bitmap,
            constraint_written_bitmap: self.constraint_bitmap,
            fill_sequence: self.fill_sequence,
        };
        header
            .encode()
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        Ok(header)
    }

    #[cfg(not(target_os = "solana"))]
    pub fn validate_account(
        &self,
        core_program: &Pubkey,
        account_key: &Pubkey,
    ) -> Result<StoredAuthorizationLifecycle> {
        let lifecycle = self.validate_header(core_program)?;
        let (expected, bump) = Self::address(core_program, &self.identity.intent_digest);
        require_keys_eq!(
            *account_key,
            expected,
            CoreError::AuthorizationIdentityMismatch
        );
        require_eq!(self.bump, bump, CoreError::AuthorizationIdentityMismatch);
        Ok(lifecycle)
    }

    #[cfg(not(target_os = "solana"))]
    pub fn validate_header(&self, core_program: &Pubkey) -> Result<StoredAuthorizationLifecycle> {
        self.header_row()?;
        require_eq!(
            self.wire_version,
            WIRE_VERSION_V0,
            CoreError::InvalidWireEncoding
        );
        require!(
            self.flags == 0 && self.reserved == 0,
            CoreError::InvalidWireEncoding
        );
        require!(
            self.term_count != 0
                && usize::from(self.term_count) <= MAX_STORED_INTENT_TERMS
                && usize::from(self.constraint_count) <= MAX_STORED_CREDIT_CONSTRAINTS
                && usize::from(self.fee_state_count) <= MAX_STORED_FEE_STATES,
            CoreError::ExperimentLimitExceeded
        );
        let term_count = usize::from(self.term_count);
        let constraint_count = usize::from(self.constraint_count);
        let fee_state_count = usize::from(self.fee_state_count);
        let expected_term_bitmap = exact_prefix_bitmap(term_count)?;
        let expected_constraint_bitmap = exact_prefix_bitmap(constraint_count)?;
        require!(
            self.term_bitmap & !expected_term_bitmap == 0
                && self.constraint_bitmap & !expected_constraint_bitmap == 0,
            CoreError::InvalidWireEncoding
        );
        require!(
            self.immutable_terms[term_count..]
                .iter()
                .all(|entry| *entry == StoredIntentCapabilityTermCandidateV0::default())
                && self.credit_constraints[constraint_count..]
                    .iter()
                    .all(|entry| *entry == StoredCreditConstraintCandidateV0::default()),
            CoreError::InvalidWireEncoding
        );
        self.identity.validate(core_program)?;
        require!(
            self.fill_sequence <= self.identity.max_fills,
            CoreError::AuthorizationFillSequenceMismatch
        );

        let lifecycle = StoredAuthorizationLifecycle::decode(self.lifecycle)?;
        let mutable_rows_are_default = self
            .capabilities
            .iter()
            .all(|entry| *entry == AuthorizationCapabilityStateCandidateV0::default())
            && self
                .fee_states
                .iter()
                .all(|entry| *entry == AuthorizationFeeStateCandidateV0::default());
        if lifecycle == StoredAuthorizationLifecycle::Draft
            || (lifecycle == StoredAuthorizationLifecycle::Cancelled && mutable_rows_are_default)
        {
            require!(
                self.fill_sequence == 0
                    && self.fee_state_count == 0
                    && self.pending_execution_digest == [0; 32]
                    && mutable_rows_are_default,
                CoreError::InvalidWireEncoding
            );
            for index in 0..term_count {
                if self.term_bitmap & (1_u16 << index) == 0 {
                    require!(
                        self.immutable_terms[index]
                            == StoredIntentCapabilityTermCandidateV0::default(),
                        CoreError::InvalidWireEncoding
                    );
                } else {
                    require_eq!(
                        usize::from(self.immutable_terms[index].intent_local_term_index),
                        index,
                        CoreError::InvalidWireEncoding
                    );
                    self.immutable_terms[index].wire_row()?;
                }
            }
            for index in 0..constraint_count {
                if self.constraint_bitmap & (1_u16 << index) == 0 {
                    require!(
                        self.credit_constraints[index]
                            == StoredCreditConstraintCandidateV0::default(),
                        CoreError::InvalidWireEncoding
                    );
                } else {
                    require_eq!(
                        usize::from(self.credit_constraints[index].constraint_index),
                        index,
                        CoreError::InvalidWireEncoding
                    );
                    self.credit_constraints[index].wire_row()?;
                }
            }
            return Ok(lifecycle);
        }

        require_eq!(
            self.term_bitmap,
            expected_term_bitmap,
            CoreError::InvalidWireEncoding
        );
        require_eq!(
            self.constraint_bitmap,
            expected_constraint_bitmap,
            CoreError::InvalidWireEncoding
        );
        require!(
            self.capabilities[..term_count]
                .iter()
                .all(|entry| entry.reserved.iter().all(|byte| *byte == 0))
                && self.capabilities[term_count..]
                    .iter()
                    .all(|entry| *entry == AuthorizationCapabilityStateCandidateV0::default()),
            CoreError::InvalidWireEncoding
        );
        require!(
            self.fee_states[..fee_state_count]
                .iter()
                .all(|entry| entry.reserved.iter().all(|byte| *byte == 0))
                && self.fee_states[fee_state_count..]
                    .iter()
                    .all(|entry| *entry == AuthorizationFeeStateCandidateV0::default()),
            CoreError::InvalidWireEncoding
        );
        let term_rows = self.immutable_terms[..term_count]
            .iter()
            .map(StoredIntentCapabilityTermCandidateV0::wire_row)
            .collect::<Result<Vec<_>>>()?;
        let constraint_rows = self.credit_constraints[..constraint_count]
            .iter()
            .map(StoredCreditConstraintCandidateV0::wire_row)
            .collect::<Result<Vec<_>>>()?;
        let capability_rows = self.capabilities[..term_count]
            .iter()
            .map(AuthorizationCapabilityStateCandidateV0::wire_row)
            .collect::<Result<Vec<_>>>()?;
        let fee_rows = self.fee_states[..fee_state_count]
            .iter()
            .map(AuthorizationFeeStateCandidateV0::wire_row)
            .collect::<Result<Vec<_>>>()?;
        compute_authorization_capability_state_root(&capability_rows)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        compute_authorization_fee_state_root(&fee_rows)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        Self::validate_core_terms(&self.identity, &term_rows, &constraint_rows)?;
        Self::validate_immutable_to_mutable_mapping_rows(
            &term_rows,
            &constraint_rows,
            &capability_rows,
            &fee_rows,
        )?;
        match lifecycle {
            StoredAuthorizationLifecycle::Executing => require!(
                self.pending_execution_digest != [0; 32]
                    && self.fill_sequence < self.identity.max_fills,
                CoreError::AuthorizationSnapshotMismatch
            ),
            StoredAuthorizationLifecycle::Active => require!(
                self.pending_execution_digest == [0; 32]
                    && self.fill_sequence < self.identity.max_fills,
                CoreError::AuthorizationSnapshotMismatch
            ),
            StoredAuthorizationLifecycle::Consumed => require!(
                self.pending_execution_digest == [0; 32] && self.fill_sequence != 0,
                CoreError::AuthorizationSnapshotMismatch
            ),
            StoredAuthorizationLifecycle::Cancelled => require!(
                self.pending_execution_digest == [0; 32],
                CoreError::AuthorizationSnapshotMismatch
            ),
            StoredAuthorizationLifecycle::Draft => unreachable!("draft returned above"),
        }
        Ok(lifecycle)
    }

    fn validate_core_terms(
        identity: &IntentIdentityCandidateV0,
        terms: &[IntentCapabilityTermRowCandidateV0],
        constraints: &[CreditConstraintRowCandidateV0],
    ) -> Result<()> {
        let capability_terms_root = compute_intent_capability_terms_root(terms)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        let credit_constraints_root = compute_intent_credit_constraints_root(constraints)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        let expected_core_terms_root =
            compute_intent_core_terms_root(IntentCoreTermsDigestInputs {
                maximum_successful_fills: identity.max_fills,
                capability_terms_root: &capability_terms_root,
                credit_constraints_root: &credit_constraints_root,
            })
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        require!(
            identity.core_terms_root == expected_core_terms_root,
            CoreError::AuthorizationIdentityMismatch
        );
        Ok(())
    }

    fn validate_immutable_to_mutable_mapping_rows(
        terms: &[IntentCapabilityTermRowCandidateV0],
        constraints: &[CreditConstraintRowCandidateV0],
        capabilities: &[AuthorizationCapabilityStateRowCandidateV0],
        fees: &[AuthorizationFeeStateRowCandidateV0],
    ) -> Result<()> {
        for (term, state) in terms.iter().zip(capabilities) {
            require_eq!(
                state.local_term_index,
                term.intent_local_term_index,
                CoreError::AuthorizationIdentityMismatch
            );
            require!(
                state.reserved_0 == 0
                    && state.flags == term.flags
                    && state.initial_maximum_engine_debit == term.maximum_engine_debit
                    && state.initial_minimum_credit == term.minimum_credit
                    && state.initial_maximum_total_debit == term.maximum_total_debit,
                CoreError::AuthorizationIdentityMismatch
            );
            let is_debit = term.rights_bits & RIGHT_DEBIT != 0;
            if is_debit {
                require!(
                    term.authority_class == AUTHORITY_INTENT_FUNDED_DEBIT
                        && term.rights_bits == RIGHT_DEBIT
                        && term.maximum_engine_debit != 0
                        && term.maximum_total_debit >= term.maximum_engine_debit
                        && term.minimum_credit == 0
                        && state.cumulative_engine_debit
                            <= u128::from(state.initial_maximum_engine_debit)
                        && state.cumulative_credit == 0,
                    CoreError::AuthorizationIdentityMismatch
                );
                let fee_funding = term.flags & INTENT_CAPABILITY_TERM_FLAG_FEE_FUNDING != 0;
                require!(
                    (fee_funding
                        && term.fee_class == FEE_CLASS_GROSS_DEBIT_RATE
                        && term.maximum_protocol_fee != 0)
                        || (!fee_funding
                            && term.fee_class == FEE_CLASS_GROSS_DEBIT_RATE
                            && term.maximum_protocol_fee == 0
                            && term.maximum_total_debit == term.maximum_engine_debit),
                    CoreError::AuthorizationIdentityMismatch
                );
            } else {
                require!(
                    term.authority_class == AUTHORITY_EXACT_EXTERNAL_CREDIT
                        && term.rights_bits == (RIGHT_CREDIT | RIGHT_EXACT_EXTERNAL_RECIPIENT)
                        && term.fee_class == FEE_CLASS_NONE
                        && term.flags == 0
                        && term.maximum_engine_debit == 0
                        && term.maximum_total_debit == 0
                        && term.maximum_protocol_fee == 0
                        && state.cumulative_engine_debit == 0
                        && state.cumulative_fee_debit == 0
                        && state.remaining_total_debit == 0,
                    CoreError::AuthorizationIdentityMismatch
                );
            }
        }
        for constraint in constraints {
            let credit_index = usize::from(constraint.credit_local_term_index);
            let credit_term = terms
                .get(credit_index)
                .ok_or(CoreError::AuthorizationIdentityMismatch)?;
            require!(
                credit_term.authority_class == AUTHORITY_EXACT_EXTERNAL_CREDIT
                    && credit_term.rights_bits == (RIGHT_CREDIT | RIGHT_EXACT_EXTERNAL_RECIPIENT),
                CoreError::AuthorizationIdentityMismatch
            );
            let allowed_mask = exact_prefix_bitmap(terms.len())?;
            require!(
                constraint.debit_source_bitmap != 0
                    && constraint.debit_source_bitmap & !allowed_mask == 0,
                CoreError::AuthorizationIdentityMismatch
            );
            let source_indices = (0..terms.len())
                .filter(|index| constraint.debit_source_bitmap & (1_u16 << index) != 0)
                .map(|index| {
                    require!(
                        terms[index].authority_class == AUTHORITY_INTENT_FUNDED_DEBIT
                            && terms[index].rights_bits == RIGHT_DEBIT,
                        CoreError::AuthorizationIdentityMismatch
                    );
                    u8::try_from(index).map_err(|_| error!(CoreError::ExperimentLimitExceeded))
                })
                .collect::<Result<Vec<_>>>()?;
            let group_root =
                generic_effect_private_wire::compute_intent_debit_group_root(&source_indices)
                    .map_err(|_| error!(CoreError::AuthorizationIdentityMismatch))?;
            require!(
                group_root == constraint.debit_group_root,
                CoreError::AuthorizationIdentityMismatch
            );
        }
        for term in terms
            .iter()
            .filter(|term| term.authority_class == AUTHORITY_INTENT_FUNDED_DEBIT)
        {
            let source_bit = 1_u16
                .checked_shl(u32::from(term.intent_local_term_index))
                .ok_or(CoreError::ExperimentLimitExceeded)?;
            let has_effective_constraint = constraints.iter().any(|constraint| {
                constraint.minimum_credit_numerator != 0
                    && constraint.nonzero_debit_denominator != 0
                    && constraint.debit_source_bitmap & source_bit != 0
            });
            let allows_unconstrained =
                term.flags & INTENT_CAPABILITY_TERM_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT != 0;
            require!(
                has_effective_constraint != allows_unconstrained,
                CoreError::AuthorizationIdentityMismatch
            );
        }
        for fee in fees {
            let term = terms
                .get(usize::from(fee.funding_local_term_index))
                .ok_or(CoreError::AuthorizationIdentityMismatch)?;
            require!(
                term.flags & INTENT_CAPABILITY_TERM_FLAG_FEE_FUNDING != 0
                    && term.fee_class == fee.fee_class
                    && term.maximum_protocol_fee == fee.maximum_fee,
                CoreError::AuthorizationIdentityMismatch
            );
        }
        for capability in capabilities {
            let expected_fee_debit = fees
                .iter()
                .filter(|fee| fee.funding_local_term_index == capability.local_term_index)
                .try_fold(0_u128, |sum, fee| {
                    sum.checked_add(fee.cumulative_assessed_fee)
                        .ok_or(CoreError::ArithmeticOverflow)
                })?;
            require_eq!(
                capability.cumulative_fee_debit,
                expected_fee_debit,
                CoreError::AuthorizationIdentityMismatch
            );
        }
        Ok(())
    }

    #[cfg(not(target_os = "solana"))]
    pub fn capability_state_root(&self) -> Result<[u8; 32]> {
        let rows = self.capabilities[..usize::from(self.term_count)]
            .iter()
            .map(AuthorizationCapabilityStateCandidateV0::wire_row)
            .collect::<Result<Vec<_>>>()?;
        compute_authorization_capability_state_root(&rows)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))
    }

    #[cfg(not(target_os = "solana"))]
    pub fn fee_state_root(&self) -> Result<[u8; 32]> {
        let rows = self.fee_states[..usize::from(self.fee_state_count)]
            .iter()
            .map(AuthorizationFeeStateCandidateV0::wire_row)
            .collect::<Result<Vec<_>>>()?;
        compute_authorization_fee_state_root(&rows)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))
    }

    #[cfg(not(target_os = "solana"))]
    pub fn reserve_execution(
        &mut self,
        core_program: &Pubkey,
        account_key: &Pubkey,
        current_slot: u64,
        expected_fill_sequence: u32,
        execution_digest: [u8; 32],
    ) -> Result<()> {
        require!(
            execution_digest != [0; 32],
            CoreError::AuthorizationSnapshotMismatch
        );
        require!(
            self.validate_account(core_program, account_key)?
                == StoredAuthorizationLifecycle::Active,
            CoreError::AuthorizationUnavailable
        );
        require!(
            current_slot < self.identity.expires_at_slot_exclusive,
            CoreError::AuthorizationExpired
        );
        require_eq!(
            self.fill_sequence,
            expected_fill_sequence,
            CoreError::AuthorizationFillSequenceMismatch
        );
        require!(
            self.fill_sequence < self.identity.max_fills,
            CoreError::AuthorizationUnavailable
        );
        self.lifecycle = StoredAuthorizationLifecycle::Executing.encode();
        self.pending_execution_digest = execution_digest;
        self.validate_account(core_program, account_key)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    #[cfg(not(target_os = "solana"))]
    pub fn commit_execution(
        &mut self,
        core_program: &Pubkey,
        account_key: &Pubkey,
        execution_digest: &[u8; 32],
        next_capabilities: [AuthorizationCapabilityStateCandidateV0; MAX_SETTLEMENT_CAPABILITIES],
        next_fee_states: [AuthorizationFeeStateCandidateV0; MAX_STORED_FEE_STATES],
        next_fee_state_count: u8,
        terminal: bool,
    ) -> Result<()> {
        require!(
            self.validate_account(core_program, account_key)?
                == StoredAuthorizationLifecycle::Executing,
            CoreError::AuthorizationUnavailable
        );
        require!(
            self.pending_execution_digest == *execution_digest,
            CoreError::AuthorizationSnapshotMismatch
        );
        self.fill_sequence = self
            .fill_sequence
            .checked_add(1)
            .ok_or(CoreError::ArithmeticOverflow)?;
        require!(
            usize::from(next_fee_state_count) <= MAX_STORED_FEE_STATES,
            CoreError::ExperimentLimitExceeded
        );
        self.capabilities = next_capabilities;
        self.fee_states = next_fee_states;
        self.fee_state_count = next_fee_state_count;
        self.pending_execution_digest = [0; 32];
        let reached_max_fills = self.fill_sequence >= self.identity.max_fills;
        self.lifecycle = if terminal || reached_max_fills {
            StoredAuthorizationLifecycle::Consumed.encode()
        } else {
            StoredAuthorizationLifecycle::Active.encode()
        };
        self.validate_account(core_program, account_key)?;
        Ok(())
    }

    #[cfg(not(target_os = "solana"))]
    pub fn cancel(&mut self, core_program: &Pubkey, account_key: &Pubkey) -> Result<()> {
        let lifecycle = self.validate_account(core_program, account_key)?;
        require!(
            lifecycle == StoredAuthorizationLifecycle::Draft
                || lifecycle == StoredAuthorizationLifecycle::Active,
            CoreError::AuthorizationUnavailable
        );
        self.lifecycle = StoredAuthorizationLifecycle::Cancelled.encode();
        self.validate_account(core_program, account_key)?;
        Ok(())
    }

    /// Atomically prepares a same-actor replacement transition. Cross-actor
    /// novation is not part of this state machine and cannot be inferred from
    /// two otherwise valid tombstones.
    #[cfg(not(target_os = "solana"))]
    pub fn replace_same_actor(
        core_program: &Pubkey,
        actor: &Pubkey,
        old_account_key: &Pubkey,
        old: &mut Self,
        new_account_key: &Pubkey,
        new: &mut Self,
    ) -> Result<()> {
        require!(
            old.validate_account(core_program, old_account_key)?
                == StoredAuthorizationLifecycle::Active
                && new.validate_account(core_program, new_account_key)?
                    == StoredAuthorizationLifecycle::Draft,
            CoreError::AuthorizationUnavailable
        );
        require_keys_eq!(
            old.identity.actor,
            *actor,
            CoreError::AuthorizationIdentityMismatch
        );
        require_keys_eq!(
            new.identity.actor,
            *actor,
            CoreError::AuthorizationIdentityMismatch
        );
        require!(
            old.identity.actor == new.identity.actor
                && old.identity.intent_digest != new.identity.intent_digest
                && old_account_key != new_account_key,
            CoreError::AuthorizationIdentityMismatch
        );
        new.activate(core_program, new_account_key)?;
        old.cancel(core_program, old_account_key)?;
        Ok(())
    }
}

/// Heap-backed execution view of the exact 4,776-byte stored payload. The
/// account itself is never deserialized into a 4,776-byte Rust value on SBF;
/// only the small identity and typed rows are decoded from fixed byte ranges.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredAuthorizationCompactCandidateV0 {
    pub header: StoredAuthorizationHeaderCandidateV0,
    pub identity: IntentIdentityCandidateV0,
    pub pending_execution_digest: [u8; 32],
    pub immutable_terms: Vec<StoredIntentCapabilityTermCandidateV0>,
    pub credit_constraints: Vec<StoredCreditConstraintCandidateV0>,
    pub capabilities: Vec<AuthorizationCapabilityStateCandidateV0>,
    pub fee_states: Vec<AuthorizationFeeStateCandidateV0>,
}

impl StoredAuthorizationCompactCandidateV0 {
    pub fn lifecycle(&self) -> Result<StoredAuthorizationLifecycle> {
        StoredAuthorizationLifecycle::decode(self.header.lifecycle)
    }

    pub fn validate_account(&self, core_program: &Pubkey, account_key: &Pubkey) -> Result<()> {
        self.header
            .encode()
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        require!(
            self.header.term_count != 0
                && usize::from(self.header.term_count) == self.immutable_terms.len()
                && usize::from(self.header.constraint_count) == self.credit_constraints.len()
                && usize::from(self.header.term_count) == self.capabilities.len()
                && usize::from(self.header.fee_state_count) == self.fee_states.len(),
            CoreError::InvalidWireEncoding
        );
        self.identity.validate(core_program)?;
        require!(
            self.header.fill_sequence <= self.identity.max_fills,
            CoreError::AuthorizationFillSequenceMismatch
        );
        let (expected_key, expected_bump) =
            StoredAuthorizationCandidateV0::address(core_program, &self.identity.intent_digest);
        require_keys_eq!(
            *account_key,
            expected_key,
            CoreError::AuthorizationIdentityMismatch
        );
        require_eq!(
            self.header.bump,
            expected_bump,
            CoreError::AuthorizationIdentityMismatch
        );

        let lifecycle = self.lifecycle()?;
        let mutable_rows_are_default = self
            .capabilities
            .iter()
            .all(|row| *row == AuthorizationCapabilityStateCandidateV0::default())
            && self.fee_states.is_empty();
        if lifecycle == StoredAuthorizationLifecycle::Draft
            || (lifecycle == StoredAuthorizationLifecycle::Cancelled && mutable_rows_are_default)
        {
            require!(
                self.header.fill_sequence == 0
                    && self.header.fee_state_count == 0
                    && self.pending_execution_digest == [0; 32]
                    && mutable_rows_are_default,
                CoreError::InvalidWireEncoding
            );
            for (index, term) in self.immutable_terms.iter().enumerate() {
                if self.header.term_written_bitmap & (1_u16 << index) == 0 {
                    require!(
                        *term == StoredIntentCapabilityTermCandidateV0::default(),
                        CoreError::InvalidWireEncoding
                    );
                } else {
                    require_eq!(
                        usize::from(term.intent_local_term_index),
                        index,
                        CoreError::InvalidWireEncoding
                    );
                    term.wire_row()?;
                }
            }
            for (index, constraint) in self.credit_constraints.iter().enumerate() {
                if self.header.constraint_written_bitmap & (1_u16 << index) == 0 {
                    require!(
                        *constraint == StoredCreditConstraintCandidateV0::default(),
                        CoreError::InvalidWireEncoding
                    );
                } else {
                    require_eq!(
                        usize::from(constraint.constraint_index),
                        index,
                        CoreError::InvalidWireEncoding
                    );
                    constraint.wire_row()?;
                }
            }
            return Ok(());
        }

        require_eq!(
            self.header.term_written_bitmap,
            exact_prefix_bitmap(self.immutable_terms.len())?,
            CoreError::InvalidWireEncoding
        );
        require_eq!(
            self.header.constraint_written_bitmap,
            exact_prefix_bitmap(self.credit_constraints.len())?,
            CoreError::InvalidWireEncoding
        );
        let term_rows = self
            .immutable_terms
            .iter()
            .map(StoredIntentCapabilityTermCandidateV0::wire_row)
            .collect::<Result<Vec<_>>>()?;
        let constraint_rows = self
            .credit_constraints
            .iter()
            .map(StoredCreditConstraintCandidateV0::wire_row)
            .collect::<Result<Vec<_>>>()?;
        let capability_rows = self
            .capabilities
            .iter()
            .map(AuthorizationCapabilityStateCandidateV0::wire_row)
            .collect::<Result<Vec<_>>>()?;
        let fee_rows = self
            .fee_states
            .iter()
            .map(AuthorizationFeeStateCandidateV0::wire_row)
            .collect::<Result<Vec<_>>>()?;
        compute_authorization_capability_state_root(&capability_rows)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        compute_authorization_fee_state_root(&fee_rows)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        StoredAuthorizationCandidateV0::validate_core_terms(
            &self.identity,
            &term_rows,
            &constraint_rows,
        )?;
        StoredAuthorizationCandidateV0::validate_immutable_to_mutable_mapping_rows(
            &term_rows,
            &constraint_rows,
            &capability_rows,
            &fee_rows,
        )?;
        match lifecycle {
            StoredAuthorizationLifecycle::Executing => require!(
                self.pending_execution_digest != [0; 32]
                    && self.header.fill_sequence < self.identity.max_fills,
                CoreError::AuthorizationSnapshotMismatch
            ),
            StoredAuthorizationLifecycle::Active => require!(
                self.pending_execution_digest == [0; 32]
                    && self.header.fill_sequence < self.identity.max_fills,
                CoreError::AuthorizationSnapshotMismatch
            ),
            StoredAuthorizationLifecycle::Consumed => require!(
                self.pending_execution_digest == [0; 32] && self.header.fill_sequence != 0,
                CoreError::AuthorizationSnapshotMismatch
            ),
            StoredAuthorizationLifecycle::Cancelled => require!(
                self.pending_execution_digest == [0; 32],
                CoreError::AuthorizationSnapshotMismatch
            ),
            StoredAuthorizationLifecycle::Draft => unreachable!("draft returned above"),
        }
        Ok(())
    }

    pub fn capability_state_root(&self) -> Result<[u8; 32]> {
        let rows = self
            .capabilities
            .iter()
            .map(AuthorizationCapabilityStateCandidateV0::wire_row)
            .collect::<Result<Vec<_>>>()?;
        compute_authorization_capability_state_root(&rows)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))
    }

    pub fn fee_state_root(&self) -> Result<[u8; 32]> {
        let rows = self
            .fee_states
            .iter()
            .map(AuthorizationFeeStateCandidateV0::wire_row)
            .collect::<Result<Vec<_>>>()?;
        compute_authorization_fee_state_root(&rows)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))
    }
}

fn exact_stored_slice(data: &[u8], offset: usize, length: usize) -> Result<&[u8]> {
    data.get(offset..offset + length)
        .ok_or_else(|| error!(CoreError::InvalidWireEncoding))
}

fn exact_stored_slice_mut(data: &mut [u8], offset: usize, length: usize) -> Result<&mut [u8]> {
    data.get_mut(offset..offset + length)
        .ok_or_else(|| error!(CoreError::InvalidWireEncoding))
}

fn require_zero_storage(bytes: &[u8]) -> Result<()> {
    require!(
        bytes.iter().all(|byte| *byte == 0),
        CoreError::InvalidWireEncoding
    );
    Ok(())
}

pub fn decode_stored_authorization_compact_exact(
    data: &[u8],
    core_program: &Pubkey,
    account_key: &Pubkey,
) -> Result<StoredAuthorizationCompactCandidateV0> {
    require_eq!(
        data.len(),
        StoredAuthorizationCandidateV0::SPACE,
        CoreError::InvalidWireEncoding
    );
    require!(
        data[..8] == STORED_AUTHORIZATION_ACCOUNT_DISCRIMINATOR,
        CoreError::InvalidWireEncoding
    );
    let header = StoredAuthorizationHeaderCandidateV0::decode_exact(exact_stored_slice(
        data,
        STORED_AUTHORIZATION_HEADER_OFFSET,
        generic_effect_private_wire::STORED_AUTHORIZATION_HEADER_LEN,
    )?)
    .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    let identity = IntentIdentityCandidateV0::decode_storage_exact(exact_stored_slice(
        data,
        STORED_AUTHORIZATION_IDENTITY_OFFSET,
        IntentIdentityCandidateV0::ENCODED_LEN,
    )?)?;
    let pending_execution_digest =
        exact_stored_slice(data, STORED_AUTHORIZATION_PENDING_OFFSET, 32)?
            .try_into()
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;

    let term_count = usize::from(header.term_count);
    let constraint_count = usize::from(header.constraint_count);
    let mut immutable_terms = Vec::with_capacity(term_count);
    for index in 0..MAX_STORED_INTENT_TERMS {
        let bytes = exact_stored_slice(
            data,
            STORED_AUTHORIZATION_TERMS_OFFSET
                + index * StoredIntentCapabilityTermCandidateV0::ENCODED_LEN,
            StoredIntentCapabilityTermCandidateV0::ENCODED_LEN,
        )?;
        if index < term_count && header.term_written_bitmap & (1_u16 << index) != 0 {
            let row = IntentCapabilityTermRowCandidateV0::decode_exact(bytes)
                .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
            immutable_terms.push(StoredIntentCapabilityTermCandidateV0::from_wire_row(row));
        } else {
            require_zero_storage(bytes)?;
            if index < term_count {
                immutable_terms.push(StoredIntentCapabilityTermCandidateV0::default());
            }
        }
    }

    let mut credit_constraints = Vec::with_capacity(constraint_count);
    for index in 0..MAX_STORED_CREDIT_CONSTRAINTS {
        let bytes = exact_stored_slice(
            data,
            STORED_AUTHORIZATION_CONSTRAINTS_OFFSET
                + index * StoredCreditConstraintCandidateV0::ENCODED_LEN,
            StoredCreditConstraintCandidateV0::ENCODED_LEN,
        )?;
        if index < constraint_count && header.constraint_written_bitmap & (1_u16 << index) != 0 {
            let row = CreditConstraintRowCandidateV0::decode_exact(bytes)
                .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
            credit_constraints.push(StoredCreditConstraintCandidateV0::from_wire_row(row));
        } else {
            require_zero_storage(bytes)?;
            if index < constraint_count {
                credit_constraints.push(StoredCreditConstraintCandidateV0::default());
            }
        }
    }

    let lifecycle = StoredAuthorizationLifecycle::decode(header.lifecycle)?;
    let capability_region = exact_stored_slice(
        data,
        STORED_AUTHORIZATION_CAPABILITIES_OFFSET,
        MAX_SETTLEMENT_CAPABILITIES * AuthorizationCapabilityStateCandidateV0::ENCODED_LEN,
    )?;
    let fee_region = exact_stored_slice(
        data,
        STORED_AUTHORIZATION_FEES_OFFSET,
        MAX_STORED_FEE_STATES * AuthorizationFeeStateCandidateV0::ENCODED_LEN,
    )?;
    let draft_like = lifecycle == StoredAuthorizationLifecycle::Draft
        || (lifecycle == StoredAuthorizationLifecycle::Cancelled
            && capability_region.iter().all(|byte| *byte == 0)
            && fee_region.iter().all(|byte| *byte == 0));
    let mut capabilities = Vec::with_capacity(term_count);
    for index in 0..MAX_SETTLEMENT_CAPABILITIES {
        let bytes = exact_stored_slice(
            data,
            STORED_AUTHORIZATION_CAPABILITIES_OFFSET
                + index * AuthorizationCapabilityStateCandidateV0::ENCODED_LEN,
            AuthorizationCapabilityStateCandidateV0::ENCODED_LEN,
        )?;
        if index < term_count && !draft_like {
            let row = AuthorizationCapabilityStateRowCandidateV0::decode_exact(bytes)
                .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
            capabilities.push(AuthorizationCapabilityStateCandidateV0::from_wire_row(row));
        } else {
            require_zero_storage(bytes)?;
            if index < term_count {
                capabilities.push(AuthorizationCapabilityStateCandidateV0::default());
            }
        }
    }

    let fee_state_count = usize::from(header.fee_state_count);
    let mut fee_states = Vec::with_capacity(fee_state_count);
    for index in 0..MAX_STORED_FEE_STATES {
        let bytes = exact_stored_slice(
            data,
            STORED_AUTHORIZATION_FEES_OFFSET
                + index * AuthorizationFeeStateCandidateV0::ENCODED_LEN,
            AuthorizationFeeStateCandidateV0::ENCODED_LEN,
        )?;
        if index < fee_state_count && !draft_like {
            let row = AuthorizationFeeStateRowCandidateV0::decode_exact(bytes)
                .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
            fee_states.push(AuthorizationFeeStateCandidateV0::from_wire_row(row));
        } else {
            require_zero_storage(bytes)?;
        }
    }

    let compact = StoredAuthorizationCompactCandidateV0 {
        header,
        identity,
        pending_execution_digest,
        immutable_terms,
        credit_constraints,
        capabilities,
        fee_states,
    };
    compact.validate_account(core_program, account_key)?;
    Ok(compact)
}

pub fn read_stored_authorization_compact(
    account: &AccountInfo<'_>,
    core_program: &Pubkey,
) -> Result<StoredAuthorizationCompactCandidateV0> {
    require_keys_eq!(
        *account.owner,
        *core_program,
        CoreError::AuthorizationIdentityMismatch
    );
    let data = account
        .try_borrow_data()
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    decode_stored_authorization_compact_exact(&data, core_program, account.key)
}

fn write_stored_header_exact(
    data: &mut [u8],
    header: &StoredAuthorizationHeaderCandidateV0,
) -> Result<()> {
    let encoded = header
        .encode()
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    exact_stored_slice_mut(data, STORED_AUTHORIZATION_HEADER_OFFSET, encoded.len())?
        .copy_from_slice(&encoded);
    Ok(())
}

fn write_stored_term_exact(
    data: &mut [u8],
    index: usize,
    term: &StoredIntentCapabilityTermCandidateV0,
) -> Result<()> {
    require!(
        index < MAX_STORED_INTENT_TERMS,
        CoreError::ExperimentLimitExceeded
    );
    let encoded = term
        .wire_row()?
        .encode()
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    exact_stored_slice_mut(
        data,
        STORED_AUTHORIZATION_TERMS_OFFSET
            + index * StoredIntentCapabilityTermCandidateV0::ENCODED_LEN,
        encoded.len(),
    )?
    .copy_from_slice(&encoded);
    Ok(())
}

fn write_stored_constraint_exact(
    data: &mut [u8],
    index: usize,
    constraint: &StoredCreditConstraintCandidateV0,
) -> Result<()> {
    require!(
        index < MAX_STORED_CREDIT_CONSTRAINTS,
        CoreError::ExperimentLimitExceeded
    );
    let encoded = constraint
        .wire_row()?
        .encode()
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    exact_stored_slice_mut(
        data,
        STORED_AUTHORIZATION_CONSTRAINTS_OFFSET
            + index * StoredCreditConstraintCandidateV0::ENCODED_LEN,
        encoded.len(),
    )?
    .copy_from_slice(&encoded);
    Ok(())
}

fn write_stored_capability_exact(
    data: &mut [u8],
    index: usize,
    capability: &AuthorizationCapabilityStateCandidateV0,
) -> Result<()> {
    require!(
        index < MAX_SETTLEMENT_CAPABILITIES,
        CoreError::ExperimentLimitExceeded
    );
    let encoded = capability
        .wire_row()?
        .encode()
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    exact_stored_slice_mut(
        data,
        STORED_AUTHORIZATION_CAPABILITIES_OFFSET
            + index * AuthorizationCapabilityStateCandidateV0::ENCODED_LEN,
        encoded.len(),
    )?
    .copy_from_slice(&encoded);
    Ok(())
}

fn write_stored_fee_exact(
    data: &mut [u8],
    index: usize,
    fee: &AuthorizationFeeStateCandidateV0,
) -> Result<()> {
    require!(
        index < MAX_STORED_FEE_STATES,
        CoreError::ExperimentLimitExceeded
    );
    let encoded = fee
        .wire_row()?
        .encode()
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    exact_stored_slice_mut(
        data,
        STORED_AUTHORIZATION_FEES_OFFSET + index * AuthorizationFeeStateCandidateV0::ENCODED_LEN,
        encoded.len(),
    )?
    .copy_from_slice(&encoded);
    Ok(())
}

pub fn initialize_stored_authorization_draft_exact(
    account: &AccountInfo<'_>,
    core_program: &Pubkey,
    actor: &Pubkey,
    identity: &IntentIdentityCandidateV0,
    term_count: u8,
    constraint_count: u8,
) -> Result<()> {
    require_keys_eq!(
        *account.owner,
        *core_program,
        CoreError::AuthorizationIdentityMismatch
    );
    require!(account.is_writable, CoreError::InvalidWireEncoding);
    require_keys_eq!(
        identity.actor,
        *actor,
        CoreError::AuthorizationIdentityMismatch
    );
    identity.validate(core_program)?;
    require!(
        term_count != 0
            && usize::from(term_count) <= MAX_STORED_INTENT_TERMS
            && usize::from(constraint_count) <= MAX_STORED_CREDIT_CONSTRAINTS,
        CoreError::ExperimentLimitExceeded
    );
    let (expected_key, bump) =
        StoredAuthorizationCandidateV0::address(core_program, &identity.intent_digest);
    require_keys_eq!(
        *account.key,
        expected_key,
        CoreError::AuthorizationIdentityMismatch
    );
    let header = StoredAuthorizationHeaderCandidateV0 {
        wire_version: WIRE_VERSION_V0,
        lifecycle: StoredAuthorizationLifecycle::DRAFT,
        bump,
        term_count,
        constraint_count,
        fee_state_count: 0,
        flags: 0,
        reserved: 0,
        term_written_bitmap: 0,
        constraint_written_bitmap: 0,
        fill_sequence: 0,
    };
    let mut data = account
        .try_borrow_mut_data()
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    require_eq!(
        data.len(),
        StoredAuthorizationCandidateV0::SPACE,
        CoreError::InvalidWireEncoding
    );
    require_zero_storage(&data)?;
    data[..8].copy_from_slice(&STORED_AUTHORIZATION_ACCOUNT_DISCRIMINATOR);
    write_stored_header_exact(&mut data, &header)?;
    exact_stored_slice_mut(
        &mut data,
        STORED_AUTHORIZATION_IDENTITY_OFFSET,
        IntentIdentityCandidateV0::ENCODED_LEN,
    )?
    .copy_from_slice(&identity.encode_storage_exact());
    drop(data);
    read_stored_authorization_compact(account, core_program)?;
    Ok(())
}

pub fn write_stored_authorization_term_chunk_exact(
    account: &AccountInfo<'_>,
    core_program: &Pubkey,
    actor: &Pubkey,
    start_index: u8,
    rows: &[StoredIntentCapabilityTermCandidateV0],
) -> Result<()> {
    require!(account.is_writable, CoreError::InvalidWireEncoding);
    let mut compact = read_stored_authorization_compact(account, core_program)?;
    require_keys_eq!(
        compact.identity.actor,
        *actor,
        CoreError::AuthorizationIdentityMismatch
    );
    require!(
        compact.lifecycle()? == StoredAuthorizationLifecycle::Draft && !rows.is_empty(),
        CoreError::AuthorizationUnavailable
    );
    let start = usize::from(start_index);
    let end = start
        .checked_add(rows.len())
        .ok_or(CoreError::ArithmeticOverflow)?;
    require!(
        end <= compact.immutable_terms.len(),
        CoreError::ExperimentLimitExceeded
    );
    for (offset, row) in rows.iter().enumerate() {
        let index = start + offset;
        require_eq!(
            usize::from(row.intent_local_term_index),
            index,
            CoreError::InvalidWireEncoding
        );
        require_eq!(
            compact.header.term_written_bitmap & (1_u16 << index),
            0,
            CoreError::InvalidWireEncoding
        );
        row.wire_row()?;
    }
    let mut data = account
        .try_borrow_mut_data()
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    for (offset, row) in rows.iter().enumerate() {
        let index = start + offset;
        write_stored_term_exact(&mut data, index, row)?;
        compact.header.term_written_bitmap |= 1_u16 << index;
    }
    write_stored_header_exact(&mut data, &compact.header)?;
    drop(data);
    read_stored_authorization_compact(account, core_program)?;
    Ok(())
}

pub fn write_stored_authorization_constraint_chunk_exact(
    account: &AccountInfo<'_>,
    core_program: &Pubkey,
    actor: &Pubkey,
    start_index: u8,
    rows: &[StoredCreditConstraintCandidateV0],
) -> Result<()> {
    require!(account.is_writable, CoreError::InvalidWireEncoding);
    let mut compact = read_stored_authorization_compact(account, core_program)?;
    require_keys_eq!(
        compact.identity.actor,
        *actor,
        CoreError::AuthorizationIdentityMismatch
    );
    require!(
        compact.lifecycle()? == StoredAuthorizationLifecycle::Draft && !rows.is_empty(),
        CoreError::AuthorizationUnavailable
    );
    let start = usize::from(start_index);
    let end = start
        .checked_add(rows.len())
        .ok_or(CoreError::ArithmeticOverflow)?;
    require!(
        end <= compact.credit_constraints.len(),
        CoreError::ExperimentLimitExceeded
    );
    for (offset, row) in rows.iter().enumerate() {
        let index = start + offset;
        require_eq!(
            usize::from(row.constraint_index),
            index,
            CoreError::InvalidWireEncoding
        );
        require_eq!(
            compact.header.constraint_written_bitmap & (1_u16 << index),
            0,
            CoreError::InvalidWireEncoding
        );
        row.wire_row()?;
    }
    let mut data = account
        .try_borrow_mut_data()
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    for (offset, row) in rows.iter().enumerate() {
        let index = start + offset;
        write_stored_constraint_exact(&mut data, index, row)?;
        compact.header.constraint_written_bitmap |= 1_u16 << index;
    }
    write_stored_header_exact(&mut data, &compact.header)?;
    drop(data);
    read_stored_authorization_compact(account, core_program)?;
    Ok(())
}

pub fn activate_stored_authorization_exact(
    account: &AccountInfo<'_>,
    core_program: &Pubkey,
    actor: &Pubkey,
) -> Result<()> {
    require!(account.is_writable, CoreError::InvalidWireEncoding);
    let mut compact = read_stored_authorization_compact(account, core_program)?;
    require_keys_eq!(
        compact.identity.actor,
        *actor,
        CoreError::AuthorizationIdentityMismatch
    );
    require!(
        compact.lifecycle()? == StoredAuthorizationLifecycle::Draft,
        CoreError::AuthorizationUnavailable
    );
    require_eq!(
        compact.header.term_written_bitmap,
        exact_prefix_bitmap(compact.immutable_terms.len())?,
        CoreError::InvalidWireEncoding
    );
    require_eq!(
        compact.header.constraint_written_bitmap,
        exact_prefix_bitmap(compact.credit_constraints.len())?,
        CoreError::InvalidWireEncoding
    );
    let term_rows = compact
        .immutable_terms
        .iter()
        .map(StoredIntentCapabilityTermCandidateV0::wire_row)
        .collect::<Result<Vec<_>>>()?;
    let constraint_rows = compact
        .credit_constraints
        .iter()
        .map(StoredCreditConstraintCandidateV0::wire_row)
        .collect::<Result<Vec<_>>>()?;
    StoredAuthorizationCandidateV0::validate_core_terms(
        &compact.identity,
        &term_rows,
        &constraint_rows,
    )?;
    compact.capabilities = term_rows
        .iter()
        .map(|term| AuthorizationCapabilityStateCandidateV0 {
            local_term_index: term.intent_local_term_index,
            reserved_0: 0,
            flags: term.flags,
            reserved: [0; 5],
            initial_maximum_engine_debit: term.maximum_engine_debit,
            initial_minimum_credit: term.minimum_credit,
            initial_maximum_total_debit: term.maximum_total_debit,
            remaining_total_debit: term.maximum_total_debit,
            cumulative_engine_debit: 0,
            cumulative_fee_debit: 0,
            cumulative_credit: 0,
        })
        .collect();
    let capability_rows = compact
        .capabilities
        .iter()
        .map(AuthorizationCapabilityStateCandidateV0::wire_row)
        .collect::<Result<Vec<_>>>()?;
    StoredAuthorizationCandidateV0::validate_immutable_to_mutable_mapping_rows(
        &term_rows,
        &constraint_rows,
        &capability_rows,
        &[],
    )?;
    compact.header.lifecycle = StoredAuthorizationLifecycle::ACTIVE;
    compact.validate_account(core_program, account.key)?;
    let mut data = account
        .try_borrow_mut_data()
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    exact_stored_slice_mut(
        &mut data,
        STORED_AUTHORIZATION_CAPABILITIES_OFFSET,
        MAX_SETTLEMENT_CAPABILITIES * AuthorizationCapabilityStateCandidateV0::ENCODED_LEN,
    )?
    .fill(0);
    for (index, row) in compact.capabilities.iter().enumerate() {
        write_stored_capability_exact(&mut data, index, row)?;
    }
    write_stored_header_exact(&mut data, &compact.header)?;
    drop(data);
    read_stored_authorization_compact(account, core_program)?;
    Ok(())
}

pub fn cancel_stored_authorization_exact(
    account: &AccountInfo<'_>,
    core_program: &Pubkey,
    actor: &Pubkey,
) -> Result<()> {
    require!(account.is_writable, CoreError::InvalidWireEncoding);
    let mut compact = read_stored_authorization_compact(account, core_program)?;
    require_keys_eq!(
        compact.identity.actor,
        *actor,
        CoreError::AuthorizationIdentityMismatch
    );
    require!(
        matches!(
            compact.lifecycle()?,
            StoredAuthorizationLifecycle::Draft | StoredAuthorizationLifecycle::Active
        ),
        CoreError::AuthorizationUnavailable
    );
    compact.header.lifecycle = StoredAuthorizationLifecycle::CANCELLED;
    let mut data = account
        .try_borrow_mut_data()
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    write_stored_header_exact(&mut data, &compact.header)?;
    drop(data);
    read_stored_authorization_compact(account, core_program)?;
    Ok(())
}

pub fn reserve_stored_authorization_execution_exact(
    account: &AccountInfo<'_>,
    core_program: &Pubkey,
    current_slot: u64,
    expected_fill_sequence: u32,
    execution_digest: [u8; 32],
) -> Result<StoredAuthorizationCompactCandidateV0> {
    require!(account.is_writable, CoreError::InvalidWireEncoding);
    require!(
        execution_digest != [0; 32],
        CoreError::AuthorizationSnapshotMismatch
    );
    let mut compact = read_stored_authorization_compact(account, core_program)?;
    require!(
        compact.lifecycle()? == StoredAuthorizationLifecycle::Active,
        CoreError::AuthorizationUnavailable
    );
    require!(
        current_slot < compact.identity.expires_at_slot_exclusive,
        CoreError::AuthorizationExpired
    );
    require_eq!(
        compact.header.fill_sequence,
        expected_fill_sequence,
        CoreError::AuthorizationFillSequenceMismatch
    );
    compact.header.lifecycle = StoredAuthorizationLifecycle::EXECUTING;
    compact.pending_execution_digest = execution_digest;
    let mut data = account
        .try_borrow_mut_data()
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    write_stored_header_exact(&mut data, &compact.header)?;
    exact_stored_slice_mut(&mut data, STORED_AUTHORIZATION_PENDING_OFFSET, 32)?
        .copy_from_slice(&execution_digest);
    drop(data);
    Ok(compact)
}

/// Reborrows and authenticates the post-CPI tombstone before any settlement
/// result is accepted. Callers must not keep account-data borrows across the
/// engine invocation; this helper deliberately performs a fresh exact decode.
pub fn verify_stored_authorization_execution_pending_exact(
    account: &AccountInfo<'_>,
    core_program: &Pubkey,
    execution_digest: &[u8; 32],
) -> Result<StoredAuthorizationCompactCandidateV0> {
    require!(
        *execution_digest != [0; 32],
        CoreError::AuthorizationSnapshotMismatch
    );
    let compact = read_stored_authorization_compact(account, core_program)?;
    require!(
        compact.lifecycle()? == StoredAuthorizationLifecycle::Executing,
        CoreError::AuthorizationUnavailable
    );
    require!(
        compact.pending_execution_digest == *execution_digest,
        CoreError::AuthorizationSnapshotMismatch
    );
    Ok(compact)
}

/// Minimal commit authority captured from a fully authenticated post-CPI
/// stored-authorization view. The fields are private so callers cannot bypass
/// the exact decode performed by [`verify_stored_authorization_execution_pending_exact`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct VerifiedStoredAuthorizationCommitV0 {
    account_key: Pubkey,
    header: StoredAuthorizationHeaderCandidateV0,
    intent_digest: [u8; 32],
    max_fills: u32,
}

impl VerifiedStoredAuthorizationCommitV0 {
    fn capture(account: &AccountInfo<'_>, compact: &StoredAuthorizationCompactCandidateV0) -> Self {
        Self {
            account_key: *account.key,
            header: compact.header,
            intent_digest: compact.identity.intent_digest,
            max_fills: compact.identity.max_fills,
        }
    }
}

/// Returns the complete authenticated view needed for preview construction and
/// a sealed, allocation-free commit witness for that same account state.
pub(crate) fn verify_stored_authorization_execution_for_commit_exact(
    account: &AccountInfo<'_>,
    core_program: &Pubkey,
    execution_digest: &[u8; 32],
) -> Result<(
    StoredAuthorizationCompactCandidateV0,
    VerifiedStoredAuthorizationCommitV0,
)> {
    let compact = verify_stored_authorization_execution_pending_exact(
        account,
        core_program,
        execution_digest,
    )?;
    let verified = VerifiedStoredAuthorizationCommitV0::capture(account, &compact);
    Ok((compact, verified))
}

/// Commits a preview whose complete stored account was authenticated after the
/// engine callback and before settlement. No untrusted CPI occurs between that
/// authentication and this write. Rechecking the owner, PDA identity, exact
/// header, pending digest, and immutable max-fill field makes that invariant
/// explicit without decoding and allocating all fixed-capacity stored rows a
/// second time.
#[inline(never)]
pub(crate) fn commit_verified_stored_authorization_execution_exact(
    account: &AccountInfo<'_>,
    core_program: &Pubkey,
    execution_digest: &[u8; 32],
    verified: &VerifiedStoredAuthorizationCommitV0,
    next_capabilities: &[AuthorizationCapabilityStateCandidateV0],
    next_fee_states: &[AuthorizationFeeStateCandidateV0],
    terminal: bool,
) -> Result<()> {
    require!(account.is_writable, CoreError::InvalidWireEncoding);
    require_keys_eq!(
        *account.owner,
        *core_program,
        CoreError::AuthorizationIdentityMismatch
    );
    require_keys_eq!(
        *account.key,
        verified.account_key,
        CoreError::AuthorizationIdentityMismatch
    );
    let (expected_key, _) =
        StoredAuthorizationCandidateV0::address(core_program, &verified.intent_digest);
    require_keys_eq!(
        *account.key,
        expected_key,
        CoreError::AuthorizationIdentityMismatch
    );
    require!(
        *execution_digest != [0; 32],
        CoreError::AuthorizationSnapshotMismatch
    );
    require!(
        verified.header.lifecycle == StoredAuthorizationLifecycle::EXECUTING,
        CoreError::AuthorizationUnavailable
    );
    require_eq!(
        next_capabilities.len(),
        usize::from(verified.header.term_count),
        CoreError::InvalidWireEncoding
    );
    require!(
        next_fee_states.len() <= MAX_STORED_FEE_STATES,
        CoreError::ExperimentLimitExceeded
    );

    {
        let data = account
            .try_borrow_data()
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        require_eq!(
            data.len(),
            StoredAuthorizationCandidateV0::SPACE,
            CoreError::InvalidWireEncoding
        );
        require!(
            data[..8] == STORED_AUTHORIZATION_ACCOUNT_DISCRIMINATOR,
            CoreError::InvalidWireEncoding
        );
        let current_header =
            StoredAuthorizationHeaderCandidateV0::decode_exact(exact_stored_slice(
                &data,
                STORED_AUTHORIZATION_HEADER_OFFSET,
                generic_effect_private_wire::STORED_AUTHORIZATION_HEADER_LEN,
            )?)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        require!(
            current_header == verified.header,
            CoreError::AuthorizationSnapshotMismatch
        );
        let current_max_fills = u32::from_le_bytes(
            exact_stored_slice(
                &data,
                STORED_AUTHORIZATION_IDENTITY_OFFSET
                    + IntentIdentityCandidateV0::MAX_FILLS_STORAGE_OFFSET,
                core::mem::size_of::<u32>(),
            )?
            .try_into()
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?,
        );
        require_eq!(
            current_max_fills,
            verified.max_fills,
            CoreError::AuthorizationSnapshotMismatch
        );
        require!(
            exact_stored_slice(
                &data,
                STORED_AUTHORIZATION_IDENTITY_OFFSET
                    + IntentIdentityCandidateV0::INTENT_DIGEST_STORAGE_OFFSET,
                32,
            )? == verified.intent_digest,
            CoreError::AuthorizationSnapshotMismatch
        );
        require!(
            exact_stored_slice(&data, STORED_AUTHORIZATION_PENDING_OFFSET, 32)? == execution_digest,
            CoreError::AuthorizationSnapshotMismatch
        );
    }

    let mut next_header = verified.header;
    next_header.fill_sequence = next_header
        .fill_sequence
        .checked_add(1)
        .ok_or(CoreError::ArithmeticOverflow)?;
    next_header.fee_state_count =
        u8::try_from(next_fee_states.len()).map_err(|_| CoreError::ExperimentLimitExceeded)?;
    let reached_max_fills = next_header.fill_sequence >= verified.max_fills;
    next_header.lifecycle = if terminal || reached_max_fills {
        StoredAuthorizationLifecycle::CONSUMED
    } else {
        StoredAuthorizationLifecycle::ACTIVE
    };

    let mut data = account
        .try_borrow_mut_data()
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    exact_stored_slice_mut(
        &mut data,
        STORED_AUTHORIZATION_CAPABILITIES_OFFSET,
        MAX_SETTLEMENT_CAPABILITIES * AuthorizationCapabilityStateCandidateV0::ENCODED_LEN,
    )?
    .fill(0);
    exact_stored_slice_mut(
        &mut data,
        STORED_AUTHORIZATION_FEES_OFFSET,
        MAX_STORED_FEE_STATES * AuthorizationFeeStateCandidateV0::ENCODED_LEN,
    )?
    .fill(0);
    for (index, row) in next_capabilities.iter().enumerate() {
        write_stored_capability_exact(&mut data, index, row)?;
    }
    for (index, row) in next_fee_states.iter().enumerate() {
        write_stored_fee_exact(&mut data, index, row)?;
    }
    exact_stored_slice_mut(&mut data, STORED_AUTHORIZATION_PENDING_OFFSET, 32)?.fill(0);
    write_stored_header_exact(&mut data, &next_header)?;
    Ok(())
}

pub fn commit_stored_authorization_execution_exact(
    account: &AccountInfo<'_>,
    core_program: &Pubkey,
    execution_digest: &[u8; 32],
    next_capabilities: &[AuthorizationCapabilityStateCandidateV0],
    next_fee_states: &[AuthorizationFeeStateCandidateV0],
    terminal: bool,
) -> Result<()> {
    require!(account.is_writable, CoreError::InvalidWireEncoding);
    let mut compact = verify_stored_authorization_execution_pending_exact(
        account,
        core_program,
        execution_digest,
    )?;
    require_eq!(
        next_capabilities.len(),
        compact.immutable_terms.len(),
        CoreError::InvalidWireEncoding
    );
    require!(
        next_fee_states.len() <= MAX_STORED_FEE_STATES,
        CoreError::ExperimentLimitExceeded
    );
    compact.header.fill_sequence = compact
        .header
        .fill_sequence
        .checked_add(1)
        .ok_or(CoreError::ArithmeticOverflow)?;
    compact.header.fee_state_count =
        u8::try_from(next_fee_states.len()).map_err(|_| CoreError::ExperimentLimitExceeded)?;
    compact.capabilities = next_capabilities.to_vec();
    compact.fee_states = next_fee_states.to_vec();
    compact.pending_execution_digest = [0; 32];
    let reached_max_fills = compact.header.fill_sequence >= compact.identity.max_fills;
    compact.header.lifecycle = if terminal || reached_max_fills {
        StoredAuthorizationLifecycle::CONSUMED
    } else {
        StoredAuthorizationLifecycle::ACTIVE
    };
    compact.validate_account(core_program, account.key)?;
    let mut data = account
        .try_borrow_mut_data()
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    exact_stored_slice_mut(
        &mut data,
        STORED_AUTHORIZATION_CAPABILITIES_OFFSET,
        MAX_SETTLEMENT_CAPABILITIES * AuthorizationCapabilityStateCandidateV0::ENCODED_LEN,
    )?
    .fill(0);
    exact_stored_slice_mut(
        &mut data,
        STORED_AUTHORIZATION_FEES_OFFSET,
        MAX_STORED_FEE_STATES * AuthorizationFeeStateCandidateV0::ENCODED_LEN,
    )?
    .fill(0);
    for (index, row) in compact.capabilities.iter().enumerate() {
        write_stored_capability_exact(&mut data, index, row)?;
    }
    for (index, row) in compact.fee_states.iter().enumerate() {
        write_stored_fee_exact(&mut data, index, row)?;
    }
    exact_stored_slice_mut(&mut data, STORED_AUTHORIZATION_PENDING_OFFSET, 32)?.fill(0);
    write_stored_header_exact(&mut data, &compact.header)?;
    Ok(())
}

pub fn replace_stored_authorization_same_actor_exact(
    old_account: &AccountInfo<'_>,
    new_account: &AccountInfo<'_>,
    core_program: &Pubkey,
    actor: &Pubkey,
) -> Result<()> {
    require!(
        old_account.key != new_account.key,
        CoreError::AuthorizationIdentityMismatch
    );
    let old = read_stored_authorization_compact(old_account, core_program)?;
    let new = read_stored_authorization_compact(new_account, core_program)?;
    require_keys_eq!(
        old.identity.actor,
        *actor,
        CoreError::AuthorizationIdentityMismatch
    );
    require_keys_eq!(
        new.identity.actor,
        *actor,
        CoreError::AuthorizationIdentityMismatch
    );
    require!(
        old.identity.actor == new.identity.actor,
        CoreError::AuthorizationIdentityMismatch
    );
    require!(
        old.lifecycle()? == StoredAuthorizationLifecycle::Active,
        CoreError::AuthorizationUnavailable
    );
    require!(
        new.lifecycle()? == StoredAuthorizationLifecycle::Draft,
        CoreError::AuthorizationUnavailable
    );
    require!(
        old.identity.intent_digest != new.identity.intent_digest,
        CoreError::AuthorizationIdentityMismatch
    );
    activate_stored_authorization_exact(new_account, core_program, actor)?;
    cancel_stored_authorization_exact(old_account, core_program, actor)
}

#[cfg(not(target_os = "solana"))]
pub fn read_stored_authorization_exact(
    account: &AccountInfo<'_>,
    core_program: &Pubkey,
) -> Result<StoredAuthorizationCandidateV0> {
    require_keys_eq!(
        *account.owner,
        *core_program,
        CoreError::AuthorizationIdentityMismatch
    );
    let data = account
        .try_borrow_data()
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    let state = deserialize_account_exact::<StoredAuthorizationCandidateV0>(
        &data,
        StoredAuthorizationCandidateV0::SPACE,
    )?;
    state.validate_account(core_program, account.key)?;
    Ok(state)
}

#[cfg(not(target_os = "solana"))]
pub fn write_stored_authorization_exact(
    account: &AccountInfo<'_>,
    core_program: &Pubkey,
    state: &StoredAuthorizationCandidateV0,
) -> Result<()> {
    require_keys_eq!(
        *account.owner,
        *core_program,
        CoreError::AuthorizationIdentityMismatch
    );
    require!(account.is_writable, CoreError::InvalidWireEncoding);
    state.validate_account(core_program, account.key)?;
    let mut data = account
        .try_borrow_mut_data()
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    serialize_account_exact(state, &mut data, StoredAuthorizationCandidateV0::SPACE)
}

pub fn deserialize_account_exact<T>(data: &[u8], expected_space: usize) -> Result<T>
where
    T: AccountDeserialize + AnchorDeserialize + Discriminator,
{
    require_eq!(data.len(), expected_space, CoreError::InvalidWireEncoding);
    let discriminator_len = T::DISCRIMINATOR.len();
    require!(
        discriminator_len != 0 && data.len() >= discriminator_len,
        CoreError::InvalidWireEncoding
    );
    require!(
        &data[..discriminator_len] == T::DISCRIMINATOR,
        CoreError::InvalidWireEncoding
    );
    let mut payload = &data[discriminator_len..];
    let account = <T as AnchorDeserialize>::deserialize(&mut payload)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    require!(payload.is_empty(), CoreError::InvalidWireEncoding);
    Ok(account)
}

pub fn serialize_account_exact<T: AccountSerialize>(
    account: &T,
    destination: &mut [u8],
    expected_space: usize,
) -> Result<()> {
    require_eq!(
        destination.len(),
        expected_space,
        CoreError::InvalidWireEncoding
    );
    let mut encoded = Vec::with_capacity(expected_space);
    account
        .try_serialize(&mut encoded)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    require_eq!(
        encoded.len(),
        expected_space,
        CoreError::InvalidWireEncoding
    );
    destination.copy_from_slice(&encoded);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refresh_market_binding_digest(
        market: &mut MarketDescriptorCandidateV0,
        core: &Pubkey,
        market_key: &Pubkey,
    ) {
        let row = generic_effect_private_wire::MarketBindingRowCandidateV0 {
            core_program: core.to_bytes(),
            core_experimental_major: market.experimental_major,
            market_descriptor_key: market_key.to_bytes(),
            market_descriptor_revision: market.market_descriptor_revision,
            engine_program: market.engine_program.to_bytes(),
            engine_interface_id: market.engine_interface_id,
            engine_instance_id: market.engine_instance_id,
            engine_admission_policy_digest: market.engine_admission_policy_digest,
            domain_admission_profile_digest: market.domain_admission_profile_digest,
            protected_profile_digest: market.protected_profile_digest,
            fee_policy_digest: market.fee_policy_digest,
            opaque_schema_digest: market.opaque_schema_digest,
        };
        market.market_binding_digest = row.digest().unwrap();
    }

    fn market_fixture(
        core: &Pubkey,
        market_key: &Pubkey,
        protected_profile_digest: [u8; 32],
    ) -> MarketDescriptorCandidateV0 {
        let mut market = MarketDescriptorCandidateV0 {
            wire_version: WIRE_VERSION_V0,
            experimental_major: crate::constants::EXPERIMENTAL_MAJOR,
            bump: 0,
            reserved: [0; 2],
            market_binding_digest: [0; 32],
            market_descriptor_revision: 1,
            engine_program: Pubkey::new_from_array([2; 32]),
            engine_interface_id: [3; 32],
            engine_instance_id: [4; 32],
            engine_admission_policy_digest: [5; 32],
            protected_profile_digest,
            domain_admission_profile_digest: [6; 32],
            fee_policy_digest: [7; 32],
            fee_policy_revision: 1,
            opaque_schema_digest: [8; 32],
        };
        refresh_market_binding_digest(&mut market, core, market_key);
        market
    }

    fn immutable_release_fixture() -> (Pubkey, Pubkey, ImmutableEngineReleaseCandidateV0) {
        let core = Pubkey::new_from_array([41; 32]);
        let engine = Pubkey::new_from_array([42; 32]);
        let loader = anchor_lang::solana_program::bpf_loader_upgradeable::ID;
        let program_data = Pubkey::find_program_address(&[engine.as_ref()], &loader).0;
        let policy = EngineAdmissionPolicyCandidateV0 {
            policy_kind: POLICY_IMMUTABLE_DEPLOYMENT,
            engine_program: engine,
            loader_program: loader,
            program_data_or_zero: program_data,
            expected_controller_or_zero: Pubkey::default(),
            captured_programdata_slot_or_zero: 7,
        };
        let snapshot = EngineLoaderStateSnapshotCandidateV0 {
            engine_program: engine,
            loader_program: loader,
            program_data_or_zero: program_data,
            observed_programdata_slot: 7,
            observed_controller_or_zero: Pubkey::default(),
        };
        let (key, bump) = ImmutableEngineReleaseCandidateV0::address(&core, &engine);
        let mut release = ImmutableEngineReleaseCandidateV0 {
            wire_version: WIRE_VERSION_V0,
            bump,
            reserved: [0; 6],
            engine_program: engine,
            loader_program: loader,
            canonical_program_data: program_data,
            captured_programdata_slot: 7,
            observed_controller_or_zero: Pubkey::default(),
            captured_programdata_data_len: 46,
            engine_admission_policy_digest: policy.digest().unwrap(),
            loader_state_snapshot_digest: snapshot.digest().unwrap(),
            release_observation_digest: [0; 32],
        };
        release.release_observation_digest =
            release.derive_observation_digest_for_core(&core).unwrap();
        (core, key, release)
    }

    fn staged_fixture() -> (
        Pubkey,
        Pubkey,
        IntentIdentityCandidateV0,
        [StoredIntentCapabilityTermCandidateV0; 2],
    ) {
        let core = Pubkey::new_from_array([41; 32]);
        let actor = Pubkey::new_from_array([7; 32]);
        let terms = [
            StoredIntentCapabilityTermCandidateV0 {
                intent_local_term_index: 0,
                authority_class: AUTHORITY_INTENT_FUNDED_DEBIT,
                fee_class: FEE_CLASS_GROSS_DEBIT_RATE,
                flags: INTENT_CAPABILITY_TERM_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT,
                rights_bits: RIGHT_DEBIT,
                endpoint_key: Pubkey::new_from_array([8; 32]),
                asset_binding_digest: [9; 32],
                maximum_engine_debit: 100,
                maximum_total_debit: 100,
                ..Default::default()
            },
            StoredIntentCapabilityTermCandidateV0 {
                intent_local_term_index: 1,
                authority_class: AUTHORITY_EXACT_EXTERNAL_CREDIT,
                fee_class: FEE_CLASS_NONE,
                flags: 0,
                rights_bits: RIGHT_CREDIT | RIGHT_EXACT_EXTERNAL_RECIPIENT,
                endpoint_key: Pubkey::new_from_array([10; 32]),
                asset_binding_digest: [9; 32],
                minimum_credit: 10,
                ..Default::default()
            },
        ];
        let term_rows = terms
            .iter()
            .map(StoredIntentCapabilityTermCandidateV0::wire_row)
            .collect::<Result<Vec<_>>>()
            .unwrap();
        let capability_terms_root = compute_intent_capability_terms_root(&term_rows).unwrap();
        let credit_constraints_root = compute_intent_credit_constraints_root(&[]).unwrap();
        let core_terms_root = compute_intent_core_terms_root(IntentCoreTermsDigestInputs {
            maximum_successful_fills: 2,
            capability_terms_root: &capability_terms_root,
            credit_constraints_root: &credit_constraints_root,
        })
        .unwrap();
        let mut identity = IntentIdentityCandidateV0 {
            experimental_major: crate::constants::EXPERIMENTAL_MAJOR,
            core_program: core,
            actor,
            authorization_nonce: 11,
            market_binding_digest: [12; 32],
            loader_state_snapshot_digest: [13; 32],
            fee_policy_digest: [14; 32],
            engine_terms_commitment: [15; 32],
            core_terms_root,
            reserved_digest: [0; 32],
            expires_at_slot_exclusive: 1_000,
            max_fills: 2,
            intent_digest: [0; 32],
        };
        identity.intent_digest = identity.compute_intent_digest(&core).unwrap();
        (core, actor, identity, terms)
    }

    #[test]
    fn stored_layout_and_identity_lengths_are_exact() {
        assert_eq!(IntentIdentityCandidateV0::ENCODED_LEN, 312);
        assert_eq!(StoredAuthorizationCandidateV0::DATA_LEN, 4_776);
        assert_eq!(StoredAuthorizationCandidateV0::SPACE, 4_784);
        assert_eq!(
            StoredAuthorizationCandidateV0::DISCRIMINATOR,
            STORED_AUTHORIZATION_ACCOUNT_DISCRIMINATOR
        );
        const HEADER_OFFSET: usize = 8;
        const IDENTITY_OFFSET: usize = HEADER_OFFSET + 16;
        const PENDING_OFFSET: usize = IDENTITY_OFFSET + 312;
        const TERM_OFFSET: usize = PENDING_OFFSET + 32;
        const CONSTRAINT_OFFSET: usize = TERM_OFFSET + 12 * 136;
        const CAPABILITY_OFFSET: usize = CONSTRAINT_OFFSET + 12 * 64;
        const FEE_OFFSET: usize = CAPABILITY_OFFSET + 12 * 88;
        assert_eq!(FEE_OFFSET + 12 * 80, StoredAuthorizationCandidateV0::SPACE);

        let (core, actor, identity, _) = staged_fixture();
        let key = StoredAuthorizationCandidateV0::address(&core, &identity.intent_digest).0;
        let draft = StoredAuthorizationCandidateV0::initialize_draft(&core, &actor, identity, 2, 0)
            .unwrap();
        let mut bytes = vec![0_u8; StoredAuthorizationCandidateV0::SPACE];
        serialize_account_exact(&draft, &mut bytes, StoredAuthorizationCandidateV0::SPACE).unwrap();
        let encoded_header = draft.header_row().unwrap().encode().unwrap();
        assert_eq!(
            &bytes[HEADER_OFFSET..IDENTITY_OFFSET],
            encoded_header.as_slice()
        );
        assert_eq!(
            &bytes[IDENTITY_OFFSET..IDENTITY_OFFSET + 4],
            &identity.experimental_major.to_le_bytes()
        );
        assert_eq!(
            &bytes[IDENTITY_OFFSET + 4..IDENTITY_OFFSET + 36],
            identity.core_program.as_ref()
        );
        assert_eq!(
            &bytes[IDENTITY_OFFSET + 36..IDENTITY_OFFSET + 68],
            identity.actor.as_ref()
        );
        assert_eq!(
            &bytes[IDENTITY_OFFSET + IntentIdentityCandidateV0::MAX_FILLS_STORAGE_OFFSET
                ..IDENTITY_OFFSET
                    + IntentIdentityCandidateV0::MAX_FILLS_STORAGE_OFFSET
                    + core::mem::size_of::<u32>()],
            &identity.max_fills.to_le_bytes()
        );
        assert_eq!(
            &bytes[IDENTITY_OFFSET + IntentIdentityCandidateV0::INTENT_DIGEST_STORAGE_OFFSET
                ..IDENTITY_OFFSET + IntentIdentityCandidateV0::INTENT_DIGEST_STORAGE_OFFSET + 32],
            identity.intent_digest.as_ref()
        );
        assert_eq!(&bytes[PENDING_OFFSET..TERM_OFFSET], &[0; 32]);
        let decoded: StoredAuthorizationCandidateV0 =
            deserialize_account_exact(&bytes, StoredAuthorizationCandidateV0::SPACE).unwrap();
        assert_eq!(decoded.identity, identity);
        assert_eq!(decoded.lifecycle, StoredAuthorizationLifecycle::DRAFT);
        let compact = decode_stored_authorization_compact_exact(&bytes, &core, &key).unwrap();
        assert_eq!(compact.header, draft.header_row().unwrap());
        assert_eq!(compact.identity, identity);
        assert_eq!(compact.immutable_terms.len(), 2);
        assert_eq!(compact.capabilities.len(), 2);
        assert!(compact
            .immutable_terms
            .iter()
            .all(|term| *term == StoredIntentCapabilityTermCandidateV0::default()));
        assert!(deserialize_account_exact::<StoredAuthorizationCandidateV0>(
            &bytes[..bytes.len() - 1],
            StoredAuthorizationCandidateV0::SPACE,
        )
        .is_err());
        assert!(
            decode_stored_authorization_compact_exact(&bytes[..bytes.len() - 1], &core, &key)
                .is_err()
        );
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(deserialize_account_exact::<StoredAuthorizationCandidateV0>(
            &trailing,
            StoredAuthorizationCandidateV0::SPACE,
        )
        .is_err());
        assert!(decode_stored_authorization_compact_exact(&trailing, &core, &key).is_err());
        let mut wrong_discriminator = bytes;
        wrong_discriminator[0] ^= 1;
        assert!(deserialize_account_exact::<StoredAuthorizationCandidateV0>(
            &wrong_discriminator,
            StoredAuthorizationCandidateV0::SPACE,
        )
        .is_err());
        assert!(
            decode_stored_authorization_compact_exact(&wrong_discriminator, &core, &key).is_err()
        );
    }

    #[test]
    fn in_place_codec_matches_host_oracle_through_activation() {
        let (core, actor, identity, terms) = staged_fixture();
        let key = StoredAuthorizationCandidateV0::address(&core, &identity.intent_digest).0;
        let mut lamports = 1_u64;
        let mut data = vec![0_u8; StoredAuthorizationCandidateV0::SPACE];
        let account = AccountInfo::new(&key, false, true, &mut lamports, &mut data, &core, false);

        initialize_stored_authorization_draft_exact(&account, &core, &actor, &identity, 2, 0)
            .unwrap();
        write_stored_authorization_term_chunk_exact(&account, &core, &actor, 0, &terms[..1])
            .unwrap();
        write_stored_authorization_term_chunk_exact(&account, &core, &actor, 1, &terms[1..])
            .unwrap();
        activate_stored_authorization_exact(&account, &core, &actor).unwrap();

        let compact = read_stored_authorization_compact(&account, &core).unwrap();
        assert_eq!(
            compact.lifecycle().unwrap(),
            StoredAuthorizationLifecycle::Active
        );
        assert_eq!(compact.immutable_terms, terms);
        assert_eq!(compact.capabilities.len(), 2);
        assert_eq!(compact.capabilities[0].remaining_total_debit, 100);
        assert_eq!(compact.capabilities[1].initial_minimum_credit, 10);

        let mut oracle =
            StoredAuthorizationCandidateV0::initialize_draft(&core, &actor, identity, 2, 0)
                .unwrap();
        oracle.write_term_chunk(&core, &key, 0, &terms).unwrap();
        oracle.activate(&core, &key).unwrap();
        let mut oracle_bytes = vec![0_u8; StoredAuthorizationCandidateV0::SPACE];
        serialize_account_exact(
            &oracle,
            &mut oracle_bytes,
            StoredAuthorizationCandidateV0::SPACE,
        )
        .unwrap();
        assert_eq!(account.try_borrow_data().unwrap().as_ref(), oracle_bytes);
    }

    #[test]
    fn in_place_replacement_is_same_actor_only() {
        let (core, actor, identity, terms) = staged_fixture();
        let old_key = StoredAuthorizationCandidateV0::address(&core, &identity.intent_digest).0;
        let mut replacement_identity = identity;
        replacement_identity.authorization_nonce += 1;
        replacement_identity.intent_digest =
            replacement_identity.compute_intent_digest(&core).unwrap();
        let new_key =
            StoredAuthorizationCandidateV0::address(&core, &replacement_identity.intent_digest).0;

        let mut old_lamports = 1_u64;
        let mut old_data = vec![0_u8; StoredAuthorizationCandidateV0::SPACE];
        let old_account = AccountInfo::new(
            &old_key,
            false,
            true,
            &mut old_lamports,
            &mut old_data,
            &core,
            false,
        );
        initialize_stored_authorization_draft_exact(&old_account, &core, &actor, &identity, 2, 0)
            .unwrap();
        write_stored_authorization_term_chunk_exact(&old_account, &core, &actor, 0, &terms)
            .unwrap();
        activate_stored_authorization_exact(&old_account, &core, &actor).unwrap();

        let mut new_lamports = 1_u64;
        let mut new_data = vec![0_u8; StoredAuthorizationCandidateV0::SPACE];
        let new_account = AccountInfo::new(
            &new_key,
            false,
            true,
            &mut new_lamports,
            &mut new_data,
            &core,
            false,
        );
        initialize_stored_authorization_draft_exact(
            &new_account,
            &core,
            &actor,
            &replacement_identity,
            2,
            0,
        )
        .unwrap();
        write_stored_authorization_term_chunk_exact(&new_account, &core, &actor, 0, &terms)
            .unwrap();
        replace_stored_authorization_same_actor_exact(&old_account, &new_account, &core, &actor)
            .unwrap();
        assert_eq!(
            read_stored_authorization_compact(&old_account, &core)
                .unwrap()
                .lifecycle()
                .unwrap(),
            StoredAuthorizationLifecycle::Cancelled
        );
        assert_eq!(
            read_stored_authorization_compact(&new_account, &core)
                .unwrap()
                .lifecycle()
                .unwrap(),
            StoredAuthorizationLifecycle::Active
        );

        let other_actor = Pubkey::new_unique();
        let mut foreign_identity = identity;
        foreign_identity.actor = other_actor;
        foreign_identity.authorization_nonce += 2;
        foreign_identity.intent_digest = foreign_identity.compute_intent_digest(&core).unwrap();
        let foreign_key =
            StoredAuthorizationCandidateV0::address(&core, &foreign_identity.intent_digest).0;
        let mut foreign_lamports = 1_u64;
        let mut foreign_data = vec![0_u8; StoredAuthorizationCandidateV0::SPACE];
        let foreign_account = AccountInfo::new(
            &foreign_key,
            false,
            true,
            &mut foreign_lamports,
            &mut foreign_data,
            &core,
            false,
        );
        initialize_stored_authorization_draft_exact(
            &foreign_account,
            &core,
            &other_actor,
            &foreign_identity,
            2,
            0,
        )
        .unwrap();
        write_stored_authorization_term_chunk_exact(
            &foreign_account,
            &core,
            &other_actor,
            0,
            &terms,
        )
        .unwrap();
        assert!(replace_stored_authorization_same_actor_exact(
            &new_account,
            &foreign_account,
            &core,
            &actor
        )
        .is_err());
    }

    #[test]
    fn immutable_release_is_exact_idempotent_and_rejects_poisoning() {
        let (core, key, release) = immutable_release_fixture();
        release.validate(&core, &key).unwrap();
        release.require_exact_existing(&release).unwrap();

        let mut arbitrary_loader = immutable_release_fixture().2;
        arbitrary_loader.loader_program = Pubkey::new_unique();
        assert!(arbitrary_loader.validate(&core, &key).is_err());

        let mut conflicting_capture = immutable_release_fixture().2;
        conflicting_capture.captured_programdata_slot += 1;
        assert!(release
            .require_exact_existing(&conflicting_capture)
            .is_err());
    }

    #[test]
    fn market_and_domain_profile_invariants_are_fail_closed() {
        let core = Pubkey::new_from_array([41; 32]);
        let market_key = Pubkey::new_unique();
        let descriptor_key = Pubkey::new_unique();
        let classic_profile = [21; 32];
        let mut market = market_fixture(&core, &market_key, classic_profile);
        market.binding_row(&core, &market_key).unwrap();

        market.engine_instance_id = [0; 32];
        assert!(market.binding_row(&core, &market_key).is_err());
        market.engine_instance_id = [4; 32];
        market.domain_admission_profile_digest = [0; 32];
        assert!(market.binding_row(&core, &market_key).is_err());
        market.domain_admission_profile_digest = [6; 32];
        market.opaque_schema_digest = [0; 32];
        assert!(market.binding_row(&core, &market_key).is_err());
        market.opaque_schema_digest = [8; 32];
        refresh_market_binding_digest(&mut market, &core, &market_key);

        let mut descriptor = DomainDescriptorAccountCandidateV0 {
            wire_version: WIRE_VERSION_V0,
            rule_kind: generic_effect_private_wire::DOMAIN_RULE_CLOSED,
            reserved: [0; 6],
            controller_program: Pubkey::new_from_array([9; 32]),
            controller_identity: Pubkey::new_from_array([10; 32]),
            domain_revision: 3,
            namespace_or_instance: [0; 32],
            custody_profile_digest: [11; 32],
            asset_profile_digest: [12; 32],
            accounting_profile_digest: [13; 32],
            exit_class_digest: [14; 32],
            admission_rule_digest: [15; 32],
            protected_profile_digest: classic_profile,
        };
        assert_ne!(descriptor.digest(&core).unwrap(), [0; 32]);
        descriptor.custody_profile_digest = [0; 32];
        assert!(descriptor.digest(&core).is_err());
        descriptor.custody_profile_digest = [11; 32];
        descriptor.asset_profile_digest = [0; 32];
        assert!(descriptor.digest(&core).is_err());
        descriptor.asset_profile_digest = [12; 32];
        descriptor.exit_class_digest = [0; 32];
        assert!(descriptor.digest(&core).is_err());
        descriptor.exit_class_digest = [14; 32];

        let engine_instance_policy_digest = market
            .exact_engine_instance_policy_digest(&core, &market_key)
            .unwrap();
        let mut admission = DomainAdmissionAccountCandidateV0 {
            wire_version: WIRE_VERSION_V0,
            reserved: [0; 7],
            domain_descriptor: descriptor_key.to_bytes(),
            domain_revision: descriptor.domain_revision,
            market: market_key.to_bytes(),
            engine_program: market.engine_program.to_bytes(),
            engine_interface_id: market.engine_interface_id,
            engine_instance_policy_digest,
            engine_admission_policy_digest: market.engine_admission_policy_digest,
            settlement_profile_digest: classic_profile,
            admission_rule_digest: descriptor.admission_rule_digest,
            active_from_slot: 1,
            expires_at_slot_or_zero: 100,
            revoked_at_slot_or_zero: 0,
        };
        let admission_key =
            DomainAdmissionAccountCandidateV0::address(&core, &admission.wire_row().unwrap())
                .unwrap()
                .0;
        admission
            .validate_authenticated(
                &core,
                &admission_key,
                &descriptor_key,
                &descriptor,
                &market_key,
                &market,
                &classic_profile,
                50,
            )
            .unwrap();

        admission.settlement_profile_digest = [22; 32];
        let mismatched_key =
            DomainAdmissionAccountCandidateV0::address(&core, &admission.wire_row().unwrap())
                .unwrap()
                .0;
        assert!(admission
            .validate_authenticated(
                &core,
                &mismatched_key,
                &descriptor_key,
                &descriptor,
                &market_key,
                &market,
                &classic_profile,
                50,
            )
            .is_err());
    }

    #[test]
    fn draft_requires_complete_non_overlapping_chunks_before_activation() {
        let (core, actor, identity, terms) = staged_fixture();
        let key = StoredAuthorizationCandidateV0::address(&core, &identity.intent_digest).0;
        let mut draft =
            StoredAuthorizationCandidateV0::initialize_draft(&core, &actor, identity, 2, 0)
                .unwrap();
        draft.write_term_chunk(&core, &key, 1, &terms[1..]).unwrap();
        assert!(draft.activate(&core, &key).is_err());
        assert_eq!(draft.lifecycle, StoredAuthorizationLifecycle::DRAFT);
        assert!(draft.write_term_chunk(&core, &key, 1, &terms[1..]).is_err());
        draft.write_term_chunk(&core, &key, 0, &terms[..1]).unwrap();
        draft.activate(&core, &key).unwrap();
        assert_eq!(
            draft.validate_account(&core, &key).unwrap(),
            StoredAuthorizationLifecycle::Active
        );
        assert_eq!(draft.capabilities[0].remaining_total_debit, 100);
        assert_eq!(draft.capabilities[1].initial_minimum_credit, 10);
        assert_eq!(draft.fee_state_count, 0);
    }

    #[test]
    fn cancelled_partial_draft_is_a_valid_non_reusable_tombstone() {
        let (core, actor, identity, terms) = staged_fixture();
        let key = StoredAuthorizationCandidateV0::address(&core, &identity.intent_digest).0;
        let mut draft =
            StoredAuthorizationCandidateV0::initialize_draft(&core, &actor, identity, 2, 0)
                .unwrap();
        draft.write_term_chunk(&core, &key, 0, &terms[..1]).unwrap();
        draft.cancel(&core, &key).unwrap();
        assert_eq!(
            draft.validate_account(&core, &key).unwrap(),
            StoredAuthorizationLifecycle::Cancelled
        );
        assert!(draft.write_term_chunk(&core, &key, 1, &terms[1..]).is_err());
        assert!(draft.activate(&core, &key).is_err());
    }

    #[test]
    fn stored_debit_coverage_is_exact_xor_and_relations_may_overlap() {
        let debit = |index: u8| IntentCapabilityTermRowCandidateV0 {
            intent_local_term_index: index,
            authority_class: AUTHORITY_INTENT_FUNDED_DEBIT,
            fee_class: FEE_CLASS_GROSS_DEBIT_RATE,
            flags: 0,
            rights_bits: RIGHT_DEBIT,
            endpoint_key: [index.saturating_add(1); 32],
            asset_binding_digest: [9; 32],
            required_domain_descriptor_digest_or_zero: [0; 32],
            maximum_engine_debit: 100,
            maximum_total_debit: 100,
            minimum_credit: 0,
            maximum_protocol_fee: 0,
        };
        let credit = IntentCapabilityTermRowCandidateV0 {
            intent_local_term_index: 2,
            authority_class: AUTHORITY_EXACT_EXTERNAL_CREDIT,
            fee_class: FEE_CLASS_NONE,
            flags: 0,
            rights_bits: RIGHT_CREDIT | RIGHT_EXACT_EXTERNAL_RECIPIENT,
            endpoint_key: [3; 32],
            asset_binding_digest: [9; 32],
            required_domain_descriptor_digest_or_zero: [0; 32],
            maximum_engine_debit: 0,
            maximum_total_debit: 0,
            minimum_credit: 1,
            maximum_protocol_fee: 0,
        };
        let mut terms = vec![debit(0), debit(1), credit];
        let mut capabilities = terms
            .iter()
            .map(|term| AuthorizationCapabilityStateRowCandidateV0 {
                local_term_index: term.intent_local_term_index,
                reserved_0: 0,
                flags: term.flags,
                initial_maximum_engine_debit: term.maximum_engine_debit,
                initial_minimum_credit: term.minimum_credit,
                initial_maximum_total_debit: term.maximum_total_debit,
                remaining_total_debit: term.maximum_total_debit,
                cumulative_engine_debit: 0,
                cumulative_fee_debit: 0,
                cumulative_credit: 0,
            })
            .collect::<Vec<_>>();
        let group_01 =
            generic_effect_private_wire::compute_intent_debit_group_root(&[0, 1]).unwrap();
        let group_0 = generic_effect_private_wire::compute_intent_debit_group_root(&[0]).unwrap();
        let constraints = vec![
            CreditConstraintRowCandidateV0 {
                constraint_index: 0,
                credit_local_term_index: 2,
                flags: 0,
                debit_source_bitmap: 0b11,
                debit_group_root: group_01,
                minimum_credit_numerator: 6,
                nonzero_debit_denominator: 10,
                terminal_absolute_minimum: 0,
            },
            CreditConstraintRowCandidateV0 {
                constraint_index: 1,
                credit_local_term_index: 2,
                flags: 0,
                debit_source_bitmap: 0b01,
                debit_group_root: group_0,
                minimum_credit_numerator: 1,
                nonzero_debit_denominator: 1,
                terminal_absolute_minimum: 0,
            },
        ];
        assert!(
            StoredAuthorizationCandidateV0::validate_immutable_to_mutable_mapping_rows(
                &terms,
                &constraints,
                &capabilities,
                &[],
            )
            .is_ok()
        );

        terms[0].flags = INTENT_CAPABILITY_TERM_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT;
        capabilities[0].flags = INTENT_CAPABILITY_TERM_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT;
        assert!(
            StoredAuthorizationCandidateV0::validate_immutable_to_mutable_mapping_rows(
                &terms,
                &constraints,
                &capabilities,
                &[],
            )
            .is_err()
        );
        terms[0].flags = 0;
        capabilities[0].flags = 0;
        let mut zero_only = constraints.clone();
        zero_only[0].minimum_credit_numerator = 0;
        zero_only[1].minimum_credit_numerator = 0;
        assert!(
            StoredAuthorizationCandidateV0::validate_immutable_to_mutable_mapping_rows(
                &terms,
                &zero_only,
                &capabilities,
                &[],
            )
            .is_err()
        );
    }
}
