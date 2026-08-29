//! Core-derived exact fee algebra. Engines never construct fee effects.

use anchor_lang::prelude::*;
use generic_effect_private_wire::{
    compute_fee_assessment_digest, compute_fee_assessment_set_root, compute_fee_collection_digest,
    compute_fee_principal_digest, compute_fee_rounding_group_digest, compute_fee_shard_set_digest,
    FeeAssessmentDigestInputs, FeeAssessmentSetRowCandidateV0, FeeCollectionDigestInputs,
    FeeRoundingGroupRowCandidateV0, FeeShardDigestRowCandidateV0,
};

use crate::{
    capabilities::AssetProfileIdentity,
    constants::{EXPERIMENTAL_MAJOR, ROUND_CEILING, ROUND_FLOOR},
    error::CoreError,
};

#[allow(clippy::assign_op_pattern, clippy::manual_div_ceil)]
mod wide {
    use uint::construct_uint;

    construct_uint! {
        /// Private arithmetic carrier. It is never serialized into an account
        /// or exposed as an interface type.
        pub struct U256(4);
    }
}

pub use wide::U256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoundingMode {
    Floor,
    Ceiling,
}

impl RoundingMode {
    pub fn decode(value: u8) -> Result<Self> {
        match value {
            ROUND_FLOOR => Ok(Self::Floor),
            ROUND_CEILING => Ok(Self::Ceiling),
            _ => err!(CoreError::UnsupportedFeeRounding),
        }
    }

    pub fn encode(self) -> u8 {
        match self {
            Self::Floor => ROUND_FLOOR,
            Self::Ceiling => ROUND_CEILING,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RatePolicy {
    pub rate: u64,
    pub denominator: u64,
    pub rounding: RoundingMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FeeBucketKey {
    pub actor: Pubkey,
    pub intent_digest: [u8; 32],
    pub fee_policy_digest: [u8; 32],
    pub asset: AssetProfileIdentity,
    pub fee_class: u8,
    pub fee_policy_revision: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FeeCollectionRoute {
    pub funding_capability_index: u8,
    pub designated_endpoint: Pubkey,
    pub fee_shard_index: u8,
    pub maximum_protocol_fee: u64,
    pub maximum_total_debit: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FeeBasisContribution {
    pub key: FeeBucketKey,
    pub basis: u128,
    /// Present only on the single source capability carrying the canonical
    /// FEE_FUNDING flag for this economic group.
    pub collection_route: Option<FeeCollectionRoute>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AggregatedFeeBucket {
    pub key: FeeBucketKey,
    pub fill_basis: u128,
    pub collection_route: FeeCollectionRoute,
}

pub fn aggregate_fee_bases(
    contributions: &[FeeBasisContribution],
) -> Result<Vec<AggregatedFeeBucket>> {
    struct PendingBucket {
        key: FeeBucketKey,
        fill_basis: u128,
        collection_route: Option<FeeCollectionRoute>,
    }

    let mut pending: Vec<PendingBucket> = Vec::new();
    for contribution in contributions {
        if let Some(bucket) = pending
            .iter_mut()
            .find(|bucket| bucket.key == contribution.key)
        {
            bucket.fill_basis = bucket
                .fill_basis
                .checked_add(contribution.basis)
                .ok_or(CoreError::ArithmeticOverflow)?;
            if let Some(route) = contribution.collection_route {
                require!(
                    bucket.collection_route.is_none(),
                    CoreError::InvalidSettlementFeeShard
                );
                bucket.collection_route = Some(route);
            }
        } else {
            pending.push(PendingBucket {
                key: contribution.key,
                fill_basis: contribution.basis,
                collection_route: contribution.collection_route,
            });
        }
    }
    let mut buckets = Vec::with_capacity(pending.len());
    for bucket in pending {
        require!(bucket.fill_basis != 0, CoreError::InvalidSettlementRights);
        buckets.push(AggregatedFeeBucket {
            key: bucket.key,
            fill_basis: bucket.fill_basis,
            collection_route: bucket
                .collection_route
                .ok_or(CoreError::InvalidSettlementFeeShard)?,
        });
    }
    Ok(buckets)
}

pub fn rounded_rate_fee(basis: u128, policy: RatePolicy) -> Result<u128> {
    require!(policy.denominator != 0, CoreError::ZeroFeeDenominator);
    let numerator = U256::from(basis) * U256::from(policy.rate);
    let denominator = U256::from(policy.denominator);
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let rounded = match policy.rounding {
        RoundingMode::Floor => quotient,
        RoundingMode::Ceiling => {
            if remainder.is_zero() {
                quotient
            } else {
                quotient
                    .checked_add(U256::one())
                    .ok_or(CoreError::ArithmeticOverflow)?
            }
        }
    };
    require!(
        rounded <= U256::from(u128::MAX),
        CoreError::AmountConversionFailed
    );
    Ok(rounded.low_u128())
}

/// Partition-independent incremental fee for a stored authorization bucket.
pub fn incremental_rate_fee(
    cumulative_basis_before: u128,
    fill_basis: u128,
    policy: RatePolicy,
) -> Result<(u128, u128)> {
    let cumulative_basis_after = cumulative_basis_before
        .checked_add(fill_basis)
        .ok_or(CoreError::ArithmeticOverflow)?;
    require!(
        cumulative_basis_after >= cumulative_basis_before,
        CoreError::CumulativeFeeBasisDecreased
    );
    let fee_before = rounded_rate_fee(cumulative_basis_before, policy)?;
    let fee_after = rounded_rate_fee(cumulative_basis_after, policy)?;
    let delta = fee_after
        .checked_sub(fee_before)
        .ok_or(CoreError::CumulativeFeeBasisDecreased)?;
    Ok((delta, cumulative_basis_after))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FeeAssessmentContext {
    pub core_program: Pubkey,
    pub experimental_major: u32,
    pub market_binding_digest: [u8; 32],
    pub policy_digest: [u8; 32],
    pub policy_revision: u64,
    pub intent_set_digest: [u8; 32],
    pub protected_execution_root: [u8; 32],
    pub effect_digest: [u8; 32],
    pub fill_sequence: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FeeAssessment {
    pub identity: [u8; 32],
    pub key: FeeBucketKey,
    pub collection_route: FeeCollectionRoute,
    pub rounding_group_digest: [u8; 32],
    pub fee_collection_digest: [u8; 32],
    pub fill_basis: u128,
    pub cumulative_basis_before: u128,
    pub cumulative_basis_after: u128,
    pub rate_fee: u64,
    pub total_fee: u64,
}

pub fn derive_fee_assessment(
    context: FeeAssessmentContext,
    bucket: AggregatedFeeBucket,
    cumulative_basis_before: u128,
    cumulative_assessed_before: u128,
    policy: RatePolicy,
) -> Result<FeeAssessment> {
    require_eq!(
        bucket.key.fee_policy_revision,
        context.policy_revision,
        CoreError::InvalidSettlementRights
    );
    require_eq!(
        context.experimental_major,
        EXPERIMENTAL_MAJOR,
        CoreError::InvalidWireEncoding
    );
    require!(
        bucket.key.fee_policy_digest == context.policy_digest,
        CoreError::InvalidSettlementRights
    );
    let expected_assessed_before = rounded_rate_fee(cumulative_basis_before, policy)?;
    require_eq!(
        cumulative_assessed_before,
        expected_assessed_before,
        CoreError::FeeLiabilityMismatch
    );
    let (rate_fee_wide, cumulative_basis_after) =
        incremental_rate_fee(cumulative_basis_before, bucket.fill_basis, policy)?;
    let rate_fee = u64::try_from(rate_fee_wide).map_err(|_| CoreError::AmountConversionFailed)?;
    // This spike intentionally disables fixed per-envelope fees. There is no
    // product-neutral source authorization for a fee-only or zero-debit
    // envelope in the accepted private profile.
    let total_fee = rate_fee;
    let principal_digest =
        compute_fee_principal_digest(&bucket.key.actor.to_bytes(), &bucket.key.intent_digest)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    let rounding_group = FeeRoundingGroupRowCandidateV0 {
        fee_principal_digest: principal_digest,
        fee_policy_digest: bucket.key.fee_policy_digest,
        asset_identity: bucket.key.asset.asset_identity.to_bytes(),
        asset_program: bucket.key.asset.asset_program.to_bytes(),
        settlement_profile_digest: bucket.key.asset.settlement_profile_digest,
        fee_class: bucket.key.fee_class,
        fee_policy_revision: bucket.key.fee_policy_revision,
    };
    let rounding_group_digest = compute_fee_rounding_group_digest(&rounding_group)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    let fee_collection_digest = compute_fee_collection_digest(FeeCollectionDigestInputs {
        assessment_group_digest: &rounding_group_digest,
        designated_funding_endpoint_key: &bucket.collection_route.designated_endpoint.to_bytes(),
        maximum_protocol_fee: bucket.collection_route.maximum_protocol_fee,
        maximum_total_debit: bucket.collection_route.maximum_total_debit,
        fee_shard_index: bucket.collection_route.fee_shard_index,
        fee_delta: total_fee,
    })
    .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    let identity = compute_fee_assessment_digest(FeeAssessmentDigestInputs {
        core_program: &context.core_program.to_bytes(),
        market_binding_digest: &context.market_binding_digest,
        fee_policy_digest: &context.policy_digest,
        fee_policy_revision: context.policy_revision,
        intent_set_digest: &context.intent_set_digest,
        protected_execution_root: &context.protected_execution_root,
        effect_digest: &context.effect_digest,
        rounding_group_digest: &rounding_group_digest,
        fee_collection_digest: &fee_collection_digest,
        fill_sequence: context.fill_sequence,
        cumulative_before: cumulative_basis_before,
        fill_basis: bucket.fill_basis,
        cumulative_after: cumulative_basis_after,
        fee_delta: total_fee,
    })
    .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    Ok(FeeAssessment {
        identity,
        key: bucket.key,
        collection_route: bucket.collection_route,
        rounding_group_digest,
        fee_collection_digest,
        fill_basis: bucket.fill_basis,
        cumulative_basis_before,
        cumulative_basis_after,
        rate_fee,
        total_fee,
    })
}

pub fn fee_assessment_set_root(assessments: &[FeeAssessment]) -> Result<[u8; 32]> {
    let mut rows = assessments
        .iter()
        .map(|assessment| FeeAssessmentSetRowCandidateV0 {
            assessment_group_digest: assessment.rounding_group_digest,
            assessment_digest: assessment.identity,
        })
        .collect::<Vec<_>>();
    rows.sort_unstable_by_key(FeeAssessmentSetRowCandidateV0::encode);
    require!(
        rows.windows(2)
            .all(|pair| pair[0].encode() < pair[1].encode()),
        CoreError::DuplicateFeeAssessment
    );
    compute_fee_assessment_set_root(&rows).map_err(|_| error!(CoreError::InvalidWireEncoding))
}

pub fn fee_shard_set_root(rows: &[FeeShardDigestRowCandidateV0]) -> Result<[u8; 32]> {
    compute_fee_shard_set_digest(rows).map_err(|_| error!(CoreError::InvalidWireEncoding))
}

pub fn validate_user_fee_and_total_debit(
    engine_debit: u128,
    assessment: &FeeAssessment,
    maximum_total_debit: u64,
    maximum_fee: u64,
) -> Result<()> {
    require!(
        assessment.total_fee <= maximum_fee,
        CoreError::FeeCeilingExceeded
    );
    let total_debit = engine_debit
        .checked_add(u128::from(assessment.total_fee))
        .ok_or(CoreError::ArithmeticOverflow)?;
    require!(
        total_debit <= u128::from(maximum_total_debit),
        CoreError::CapabilityMaximumDebitExceeded
    );
    Ok(())
}

pub fn update_fee_liability(
    liability_before: u128,
    observed_net_fee_vault_credit: u64,
) -> Result<u128> {
    liability_before
        .checked_add(u128::from(observed_net_fee_vault_credit))
        .ok_or_else(|| error!(CoreError::ArithmeticOverflow))
}

pub fn require_exact_fee_vault_delta(
    balance_before: u64,
    balance_after: u64,
    assessment: &FeeAssessment,
) -> Result<u64> {
    let observed_credit = balance_after
        .checked_sub(balance_before)
        .ok_or(CoreError::FeeLiabilityMismatch)?;
    require_eq!(
        observed_credit,
        assessment.total_fee,
        CoreError::FeeLiabilityMismatch
    );
    Ok(observed_credit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(rounding: RoundingMode) -> RatePolicy {
        RatePolicy {
            rate: 37,
            denominator: 10_000,
            rounding,
        }
    }

    fn bucket_fixture(fill_basis: u128) -> AggregatedFeeBucket {
        AggregatedFeeBucket {
            key: FeeBucketKey {
                actor: Pubkey::new_unique(),
                intent_digest: [1; 32],
                fee_policy_digest: [2; 32],
                asset: AssetProfileIdentity {
                    asset_identity: Pubkey::new_unique(),
                    asset_program: Pubkey::new_unique(),
                    settlement_profile_digest: [3; 32],
                },
                fee_class: 1,
                fee_policy_revision: 7,
            },
            fill_basis,
            collection_route: FeeCollectionRoute {
                funding_capability_index: 0,
                designated_endpoint: Pubkey::new_unique(),
                fee_shard_index: 0,
                maximum_protocol_fee: 100,
                maximum_total_debit: 1_000,
            },
        }
    }

    fn context_for(bucket: &AggregatedFeeBucket) -> FeeAssessmentContext {
        FeeAssessmentContext {
            core_program: Pubkey::new_unique(),
            experimental_major: EXPERIMENTAL_MAJOR,
            market_binding_digest: [4; 32],
            policy_digest: bucket.key.fee_policy_digest,
            policy_revision: bucket.key.fee_policy_revision,
            intent_set_digest: [5; 32],
            protected_execution_root: [6; 32],
            effect_digest: [7; 32],
            fill_sequence: 0,
        }
    }

    #[test]
    fn wide_floor_and_ceiling_are_exact_at_boundaries() {
        assert_eq!(rounded_rate_fee(1, policy(RoundingMode::Floor)).unwrap(), 0);
        assert_eq!(
            rounded_rate_fee(1, policy(RoundingMode::Ceiling)).unwrap(),
            1
        );
        assert_eq!(
            rounded_rate_fee(
                u128::MAX,
                RatePolicy {
                    rate: 1,
                    denominator: u64::MAX,
                    rounding: RoundingMode::Floor,
                }
            )
            .unwrap(),
            u128::MAX / u128::from(u64::MAX)
        );
        assert!(rounded_rate_fee(
            1,
            RatePolicy {
                rate: 1,
                denominator: 0,
                rounding: RoundingMode::Floor,
            }
        )
        .is_err());
    }

    #[test]
    fn cumulative_fee_is_equal_across_4096_partitions() {
        let total_basis = 4_096u128 * 17 + 3;
        for rounding in [RoundingMode::Floor, RoundingMode::Ceiling] {
            let rate_policy = policy(rounding);
            let unsplit = rounded_rate_fee(total_basis, rate_policy).unwrap();
            let mut basis = 0u128;
            let mut cumulative_fee = 0u128;
            for position in 0..4_096u128 {
                let fill = if position == 4_095 { 20 } else { 17 };
                let (delta, after) = incremental_rate_fee(basis, fill, rate_policy).unwrap();
                basis = after;
                cumulative_fee += delta;
            }
            assert_eq!(basis, total_basis);
            assert_eq!(cumulative_fee, unsplit);
        }
    }

    #[test]
    fn zero_basis_designated_source_keeps_collection_route_for_positive_group() {
        let bucket = bucket_fixture(0);
        let contributions = [
            FeeBasisContribution {
                key: bucket.key,
                basis: 0,
                collection_route: Some(bucket.collection_route),
            },
            FeeBasisContribution {
                key: bucket.key,
                basis: 19,
                collection_route: None,
            },
        ];
        let aggregated = aggregate_fee_bases(&contributions).unwrap();
        assert_eq!(aggregated.len(), 1);
        assert_eq!(aggregated[0].fill_basis, 19);
        assert_eq!(aggregated[0].collection_route, bucket.collection_route);

        let all_zero = [FeeBasisContribution {
            key: bucket.key,
            basis: 0,
            collection_route: Some(bucket.collection_route),
        }];
        assert!(aggregate_fee_bases(&all_zero).is_err());
    }

    #[test]
    fn stored_assessed_fee_must_match_exact_cumulative_rounding_state() {
        let bucket = bucket_fixture(1);
        let context = context_for(&bucket);
        let thirds = RatePolicy {
            rate: 1,
            denominator: 3,
            rounding: RoundingMode::Floor,
        };

        assert!(derive_fee_assessment(context, bucket, 2, 1, thirds).is_err());
        let crossing = derive_fee_assessment(context, bucket, 2, 0, thirds).unwrap();
        assert_eq!(crossing.rate_fee, 1);
        assert_eq!(crossing.cumulative_basis_after, 3);

        let next = derive_fee_assessment(context, bucket, 3, 1, thirds).unwrap();
        assert_eq!(next.rate_fee, 0);
        assert_eq!(next.cumulative_basis_after, 4);
        assert_eq!(
            u128::from(crossing.rate_fee) + u128::from(next.rate_fee),
            rounded_rate_fee(4, thirds).unwrap()
        );
    }

    #[test]
    fn fee_vault_delta_uses_net_credit_not_post_balance() {
        let bucket = bucket_fixture(7);
        let assessment = FeeAssessment {
            identity: [8; 32],
            key: bucket.key,
            collection_route: bucket.collection_route,
            rounding_group_digest: [9; 32],
            fee_collection_digest: [10; 32],
            fill_basis: 7,
            cumulative_basis_before: 0,
            cumulative_basis_after: 7,
            rate_fee: 7,
            total_fee: 7,
        };
        assert_eq!(
            require_exact_fee_vault_delta(100, 107, &assessment).unwrap(),
            7
        );
        assert!(require_exact_fee_vault_delta(100, 207, &assessment).is_err());
        assert!(require_exact_fee_vault_delta(108, 107, &assessment).is_err());
    }
}
