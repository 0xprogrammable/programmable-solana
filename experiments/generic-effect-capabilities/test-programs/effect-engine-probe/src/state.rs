//! Minimal engine-owned opaque state used to prove zero/one/many-state tails.

use anchor_lang::solana_program::account_info::AccountInfo;

use crate::{engine_error, EngineError, EngineResult};

pub const ENGINE_STATE_MAGIC: [u8; 8] = *b"PMBGES00";
pub const ENGINE_STATE_VERSION: u8 = 0;
pub const ENGINE_STATE_LEN: usize = 72;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineStateCandidateV0 {
    pub sequence: u64,
    pub accumulator: u64,
    pub last_request_digest: [u8; 32],
    pub last_move_count: u64,
}

impl EngineStateCandidateV0 {
    pub const fn fresh() -> Self {
        Self {
            sequence: 0,
            accumulator: 0,
            last_request_digest: [0; 32],
            last_move_count: 0,
        }
    }

    pub fn encode(self) -> [u8; ENGINE_STATE_LEN] {
        let mut encoded = [0_u8; ENGINE_STATE_LEN];
        encoded[..8].copy_from_slice(&ENGINE_STATE_MAGIC);
        encoded[8] = ENGINE_STATE_VERSION;
        encoded[16..24].copy_from_slice(&self.sequence.to_le_bytes());
        encoded[24..32].copy_from_slice(&self.accumulator.to_le_bytes());
        encoded[32..64].copy_from_slice(&self.last_request_digest);
        encoded[64..72].copy_from_slice(&self.last_move_count.to_le_bytes());
        encoded
    }

    pub fn decode_exact(encoded: &[u8]) -> EngineResult<Self> {
        if encoded.len() != ENGINE_STATE_LEN {
            return Err(engine_error(EngineError::InvalidEngineState));
        }
        if encoded[..8] != ENGINE_STATE_MAGIC {
            return Err(engine_error(EngineError::InvalidEngineState));
        }
        if encoded[8] != ENGINE_STATE_VERSION {
            return Err(engine_error(EngineError::InvalidEngineState));
        }
        if encoded[9..16].iter().any(|byte| *byte != 0) {
            return Err(engine_error(EngineError::InvalidEngineState));
        }
        let mut sequence = [0_u8; 8];
        sequence.copy_from_slice(&encoded[16..24]);
        let mut accumulator = [0_u8; 8];
        accumulator.copy_from_slice(&encoded[24..32]);
        let mut last_request_digest = [0_u8; 32];
        last_request_digest.copy_from_slice(&encoded[32..64]);
        let mut last_move_count = [0_u8; 8];
        last_move_count.copy_from_slice(&encoded[64..72]);
        Ok(Self {
            sequence: u64::from_le_bytes(sequence),
            accumulator: u64::from_le_bytes(accumulator),
            last_request_digest,
            last_move_count: u64::from_le_bytes(last_move_count),
        })
    }
}

pub fn mutate_state_account(
    account: &AccountInfo<'_>,
    request_digest: [u8; 32],
    move_count: usize,
    mutation_amount: u64,
) -> EngineResult<u64> {
    if *account.owner != crate::ID
        || !account.is_writable
        || account.is_signer
        || account.executable
    {
        return Err(engine_error(EngineError::InvalidEngineStateCapability));
    }

    let mut data = account
        .try_borrow_mut_data()
        .map_err(|_| engine_error(EngineError::AccountBorrowFailed))?;
    let mut state = EngineStateCandidateV0::decode_exact(&data)?;
    state.sequence = state
        .sequence
        .checked_add(1)
        .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?;
    state.accumulator = state
        .accumulator
        .checked_add(mutation_amount)
        .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?;
    state.last_request_digest = request_digest;
    state.last_move_count =
        u64::try_from(move_count).map_err(|_| engine_error(EngineError::ArithmeticOverflow))?;
    data.copy_from_slice(&state.encode());
    Ok(state.sequence)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_codec_is_exact_and_reserved_bytes_are_zero() {
        let state = EngineStateCandidateV0 {
            sequence: 7,
            accumulator: 99,
            last_request_digest: [3; 32],
            last_move_count: 12,
        };
        let encoded = state.encode();
        assert_eq!(encoded.len(), ENGINE_STATE_LEN);
        assert_eq!(
            EngineStateCandidateV0::decode_exact(&encoded).unwrap(),
            state
        );

        let mut noncanonical = encoded;
        noncanonical[10] = 1;
        assert!(EngineStateCandidateV0::decode_exact(&noncanonical).is_err());
    }
}
