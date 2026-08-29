use alloc::vec::Vec;

use crate::codec::{put_bytes, put_u64, put_u8, require_exact_length, require_zero, Reader};
use crate::hashes::{hash_private, LABEL_EXACT_FEE_RECIPIENT, LABEL_FEE_SHARD_DESCRIPTOR};
use crate::{WireError, WireResult, CORE_EXPERIMENTAL_MAJOR, MAX_FEE_SHARDS, WIRE_VERSION};

pub const FEE_SHARD_DESCRIPTOR_ROW_LEN: usize = 272;
pub const FEE_SHARD_SEED_PREFIX: &[u8] = b"fee-shard-v0";

/// Canonical authenticated contents of a fee-shard descriptor account.
/// Stored bump and self-digest fields are intentionally excluded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeeShardDescriptorRowCandidateV0 {
    pub wire_version: u8,
    pub shard_index: u8,
    pub market_binding_digest: [u8; 32],
    pub fee_policy_digest: [u8; 32],
    pub fee_policy_revision: u64,
    pub asset_identity: [u8; 32],
    pub asset_program: [u8; 32],
    pub settlement_profile_digest: [u8; 32],
    pub vault: [u8; 32],
    pub liability_ledger: [u8; 32],
    pub recipient_policy_digest: [u8; 32],
}

impl FeeShardDescriptorRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; FEE_SHARD_DESCRIPTOR_ROW_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(FEE_SHARD_DESCRIPTOR_ROW_LEN);
        put_u8(&mut output, self.wire_version);
        put_u8(&mut output, self.shard_index);
        put_bytes(&mut output, &[0; 6]);
        put_bytes(&mut output, &self.market_binding_digest);
        put_bytes(&mut output, &self.fee_policy_digest);
        put_u64(&mut output, self.fee_policy_revision);
        put_bytes(&mut output, &self.asset_identity);
        put_bytes(&mut output, &self.asset_program);
        put_bytes(&mut output, &self.settlement_profile_digest);
        put_bytes(&mut output, &self.vault);
        put_bytes(&mut output, &self.liability_ledger);
        put_bytes(&mut output, &self.recipient_policy_digest);
        Ok(output
            .try_into()
            .expect("fee-shard descriptor row has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, FEE_SHARD_DESCRIPTOR_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let wire_version = reader.read_u8()?;
        let shard_index = reader.read_u8()?;
        let reserved = reader.read_array::<6>()?;
        require_zero("fee-shard descriptor reserved", &reserved)?;
        let row = Self {
            wire_version,
            shard_index,
            market_binding_digest: reader.read_array()?,
            fee_policy_digest: reader.read_array()?,
            fee_policy_revision: reader.read_u64()?,
            asset_identity: reader.read_array()?,
            asset_program: reader.read_array()?,
            settlement_profile_digest: reader.read_array()?,
            vault: reader.read_array()?,
            liability_ledger: reader.read_array()?,
            recipient_policy_digest: reader.read_array()?,
        };
        reader.finish()?;
        row.validate()?;
        Ok(row)
    }

    pub fn validate(&self) -> WireResult<()> {
        if self.wire_version != WIRE_VERSION {
            return Err(WireError::UnsupportedVersion {
                expected: WIRE_VERSION,
                actual: self.wire_version,
            });
        }
        if usize::from(self.shard_index) >= MAX_FEE_SHARDS {
            return Err(WireError::InvalidIndex {
                field: "fee-shard descriptor index",
                index: self.shard_index,
                count: MAX_FEE_SHARDS as u8,
            });
        }
        for (field, value) in [
            ("market binding", self.market_binding_digest),
            ("fee policy", self.fee_policy_digest),
            ("asset identity", self.asset_identity),
            ("asset program", self.asset_program),
            ("settlement profile", self.settlement_profile_digest),
            ("fee vault", self.vault),
            ("fee liability ledger", self.liability_ledger),
            ("fee recipient policy", self.recipient_policy_digest),
        ] {
            if value == [0; 32] {
                return Err(WireError::UnsupportedValue { field, value: 0 });
            }
        }
        Ok(())
    }
}

pub fn compute_exact_fee_recipient_policy_digest(
    core_program: &[u8; 32],
    market_binding_digest: &[u8; 32],
    vault: &[u8; 32],
    asset_identity: &[u8; 32],
    asset_program: &[u8; 32],
    settlement_profile_digest: &[u8; 32],
) -> WireResult<[u8; 32]> {
    let major = CORE_EXPERIMENTAL_MAJOR.to_le_bytes();
    hash_private(
        LABEL_EXACT_FEE_RECIPIENT,
        &[
            core_program,
            &major,
            market_binding_digest,
            vault,
            asset_identity,
            asset_program,
            settlement_profile_digest,
        ],
    )
}

pub fn compute_fee_shard_descriptor_digest(
    core_program: &[u8; 32],
    descriptor: &FeeShardDescriptorRowCandidateV0,
) -> WireResult<[u8; 32]> {
    let major = CORE_EXPERIMENTAL_MAJOR.to_le_bytes();
    hash_private(
        LABEL_FEE_SHARD_DESCRIPTOR,
        &[core_program, &major, &descriptor.encode()?],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor() -> FeeShardDescriptorRowCandidateV0 {
        FeeShardDescriptorRowCandidateV0 {
            wire_version: WIRE_VERSION,
            shard_index: 1,
            market_binding_digest: [1; 32],
            fee_policy_digest: [2; 32],
            fee_policy_revision: 3,
            asset_identity: [4; 32],
            asset_program: [5; 32],
            settlement_profile_digest: [6; 32],
            vault: [7; 32],
            liability_ledger: [8; 32],
            recipient_policy_digest: [9; 32],
        }
    }

    #[test]
    fn fee_shard_descriptor_is_exact_and_typed() {
        let row = descriptor();
        let encoded = row.encode().unwrap();
        assert_eq!(encoded.len(), FEE_SHARD_DESCRIPTOR_ROW_LEN);
        assert_eq!(
            FeeShardDescriptorRowCandidateV0::decode_exact(&encoded),
            Ok(row)
        );
        assert!(FeeShardDescriptorRowCandidateV0::decode_exact(&encoded[..271]).is_err());
        let mut trailing = encoded.to_vec();
        trailing.push(0);
        assert!(FeeShardDescriptorRowCandidateV0::decode_exact(&trailing).is_err());
        let mut reserved = encoded;
        reserved[2] = 1;
        assert!(FeeShardDescriptorRowCandidateV0::decode_exact(&reserved).is_err());
    }

    #[test]
    fn descriptor_and_recipient_digests_bind_each_exact_fact() {
        let core = [10; 32];
        let row = descriptor();
        let descriptor_digest = compute_fee_shard_descriptor_digest(&core, &row).unwrap();
        let mut changed = row;
        changed.liability_ledger = [11; 32];
        assert_ne!(
            descriptor_digest,
            compute_fee_shard_descriptor_digest(&core, &changed).unwrap()
        );

        let recipient = compute_exact_fee_recipient_policy_digest(
            &core,
            &row.market_binding_digest,
            &row.vault,
            &row.asset_identity,
            &row.asset_program,
            &row.settlement_profile_digest,
        )
        .unwrap();
        assert_ne!(
            recipient,
            compute_exact_fee_recipient_policy_digest(
                &core,
                &row.market_binding_digest,
                &[12; 32],
                &row.asset_identity,
                &row.asset_program,
                &row.settlement_profile_digest,
            )
            .unwrap()
        );
    }
}
