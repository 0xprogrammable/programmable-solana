//! Product-neutral protected and opaque capability closure.

use anchor_lang::prelude::*;
use generic_effect_private_wire::{
    compute_opaque_capability_root, compute_protected_capability_set_digest,
    IntentCapabilityTermRowCandidateV0, OpaqueCapabilityDescriptorCandidateV0,
    ProtectedCapabilityDigestRowCandidateV0, SettlementCapabilityRowCandidateV0,
    SETTLEMENT_FLAG_FEE_FUNDING,
};

use crate::{
    account_segments::EffectivePrivilege,
    constants::{
        ABSENT_INDEX, AUTHORITY_CORE_RESERVED_FEE_CREDIT, AUTHORITY_DOMAIN_ACCOUNTED,
        AUTHORITY_EXACT_EXTERNAL_CREDIT, AUTHORITY_INTENT_FUNDED_DEBIT, FEE_CLASS_GROSS_DEBIT_RATE,
        FEE_CLASS_NONE, KNOWN_SETTLEMENT_RIGHTS, MAX_OPAQUE_CAPABILITIES,
        MAX_SETTLEMENT_CAPABILITIES, RIGHT_CORE_RESERVED_FEE, RIGHT_CREDIT, RIGHT_DEBIT,
        RIGHT_DOMAIN_ACCOUNTED, RIGHT_EXACT_EXTERNAL_RECIPIENT,
    },
    error::CoreError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssetProfileIdentity {
    pub asset_identity: Pubkey,
    pub asset_program: Pubkey,
    pub settlement_profile_digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DomainCapabilityIdentity {
    pub domain_index: u8,
    pub domain_descriptor: Pubkey,
    pub domain_revision: u64,
    pub admission_digest: [u8; 32],
    /// Exact local accounting slot for AUTHORITY_DOMAIN_ACCOUNTED; ABSENT_INDEX
    /// when this relation is only an intent's authenticated domain predicate.
    pub accounting_slot: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettlementCapability {
    pub position: u8,
    /// Canonical declaration decoded by the private wire crate. Core never
    /// reconstructs this security-critical row from a semantic subset.
    pub declaration: SettlementCapabilityRowCandidateV0,
    pub core_program: Pubkey,
    pub experimental_major: u32,
    pub market: Pubkey,
    pub endpoint: EffectivePrivilege,
    pub transfer_authority_or_zero: Pubkey,
    pub asset: AssetProfileIdentity,
    /// Authenticated domain relation. For intent-funded debit and exact credit
    /// this is a predicate only and carries accounting_slot = ABSENT_INDEX.
    pub domain: Option<DomainCapabilityIdentity>,
    pub fee_policy_revision: u64,
    pub lifecycle_digest: [u8; 32],
    /// Exact authenticated accounting prestate for domain capabilities; zero
    /// for every non-domain capability.
    pub accounted_before_or_zero: u128,
}

impl SettlementCapability {
    pub fn has_right(self, right: u16) -> bool {
        self.declaration.rights_bits & right != 0
    }

    pub fn is_engine_fee_reserved(self) -> bool {
        self.has_right(RIGHT_CORE_RESERVED_FEE)
            || self.declaration.authority_class == AUTHORITY_CORE_RESERVED_FEE_CREDIT
    }

    pub fn is_fee_funding(self) -> bool {
        self.declaration.flags & SETTLEMENT_FLAG_FEE_FUNDING != 0
    }

    pub fn authorization_slot(self) -> Option<u8> {
        (self.declaration.authorization_slot_or_none != ABSENT_INDEX)
            .then_some(self.declaration.authorization_slot_or_none)
    }

    pub fn fee_shard_index(self) -> Option<u8> {
        (self.declaration.fee_shard_index_or_none != ABSENT_INDEX)
            .then_some(self.declaration.fee_shard_index_or_none)
    }

    pub fn protected_digest_row(self) -> ProtectedCapabilityDigestRowCandidateV0 {
        let (domain_descriptor_or_zero, domain_admission_digest_or_zero, domain_revision) = self
            .domain
            .map(|domain| {
                (
                    domain.domain_descriptor.to_bytes(),
                    domain.admission_digest,
                    domain.domain_revision,
                )
            })
            .unwrap_or(([0; 32], [0; 32], 0));
        ProtectedCapabilityDigestRowCandidateV0 {
            capability_position: self.position,
            asset_index: self.declaration.asset_index,
            domain_index_or_none: self.declaration.domain_index_or_none,
            authorization_slot_or_none: self.declaration.authorization_slot_or_none,
            authority_class: self.declaration.authority_class,
            fee_class: self.declaration.fee_class,
            fee_shard_index_or_none: self.declaration.fee_shard_index_or_none,
            flags: self.declaration.flags,
            rights_bits: self.declaration.rights_bits,
            domain_accounting_slot_or_none: self.declaration.domain_accounting_slot_or_none,
            spend_control_offset_or_none: self.declaration.spend_authority_control_offset_or_none,
            endpoint_executable: self.endpoint.executable,
            effective_signer: self.endpoint.signer,
            effective_writable: self.endpoint.writable,
            endpoint_key: self.endpoint.key.to_bytes(),
            endpoint_owner: self.endpoint.owner.to_bytes(),
            transfer_authority_key_or_zero: self.transfer_authority_or_zero.to_bytes(),
            asset_identity: self.asset.asset_identity.to_bytes(),
            asset_program: self.asset.asset_program.to_bytes(),
            settlement_profile_digest: self.asset.settlement_profile_digest,
            domain_descriptor_or_zero,
            domain_admission_digest_or_zero,
            lifecycle_digest: self.lifecycle_digest,
            domain_revision,
            maximum_engine_debit: self.declaration.maximum_engine_debit,
            maximum_total_debit: self.declaration.maximum_total_debit,
            minimum_credit: self.declaration.minimum_credit,
            maximum_protocol_fee: self.declaration.maximum_protocol_fee,
            fee_policy_revision: self.fee_policy_revision,
            accounted_before_or_zero: self.accounted_before_or_zero,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityValidationContext {
    pub core_program: Pubkey,
    pub market: Pubkey,
    pub classic_token_program: Pubkey,
    pub experimental_major: u32,
    pub intent_count: u8,
    pub asset_count: u8,
    pub domain_count: u8,
    pub fee_shard_count: u8,
    pub fee_policy_revision: u64,
}

pub fn validate_settlement_capabilities(
    capabilities: &[SettlementCapability],
    context: CapabilityValidationContext,
) -> Result<[u8; 32]> {
    require!(
        capabilities.len() <= MAX_SETTLEMENT_CAPABILITIES,
        CoreError::ExperimentLimitExceeded
    );
    for (position, capability) in capabilities.iter().enumerate() {
        require_eq!(
            usize::from(capability.position),
            position,
            CoreError::NonCanonicalSettlementCapabilityPosition
        );
        require_keys_eq!(
            capability.core_program,
            context.core_program,
            CoreError::InvalidSettlementRights
        );
        require_keys_eq!(
            capability.market,
            context.market,
            CoreError::InvalidSettlementRights
        );
        require_eq!(
            capability.experimental_major,
            context.experimental_major,
            CoreError::InvalidSettlementRights
        );
        require_keys_eq!(
            capability.endpoint.owner,
            context.classic_token_program,
            CoreError::MoveAssetProfileMismatch
        );
        require_keys_eq!(
            capability.asset.asset_program,
            context.classic_token_program,
            CoreError::MoveAssetProfileMismatch
        );
        require!(
            !capability.endpoint.executable
                && !capability.endpoint.signer
                && capability.endpoint.writable
                && capability.lifecycle_digest != [0; 32],
            CoreError::InvalidSettlementRights
        );
        capability
            .declaration
            .validate_indices(
                context.asset_count,
                context.domain_count,
                context.intent_count,
                context.fee_shard_count,
            )
            .map_err(|_| error!(CoreError::InvalidSettlementRights))?;
        require!(
            capability.declaration.rights_bits != 0
                && capability.declaration.rights_bits & !KNOWN_SETTLEMENT_RIGHTS == 0,
            CoreError::InvalidSettlementRights
        );
        require_eq!(
            capability.fee_policy_revision,
            context.fee_policy_revision,
            CoreError::InvalidSettlementRights
        );
        require!(
            capabilities[..position]
                .iter()
                .all(|earlier| earlier.endpoint.key != capability.endpoint.key),
            CoreError::DuplicateSettlementCapability
        );
        match capability.domain {
            Some(domain) => {
                require_eq!(
                    capability.declaration.domain_index_or_none,
                    domain.domain_index,
                    CoreError::InvalidSettlementDomain
                );
                require!(
                    domain.domain_index < context.domain_count,
                    CoreError::InvalidSettlementDomain
                );
                if capability.declaration.authority_class == AUTHORITY_DOMAIN_ACCOUNTED {
                    require!(
                        domain.accounting_slot != ABSENT_INDEX
                            && capability.declaration.domain_accounting_slot_or_none
                                == domain.accounting_slot,
                        CoreError::InvalidSettlementDomain
                    );
                } else {
                    require!(
                        domain.accounting_slot == ABSENT_INDEX
                            && capability.declaration.domain_accounting_slot_or_none
                                == ABSENT_INDEX
                            && capability.accounted_before_or_zero == 0,
                        CoreError::InvalidSettlementDomain
                    );
                }
            }
            None => {
                require_eq!(
                    capability.declaration.domain_index_or_none,
                    ABSENT_INDEX,
                    CoreError::InvalidSettlementDomain
                );
                require_eq!(
                    capability.declaration.domain_accounting_slot_or_none,
                    ABSENT_INDEX,
                    CoreError::InvalidSettlementDomain
                );
                require_eq!(
                    capability.accounted_before_or_zero,
                    0,
                    CoreError::InvalidSettlementDomain
                );
            }
        }
        validate_authority_shape(*capability, context)?;
    }

    let rows = capabilities
        .iter()
        .copied()
        .map(SettlementCapability::protected_digest_row)
        .collect::<Vec<_>>();
    compute_protected_capability_set_digest(&rows)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))
}

/// Proves that an execution-local settlement declaration is the exact signed
/// local term. This must be used for stored witnesses instead of rebuilding a
/// fresh term from caller-selected execution bytes.
pub fn validate_intent_term_binding(
    capability: &SettlementCapability,
    asset_binding_digest: &[u8; 32],
    required_domain_descriptor_digest_or_zero: &[u8; 32],
    term: &IntentCapabilityTermRowCandidateV0,
) -> Result<()> {
    let row = capability.declaration;
    require!(
        term.intent_local_term_index == row.intent_local_term_index_or_none
            && term.authority_class == row.authority_class
            && term.fee_class == row.fee_class
            && term.flags == row.flags
            && term.rights_bits == row.rights_bits
            && term.endpoint_key == capability.endpoint.key.to_bytes()
            && term.asset_binding_digest == *asset_binding_digest
            && term.required_domain_descriptor_digest_or_zero
                == *required_domain_descriptor_digest_or_zero
            && term.maximum_engine_debit == row.maximum_engine_debit
            && term.maximum_total_debit == row.maximum_total_debit
            && term.minimum_credit == row.minimum_credit
            && term.maximum_protocol_fee == row.maximum_protocol_fee,
        CoreError::AuthorizationIdentityMismatch
    );
    term.encode()
        .map_err(|_| error!(CoreError::AuthorizationIdentityMismatch))?;
    Ok(())
}

/// A nonzero signed domain requirement must resolve to exactly one authenticated
/// descriptor in the current execution. Duplicated authenticated descriptors
/// are rejected even if they would otherwise satisfy the same term.
pub fn validate_required_domain_descriptor_bindings(
    terms: &[IntentCapabilityTermRowCandidateV0],
    authenticated_domain_descriptor_digests: &[[u8; 32]],
) -> Result<()> {
    for (position, digest) in authenticated_domain_descriptor_digests.iter().enumerate() {
        require!(*digest != [0; 32], CoreError::InvalidSettlementDomain);
        require!(
            authenticated_domain_descriptor_digests[..position]
                .iter()
                .all(|earlier| earlier != digest),
            CoreError::InvalidSettlementDomain
        );
    }
    for term in terms {
        term.encode()
            .map_err(|_| error!(CoreError::AuthorizationIdentityMismatch))?;
        let required = term.required_domain_descriptor_digest_or_zero;
        if required == [0; 32] {
            continue;
        }
        require!(
            authenticated_domain_descriptor_digests
                .iter()
                .filter(|candidate| **candidate == required)
                .count()
                == 1,
            CoreError::InvalidSettlementDomain
        );
    }
    Ok(())
}

fn validate_authority_shape(
    capability: SettlementCapability,
    context: CapabilityValidationContext,
) -> Result<()> {
    let row = capability.declaration;
    let fee_funding = capability.is_fee_funding();
    match row.authority_class {
        AUTHORITY_INTENT_FUNDED_DEBIT => {
            require_eq!(
                row.rights_bits,
                RIGHT_DEBIT,
                CoreError::InvalidSettlementRights
            );
            require!(
                capability
                    .authorization_slot()
                    .is_some_and(|slot| slot < context.intent_count)
                    && row.intent_local_term_index_or_none != ABSENT_INDEX,
                CoreError::InvalidSettlementAuthorization
            );
            require!(
                (fee_funding
                    && capability
                        .fee_shard_index()
                        .is_some_and(|index| index < context.fee_shard_count)
                    && row.maximum_protocol_fee > 0
                    && row.fee_class == FEE_CLASS_GROSS_DEBIT_RATE)
                    || (!fee_funding
                        && capability.fee_shard_index().is_none()
                        && row.maximum_protocol_fee == 0
                        && row.fee_class == FEE_CLASS_GROSS_DEBIT_RATE
                        && row.maximum_total_debit == row.maximum_engine_debit),
                CoreError::InvalidSettlementFeeShard
            );
            require!(
                row.maximum_engine_debit > 0
                    && row.maximum_total_debit >= row.maximum_engine_debit
                    && row.minimum_credit == 0
                    && capability.transfer_authority_or_zero != Pubkey::default(),
                CoreError::InvalidSettlementRights
            );
        }
        AUTHORITY_DOMAIN_ACCOUNTED => {
            let directional = row.rights_bits & (RIGHT_DEBIT | RIGHT_CREDIT);
            require!(
                (directional == RIGHT_DEBIT || directional == RIGHT_CREDIT)
                    && row.rights_bits == directional | RIGHT_DOMAIN_ACCOUNTED,
                CoreError::InvalidSettlementRights
            );
            require!(
                capability
                    .domain
                    .is_some_and(|domain| domain.domain_index < context.domain_count),
                CoreError::InvalidSettlementDomain
            );
            require!(
                capability.authorization_slot().is_none()
                    && row.intent_local_term_index_or_none == ABSENT_INDEX
                    && row.spend_authority_control_offset_or_none == ABSENT_INDEX,
                CoreError::InvalidSettlementAuthorization
            );
            require!(
                capability.fee_shard_index().is_none() && !fee_funding,
                CoreError::InvalidSettlementFeeShard
            );
            require_eq!(row.flags, 0, CoreError::InvalidSettlementRights);
            require_eq!(
                row.fee_class,
                FEE_CLASS_NONE,
                CoreError::InvalidSettlementRights
            );
            require_eq!(
                row.maximum_total_debit,
                row.maximum_engine_debit,
                CoreError::InvalidSettlementRights
            );
            require_eq!(
                row.maximum_protocol_fee,
                0,
                CoreError::InvalidSettlementRights
            );
            if directional == RIGHT_DEBIT {
                require!(
                    row.maximum_engine_debit > 0
                        && capability.transfer_authority_or_zero != Pubkey::default(),
                    CoreError::InvalidSettlementRights
                );
                require_eq!(row.minimum_credit, 0, CoreError::InvalidSettlementRights);
            } else {
                require_eq!(
                    row.maximum_engine_debit,
                    0,
                    CoreError::InvalidSettlementRights
                );
                require_keys_eq!(
                    capability.transfer_authority_or_zero,
                    Pubkey::default(),
                    CoreError::InvalidSettlementRights
                );
            }
        }
        AUTHORITY_EXACT_EXTERNAL_CREDIT => {
            require_eq!(
                row.rights_bits,
                RIGHT_CREDIT | RIGHT_EXACT_EXTERNAL_RECIPIENT,
                CoreError::InvalidSettlementRights
            );
            require!(
                capability
                    .authorization_slot()
                    .is_some_and(|slot| slot < context.intent_count)
                    && row.intent_local_term_index_or_none != ABSENT_INDEX
                    && row.spend_authority_control_offset_or_none == ABSENT_INDEX,
                CoreError::InvalidSettlementAuthorization
            );
            require!(
                capability.fee_shard_index().is_none() && !fee_funding,
                CoreError::InvalidSettlementFeeShard
            );
            require_eq!(row.flags, 0, CoreError::InvalidSettlementRights);
            require_eq!(
                row.maximum_engine_debit,
                0,
                CoreError::InvalidSettlementRights
            );
            require_keys_eq!(
                capability.transfer_authority_or_zero,
                Pubkey::default(),
                CoreError::InvalidSettlementRights
            );
            require_eq!(
                row.maximum_total_debit,
                0,
                CoreError::InvalidSettlementRights
            );
            require_eq!(
                row.maximum_protocol_fee,
                0,
                CoreError::InvalidSettlementRights
            );
            require_eq!(
                row.fee_class,
                FEE_CLASS_NONE,
                CoreError::InvalidSettlementRights
            );
        }
        AUTHORITY_CORE_RESERVED_FEE_CREDIT => {
            require_eq!(
                row.rights_bits,
                RIGHT_CREDIT | RIGHT_CORE_RESERVED_FEE,
                CoreError::InvalidSettlementRights
            );
            require!(
                capability.domain.is_none(),
                CoreError::InvalidSettlementDomain
            );
            require!(
                capability.authorization_slot().is_none()
                    && row.intent_local_term_index_or_none == ABSENT_INDEX
                    && row.spend_authority_control_offset_or_none == ABSENT_INDEX,
                CoreError::InvalidSettlementAuthorization
            );
            require!(
                capability
                    .fee_shard_index()
                    .is_some_and(|index| index < context.fee_shard_count),
                CoreError::InvalidSettlementFeeShard
            );
            require_eq!(
                row.maximum_engine_debit,
                0,
                CoreError::InvalidSettlementRights
            );
            require_keys_eq!(
                capability.transfer_authority_or_zero,
                Pubkey::default(),
                CoreError::InvalidSettlementRights
            );
            require_eq!(
                row.maximum_total_debit,
                0,
                CoreError::InvalidSettlementRights
            );
            require_eq!(row.minimum_credit, 0, CoreError::InvalidSettlementRights);
            require_eq!(
                row.maximum_protocol_fee,
                0,
                CoreError::InvalidSettlementRights
            );
            require_eq!(
                row.fee_class,
                FEE_CLASS_NONE,
                CoreError::InvalidSettlementRights
            );
            require!(!fee_funding, CoreError::InvalidSettlementRights);
            require_eq!(row.flags, 0, CoreError::InvalidSettlementRights);
        }
        _ => return err!(CoreError::UnknownAuthorityClass),
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpaqueCapability {
    pub position: u8,
    pub privilege: EffectivePrivilege,
}

impl OpaqueCapability {
    pub fn digest_row(self) -> [u8; 68] {
        OpaqueCapabilityDescriptorCandidateV0 {
            position: self.position,
            key: self.privilege.key.to_bytes(),
            owner: self.privilege.owner.to_bytes(),
            executable: self.privilege.executable,
            effective_signer: self.privilege.signer,
            effective_writable: self.privilege.writable,
        }
        .encode()
    }
}

pub fn validate_opaque_capabilities(
    opaque_privileges: &[EffectivePrivilege],
    protected_keys: &[Pubkey],
    core_program: &Pubkey,
    classic_token_program: &Pubkey,
    token_2022_program: &Pubkey,
) -> Result<(Vec<OpaqueCapability>, [u8; 32])> {
    require!(
        opaque_privileges.len() <= MAX_OPAQUE_CAPABILITIES,
        CoreError::ExperimentLimitExceeded
    );
    let mut capabilities = Vec::with_capacity(opaque_privileges.len());
    for (position, privilege) in opaque_privileges.iter().copied().enumerate() {
        require!(!privilege.signer, CoreError::OpaqueSignerForbidden);
        require!(
            protected_keys.iter().all(|key| *key != privilege.key),
            CoreError::OpaqueProtectedAlias
        );
        require_keys_neq!(
            privilege.owner,
            *core_program,
            CoreError::OpaqueCoreOwnedAccount
        );
        require!(
            !(privilege.executable && privilege.writable),
            CoreError::OpaqueExecutableWritable
        );
        require!(
            !(privilege.writable
                && (privilege.owner == *classic_token_program
                    || privilege.owner == *token_2022_program)),
            CoreError::OpaqueProtectedTokenAccountWritable
        );
        capabilities.push(OpaqueCapability {
            position: u8::try_from(position).map_err(|_| CoreError::ExperimentLimitExceeded)?,
            privilege,
        });
    }

    let descriptors = capabilities
        .iter()
        .copied()
        .map(|capability| OpaqueCapabilityDescriptorCandidateV0 {
            position: capability.position,
            key: capability.privilege.key.to_bytes(),
            owner: capability.privilege.owner.to_bytes(),
            executable: capability.privilege.executable,
            effective_signer: capability.privilege.signer,
            effective_writable: capability.privilege.writable,
        })
        .collect::<Vec<_>>();
    let root = compute_opaque_capability_root(&descriptors)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    Ok((capabilities, root))
}

pub fn validate_plane_disjointness(
    settlement: &[SettlementCapability],
    opaque: &[OpaqueCapability],
) -> Result<()> {
    require!(
        settlement.iter().all(|protected| opaque
            .iter()
            .all(|untrusted| protected.endpoint.key != untrusted.privilege.key)),
        CoreError::CapabilityPlaneAlias
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn predicate_capability(
        authority_class: u8,
    ) -> (SettlementCapability, CapabilityValidationContext) {
        let core = Pubkey::new_unique();
        let market = Pubkey::new_unique();
        let token = Pubkey::new_unique();
        let is_debit = authority_class == AUTHORITY_INTENT_FUNDED_DEBIT;
        let declaration = SettlementCapabilityRowCandidateV0 {
            asset_index: 0,
            domain_index_or_none: 0,
            authorization_slot_or_none: 0,
            intent_local_term_index_or_none: 0,
            authority_class,
            fee_shard_index_or_none: ABSENT_INDEX,
            fee_class: if is_debit {
                FEE_CLASS_GROSS_DEBIT_RATE
            } else {
                FEE_CLASS_NONE
            },
            flags: 0,
            rights_bits: if is_debit {
                RIGHT_DEBIT
            } else {
                RIGHT_CREDIT | RIGHT_EXACT_EXTERNAL_RECIPIENT
            },
            domain_accounting_slot_or_none: ABSENT_INDEX,
            spend_authority_control_offset_or_none: ABSENT_INDEX,
            reserved_0: 0,
            maximum_engine_debit: u64::from(is_debit),
            maximum_total_debit: u64::from(is_debit),
            minimum_credit: u64::from(!is_debit),
            maximum_protocol_fee: 0,
        };
        (
            SettlementCapability {
                position: 0,
                declaration,
                core_program: core,
                experimental_major: 1,
                market,
                endpoint: EffectivePrivilege {
                    key: Pubkey::new_unique(),
                    owner: token,
                    executable: false,
                    signer: false,
                    writable: true,
                },
                transfer_authority_or_zero: if is_debit {
                    Pubkey::new_unique()
                } else {
                    Pubkey::default()
                },
                asset: AssetProfileIdentity {
                    asset_identity: Pubkey::new_unique(),
                    asset_program: token,
                    settlement_profile_digest: [2; 32],
                },
                domain: Some(DomainCapabilityIdentity {
                    domain_index: 0,
                    domain_descriptor: Pubkey::new_unique(),
                    domain_revision: 1,
                    admission_digest: [3; 32],
                    accounting_slot: ABSENT_INDEX,
                }),
                fee_policy_revision: 9,
                lifecycle_digest: [4; 32],
                accounted_before_or_zero: 0,
            },
            CapabilityValidationContext {
                core_program: core,
                market,
                classic_token_program: token,
                experimental_major: 1,
                intent_count: 1,
                asset_count: 1,
                domain_count: 1,
                fee_shard_count: 0,
                fee_policy_revision: 9,
            },
        )
    }

    #[test]
    fn opaque_root_preserves_multiplicity_and_effective_privilege() {
        let core = Pubkey::new_unique();
        let token = Pubkey::new_unique();
        let token_2022 = Pubkey::new_unique();
        let opaque_key = Pubkey::new_unique();
        let base = EffectivePrivilege {
            key: opaque_key,
            owner: Pubkey::new_unique(),
            executable: false,
            signer: false,
            writable: false,
        };
        let (_, one) =
            validate_opaque_capabilities(&[base], &[], &core, &token, &token_2022).unwrap();
        let (_, two) =
            validate_opaque_capabilities(&[base, base], &[], &core, &token, &token_2022).unwrap();
        assert_ne!(one, two);
        assert!(validate_opaque_capabilities(
            &[EffectivePrivilege {
                signer: true,
                ..base
            }],
            &[],
            &core,
            &token,
            &token_2022,
        )
        .is_err());
        assert!(
            validate_opaque_capabilities(&[base], &[opaque_key], &core, &token, &token_2022,)
                .is_err()
        );
    }

    #[test]
    fn signed_domain_requirement_rejects_substitution_and_duplicates() {
        let required = [7; 32];
        let term = IntentCapabilityTermRowCandidateV0 {
            intent_local_term_index: 0,
            authority_class: AUTHORITY_EXACT_EXTERNAL_CREDIT,
            fee_class: FEE_CLASS_NONE,
            flags: 0,
            rights_bits: RIGHT_CREDIT | RIGHT_EXACT_EXTERNAL_RECIPIENT,
            endpoint_key: [1; 32],
            asset_binding_digest: [2; 32],
            required_domain_descriptor_digest_or_zero: required,
            maximum_engine_debit: 0,
            maximum_total_debit: 0,
            minimum_credit: 1,
            maximum_protocol_fee: 0,
        };
        assert!(validate_required_domain_descriptor_bindings(&[term], &[required]).is_ok());
        assert!(validate_required_domain_descriptor_bindings(&[term], &[[8; 32]]).is_err());
        assert!(
            validate_required_domain_descriptor_bindings(&[term], &[required, required]).is_err()
        );

        let unrestricted = IntentCapabilityTermRowCandidateV0 {
            required_domain_descriptor_digest_or_zero: [0; 32],
            ..term
        };
        assert!(validate_required_domain_descriptor_bindings(&[unrestricted], &[]).is_ok());
    }

    #[test]
    fn intent_rows_may_bind_domain_predicate_without_accounting_authority() {
        for authority in [
            AUTHORITY_INTENT_FUNDED_DEBIT,
            AUTHORITY_EXACT_EXTERNAL_CREDIT,
        ] {
            let (capability, context) = predicate_capability(authority);
            assert!(validate_settlement_capabilities(&[capability], context).is_ok());

            let mut forged_accounting = capability;
            forged_accounting.domain.as_mut().unwrap().accounting_slot = 0;
            forged_accounting.declaration.domain_accounting_slot_or_none = 0;
            assert!(validate_settlement_capabilities(&[forged_accounting], context).is_err());
        }
    }
}
