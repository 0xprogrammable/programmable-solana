use alloc::vec::Vec;

use solana_pubkey::Pubkey;

use crate::codec::{put_bytes, put_u8, Reader};
use crate::hashes::{
    hash_list, hash_private, LABEL_INTENT_SPEND_SEED, LABEL_OPAQUE_CAPABILITY_LIST,
};
use crate::{WireError, WireResult};

pub const DISPOSABLE_CORE_PROGRAM_ID: Pubkey =
    solana_pubkey::pubkey!("3mg7sM6RFEBHiiFotFNfvteH1WdFcc9cujKuPaqZdfDz");
pub const DISPOSABLE_ENGINE_PROGRAM_ID: Pubkey =
    solana_pubkey::pubkey!("3qbR1eZRqXUWroWKKYhbDmR3FfqTHfqSU8zZSxtANzYh");
pub const DISPOSABLE_ROUTER_PROGRAM_ID: Pubkey =
    solana_pubkey::pubkey!("3uWi9x2SRpmjztkpkr2WWeBoVq3exjXG2YfDWLvm8KsQ");
pub const DISPOSABLE_HELPER_PROGRAM_ID: Pubkey =
    solana_pubkey::pubkey!("3yS1JFVT284y8z1LC9MRoWxZjzFrdoD5axKsZiyMsfC7");

pub const CORE_EXPERIMENTAL_MAJOR: u32 = 0;
pub const NONE_INDEX: u8 = u8::MAX;

pub const CORE_EXECUTE_EFFECT_DISCRIMINATOR: [u8; 8] =
    [0x83, 0xfe, 0x02, 0x02, 0xa2, 0x5d, 0xb8, 0x78];
pub const ENGINE_TRANSITION_DISCRIMINATOR: [u8; 8] =
    [0xff, 0xef, 0x7b, 0x4f, 0x88, 0x31, 0x45, 0xc7];

pub const ENGINE_REQUEST_MAGIC: [u8; 8] = *b"PMBGEQ00";
pub const EFFECT_RECEIPT_MAGIC: [u8; 8] = *b"PMBGER00";
pub const WIRE_VERSION: u8 = 0;
pub const PHASE_TRANSITION: u8 = 0;

pub const MAX_DOMAINS: usize = 4;
pub const MAX_INTENTS: usize = 8;
pub const MAX_INLINE_INTENTS: usize = 4;
pub const MAX_ASSETS: usize = 8;
pub const MAX_LOADER_POLICY_ACCOUNTS: usize = 1;
pub const MAX_DOMAIN_CONTROL_ACCOUNTS: usize = 12;
pub const MAX_AUTHORIZATION_ACCOUNTS: usize = 20;
pub const MAX_PROTECTED_PROFILE_ACCOUNTS: usize = 9;
pub const MAX_FEE_SHARDS: usize = 4;
pub const MAX_FEE_CONTROL_ACCOUNTS: usize = 8;
pub const MAX_SETTLEMENT_CAPABILITIES: usize = 12;
pub const MAX_ENGINE_MOVES: usize = 12;
pub const MAX_OPAQUE_CAPABILITIES: usize = 8;
pub const MAX_CONTEXT_ROWS: usize = 12;
pub const MAX_OPAQUE_PAYLOAD_LEN: usize = 128;
pub const MAX_ENGINE_REQUEST_LEN: usize = 3_744;
pub const MAX_ENGINE_CPI_DATA_HEADROOM: usize = 8_192;

pub const OPAQUE_CAPABILITY_DESCRIPTOR_LEN: usize = 68;

/// Derives the exact source-specific seed committed by a one-shot delegated
/// intent. The resulting digest, rather than the two inputs directly, is the
/// second PDA seed after `b"intent-spend-v0"`.
pub fn compute_intent_spend_seed(
    intent_digest: &[u8; 32],
    source_token_account: &[u8; 32],
) -> WireResult<[u8; 32]> {
    hash_private(
        LABEL_INTENT_SPEND_SEED,
        &[intent_digest, source_token_account],
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpaqueCapabilityDescriptorCandidateV0 {
    pub position: u8,
    pub key: [u8; 32],
    pub owner: [u8; 32],
    pub executable: bool,
    pub effective_signer: bool,
    pub effective_writable: bool,
}

impl OpaqueCapabilityDescriptorCandidateV0 {
    pub fn encode(&self) -> [u8; OPAQUE_CAPABILITY_DESCRIPTOR_LEN] {
        let mut encoded = Vec::with_capacity(OPAQUE_CAPABILITY_DESCRIPTOR_LEN);
        put_u8(&mut encoded, self.position);
        put_bytes(&mut encoded, &self.key);
        put_bytes(&mut encoded, &self.owner);
        put_u8(&mut encoded, u8::from(self.executable));
        put_u8(&mut encoded, u8::from(self.effective_signer));
        put_u8(&mut encoded, u8::from(self.effective_writable));
        encoded
            .try_into()
            .expect("opaque descriptor has a fixed encoded length")
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        crate::codec::require_exact_length(data, OPAQUE_CAPABILITY_DESCRIPTOR_LEN)?;
        let mut reader = Reader::new(data);
        let descriptor = Self {
            position: reader.read_u8()?,
            key: reader.read_array()?,
            owner: reader.read_array()?,
            executable: decode_bool("opaque executable", reader.read_u8()?)?,
            effective_signer: decode_bool("opaque signer", reader.read_u8()?)?,
            effective_writable: decode_bool("opaque writable", reader.read_u8()?)?,
        };
        reader.finish()?;
        Ok(descriptor)
    }
}

pub fn compute_opaque_capability_root(
    descriptors: &[OpaqueCapabilityDescriptorCandidateV0],
) -> WireResult<[u8; 32]> {
    if descriptors.len() > MAX_OPAQUE_CAPABILITIES {
        return Err(WireError::LimitExceeded {
            field: "opaque capability count",
            maximum: MAX_OPAQUE_CAPABILITIES,
            actual: descriptors.len(),
        });
    }
    for (position, descriptor) in descriptors.iter().enumerate() {
        if usize::from(descriptor.position) != position {
            return Err(WireError::InvalidIndex {
                field: "opaque capability position",
                index: descriptor.position,
                count: descriptors.len() as u8,
            });
        }
    }
    let encoded: Vec<_> = descriptors.iter().map(|row| row.encode()).collect();
    let rows: Vec<&[u8]> = encoded.iter().map(|row| row.as_slice()).collect();
    hash_list(LABEL_OPAQUE_CAPABILITY_LIST, &rows)
}

fn decode_bool(field: &'static str, value: u8) -> WireResult<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(WireError::UnsupportedValue {
            field,
            value: u64::from(value),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_root_preserves_position_and_multiplicity() {
        let first = OpaqueCapabilityDescriptorCandidateV0 {
            position: 0,
            key: [1; 32],
            owner: [2; 32],
            executable: false,
            effective_signer: false,
            effective_writable: true,
        };
        let second = OpaqueCapabilityDescriptorCandidateV0 {
            position: 1,
            ..first
        };
        let singleton = compute_opaque_capability_root(&[first]).unwrap();
        let duplicate = compute_opaque_capability_root(&[first, second]).unwrap();
        assert_ne!(singleton, duplicate);
        assert!(compute_opaque_capability_root(&[second]).is_err());
    }

    #[test]
    fn opaque_descriptor_rejects_non_boolean_privilege() {
        let mut encoded = OpaqueCapabilityDescriptorCandidateV0 {
            position: 0,
            key: [1; 32],
            owner: [2; 32],
            executable: false,
            effective_signer: false,
            effective_writable: false,
        }
        .encode();
        encoded[65] = 2;
        assert!(OpaqueCapabilityDescriptorCandidateV0::decode_exact(&encoded).is_err());
    }

    #[test]
    fn intent_spend_seed_binds_intent_and_source_separately() {
        let base = compute_intent_spend_seed(&[1; 32], &[2; 32]).unwrap();
        assert_ne!(base, compute_intent_spend_seed(&[3; 32], &[2; 32]).unwrap());
        assert_ne!(base, compute_intent_spend_seed(&[1; 32], &[4; 32]).unwrap());
    }
}
