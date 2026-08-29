use std::collections::{BTreeMap, BTreeSet};

use generic_effect_private_wire::*;

const EXPECTED_SEMANTIC_FIELDS: usize = 150;
const EXPECTED_FIELDS_BY_FAMILY: [(&str, usize); 16] = [
    ("AssetBinding", 6),
    ("AuthorizationSnapshot", 5),
    ("InlineIntentIdentity", 4),
    ("IntentCapabilityTerm", 12),
    ("CreditConstraint", 8),
    ("CapabilityState", 9),
    ("FeeState", 7),
    ("DomainControl", 4),
    ("FeeShard", 5),
    ("SettlementCapability", 15),
    ("EngineAsset", 6),
    ("EngineIntent", 6),
    ("FeePolicy", 6),
    ("Context", 14),
    ("ProtectedCapabilityDigest", 30),
    ("FeeShardDigest", 13),
];

#[derive(Default)]
struct FieldCoverage {
    seen: BTreeSet<(&'static str, &'static str)>,
    changed_encoding: usize,
    rejected_mutation: usize,
}

impl FieldCoverage {
    fn check(
        &mut self,
        family: &'static str,
        field: &'static str,
        baseline: &[u8],
        mutation: WireResult<Vec<u8>>,
    ) {
        assert!(
            self.seen.insert((family, field)),
            "duplicate security-field coverage entry: {family}.{field}"
        );
        match mutation {
            Ok(encoded) => {
                assert_ne!(
                    encoded, baseline,
                    "semantic field is neither rejected nor represented in the immediate row bytes: {family}.{field}"
                );
                self.changed_encoding += 1;
            }
            Err(_) => self.rejected_mutation += 1,
        }
    }

    fn finish(self) {
        let mut actual_by_family = BTreeMap::<&str, usize>::new();
        for (family, _) in &self.seen {
            *actual_by_family.entry(family).or_default() += 1;
        }
        assert_eq!(
            actual_by_family.len(),
            EXPECTED_FIELDS_BY_FAMILY.len(),
            "security-row family coverage changed"
        );
        for (family, expected) in EXPECTED_FIELDS_BY_FAMILY {
            assert_eq!(
                actual_by_family.get(family),
                Some(&expected),
                "semantic field coverage changed for {family}"
            );
        }
        assert_eq!(
            self.seen.len(),
            EXPECTED_SEMANTIC_FIELDS,
            "semantic security-field coverage changed"
        );
        assert_eq!(
            self.changed_encoding + self.rejected_mutation,
            EXPECTED_SEMANTIC_FIELDS
        );
        eprintln!(
            "security row field coverage: {} semantic fields ({} encoding changes, {} validation rejections), 16 families",
            self.seen.len(),
            self.changed_encoding,
            self.rejected_mutation,
        );
    }
}

macro_rules! check_field {
    ($coverage:ident, $family:literal, $baseline:ident, $field:ident, $value:expr) => {{
        let baseline_bytes = $baseline
            .encode()
            .unwrap_or_else(|error| panic!("invalid {} baseline: {error:?}", $family))
            .to_vec();
        let mut mutation = $baseline;
        mutation.$field = $value;
        $coverage.check(
            $family,
            stringify!($field),
            &baseline_bytes,
            mutation.encode().map(|encoded| encoded.to_vec()),
        );
    }};
}

macro_rules! check_nested_field {
    ($coverage:ident, $family:literal, $field:literal, $baseline:ident, $mutation:expr) => {{
        let baseline_bytes = $baseline
            .encode()
            .unwrap_or_else(|error| panic!("invalid {} baseline: {error:?}", $family))
            .to_vec();
        let mut changed = $baseline;
        ($mutation)(&mut changed);
        $coverage.check(
            $family,
            $field,
            &baseline_bytes,
            changed.encode().map(|encoded| encoded.to_vec()),
        );
    }};
}

#[test]
fn all_security_row_fields_are_validated_or_commitment_bound() {
    let mut coverage = FieldCoverage::default();

    let asset = AssetBindingRowCandidateV0 {
        wire_version: WIRE_VERSION,
        flags: 0,
        decimals: 9,
        reserved: 0,
        asset_identity: [1; 32],
        asset_program: [2; 32],
        settlement_profile_digest: [3; 32],
    };
    check_field!(
        coverage,
        "AssetBinding",
        asset,
        wire_version,
        WIRE_VERSION + 1
    );
    check_field!(coverage, "AssetBinding", asset, flags, 1);
    check_field!(coverage, "AssetBinding", asset, decimals, 8);
    check_field!(coverage, "AssetBinding", asset, asset_identity, [4; 32]);
    check_field!(coverage, "AssetBinding", asset, asset_program, [5; 32]);
    check_field!(
        coverage,
        "AssetBinding",
        asset,
        settlement_profile_digest,
        [6; 32]
    );

    let snapshot = AuthorizationSnapshotRowCandidateV0 {
        authorization_slot: 0,
        witness_kind: WITNESS_DIRECT_ACTOR,
        authorization_control_offset_or_none: 0,
        inline_identity_index_or_none: 0,
        expected_fill_sequence: 0,
    };
    check_field!(
        coverage,
        "AuthorizationSnapshot",
        snapshot,
        authorization_slot,
        1
    );
    check_field!(
        coverage,
        "AuthorizationSnapshot",
        snapshot,
        witness_kind,
        WITNESS_EXACT_DELEGATE
    );
    check_field!(
        coverage,
        "AuthorizationSnapshot",
        snapshot,
        authorization_control_offset_or_none,
        1
    );
    check_field!(
        coverage,
        "AuthorizationSnapshot",
        snapshot,
        inline_identity_index_or_none,
        1
    );
    check_field!(
        coverage,
        "AuthorizationSnapshot",
        snapshot,
        expected_fill_sequence,
        1
    );

    let identity = InlineIntentIdentityRowCandidateV0 {
        actor: [7; 32],
        engine_terms_commitment: [8; 32],
        authorization_nonce: 9,
        expires_at_slot_exclusive: 100,
    };
    check_field!(coverage, "InlineIntentIdentity", identity, actor, [9; 32]);
    check_field!(
        coverage,
        "InlineIntentIdentity",
        identity,
        engine_terms_commitment,
        [10; 32]
    );
    check_field!(
        coverage,
        "InlineIntentIdentity",
        identity,
        authorization_nonce,
        10
    );
    check_field!(
        coverage,
        "InlineIntentIdentity",
        identity,
        expires_at_slot_exclusive,
        101
    );

    let term = IntentCapabilityTermRowCandidateV0 {
        intent_local_term_index: 0,
        authority_class: AUTHORITY_INTENT_FUNDED,
        fee_class: FEE_CLASS_GROSS_DEBIT_RATE,
        flags: INTENT_CAPABILITY_TERM_FLAG_FEE_FUNDING,
        rights_bits: RIGHT_DEBIT,
        endpoint_key: [11; 32],
        asset_binding_digest: [12; 32],
        required_domain_descriptor_digest_or_zero: [0; 32],
        maximum_engine_debit: 100,
        maximum_total_debit: 110,
        minimum_credit: 0,
        maximum_protocol_fee: 10,
    };
    check_field!(
        coverage,
        "IntentCapabilityTerm",
        term,
        intent_local_term_index,
        1
    );
    check_field!(
        coverage,
        "IntentCapabilityTerm",
        term,
        authority_class,
        AUTHORITY_EXACT_EXTERNAL_CREDIT
    );
    check_field!(
        coverage,
        "IntentCapabilityTerm",
        term,
        fee_class,
        FEE_CLASS_NONE
    );
    check_field!(
        coverage,
        "IntentCapabilityTerm",
        term,
        flags,
        INTENT_CAPABILITY_TERM_FLAG_FEE_FUNDING
            | INTENT_CAPABILITY_TERM_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT
    );
    check_field!(
        coverage,
        "IntentCapabilityTerm",
        term,
        rights_bits,
        RIGHT_CREDIT
    );
    check_field!(
        coverage,
        "IntentCapabilityTerm",
        term,
        endpoint_key,
        [13; 32]
    );
    check_field!(
        coverage,
        "IntentCapabilityTerm",
        term,
        asset_binding_digest,
        [14; 32]
    );
    check_field!(
        coverage,
        "IntentCapabilityTerm",
        term,
        required_domain_descriptor_digest_or_zero,
        [15; 32]
    );
    check_field!(
        coverage,
        "IntentCapabilityTerm",
        term,
        maximum_engine_debit,
        99
    );
    check_field!(
        coverage,
        "IntentCapabilityTerm",
        term,
        maximum_total_debit,
        111
    );
    check_field!(coverage, "IntentCapabilityTerm", term, minimum_credit, 1);
    check_field!(
        coverage,
        "IntentCapabilityTerm",
        term,
        maximum_protocol_fee,
        9
    );

    let constraint = CreditConstraintRowCandidateV0 {
        constraint_index: 0,
        credit_local_term_index: 1,
        flags: 0,
        debit_source_bitmap: 0b1,
        debit_group_root: compute_intent_debit_group_root(&[0]).expect("debit group root"),
        minimum_credit_numerator: 9,
        nonzero_debit_denominator: 10,
        terminal_absolute_minimum: 100,
    };
    check_field!(
        coverage,
        "CreditConstraint",
        constraint,
        constraint_index,
        1
    );
    check_field!(
        coverage,
        "CreditConstraint",
        constraint,
        credit_local_term_index,
        2
    );
    check_field!(coverage, "CreditConstraint", constraint, flags, 1);
    check_field!(
        coverage,
        "CreditConstraint",
        constraint,
        debit_source_bitmap,
        0b100
    );
    check_field!(
        coverage,
        "CreditConstraint",
        constraint,
        debit_group_root,
        [16; 32]
    );
    check_field!(
        coverage,
        "CreditConstraint",
        constraint,
        minimum_credit_numerator,
        8
    );
    check_field!(
        coverage,
        "CreditConstraint",
        constraint,
        nonzero_debit_denominator,
        11
    );
    check_field!(
        coverage,
        "CreditConstraint",
        constraint,
        terminal_absolute_minimum,
        99
    );

    let capability_state = AuthorizationCapabilityStateRowCandidateV0 {
        local_term_index: 0,
        reserved_0: 0,
        flags: AUTHORIZATION_CAPABILITY_STATE_FLAG_FEE_FUNDING,
        initial_maximum_engine_debit: 100,
        initial_minimum_credit: 0,
        initial_maximum_total_debit: 110,
        remaining_total_debit: 80,
        cumulative_engine_debit: 20,
        cumulative_fee_debit: 10,
        cumulative_credit: 0,
    };
    check_field!(
        coverage,
        "CapabilityState",
        capability_state,
        local_term_index,
        1
    );
    check_field!(coverage, "CapabilityState", capability_state, flags, 0);
    check_field!(
        coverage,
        "CapabilityState",
        capability_state,
        initial_maximum_engine_debit,
        101
    );
    check_field!(
        coverage,
        "CapabilityState",
        capability_state,
        initial_minimum_credit,
        1
    );
    check_field!(
        coverage,
        "CapabilityState",
        capability_state,
        initial_maximum_total_debit,
        111
    );
    check_field!(
        coverage,
        "CapabilityState",
        capability_state,
        remaining_total_debit,
        81
    );
    check_field!(
        coverage,
        "CapabilityState",
        capability_state,
        cumulative_engine_debit,
        21
    );
    check_field!(
        coverage,
        "CapabilityState",
        capability_state,
        cumulative_fee_debit,
        11
    );
    check_field!(
        coverage,
        "CapabilityState",
        capability_state,
        cumulative_credit,
        1
    );

    let fee_state = AuthorizationFeeStateRowCandidateV0 {
        rounding_group_digest: [17; 32],
        funding_local_term_index: 0,
        fee_class: FEE_CLASS_GROSS_DEBIT_RATE,
        flags: 0,
        cumulative_basis: 100,
        cumulative_assessed_fee: 3,
        maximum_fee: 10,
    };
    check_field!(
        coverage,
        "FeeState",
        fee_state,
        rounding_group_digest,
        [18; 32]
    );
    check_field!(coverage, "FeeState", fee_state, funding_local_term_index, 1);
    check_field!(coverage, "FeeState", fee_state, fee_class, FEE_CLASS_NONE);
    check_field!(coverage, "FeeState", fee_state, flags, 1);
    check_field!(coverage, "FeeState", fee_state, cumulative_basis, 101);
    check_field!(coverage, "FeeState", fee_state, cumulative_assessed_fee, 4);
    check_field!(coverage, "FeeState", fee_state, maximum_fee, 11);

    let domain_control = DomainControlRowCandidateV0 {
        descriptor_control_offset: 0,
        admission_control_offset_or_none: NONE_INDEX,
        accounting_control_offset: 1,
        flags: 0,
    };
    check_field!(
        coverage,
        "DomainControl",
        domain_control,
        descriptor_control_offset,
        2
    );
    check_field!(
        coverage,
        "DomainControl",
        domain_control,
        admission_control_offset_or_none,
        2
    );
    check_field!(
        coverage,
        "DomainControl",
        domain_control,
        accounting_control_offset,
        2
    );
    check_field!(coverage, "DomainControl", domain_control, flags, 1);

    let fee_shard = FeeShardRowCandidateV0 {
        descriptor_control_offset: 0,
        liability_control_offset: 1,
        vault_settlement_capability_index: 2,
        asset_index: 0,
        flags: 0,
    };
    check_field!(
        coverage,
        "FeeShard",
        fee_shard,
        descriptor_control_offset,
        1
    );
    check_field!(coverage, "FeeShard", fee_shard, liability_control_offset, 2);
    check_field!(
        coverage,
        "FeeShard",
        fee_shard,
        vault_settlement_capability_index,
        3
    );
    check_field!(coverage, "FeeShard", fee_shard, asset_index, 1);
    check_field!(coverage, "FeeShard", fee_shard, flags, 1);

    let settlement = SettlementCapabilityRowCandidateV0 {
        asset_index: 0,
        domain_index_or_none: NONE_INDEX,
        authorization_slot_or_none: 0,
        intent_local_term_index_or_none: 0,
        authority_class: AUTHORITY_INTENT_FUNDED,
        fee_shard_index_or_none: 0,
        fee_class: FEE_CLASS_GROSS_DEBIT_RATE,
        flags: SETTLEMENT_FLAG_FEE_FUNDING,
        rights_bits: RIGHT_DEBIT,
        domain_accounting_slot_or_none: NONE_INDEX,
        spend_authority_control_offset_or_none: NONE_INDEX,
        reserved_0: 0,
        maximum_engine_debit: 100,
        maximum_total_debit: 110,
        minimum_credit: 0,
        maximum_protocol_fee: 10,
    };
    check_field!(coverage, "SettlementCapability", settlement, asset_index, 1);
    check_field!(
        coverage,
        "SettlementCapability",
        settlement,
        domain_index_or_none,
        0
    );
    check_field!(
        coverage,
        "SettlementCapability",
        settlement,
        authorization_slot_or_none,
        1
    );
    check_field!(
        coverage,
        "SettlementCapability",
        settlement,
        intent_local_term_index_or_none,
        1
    );
    check_field!(
        coverage,
        "SettlementCapability",
        settlement,
        authority_class,
        AUTHORITY_CORE_RESERVED_FEE + 1
    );
    check_field!(
        coverage,
        "SettlementCapability",
        settlement,
        fee_shard_index_or_none,
        1
    );
    check_field!(
        coverage,
        "SettlementCapability",
        settlement,
        fee_class,
        FEE_CLASS_NONE
    );
    check_field!(
        coverage,
        "SettlementCapability",
        settlement,
        flags,
        SETTLEMENT_FLAG_FEE_FUNDING | SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT
    );
    check_field!(
        coverage,
        "SettlementCapability",
        settlement,
        rights_bits,
        RIGHT_CREDIT
    );
    check_field!(
        coverage,
        "SettlementCapability",
        settlement,
        domain_accounting_slot_or_none,
        0
    );
    check_field!(
        coverage,
        "SettlementCapability",
        settlement,
        spend_authority_control_offset_or_none,
        0
    );
    check_field!(
        coverage,
        "SettlementCapability",
        settlement,
        maximum_engine_debit,
        99
    );
    check_field!(
        coverage,
        "SettlementCapability",
        settlement,
        maximum_total_debit,
        111
    );
    check_field!(
        coverage,
        "SettlementCapability",
        settlement,
        minimum_credit,
        1
    );
    check_field!(
        coverage,
        "SettlementCapability",
        settlement,
        maximum_protocol_fee,
        9
    );

    let engine_asset = EngineAssetRowCandidateV0 {
        asset_index: 0,
        asset_flags: 0,
        decimals: 9,
        reserved: 0,
        asset_identity: [19; 32],
        asset_program: [20; 32],
        settlement_profile_digest: [21; 32],
    };
    check_field!(coverage, "EngineAsset", engine_asset, asset_index, 1);
    check_field!(coverage, "EngineAsset", engine_asset, asset_flags, 1);
    check_field!(coverage, "EngineAsset", engine_asset, decimals, 8);
    check_field!(
        coverage,
        "EngineAsset",
        engine_asset,
        asset_identity,
        [22; 32]
    );
    check_field!(
        coverage,
        "EngineAsset",
        engine_asset,
        asset_program,
        [23; 32]
    );
    check_field!(
        coverage,
        "EngineAsset",
        engine_asset,
        settlement_profile_digest,
        [24; 32]
    );

    let engine_intent = EngineIntentRowCandidateV0 {
        authorization_slot: 0,
        identity,
        intent_digest: [25; 32],
    };
    check_field!(
        coverage,
        "EngineIntent",
        engine_intent,
        authorization_slot,
        1
    );
    check_nested_field!(
        coverage,
        "EngineIntent",
        "identity.actor",
        engine_intent,
        |row: &mut EngineIntentRowCandidateV0| row.identity.actor = [26; 32]
    );
    check_nested_field!(
        coverage,
        "EngineIntent",
        "identity.engine_terms_commitment",
        engine_intent,
        |row: &mut EngineIntentRowCandidateV0| row.identity.engine_terms_commitment = [27; 32]
    );
    check_nested_field!(
        coverage,
        "EngineIntent",
        "identity.authorization_nonce",
        engine_intent,
        |row: &mut EngineIntentRowCandidateV0| row.identity.authorization_nonce += 1
    );
    check_nested_field!(
        coverage,
        "EngineIntent",
        "identity.expires_at_slot_exclusive",
        engine_intent,
        |row: &mut EngineIntentRowCandidateV0| row.identity.expires_at_slot_exclusive += 1
    );
    check_field!(
        coverage,
        "EngineIntent",
        engine_intent,
        intent_digest,
        [28; 32]
    );

    let fee_policy = FeePolicyRowCandidateV0 {
        wire_version: WIRE_VERSION,
        rounding_mode: ROUNDING_FLOOR,
        flags: 0,
        revision: 1,
        rate_numerator: 3,
        nonzero_denominator: 1_000,
    };
    check_field!(
        coverage,
        "FeePolicy",
        fee_policy,
        wire_version,
        WIRE_VERSION + 1
    );
    check_field!(
        coverage,
        "FeePolicy",
        fee_policy,
        rounding_mode,
        ROUNDING_CEILING
    );
    check_field!(coverage, "FeePolicy", fee_policy, flags, 1);
    check_field!(coverage, "FeePolicy", fee_policy, revision, 2);
    check_field!(coverage, "FeePolicy", fee_policy, rate_numerator, 4);
    check_field!(
        coverage,
        "FeePolicy",
        fee_policy,
        nonzero_denominator,
        1_001
    );

    let context = EngineContextRowCandidateV0 {
        settlement_capability_index: 0,
        asset_index: 0,
        domain_index_or_none: NONE_INDEX,
        authorization_slot_or_none: 0,
        rights_bits: RIGHT_DEBIT,
        fee_class: FEE_CLASS_GROSS_DEBIT_RATE,
        context_flags: 0,
        endpoint_key: [29; 32],
        observed_before: 100,
        accounted_before_or_zero: 0,
        remaining_maximum_engine_debit: 90,
        remaining_maximum_total_debit: 100,
        remaining_minimum_credit: 0,
        remaining_maximum_protocol_fee: 10,
    };
    check_field!(coverage, "Context", context, settlement_capability_index, 1);
    check_field!(coverage, "Context", context, asset_index, 1);
    check_field!(coverage, "Context", context, domain_index_or_none, 0);
    check_field!(coverage, "Context", context, authorization_slot_or_none, 1);
    check_field!(
        coverage,
        "Context",
        context,
        rights_bits,
        RIGHT_DOMAIN_ACCOUNTED | RIGHT_DEBIT
    );
    check_field!(coverage, "Context", context, fee_class, FEE_CLASS_NONE);
    check_field!(coverage, "Context", context, context_flags, 1);
    check_field!(coverage, "Context", context, endpoint_key, [30; 32]);
    check_field!(coverage, "Context", context, observed_before, 101);
    check_field!(coverage, "Context", context, accounted_before_or_zero, 1);
    check_field!(
        coverage,
        "Context",
        context,
        remaining_maximum_engine_debit,
        89
    );
    check_field!(
        coverage,
        "Context",
        context,
        remaining_maximum_total_debit,
        101
    );
    check_field!(coverage, "Context", context, remaining_minimum_credit, 1);
    check_field!(
        coverage,
        "Context",
        context,
        remaining_maximum_protocol_fee,
        9
    );

    let protected = ProtectedCapabilityDigestRowCandidateV0 {
        capability_position: 0,
        asset_index: 0,
        domain_index_or_none: NONE_INDEX,
        authorization_slot_or_none: 0,
        authority_class: AUTHORITY_INTENT_FUNDED,
        fee_class: FEE_CLASS_GROSS_DEBIT_RATE,
        fee_shard_index_or_none: 0,
        flags: SETTLEMENT_FLAG_FEE_FUNDING,
        rights_bits: RIGHT_DEBIT,
        domain_accounting_slot_or_none: NONE_INDEX,
        spend_control_offset_or_none: 2,
        endpoint_executable: false,
        effective_signer: false,
        effective_writable: true,
        endpoint_key: [31; 32],
        endpoint_owner: [32; 32],
        transfer_authority_key_or_zero: [33; 32],
        asset_identity: [34; 32],
        asset_program: [35; 32],
        settlement_profile_digest: [36; 32],
        domain_descriptor_or_zero: [0; 32],
        domain_admission_digest_or_zero: [0; 32],
        lifecycle_digest: [37; 32],
        domain_revision: 0,
        maximum_engine_debit: 1_000,
        maximum_total_debit: 1_100,
        minimum_credit: 0,
        maximum_protocol_fee: 100,
        fee_policy_revision: 7,
        accounted_before_or_zero: 0,
    };
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        capability_position,
        1
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        asset_index,
        1
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        domain_index_or_none,
        0
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        authorization_slot_or_none,
        1
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        authority_class,
        AUTHORITY_CORE_RESERVED_FEE + 1
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        fee_class,
        FEE_CLASS_NONE
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        fee_shard_index_or_none,
        1
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        flags,
        SETTLEMENT_FLAG_FEE_FUNDING | SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        rights_bits,
        RIGHT_CREDIT
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        domain_accounting_slot_or_none,
        0
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        spend_control_offset_or_none,
        1
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        endpoint_executable,
        true
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        effective_signer,
        true
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        effective_writable,
        false
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        endpoint_key,
        [38; 32]
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        endpoint_owner,
        [39; 32]
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        transfer_authority_key_or_zero,
        [40; 32]
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        asset_identity,
        [41; 32]
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        asset_program,
        [42; 32]
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        settlement_profile_digest,
        [43; 32]
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        domain_descriptor_or_zero,
        [44; 32]
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        domain_admission_digest_or_zero,
        [45; 32]
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        lifecycle_digest,
        [46; 32]
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        domain_revision,
        1
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        maximum_engine_debit,
        999
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        maximum_total_debit,
        1_101
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        minimum_credit,
        1
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        maximum_protocol_fee,
        99
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        fee_policy_revision,
        8
    );
    check_field!(
        coverage,
        "ProtectedCapabilityDigest",
        protected,
        accounted_before_or_zero,
        1
    );

    let fee_shard_digest = FeeShardDigestRowCandidateV0 {
        shard_index: 0,
        asset_index: 0,
        vault_settlement_capability_index: 2,
        flags: 0,
        descriptor_key: [47; 32],
        descriptor_digest: [48; 32],
        liability_key: [49; 32],
        vault_key: [50; 32],
        asset_binding_digest: [51; 32],
        fee_policy_digest: [52; 32],
        recipient_policy_digest: [53; 32],
        fee_policy_revision: 7,
        liability_before: 500,
    };
    check_field!(coverage, "FeeShardDigest", fee_shard_digest, shard_index, 1);
    check_field!(coverage, "FeeShardDigest", fee_shard_digest, asset_index, 1);
    check_field!(
        coverage,
        "FeeShardDigest",
        fee_shard_digest,
        vault_settlement_capability_index,
        3
    );
    check_field!(coverage, "FeeShardDigest", fee_shard_digest, flags, 1);
    check_field!(
        coverage,
        "FeeShardDigest",
        fee_shard_digest,
        descriptor_key,
        [54; 32]
    );
    check_field!(
        coverage,
        "FeeShardDigest",
        fee_shard_digest,
        descriptor_digest,
        [55; 32]
    );
    check_field!(
        coverage,
        "FeeShardDigest",
        fee_shard_digest,
        liability_key,
        [56; 32]
    );
    check_field!(
        coverage,
        "FeeShardDigest",
        fee_shard_digest,
        vault_key,
        [57; 32]
    );
    check_field!(
        coverage,
        "FeeShardDigest",
        fee_shard_digest,
        asset_binding_digest,
        [58; 32]
    );
    check_field!(
        coverage,
        "FeeShardDigest",
        fee_shard_digest,
        fee_policy_digest,
        [59; 32]
    );
    check_field!(
        coverage,
        "FeeShardDigest",
        fee_shard_digest,
        recipient_policy_digest,
        [60; 32]
    );
    check_field!(
        coverage,
        "FeeShardDigest",
        fee_shard_digest,
        fee_policy_revision,
        8
    );
    check_field!(
        coverage,
        "FeeShardDigest",
        fee_shard_digest,
        liability_before,
        501
    );

    // These are structural reserved fields, not semantic inputs. Keep their
    // fail-closed behavior adjacent to the semantic matrix without counting
    // them as commitment fields.
    let mut reserved_asset = asset;
    reserved_asset.reserved = 1;
    assert!(reserved_asset.encode().is_err());
    let mut reserved_capability_state = capability_state;
    reserved_capability_state.reserved_0 = 1;
    assert!(reserved_capability_state.encode().is_err());
    let mut reserved_settlement = settlement;
    reserved_settlement.reserved_0 = 1;
    assert!(reserved_settlement.encode().is_err());
    let mut reserved_engine_asset = engine_asset;
    reserved_engine_asset.reserved = 1;
    assert!(reserved_engine_asset.encode().is_err());

    coverage.finish();
}
