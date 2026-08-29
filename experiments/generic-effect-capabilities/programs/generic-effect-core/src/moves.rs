//! Canonical Move normal form, exact conservation, and local accounting plans.

use anchor_lang::prelude::*;
use generic_effect_private_wire::{
    compute_observed_protected_delta_set_root, ObservedProtectedDeltaRowCandidateV0,
};

use crate::{
    capabilities::{AssetProfileIdentity, SettlementCapability},
    constants::{AUTHORITY_DOMAIN_ACCOUNTED, MAX_ENGINE_MOVES, RIGHT_CREDIT, RIGHT_DEBIT},
    error::CoreError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CanonicalMove {
    pub source_capability_index: u8,
    pub destination_capability_index: u8,
    pub amount: u64,
}

impl CanonicalMove {
    pub fn encode(self) -> [u8; 10] {
        let mut encoded = [0u8; 10];
        encoded[0] = self.source_capability_index;
        encoded[1] = self.destination_capability_index;
        encoded[2..10].copy_from_slice(&self.amount.to_le_bytes());
        encoded
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedMovePlan {
    pub moves: Vec<CanonicalMove>,
    pub gross_debits: Vec<u128>,
    pub gross_credits: Vec<u128>,
    pub asset_totals: Vec<AssetConservation>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssetConservation {
    pub asset: AssetProfileIdentity,
    pub aggregate_debit: u128,
    pub aggregate_credit: u128,
}

pub fn validate_move_normal_form(
    moves: &[CanonicalMove],
    capabilities: &[SettlementCapability],
) -> Result<ValidatedMovePlan> {
    require!(
        moves.len() <= MAX_ENGINE_MOVES,
        CoreError::ExperimentLimitExceeded
    );
    let mut gross_debits = vec![0u128; capabilities.len()];
    let mut gross_credits = vec![0u128; capabilities.len()];
    // 0 = unused, 1 = source, 2 = destination.
    let mut graph_side = vec![0u8; capabilities.len()];
    let mut previous_pair: Option<(u8, u8)> = None;

    for movement in moves {
        require!(movement.amount != 0, CoreError::ZeroMoveAmount);
        let source_index = usize::from(movement.source_capability_index);
        let destination_index = usize::from(movement.destination_capability_index);
        require!(
            source_index < capabilities.len() && destination_index < capabilities.len(),
            CoreError::MoveCapabilityIndexOutOfRange
        );
        require!(
            source_index != destination_index,
            CoreError::MoveEndpointsIdentical
        );
        let pair = (
            movement.source_capability_index,
            movement.destination_capability_index,
        );
        if let Some(previous) = previous_pair {
            require!(pair > previous, CoreError::NonCanonicalMoveOrder);
        }
        previous_pair = Some(pair);

        let source = capabilities[source_index];
        let destination = capabilities[destination_index];
        require!(source.has_right(RIGHT_DEBIT), CoreError::MoveRightMissing);
        require!(
            destination.has_right(RIGHT_CREDIT),
            CoreError::MoveRightMissing
        );
        require!(
            !source.is_engine_fee_reserved() && !destination.is_engine_fee_reserved(),
            CoreError::EngineReferencedFeeCapability
        );
        require!(
            source.asset == destination.asset,
            CoreError::MoveAssetProfileMismatch
        );

        require!(graph_side[source_index] != 2, CoreError::MoveGraphCycle);
        require!(
            graph_side[destination_index] != 1,
            CoreError::MoveGraphCycle
        );
        graph_side[source_index] = 1;
        graph_side[destination_index] = 2;
        gross_debits[source_index] =
            checked_add_amount(gross_debits[source_index], u128::from(movement.amount))?;
        gross_credits[destination_index] = checked_add_amount(
            gross_credits[destination_index],
            u128::from(movement.amount),
        )?;
    }

    for (position, capability) in capabilities.iter().enumerate() {
        require!(
            gross_debits[position] <= u128::from(capability.declaration.maximum_engine_debit),
            CoreError::CapabilityMaximumDebitExceeded
        );
        // Intent-funded source minimum_credit is a cumulative prefix ratio
        // numerator and exact-recipient minimum_credit is terminal. Both are
        // enforced by authorization state, not as a per-envelope scalar min.
        if capability.declaration.authority_class == crate::constants::AUTHORITY_DOMAIN_ACCOUNTED
            && capability.has_right(RIGHT_CREDIT)
        {
            require!(
                gross_credits[position] >= u128::from(capability.declaration.minimum_credit),
                CoreError::CapabilityMinimumCreditNotMet
            );
        }
    }

    let mut asset_totals: Vec<AssetConservation> = Vec::new();
    for (position, capability) in capabilities.iter().enumerate() {
        let slot = if let Some(slot) = asset_totals
            .iter_mut()
            .find(|slot| slot.asset == capability.asset)
        {
            slot
        } else {
            asset_totals.push(AssetConservation {
                asset: capability.asset,
                aggregate_debit: 0,
                aggregate_credit: 0,
            });
            asset_totals
                .last_mut()
                .ok_or(CoreError::ArithmeticOverflow)?
        };
        slot.aggregate_debit = checked_add_amount(slot.aggregate_debit, gross_debits[position])?;
        slot.aggregate_credit = checked_add_amount(slot.aggregate_credit, gross_credits[position])?;
    }
    require!(
        asset_totals
            .iter()
            .all(|slot| slot.aggregate_debit == slot.aggregate_credit),
        CoreError::AssetConservationMismatch
    );

    Ok(ValidatedMovePlan {
        moves: moves.to_vec(),
        gross_debits,
        gross_credits,
        asset_totals,
    })
}

pub fn checked_add_amount(left: u128, right: u128) -> Result<u128> {
    left.checked_add(right)
        .ok_or_else(|| error!(CoreError::ArithmeticOverflow))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DomainAccountingState {
    pub domain_index: u8,
    pub accounting_slot: u8,
    pub asset: AssetProfileIdentity,
    pub accounted_before: u128,
    pub raw_balance_before: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DomainAccountingDelta {
    pub domain_index: u8,
    pub accounting_slot: u8,
    pub asset: AssetProfileIdentity,
    pub local_debits: u128,
    pub local_credits: u128,
    pub accounted_before: u128,
    pub accounted_after: u128,
    pub expected_raw_after: u64,
}

pub fn derive_domain_accounting(
    plan: &ValidatedMovePlan,
    capabilities: &[SettlementCapability],
    states: &[DomainAccountingState],
) -> Result<Vec<DomainAccountingDelta>> {
    require_eq!(
        plan.gross_debits.len(),
        capabilities.len(),
        CoreError::InvalidWireEncoding
    );
    require_eq!(
        plan.gross_credits.len(),
        capabilities.len(),
        CoreError::InvalidWireEncoding
    );
    for (index, state) in states.iter().enumerate() {
        require!(
            states[..index].iter().all(|earlier| {
                earlier.domain_index != state.domain_index
                    || earlier.accounting_slot != state.accounting_slot
                    || earlier.asset != state.asset
            }),
            CoreError::InvalidSettlementDomain
        );
        require!(
            u128::from(state.raw_balance_before) >= state.accounted_before,
            CoreError::RawBalanceBelowAccounted
        );
    }

    let mut output = Vec::with_capacity(states.len());
    for state in states {
        let mut local_debits = 0u128;
        let mut local_credits = 0u128;
        for (position, capability) in capabilities.iter().enumerate() {
            if capability.declaration.authority_class == AUTHORITY_DOMAIN_ACCOUNTED
                && capability.domain.is_some_and(|domain| {
                    domain.domain_index == state.domain_index
                        && domain.accounting_slot == state.accounting_slot
                        && capability.asset == state.asset
                })
            {
                local_debits = checked_add_amount(local_debits, plan.gross_debits[position])?;
                local_credits = checked_add_amount(local_credits, plan.gross_credits[position])?;
            }
        }
        require!(
            local_debits <= state.accounted_before,
            CoreError::DomainAccountedLiquidityExceeded
        );
        let accounted_after = state
            .accounted_before
            .checked_add(local_credits)
            .and_then(|value| value.checked_sub(local_debits))
            .ok_or(CoreError::ArithmeticOverflow)?;
        let raw_after_u128 = u128::from(state.raw_balance_before)
            .checked_add(local_credits)
            .and_then(|value| value.checked_sub(local_debits))
            .ok_or(CoreError::ArithmeticOverflow)?;
        let expected_raw_after =
            u64::try_from(raw_after_u128).map_err(|_| CoreError::AmountConversionFailed)?;
        require!(
            u128::from(expected_raw_after) >= accounted_after,
            CoreError::RawBalanceBelowAccounted
        );
        output.push(DomainAccountingDelta {
            domain_index: state.domain_index,
            accounting_slot: state.accounting_slot,
            asset: state.asset,
            local_debits,
            local_credits,
            accounted_before: state.accounted_before,
            accounted_after,
            expected_raw_after,
        });
    }

    // Every domain capability must map to one and only one explicit local
    // accounting state. Global asset conservation is never a substitute.
    for capability in capabilities
        .iter()
        .filter(|capability| capability.declaration.authority_class == AUTHORITY_DOMAIN_ACCOUNTED)
    {
        let domain = capability
            .domain
            .ok_or(CoreError::InvalidSettlementDomain)?;
        require_eq!(
            states
                .iter()
                .filter(|state| {
                    state.domain_index == domain.domain_index
                        && state.accounting_slot == domain.accounting_slot
                        && state.asset == capability.asset
                })
                .count(),
            1,
            CoreError::InvalidSettlementDomain
        );
    }
    Ok(output)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObservedProtectedBalance {
    pub capability_index: u8,
    pub before: u64,
    pub after: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtectedFeeTransfer {
    pub source_capability_index: u8,
    pub destination_capability_index: u8,
    pub amount: u64,
}

pub fn verify_exact_observed_deltas(
    plan: &ValidatedMovePlan,
    fee_transfers: &[ProtectedFeeTransfer],
    observations: &[ObservedProtectedBalance],
) -> Result<[u8; 32]> {
    require_eq!(
        plan.gross_debits.len(),
        plan.gross_credits.len(),
        CoreError::ObservedProtectedDeltaMismatch
    );
    let mut total_debits = plan.gross_debits.clone();
    let mut total_credits = plan.gross_credits.clone();
    let mut previous_fee_pair = None;
    for transfer in fee_transfers {
        require!(
            transfer.amount != 0,
            CoreError::ObservedProtectedDeltaMismatch
        );
        let source = usize::from(transfer.source_capability_index);
        let destination = usize::from(transfer.destination_capability_index);
        require!(
            source < total_debits.len()
                && destination < total_credits.len()
                && source != destination,
            CoreError::ObservedProtectedDeltaMismatch
        );
        let pair = (
            transfer.source_capability_index,
            transfer.destination_capability_index,
        );
        if let Some(previous) = previous_fee_pair {
            require!(pair > previous, CoreError::NonCanonicalMoveOrder);
        }
        previous_fee_pair = Some(pair);
        total_debits[source] =
            checked_add_amount(total_debits[source], u128::from(transfer.amount))?;
        total_credits[destination] =
            checked_add_amount(total_credits[destination], u128::from(transfer.amount))?;
    }

    let changed_count = total_debits
        .iter()
        .zip(&total_credits)
        .filter(|(debit, credit)| **debit != 0 || **credit != 0)
        .count();
    require_eq!(
        observations.len(),
        changed_count,
        CoreError::ObservedProtectedDeltaMismatch
    );

    let mut rows = Vec::with_capacity(changed_count);
    let mut observation_position = 0usize;
    for capability_index in 0..total_debits.len() {
        let gross_debit = total_debits[capability_index];
        let gross_credit = total_credits[capability_index];
        if gross_debit == 0 && gross_credit == 0 {
            continue;
        }
        require!(
            gross_debit == 0 || gross_credit == 0,
            CoreError::ObservedProtectedDeltaMismatch
        );
        let observation = observations
            .get(observation_position)
            .ok_or(CoreError::ObservedProtectedDeltaMismatch)?;
        observation_position = observation_position
            .checked_add(1)
            .ok_or(CoreError::ArithmeticOverflow)?;
        require_eq!(
            usize::from(observation.capability_index),
            capability_index,
            CoreError::ObservedProtectedDeltaMismatch
        );
        let debit = u64::try_from(gross_debit).map_err(|_| CoreError::AmountConversionFailed)?;
        let credit = u64::try_from(gross_credit).map_err(|_| CoreError::AmountConversionFailed)?;
        let row = ObservedProtectedDeltaRowCandidateV0 {
            capability_index: observation.capability_index,
            before: observation.before,
            after: observation.after,
            gross_debit: debit,
            gross_credit: credit,
        };
        row.encode()
            .map_err(|_| error!(CoreError::ObservedProtectedDeltaMismatch))?;
        rows.push(row);
    }
    compute_observed_protected_delta_set_root(&rows)
        .map_err(|_| error!(CoreError::ObservedProtectedDeltaMismatch))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        account_segments::EffectivePrivilege, capabilities::DomainCapabilityIdentity, constants::*,
    };
    use generic_effect_private_wire::SettlementCapabilityRowCandidateV0;

    fn cap(position: u8, asset: Pubkey, rights: u16) -> SettlementCapability {
        let token = Pubkey::new_from_array([9; 32]);
        let authority_class = if rights == RIGHT_DEBIT {
            AUTHORITY_INTENT_FUNDED_DEBIT
        } else {
            AUTHORITY_EXACT_EXTERNAL_CREDIT
        };
        SettlementCapability {
            position,
            declaration: SettlementCapabilityRowCandidateV0 {
                asset_index: 0,
                domain_index_or_none: ABSENT_INDEX,
                authorization_slot_or_none: 0,
                intent_local_term_index_or_none: position,
                authority_class,
                fee_shard_index_or_none: if rights == RIGHT_DEBIT {
                    0
                } else {
                    ABSENT_INDEX
                },
                fee_class: if rights == RIGHT_DEBIT {
                    FEE_CLASS_GROSS_DEBIT_RATE
                } else {
                    FEE_CLASS_NONE
                },
                flags: if rights == RIGHT_DEBIT {
                    generic_effect_private_wire::SETTLEMENT_FLAG_FEE_FUNDING
                } else {
                    0
                },
                rights_bits: rights,
                domain_accounting_slot_or_none: ABSENT_INDEX,
                spend_authority_control_offset_or_none: ABSENT_INDEX,
                reserved_0: 0,
                maximum_engine_debit: if rights == RIGHT_DEBIT { 100 } else { 0 },
                maximum_total_debit: if rights == RIGHT_DEBIT { 101 } else { 0 },
                minimum_credit: 0,
                maximum_protocol_fee: if rights == RIGHT_DEBIT { 1 } else { 0 },
            },
            core_program: Pubkey::new_from_array([41; 32]),
            experimental_major: 0,
            market: Pubkey::new_from_array([3; 32]),
            endpoint: EffectivePrivilege {
                key: Pubkey::new_from_array([position.saturating_add(20); 32]),
                owner: token,
                executable: false,
                signer: false,
                writable: true,
            },
            transfer_authority_or_zero: Pubkey::new_from_array([8; 32]),
            asset: AssetProfileIdentity {
                asset_identity: asset,
                asset_program: token,
                settlement_profile_digest: [7; 32],
            },
            domain: None,
            fee_policy_revision: 1,
            lifecycle_digest: [1; 32],
            accounted_before_or_zero: 0,
        }
    }

    #[test]
    fn move_normal_form_rejects_cycles_and_accepts_allocations() {
        let asset = Pubkey::new_unique();
        let capabilities = vec![
            cap(0, asset, RIGHT_DEBIT),
            cap(1, asset, RIGHT_CREDIT | RIGHT_EXACT_EXTERNAL_RECIPIENT),
            cap(2, asset, RIGHT_CREDIT | RIGHT_EXACT_EXTERNAL_RECIPIENT),
        ];
        let accepted = validate_move_normal_form(
            &[
                CanonicalMove {
                    source_capability_index: 0,
                    destination_capability_index: 1,
                    amount: 40,
                },
                CanonicalMove {
                    source_capability_index: 0,
                    destination_capability_index: 2,
                    amount: 60,
                },
            ],
            &capabilities,
        )
        .unwrap();
        assert_eq!(accepted.gross_debits, vec![100, 0, 0]);
        assert_eq!(accepted.gross_credits, vec![0, 40, 60]);

        assert!(validate_move_normal_form(
            &[
                CanonicalMove {
                    source_capability_index: 0,
                    destination_capability_index: 1,
                    amount: 40,
                },
                CanonicalMove {
                    source_capability_index: 1,
                    destination_capability_index: 2,
                    amount: 40,
                },
            ],
            &capabilities,
        )
        .is_err());
    }

    #[test]
    fn u128_aggregation_is_checked() {
        assert_eq!(checked_add_amount(u128::MAX - 1, 1).unwrap(), u128::MAX);
        assert!(checked_add_amount(u128::MAX, 1).is_err());
    }

    #[test]
    fn observed_delta_root_is_changed_only_and_includes_fees() {
        let asset = Pubkey::new_unique();
        let capabilities = vec![
            cap(0, asset, RIGHT_DEBIT),
            cap(1, asset, RIGHT_CREDIT | RIGHT_EXACT_EXTERNAL_RECIPIENT),
            cap(2, asset, RIGHT_CREDIT | RIGHT_EXACT_EXTERNAL_RECIPIENT),
        ];
        let plan = validate_move_normal_form(
            &[CanonicalMove {
                source_capability_index: 0,
                destination_capability_index: 1,
                amount: 40,
            }],
            &capabilities,
        )
        .unwrap();
        let fee = [ProtectedFeeTransfer {
            source_capability_index: 0,
            destination_capability_index: 2,
            amount: 5,
        }];
        let observations = [
            ObservedProtectedBalance {
                capability_index: 0,
                before: 100,
                after: 55,
            },
            ObservedProtectedBalance {
                capability_index: 1,
                before: 0,
                after: 40,
            },
            ObservedProtectedBalance {
                capability_index: 2,
                before: 0,
                after: 5,
            },
        ];
        assert_ne!(
            verify_exact_observed_deltas(&plan, &fee, &observations).unwrap(),
            [0; 32]
        );
        assert!(verify_exact_observed_deltas(&plan, &fee, &observations[..2]).is_err());
        let mut wrong = observations;
        wrong[0].after = 56;
        assert!(verify_exact_observed_deltas(&plan, &fee, &wrong).is_err());
    }

    #[test]
    fn descriptor_predicates_do_not_create_domain_accounting_rights() {
        let asset = Pubkey::new_unique();
        let mut capabilities = vec![
            cap(0, asset, RIGHT_DEBIT),
            cap(1, asset, RIGHT_CREDIT | RIGHT_EXACT_EXTERNAL_RECIPIENT),
        ];
        let predicate = DomainCapabilityIdentity {
            domain_index: 0,
            domain_descriptor: Pubkey::new_unique(),
            domain_revision: 1,
            admission_digest: [11; 32],
            accounting_slot: ABSENT_INDEX,
        };
        capabilities[0].declaration.domain_index_or_none = 0;
        capabilities[0].domain = Some(predicate);
        capabilities[1].declaration.domain_index_or_none = 0;
        capabilities[1].domain = Some(predicate);

        let plan = validate_move_normal_form(
            &[CanonicalMove {
                source_capability_index: 0,
                destination_capability_index: 1,
                amount: 10,
            }],
            &capabilities,
        )
        .unwrap();
        assert!(derive_domain_accounting(&plan, &capabilities, &[])
            .unwrap()
            .is_empty());
    }
}
