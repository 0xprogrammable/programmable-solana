use alloc::vec::Vec;

use crate::codec::{put_bytes, put_u64, put_u8, require_exact_length, require_zero, Reader};
use crate::hashes::{
    hash_private, LABEL_ENGINE_ADMISSION_POLICY, LABEL_ENGINE_LOADER_STATE_SNAPSHOT,
};
use crate::{WireError, WireResult};

pub const LOADER_V3_PROGRAM_ID: solana_pubkey::Pubkey =
    solana_pubkey::pubkey!("BPFLoaderUpgradeab1e11111111111111111111111");

pub const ENGINE_POLICY_IMMUTABLE: u8 = 0;
/// Rejected liveness-DoS fixture, not an accepted strong policy class.
pub const ENGINE_POLICY_PINNED_MUTABLE_REJECTED: u8 = 1;
pub const ENGINE_POLICY_MUTABLE_CONTROLLER_RISK: u8 = 2;

pub const ENGINE_ADMISSION_POLICY_LEN: usize = 144;
pub const ENGINE_LOADER_STATE_SNAPSHOT_LEN: usize = 136;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineAdmissionPolicyCandidateV0 {
    pub policy_kind: u8,
    pub engine_program: [u8; 32],
    pub loader_program: [u8; 32],
    pub program_data_or_zero: [u8; 32],
    pub expected_controller_or_zero: [u8; 32],
    pub captured_programdata_slot_or_zero: u64,
}

impl EngineAdmissionPolicyCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; ENGINE_ADMISSION_POLICY_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(ENGINE_ADMISSION_POLICY_LEN);
        put_u8(&mut output, self.policy_kind);
        put_bytes(&mut output, &[0; 7]);
        put_bytes(&mut output, &self.engine_program);
        put_bytes(&mut output, &self.loader_program);
        put_bytes(&mut output, &self.program_data_or_zero);
        put_bytes(&mut output, &self.expected_controller_or_zero);
        put_u64(&mut output, self.captured_programdata_slot_or_zero);
        Ok(output
            .try_into()
            .expect("admission policy has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, ENGINE_ADMISSION_POLICY_LEN)?;
        let mut reader = Reader::new(data);
        let policy_kind = reader.read_u8()?;
        let reserved = reader.read_array::<7>()?;
        require_zero("engine admission policy reserved", &reserved)?;
        let policy = Self {
            policy_kind,
            engine_program: reader.read_array()?,
            loader_program: reader.read_array()?,
            program_data_or_zero: reader.read_array()?,
            expected_controller_or_zero: reader.read_array()?,
            captured_programdata_slot_or_zero: reader.read_u64()?,
        };
        reader.finish()?;
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(&self) -> WireResult<()> {
        let zero = [0_u8; 32];
        if self.engine_program == zero
            || self.loader_program != LOADER_V3_PROGRAM_ID.to_bytes()
            || self.program_data_or_zero == zero
        {
            return Err(WireError::UnsupportedValue {
                field: "engine admission loader relation",
                value: 0,
            });
        }
        match self.policy_kind {
            ENGINE_POLICY_IMMUTABLE => {
                if self.expected_controller_or_zero != zero {
                    return Err(WireError::UnsupportedValue {
                        field: "immutable admission policy fields",
                        value: u64::from(self.policy_kind),
                    });
                }
            }
            ENGINE_POLICY_PINNED_MUTABLE_REJECTED => {
                if self.expected_controller_or_zero == zero {
                    return Err(WireError::UnsupportedValue {
                        field: "rejected pinned-mutable policy fields",
                        value: u64::from(self.policy_kind),
                    });
                }
            }
            ENGINE_POLICY_MUTABLE_CONTROLLER_RISK => {
                if self.expected_controller_or_zero == zero
                    || self.captured_programdata_slot_or_zero != 0
                {
                    return Err(WireError::UnsupportedValue {
                        field: "mutable-controller admission policy fields",
                        value: u64::from(self.policy_kind),
                    });
                }
            }
            value => {
                return Err(WireError::UnsupportedValue {
                    field: "engine admission policy kind",
                    value: u64::from(value),
                });
            }
        }
        Ok(())
    }

    pub fn digest(&self) -> WireResult<[u8; 32]> {
        let encoded = self.encode()?;
        hash_private(LABEL_ENGINE_ADMISSION_POLICY, &[&encoded])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineLoaderStateSnapshotCandidateV0 {
    pub engine_program: [u8; 32],
    pub loader_program: [u8; 32],
    pub program_data_or_zero: [u8; 32],
    pub observed_programdata_slot: u64,
    pub observed_controller_or_zero: [u8; 32],
}

impl EngineLoaderStateSnapshotCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; ENGINE_LOADER_STATE_SNAPSHOT_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(ENGINE_LOADER_STATE_SNAPSHOT_LEN);
        put_bytes(&mut output, &self.engine_program);
        put_bytes(&mut output, &self.loader_program);
        put_bytes(&mut output, &self.program_data_or_zero);
        put_u64(&mut output, self.observed_programdata_slot);
        put_bytes(&mut output, &self.observed_controller_or_zero);
        Ok(output
            .try_into()
            .expect("loader-state snapshot has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, ENGINE_LOADER_STATE_SNAPSHOT_LEN)?;
        let mut reader = Reader::new(data);
        let snapshot = Self {
            engine_program: reader.read_array()?,
            loader_program: reader.read_array()?,
            program_data_or_zero: reader.read_array()?,
            observed_programdata_slot: reader.read_u64()?,
            observed_controller_or_zero: reader.read_array()?,
        };
        reader.finish()?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn validate(&self) -> WireResult<()> {
        let zero = [0_u8; 32];
        if self.engine_program == zero
            || self.loader_program != LOADER_V3_PROGRAM_ID.to_bytes()
            || self.program_data_or_zero == zero
        {
            return Err(WireError::UnsupportedValue {
                field: "engine loader-state snapshot relation",
                value: 0,
            });
        }
        Ok(())
    }

    pub fn digest(&self) -> WireResult<[u8; 32]> {
        let encoded = self.encode()?;
        hash_private(LABEL_ENGINE_LOADER_STATE_SNAPSHOT, &[&encoded])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(kind: u8) -> EngineAdmissionPolicyCandidateV0 {
        EngineAdmissionPolicyCandidateV0 {
            policy_kind: kind,
            engine_program: [1; 32],
            loader_program: LOADER_V3_PROGRAM_ID.to_bytes(),
            program_data_or_zero: [3; 32],
            expected_controller_or_zero: if kind == ENGINE_POLICY_IMMUTABLE {
                [0; 32]
            } else {
                [4; 32]
            },
            captured_programdata_slot_or_zero: if kind == ENGINE_POLICY_MUTABLE_CONTROLLER_RISK {
                0
            } else {
                5
            },
        }
    }

    #[test]
    fn all_three_policies_round_trip_and_hash_differently() {
        let immutable = policy(ENGINE_POLICY_IMMUTABLE);
        let pinned = policy(ENGINE_POLICY_PINNED_MUTABLE_REJECTED);
        let controller = policy(ENGINE_POLICY_MUTABLE_CONTROLLER_RISK);
        for value in [immutable, pinned, controller] {
            let encoded = value.encode().unwrap();
            assert_eq!(encoded.len(), ENGINE_ADMISSION_POLICY_LEN);
            assert_eq!(
                EngineAdmissionPolicyCandidateV0::decode_exact(&encoded),
                Ok(value)
            );
        }
        assert_ne!(immutable.digest().unwrap(), pinned.digest().unwrap());
        assert_ne!(pinned.digest().unwrap(), controller.digest().unwrap());
    }

    #[test]
    fn admission_reserved_bytes_and_impossible_combinations_fail() {
        let mut encoded = policy(ENGINE_POLICY_IMMUTABLE).encode().unwrap();
        encoded[1] = 1;
        assert!(EngineAdmissionPolicyCandidateV0::decode_exact(&encoded).is_err());

        let mut invalid = policy(ENGINE_POLICY_MUTABLE_CONTROLLER_RISK);
        invalid.captured_programdata_slot_or_zero = 9;
        assert!(invalid.encode().is_err());
    }

    #[test]
    fn snapshot_is_exact_and_binds_slot_and_controller() {
        let snapshot = EngineLoaderStateSnapshotCandidateV0 {
            engine_program: [1; 32],
            loader_program: LOADER_V3_PROGRAM_ID.to_bytes(),
            program_data_or_zero: [3; 32],
            observed_programdata_slot: 4,
            observed_controller_or_zero: [5; 32],
        };
        let encoded = snapshot.encode().unwrap();
        assert_eq!(encoded.len(), ENGINE_LOADER_STATE_SNAPSHOT_LEN);
        assert_eq!(
            EngineLoaderStateSnapshotCandidateV0::decode_exact(&encoded),
            Ok(snapshot)
        );
        let mut changed = snapshot;
        changed.observed_programdata_slot += 1;
        assert_ne!(snapshot.digest().unwrap(), changed.digest().unwrap());
        assert!(EngineLoaderStateSnapshotCandidateV0::decode_exact(&encoded[..135]).is_err());
    }
}
