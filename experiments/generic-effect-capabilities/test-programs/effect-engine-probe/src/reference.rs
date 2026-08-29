//! Checked materialization for the five private reference semantics.
//!
//! This module is deliberately engine-local. It proves that one unchanged
//! generic Core can settle materially different policies while every policy
//! still reduces to the same canonical `MoveCandidateV0` effect rows.

use anchor_lang::solana_program::account_info::AccountInfo;
use generic_effect_private_wire::{
    EngineAssetRowCandidateV0, EngineContextRowCandidateV0, MoveCandidateV0, NONE_INDEX,
    RIGHT_CREDIT, RIGHT_DEBIT, RIGHT_DOMAIN_ACCOUNTED, RIGHT_EXACT_EXTERNAL_RECIPIENT,
};

use crate::{
    engine_error,
    plan::{
        BatchClearingPlan, ConstantProductPlan, EnginePlan, InventoryDistributionPlan,
        PartialAuctionPlan, PlanBody, WeightedAllocationPlan, BASIS_POINTS_DENOMINATOR,
    },
    reference_state::{
        auction_price_within_order_limit, AuctionStateCandidateV0, ConstantProductStateCandidateV0,
        OrderStateCandidateV0,
    },
    EngineError, EngineResult,
};

pub fn validate_reference_state_positions(
    plan: &EnginePlan,
    opaque_account_count: usize,
) -> EngineResult<()> {
    let Some(required_count) = plan.required_reference_state_count() else {
        return Ok(());
    };
    if plan.state_position_bitmap.count_ones() as usize != required_count {
        return Err(engine_error(EngineError::InvalidSemanticPlan));
    }
    for position in 0..8_u8 {
        if plan.state_position_bitmap & (1_u8 << position) != 0
            && usize::from(position) >= opaque_account_count
        {
            return Err(engine_error(EngineError::InvalidOpaquePosition));
        }
    }
    Ok(())
}

pub fn materialize_reference_moves(
    body: &PlanBody,
    assets: &[EngineAssetRowCandidateV0],
    contexts: &[EngineContextRowCandidateV0],
    opaque_accounts: &[AccountInfo<'_>],
) -> EngineResult<Vec<MoveCandidateV0>> {
    match body {
        PlanBody::WeightedAllocation(plan) => materialize_weighted_allocation(*plan, contexts),
        PlanBody::ConstantProduct(plan) => {
            let state =
                decode_constant_product_state(account_at(opaque_accounts, plan.state_position)?)?;
            materialize_constant_product(*plan, contexts, &state)
        }
        PlanBody::PartialAuction(plan) => {
            let auction =
                decode_auction_state(account_at(opaque_accounts, plan.auction_state_position)?)?;
            let order =
                decode_order_state(account_at(opaque_accounts, plan.order_state_position)?)?;
            materialize_partial_auction(*plan, contexts, &auction, &order)
        }
        PlanBody::BatchClearing(plan) => materialize_batch_clearing(*plan, contexts),
        PlanBody::InventoryDistribution(plan) => {
            materialize_inventory_distribution(*plan, assets, contexts)
        }
        PlanBody::Explicit(_) | PlanBody::ContextFanout { .. } => {
            Err(engine_error(EngineError::InvalidSemanticPlan))
        }
    }
}

pub fn mutate_reference_states(
    body: &PlanBody,
    opaque_accounts: &[AccountInfo<'_>],
    request_digest: [u8; 32],
    moves: &[MoveCandidateV0],
) -> EngineResult<Option<u64>> {
    match body {
        PlanBody::WeightedAllocation(_)
        | PlanBody::BatchClearing(_)
        | PlanBody::InventoryDistribution(_) => Ok(Some(0)),
        PlanBody::ConstantProduct(plan) => {
            let input = find_move(
                moves,
                plan.input_source_capability_index,
                plan.pool_input_capability_index,
            )?;
            let output = find_move(
                moves,
                plan.pool_output_capability_index,
                plan.output_destination_capability_index,
            )?;
            let account = account_at(opaque_accounts, plan.state_position)?;
            let next = decode_constant_product_state(account)?.advance(
                request_digest,
                input.amount,
                output.amount,
            )?;
            write_reference_state(account, &next.encode()?)?;
            Ok(Some(next.sequence))
        }
        PlanBody::PartialAuction(plan) => {
            let payment = find_move(
                moves,
                plan.payment_source_capability_index,
                plan.payment_destination_capability_index,
            )?;
            let inventory = find_move(
                moves,
                plan.inventory_source_capability_index,
                plan.inventory_destination_capability_index,
            )?;
            let auction_account = account_at(opaque_accounts, plan.auction_state_position)?;
            let order_account = account_at(opaque_accounts, plan.order_state_position)?;
            let auction = decode_auction_state(auction_account)?;
            let order = decode_order_state(order_account)?;
            auction_price_within_order_limit(&auction, &order)?;
            let next_auction = auction.advance(request_digest, inventory.amount)?;
            let next_order = order.advance(request_digest, payment.amount)?;
            if next_auction.sequence != next_order.sequence {
                return Err(engine_error(EngineError::StateSequenceMismatch));
            }
            let auction_bytes = next_auction.encode()?;
            let order_bytes = next_order.encode()?;
            write_reference_state(auction_account, &auction_bytes)?;
            write_reference_state(order_account, &order_bytes)?;
            Ok(Some(next_auction.sequence))
        }
        PlanBody::Explicit(_) | PlanBody::ContextFanout { .. } => Ok(None),
    }
}

pub fn constant_product_output(
    reserve_input: u64,
    reserve_output: u64,
    exact_input: u64,
    fee_numerator: u64,
    nonzero_fee_denominator: u64,
) -> EngineResult<u64> {
    if reserve_input == 0
        || reserve_output == 0
        || exact_input == 0
        || nonzero_fee_denominator == 0
        || fee_numerator >= nonzero_fee_denominator
    {
        return Err(engine_error(EngineError::InvalidSemanticContext));
    }
    let retained_numerator = nonzero_fee_denominator
        .checked_sub(fee_numerator)
        .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?;
    let adjusted_input = u128::from(exact_input)
        .checked_mul(u128::from(retained_numerator))
        .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?;
    let denominator = u128::from(reserve_input)
        .checked_mul(u128::from(nonzero_fee_denominator))
        .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?
        .checked_add(adjusted_input)
        .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?;
    let numerator = u128::from(reserve_output)
        .checked_mul(adjusted_input)
        .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?;
    let output = u64::try_from(numerator / denominator)
        .map_err(|_| engine_error(EngineError::ArithmeticOverflow))?;
    if output == 0 || output >= reserve_output {
        return Err(engine_error(EngineError::InvalidSemanticContext));
    }
    Ok(output)
}

pub fn auction_payment(
    inventory_amount: u64,
    unit_price_numerator: u64,
    nonzero_unit_price_denominator: u64,
) -> EngineResult<u64> {
    checked_ceil_mul_div(
        inventory_amount,
        unit_price_numerator,
        nonzero_unit_price_denominator,
    )
}

pub fn auction_payment_delta(
    already_filled_inventory: u64,
    fill_inventory: u64,
    unit_price_numerator: u64,
    nonzero_unit_price_denominator: u64,
) -> EngineResult<u64> {
    let filled_after = already_filled_inventory
        .checked_add(fill_inventory)
        .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?;
    let payment_before = if already_filled_inventory == 0 {
        0
    } else {
        auction_payment(
            already_filled_inventory,
            unit_price_numerator,
            nonzero_unit_price_denominator,
        )?
    };
    let payment_after = auction_payment(
        filled_after,
        unit_price_numerator,
        nonzero_unit_price_denominator,
    )?;
    let payment_delta = payment_after
        .checked_sub(payment_before)
        .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?;
    if payment_delta == 0 {
        Err(engine_error(EngineError::InvalidSemanticContext))
    } else {
        Ok(payment_delta)
    }
}

pub fn inventory_distribution_amounts(
    inventory_quantity: u64,
    payment_units_per_inventory_unit: u64,
    seller_basis_points: u16,
    creator_basis_points: u16,
) -> EngineResult<(u64, u64, u64)> {
    if inventory_quantity == 0
        || payment_units_per_inventory_unit == 0
        || seller_basis_points == 0
        || creator_basis_points == 0
        || u32::from(seller_basis_points) + u32::from(creator_basis_points)
            != u32::from(BASIS_POINTS_DENOMINATOR)
    {
        return Err(engine_error(EngineError::InvalidSemanticContext));
    }
    let total = u64::try_from(
        u128::from(inventory_quantity)
            .checked_mul(u128::from(payment_units_per_inventory_unit))
            .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?,
    )
    .map_err(|_| engine_error(EngineError::ArithmeticOverflow))?;
    let seller = checked_floor_mul_div(
        total,
        u64::from(seller_basis_points),
        u64::from(BASIS_POINTS_DENOMINATOR),
    )?;
    let creator = total
        .checked_sub(seller)
        .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?;
    if seller == 0 || creator == 0 {
        return Err(engine_error(EngineError::InvalidSemanticContext));
    }
    Ok((total, seller, creator))
}

fn materialize_weighted_allocation(
    plan: WeightedAllocationPlan,
    contexts: &[EngineContextRowCandidateV0],
) -> EngineResult<Vec<MoveCandidateV0>> {
    let source = context_at(contexts, plan.source_capability_index)?;
    let first = context_at(contexts, plan.first_destination_capability_index)?;
    let second = context_at(contexts, plan.second_destination_capability_index)?;
    require_rights(source, RIGHT_DEBIT)?;
    require_rights(first, RIGHT_EXACT_EXTERNAL_RECIPIENT | RIGHT_CREDIT)?;
    require_rights(second, RIGHT_EXACT_EXTERNAL_RECIPIENT | RIGHT_CREDIT)?;
    require_same_asset(&[source, first, second])?;
    require_debit_capacity(source, plan.total_amount)?;

    let total_weight = u64::from(plan.first_weight)
        .checked_add(u64::from(plan.second_weight))
        .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?;
    let first_amount = checked_floor_mul_div(
        plan.total_amount,
        u64::from(plan.first_weight),
        total_weight,
    )?;
    let second_amount = plan
        .total_amount
        .checked_sub(first_amount)
        .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?;
    if first_amount == 0 || second_amount == 0 {
        return Err(engine_error(EngineError::InvalidSemanticContext));
    }
    sorted_moves(vec![
        movement(source, first, first_amount),
        movement(source, second, second_amount),
    ])
}

fn materialize_constant_product(
    plan: ConstantProductPlan,
    contexts: &[EngineContextRowCandidateV0],
    state: &ConstantProductStateCandidateV0,
) -> EngineResult<Vec<MoveCandidateV0>> {
    let input_source = context_at(contexts, plan.input_source_capability_index)?;
    let pool_input = context_at(contexts, plan.pool_input_capability_index)?;
    let pool_output = context_at(contexts, plan.pool_output_capability_index)?;
    let output_destination = context_at(contexts, plan.output_destination_capability_index)?;
    require_rights(input_source, RIGHT_DEBIT)?;
    require_rights(pool_input, RIGHT_DOMAIN_ACCOUNTED | RIGHT_CREDIT)?;
    require_rights(pool_output, RIGHT_DOMAIN_ACCOUNTED | RIGHT_DEBIT)?;
    require_rights(
        output_destination,
        RIGHT_EXACT_EXTERNAL_RECIPIENT | RIGHT_CREDIT,
    )?;
    require_asset(input_source, state.input_asset_index)?;
    require_asset(pool_input, state.input_asset_index)?;
    require_asset(pool_output, state.output_asset_index)?;
    require_asset(output_destination, state.output_asset_index)?;
    require_shared_domain(pool_input, pool_output)?;
    require_debit_capacity(input_source, plan.exact_input_amount)?;
    let output_amount = constant_product_output(
        pool_input.accounted_before_or_zero,
        pool_output.accounted_before_or_zero,
        plan.exact_input_amount,
        state.swap_fee_numerator,
        state.nonzero_swap_fee_denominator,
    )?;
    require_debit_capacity(pool_output, output_amount)?;
    sorted_moves(vec![
        movement(input_source, pool_input, plan.exact_input_amount),
        movement(pool_output, output_destination, output_amount),
    ])
}

fn materialize_partial_auction(
    plan: PartialAuctionPlan,
    contexts: &[EngineContextRowCandidateV0],
    auction: &AuctionStateCandidateV0,
    order: &OrderStateCandidateV0,
) -> EngineResult<Vec<MoveCandidateV0>> {
    auction_price_within_order_limit(auction, order)?;
    if plan.fill_inventory_amount > auction.remaining_inventory {
        return Err(engine_error(EngineError::InvalidSemanticContext));
    }
    let payment_amount = auction_payment_delta(
        auction.filled_inventory,
        plan.fill_inventory_amount,
        auction.unit_price_numerator,
        auction.nonzero_unit_price_denominator,
    )?;
    if payment_amount > order.remaining_payment {
        return Err(engine_error(EngineError::InvalidSemanticContext));
    }

    let payment_source = context_at(contexts, plan.payment_source_capability_index)?;
    let payment_destination = context_at(contexts, plan.payment_destination_capability_index)?;
    let inventory_source = context_at(contexts, plan.inventory_source_capability_index)?;
    let inventory_destination = context_at(contexts, plan.inventory_destination_capability_index)?;
    require_rights(payment_source, RIGHT_DEBIT)?;
    require_rights(payment_destination, RIGHT_DOMAIN_ACCOUNTED | RIGHT_CREDIT)?;
    require_rights(inventory_source, RIGHT_DOMAIN_ACCOUNTED | RIGHT_DEBIT)?;
    require_rights(
        inventory_destination,
        RIGHT_EXACT_EXTERNAL_RECIPIENT | RIGHT_CREDIT,
    )?;
    require_asset(payment_source, auction.payment_asset_index)?;
    require_asset(payment_destination, auction.payment_asset_index)?;
    require_asset(inventory_source, auction.inventory_asset_index)?;
    require_asset(inventory_destination, auction.inventory_asset_index)?;
    require_shared_domain(payment_destination, inventory_source)?;
    require_debit_capacity(payment_source, payment_amount)?;
    require_debit_capacity(inventory_source, plan.fill_inventory_amount)?;
    sorted_moves(vec![
        movement(payment_source, payment_destination, payment_amount),
        movement(
            inventory_source,
            inventory_destination,
            plan.fill_inventory_amount,
        ),
    ])
}

fn materialize_batch_clearing(
    plan: BatchClearingPlan,
    contexts: &[EngineContextRowCandidateV0],
) -> EngineResult<Vec<MoveCandidateV0>> {
    let first_source = unique_asset_role(contexts, plan.first_asset_index, RIGHT_DEBIT)?;
    let first_destination = unique_asset_role(
        contexts,
        plan.first_asset_index,
        RIGHT_EXACT_EXTERNAL_RECIPIENT | RIGHT_CREDIT,
    )?;
    let second_source = unique_asset_role(contexts, plan.second_asset_index, RIGHT_DEBIT)?;
    let second_destination = unique_asset_role(
        contexts,
        plan.second_asset_index,
        RIGHT_EXACT_EXTERNAL_RECIPIENT | RIGHT_CREDIT,
    )?;
    let first_amount = first_source.remaining_maximum_engine_debit;
    let second_amount = second_source.remaining_maximum_engine_debit;
    if first_amount == 0 || second_amount == 0 {
        return Err(engine_error(EngineError::InvalidSemanticContext));
    }
    let offered = u128::from(second_amount)
        .checked_mul(u128::from(plan.nonzero_first_asset_denominator))
        .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?;
    let required = u128::from(first_amount)
        .checked_mul(u128::from(plan.second_asset_per_first_numerator))
        .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?;
    if offered != required {
        return Err(engine_error(EngineError::InvalidSemanticContext));
    }
    sorted_moves(vec![
        movement(first_source, first_destination, first_amount),
        movement(second_source, second_destination, second_amount),
    ])
}

fn materialize_inventory_distribution(
    plan: InventoryDistributionPlan,
    assets: &[EngineAssetRowCandidateV0],
    contexts: &[EngineContextRowCandidateV0],
) -> EngineResult<Vec<MoveCandidateV0>> {
    let payment_source = context_at(contexts, plan.payment_source_capability_index)?;
    let seller_payment = context_at(contexts, plan.seller_payment_capability_index)?;
    let creator_payment = context_at(contexts, plan.creator_payment_capability_index)?;
    let inventory_source = context_at(contexts, plan.inventory_source_capability_index)?;
    let inventory_destination = context_at(contexts, plan.inventory_destination_capability_index)?;
    require_rights(payment_source, RIGHT_DEBIT)?;
    require_rights(seller_payment, RIGHT_DOMAIN_ACCOUNTED | RIGHT_CREDIT)?;
    require_rights(
        creator_payment,
        RIGHT_EXACT_EXTERNAL_RECIPIENT | RIGHT_CREDIT,
    )?;
    require_rights(inventory_source, RIGHT_DOMAIN_ACCOUNTED | RIGHT_DEBIT)?;
    require_rights(
        inventory_destination,
        RIGHT_EXACT_EXTERNAL_RECIPIENT | RIGHT_CREDIT,
    )?;
    require_same_asset(&[payment_source, seller_payment, creator_payment])?;
    require_same_asset(&[inventory_source, inventory_destination])?;
    if payment_source.asset_index == inventory_source.asset_index {
        return Err(engine_error(EngineError::InvalidSemanticContext));
    }
    require_shared_domain(seller_payment, inventory_source)?;
    let inventory_asset = assets
        .get(usize::from(inventory_source.asset_index))
        .ok_or_else(|| engine_error(EngineError::InvalidSemanticContext))?;
    if inventory_asset.asset_index != inventory_source.asset_index || inventory_asset.decimals != 0
    {
        return Err(engine_error(EngineError::InvalidSemanticContext));
    }
    let (total, seller_amount, creator_amount) = inventory_distribution_amounts(
        plan.inventory_quantity,
        plan.payment_units_per_inventory_unit,
        plan.seller_basis_points,
        plan.creator_basis_points,
    )?;
    require_debit_capacity(payment_source, total)?;
    require_debit_capacity(inventory_source, plan.inventory_quantity)?;
    sorted_moves(vec![
        movement(payment_source, seller_payment, seller_amount),
        movement(payment_source, creator_payment, creator_amount),
        movement(
            inventory_source,
            inventory_destination,
            plan.inventory_quantity,
        ),
    ])
}

fn checked_floor_mul_div(value: u64, numerator: u64, denominator: u64) -> EngineResult<u64> {
    if denominator == 0 {
        return Err(engine_error(EngineError::InvalidSemanticContext));
    }
    u64::try_from(
        u128::from(value)
            .checked_mul(u128::from(numerator))
            .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?
            / u128::from(denominator),
    )
    .map_err(|_| engine_error(EngineError::ArithmeticOverflow))
}

fn checked_ceil_mul_div(value: u64, numerator: u64, denominator: u64) -> EngineResult<u64> {
    if value == 0 || numerator == 0 || denominator == 0 {
        return Err(engine_error(EngineError::InvalidSemanticContext));
    }
    let product = u128::from(value)
        .checked_mul(u128::from(numerator))
        .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?;
    let rounded = product
        .checked_add(u128::from(denominator) - 1)
        .ok_or_else(|| engine_error(EngineError::ArithmeticOverflow))?
        / u128::from(denominator);
    let amount =
        u64::try_from(rounded).map_err(|_| engine_error(EngineError::ArithmeticOverflow))?;
    if amount == 0 {
        Err(engine_error(EngineError::InvalidSemanticContext))
    } else {
        Ok(amount)
    }
}

fn context_at(
    contexts: &[EngineContextRowCandidateV0],
    capability_index: u8,
) -> EngineResult<&EngineContextRowCandidateV0> {
    let mut matches = contexts
        .iter()
        .filter(|row| row.settlement_capability_index == capability_index);
    let context = matches
        .next()
        .ok_or_else(|| engine_error(EngineError::InvalidSemanticContext))?;
    if matches.next().is_some() {
        return Err(engine_error(EngineError::InvalidSemanticContext));
    }
    Ok(context)
}

fn unique_asset_role(
    contexts: &[EngineContextRowCandidateV0],
    asset_index: u8,
    rights: u16,
) -> EngineResult<&EngineContextRowCandidateV0> {
    let mut matches = contexts
        .iter()
        .filter(|row| row.asset_index == asset_index && row.rights_bits == rights);
    let context = matches
        .next()
        .ok_or_else(|| engine_error(EngineError::InvalidSemanticContext))?;
    if matches.next().is_some() {
        return Err(engine_error(EngineError::InvalidSemanticContext));
    }
    Ok(context)
}

fn require_rights(context: &EngineContextRowCandidateV0, rights: u16) -> EngineResult<()> {
    if context.rights_bits == rights {
        Ok(())
    } else {
        Err(engine_error(EngineError::InvalidSemanticContext))
    }
}

fn require_asset(context: &EngineContextRowCandidateV0, asset_index: u8) -> EngineResult<()> {
    if context.asset_index == asset_index {
        Ok(())
    } else {
        Err(engine_error(EngineError::InvalidSemanticContext))
    }
}

fn require_same_asset(contexts: &[&EngineContextRowCandidateV0]) -> EngineResult<()> {
    if contexts
        .windows(2)
        .all(|pair| pair[0].asset_index == pair[1].asset_index)
    {
        Ok(())
    } else {
        Err(engine_error(EngineError::InvalidSemanticContext))
    }
}

fn require_shared_domain(
    first: &EngineContextRowCandidateV0,
    second: &EngineContextRowCandidateV0,
) -> EngineResult<()> {
    if first.domain_index_or_none != NONE_INDEX
        && first.domain_index_or_none == second.domain_index_or_none
    {
        Ok(())
    } else {
        Err(engine_error(EngineError::InvalidSemanticContext))
    }
}

fn require_debit_capacity(context: &EngineContextRowCandidateV0, amount: u64) -> EngineResult<()> {
    if amount != 0 && amount <= context.remaining_maximum_engine_debit {
        Ok(())
    } else {
        Err(engine_error(EngineError::InvalidSemanticContext))
    }
}

fn movement(
    source: &EngineContextRowCandidateV0,
    destination: &EngineContextRowCandidateV0,
    amount: u64,
) -> MoveCandidateV0 {
    MoveCandidateV0 {
        source_capability_index: source.settlement_capability_index,
        destination_capability_index: destination.settlement_capability_index,
        amount,
    }
}

fn sorted_moves(mut moves: Vec<MoveCandidateV0>) -> EngineResult<Vec<MoveCandidateV0>> {
    moves.sort_by_key(|movement| {
        (
            movement.source_capability_index,
            movement.destination_capability_index,
        )
    });
    if moves.iter().any(|movement| movement.amount == 0) {
        return Err(engine_error(EngineError::InvalidSemanticContext));
    }
    Ok(moves)
}

fn find_move(
    moves: &[MoveCandidateV0],
    source_capability_index: u8,
    destination_capability_index: u8,
) -> EngineResult<&MoveCandidateV0> {
    let mut matches = moves.iter().filter(|movement| {
        movement.source_capability_index == source_capability_index
            && movement.destination_capability_index == destination_capability_index
    });
    let movement = matches
        .next()
        .ok_or_else(|| engine_error(EngineError::InvalidSemanticContext))?;
    if matches.next().is_some() {
        return Err(engine_error(EngineError::InvalidSemanticContext));
    }
    Ok(movement)
}

fn account_at<'slice, 'info>(
    accounts: &'slice [AccountInfo<'info>],
    position: u8,
) -> EngineResult<&'slice AccountInfo<'info>> {
    accounts
        .get(usize::from(position))
        .ok_or_else(|| engine_error(EngineError::InvalidOpaquePosition))
}

fn validate_reference_state_capability(account: &AccountInfo<'_>) -> EngineResult<()> {
    if *account.owner == crate::ID
        && account.is_writable
        && !account.is_signer
        && !account.executable
    {
        Ok(())
    } else {
        Err(engine_error(EngineError::InvalidReferenceStateCapability))
    }
}

fn decode_constant_product_state(
    account: &AccountInfo<'_>,
) -> EngineResult<ConstantProductStateCandidateV0> {
    validate_reference_state_capability(account)?;
    let data = account
        .try_borrow_data()
        .map_err(|_| engine_error(EngineError::AccountBorrowFailed))?;
    ConstantProductStateCandidateV0::decode_exact(&data)
}

fn decode_auction_state(account: &AccountInfo<'_>) -> EngineResult<AuctionStateCandidateV0> {
    validate_reference_state_capability(account)?;
    let data = account
        .try_borrow_data()
        .map_err(|_| engine_error(EngineError::AccountBorrowFailed))?;
    AuctionStateCandidateV0::decode_exact(&data)
}

fn decode_order_state(account: &AccountInfo<'_>) -> EngineResult<OrderStateCandidateV0> {
    validate_reference_state_capability(account)?;
    let data = account
        .try_borrow_data()
        .map_err(|_| engine_error(EngineError::AccountBorrowFailed))?;
    OrderStateCandidateV0::decode_exact(&data)
}

fn write_reference_state(account: &AccountInfo<'_>, encoded: &[u8]) -> EngineResult<()> {
    validate_reference_state_capability(account)?;
    let mut data = account
        .try_borrow_mut_data()
        .map_err(|_| engine_error(EngineError::AccountBorrowFailed))?;
    if data.len() != encoded.len() {
        return Err(engine_error(EngineError::InvalidReferenceState));
    }
    data.copy_from_slice(encoded);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use generic_effect_private_wire::{FEE_CLASS_GROSS_DEBIT_RATE, FEE_CLASS_NONE};

    fn context(
        index: u8,
        asset: u8,
        rights: u16,
        domain: u8,
        accounted: u64,
        maximum_debit: u64,
    ) -> EngineContextRowCandidateV0 {
        EngineContextRowCandidateV0 {
            settlement_capability_index: index,
            asset_index: asset,
            domain_index_or_none: domain,
            authorization_slot_or_none: if rights == RIGHT_DEBIT { 0 } else { NONE_INDEX },
            rights_bits: rights,
            fee_class: if rights == RIGHT_DEBIT {
                FEE_CLASS_GROSS_DEBIT_RATE
            } else {
                FEE_CLASS_NONE
            },
            context_flags: 0,
            endpoint_key: [index.saturating_add(1); 32],
            observed_before: accounted,
            accounted_before_or_zero: accounted,
            remaining_maximum_engine_debit: maximum_debit,
            remaining_maximum_total_debit: maximum_debit,
            remaining_minimum_credit: 0,
            remaining_maximum_protocol_fee: 0,
        }
    }

    fn asset(index: u8, decimals: u8) -> EngineAssetRowCandidateV0 {
        EngineAssetRowCandidateV0 {
            asset_index: index,
            asset_flags: 0,
            decimals,
            reserved: 0,
            asset_identity: [index.saturating_add(1); 32],
            asset_program: [20 + index; 32],
            settlement_profile_digest: [40 + index; 32],
        }
    }

    #[test]
    fn checked_formula_vectors_are_exact() {
        assert_eq!(
            constant_product_output(1_000_000, 2_000_000, 100_000, 3, 1_000).unwrap(),
            181_322
        );
        assert_eq!(
            constant_product_output(1_000, 1_000, 2, 3, 1_000).unwrap(),
            1
        );
        assert_eq!(auction_payment(10_001, 2, 1).unwrap(), 20_002);
        assert_eq!(auction_payment(3, 2, 3).unwrap(), 2);
        assert_eq!(auction_payment_delta(0, 1, 2, 3).unwrap(), 1);
        assert_eq!(auction_payment_delta(1, 2, 2, 3).unwrap(), 1);
        assert_eq!(auction_payment_delta(0, 3, 2, 3).unwrap(), 2);
        assert_eq!(
            inventory_distribution_amounts(3, 1_000_000, 8_000, 2_000).unwrap(),
            (3_000_000, 2_400_000, 600_000)
        );
    }

    #[test]
    fn formula_edges_reject_zero_and_u64_overflow() {
        assert!(constant_product_output(0, 1, 1, 0, 1).is_err());
        assert!(constant_product_output(1, 1, 1, 1, 1).is_err());
        assert!(auction_payment(u64::MAX, u64::MAX, 1).is_err());
        assert!(auction_payment_delta(u64::MAX, 1, 1, 1).is_err());
        assert!(auction_payment_delta(1, 1, 1, 100).is_err());
        assert!(inventory_distribution_amounts(u64::MAX, 2, 8_000, 2_000).is_err());
    }

    #[test]
    fn weighted_and_explicit_materialize_identical_move_bytes() {
        let contexts = vec![
            context(0, 0, RIGHT_DEBIT, NONE_INDEX, 0, 10_000),
            context(
                1,
                0,
                RIGHT_EXACT_EXTERNAL_RECIPIENT | RIGHT_CREDIT,
                NONE_INDEX,
                0,
                0,
            ),
            context(
                2,
                0,
                RIGHT_EXACT_EXTERNAL_RECIPIENT | RIGHT_CREDIT,
                NONE_INDEX,
                0,
                0,
            ),
        ];
        let weighted = materialize_weighted_allocation(
            WeightedAllocationPlan {
                source_capability_index: 0,
                first_destination_capability_index: 1,
                second_destination_capability_index: 2,
                total_amount: 10_000,
                first_weight: 3,
                second_weight: 1,
            },
            &contexts,
        )
        .unwrap();
        let explicit = [
            MoveCandidateV0 {
                source_capability_index: 0,
                destination_capability_index: 1,
                amount: 7_500,
            },
            MoveCandidateV0 {
                source_capability_index: 0,
                destination_capability_index: 2,
                amount: 2_500,
            },
        ];
        let weighted_bytes: Vec<_> = weighted.iter().flat_map(MoveCandidateV0::encode).collect();
        let explicit_bytes: Vec<_> = explicit.iter().flat_map(MoveCandidateV0::encode).collect();
        assert_eq!(weighted_bytes, explicit_bytes);
    }

    #[test]
    fn reference_state_bitmap_count_and_positions_are_enforced() {
        let constant_product = crate::plan::EnginePlan::decode_exact(
            &crate::plan::encode_constant_product_plan(
                crate::plan::RECEIPT_ACCEPT,
                ConstantProductPlan {
                    state_position: 3,
                    input_source_capability_index: 0,
                    pool_input_capability_index: 1,
                    pool_output_capability_index: 2,
                    output_destination_capability_index: 3,
                    exact_input_amount: 1,
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert!(validate_reference_state_positions(&constant_product, 3).is_err());
        validate_reference_state_positions(&constant_product, 4).unwrap();

        let mut extra_state = constant_product;
        extra_state.state_position_bitmap |= 1;
        assert_eq!(
            validate_reference_state_positions(&extra_state, 4).unwrap_err(),
            engine_error(EngineError::InvalidSemanticPlan)
        );

        let auction = crate::plan::EnginePlan::decode_exact(
            &crate::plan::encode_partial_auction_plan(
                crate::plan::RECEIPT_ACCEPT,
                PartialAuctionPlan {
                    auction_state_position: 1,
                    order_state_position: 6,
                    payment_source_capability_index: 0,
                    payment_destination_capability_index: 1,
                    inventory_source_capability_index: 2,
                    inventory_destination_capability_index: 3,
                    fill_inventory_amount: 1,
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert!(validate_reference_state_positions(&auction, 6).is_err());
        validate_reference_state_positions(&auction, 7).unwrap();
    }

    #[test]
    fn constant_product_uses_accounted_reserves_and_exact_roles() {
        let contexts = vec![
            context(0, 0, RIGHT_DEBIT, NONE_INDEX, 0, 100_000),
            context(1, 0, RIGHT_DOMAIN_ACCOUNTED | RIGHT_CREDIT, 0, 1_000_000, 0),
            context(
                2,
                1,
                RIGHT_DOMAIN_ACCOUNTED | RIGHT_DEBIT,
                0,
                2_000_000,
                2_000_000,
            ),
            context(
                3,
                1,
                RIGHT_EXACT_EXTERNAL_RECIPIENT | RIGHT_CREDIT,
                NONE_INDEX,
                0,
                0,
            ),
        ];
        let state = ConstantProductStateCandidateV0 {
            sequence: 0,
            input_asset_index: 0,
            output_asset_index: 1,
            swap_fee_numerator: 3,
            nonzero_swap_fee_denominator: 1_000,
            last_request_digest: [0; 32],
            last_input_amount: 0,
            last_output_amount: 0,
        };
        let moves = materialize_constant_product(
            ConstantProductPlan {
                state_position: 0,
                input_source_capability_index: 0,
                pool_input_capability_index: 1,
                pool_output_capability_index: 2,
                output_destination_capability_index: 3,
                exact_input_amount: 100_000,
            },
            &contexts,
            &state,
        )
        .unwrap();
        assert_eq!(moves[0].amount, 100_000);
        assert_eq!(moves[1].amount, 181_322);

        let mut malformed = contexts;
        malformed[2].domain_index_or_none = 1;
        assert!(materialize_constant_product(
            ConstantProductPlan {
                state_position: 0,
                input_source_capability_index: 0,
                pool_input_capability_index: 1,
                pool_output_capability_index: 2,
                output_destination_capability_index: 3,
                exact_input_amount: 100_000,
            },
            &malformed,
            &state,
        )
        .is_err());
    }

    #[test]
    fn partial_fill_checks_price_inventory_payment_and_domains() {
        let contexts = vec![
            context(0, 0, RIGHT_DEBIT, NONE_INDEX, 0, 20_002),
            context(1, 0, RIGHT_DOMAIN_ACCOUNTED | RIGHT_CREDIT, 0, 0, 0),
            context(
                2,
                1,
                RIGHT_DOMAIN_ACCOUNTED | RIGHT_DEBIT,
                0,
                30_000,
                30_000,
            ),
            context(
                3,
                1,
                RIGHT_EXACT_EXTERNAL_RECIPIENT | RIGHT_CREDIT,
                NONE_INDEX,
                0,
                0,
            ),
        ];
        let auction = AuctionStateCandidateV0 {
            sequence: 0,
            payment_asset_index: 0,
            inventory_asset_index: 1,
            unit_price_numerator: 2,
            nonzero_unit_price_denominator: 1,
            remaining_inventory: 30_000,
            filled_inventory: 0,
            last_request_digest: [0; 32],
        };
        let order = OrderStateCandidateV0 {
            sequence: 0,
            payment_asset_index: 0,
            inventory_asset_index: 1,
            maximum_unit_price_numerator: 2,
            nonzero_maximum_unit_price_denominator: 1,
            remaining_payment: 20_002,
            paid_payment: 0,
            last_request_digest: [0; 32],
        };
        let plan = PartialAuctionPlan {
            auction_state_position: 0,
            order_state_position: 1,
            payment_source_capability_index: 0,
            payment_destination_capability_index: 1,
            inventory_source_capability_index: 2,
            inventory_destination_capability_index: 3,
            fill_inventory_amount: 10_001,
        };
        let moves = materialize_partial_auction(plan, &contexts, &auction, &order).unwrap();
        assert_eq!(moves[0].amount, 20_002);
        assert_eq!(moves[1].amount, 10_001);

        let mut too_small = order;
        too_small.remaining_payment = 20_001;
        assert!(materialize_partial_auction(plan, &contexts, &auction, &too_small).is_err());
    }

    #[test]
    fn four_actor_batch_is_exact_and_rejects_ambiguity_or_bad_ratio() {
        let contexts = vec![
            context(0, 0, RIGHT_DEBIT, NONE_INDEX, 0, 10_001),
            context(
                1,
                0,
                RIGHT_EXACT_EXTERNAL_RECIPIENT | RIGHT_CREDIT,
                NONE_INDEX,
                0,
                0,
            ),
            context(2, 1, RIGHT_DEBIT, NONE_INDEX, 0, 20_002),
            context(
                3,
                1,
                RIGHT_EXACT_EXTERNAL_RECIPIENT | RIGHT_CREDIT,
                NONE_INDEX,
                0,
                0,
            ),
        ];
        let plan = BatchClearingPlan {
            first_asset_index: 0,
            second_asset_index: 1,
            second_asset_per_first_numerator: 2,
            nonzero_first_asset_denominator: 1,
        };
        let moves = materialize_batch_clearing(plan, &contexts).unwrap();
        assert_eq!(
            moves
                .iter()
                .map(|movement| movement.amount)
                .collect::<Vec<_>>(),
            vec![10_001, 20_002]
        );

        let mut wrong_ratio = contexts.clone();
        wrong_ratio[2].remaining_maximum_engine_debit = 20_001;
        assert!(materialize_batch_clearing(plan, &wrong_ratio).is_err());
        let mut ambiguous = contexts;
        ambiguous.push(context(4, 0, RIGHT_DEBIT, NONE_INDEX, 0, 10_001));
        assert!(materialize_batch_clearing(plan, &ambiguous).is_err());
    }

    #[test]
    fn zero_decimal_inventory_distribution_materializes_three_moves() {
        let assets = vec![asset(0, 6), asset(1, 0)];
        let contexts = vec![
            context(0, 0, RIGHT_DEBIT, NONE_INDEX, 0, 3_000_000),
            context(1, 0, RIGHT_DOMAIN_ACCOUNTED | RIGHT_CREDIT, 0, 0, 0),
            context(
                2,
                0,
                RIGHT_EXACT_EXTERNAL_RECIPIENT | RIGHT_CREDIT,
                NONE_INDEX,
                0,
                0,
            ),
            context(3, 1, RIGHT_DOMAIN_ACCOUNTED | RIGHT_DEBIT, 0, 3, 3),
            context(
                4,
                1,
                RIGHT_EXACT_EXTERNAL_RECIPIENT | RIGHT_CREDIT,
                NONE_INDEX,
                0,
                0,
            ),
        ];
        let plan = InventoryDistributionPlan {
            payment_source_capability_index: 0,
            seller_payment_capability_index: 1,
            creator_payment_capability_index: 2,
            inventory_source_capability_index: 3,
            inventory_destination_capability_index: 4,
            inventory_quantity: 3,
            payment_units_per_inventory_unit: 1_000_000,
            seller_basis_points: 8_000,
            creator_basis_points: 2_000,
        };
        let moves = materialize_inventory_distribution(plan, &assets, &contexts).unwrap();
        assert_eq!(
            moves
                .iter()
                .map(|movement| movement.amount)
                .collect::<Vec<_>>(),
            vec![2_400_000, 600_000, 3]
        );

        let mut fractional_inventory = assets;
        fractional_inventory[1].decimals = 1;
        assert!(
            materialize_inventory_distribution(plan, &fractional_inventory, &contexts).is_err()
        );
    }
}
