use generic_effect_private_wire::{
    compute_canonical_effect_digest, decode_effect_receipt, decode_engine_request,
    decode_execute_envelope, encode_effect_receipt, EffectReceiptCandidateV0, MoveCandidateV0,
    EFFECT_RECEIPT_MAGIC, MAX_EFFECT_RECEIPT_LEN, MAX_ENGINE_MOVES, MAX_ENGINE_REQUEST_LEN,
    MAX_EXECUTE_ENVELOPE_LEN, PHASE_TRANSITION, WIRE_VERSION,
};
use proptest::prelude::*;

proptest! {
    #[test]
    fn move_rows_round_trip_exactly(source in any::<u8>(), destination in any::<u8>(), amount in any::<u64>()) {
        let movement = MoveCandidateV0 {
            source_capability_index: source,
            destination_capability_index: destination,
            amount,
        };
        let encoded = movement.encode();
        prop_assert_eq!(MoveCandidateV0::decode_exact(&encoded), Ok(movement));
    }

    #[test]
    fn receipt_round_trip_preserves_random_canonical_bytes(
        raw_moves in prop::collection::vec((any::<u8>(), any::<u8>(), 1_u64..=u64::MAX), 0..=MAX_ENGINE_MOVES),
        sequence in any::<u64>(),
    ) {
        let moves = raw_moves
            .into_iter()
            .map(|(source, destination, amount)| MoveCandidateV0 {
                source_capability_index: source,
                destination_capability_index: destination,
                amount,
            })
            .collect();
        let receipt = EffectReceiptCandidateV0 {
            magic: EFFECT_RECEIPT_MAGIC,
            wire_version: WIRE_VERSION,
            phase: PHASE_TRANSITION,
            flags: 0,
            request_digest: [1; 32],
            intent_set_digest: [2; 32],
            protected_execution_root: [3; 32],
            engine_sequence: sequence,
            engine_supplied_evidence_digest: [4; 32],
            moves,
        };
        let encoded = encode_effect_receipt(&receipt).unwrap();
        prop_assert_eq!(decode_effect_receipt(&encoded), Ok(receipt));
    }

    #[test]
    fn canonical_effect_digest_binds_every_amount(
        source in any::<u8>(),
        destination in any::<u8>(),
        amount in 1_u64..u64::MAX,
    ) {
        let original = [MoveCandidateV0 {
            source_capability_index: source,
            destination_capability_index: destination,
            amount,
        }];
        let changed = [MoveCandidateV0 {
            amount: amount + 1,
            ..original[0]
        }];
        prop_assert_ne!(
            compute_canonical_effect_digest(&[7; 32], &[8; 32], &original).unwrap(),
            compute_canonical_effect_digest(&[7; 32], &[8; 32], &changed).unwrap()
        );
    }

    #[test]
    fn arbitrary_bounded_bytes_never_escape_fail_closed_decoders(
        bytes in prop::collection::vec(any::<u8>(), 0..=(MAX_EXECUTE_ENVELOPE_LEN + 32)),
    ) {
        let _ = decode_execute_envelope(&bytes);
        let _ = decode_engine_request(&bytes[..bytes.len().min(MAX_ENGINE_REQUEST_LEN + 1)]);
        let _ = decode_effect_receipt(&bytes[..bytes.len().min(MAX_EFFECT_RECEIPT_LEN + 1)]);
    }

    #[test]
    fn any_trailing_receipt_byte_is_rejected(trailing in any::<u8>()) {
        let receipt = EffectReceiptCandidateV0 {
            magic: EFFECT_RECEIPT_MAGIC,
            wire_version: WIRE_VERSION,
            phase: PHASE_TRANSITION,
            flags: 0,
            request_digest: [1; 32],
            intent_set_digest: [2; 32],
            protected_execution_root: [3; 32],
            engine_sequence: 4,
            engine_supplied_evidence_digest: [5; 32],
            moves: vec![],
        };
        let mut encoded = encode_effect_receipt(&receipt).unwrap();
        encoded.push(trailing);
        prop_assert!(decode_effect_receipt(&encoded).is_err());
    }
}
