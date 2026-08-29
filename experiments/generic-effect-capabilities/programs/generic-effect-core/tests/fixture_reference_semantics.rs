mod common;

use anchor_lang::solana_program::program_pack::Pack;
use common::{
    assert_controlled_resource_headroom, contains_program_path, decode_execution_evidence,
    decode_requested_heap_frame, request_heap_frame_instruction, snapshot_accounts,
    ReferenceAssetSpec, ReferenceCapabilityKind, ReferenceCapabilitySpec,
    ReferenceCreditConstraintSpec, ReferenceFixtureCompiler, ReferenceFixtureSpec,
    ReferenceIntentSpec, ReferenceOpaqueSpec, ReferencePlanSpec, SbfArtifacts,
    CONTROLLED_HEAP_FRAME_BYTES,
};
use effect_engine_probe::{
    plan::{
        BatchClearingPlan, ConstantProductPlan, InventoryDistributionPlan, PartialAuctionPlan,
        PlannedMove, WeightedAllocationPlan, RECEIPT_ACCEPT, RECEIPT_LATE_FAILURE,
    },
    reference_state::{
        AuctionStateCandidateV0, ConstantProductStateCandidateV0, OrderStateCandidateV0,
    },
};
use generic_effect_private_wire::{
    EffectReceiptCandidateV0, MoveCandidateV0, AUTHORITY_EXACT_EXTERNAL_CREDIT,
    AUTHORITY_INTENT_FUNDED, EFFECT_RECEIPT_MAGIC, PHASE_TRANSITION,
    SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT, WIRE_VERSION,
};
use litesvm::types::TransactionMetadata;
use litesvm_token::spl_token::state::Mint as SplMint;
use programmable_generic_effect_core::state::StoredAuthorizationLifecycle;

const FEE_NUMERATOR: u64 = 3;
const FEE_DENOMINATOR: u64 = 1_000;

// Frozen from the final instrumented 208-KiB Core build. The production
// artifact deliberately omits allocator telemetry; every case still executes
// against the real requested frame, so an underestimate fails at runtime.
const INSTRUMENTED_HEAP_ARTIFACT_SHA256: &str =
    "e9f9cb6fbaf17bba498d02d20791874c8f3eddb5814173442d66441c9151ad1d";
const WEIGHTED_HEAP_PEAK_BYTES: u32 = 89_168;
const EXPLICIT_HEAP_PEAK_BYTES: u32 = 89_168;
const CONSTANT_PRODUCT_HEAP_PEAK_BYTES: u32 = 103_224;
const AUCTION_FIRST_FILL_HEAP_PEAK_BYTES: u32 = 103_984;
const AUCTION_SECOND_FILL_HEAP_PEAK_BYTES: u32 = 109_480;
const AUCTION_ONE_SHOT_HEAP_PEAK_BYTES: u32 = 103_984;
const FOUR_ACTOR_BATCH_HEAP_PEAK_BYTES: u32 = 169_992;
const INVENTORY_HEAP_PEAK_BYTES: u32 = 113_760;

#[test]
fn weighted_allocation_and_explicit_moves_are_exact_sbf_equivalent() {
    let artifacts = artifacts();
    let weighted_spec = weighted_spec(RECEIPT_ACCEPT);
    let explicit_spec = ReferenceFixtureSpec {
        label: "explicit-two-move-comparator",
        plan: ReferencePlanSpec::Explicit(vec![
            PlannedMove {
                source_capability_index: 0,
                destination_capability_index: 1,
                amount: 7_500,
            },
            PlannedMove {
                source_capability_index: 0,
                destination_capability_index: 2,
                amount: 2_500,
            },
        ]),
        ..weighted_spec.clone()
    };

    let mut weighted = ReferenceFixtureCompiler::new(&artifacts, weighted_spec);
    let (weighted_metadata, weighted_message, weighted_execution) = weighted.send_success();
    assert_resource_row(
        "weighted-allocation",
        &weighted,
        &weighted_metadata,
        &weighted_message,
        &weighted_execution,
        28,
        WEIGHTED_HEAP_PEAK_BYTES,
    );
    let weighted_receipt = assert_expected_receipt(
        &weighted_metadata,
        &weighted,
        &[(0, 1, 7_500), (0, 2, 2_500)],
        0,
    );
    assert_moves(&weighted_receipt.moves, &[(0, 1, 7_500), (0, 2, 2_500)]);
    assert_eq!(weighted.endpoint_balance(0), 0);
    assert_eq!(weighted.endpoint_balance(1), 7_500);
    assert_eq!(weighted.endpoint_balance(2), 2_500);
    assert_eq!(weighted.fee_vault_balance(0), 30);
    assert_eq!(weighted.fee_liability(0), 30);
    assert_consumed(&weighted, 0, 1);
    assert_eq!(weighted.stored_state(0).constraint_count, 2);
    assert_eq!(
        weighted.stored_state(0).immutable_terms[0].flags
            & SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT,
        0
    );

    let mut explicit = ReferenceFixtureCompiler::new(&artifacts, explicit_spec);
    let (explicit_metadata, explicit_message, explicit_execution) = explicit.send_success();
    assert_resource_row(
        "explicit-two-move-comparator",
        &explicit,
        &explicit_metadata,
        &explicit_message,
        &explicit_execution,
        28,
        EXPLICIT_HEAP_PEAK_BYTES,
    );
    let explicit_receipt = assert_expected_receipt(
        &explicit_metadata,
        &explicit,
        &[(0, 1, 7_500), (0, 2, 2_500)],
        0,
    );

    let weighted_move_bytes = move_bytes(&weighted_receipt.moves);
    let explicit_move_bytes = move_bytes(&explicit_receipt.moves);
    assert_eq!(weighted_move_bytes, explicit_move_bytes);
    assert_eq!(weighted.endpoint_balance(0), explicit.endpoint_balance(0));
    assert_eq!(weighted.endpoint_balance(1), explicit.endpoint_balance(1));
    assert_eq!(weighted.endpoint_balance(2), explicit.endpoint_balance(2));
    assert_eq!(weighted.fee_vault_balance(0), explicit.fee_vault_balance(0));
    assert_eq!(weighted.fee_liability(0), explicit.fee_liability(0));
    assert_consumed(&explicit, 0, 1);
}

#[test]
fn constant_product_reads_and_mutates_typed_pool_state_on_sbf() {
    let artifacts = artifacts();
    let mut fixture =
        ReferenceFixtureCompiler::new(&artifacts, constant_product_spec(RECEIPT_ACCEPT));
    let before = fixture.constant_product_state(0);
    assert_eq!(before.sequence, 0);
    assert_eq!(before.last_input_amount, 0);
    assert_eq!(before.last_output_amount, 0);

    let (metadata, message, execution) = fixture.send_success();
    assert_resource_row(
        "constant-product",
        &fixture,
        &metadata,
        &message,
        &execution,
        24,
        CONSTANT_PRODUCT_HEAP_PEAK_BYTES,
    );
    let receipt =
        assert_expected_receipt(&metadata, &fixture, &[(0, 1, 100_000), (2, 3, 181_322)], 1);
    assert_moves(&receipt.moves, &[(0, 1, 100_000), (2, 3, 181_322)]);
    assert_eq!(fixture.endpoint_balance(0), 0);
    assert_eq!(fixture.endpoint_balance(1), 1_100_000);
    assert_eq!(fixture.endpoint_balance(2), 1_818_678);
    assert_eq!(fixture.endpoint_balance(3), 181_322);
    assert_eq!(fixture.domain_accounted(0), 1_100_000);
    assert_eq!(fixture.domain_accounted(1), 1_818_678);
    assert_eq!(fixture.fee_vault_balance(0), 300);
    assert_eq!(fixture.fee_liability(0), 300);
    assert_consumed(&fixture, 0, 1);
    assert_eq!(fixture.stored_state(0).constraint_count, 1);

    let after = fixture.constant_product_state(0);
    assert_eq!(after.sequence, 1);
    assert_eq!(after.last_request_digest, receipt.request_digest);
    assert_eq!(after.last_input_amount, 100_000);
    assert_eq!(after.last_output_amount, 181_322);
}

#[test]
fn partial_auction_split_and_one_shot_have_cumulative_payment_and_fee_parity() {
    let artifacts = artifacts();
    let mut split = ReferenceFixtureCompiler::new(&artifacts, auction_spec(1, 2, RECEIPT_ACCEPT));
    let initial_authorization = split.stored_state(0);
    assert_eq!(initial_authorization.constraint_count, 1);
    assert_eq!(
        initial_authorization.immutable_terms[0].flags
            & SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT,
        0
    );
    let (first_metadata, first_message, first_execution) = split.send_success();
    assert_resource_row(
        "auction-split-fill-one",
        &split,
        &first_metadata,
        &first_message,
        &first_execution,
        24,
        AUCTION_FIRST_FILL_HEAP_PEAK_BYTES,
    );
    let first_receipt =
        assert_expected_receipt(&first_metadata, &split, &[(0, 1, 667), (2, 3, 1)], 1);
    assert_moves(&first_receipt.moves, &[(0, 1, 667), (2, 3, 1)]);
    assert_eq!(split.fee_vault_balance(0), 2);
    assert_eq!(split.auction_state(0).filled_inventory, 1);
    assert_eq!(split.order_state(1).paid_payment, 667);
    let first_authorization = split.stored_state(0);
    assert_eq!(
        first_authorization.lifecycle,
        StoredAuthorizationLifecycle::ACTIVE
    );
    assert_eq!(
        first_authorization.capabilities[0].cumulative_engine_debit,
        667
    );
    assert_eq!(first_authorization.capabilities[1].cumulative_credit, 1);
    assert_eq!(first_authorization.fee_states[0].cumulative_basis, 667);
    assert_eq!(first_authorization.fee_states[0].cumulative_assessed_fee, 2);

    split.set_plan(
        ReferencePlanSpec::PartialAuction(PartialAuctionPlan {
            auction_state_position: 0,
            order_state_position: 1,
            payment_source_capability_index: 0,
            payment_destination_capability_index: 1,
            inventory_source_capability_index: 2,
            inventory_destination_capability_index: 3,
            fill_inventory_amount: 2,
        }),
        RECEIPT_ACCEPT,
    );
    let (second_metadata, second_message, second_execution) = split.send_success();
    assert_resource_row(
        "auction-split-fill-two",
        &split,
        &second_metadata,
        &second_message,
        &second_execution,
        24,
        AUCTION_SECOND_FILL_HEAP_PEAK_BYTES,
    );
    let second_receipt =
        assert_expected_receipt(&second_metadata, &split, &[(0, 1, 1_333), (2, 3, 2)], 2);
    assert_moves(&second_receipt.moves, &[(0, 1, 1_333), (2, 3, 2)]);
    assert_eq!(split.endpoint_balance(0), 0);
    assert_eq!(split.endpoint_balance(1), 2_000);
    assert_eq!(split.endpoint_balance(2), 0);
    assert_eq!(split.endpoint_balance(3), 3);
    assert_eq!(split.domain_accounted(0), 2_000);
    assert_eq!(split.domain_accounted(1), 0);
    assert_eq!(split.fee_vault_balance(0), 6);
    assert_eq!(split.fee_liability(0), 6);
    assert_consumed(&split, 0, 2);
    let split_authorization = split.stored_state(0);
    assert_eq!(
        split_authorization.capabilities[0].cumulative_engine_debit,
        2_000
    );
    assert_eq!(split_authorization.capabilities[1].cumulative_credit, 3);
    assert_eq!(split_authorization.fee_states[0].cumulative_basis, 2_000);
    assert_eq!(split_authorization.fee_states[0].cumulative_assessed_fee, 6);
    assert_eq!(split.auction_state(0).filled_inventory, 3);
    assert_eq!(split.auction_state(0).remaining_inventory, 0);
    assert_eq!(split.order_state(1).paid_payment, 2_000);
    assert_eq!(split.order_state(1).remaining_payment, 0);

    let mut one_shot =
        ReferenceFixtureCompiler::new(&artifacts, auction_spec(3, 1, RECEIPT_ACCEPT));
    let (one_metadata, one_message, one_execution) = one_shot.send_success();
    assert_resource_row(
        "auction-one-shot",
        &one_shot,
        &one_metadata,
        &one_message,
        &one_execution,
        24,
        AUCTION_ONE_SHOT_HEAP_PEAK_BYTES,
    );
    let one_receipt =
        assert_expected_receipt(&one_metadata, &one_shot, &[(0, 1, 2_000), (2, 3, 3)], 1);
    assert_moves(&one_receipt.moves, &[(0, 1, 2_000), (2, 3, 3)]);
    assert_eq!(one_shot.endpoint_balance(1), split.endpoint_balance(1));
    assert_eq!(one_shot.endpoint_balance(3), split.endpoint_balance(3));
    assert_eq!(one_shot.domain_accounted(0), split.domain_accounted(0));
    assert_eq!(one_shot.domain_accounted(1), split.domain_accounted(1));
    assert_eq!(one_shot.fee_vault_balance(0), split.fee_vault_balance(0));
    assert_eq!(one_shot.fee_liability(0), split.fee_liability(0));
    assert_consumed(&one_shot, 0, 1);
}

#[test]
fn four_actor_batch_clearing_consumes_four_real_stored_authorizations() {
    let artifacts = artifacts();
    let mut fixture = ReferenceFixtureCompiler::new(&artifacts, batch_spec(RECEIPT_ACCEPT));
    assert_eq!(fixture.actor_count(), 4);
    assert_eq!(fixture.authorizations.len(), 4);
    assert_eq!(fixture.spend_authorities.iter().flatten().count(), 2);
    let mut source_only_intents = 0;
    let mut finite_credit_only_intents = 0;
    for slot in 0..4 {
        let state = fixture.stored_state(slot);
        assert_eq!(state.lifecycle, StoredAuthorizationLifecycle::ACTIVE);
        assert_eq!(state.fill_sequence, 0);
        assert_eq!(state.term_count, 1);
        assert_eq!(state.constraint_count, 0);
        let term = state.immutable_terms[0];
        match term.authority_class {
            AUTHORITY_INTENT_FUNDED => {
                source_only_intents += 1;
                assert_ne!(term.maximum_engine_debit, 0);
                assert_ne!(
                    term.flags & SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT,
                    0,
                    "a source-only actor deliberately authorizes its one-sided batch leg",
                );
            }
            AUTHORITY_EXACT_EXTERNAL_CREDIT => {
                finite_credit_only_intents += 1;
                assert_ne!(term.minimum_credit, 0);
                assert_eq!(
                    term.flags & SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT,
                    0,
                );
            }
            authority => panic!("unexpected four-actor batch authority class {authority}"),
        }
    }
    assert_eq!(source_only_intents, 2);
    assert_eq!(finite_credit_only_intents, 2);

    let (metadata, message, execution) = fixture.send_success();
    assert_resource_row(
        "four-actor-batch-clearing",
        &fixture,
        &metadata,
        &message,
        &execution,
        32,
        FOUR_ACTOR_BATCH_HEAP_PEAK_BYTES,
    );
    let receipt =
        assert_expected_receipt(&metadata, &fixture, &[(0, 1, 10_001), (2, 3, 20_002)], 0);
    assert_moves(&receipt.moves, &[(0, 1, 10_001), (2, 3, 20_002)]);
    assert_eq!(fixture.endpoint_balance(0), 0);
    assert_eq!(fixture.endpoint_balance(1), 10_001);
    assert_eq!(fixture.endpoint_balance(2), 0);
    assert_eq!(fixture.endpoint_balance(3), 20_002);
    assert_eq!(fixture.fee_vault_balance(0), 30);
    assert_eq!(fixture.fee_liability(0), 30);
    assert_eq!(fixture.fee_vault_balance(1), 60);
    assert_eq!(fixture.fee_liability(1), 60);
    for slot in 0..4 {
        assert_consumed(&fixture, slot, 1);
    }
}

#[test]
fn zero_decimal_inventory_is_distributed_with_real_domain_accounting() {
    let artifacts = artifacts();
    let mut fixture = ReferenceFixtureCompiler::new(&artifacts, inventory_spec(RECEIPT_ACCEPT));
    let mint = fixture
        .base
        .svm
        .get_account(&fixture.asset_mint(1))
        .expect("inventory mint exists");
    let mint = SplMint::unpack(&mint.data).expect("decode inventory mint");
    assert_eq!(mint.decimals, 0);

    let (metadata, message, execution) = fixture.send_success();
    assert_resource_row(
        "zero-decimal-inventory-distribution",
        &fixture,
        &metadata,
        &message,
        &execution,
        40,
        INVENTORY_HEAP_PEAK_BYTES,
    );
    let receipt = assert_expected_receipt(
        &metadata,
        &fixture,
        &[(0, 1, 2_400_000), (0, 2, 600_000), (3, 4, 3)],
        0,
    );
    assert_moves(
        &receipt.moves,
        &[(0, 1, 2_400_000), (0, 2, 600_000), (3, 4, 3)],
    );
    assert_eq!(fixture.endpoint_balance(0), 0);
    assert_eq!(fixture.endpoint_balance(1), 2_400_000);
    assert_eq!(fixture.endpoint_balance(2), 600_000);
    assert_eq!(fixture.endpoint_balance(3), 0);
    assert_eq!(fixture.endpoint_balance(4), 3);
    assert_eq!(fixture.domain_accounted(0), 2_400_000);
    assert_eq!(fixture.domain_accounted(1), 0);
    assert_eq!(fixture.fee_vault_balance(0), 9_000);
    assert_eq!(fixture.fee_liability(0), 9_000);
    assert_consumed(&fixture, 0, 1);
    assert_eq!(fixture.stored_state(0).constraint_count, 2);
}

#[test]
fn every_reference_semantic_rolls_back_protected_and_opaque_state_on_hostile_failure() {
    let artifacts = artifacts();
    let cases = [
        weighted_spec(RECEIPT_LATE_FAILURE),
        constant_product_spec(RECEIPT_LATE_FAILURE),
        auction_spec(1, 2, RECEIPT_LATE_FAILURE),
        batch_spec(RECEIPT_LATE_FAILURE),
        inventory_spec(RECEIPT_LATE_FAILURE),
    ];

    for spec in cases {
        let mut fixture = ReferenceFixtureCompiler::new(&artifacts, spec);
        let addresses = fixture.rollback_addresses();
        let before = snapshot_accounts(&fixture.base.svm, &addresses);
        let (transaction, _) = fixture.compile_v0();
        let failure = fixture
            .base
            .svm
            .send_transaction(transaction)
            .expect_err("hostile reference semantic unexpectedly committed");
        assert!(program_invoked(
            &failure.meta.logs,
            programmable_generic_effect_core::ID
        ));
        assert!(program_invoked(&failure.meta.logs, effect_engine_probe::ID));
        assert!(!program_invoked(
            &failure.meta.logs,
            litesvm_token::TOKEN_ID
        ));
        assert_eq!(
            snapshot_accounts(&fixture.base.svm, &addresses),
            before,
            "{} hostile execution did not roll back exactly",
            fixture.spec.label,
        );
    }
}

fn artifacts() -> SbfArtifacts {
    SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests")
}

fn weighted_spec(receipt_mode: u8) -> ReferenceFixtureSpec {
    ReferenceFixtureSpec {
        label: "weighted-allocation",
        assets: vec![ReferenceAssetSpec { decimals: 6 }],
        intents: vec![ReferenceIntentSpec {
            maximum_successful_fills: 1,
        }],
        capabilities: vec![
            intent_debit(0, 0, 10_000, 10_030),
            exact_credit(0, 0, 7_500),
            exact_credit(0, 0, 2_500),
        ],
        credit_constraints: vec![
            credit_constraint(0, 1, &[0], 3, 4, 0),
            credit_constraint(0, 2, &[0], 1, 4, 0),
        ],
        opaque: vec![],
        plan: ReferencePlanSpec::Weighted(WeightedAllocationPlan {
            source_capability_index: 0,
            first_destination_capability_index: 1,
            second_destination_capability_index: 2,
            total_amount: 10_000,
            first_weight: 3,
            second_weight: 1,
        }),
        receipt_mode,
    }
}

fn constant_product_spec(receipt_mode: u8) -> ReferenceFixtureSpec {
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
    ReferenceFixtureSpec {
        label: "constant-product",
        assets: vec![
            ReferenceAssetSpec { decimals: 6 },
            ReferenceAssetSpec { decimals: 6 },
        ],
        intents: vec![ReferenceIntentSpec {
            maximum_successful_fills: 1,
        }],
        capabilities: vec![
            intent_debit(0, 0, 100_000, 100_300),
            domain_credit(0, 1_000_000, 1_000_000),
            domain_debit(1, 2_000_000, 2_000_000, 2_000_000),
            exact_credit(1, 0, 181_322),
        ],
        credit_constraints: vec![credit_constraint(0, 1, &[0], 181_322, 100_000, 0)],
        opaque: vec![ReferenceOpaqueSpec {
            address_tag: 220,
            data: state.encode().expect("encode CP reference state").to_vec(),
        }],
        plan: ReferencePlanSpec::ConstantProduct(ConstantProductPlan {
            state_position: 0,
            input_source_capability_index: 0,
            pool_input_capability_index: 1,
            pool_output_capability_index: 2,
            output_destination_capability_index: 3,
            exact_input_amount: 100_000,
        }),
        receipt_mode,
    }
}

fn auction_spec(
    fill_inventory_amount: u64,
    maximum_successful_fills: u32,
    receipt_mode: u8,
) -> ReferenceFixtureSpec {
    let auction = AuctionStateCandidateV0 {
        sequence: 0,
        payment_asset_index: 0,
        inventory_asset_index: 1,
        unit_price_numerator: 2_000,
        nonzero_unit_price_denominator: 3,
        remaining_inventory: 3,
        filled_inventory: 0,
        last_request_digest: [0; 32],
    };
    let order = OrderStateCandidateV0 {
        sequence: 0,
        payment_asset_index: 0,
        inventory_asset_index: 1,
        maximum_unit_price_numerator: 2_000,
        nonzero_maximum_unit_price_denominator: 3,
        remaining_payment: 2_000,
        paid_payment: 0,
        last_request_digest: [0; 32],
    };
    ReferenceFixtureSpec {
        label: "partial-auction",
        assets: vec![
            ReferenceAssetSpec { decimals: 6 },
            ReferenceAssetSpec { decimals: 0 },
        ],
        intents: vec![ReferenceIntentSpec {
            maximum_successful_fills,
        }],
        capabilities: vec![
            intent_debit(0, 0, 2_000, 2_006),
            domain_credit(0, 0, 0),
            domain_debit(1, 3, 3, 3),
            exact_credit(1, 0, 3),
        ],
        credit_constraints: vec![credit_constraint(0, 1, &[0], 1, 667, 0)],
        opaque: vec![
            ReferenceOpaqueSpec {
                address_tag: 221,
                data: auction
                    .encode()
                    .expect("encode auction reference state")
                    .to_vec(),
            },
            ReferenceOpaqueSpec {
                address_tag: 222,
                data: order
                    .encode()
                    .expect("encode order reference state")
                    .to_vec(),
            },
        ],
        plan: ReferencePlanSpec::PartialAuction(PartialAuctionPlan {
            auction_state_position: 0,
            order_state_position: 1,
            payment_source_capability_index: 0,
            payment_destination_capability_index: 1,
            inventory_source_capability_index: 2,
            inventory_destination_capability_index: 3,
            fill_inventory_amount,
        }),
        receipt_mode,
    }
}

fn batch_spec(receipt_mode: u8) -> ReferenceFixtureSpec {
    ReferenceFixtureSpec {
        label: "four-actor-batch-clearing",
        assets: vec![
            ReferenceAssetSpec { decimals: 6 },
            ReferenceAssetSpec { decimals: 6 },
        ],
        intents: (0..4)
            .map(|_| ReferenceIntentSpec {
                maximum_successful_fills: 1,
            })
            .collect(),
        capabilities: vec![
            intent_debit(0, 0, 10_001, 10_031),
            exact_credit(0, 1, 10_001),
            intent_debit(1, 2, 20_002, 20_062),
            exact_credit(1, 3, 20_002),
        ],
        credit_constraints: vec![],
        opaque: vec![],
        plan: ReferencePlanSpec::BatchClearing(BatchClearingPlan {
            first_asset_index: 0,
            second_asset_index: 1,
            second_asset_per_first_numerator: 2,
            nonzero_first_asset_denominator: 1,
        }),
        receipt_mode,
    }
}

fn inventory_spec(receipt_mode: u8) -> ReferenceFixtureSpec {
    ReferenceFixtureSpec {
        label: "zero-decimal-inventory-distribution",
        assets: vec![
            ReferenceAssetSpec { decimals: 6 },
            ReferenceAssetSpec { decimals: 0 },
        ],
        intents: vec![ReferenceIntentSpec {
            maximum_successful_fills: 1,
        }],
        capabilities: vec![
            intent_debit(0, 0, 3_000_000, 3_009_000),
            domain_credit(0, 0, 0),
            exact_credit(0, 0, 600_000),
            domain_debit(1, 3, 3, 3),
            exact_credit(1, 0, 3),
        ],
        credit_constraints: vec![
            credit_constraint(0, 1, &[0], 1, 5, 0),
            credit_constraint(0, 2, &[0], 1, 1_000_000, 0),
        ],
        opaque: vec![],
        plan: ReferencePlanSpec::InventoryDistribution(InventoryDistributionPlan {
            payment_source_capability_index: 0,
            seller_payment_capability_index: 1,
            creator_payment_capability_index: 2,
            inventory_source_capability_index: 3,
            inventory_destination_capability_index: 4,
            inventory_quantity: 3,
            payment_units_per_inventory_unit: 1_000_000,
            seller_basis_points: 8_000,
            creator_basis_points: 2_000,
        }),
        receipt_mode,
    }
}

fn intent_debit(
    asset_index: u8,
    authorization_slot: u8,
    maximum_engine_debit: u64,
    initial_balance: u64,
) -> ReferenceCapabilitySpec {
    assert_eq!(
        initial_balance,
        maximum_engine_debit + protocol_fee(maximum_engine_debit)
    );
    ReferenceCapabilitySpec {
        asset_index,
        initial_balance,
        kind: ReferenceCapabilityKind::IntentDebit {
            authorization_slot,
            maximum_engine_debit,
        },
    }
}

fn exact_credit(
    asset_index: u8,
    authorization_slot: u8,
    minimum_credit: u64,
) -> ReferenceCapabilitySpec {
    ReferenceCapabilitySpec {
        asset_index,
        initial_balance: 0,
        kind: ReferenceCapabilityKind::ExactCredit {
            authorization_slot,
            minimum_credit,
        },
    }
}

fn domain_debit(
    asset_index: u8,
    maximum_engine_debit: u64,
    initial_balance: u64,
    accounted_before: u64,
) -> ReferenceCapabilitySpec {
    ReferenceCapabilitySpec {
        asset_index,
        initial_balance,
        kind: ReferenceCapabilityKind::DomainDebit {
            maximum_engine_debit,
            accounted_before,
        },
    }
}

fn domain_credit(
    asset_index: u8,
    initial_balance: u64,
    accounted_before: u64,
) -> ReferenceCapabilitySpec {
    ReferenceCapabilitySpec {
        asset_index,
        initial_balance,
        kind: ReferenceCapabilityKind::DomainCredit { accounted_before },
    }
}

fn protocol_fee(amount: u64) -> u64 {
    amount * FEE_NUMERATOR / FEE_DENOMINATOR
}

fn credit_constraint(
    authorization_slot: u8,
    credit_local_term_index: u8,
    debit_local_term_indices: &[u8],
    minimum_credit_numerator: u64,
    nonzero_debit_denominator: u64,
    terminal_absolute_minimum: u64,
) -> ReferenceCreditConstraintSpec {
    ReferenceCreditConstraintSpec {
        authorization_slot,
        credit_local_term_index,
        debit_local_term_indices: debit_local_term_indices.to_vec(),
        minimum_credit_numerator,
        nonzero_debit_denominator,
        terminal_absolute_minimum,
    }
}

fn assert_resource_row(
    label: &str,
    fixture: &ReferenceFixtureCompiler,
    metadata: &TransactionMetadata,
    message: &common::V0MessageResources,
    execution: &common::ExecutionResources,
    payload_len: usize,
    measured_heap_peak_bytes: u32,
) {
    assert_eq!(fixture.base.engine_request.payload.len(), payload_len);
    assert_eq!(message.unique_locks, message.resolved_unique_keys.len());
    assert!(contains_program_path(
        metadata,
        &[
            programmable_generic_effect_core::ID,
            effect_engine_probe::ID
        ]
    ));
    assert!(contains_program_path(
        metadata,
        &[
            programmable_generic_effect_core::ID,
            litesvm_token::TOKEN_ID
        ]
    ));
    let actual_request = actual_engine_request(metadata, message);
    assert_eq!(
        actual_request, fixture.base.engine_request,
        "test-only compiler must reconstruct the exact request executed by Core"
    );
    assert_eq!(
        decode_requested_heap_frame(&request_heap_frame_instruction(CONTROLLED_HEAP_FRAME_BYTES)),
        Some(CONTROLLED_HEAP_FRAME_BYTES)
    );
    assert_ne!(measured_heap_peak_bytes, 0);
    eprintln!(
        "REFERENCE_RESOURCE {label}: core_data={} core_positions={} measured_heap_peak={} heap_frame={} instrumented_artifact={} static_keys={:?} lookup_tables={:?} loaded_writable={:?} loaded_readonly={:?} resolved_unique={:?}",
        fixture.base.instruction.data.len(),
        fixture.base.instruction.accounts.len(),
        measured_heap_peak_bytes,
        CONTROLLED_HEAP_FRAME_BYTES,
        INSTRUMENTED_HEAP_ARTIFACT_SHA256,
        message.static_keys,
        message.lookup_table_keys,
        message.loaded_writable_keys,
        message.loaded_readonly_keys,
        message.resolved_unique_keys,
    );
    assert!(
        u128::from(measured_heap_peak_bytes) * 5 <= u128::from(CONTROLLED_HEAP_FRAME_BYTES) * 4,
        "{label}: measured heap peak must leave at least 20% of the authenticated frame free",
    );
    assert_controlled_resource_headroom(label, message, execution);
}

fn actual_engine_request(
    metadata: &TransactionMetadata,
    message: &common::V0MessageResources,
) -> generic_effect_private_wire::EngineRequestCandidateV0 {
    let inner = metadata
        .inner_instructions
        .iter()
        .flatten()
        .find(|inner| {
            message.resolved_unique_keys[usize::from(inner.instruction.program_id_index)]
                == effect_engine_probe::ID
        })
        .expect("Core-to-engine instruction exists");
    generic_effect_private_wire::decode_engine_request(&inner.instruction.data)
        .expect("decode actual Core-to-engine request")
}

fn assert_moves(actual: &[MoveCandidateV0], expected: &[(u8, u8, u64)]) {
    assert_eq!(actual.len(), expected.len());
    for (movement, (source, destination, amount)) in actual.iter().zip(expected) {
        assert_eq!(movement.source_capability_index, *source);
        assert_eq!(movement.destination_capability_index, *destination);
        assert_eq!(movement.amount, *amount);
    }
}

fn assert_expected_receipt(
    metadata: &TransactionMetadata,
    fixture: &ReferenceFixtureCompiler,
    moves: &[(u8, u8, u64)],
    engine_sequence: u64,
) -> EffectReceiptCandidateV0 {
    let request = &fixture.base.engine_request;
    let receipt = EffectReceiptCandidateV0 {
        magic: EFFECT_RECEIPT_MAGIC,
        wire_version: WIRE_VERSION,
        phase: PHASE_TRANSITION,
        flags: 0,
        request_digest: request
            .digest()
            .expect("canonical reference request digest"),
        intent_set_digest: request.header.intent_set_digest,
        protected_execution_root: request.header.protected_execution_root,
        engine_sequence,
        engine_supplied_evidence_digest:
            effect_engine_probe::primary_engine_supplied_evidence_digest(engine_sequence),
        moves: moves
            .iter()
            .map(|(source, destination, amount)| MoveCandidateV0 {
                source_capability_index: *source,
                destination_capability_index: *destination,
                amount: *amount,
            })
            .collect(),
    };
    decode_execution_evidence(metadata, effect_engine_probe::ID, request, &receipt).unwrap_or_else(
        |error| {
            let (core, engine) = common::decode_core_evidence_events(&metadata.logs)
                .expect("decode reference evidence diagnostics");
            let expected_effect = generic_effect_private_wire::compute_canonical_effect_digest(
                &receipt.request_digest,
                &receipt.protected_execution_root,
                &receipt.moves,
            )
            .expect("reference expected effect digest");
            panic!(
                "Core evidence did not bind expected reference receipt: {error}; request={:?}/{:?} intent={:?}/{:?} domain={:?}/{:?} protected={:?}/{:?} opaque={:?}/{:?} effect={:?}/{:?} moves={}/{} engine_supplied={:?}/{:?}",
                core.request_digest,
                receipt.request_digest,
                core.intent_set_digest,
                receipt.intent_set_digest,
                core.domain_set_digest,
                request.header.domain_set_digest,
                core.protected_execution_root,
                receipt.protected_execution_root,
                core.opaque_capability_root,
                request.header.opaque_capability_root,
                core.effect_digest,
                expected_effect,
                core.move_count,
                receipt.moves.len(),
                engine.engine_supplied_digest,
                receipt.engine_supplied_evidence_digest,
            )
        },
    );
    receipt
}

fn move_bytes(moves: &[MoveCandidateV0]) -> Vec<u8> {
    moves.iter().flat_map(MoveCandidateV0::encode).collect()
}

fn assert_consumed(fixture: &ReferenceFixtureCompiler, slot: usize, fill_sequence: u32) {
    let state = fixture.stored_state(slot);
    assert_eq!(state.lifecycle, StoredAuthorizationLifecycle::CONSUMED);
    assert_eq!(state.fill_sequence, fill_sequence);
    assert_eq!(state.pending_execution_digest, [0; 32]);
}

fn program_invoked(logs: &[String], program: anchor_lang::prelude::Pubkey) -> bool {
    let prefix = format!("Program {program} invoke [");
    logs.iter().any(|line| line.starts_with(&prefix))
}
