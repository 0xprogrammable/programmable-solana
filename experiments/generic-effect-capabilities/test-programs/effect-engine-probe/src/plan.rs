//! Engine-local payload codec for configuring the disposable fixture.
//!
//! This payload is deliberately opaque to Core. It is not part of the private
//! effect wire and carries no authority. Every plan kind remains within the
//! private 128-byte payload ceiling; reference-semantic plans carry formula
//! inputs and role indices, never precomputed output Move rows.

use generic_effect_private_wire::{MAX_ENGINE_MOVES, MAX_OPAQUE_CAPABILITIES, NONE_INDEX};

use crate::{engine_error, EngineError, EngineResult};

pub const PLAN_VERSION: u8 = 0;
pub const PLAN_HEADER_LEN: usize = 8;
pub const MOVE_LEN: usize = 10;

pub const PLAN_EXPLICIT_MOVES: u8 = 0;
pub const PLAN_CONTEXT_FANOUT: u8 = 1;
pub const PLAN_WEIGHTED_ALLOCATION: u8 = 2;
pub const PLAN_CONSTANT_PRODUCT: u8 = 3;
pub const PLAN_PARTIAL_AUCTION: u8 = 4;
pub const PLAN_BATCH_CLEARING: u8 = 5;
pub const PLAN_INVENTORY_DISTRIBUTION: u8 = 6;

pub const WEIGHTED_ALLOCATION_BODY_LEN: usize = 20;
pub const CONSTANT_PRODUCT_BODY_LEN: usize = 16;
pub const PARTIAL_AUCTION_BODY_LEN: usize = 16;
pub const BATCH_CLEARING_BODY_LEN: usize = 24;
pub const INVENTORY_DISTRIBUTION_BODY_LEN: usize = 32;

pub const WEIGHTED_ALLOCATION_MOVE_COUNT: u8 = 2;
pub const CONSTANT_PRODUCT_MOVE_COUNT: u8 = 2;
pub const PARTIAL_AUCTION_MOVE_COUNT: u8 = 2;
pub const BATCH_CLEARING_MOVE_COUNT: u8 = 2;
pub const INVENTORY_DISTRIBUTION_MOVE_COUNT: u8 = 3;

pub const BASIS_POINTS_DENOMINATOR: u16 = 10_000;

pub const RECEIPT_ACCEPT: u8 = 0;
pub const RECEIPT_MISSING: u8 = 1;
pub const RECEIPT_TRUNCATED: u8 = 2;
pub const RECEIPT_TRAILING_BYTE: u8 = 3;
pub const RECEIPT_WRONG_MAGIC: u8 = 4;
pub const RECEIPT_WRONG_VERSION: u8 = 5;
pub const RECEIPT_WRONG_PHASE: u8 = 6;
pub const RECEIPT_WRONG_REQUEST_DIGEST: u8 = 7;
pub const RECEIPT_WRONG_INTENT_SET: u8 = 8;
pub const RECEIPT_WRONG_CAPABILITY_ROOT: u8 = 9;
pub const RECEIPT_NONZERO_FLAGS: u8 = 10;
pub const RECEIPT_OVERSIZED_MOVE_COUNT: u8 = 11;
pub const RECEIPT_LATE_FAILURE: u8 = 12;
pub const RECEIPT_DESCENDANT_SETTER: u8 = 13;

pub const KNOWN_RECEIPT_MODE_MAX: u8 = RECEIPT_DESCENDANT_SETTER;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlannedMove {
    pub source_capability_index: u8,
    pub destination_capability_index: u8,
    pub amount: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeightedAllocationPlan {
    pub source_capability_index: u8,
    pub first_destination_capability_index: u8,
    pub second_destination_capability_index: u8,
    pub total_amount: u64,
    pub first_weight: u32,
    pub second_weight: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstantProductPlan {
    pub state_position: u8,
    pub input_source_capability_index: u8,
    pub pool_input_capability_index: u8,
    pub pool_output_capability_index: u8,
    pub output_destination_capability_index: u8,
    pub exact_input_amount: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartialAuctionPlan {
    pub auction_state_position: u8,
    pub order_state_position: u8,
    pub payment_source_capability_index: u8,
    pub payment_destination_capability_index: u8,
    pub inventory_source_capability_index: u8,
    pub inventory_destination_capability_index: u8,
    pub fill_inventory_amount: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatchClearingPlan {
    pub first_asset_index: u8,
    pub second_asset_index: u8,
    pub second_asset_per_first_numerator: u64,
    pub nonzero_first_asset_denominator: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InventoryDistributionPlan {
    pub payment_source_capability_index: u8,
    pub seller_payment_capability_index: u8,
    pub creator_payment_capability_index: u8,
    pub inventory_source_capability_index: u8,
    pub inventory_destination_capability_index: u8,
    pub inventory_quantity: u64,
    pub payment_units_per_inventory_unit: u64,
    pub seller_basis_points: u16,
    pub creator_basis_points: u16,
}

impl PlannedMove {
    pub fn encode(self) -> [u8; MOVE_LEN] {
        let mut encoded = [0_u8; MOVE_LEN];
        encoded[0] = self.source_capability_index;
        encoded[1] = self.destination_capability_index;
        encoded[2..].copy_from_slice(&self.amount.to_le_bytes());
        encoded
    }

    fn decode_exact(encoded: &[u8]) -> EngineResult<Self> {
        if encoded.len() != MOVE_LEN {
            return Err(engine_error(EngineError::InvalidPlanLength));
        }
        let mut amount = [0_u8; 8];
        amount.copy_from_slice(&encoded[2..]);
        Ok(Self {
            source_capability_index: encoded[0],
            destination_capability_index: encoded[1],
            amount: u64::from_le_bytes(amount),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanBody {
    Explicit(Vec<PlannedMove>),
    ContextFanout { unit_amount: u64 },
    WeightedAllocation(WeightedAllocationPlan),
    ConstantProduct(ConstantProductPlan),
    PartialAuction(PartialAuctionPlan),
    BatchClearing(BatchClearingPlan),
    InventoryDistribution(InventoryDistributionPlan),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnginePlan {
    pub receipt_mode: u8,
    pub state_position_bitmap: u8,
    pub helper_program_position_or_none: u8,
    pub helper_state_position_or_none: u8,
    pub move_count: u8,
    pub body: PlanBody,
}

impl EnginePlan {
    pub fn decode_exact(payload: &[u8]) -> EngineResult<Self> {
        if payload.len() < PLAN_HEADER_LEN {
            return Err(engine_error(EngineError::InvalidPlanLength));
        }
        if payload[0] != PLAN_VERSION {
            return Err(engine_error(EngineError::InvalidPlanVersion));
        }
        let receipt_mode = payload[1];
        if receipt_mode > KNOWN_RECEIPT_MODE_MAX {
            return Err(engine_error(EngineError::InvalidReceiptMode));
        }
        let plan_kind = payload[2];
        let move_count = payload[3];
        if usize::from(move_count) > MAX_ENGINE_MOVES {
            return Err(engine_error(EngineError::TooManyMoves));
        }
        let state_position_bitmap = payload[4];
        let helper_program_position_or_none = payload[5];
        let helper_state_position_or_none = payload[6];
        if payload[7] != 0 {
            return Err(engine_error(EngineError::InvalidPlanFlags));
        }

        validate_optional_position(helper_program_position_or_none)?;
        validate_optional_position(helper_state_position_or_none)?;
        if (helper_program_position_or_none == NONE_INDEX)
            != (helper_state_position_or_none == NONE_INDEX)
        {
            return Err(engine_error(EngineError::IncompleteHelperClosure));
        }
        if helper_program_position_or_none != NONE_INDEX {
            if helper_program_position_or_none == helper_state_position_or_none {
                return Err(engine_error(EngineError::AliasedHelperClosure));
            }
            let helper_state_bit = 1_u8
                .checked_shl(u32::from(helper_state_position_or_none))
                .ok_or_else(|| engine_error(EngineError::InvalidOpaquePosition))?;
            if state_position_bitmap & helper_state_bit != 0 {
                return Err(engine_error(EngineError::AliasedHelperClosure));
            }
        } else if receipt_mode == RECEIPT_DESCENDANT_SETTER {
            return Err(engine_error(EngineError::IncompleteHelperClosure));
        }

        let body = match plan_kind {
            PLAN_EXPLICIT_MOVES => {
                let expected_len = PLAN_HEADER_LEN
                    .checked_add(
                        usize::from(move_count)
                            .checked_mul(MOVE_LEN)
                            .ok_or_else(|| engine_error(EngineError::InvalidPlanLength))?,
                    )
                    .ok_or_else(|| engine_error(EngineError::InvalidPlanLength))?;
                if payload.len() != expected_len {
                    return Err(engine_error(EngineError::InvalidPlanLength));
                }
                let mut moves = Vec::with_capacity(usize::from(move_count));
                for row in payload[PLAN_HEADER_LEN..].chunks_exact(MOVE_LEN) {
                    moves.push(PlannedMove::decode_exact(row)?);
                }
                PlanBody::Explicit(moves)
            }
            PLAN_CONTEXT_FANOUT => {
                if payload.len() != PLAN_HEADER_LEN + 8 {
                    return Err(engine_error(EngineError::InvalidPlanLength));
                }
                let mut amount = [0_u8; 8];
                amount.copy_from_slice(&payload[PLAN_HEADER_LEN..]);
                let unit_amount = u64::from_le_bytes(amount);
                if unit_amount == 0 {
                    return Err(engine_error(EngineError::ZeroFanoutAmount));
                }
                PlanBody::ContextFanout { unit_amount }
            }
            PLAN_WEIGHTED_ALLOCATION => {
                require_reference_header(
                    move_count,
                    WEIGHTED_ALLOCATION_MOVE_COUNT,
                    state_position_bitmap,
                    0,
                    helper_program_position_or_none,
                )?;
                require_exact_body_len(payload, WEIGHTED_ALLOCATION_BODY_LEN)?;
                require_reserved_zero(&payload[11..12])?;
                let plan = WeightedAllocationPlan {
                    source_capability_index: payload[8],
                    first_destination_capability_index: payload[9],
                    second_destination_capability_index: payload[10],
                    total_amount: read_u64(payload, 12)?,
                    first_weight: read_u32(payload, 20)?,
                    second_weight: read_u32(payload, 24)?,
                };
                validate_weighted_allocation(plan)?;
                PlanBody::WeightedAllocation(plan)
            }
            PLAN_CONSTANT_PRODUCT => {
                require_exact_body_len(payload, CONSTANT_PRODUCT_BODY_LEN)?;
                require_reserved_zero(&payload[13..16])?;
                let plan = ConstantProductPlan {
                    state_position: payload[8],
                    input_source_capability_index: payload[9],
                    pool_input_capability_index: payload[10],
                    pool_output_capability_index: payload[11],
                    output_destination_capability_index: payload[12],
                    exact_input_amount: read_u64(payload, 16)?,
                };
                validate_constant_product(plan)?;
                require_reference_header(
                    move_count,
                    CONSTANT_PRODUCT_MOVE_COUNT,
                    state_position_bitmap,
                    position_bit(plan.state_position)?,
                    helper_program_position_or_none,
                )?;
                PlanBody::ConstantProduct(plan)
            }
            PLAN_PARTIAL_AUCTION => {
                require_exact_body_len(payload, PARTIAL_AUCTION_BODY_LEN)?;
                require_reserved_zero(&payload[14..16])?;
                let plan = PartialAuctionPlan {
                    auction_state_position: payload[8],
                    order_state_position: payload[9],
                    payment_source_capability_index: payload[10],
                    payment_destination_capability_index: payload[11],
                    inventory_source_capability_index: payload[12],
                    inventory_destination_capability_index: payload[13],
                    fill_inventory_amount: read_u64(payload, 16)?,
                };
                validate_partial_auction(plan)?;
                let expected_bitmap = position_bit(plan.auction_state_position)?
                    | position_bit(plan.order_state_position)?;
                require_reference_header(
                    move_count,
                    PARTIAL_AUCTION_MOVE_COUNT,
                    state_position_bitmap,
                    expected_bitmap,
                    helper_program_position_or_none,
                )?;
                PlanBody::PartialAuction(plan)
            }
            PLAN_BATCH_CLEARING => {
                require_reference_header(
                    move_count,
                    BATCH_CLEARING_MOVE_COUNT,
                    state_position_bitmap,
                    0,
                    helper_program_position_or_none,
                )?;
                require_exact_body_len(payload, BATCH_CLEARING_BODY_LEN)?;
                require_reserved_zero(&payload[10..16])?;
                let plan = BatchClearingPlan {
                    first_asset_index: payload[8],
                    second_asset_index: payload[9],
                    second_asset_per_first_numerator: read_u64(payload, 16)?,
                    nonzero_first_asset_denominator: read_u64(payload, 24)?,
                };
                validate_batch_clearing(plan)?;
                PlanBody::BatchClearing(plan)
            }
            PLAN_INVENTORY_DISTRIBUTION => {
                require_reference_header(
                    move_count,
                    INVENTORY_DISTRIBUTION_MOVE_COUNT,
                    state_position_bitmap,
                    0,
                    helper_program_position_or_none,
                )?;
                require_exact_body_len(payload, INVENTORY_DISTRIBUTION_BODY_LEN)?;
                require_reserved_zero(&payload[13..16])?;
                require_reserved_zero(&payload[36..40])?;
                let plan = InventoryDistributionPlan {
                    payment_source_capability_index: payload[8],
                    seller_payment_capability_index: payload[9],
                    creator_payment_capability_index: payload[10],
                    inventory_source_capability_index: payload[11],
                    inventory_destination_capability_index: payload[12],
                    inventory_quantity: read_u64(payload, 16)?,
                    payment_units_per_inventory_unit: read_u64(payload, 24)?,
                    seller_basis_points: read_u16(payload, 32)?,
                    creator_basis_points: read_u16(payload, 34)?,
                };
                validate_inventory_distribution(plan)?;
                PlanBody::InventoryDistribution(plan)
            }
            _ => return Err(engine_error(EngineError::InvalidPlanKind)),
        };

        Ok(Self {
            receipt_mode,
            state_position_bitmap,
            helper_program_position_or_none,
            helper_state_position_or_none,
            move_count,
            body,
        })
    }

    pub fn has_helper(&self) -> bool {
        self.helper_program_position_or_none != NONE_INDEX
    }

    pub fn required_reference_state_count(&self) -> Option<usize> {
        match self.body {
            PlanBody::WeightedAllocation(_)
            | PlanBody::BatchClearing(_)
            | PlanBody::InventoryDistribution(_) => Some(0),
            PlanBody::ConstantProduct(_) => Some(1),
            PlanBody::PartialAuction(_) => Some(2),
            PlanBody::Explicit(_) | PlanBody::ContextFanout { .. } => None,
        }
    }
}

pub fn encode_explicit_plan(
    receipt_mode: u8,
    state_position_bitmap: u8,
    helper_program_position_or_none: u8,
    helper_state_position_or_none: u8,
    moves: &[PlannedMove],
) -> EngineResult<Vec<u8>> {
    let move_count =
        u8::try_from(moves.len()).map_err(|_| engine_error(EngineError::TooManyMoves))?;
    let mut encoded = encode_header(
        receipt_mode,
        PLAN_EXPLICIT_MOVES,
        move_count,
        state_position_bitmap,
        helper_program_position_or_none,
        helper_state_position_or_none,
    );
    for planned_move in moves {
        encoded.extend_from_slice(&planned_move.encode());
    }
    EnginePlan::decode_exact(&encoded)?;
    Ok(encoded)
}

pub fn encode_context_fanout_plan(
    receipt_mode: u8,
    state_position_bitmap: u8,
    helper_program_position_or_none: u8,
    helper_state_position_or_none: u8,
    move_count: u8,
    unit_amount: u64,
) -> EngineResult<Vec<u8>> {
    let mut encoded = encode_header(
        receipt_mode,
        PLAN_CONTEXT_FANOUT,
        move_count,
        state_position_bitmap,
        helper_program_position_or_none,
        helper_state_position_or_none,
    );
    encoded.extend_from_slice(&unit_amount.to_le_bytes());
    EnginePlan::decode_exact(&encoded)?;
    Ok(encoded)
}

pub fn encode_weighted_allocation_plan(
    receipt_mode: u8,
    plan: WeightedAllocationPlan,
) -> EngineResult<Vec<u8>> {
    validate_weighted_allocation(plan)?;
    let mut encoded = encode_header(
        receipt_mode,
        PLAN_WEIGHTED_ALLOCATION,
        WEIGHTED_ALLOCATION_MOVE_COUNT,
        0,
        NONE_INDEX,
        NONE_INDEX,
    );
    encoded.extend_from_slice(&[
        plan.source_capability_index,
        plan.first_destination_capability_index,
        plan.second_destination_capability_index,
        0,
    ]);
    encoded.extend_from_slice(&plan.total_amount.to_le_bytes());
    encoded.extend_from_slice(&plan.first_weight.to_le_bytes());
    encoded.extend_from_slice(&plan.second_weight.to_le_bytes());
    validate_encoded_plan(encoded)
}

pub fn encode_constant_product_plan(
    receipt_mode: u8,
    plan: ConstantProductPlan,
) -> EngineResult<Vec<u8>> {
    validate_constant_product(plan)?;
    let mut encoded = encode_header(
        receipt_mode,
        PLAN_CONSTANT_PRODUCT,
        CONSTANT_PRODUCT_MOVE_COUNT,
        position_bit(plan.state_position)?,
        NONE_INDEX,
        NONE_INDEX,
    );
    encoded.extend_from_slice(&[
        plan.state_position,
        plan.input_source_capability_index,
        plan.pool_input_capability_index,
        plan.pool_output_capability_index,
        plan.output_destination_capability_index,
        0,
        0,
        0,
    ]);
    encoded.extend_from_slice(&plan.exact_input_amount.to_le_bytes());
    validate_encoded_plan(encoded)
}

pub fn encode_partial_auction_plan(
    receipt_mode: u8,
    plan: PartialAuctionPlan,
) -> EngineResult<Vec<u8>> {
    validate_partial_auction(plan)?;
    let bitmap =
        position_bit(plan.auction_state_position)? | position_bit(plan.order_state_position)?;
    let mut encoded = encode_header(
        receipt_mode,
        PLAN_PARTIAL_AUCTION,
        PARTIAL_AUCTION_MOVE_COUNT,
        bitmap,
        NONE_INDEX,
        NONE_INDEX,
    );
    encoded.extend_from_slice(&[
        plan.auction_state_position,
        plan.order_state_position,
        plan.payment_source_capability_index,
        plan.payment_destination_capability_index,
        plan.inventory_source_capability_index,
        plan.inventory_destination_capability_index,
        0,
        0,
    ]);
    encoded.extend_from_slice(&plan.fill_inventory_amount.to_le_bytes());
    validate_encoded_plan(encoded)
}

pub fn encode_batch_clearing_plan(
    receipt_mode: u8,
    plan: BatchClearingPlan,
) -> EngineResult<Vec<u8>> {
    validate_batch_clearing(plan)?;
    let mut encoded = encode_header(
        receipt_mode,
        PLAN_BATCH_CLEARING,
        BATCH_CLEARING_MOVE_COUNT,
        0,
        NONE_INDEX,
        NONE_INDEX,
    );
    encoded.extend_from_slice(&[
        plan.first_asset_index,
        plan.second_asset_index,
        0,
        0,
        0,
        0,
        0,
        0,
    ]);
    encoded.extend_from_slice(&plan.second_asset_per_first_numerator.to_le_bytes());
    encoded.extend_from_slice(&plan.nonzero_first_asset_denominator.to_le_bytes());
    validate_encoded_plan(encoded)
}

pub fn encode_inventory_distribution_plan(
    receipt_mode: u8,
    plan: InventoryDistributionPlan,
) -> EngineResult<Vec<u8>> {
    validate_inventory_distribution(plan)?;
    let mut encoded = encode_header(
        receipt_mode,
        PLAN_INVENTORY_DISTRIBUTION,
        INVENTORY_DISTRIBUTION_MOVE_COUNT,
        0,
        NONE_INDEX,
        NONE_INDEX,
    );
    encoded.extend_from_slice(&[
        plan.payment_source_capability_index,
        plan.seller_payment_capability_index,
        plan.creator_payment_capability_index,
        plan.inventory_source_capability_index,
        plan.inventory_destination_capability_index,
        0,
        0,
        0,
    ]);
    encoded.extend_from_slice(&plan.inventory_quantity.to_le_bytes());
    encoded.extend_from_slice(&plan.payment_units_per_inventory_unit.to_le_bytes());
    encoded.extend_from_slice(&plan.seller_basis_points.to_le_bytes());
    encoded.extend_from_slice(&plan.creator_basis_points.to_le_bytes());
    encoded.extend_from_slice(&[0; 4]);
    validate_encoded_plan(encoded)
}

fn encode_header(
    receipt_mode: u8,
    plan_kind: u8,
    move_count: u8,
    state_position_bitmap: u8,
    helper_program_position_or_none: u8,
    helper_state_position_or_none: u8,
) -> Vec<u8> {
    vec![
        PLAN_VERSION,
        receipt_mode,
        plan_kind,
        move_count,
        state_position_bitmap,
        helper_program_position_or_none,
        helper_state_position_or_none,
        0,
    ]
}

fn validate_encoded_plan(encoded: Vec<u8>) -> EngineResult<Vec<u8>> {
    if encoded.len() > generic_effect_private_wire::MAX_OPAQUE_PAYLOAD_LEN {
        return Err(engine_error(EngineError::InvalidPlanLength));
    }
    EnginePlan::decode_exact(&encoded)?;
    Ok(encoded)
}

fn require_exact_body_len(payload: &[u8], body_len: usize) -> EngineResult<()> {
    if payload.len() == PLAN_HEADER_LEN + body_len {
        Ok(())
    } else {
        Err(engine_error(EngineError::InvalidPlanLength))
    }
}

fn require_reference_header(
    actual_move_count: u8,
    expected_move_count: u8,
    actual_state_bitmap: u8,
    expected_state_bitmap: u8,
    helper_program_position_or_none: u8,
) -> EngineResult<()> {
    if actual_move_count != expected_move_count
        || actual_state_bitmap != expected_state_bitmap
        || helper_program_position_or_none != NONE_INDEX
    {
        Err(engine_error(EngineError::InvalidSemanticPlan))
    } else {
        Ok(())
    }
}

fn validate_weighted_allocation(plan: WeightedAllocationPlan) -> EngineResult<()> {
    require_distinct_capabilities(&[
        plan.source_capability_index,
        plan.first_destination_capability_index,
        plan.second_destination_capability_index,
    ])?;
    if plan.total_amount == 0 || plan.first_weight == 0 || plan.second_weight == 0 {
        return Err(engine_error(EngineError::InvalidSemanticPlan));
    }
    Ok(())
}

fn validate_constant_product(plan: ConstantProductPlan) -> EngineResult<()> {
    validate_state_position(plan.state_position)?;
    require_distinct_capabilities(&[
        plan.input_source_capability_index,
        plan.pool_input_capability_index,
        plan.pool_output_capability_index,
        plan.output_destination_capability_index,
    ])?;
    if plan.exact_input_amount == 0 {
        return Err(engine_error(EngineError::InvalidSemanticPlan));
    }
    Ok(())
}

fn validate_partial_auction(plan: PartialAuctionPlan) -> EngineResult<()> {
    validate_state_position(plan.auction_state_position)?;
    validate_state_position(plan.order_state_position)?;
    if plan.auction_state_position == plan.order_state_position {
        return Err(engine_error(EngineError::InvalidSemanticPlan));
    }
    require_distinct_capabilities(&[
        plan.payment_source_capability_index,
        plan.payment_destination_capability_index,
        plan.inventory_source_capability_index,
        plan.inventory_destination_capability_index,
    ])?;
    if plan.fill_inventory_amount == 0 {
        return Err(engine_error(EngineError::InvalidSemanticPlan));
    }
    Ok(())
}

fn validate_batch_clearing(plan: BatchClearingPlan) -> EngineResult<()> {
    if plan.first_asset_index == plan.second_asset_index
        || usize::from(plan.first_asset_index) >= generic_effect_private_wire::MAX_ASSETS
        || usize::from(plan.second_asset_index) >= generic_effect_private_wire::MAX_ASSETS
        || plan.second_asset_per_first_numerator == 0
        || plan.nonzero_first_asset_denominator == 0
    {
        return Err(engine_error(EngineError::InvalidSemanticPlan));
    }
    Ok(())
}

fn validate_inventory_distribution(plan: InventoryDistributionPlan) -> EngineResult<()> {
    require_distinct_capabilities(&[
        plan.payment_source_capability_index,
        plan.seller_payment_capability_index,
        plan.creator_payment_capability_index,
        plan.inventory_source_capability_index,
        plan.inventory_destination_capability_index,
    ])?;
    let basis_points = u32::from(plan.seller_basis_points)
        .checked_add(u32::from(plan.creator_basis_points))
        .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?;
    if plan.inventory_quantity == 0
        || plan.payment_units_per_inventory_unit == 0
        || plan.seller_basis_points == 0
        || plan.creator_basis_points == 0
        || basis_points != u32::from(BASIS_POINTS_DENOMINATOR)
    {
        return Err(engine_error(EngineError::InvalidSemanticPlan));
    }
    Ok(())
}

fn require_distinct_capabilities(indices: &[u8]) -> EngineResult<()> {
    for (position, index) in indices.iter().enumerate() {
        if usize::from(*index) >= generic_effect_private_wire::MAX_SETTLEMENT_CAPABILITIES
            || indices[..position].contains(index)
        {
            return Err(engine_error(EngineError::InvalidSemanticPlan));
        }
    }
    Ok(())
}

fn validate_state_position(position: u8) -> EngineResult<()> {
    if usize::from(position) < MAX_OPAQUE_CAPABILITIES {
        Ok(())
    } else {
        Err(engine_error(EngineError::InvalidOpaquePosition))
    }
}

fn position_bit(position: u8) -> EngineResult<u8> {
    validate_state_position(position)?;
    1_u8.checked_shl(u32::from(position))
        .ok_or_else(|| engine_error(EngineError::InvalidOpaquePosition))
}

fn require_reserved_zero(bytes: &[u8]) -> EngineResult<()> {
    if bytes.iter().all(|byte| *byte == 0) {
        Ok(())
    } else {
        Err(engine_error(EngineError::InvalidPlanFlags))
    }
}

fn read_u16(payload: &[u8], offset: usize) -> EngineResult<u16> {
    let bytes = payload
        .get(offset..offset + 2)
        .ok_or_else(|| engine_error(EngineError::InvalidPlanLength))?;
    Ok(u16::from_le_bytes(bytes.try_into().map_err(|_| {
        engine_error(EngineError::InvalidPlanLength)
    })?))
}

fn read_u32(payload: &[u8], offset: usize) -> EngineResult<u32> {
    let bytes = payload
        .get(offset..offset + 4)
        .ok_or_else(|| engine_error(EngineError::InvalidPlanLength))?;
    Ok(u32::from_le_bytes(bytes.try_into().map_err(|_| {
        engine_error(EngineError::InvalidPlanLength)
    })?))
}

fn read_u64(payload: &[u8], offset: usize) -> EngineResult<u64> {
    let bytes = payload
        .get(offset..offset + 8)
        .ok_or_else(|| engine_error(EngineError::InvalidPlanLength))?;
    Ok(u64::from_le_bytes(bytes.try_into().map_err(|_| {
        engine_error(EngineError::InvalidPlanLength)
    })?))
}

fn validate_optional_position(position: u8) -> EngineResult<()> {
    if position != NONE_INDEX && usize::from(position) >= MAX_OPAQUE_CAPABILITIES {
        Err(engine_error(EngineError::InvalidOpaquePosition))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weighted_plan() -> WeightedAllocationPlan {
        WeightedAllocationPlan {
            source_capability_index: 0,
            first_destination_capability_index: 1,
            second_destination_capability_index: 2,
            total_amount: 10_000,
            first_weight: 3,
            second_weight: 1,
        }
    }

    fn constant_product_plan() -> ConstantProductPlan {
        ConstantProductPlan {
            state_position: 3,
            input_source_capability_index: 0,
            pool_input_capability_index: 1,
            pool_output_capability_index: 2,
            output_destination_capability_index: 3,
            exact_input_amount: 100_000,
        }
    }

    fn partial_auction_plan() -> PartialAuctionPlan {
        PartialAuctionPlan {
            auction_state_position: 1,
            order_state_position: 6,
            payment_source_capability_index: 0,
            payment_destination_capability_index: 1,
            inventory_source_capability_index: 2,
            inventory_destination_capability_index: 3,
            fill_inventory_amount: 10_001,
        }
    }

    fn batch_clearing_plan() -> BatchClearingPlan {
        BatchClearingPlan {
            first_asset_index: 0,
            second_asset_index: 1,
            second_asset_per_first_numerator: 2,
            nonzero_first_asset_denominator: 1,
        }
    }

    fn inventory_distribution_plan() -> InventoryDistributionPlan {
        InventoryDistributionPlan {
            payment_source_capability_index: 0,
            seller_payment_capability_index: 1,
            creator_payment_capability_index: 2,
            inventory_source_capability_index: 3,
            inventory_destination_capability_index: 4,
            inventory_quantity: 3,
            payment_units_per_inventory_unit: 1_000_000,
            seller_basis_points: 8_000,
            creator_basis_points: 2_000,
        }
    }

    fn reference_payloads() -> Vec<(Vec<u8>, usize, usize)> {
        vec![
            (
                encode_weighted_allocation_plan(RECEIPT_ACCEPT, weighted_plan()).unwrap(),
                PLAN_HEADER_LEN + WEIGHTED_ALLOCATION_BODY_LEN,
                0,
            ),
            (
                encode_constant_product_plan(RECEIPT_ACCEPT, constant_product_plan()).unwrap(),
                PLAN_HEADER_LEN + CONSTANT_PRODUCT_BODY_LEN,
                1,
            ),
            (
                encode_partial_auction_plan(RECEIPT_ACCEPT, partial_auction_plan()).unwrap(),
                PLAN_HEADER_LEN + PARTIAL_AUCTION_BODY_LEN,
                2,
            ),
            (
                encode_batch_clearing_plan(RECEIPT_ACCEPT, batch_clearing_plan()).unwrap(),
                PLAN_HEADER_LEN + BATCH_CLEARING_BODY_LEN,
                0,
            ),
            (
                encode_inventory_distribution_plan(RECEIPT_ACCEPT, inventory_distribution_plan())
                    .unwrap(),
                PLAN_HEADER_LEN + INVENTORY_DISTRIBUTION_BODY_LEN,
                0,
            ),
        ]
    }

    #[test]
    fn twelve_explicit_moves_fill_exact_payload_ceiling() {
        let moves: Vec<_> = (0..MAX_ENGINE_MOVES)
            .map(|index| PlannedMove {
                source_capability_index: index as u8,
                destination_capability_index: 11,
                amount: index as u64 + 1,
            })
            .collect();
        let encoded = encode_explicit_plan(0, 0, NONE_INDEX, NONE_INDEX, &moves).unwrap();
        assert_eq!(encoded.len(), 128);
        assert_eq!(EnginePlan::decode_exact(&encoded).unwrap().move_count, 12);
    }

    #[test]
    fn state_positions_are_a_bitmap_not_a_fixed_prefix() {
        let encoded = encode_context_fanout_plan(0, 0b1001_0010, 2, 6, 4, 7).unwrap();
        let plan = EnginePlan::decode_exact(&encoded).unwrap();
        assert_eq!(plan.state_position_bitmap, 0b1001_0010);
        assert_eq!(plan.helper_program_position_or_none, 2);
        assert_eq!(plan.helper_state_position_or_none, 6);
    }

    #[test]
    fn helper_state_cannot_also_be_engine_state() {
        let error = encode_context_fanout_plan(0, 1 << 6, 2, 6, 4, 7).unwrap_err();
        assert_eq!(error, engine_error(EngineError::AliasedHelperClosure));
    }

    #[test]
    fn descendant_setter_requires_helper_closure() {
        let error =
            encode_context_fanout_plan(RECEIPT_DESCENDANT_SETTER, 0, NONE_INDEX, NONE_INDEX, 1, 1)
                .unwrap_err();
        assert_eq!(error, engine_error(EngineError::IncompleteHelperClosure));
    }

    #[test]
    fn five_reference_plan_codecs_are_exact_and_below_the_private_ceiling() {
        for (payload, expected_len, expected_state_count) in reference_payloads() {
            assert_eq!(payload.len(), expected_len);
            assert!(payload.len() <= generic_effect_private_wire::MAX_OPAQUE_PAYLOAD_LEN);
            let decoded = EnginePlan::decode_exact(&payload).unwrap();
            assert_eq!(
                decoded.required_reference_state_count(),
                Some(expected_state_count)
            );
            assert_eq!(
                decoded.state_position_bitmap.count_ones() as usize,
                expected_state_count
            );
            assert!(!decoded.has_helper());
        }
    }

    #[test]
    fn reference_plan_round_trips_preserve_typed_bodies() {
        assert_eq!(
            EnginePlan::decode_exact(
                &encode_weighted_allocation_plan(RECEIPT_ACCEPT, weighted_plan()).unwrap()
            )
            .unwrap()
            .body,
            PlanBody::WeightedAllocation(weighted_plan())
        );
        assert_eq!(
            EnginePlan::decode_exact(
                &encode_constant_product_plan(RECEIPT_ACCEPT, constant_product_plan()).unwrap()
            )
            .unwrap()
            .body,
            PlanBody::ConstantProduct(constant_product_plan())
        );
        assert_eq!(
            EnginePlan::decode_exact(
                &encode_partial_auction_plan(RECEIPT_ACCEPT, partial_auction_plan()).unwrap()
            )
            .unwrap()
            .body,
            PlanBody::PartialAuction(partial_auction_plan())
        );
        assert_eq!(
            EnginePlan::decode_exact(
                &encode_batch_clearing_plan(RECEIPT_ACCEPT, batch_clearing_plan()).unwrap()
            )
            .unwrap()
            .body,
            PlanBody::BatchClearing(batch_clearing_plan())
        );
        assert_eq!(
            EnginePlan::decode_exact(
                &encode_inventory_distribution_plan(RECEIPT_ACCEPT, inventory_distribution_plan())
                    .unwrap()
            )
            .unwrap()
            .body,
            PlanBody::InventoryDistribution(inventory_distribution_plan())
        );
    }

    #[test]
    fn every_reference_plan_rejects_truncation_trailing_and_header_reserved_bytes() {
        for (payload, _, _) in reference_payloads() {
            let mut truncated = payload.clone();
            truncated.pop();
            assert_eq!(
                EnginePlan::decode_exact(&truncated).unwrap_err(),
                engine_error(EngineError::InvalidPlanLength)
            );

            let mut trailing = payload.clone();
            trailing.push(0);
            assert_eq!(
                EnginePlan::decode_exact(&trailing).unwrap_err(),
                engine_error(EngineError::InvalidPlanLength)
            );

            let mut header_reserved = payload;
            header_reserved[7] = 1;
            assert_eq!(
                EnginePlan::decode_exact(&header_reserved).unwrap_err(),
                engine_error(EngineError::InvalidPlanFlags)
            );
        }
    }

    #[test]
    fn every_reference_plan_reserved_body_byte_is_strictly_zero() {
        let cases = [
            (
                encode_weighted_allocation_plan(RECEIPT_ACCEPT, weighted_plan()).unwrap(),
                vec![11],
            ),
            (
                encode_constant_product_plan(RECEIPT_ACCEPT, constant_product_plan()).unwrap(),
                (13..16).collect(),
            ),
            (
                encode_partial_auction_plan(RECEIPT_ACCEPT, partial_auction_plan()).unwrap(),
                (14..16).collect(),
            ),
            (
                encode_batch_clearing_plan(RECEIPT_ACCEPT, batch_clearing_plan()).unwrap(),
                (10..16).collect(),
            ),
            (
                encode_inventory_distribution_plan(RECEIPT_ACCEPT, inventory_distribution_plan())
                    .unwrap(),
                (13..16).chain(36..40).collect(),
            ),
        ];
        for (payload, reserved_offsets) in cases {
            for offset in reserved_offsets {
                let mut noncanonical = payload.clone();
                noncanonical[offset] = 1;
                assert_eq!(
                    EnginePlan::decode_exact(&noncanonical).unwrap_err(),
                    engine_error(EngineError::InvalidPlanFlags)
                );
            }
        }
    }

    #[test]
    fn reference_headers_require_exact_move_bitmap_and_no_helper_closure() {
        for (payload, _, _) in reference_payloads() {
            let mut wrong_move_count = payload.clone();
            wrong_move_count[3] = wrong_move_count[3].saturating_sub(1);
            assert_eq!(
                EnginePlan::decode_exact(&wrong_move_count).unwrap_err(),
                engine_error(EngineError::InvalidSemanticPlan)
            );

            let mut wrong_bitmap = payload.clone();
            wrong_bitmap[4] ^= 0b1000_0000;
            assert_eq!(
                EnginePlan::decode_exact(&wrong_bitmap).unwrap_err(),
                engine_error(EngineError::InvalidSemanticPlan)
            );

            let mut helper = payload;
            helper[5] = 4;
            helper[6] = 5;
            assert_eq!(
                EnginePlan::decode_exact(&helper).unwrap_err(),
                engine_error(EngineError::InvalidSemanticPlan)
            );
        }
    }

    #[test]
    fn weighted_plan_rejects_alias_zero_amount_and_zero_weights() {
        let mut plan = weighted_plan();
        plan.second_destination_capability_index = plan.first_destination_capability_index;
        assert_eq!(
            encode_weighted_allocation_plan(RECEIPT_ACCEPT, plan).unwrap_err(),
            engine_error(EngineError::InvalidSemanticPlan)
        );
        for mutation in [
            WeightedAllocationPlan {
                total_amount: 0,
                ..weighted_plan()
            },
            WeightedAllocationPlan {
                first_weight: 0,
                ..weighted_plan()
            },
            WeightedAllocationPlan {
                second_weight: 0,
                ..weighted_plan()
            },
        ] {
            assert!(encode_weighted_allocation_plan(RECEIPT_ACCEPT, mutation).is_err());
        }
    }

    #[test]
    fn stateful_plans_reject_bad_positions_aliases_and_zero_amounts() {
        assert_eq!(
            encode_constant_product_plan(
                RECEIPT_ACCEPT,
                ConstantProductPlan {
                    state_position: MAX_OPAQUE_CAPABILITIES as u8,
                    ..constant_product_plan()
                }
            )
            .unwrap_err(),
            engine_error(EngineError::InvalidOpaquePosition)
        );
        assert!(encode_constant_product_plan(
            RECEIPT_ACCEPT,
            ConstantProductPlan {
                pool_output_capability_index: 1,
                ..constant_product_plan()
            }
        )
        .is_err());
        assert!(encode_constant_product_plan(
            RECEIPT_ACCEPT,
            ConstantProductPlan {
                exact_input_amount: 0,
                ..constant_product_plan()
            }
        )
        .is_err());

        assert!(encode_partial_auction_plan(
            RECEIPT_ACCEPT,
            PartialAuctionPlan {
                order_state_position: 1,
                ..partial_auction_plan()
            }
        )
        .is_err());
        assert!(encode_partial_auction_plan(
            RECEIPT_ACCEPT,
            PartialAuctionPlan {
                inventory_destination_capability_index: 2,
                ..partial_auction_plan()
            }
        )
        .is_err());
        assert!(encode_partial_auction_plan(
            RECEIPT_ACCEPT,
            PartialAuctionPlan {
                fill_inventory_amount: 0,
                ..partial_auction_plan()
            }
        )
        .is_err());
    }

    #[test]
    fn stateless_market_plans_reject_invalid_asset_and_distribution_parameters() {
        for mutation in [
            BatchClearingPlan {
                second_asset_index: 0,
                ..batch_clearing_plan()
            },
            BatchClearingPlan {
                first_asset_index: generic_effect_private_wire::MAX_ASSETS as u8,
                ..batch_clearing_plan()
            },
            BatchClearingPlan {
                second_asset_per_first_numerator: 0,
                ..batch_clearing_plan()
            },
            BatchClearingPlan {
                nonzero_first_asset_denominator: 0,
                ..batch_clearing_plan()
            },
        ] {
            assert!(encode_batch_clearing_plan(RECEIPT_ACCEPT, mutation).is_err());
        }

        for mutation in [
            InventoryDistributionPlan {
                creator_payment_capability_index: 1,
                ..inventory_distribution_plan()
            },
            InventoryDistributionPlan {
                inventory_quantity: 0,
                ..inventory_distribution_plan()
            },
            InventoryDistributionPlan {
                payment_units_per_inventory_unit: 0,
                ..inventory_distribution_plan()
            },
            InventoryDistributionPlan {
                seller_basis_points: 0,
                ..inventory_distribution_plan()
            },
            InventoryDistributionPlan {
                seller_basis_points: 7_999,
                ..inventory_distribution_plan()
            },
        ] {
            assert!(encode_inventory_distribution_plan(RECEIPT_ACCEPT, mutation).is_err());
        }
    }
}
