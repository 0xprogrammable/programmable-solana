//! Exact engine-owned state codecs for the private reference-semantic plans.
//!
//! These accounts live entirely in the opaque engine plane. Core sees only
//! their landing-time account capabilities and never parses these bytes.

use crate::{engine_error, EngineError, EngineResult};

pub const CONSTANT_PRODUCT_STATE_MAGIC: [u8; 8] = *b"PMBCPS00";
pub const AUCTION_STATE_MAGIC: [u8; 8] = *b"PMBAUC00";
pub const ORDER_STATE_MAGIC: [u8; 8] = *b"PMBORD00";
pub const REFERENCE_STATE_VERSION: u8 = 0;
pub const REFERENCE_STATE_LEN: usize = 96;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstantProductStateCandidateV0 {
    pub sequence: u64,
    pub input_asset_index: u8,
    pub output_asset_index: u8,
    pub swap_fee_numerator: u64,
    pub nonzero_swap_fee_denominator: u64,
    pub last_request_digest: [u8; 32],
    pub last_input_amount: u64,
    pub last_output_amount: u64,
}

impl ConstantProductStateCandidateV0 {
    pub fn encode(self) -> EngineResult<[u8; REFERENCE_STATE_LEN]> {
        self.validate()?;
        let mut encoded = [0_u8; REFERENCE_STATE_LEN];
        encoded[..8].copy_from_slice(&CONSTANT_PRODUCT_STATE_MAGIC);
        encoded[8] = REFERENCE_STATE_VERSION;
        encoded[16..24].copy_from_slice(&self.sequence.to_le_bytes());
        encoded[24] = self.input_asset_index;
        encoded[25] = self.output_asset_index;
        encoded[32..40].copy_from_slice(&self.swap_fee_numerator.to_le_bytes());
        encoded[40..48].copy_from_slice(&self.nonzero_swap_fee_denominator.to_le_bytes());
        encoded[48..80].copy_from_slice(&self.last_request_digest);
        encoded[80..88].copy_from_slice(&self.last_input_amount.to_le_bytes());
        encoded[88..96].copy_from_slice(&self.last_output_amount.to_le_bytes());
        Ok(encoded)
    }

    pub fn decode_exact(encoded: &[u8]) -> EngineResult<Self> {
        require_header(encoded, CONSTANT_PRODUCT_STATE_MAGIC)?;
        require_zero(&encoded[9..16])?;
        require_zero(&encoded[26..32])?;
        let state = Self {
            sequence: read_u64(encoded, 16)?,
            input_asset_index: encoded[24],
            output_asset_index: encoded[25],
            swap_fee_numerator: read_u64(encoded, 32)?,
            nonzero_swap_fee_denominator: read_u64(encoded, 40)?,
            last_request_digest: read_array_32(encoded, 48)?,
            last_input_amount: read_u64(encoded, 80)?,
            last_output_amount: read_u64(encoded, 88)?,
        };
        state.validate()?;
        Ok(state)
    }

    pub fn advance(
        self,
        request_digest: [u8; 32],
        input_amount: u64,
        output_amount: u64,
    ) -> EngineResult<Self> {
        if input_amount == 0 || output_amount == 0 {
            return Err(engine_error(EngineError::InvalidReferenceState));
        }
        let next = Self {
            sequence: self
                .sequence
                .checked_add(1)
                .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?,
            last_request_digest: request_digest,
            last_input_amount: input_amount,
            last_output_amount: output_amount,
            ..self
        };
        next.validate()?;
        Ok(next)
    }

    fn validate(&self) -> EngineResult<()> {
        validate_asset_pair(self.input_asset_index, self.output_asset_index)?;
        if self.nonzero_swap_fee_denominator == 0
            || self.swap_fee_numerator >= self.nonzero_swap_fee_denominator
        {
            return Err(engine_error(EngineError::InvalidReferenceState));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuctionStateCandidateV0 {
    pub sequence: u64,
    pub payment_asset_index: u8,
    pub inventory_asset_index: u8,
    pub unit_price_numerator: u64,
    pub nonzero_unit_price_denominator: u64,
    pub remaining_inventory: u64,
    pub filled_inventory: u64,
    pub last_request_digest: [u8; 32],
}

impl AuctionStateCandidateV0 {
    pub fn encode(self) -> EngineResult<[u8; REFERENCE_STATE_LEN]> {
        self.validate()?;
        let mut encoded = [0_u8; REFERENCE_STATE_LEN];
        encoded[..8].copy_from_slice(&AUCTION_STATE_MAGIC);
        encoded[8] = REFERENCE_STATE_VERSION;
        encoded[16..24].copy_from_slice(&self.sequence.to_le_bytes());
        encoded[24] = self.payment_asset_index;
        encoded[25] = self.inventory_asset_index;
        encoded[32..40].copy_from_slice(&self.unit_price_numerator.to_le_bytes());
        encoded[40..48].copy_from_slice(&self.nonzero_unit_price_denominator.to_le_bytes());
        encoded[48..56].copy_from_slice(&self.remaining_inventory.to_le_bytes());
        encoded[56..64].copy_from_slice(&self.filled_inventory.to_le_bytes());
        encoded[64..96].copy_from_slice(&self.last_request_digest);
        Ok(encoded)
    }

    pub fn decode_exact(encoded: &[u8]) -> EngineResult<Self> {
        require_header(encoded, AUCTION_STATE_MAGIC)?;
        require_zero(&encoded[9..16])?;
        require_zero(&encoded[26..32])?;
        let state = Self {
            sequence: read_u64(encoded, 16)?,
            payment_asset_index: encoded[24],
            inventory_asset_index: encoded[25],
            unit_price_numerator: read_u64(encoded, 32)?,
            nonzero_unit_price_denominator: read_u64(encoded, 40)?,
            remaining_inventory: read_u64(encoded, 48)?,
            filled_inventory: read_u64(encoded, 56)?,
            last_request_digest: read_array_32(encoded, 64)?,
        };
        state.validate()?;
        Ok(state)
    }

    pub fn advance(self, request_digest: [u8; 32], fill_inventory: u64) -> EngineResult<Self> {
        if fill_inventory == 0 || fill_inventory > self.remaining_inventory {
            return Err(engine_error(EngineError::InvalidReferenceState));
        }
        let next = Self {
            sequence: self
                .sequence
                .checked_add(1)
                .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?,
            remaining_inventory: self
                .remaining_inventory
                .checked_sub(fill_inventory)
                .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?,
            filled_inventory: self
                .filled_inventory
                .checked_add(fill_inventory)
                .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?,
            last_request_digest: request_digest,
            ..self
        };
        next.validate()?;
        Ok(next)
    }

    fn validate(&self) -> EngineResult<()> {
        validate_asset_pair(self.payment_asset_index, self.inventory_asset_index)?;
        if self.unit_price_numerator == 0 || self.nonzero_unit_price_denominator == 0 {
            return Err(engine_error(EngineError::InvalidReferenceState));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrderStateCandidateV0 {
    pub sequence: u64,
    pub payment_asset_index: u8,
    pub inventory_asset_index: u8,
    pub maximum_unit_price_numerator: u64,
    pub nonzero_maximum_unit_price_denominator: u64,
    pub remaining_payment: u64,
    pub paid_payment: u64,
    pub last_request_digest: [u8; 32],
}

impl OrderStateCandidateV0 {
    pub fn encode(self) -> EngineResult<[u8; REFERENCE_STATE_LEN]> {
        self.validate()?;
        let mut encoded = [0_u8; REFERENCE_STATE_LEN];
        encoded[..8].copy_from_slice(&ORDER_STATE_MAGIC);
        encoded[8] = REFERENCE_STATE_VERSION;
        encoded[16..24].copy_from_slice(&self.sequence.to_le_bytes());
        encoded[24] = self.payment_asset_index;
        encoded[25] = self.inventory_asset_index;
        encoded[32..40].copy_from_slice(&self.maximum_unit_price_numerator.to_le_bytes());
        encoded[40..48].copy_from_slice(&self.nonzero_maximum_unit_price_denominator.to_le_bytes());
        encoded[48..56].copy_from_slice(&self.remaining_payment.to_le_bytes());
        encoded[56..64].copy_from_slice(&self.paid_payment.to_le_bytes());
        encoded[64..96].copy_from_slice(&self.last_request_digest);
        Ok(encoded)
    }

    pub fn decode_exact(encoded: &[u8]) -> EngineResult<Self> {
        require_header(encoded, ORDER_STATE_MAGIC)?;
        require_zero(&encoded[9..16])?;
        require_zero(&encoded[26..32])?;
        let state = Self {
            sequence: read_u64(encoded, 16)?,
            payment_asset_index: encoded[24],
            inventory_asset_index: encoded[25],
            maximum_unit_price_numerator: read_u64(encoded, 32)?,
            nonzero_maximum_unit_price_denominator: read_u64(encoded, 40)?,
            remaining_payment: read_u64(encoded, 48)?,
            paid_payment: read_u64(encoded, 56)?,
            last_request_digest: read_array_32(encoded, 64)?,
        };
        state.validate()?;
        Ok(state)
    }

    pub fn advance(self, request_digest: [u8; 32], payment: u64) -> EngineResult<Self> {
        if payment == 0 || payment > self.remaining_payment {
            return Err(engine_error(EngineError::InvalidReferenceState));
        }
        let next = Self {
            sequence: self
                .sequence
                .checked_add(1)
                .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?,
            remaining_payment: self
                .remaining_payment
                .checked_sub(payment)
                .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?,
            paid_payment: self
                .paid_payment
                .checked_add(payment)
                .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?,
            last_request_digest: request_digest,
            ..self
        };
        next.validate()?;
        Ok(next)
    }

    fn validate(&self) -> EngineResult<()> {
        validate_asset_pair(self.payment_asset_index, self.inventory_asset_index)?;
        if self.maximum_unit_price_numerator == 0
            || self.nonzero_maximum_unit_price_denominator == 0
        {
            return Err(engine_error(EngineError::InvalidReferenceState));
        }
        Ok(())
    }
}

pub fn auction_price_within_order_limit(
    auction: &AuctionStateCandidateV0,
    order: &OrderStateCandidateV0,
) -> EngineResult<()> {
    auction.validate()?;
    order.validate()?;
    if auction.sequence != order.sequence
        || auction.payment_asset_index != order.payment_asset_index
        || auction.inventory_asset_index != order.inventory_asset_index
    {
        return Err(engine_error(EngineError::InvalidReferenceState));
    }
    if cumulative_auction_payment(auction)? != order.paid_payment {
        return Err(engine_error(EngineError::InvalidReferenceState));
    }
    let auction_scaled = u128::from(auction.unit_price_numerator)
        .checked_mul(u128::from(order.nonzero_maximum_unit_price_denominator))
        .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?;
    let order_scaled = u128::from(order.maximum_unit_price_numerator)
        .checked_mul(u128::from(auction.nonzero_unit_price_denominator))
        .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?;
    if auction_scaled > order_scaled {
        Err(engine_error(EngineError::InvalidReferenceState))
    } else {
        Ok(())
    }
}

fn cumulative_auction_payment(auction: &AuctionStateCandidateV0) -> EngineResult<u64> {
    if auction.filled_inventory == 0 {
        return Ok(0);
    }
    let product = u128::from(auction.filled_inventory)
        .checked_mul(u128::from(auction.unit_price_numerator))
        .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?;
    let denominator = u128::from(auction.nonzero_unit_price_denominator);
    let rounded = product
        .checked_add(denominator - 1)
        .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?
        / denominator;
    u64::try_from(rounded).map_err(|_| engine_error(EngineError::ArithmeticOverflow))
}

fn require_header(encoded: &[u8], magic: [u8; 8]) -> EngineResult<()> {
    if encoded.len() != REFERENCE_STATE_LEN {
        return Err(engine_error(EngineError::InvalidReferenceState));
    }
    if encoded[..8] != magic || encoded[8] != REFERENCE_STATE_VERSION {
        return Err(engine_error(EngineError::InvalidReferenceState));
    }
    Ok(())
}

fn validate_asset_pair(first: u8, second: u8) -> EngineResult<()> {
    if first == second
        || usize::from(first) >= generic_effect_private_wire::MAX_ASSETS
        || usize::from(second) >= generic_effect_private_wire::MAX_ASSETS
    {
        Err(engine_error(EngineError::InvalidReferenceState))
    } else {
        Ok(())
    }
}

fn require_zero(bytes: &[u8]) -> EngineResult<()> {
    if bytes.iter().all(|byte| *byte == 0) {
        Ok(())
    } else {
        Err(engine_error(EngineError::InvalidReferenceState))
    }
}

fn read_u64(encoded: &[u8], offset: usize) -> EngineResult<u64> {
    let bytes = encoded
        .get(offset..offset + 8)
        .ok_or_else(|| engine_error(EngineError::InvalidReferenceState))?;
    Ok(u64::from_le_bytes(bytes.try_into().map_err(|_| {
        engine_error(EngineError::InvalidReferenceState)
    })?))
}

fn read_array_32(encoded: &[u8], offset: usize) -> EngineResult<[u8; 32]> {
    encoded
        .get(offset..offset + 32)
        .ok_or_else(|| engine_error(EngineError::InvalidReferenceState))?
        .try_into()
        .map_err(|_| engine_error(EngineError::InvalidReferenceState))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constant_product_state() -> ConstantProductStateCandidateV0 {
        ConstantProductStateCandidateV0 {
            sequence: 7,
            input_asset_index: 0,
            output_asset_index: 1,
            swap_fee_numerator: 3,
            nonzero_swap_fee_denominator: 1_000,
            last_request_digest: [4; 32],
            last_input_amount: 100_000,
            last_output_amount: 181_322,
        }
    }

    fn auction_state() -> AuctionStateCandidateV0 {
        AuctionStateCandidateV0 {
            sequence: 1,
            payment_asset_index: 0,
            inventory_asset_index: 1,
            unit_price_numerator: 2,
            nonzero_unit_price_denominator: 1,
            remaining_inventory: 19_999,
            filled_inventory: 10_001,
            last_request_digest: [5; 32],
        }
    }

    fn order_state() -> OrderStateCandidateV0 {
        OrderStateCandidateV0 {
            sequence: 1,
            payment_asset_index: 0,
            inventory_asset_index: 1,
            maximum_unit_price_numerator: 2,
            nonzero_maximum_unit_price_denominator: 1,
            remaining_payment: 39_998,
            paid_payment: 20_002,
            last_request_digest: [5; 32],
        }
    }

    #[test]
    fn typed_reference_states_have_exact_distinct_round_trips() {
        let constant_product = constant_product_state();
        let auction = auction_state();
        let order = order_state();
        let constant_product_bytes = constant_product.encode().unwrap();
        let auction_bytes = auction.encode().unwrap();
        let order_bytes = order.encode().unwrap();

        assert_eq!(constant_product_bytes.len(), REFERENCE_STATE_LEN);
        assert_eq!(auction_bytes.len(), REFERENCE_STATE_LEN);
        assert_eq!(order_bytes.len(), REFERENCE_STATE_LEN);
        assert_eq!(
            ConstantProductStateCandidateV0::decode_exact(&constant_product_bytes).unwrap(),
            constant_product
        );
        assert_eq!(
            AuctionStateCandidateV0::decode_exact(&auction_bytes).unwrap(),
            auction
        );
        assert_eq!(
            OrderStateCandidateV0::decode_exact(&order_bytes).unwrap(),
            order
        );
        assert!(AuctionStateCandidateV0::decode_exact(&constant_product_bytes).is_err());
        assert!(OrderStateCandidateV0::decode_exact(&auction_bytes).is_err());
    }

    #[test]
    fn every_reference_state_reserved_region_is_strictly_zero() {
        for mut encoded in [
            constant_product_state().encode().unwrap(),
            auction_state().encode().unwrap(),
            order_state().encode().unwrap(),
        ] {
            for offset in (9..16).chain(26..32) {
                let original = encoded[offset];
                encoded[offset] = 1;
                assert!(ConstantProductStateCandidateV0::decode_exact(&encoded).is_err());
                assert!(AuctionStateCandidateV0::decode_exact(&encoded).is_err());
                assert!(OrderStateCandidateV0::decode_exact(&encoded).is_err());
                encoded[offset] = original;
            }
        }
    }

    #[test]
    fn reference_state_headers_lengths_and_parameters_fail_closed() {
        let mut encoded = constant_product_state().encode().unwrap();
        assert!(ConstantProductStateCandidateV0::decode_exact(&encoded[..95]).is_err());
        let mut trailing = encoded.to_vec();
        trailing.push(0);
        assert!(ConstantProductStateCandidateV0::decode_exact(&trailing).is_err());
        encoded[0] ^= 1;
        assert!(ConstantProductStateCandidateV0::decode_exact(&encoded).is_err());

        let mut invalid = constant_product_state();
        invalid.output_asset_index = invalid.input_asset_index;
        assert!(invalid.encode().is_err());
        invalid = constant_product_state();
        invalid.swap_fee_numerator = invalid.nonzero_swap_fee_denominator;
        assert!(invalid.encode().is_err());

        let mut invalid_auction = auction_state();
        invalid_auction.nonzero_unit_price_denominator = 0;
        assert!(invalid_auction.encode().is_err());
        let mut invalid_order = order_state();
        invalid_order.maximum_unit_price_numerator = 0;
        assert!(invalid_order.encode().is_err());
    }

    #[test]
    fn state_advances_are_checked_and_bind_the_last_request() {
        let constant_product = ConstantProductStateCandidateV0 {
            sequence: 0,
            last_request_digest: [0; 32],
            last_input_amount: 0,
            last_output_amount: 0,
            ..constant_product_state()
        };
        let advanced = constant_product.advance([9; 32], 100_000, 181_322).unwrap();
        assert_eq!(advanced.sequence, 1);
        assert_eq!(advanced.last_request_digest, [9; 32]);
        assert_eq!(advanced.last_input_amount, 100_000);
        assert_eq!(advanced.last_output_amount, 181_322);

        let auction = AuctionStateCandidateV0 {
            sequence: 0,
            remaining_inventory: 30_000,
            filled_inventory: 0,
            last_request_digest: [0; 32],
            ..auction_state()
        };
        let order = OrderStateCandidateV0 {
            sequence: 0,
            remaining_payment: 60_000,
            paid_payment: 0,
            last_request_digest: [0; 32],
            ..order_state()
        };
        auction_price_within_order_limit(&auction, &order).unwrap();
        let auction = auction.advance([8; 32], 10_001).unwrap();
        let order = order.advance([8; 32], 20_002).unwrap();
        assert_eq!(auction.sequence, order.sequence);
        assert_eq!(auction.remaining_inventory, 19_999);
        assert_eq!(auction.filled_inventory, 10_001);
        assert_eq!(order.remaining_payment, 39_998);
        assert_eq!(order.paid_payment, 20_002);

        assert!(ConstantProductStateCandidateV0 {
            sequence: u64::MAX,
            ..constant_product
        }
        .advance([1; 32], 1, 1)
        .is_err());
        assert!(AuctionStateCandidateV0 {
            sequence: u64::MAX,
            ..auction_state()
        }
        .advance([1; 32], 1)
        .is_err());
        assert!(auction_state().advance([1; 32], 0).is_err());
        assert!(auction_state().advance([1; 32], 20_000).is_err());
        assert!(OrderStateCandidateV0 {
            sequence: u64::MAX,
            ..order_state()
        }
        .advance([1; 32], 1)
        .is_err());
        assert!(order_state().advance([1; 32], 0).is_err());
        assert!(order_state().advance([1; 32], 39_999).is_err());
    }

    #[test]
    fn auction_price_and_sequence_mismatches_are_rejected() {
        let auction = auction_state();
        let mut order = order_state();
        order.maximum_unit_price_numerator = 1;
        assert!(auction_price_within_order_limit(&auction, &order).is_err());
        order = order_state();
        order.sequence = 2;
        assert!(auction_price_within_order_limit(&auction, &order).is_err());
    }
}
