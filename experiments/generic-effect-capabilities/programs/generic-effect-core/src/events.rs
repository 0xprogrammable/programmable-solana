//! Product-neutral evidence classes emitted by the private experiment.

use anchor_lang::prelude::*;
use generic_effect_private_wire::{
    compute_core_verified_evidence_digest, compute_engine_attested_evidence_digest,
    CoreVerifiedEvidenceDigestInputs, EngineAttestedEvidenceDigestInputs,
};

use crate::{
    constants::{EVIDENCE_CORE_VERIFIED, EVIDENCE_ENGINE_ATTESTED},
    error::CoreError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvidenceClass {
    CoreVerified,
    EngineAttested,
}

impl EvidenceClass {
    pub fn decode(value: u8) -> Result<Self> {
        match value {
            EVIDENCE_CORE_VERIFIED => Ok(Self::CoreVerified),
            EVIDENCE_ENGINE_ATTESTED => Ok(Self::EngineAttested),
            _ => err!(CoreError::UnsupportedEvidenceClass),
        }
    }

    pub fn encode(self) -> u8 {
        match self {
            Self::CoreVerified => EVIDENCE_CORE_VERIFIED,
            Self::EngineAttested => EVIDENCE_ENGINE_ATTESTED,
        }
    }
}

/// Evidence whose complete digest is derived from facts independently checked
/// by Core. Routing and display counts are event metadata and never silently
/// enter the security preimage.
#[event]
pub struct CoreVerifiedEvidenceCandidateV0 {
    pub evidence_class: u8,
    pub routed: bool,
    pub move_count: u8,
    pub intent_count: u8,
    pub domain_count: u8,
    pub reserved: [u8; 3],
    pub core_program: Pubkey,
    pub market_binding_digest: [u8; 32],
    pub loader_state_snapshot_digest: [u8; 32],
    pub intent_set_digest: [u8; 32],
    pub domain_set_digest: [u8; 32],
    pub protected_execution_root: [u8; 32],
    pub opaque_capability_root: [u8; 32],
    pub request_digest: [u8; 32],
    pub effect_digest: [u8; 32],
    pub fee_assessment_set_root: [u8; 32],
    pub observed_delta_root: [u8; 32],
    pub evidence_digest: [u8; 32],
}

impl CoreVerifiedEvidenceCandidateV0 {
    pub fn derive_digest(&self) -> Result<[u8; 32]> {
        require_eq!(
            self.evidence_class,
            EVIDENCE_CORE_VERIFIED,
            CoreError::UnsupportedEvidenceClass
        );
        require!(
            self.reserved.iter().all(|byte| *byte == 0),
            CoreError::InvalidWireEncoding
        );
        compute_core_verified_evidence_digest(CoreVerifiedEvidenceDigestInputs {
            core_program: &self.core_program.to_bytes(),
            market_binding_digest: &self.market_binding_digest,
            loader_state_snapshot_digest: &self.loader_state_snapshot_digest,
            intent_set_digest: &self.intent_set_digest,
            domain_set_digest: &self.domain_set_digest,
            protected_execution_root: &self.protected_execution_root,
            opaque_capability_root: &self.opaque_capability_root,
            request_digest: &self.request_digest,
            effect_digest: &self.effect_digest,
            fee_assessment_set_root: &self.fee_assessment_set_root,
            observed_delta_root: &self.observed_delta_root,
        })
        .map_err(|_| error!(CoreError::InvalidWireEncoding))
    }

    pub fn validate(&self) -> Result<()> {
        require_keys_eq!(self.core_program, crate::ID, CoreError::InvalidWireEncoding);
        require!(
            self.evidence_digest == self.derive_digest()?,
            CoreError::InvalidWireEncoding
        );
        Ok(())
    }
}

/// Evidence supplied by the selected engine. This class is deliberately not
/// presented as Core-verified economics.
#[event]
pub struct EngineAttestedEvidenceCandidateV0 {
    pub evidence_class: u8,
    pub reserved: [u8; 7],
    pub engine_program: Pubkey,
    pub engine_interface_id: [u8; 32],
    pub engine_instance_id: [u8; 32],
    pub request_digest: [u8; 32],
    pub engine_supplied_digest: [u8; 32],
    pub evidence_digest: [u8; 32],
}

impl EngineAttestedEvidenceCandidateV0 {
    pub fn derive_digest(&self) -> Result<[u8; 32]> {
        require_eq!(
            self.evidence_class,
            EVIDENCE_ENGINE_ATTESTED,
            CoreError::UnsupportedEvidenceClass
        );
        require!(
            self.reserved.iter().all(|byte| *byte == 0),
            CoreError::InvalidWireEncoding
        );
        compute_engine_attested_evidence_digest(EngineAttestedEvidenceDigestInputs {
            engine_program: &self.engine_program.to_bytes(),
            engine_interface_id: &self.engine_interface_id,
            engine_instance_id: &self.engine_instance_id,
            request_digest: &self.request_digest,
            engine_supplied_digest: &self.engine_supplied_digest,
        })
        .map_err(|_| error!(CoreError::InvalidWireEncoding))
    }

    pub fn validate(&self) -> Result<()> {
        require!(
            self.evidence_digest == self.derive_digest()?,
            CoreError::InvalidWireEncoding
        );
        Ok(())
    }
}
