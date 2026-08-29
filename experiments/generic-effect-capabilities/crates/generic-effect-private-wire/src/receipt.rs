use alloc::vec::Vec;

use crate::codec::{
    checked_encoded_length, put_bytes, put_u64, put_u8, require_exact_length, Reader,
};
use crate::hashes::{hash_private, LABEL_CANONICAL_EFFECT};
use crate::{
    WireError, WireResult, EFFECT_RECEIPT_MAGIC, MAX_ENGINE_MOVES, PHASE_TRANSITION, WIRE_VERSION,
};

pub const MOVE_LEN: usize = 10;
pub const EFFECT_RECEIPT_FIXED_LEN: usize = 148;
pub const MAX_EFFECT_RECEIPT_LEN: usize = EFFECT_RECEIPT_FIXED_LEN + MAX_ENGINE_MOVES * MOVE_LEN;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoveCandidateV0 {
    pub source_capability_index: u8,
    pub destination_capability_index: u8,
    pub amount: u64,
}

impl MoveCandidateV0 {
    pub fn encode(&self) -> [u8; MOVE_LEN] {
        let mut output = Vec::with_capacity(MOVE_LEN);
        put_u8(&mut output, self.source_capability_index);
        put_u8(&mut output, self.destination_capability_index);
        put_u64(&mut output, self.amount);
        output
            .try_into()
            .expect("move row has a fixed encoded length")
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, MOVE_LEN)?;
        let mut reader = Reader::new(data);
        let row = Self {
            source_capability_index: reader.read_u8()?,
            destination_capability_index: reader.read_u8()?,
            amount: reader.read_u64()?,
        };
        reader.finish()?;
        Ok(row)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectReceiptCandidateV0 {
    pub magic: [u8; 8],
    pub wire_version: u8,
    pub phase: u8,
    pub flags: u8,
    pub request_digest: [u8; 32],
    pub intent_set_digest: [u8; 32],
    pub protected_execution_root: [u8; 32],
    pub engine_sequence: u64,
    pub engine_supplied_evidence_digest: [u8; 32],
    pub moves: Vec<MoveCandidateV0>,
}

impl EffectReceiptCandidateV0 {
    pub fn validate(&self) -> WireResult<()> {
        if self.magic != EFFECT_RECEIPT_MAGIC {
            return Err(WireError::InvalidMagic);
        }
        if self.wire_version != WIRE_VERSION {
            return Err(WireError::UnsupportedVersion {
                expected: WIRE_VERSION,
                actual: self.wire_version,
            });
        }
        if self.phase != PHASE_TRANSITION {
            return Err(WireError::UnsupportedValue {
                field: "effect receipt phase",
                value: u64::from(self.phase),
            });
        }
        if self.flags != 0 {
            return Err(WireError::UnknownFlags {
                field: "effect receipt flags",
                value: u64::from(self.flags),
            });
        }
        if self.moves.len() > MAX_ENGINE_MOVES {
            return Err(WireError::LimitExceeded {
                field: "effect receipt move count",
                maximum: MAX_ENGINE_MOVES,
                actual: self.moves.len(),
            });
        }
        if self.engine_supplied_evidence_digest == [0; 32] {
            return Err(WireError::UnsupportedValue {
                field: "engine supplied evidence digest",
                value: 0,
            });
        }
        Ok(())
    }
}

pub fn encode_effect_receipt(receipt: &EffectReceiptCandidateV0) -> WireResult<Vec<u8>> {
    receipt.validate()?;
    let expected =
        checked_encoded_length(EFFECT_RECEIPT_FIXED_LEN, &[(receipt.moves.len(), MOVE_LEN)])?;
    let mut output = Vec::with_capacity(expected);
    put_bytes(&mut output, &receipt.magic);
    put_u8(&mut output, receipt.wire_version);
    put_u8(&mut output, receipt.phase);
    put_u8(
        &mut output,
        u8::try_from(receipt.moves.len()).map_err(|_| WireError::LengthOverflow)?,
    );
    put_u8(&mut output, receipt.flags);
    put_bytes(&mut output, &receipt.request_digest);
    put_bytes(&mut output, &receipt.intent_set_digest);
    put_bytes(&mut output, &receipt.protected_execution_root);
    put_u64(&mut output, receipt.engine_sequence);
    put_bytes(&mut output, &receipt.engine_supplied_evidence_digest);
    for movement in &receipt.moves {
        put_bytes(&mut output, &movement.encode());
    }
    debug_assert_eq!(output.len(), expected);
    Ok(output)
}

pub fn decode_effect_receipt(data: &[u8]) -> WireResult<EffectReceiptCandidateV0> {
    if data.len() < EFFECT_RECEIPT_FIXED_LEN {
        return Err(WireError::InvalidLength {
            expected: EFFECT_RECEIPT_FIXED_LEN,
            actual: data.len(),
        });
    }
    let mut reader = Reader::new(data);
    let magic = reader.read_array()?;
    let wire_version = reader.read_u8()?;
    let phase = reader.read_u8()?;
    let move_count = reader.read_u8()?;
    let flags = reader.read_u8()?;
    if usize::from(move_count) > MAX_ENGINE_MOVES {
        return Err(WireError::LimitExceeded {
            field: "effect receipt move count",
            maximum: MAX_ENGINE_MOVES,
            actual: usize::from(move_count),
        });
    }
    let expected = checked_encoded_length(
        EFFECT_RECEIPT_FIXED_LEN,
        &[(usize::from(move_count), MOVE_LEN)],
    )?;
    require_exact_length(data, expected)?;
    let request_digest = reader.read_array()?;
    let intent_set_digest = reader.read_array()?;
    let protected_execution_root = reader.read_array()?;
    let engine_sequence = reader.read_u64()?;
    let engine_supplied_evidence_digest = reader.read_array()?;
    let mut moves = Vec::with_capacity(usize::from(move_count));
    for _ in 0..move_count {
        moves.push(MoveCandidateV0::decode_exact(&reader.read_vec(MOVE_LEN)?)?);
    }
    reader.finish()?;
    let receipt = EffectReceiptCandidateV0 {
        magic,
        wire_version,
        phase,
        flags,
        request_digest,
        intent_set_digest,
        protected_execution_root,
        engine_sequence,
        engine_supplied_evidence_digest,
        moves,
    };
    receipt.validate()?;
    Ok(receipt)
}

pub fn compute_canonical_effect_digest(
    request_digest: &[u8; 32],
    protected_execution_root: &[u8; 32],
    moves: &[MoveCandidateV0],
) -> WireResult<[u8; 32]> {
    if moves.len() > MAX_ENGINE_MOVES {
        return Err(WireError::LimitExceeded {
            field: "canonical effect move count",
            maximum: MAX_ENGINE_MOVES,
            actual: moves.len(),
        });
    }
    let count = u32::try_from(moves.len())
        .map_err(|_| WireError::LengthOverflow)?
        .to_le_bytes();
    let encoded: Vec<_> = moves.iter().map(MoveCandidateV0::encode).collect();
    let mut parts = Vec::with_capacity(encoded.len().saturating_add(3));
    parts.push(request_digest.as_slice());
    parts.push(protected_execution_root.as_slice());
    parts.push(count.as_slice());
    parts.extend(encoded.iter().map(|row| row.as_slice()));
    hash_private(LABEL_CANONICAL_EFFECT, &parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt() -> EffectReceiptCandidateV0 {
        EffectReceiptCandidateV0 {
            magic: EFFECT_RECEIPT_MAGIC,
            wire_version: WIRE_VERSION,
            phase: PHASE_TRANSITION,
            flags: 0,
            request_digest: [1; 32],
            intent_set_digest: [2; 32],
            protected_execution_root: [3; 32],
            engine_sequence: 4,
            engine_supplied_evidence_digest: [5; 32],
            moves: alloc::vec![
                MoveCandidateV0 {
                    source_capability_index: 0,
                    destination_capability_index: 2,
                    amount: 6,
                },
                MoveCandidateV0 {
                    source_capability_index: 1,
                    destination_capability_index: 3,
                    amount: 7,
                },
            ],
        }
    }

    #[test]
    fn receipt_round_trips_and_rejects_trailing_data() {
        let receipt = receipt();
        let encoded = encode_effect_receipt(&receipt).unwrap();
        assert_eq!(encoded.len(), 168);
        assert_eq!(decode_effect_receipt(&encoded), Ok(receipt));
        let mut trailing = encoded;
        trailing.push(0);
        assert!(decode_effect_receipt(&trailing).is_err());
    }

    #[test]
    fn effect_digest_binds_order_count_and_amount() {
        let receipt = receipt();
        let original = compute_canonical_effect_digest(
            &receipt.request_digest,
            &receipt.protected_execution_root,
            &receipt.moves,
        )
        .unwrap();
        let mut reordered = receipt.moves.clone();
        reordered.swap(0, 1);
        assert_ne!(
            original,
            compute_canonical_effect_digest(
                &receipt.request_digest,
                &receipt.protected_execution_root,
                &reordered,
            )
            .unwrap()
        );
        let mut changed = receipt.moves;
        changed[0].amount += 1;
        assert_ne!(
            original,
            compute_canonical_effect_digest(
                &receipt.request_digest,
                &receipt.protected_execution_root,
                &changed,
            )
            .unwrap()
        );
        assert_ne!(
            original,
            compute_canonical_effect_digest(
                &receipt.request_digest,
                &receipt.protected_execution_root,
                &[],
            )
            .unwrap()
        );
        assert_ne!(
            original,
            compute_canonical_effect_digest(&[9; 32], &receipt.protected_execution_root, &changed,)
                .unwrap()
        );
        assert_ne!(
            original,
            compute_canonical_effect_digest(&receipt.request_digest, &[9; 32], &changed).unwrap()
        );
    }

    #[test]
    fn malformed_header_fields_fail_closed() {
        let encoded = encode_effect_receipt(&receipt()).unwrap();
        for offset in [0_usize, 8, 9, 11] {
            let mut mutated = encoded.clone();
            mutated[offset] ^= 1;
            assert!(decode_effect_receipt(&mutated).is_err(), "offset {offset}");
        }
        let mut oversized = encoded;
        oversized[10] = (MAX_ENGINE_MOVES + 1) as u8;
        assert!(decode_effect_receipt(&oversized).is_err());
    }
}
