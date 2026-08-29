use alloc::vec::Vec;

use crate::codec::{put_bytes, put_u64, put_u8, require_exact_length, require_zero, Reader};
use crate::hashes::{
    hash_private, LABEL_DOMAIN_ADMISSION_ADDRESS, LABEL_DOMAIN_ADMISSION_RECORD,
    LABEL_EXACT_ENGINE_INSTANCE_POLICY, LABEL_OPEN_DOMAIN_ADMISSION, LABEL_OPEN_DOMAIN_RULE,
};
use crate::{WireError, WireResult, CORE_EXPERIMENTAL_MAJOR, WIRE_VERSION};

pub const DOMAIN_ADMISSION_LEN: usize = 296;
pub const DOMAIN_RULE_OPEN: u8 = 0;
pub const DOMAIN_RULE_CLOSED: u8 = 1;
pub const ADMISSION_OPEN: u8 = 0;
pub const ADMISSION_CLOSED: u8 = 1;

pub fn compute_open_domain_rule_digest() -> WireResult<[u8; 32]> {
    hash_private(LABEL_OPEN_DOMAIN_RULE, &[])
}

pub fn compute_open_domain_admission_digest(
    domain_descriptor_digest: &[u8; 32],
    market_binding_digest: &[u8; 32],
) -> WireResult<[u8; 32]> {
    hash_private(
        LABEL_OPEN_DOMAIN_ADMISSION,
        &[domain_descriptor_digest, market_binding_digest],
    )
}

pub fn compute_exact_engine_instance_policy_digest(
    core_program: &[u8; 32],
    engine_program: &[u8; 32],
    engine_interface_id: &[u8; 32],
    engine_instance_id: &[u8; 32],
) -> WireResult<[u8; 32]> {
    let major = CORE_EXPERIMENTAL_MAJOR.to_le_bytes();
    hash_private(
        LABEL_EXACT_ENGINE_INSTANCE_POLICY,
        &[
            core_program,
            &major,
            engine_program,
            engine_interface_id,
            engine_instance_id,
        ],
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DomainAdmissionCandidateV0 {
    pub wire_version: u8,
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

impl DomainAdmissionCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; DOMAIN_ADMISSION_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(DOMAIN_ADMISSION_LEN);
        put_u8(&mut output, self.wire_version);
        put_bytes(&mut output, &[0; 7]);
        put_bytes(&mut output, &self.domain_descriptor);
        put_u64(&mut output, self.domain_revision);
        put_bytes(&mut output, &self.market);
        put_bytes(&mut output, &self.engine_program);
        put_bytes(&mut output, &self.engine_interface_id);
        put_bytes(&mut output, &self.engine_instance_policy_digest);
        put_bytes(&mut output, &self.engine_admission_policy_digest);
        put_bytes(&mut output, &self.settlement_profile_digest);
        put_bytes(&mut output, &self.admission_rule_digest);
        put_u64(&mut output, self.active_from_slot);
        put_u64(&mut output, self.expires_at_slot_or_zero);
        put_u64(&mut output, self.revoked_at_slot_or_zero);
        Ok(output
            .try_into()
            .expect("domain admission has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, DOMAIN_ADMISSION_LEN)?;
        let mut reader = Reader::new(data);
        let wire_version = reader.read_u8()?;
        let reserved = reader.read_array::<7>()?;
        require_zero("domain admission reserved", &reserved)?;
        let admission = Self {
            wire_version,
            domain_descriptor: reader.read_array()?,
            domain_revision: reader.read_u64()?,
            market: reader.read_array()?,
            engine_program: reader.read_array()?,
            engine_interface_id: reader.read_array()?,
            engine_instance_policy_digest: reader.read_array()?,
            engine_admission_policy_digest: reader.read_array()?,
            settlement_profile_digest: reader.read_array()?,
            admission_rule_digest: reader.read_array()?,
            active_from_slot: reader.read_u64()?,
            expires_at_slot_or_zero: reader.read_u64()?,
            revoked_at_slot_or_zero: reader.read_u64()?,
        };
        reader.finish()?;
        admission.validate()?;
        Ok(admission)
    }

    pub fn validate(&self) -> WireResult<()> {
        if self.wire_version != WIRE_VERSION {
            return Err(WireError::UnsupportedVersion {
                expected: WIRE_VERSION,
                actual: self.wire_version,
            });
        }
        if self.domain_descriptor == [0; 32] || self.engine_program == [0; 32] {
            return Err(WireError::UnsupportedValue {
                field: "domain admission identity",
                value: 0,
            });
        }
        Ok(())
    }

    pub fn digest(&self) -> WireResult<[u8; 32]> {
        let encoded = self.encode()?;
        hash_private(LABEL_DOMAIN_ADMISSION_RECORD, &[&encoded])
    }

    pub fn address_digest(&self) -> WireResult<[u8; 32]> {
        self.validate()?;
        let revision = self.domain_revision.to_le_bytes();
        hash_private(
            LABEL_DOMAIN_ADMISSION_ADDRESS,
            &[
                &self.domain_descriptor,
                &revision,
                &self.market,
                &self.engine_program,
                &self.engine_interface_id,
                &self.engine_instance_policy_digest,
                &self.engine_admission_policy_digest,
                &self.settlement_profile_digest,
                &self.admission_rule_digest,
            ],
        )
    }
}

pub fn compute_domain_admission_address_digest(
    admission: &DomainAdmissionCandidateV0,
) -> WireResult<[u8; 32]> {
    admission.address_digest()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn admission() -> DomainAdmissionCandidateV0 {
        DomainAdmissionCandidateV0 {
            wire_version: WIRE_VERSION,
            domain_descriptor: [1; 32],
            domain_revision: 2,
            market: [3; 32],
            engine_program: [4; 32],
            engine_interface_id: [5; 32],
            engine_instance_policy_digest: [6; 32],
            engine_admission_policy_digest: [7; 32],
            settlement_profile_digest: [8; 32],
            admission_rule_digest: [9; 32],
            active_from_slot: 10,
            expires_at_slot_or_zero: 11,
            revoked_at_slot_or_zero: 0,
        }
    }

    #[test]
    fn admission_is_exactly_296_bytes() {
        let value = admission();
        let encoded = value.encode().unwrap();
        assert_eq!(encoded.len(), DOMAIN_ADMISSION_LEN);
        assert_eq!(
            DomainAdmissionCandidateV0::decode_exact(&encoded),
            Ok(value)
        );
        assert!(DomainAdmissionCandidateV0::decode_exact(&encoded[..295]).is_err());
    }

    #[test]
    fn admission_rejects_reserved_mutation() {
        let mut encoded = admission().encode().unwrap();
        encoded[7] = 1;
        assert!(DomainAdmissionCandidateV0::decode_exact(&encoded).is_err());
    }

    #[test]
    fn exact_engine_instance_policy_binds_all_typed_facts() {
        let base =
            compute_exact_engine_instance_policy_digest(&[1; 32], &[2; 32], &[3; 32], &[4; 32])
                .unwrap();
        assert_ne!(
            base,
            compute_exact_engine_instance_policy_digest(&[1; 32], &[2; 32], &[3; 32], &[5; 32],)
                .unwrap()
        );
    }
}
