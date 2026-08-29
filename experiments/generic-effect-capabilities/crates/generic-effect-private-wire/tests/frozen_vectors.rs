use generic_effect_private_wire::*;

const ASSET_BINDING_HEX: &str = concat!(
    "00000900",
    "1111111111111111111111111111111111111111111111111111111111111111",
    "2222222222222222222222222222222222222222222222222222222222222222",
    "3333333333333333333333333333333333333333333333333333333333333333",
);
const INTENT_TERM_HEX: &str = concat!(
    "0000010101000000",
    "4444444444444444444444444444444444444444444444444444444444444444",
    "5555555555555555555555555555555555555555555555555555555555555555",
    "6666666666666666666666666666666666666666666666666666666666666666",
    "e8030000000000004c0400000000000000000000000000006400000000000000",
);
const DEBIT_GROUP_ROOT_HEX: &str =
    "b82c20af815c60f529f3d7edb0fcdfcb691093d4028e535433f467b036391cb9";
const CREDIT_CONSTRAINT_HEX: &str = concat!(
    "0002000000000300",
    "b82c20af815c60f529f3d7edb0fcdfcb691093d4028e535433f467b036391cb9",
    "070000000000000003000000000000000500000000000000",
);
const CAPABILITY_STATE_HEX: &str = concat!(
    "0000010000000000",
    "e80300000000000000000000000000004c040000000000001004000000000000",
    "320000000000000000000000000000000a000000000000000000000000000000",
    "00000000000000000000000000000000",
);
const FEE_STATE_HEX: &str = concat!(
    "7777777777777777777777777777777777777777777777777777777777777777",
    "0001000000000000",
    "f401000000000000000000000000000019000000000000000000000000000000",
    "6400000000000000",
);
const AUTHORIZATION_SNAPSHOT_HEX: &str = "010203ff44332211";
const INLINE_IDENTITY_HEX: &str = concat!(
    "8888888888888888888888888888888888888888888888888888888888888888",
    "9900000000000000000000000000000000000000000000000000000000000000",
    "08070605040302011817161514131211",
);
const DOMAIN_CONTROL_HEX: &str = "01ff020000000000";
const FEE_SHARD_HEX: &str = "0102030000000000";
const SETTLEMENT_CAPABILITY_HEX: &str = concat!(
    "00ff0001000001010100ff0200000000",
    "e8030000000000004c0400000000000000000000000000006400000000000000",
);
const PROTECTED_CAPABILITY_HEX: &str = concat!(
    "0000ff00000100010100ff0200000100",
    "1010101010101010101010101010101010101010101010101010101010101010",
    "1111111111111111111111111111111111111111111111111111111111111111",
    "1212121212121212121212121212121212121212121212121212121212121212",
    "1313131313131313131313131313131313131313131313131313131313131313",
    "1414141414141414141414141414141414141414141414141414141414141414",
    "1515151515151515151515151515151515151515151515151515151515151515",
    "0000000000000000000000000000000000000000000000000000000000000000",
    "0000000000000000000000000000000000000000000000000000000000000000",
    "1616161616161616161616161616161616161616161616161616161616161616",
    "0000000000000000e8030000000000004c040000000000000000000000000000",
    "6400000000000000070000000000000000000000000000000000000000000000",
);
const FEE_SHARD_DIGEST_HEX: &str = concat!(
    "0000030000000000",
    "2121212121212121212121212121212121212121212121212121212121212121",
    "2222222222222222222222222222222222222222222222222222222222222222",
    "2323232323232323232323232323232323232323232323232323232323232323",
    "2424242424242424242424242424242424242424242424242424242424242424",
    "2525252525252525252525252525252525252525252525252525252525252525",
    "2626262626262626262626262626262626262626262626262626262626262626",
    "2727272727272727272727272727272727272727272727272727272727272727",
    "0700000000000000f4010000000000000000000000000000",
);
const ENGINE_INTENT_HEX: &str = concat!(
    "0000000000000000",
    "3131313131313131313131313131313131313131313131313131313131313131",
    "3232323232323232323232323232323232323232323232323232323232323232",
    "05000000000000006300000000000000",
    "b6d6b07410f8d99c9f78987e380b61c16b5124b073d8486d70eb88623a4a7abb",
);
const FEE_POLICY_HEX: &str = "000100000000000007000000000000000300000000000000e803000000000000";
const ENGINE_CONTEXT_HEX: &str = concat!(
    "0100ff000a000000",
    "4141414141414141414141414141414141414141414141414141414141414141",
    "6400000000000000000000000000000000000000000000000000000000000000",
    "32000000000000000000000000000000",
);
const AUTHORIZATION_VIEW_HEX: &str = concat!(
    "0000000000000000",
    "5151515151515151515151515151515151515151515151515151515151515151",
    "5252525252525252525252525252525252525252525252525252525252525252",
);

fn nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => panic!("frozen vector contains a non-hex byte"),
    }
}

fn hex_bytes(value: &str) -> Vec<u8> {
    let compact = value
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    assert_eq!(compact.len() % 2, 0, "frozen hex must have full bytes");
    compact
        .chunks_exact(2)
        .map(|pair| (nibble(pair[0]) << 4) | nibble(pair[1]))
        .collect()
}

fn fixed<const N: usize>(value: &str) -> [u8; N] {
    hex_bytes(value)
        .try_into()
        .unwrap_or_else(|bytes: Vec<u8>| panic!("expected {N} frozen bytes, got {}", bytes.len()))
}

fn assert_mutation_and_trailing_rejected<const N: usize, T>(
    expected: [u8; N],
    mutation_offset: usize,
    mutation_value: u8,
    decode: impl Fn(&[u8]) -> WireResult<T>,
) {
    assert_ne!(expected[mutation_offset], mutation_value);
    let mut mutation = expected;
    mutation[mutation_offset] = mutation_value;
    assert!(decode(&mutation).is_err());

    let mut trailing = expected.to_vec();
    trailing.push(0);
    assert!(decode(&trailing).is_err());
}

fn identity_for_engine_view() -> InlineIntentIdentityRowCandidateV0 {
    InlineIntentIdentityRowCandidateV0 {
        actor: [0x31; 32],
        engine_terms_commitment: [0x32; 32],
        authorization_nonce: 5,
        expires_at_slot_exclusive: 99,
    }
}

fn protected_intent_debit() -> ProtectedCapabilityDigestRowCandidateV0 {
    ProtectedCapabilityDigestRowCandidateV0 {
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
        endpoint_key: [0x10; 32],
        endpoint_owner: [0x11; 32],
        transfer_authority_key_or_zero: [0x12; 32],
        asset_identity: [0x13; 32],
        asset_program: [0x14; 32],
        settlement_profile_digest: [0x15; 32],
        domain_descriptor_or_zero: [0; 32],
        domain_admission_digest_or_zero: [0; 32],
        lifecycle_digest: [0x16; 32],
        domain_revision: 0,
        maximum_engine_debit: 1_000,
        maximum_total_debit: 1_100,
        minimum_credit: 0,
        maximum_protocol_fee: 100,
        fee_policy_revision: 7,
        accounted_before_or_zero: 0,
    }
}

#[test]
fn persistent_and_mutable_rows_match_frozen_bytes() {
    let asset = AssetBindingRowCandidateV0 {
        wire_version: WIRE_VERSION,
        flags: 0,
        decimals: 9,
        reserved: 0,
        asset_identity: [0x11; 32],
        asset_program: [0x22; 32],
        settlement_profile_digest: [0x33; 32],
    };
    let expected_asset = fixed::<ASSET_BINDING_ROW_LEN>(ASSET_BINDING_HEX);
    assert_eq!(asset.encode().unwrap(), expected_asset);
    assert_eq!(
        AssetBindingRowCandidateV0::decode_exact(&expected_asset),
        Ok(asset)
    );
    assert_mutation_and_trailing_rejected(expected_asset, 3, 1, |bytes| {
        AssetBindingRowCandidateV0::decode_exact(bytes)
    });

    let term = IntentCapabilityTermRowCandidateV0 {
        intent_local_term_index: 0,
        authority_class: AUTHORITY_INTENT_FUNDED,
        fee_class: FEE_CLASS_GROSS_DEBIT_RATE,
        flags: INTENT_CAPABILITY_TERM_FLAG_FEE_FUNDING,
        rights_bits: RIGHT_DEBIT,
        endpoint_key: [0x44; 32],
        asset_binding_digest: [0x55; 32],
        required_domain_descriptor_digest_or_zero: [0x66; 32],
        maximum_engine_debit: 1_000,
        maximum_total_debit: 1_100,
        minimum_credit: 0,
        maximum_protocol_fee: 100,
    };
    let expected_term = fixed::<INTENT_CAPABILITY_TERM_ROW_LEN>(INTENT_TERM_HEX);
    assert_eq!(term.encode().unwrap(), expected_term);
    assert_eq!(
        IntentCapabilityTermRowCandidateV0::decode_exact(&expected_term),
        Ok(term)
    );
    assert_mutation_and_trailing_rejected(expected_term, 6, 1, |bytes| {
        IntentCapabilityTermRowCandidateV0::decode_exact(bytes)
    });

    let debit_group_root = fixed::<32>(DEBIT_GROUP_ROOT_HEX);
    assert_eq!(
        compute_intent_debit_group_root(&[0, 1]).unwrap(),
        debit_group_root
    );
    let constraint = CreditConstraintRowCandidateV0 {
        constraint_index: 0,
        credit_local_term_index: 2,
        flags: 0,
        debit_source_bitmap: 0b11,
        debit_group_root,
        minimum_credit_numerator: 7,
        nonzero_debit_denominator: 3,
        terminal_absolute_minimum: 5,
    };
    let expected_constraint = fixed::<CREDIT_CONSTRAINT_ROW_LEN>(CREDIT_CONSTRAINT_HEX);
    assert_eq!(constraint.encode().unwrap(), expected_constraint);
    assert_eq!(
        CreditConstraintRowCandidateV0::decode_exact(&expected_constraint),
        Ok(constraint)
    );
    assert_mutation_and_trailing_rejected(expected_constraint, 3, 1, |bytes| {
        CreditConstraintRowCandidateV0::decode_exact(bytes)
    });

    let capability_state = AuthorizationCapabilityStateRowCandidateV0 {
        local_term_index: 0,
        reserved_0: 0,
        flags: AUTHORIZATION_CAPABILITY_STATE_FLAG_FEE_FUNDING,
        initial_maximum_engine_debit: 1_000,
        initial_minimum_credit: 0,
        initial_maximum_total_debit: 1_100,
        remaining_total_debit: 1_040,
        cumulative_engine_debit: 50,
        cumulative_fee_debit: 10,
        cumulative_credit: 0,
    };
    let expected_capability_state =
        fixed::<AUTHORIZATION_CAPABILITY_STATE_ROW_LEN>(CAPABILITY_STATE_HEX);
    assert_eq!(
        capability_state.encode().unwrap(),
        expected_capability_state
    );
    assert_eq!(
        AuthorizationCapabilityStateRowCandidateV0::decode_exact(&expected_capability_state),
        Ok(capability_state)
    );
    assert_mutation_and_trailing_rejected(expected_capability_state, 3, 1, |bytes| {
        AuthorizationCapabilityStateRowCandidateV0::decode_exact(bytes)
    });

    let fee_state = AuthorizationFeeStateRowCandidateV0 {
        rounding_group_digest: [0x77; 32],
        funding_local_term_index: 0,
        fee_class: FEE_CLASS_GROSS_DEBIT_RATE,
        flags: 0,
        cumulative_basis: 500,
        cumulative_assessed_fee: 25,
        maximum_fee: 100,
    };
    let expected_fee_state = fixed::<AUTHORIZATION_FEE_STATE_ROW_LEN>(FEE_STATE_HEX);
    assert_eq!(fee_state.encode().unwrap(), expected_fee_state);
    assert_eq!(
        AuthorizationFeeStateRowCandidateV0::decode_exact(&expected_fee_state),
        Ok(fee_state)
    );
    assert_mutation_and_trailing_rejected(expected_fee_state, 35, 1, |bytes| {
        AuthorizationFeeStateRowCandidateV0::decode_exact(bytes)
    });
}

#[test]
fn envelope_rows_match_frozen_bytes() {
    let snapshot = AuthorizationSnapshotRowCandidateV0 {
        authorization_slot: 1,
        witness_kind: WITNESS_STORED_AUTHORIZATION,
        authorization_control_offset_or_none: 3,
        inline_identity_index_or_none: NONE_INDEX,
        expected_fill_sequence: 0x1122_3344,
    };
    let expected_snapshot = fixed::<AUTHORIZATION_SNAPSHOT_ROW_LEN>(AUTHORIZATION_SNAPSHOT_HEX);
    assert_eq!(snapshot.encode().unwrap(), expected_snapshot);
    assert_eq!(
        AuthorizationSnapshotRowCandidateV0::decode_exact(&expected_snapshot),
        Ok(snapshot)
    );
    assert_mutation_and_trailing_rejected(expected_snapshot, 1, u8::MAX, |bytes| {
        AuthorizationSnapshotRowCandidateV0::decode_exact(bytes)
    });

    let mut engine_terms_commitment = [0; 32];
    engine_terms_commitment[0] = 0x99;
    let identity = InlineIntentIdentityRowCandidateV0 {
        actor: [0x88; 32],
        engine_terms_commitment,
        authorization_nonce: 0x0102_0304_0506_0708,
        expires_at_slot_exclusive: 0x1112_1314_1516_1718,
    };
    let expected_identity = fixed::<INLINE_INTENT_IDENTITY_ROW_LEN>(INLINE_IDENTITY_HEX);
    assert_eq!(identity.encode().unwrap(), expected_identity);
    assert_eq!(
        InlineIntentIdentityRowCandidateV0::decode_exact(&expected_identity),
        Ok(identity)
    );
    assert_mutation_and_trailing_rejected(expected_identity, 32, 0, |bytes| {
        InlineIntentIdentityRowCandidateV0::decode_exact(bytes)
    });

    let domain = DomainControlRowCandidateV0 {
        descriptor_control_offset: 1,
        admission_control_offset_or_none: NONE_INDEX,
        accounting_control_offset: 2,
        flags: 0,
    };
    let expected_domain = fixed::<DOMAIN_CONTROL_ROW_LEN>(DOMAIN_CONTROL_HEX);
    assert_eq!(domain.encode().unwrap(), expected_domain);
    assert_eq!(
        DomainControlRowCandidateV0::decode_exact(&expected_domain),
        Ok(domain)
    );
    assert_mutation_and_trailing_rejected(expected_domain, 4, 1, |bytes| {
        DomainControlRowCandidateV0::decode_exact(bytes)
    });

    let shard = FeeShardRowCandidateV0 {
        descriptor_control_offset: 1,
        liability_control_offset: 2,
        vault_settlement_capability_index: 3,
        asset_index: 0,
        flags: 0,
    };
    let expected_shard = fixed::<FEE_SHARD_ROW_LEN>(FEE_SHARD_HEX);
    assert_eq!(shard.encode().unwrap(), expected_shard);
    assert_eq!(
        FeeShardRowCandidateV0::decode_exact(&expected_shard),
        Ok(shard)
    );
    assert_mutation_and_trailing_rejected(expected_shard, 5, 1, |bytes| {
        FeeShardRowCandidateV0::decode_exact(bytes)
    });

    let settlement = SettlementCapabilityRowCandidateV0 {
        asset_index: 0,
        domain_index_or_none: NONE_INDEX,
        authorization_slot_or_none: 0,
        intent_local_term_index_or_none: 1,
        authority_class: AUTHORITY_INTENT_FUNDED,
        fee_shard_index_or_none: 0,
        fee_class: FEE_CLASS_GROSS_DEBIT_RATE,
        flags: SETTLEMENT_FLAG_FEE_FUNDING,
        rights_bits: RIGHT_DEBIT,
        domain_accounting_slot_or_none: NONE_INDEX,
        spend_authority_control_offset_or_none: 2,
        reserved_0: 0,
        maximum_engine_debit: 1_000,
        maximum_total_debit: 1_100,
        minimum_credit: 0,
        maximum_protocol_fee: 100,
    };
    let expected_settlement = fixed::<SETTLEMENT_CAPABILITY_ROW_LEN>(SETTLEMENT_CAPABILITY_HEX);
    assert_eq!(settlement.encode().unwrap(), expected_settlement);
    assert_eq!(
        SettlementCapabilityRowCandidateV0::decode_exact(&expected_settlement),
        Ok(settlement)
    );
    assert_mutation_and_trailing_rejected(expected_settlement, 12, 1, |bytes| {
        SettlementCapabilityRowCandidateV0::decode_exact(bytes)
    });
}

#[test]
fn landing_time_digest_rows_match_frozen_bytes() {
    let protected = protected_intent_debit();
    let expected_protected = fixed::<PROTECTED_CAPABILITY_DIGEST_ROW_LEN>(PROTECTED_CAPABILITY_HEX);
    assert_eq!(protected.encode().unwrap(), expected_protected);
    assert_eq!(
        ProtectedCapabilityDigestRowCandidateV0::decode_exact(&expected_protected),
        Ok(protected)
    );
    assert_mutation_and_trailing_rejected(expected_protected, 15, 1, |bytes| {
        ProtectedCapabilityDigestRowCandidateV0::decode_exact(bytes)
    });

    let shard = FeeShardDigestRowCandidateV0 {
        shard_index: 0,
        asset_index: 0,
        vault_settlement_capability_index: 3,
        flags: 0,
        descriptor_key: [0x21; 32],
        descriptor_digest: [0x22; 32],
        liability_key: [0x23; 32],
        vault_key: [0x24; 32],
        asset_binding_digest: [0x25; 32],
        fee_policy_digest: [0x26; 32],
        recipient_policy_digest: [0x27; 32],
        fee_policy_revision: 7,
        liability_before: 500,
    };
    let expected_shard = fixed::<FEE_SHARD_DIGEST_ROW_LEN>(FEE_SHARD_DIGEST_HEX);
    assert_eq!(shard.encode().unwrap(), expected_shard);
    assert_eq!(
        FeeShardDigestRowCandidateV0::decode_exact(&expected_shard),
        Ok(shard)
    );
    assert_mutation_and_trailing_rejected(expected_shard, 4, 1, |bytes| {
        FeeShardDigestRowCandidateV0::decode_exact(bytes)
    });
}

#[test]
fn engine_rows_and_stored_view_match_frozen_bytes() {
    let asset = EngineAssetRowCandidateV0 {
        asset_index: 0,
        asset_flags: 0,
        decimals: 9,
        reserved: 0,
        asset_identity: [0x11; 32],
        asset_program: [0x22; 32],
        settlement_profile_digest: [0x33; 32],
    };
    let expected_asset = fixed::<ENGINE_ASSET_ROW_LEN>(ASSET_BINDING_HEX);
    assert_eq!(asset.encode().unwrap(), expected_asset);
    assert_eq!(
        EngineAssetRowCandidateV0::decode_exact(&expected_asset),
        Ok(asset)
    );
    assert_mutation_and_trailing_rejected(expected_asset, 3, 1, |bytes| {
        EngineAssetRowCandidateV0::decode_exact(bytes)
    });

    let identity = identity_for_engine_view();
    let capability_terms_root = [0x61; 32];
    let credit_constraints_root = [0x62; 32];
    let core_terms_root =
        fixed::<32>("9b1deb3afca1c77747550492d2609a3818e89999a9e12c67101e9ccbf2c6cd1e");
    assert_eq!(
        compute_intent_core_terms_root(IntentCoreTermsDigestInputs {
            maximum_successful_fills: 3,
            capability_terms_root: &capability_terms_root,
            credit_constraints_root: &credit_constraints_root,
        })
        .unwrap(),
        core_terms_root
    );
    let intent_digest =
        fixed::<32>("b6d6b07410f8d99c9f78987e380b61c16b5124b073d8486d70eb88623a4a7abb");
    assert_eq!(
        compute_intent_digest(IntentDigestInputs {
            core_program: &DISPOSABLE_CORE_PROGRAM_ID.to_bytes(),
            market_binding_digest: &[0x63; 32],
            loader_state_snapshot_digest: &[0x64; 32],
            fee_policy_digest: &[0x65; 32],
            identity: &identity,
            core_terms_root: &core_terms_root,
        })
        .unwrap(),
        intent_digest
    );

    let stored = InitializeStoredAuthorizationArgsCandidateV0 {
        wire_version: WIRE_VERSION,
        term_count: 1,
        constraint_count: 0,
        flags: 0,
        maximum_successful_fills: 3,
        identity,
        market_binding_digest: [0x63; 32],
        engine_loader_state_snapshot_digest: [0x64; 32],
        fee_policy_digest: [0x65; 32],
        intent_capability_terms_root: capability_terms_root,
        credit_constraints_root,
        core_terms_root,
        intent_digest,
    };
    let stored =
        InitializeStoredAuthorizationArgsCandidateV0::decode_exact(&stored.encode().unwrap())
            .unwrap();
    let direct_view = EngineIntentRowCandidateV0 {
        authorization_slot: 0,
        identity,
        intent_digest,
    };
    let stored_view = EngineIntentRowCandidateV0 {
        authorization_slot: 0,
        identity: stored.identity,
        intent_digest: stored.intent_digest,
    };
    let expected_intent = fixed::<ENGINE_INTENT_ROW_LEN>(ENGINE_INTENT_HEX);
    assert_eq!(direct_view.encode().unwrap(), expected_intent);
    assert_eq!(stored_view.encode().unwrap(), expected_intent);
    assert_eq!(
        EngineIntentRowCandidateV0::decode_exact(&expected_intent),
        Ok(direct_view)
    );
    assert_mutation_and_trailing_rejected(expected_intent, 1, 1, |bytes| {
        EngineIntentRowCandidateV0::decode_exact(bytes)
    });

    let policy = FeePolicyRowCandidateV0 {
        wire_version: WIRE_VERSION,
        rounding_mode: ROUNDING_CEILING,
        flags: 0,
        revision: 7,
        rate_numerator: 3,
        nonzero_denominator: 1_000,
    };
    let expected_policy = fixed::<ENGINE_FEE_POLICY_ROW_LEN>(FEE_POLICY_HEX);
    assert_eq!(policy.encode().unwrap(), expected_policy);
    assert_eq!(
        FeePolicyRowCandidateV0::decode_exact(&expected_policy),
        Ok(policy)
    );
    assert_mutation_and_trailing_rejected(expected_policy, 3, 1, |bytes| {
        FeePolicyRowCandidateV0::decode_exact(bytes)
    });

    let context = EngineContextRowCandidateV0 {
        settlement_capability_index: 1,
        asset_index: 0,
        domain_index_or_none: NONE_INDEX,
        authorization_slot_or_none: 0,
        rights_bits: RIGHT_EXACT_EXTERNAL_RECIPIENT | RIGHT_CREDIT,
        fee_class: FEE_CLASS_NONE,
        context_flags: 0,
        endpoint_key: [0x41; 32],
        observed_before: 100,
        accounted_before_or_zero: 0,
        remaining_maximum_engine_debit: 0,
        remaining_maximum_total_debit: 0,
        remaining_minimum_credit: 50,
        remaining_maximum_protocol_fee: 0,
    };
    let expected_context = fixed::<ENGINE_CONTEXT_ROW_LEN>(ENGINE_CONTEXT_HEX);
    assert_eq!(context.encode().unwrap(), expected_context);
    assert_eq!(
        EngineContextRowCandidateV0::decode_exact(&expected_context),
        Ok(context)
    );
    assert_mutation_and_trailing_rejected(expected_context, 7, 1, |bytes| {
        EngineContextRowCandidateV0::decode_exact(bytes)
    });

    let authorization_view = AuthorizationViewRowCandidateV0 {
        authorization_slot: 0,
        intent_digest: [0x51; 32],
        authorization_state_digest: [0x52; 32],
    };
    let expected_view = fixed::<AUTHORIZATION_VIEW_ROW_LEN>(AUTHORIZATION_VIEW_HEX);
    assert_eq!(authorization_view.encode(), expected_view);
    assert_eq!(
        AuthorizationViewRowCandidateV0::decode_exact(&expected_view),
        Ok(authorization_view)
    );
    assert_mutation_and_trailing_rejected(expected_view, 1, 1, |bytes| {
        AuthorizationViewRowCandidateV0::decode_exact(bytes)
    });
}

const PROTECTED_EXECUTION_PREIMAGE_HEX: &str = concat!(
    "70726f6772616d6d61626c652f707269766174652d6566666563742d63617061",
    "62696c69746965732f323032362d30382d3238160070726f7465637465642d65",
    "7865637574696f6e2d76300b0020000000010101010101010101010101010101",
    "0101010101010101010101010101010101040000000000000020000000020202",
    "0202020202020202020202020202020202020202020202020202020202200000",
    "0003030303030303030303030303030303030303030303030303030303030303",
    "0320000000040404040404040404040404040404040404040404040404040404",
    "0404040404200000000505050505050505050505050505050505050505050505",
    "0505050505050505052000000006060606060606060606060606060606060606",
    "0606060606060606060606060620000000070707070707070707070707070707",
    "0707070707070707070707070707070707200000000808080808080808080808",
    "0808080808080808080808080808080808080808082000000009090909090909",
    "09090909090909090909090909090909090909090909090909200000000a0a0a",
    "0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a",
);
const PROTECTED_EXECUTION_DIGEST_HEX: &str =
    "337b2b2ea802b63a68541c3e9a496d13756ef9faa2fde37390f00421c34bd59c";
const FEE_ASSESSMENT_PREIMAGE_HEX: &str = concat!(
    "70726f6772616d6d61626c652f707269766174652d6566666563742d63617061",
    "62696c69746965732f323032362d30382d323811006665652d6173736573736d",
    "656e742d76300f00200000000101010101010101010101010101010101010101",
    "0101010101010101010101010400000000000000200000000202020202020202",
    "0202020202020202020202020202020202020202020202022000000003030303",
    "0303030303030303030303030303030303030303030303030303030308000000",
    "0700000000000000200000000404040404040404040404040404040404040404",
    "0404040404040404040404042000000005050505050505050505050505050505",
    "0505050505050505050505050505050520000000060606060606060606060606",
    "0606060606060606060606060606060606060606200000000707070707070707",
    "0707070707070707070707070707070707070707070707072000000008080808",
    "0808080808080808080808080808080808080808080808080808080804000000",
    "09000000100000000a000000000000000000000000000000100000000b000000",
    "0000000000000000000000001000000015000000000000000000000000000000",
    "080000000200000000000000",
);
const FEE_ASSESSMENT_DIGEST_HEX: &str =
    "c93582f48a70622db0c2933cf7eaccd240da3dd0672f8f653a9400415060ad19";
const FEE_ASSESSMENT_SET_ROW_HEX: &str = concat!(
    "0707070707070707070707070707070707070707070707070707070707070707",
    "c93582f48a70622db0c2933cf7eaccd240da3dd0672f8f653a9400415060ad19",
);
const FEE_ASSESSMENT_SET_PREIMAGE_HEX: &str = concat!(
    "70726f6772616d6d61626c652f707269766174652d6566666563742d63617061",
    "62696c69746965732f323032362d30382d323815006665652d6173736573736d",
    "656e742d7365742d763002000400000001000000400000000707070707070707",
    "070707070707070707070707070707070707070707070707c93582f48a70622d",
    "b0c2933cf7eaccd240da3dd0672f8f653a9400415060ad19",
);
const FEE_ASSESSMENT_SET_DIGEST_HEX: &str =
    "56ec75a95f96d00d1b329f6675b5f9c266d7d871c07af99c4cac45afbc9828a9";
const CORE_VERIFIED_PREIMAGE_HEX: &str = concat!(
    "70726f6772616d6d61626c652f707269766174652d6566666563742d63617061",
    "62696c69746965732f323032362d30382d32381900636f72652d766572696669",
    "65642d65766964656e63652d76300c0020000000010101010101010101010101",
    "0101010101010101010101010101010101010101040000000000000020000000",
    "0202020202020202020202020202020202020202020202020202020202020202",
    "2000000003030303030303030303030303030303030303030303030303030303",
    "0303030320000000040404040404040404040404040404040404040404040404",
    "0404040404040404200000000505050505050505050505050505050505050505",
    "0505050505050505050505052000000006060606060606060606060606060606",
    "0606060606060606060606060606060620000000070707070707070707070707",
    "0707070707070707070707070707070707070707200000000808080808080808",
    "0808080808080808080808080808080808080808080808082000000009090909",
    "0909090909090909090909090909090909090909090909090909090920000000",
    "0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a",
    "200000000b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b",
    "0b0b0b0b",
);
const CORE_VERIFIED_DIGEST_HEX: &str =
    "9e6094accf84f3f1a1213a58d6e9ab025183572041ea1bc21cea5958bf936708";
const ENGINE_ATTESTED_PREIMAGE_HEX: &str = concat!(
    "70726f6772616d6d61626c652f707269766174652d6566666563742d63617061",
    "62696c69746965732f323032362d30382d32381b00656e67696e652d61747465",
    "737465642d65766964656e63652d763005002000000001010101010101010101",
    "0101010101010101010101010101010101010101010120000000020202020202",
    "0202020202020202020202020202020202020202020202020202200000000303",
    "0303030303030303030303030303030303030303030303030303030303032000",
    "0000040404040404040404040404040404040404040404040404040404040404",
    "0404200000000505050505050505050505050505050505050505050505050505",
    "050505050505",
);
const ENGINE_ATTESTED_DIGEST_HEX: &str =
    "ff8eb8598a494a2051e2423a9cf5bb1e55a5b017d15724dfd5d586b38f75331e";

fn assert_sha256_vector(preimage_hex: &str, digest_hex: &str) {
    let preimage = hex_bytes(preimage_hex);
    let expected_digest = fixed::<32>(digest_hex);
    assert_eq!(
        solana_sha256_hasher::hash(&preimage).to_bytes(),
        expected_digest
    );

    let mut mutation = preimage;
    let last = mutation.len() - 1;
    mutation[last] ^= 1;
    assert_ne!(
        solana_sha256_hasher::hash(&mutation).to_bytes(),
        expected_digest
    );

    mutation = hex_bytes(preimage_hex);
    mutation.push(0);
    assert_ne!(
        solana_sha256_hasher::hash(&mutation).to_bytes(),
        expected_digest
    );
}

#[test]
fn protected_execution_preimage_matches_frozen_vector() {
    let expected_digest = fixed::<32>(PROTECTED_EXECUTION_DIGEST_HEX);
    assert_sha256_vector(
        PROTECTED_EXECUTION_PREIMAGE_HEX,
        PROTECTED_EXECUTION_DIGEST_HEX,
    );
    let inputs = ProtectedExecutionRootInputs {
        core_program: &[1; 32],
        market_binding_digest: &[2; 32],
        engine_loader_state_snapshot_digest: &[3; 32],
        domain_set_digest: &[4; 32],
        intent_set_digest: &[5; 32],
        fee_policy_digest: &[6; 32],
        asset_set_digest: &[7; 32],
        authorization_view_set_digest: &[8; 32],
        fee_shard_set_digest: &[9; 32],
        protected_capability_set_digest: &[10; 32],
    };
    assert_eq!(
        compute_protected_execution_root(inputs).unwrap(),
        expected_digest
    );

    let mut changed = [10; 32];
    changed[0] ^= 1;
    assert_ne!(
        compute_protected_execution_root(ProtectedExecutionRootInputs {
            protected_capability_set_digest: &changed,
            ..inputs
        })
        .unwrap(),
        expected_digest
    );
}

#[test]
fn fee_funding_state_flag_changes_the_protected_execution_root() {
    assert_eq!(
        INTENT_CAPABILITY_TERM_FLAG_FEE_FUNDING,
        AUTHORIZATION_CAPABILITY_STATE_FLAG_FEE_FUNDING
    );
    let unflagged = AuthorizationCapabilityStateRowCandidateV0 {
        local_term_index: 0,
        reserved_0: 0,
        flags: 0,
        initial_maximum_engine_debit: 1_000,
        initial_minimum_credit: 0,
        initial_maximum_total_debit: 1_100,
        remaining_total_debit: 1_100,
        cumulative_engine_debit: 0,
        cumulative_fee_debit: 0,
        cumulative_credit: 0,
    };
    let fee_funding = AuthorizationCapabilityStateRowCandidateV0 {
        flags: AUTHORIZATION_CAPABILITY_STATE_FLAG_FEE_FUNDING,
        ..unflagged
    };
    let unflagged_row = unflagged.encode().unwrap();
    let fee_funding_row = fee_funding.encode().unwrap();
    assert_eq!(unflagged_row[2], 0);
    assert_eq!(
        fee_funding_row[2],
        AUTHORIZATION_CAPABILITY_STATE_FLAG_FEE_FUNDING
    );
    assert_eq!(unflagged_row[..2], fee_funding_row[..2]);
    assert_eq!(unflagged_row[3..], fee_funding_row[3..]);

    let unflagged_capability_root =
        compute_authorization_capability_state_root(&[unflagged]).unwrap();
    let fee_funding_capability_root =
        compute_authorization_capability_state_root(&[fee_funding]).unwrap();
    assert_ne!(unflagged_capability_root, fee_funding_capability_root);

    let intent_digest = [0x51; 32];
    let fee_state_root = compute_authorization_fee_state_root(&[]).unwrap();
    let state_digest = |capability_state_root: &[u8; 32]| {
        compute_authorization_state_digest(AuthorizationStateDigestInputs {
            intent_digest: &intent_digest,
            lifecycle: AUTHORIZATION_LIFECYCLE_ACTIVE,
            fill_sequence: 0,
            successful_fills: 0,
            remaining_fills: 1,
            capability_state_root,
            fee_state_root: &fee_state_root,
            stored_authorization_key_or_zero: &[0; 32],
        })
        .unwrap()
    };
    let unflagged_state_digest = state_digest(&unflagged_capability_root);
    let fee_funding_state_digest = state_digest(&fee_funding_capability_root);
    assert_ne!(unflagged_state_digest, fee_funding_state_digest);

    let view_root = |authorization_state_digest| {
        compute_authorization_view_set_digest(&[AuthorizationViewRowCandidateV0 {
            authorization_slot: 0,
            intent_digest,
            authorization_state_digest,
        }])
        .unwrap()
    };
    let unflagged_view_root = view_root(unflagged_state_digest);
    let fee_funding_view_root = view_root(fee_funding_state_digest);
    assert_ne!(unflagged_view_root, fee_funding_view_root);

    let execution_root = |authorization_view_set_digest: &[u8; 32]| {
        compute_protected_execution_root(ProtectedExecutionRootInputs {
            core_program: &[1; 32],
            market_binding_digest: &[2; 32],
            engine_loader_state_snapshot_digest: &[3; 32],
            domain_set_digest: &[4; 32],
            intent_set_digest: &[5; 32],
            fee_policy_digest: &[6; 32],
            asset_set_digest: &[7; 32],
            authorization_view_set_digest,
            fee_shard_set_digest: &[9; 32],
            protected_capability_set_digest: &[10; 32],
        })
        .unwrap()
    };
    assert_ne!(
        execution_root(&unflagged_view_root),
        execution_root(&fee_funding_view_root)
    );
}

#[test]
fn fee_assessment_preimage_matches_frozen_vector() {
    let expected_digest = fixed::<32>(FEE_ASSESSMENT_DIGEST_HEX);
    assert_sha256_vector(FEE_ASSESSMENT_PREIMAGE_HEX, FEE_ASSESSMENT_DIGEST_HEX);
    let inputs = FeeAssessmentDigestInputs {
        core_program: &[1; 32],
        market_binding_digest: &[2; 32],
        fee_policy_digest: &[3; 32],
        fee_policy_revision: 7,
        intent_set_digest: &[4; 32],
        protected_execution_root: &[5; 32],
        effect_digest: &[6; 32],
        rounding_group_digest: &[7; 32],
        fee_collection_digest: &[8; 32],
        fill_sequence: 9,
        cumulative_before: 10,
        fill_basis: 11,
        cumulative_after: 21,
        fee_delta: 2,
    };
    assert_eq!(
        compute_fee_assessment_digest(inputs).unwrap(),
        expected_digest
    );

    assert_ne!(
        compute_fee_assessment_digest(FeeAssessmentDigestInputs {
            fee_delta: 3,
            ..inputs
        })
        .unwrap(),
        expected_digest
    );
}

#[test]
fn fee_assessment_set_preimage_matches_frozen_vector() {
    let expected_assessment = fixed::<32>(FEE_ASSESSMENT_DIGEST_HEX);
    let row = FeeAssessmentSetRowCandidateV0 {
        assessment_group_digest: [7; 32],
        assessment_digest: expected_assessment,
    };
    let expected_row = fixed::<FEE_ASSESSMENT_SET_ROW_LEN>(FEE_ASSESSMENT_SET_ROW_HEX);
    assert_eq!(row.encode(), expected_row);
    assert_eq!(
        FeeAssessmentSetRowCandidateV0::decode_exact(&expected_row),
        Ok(row)
    );
    let mut trailing_row = expected_row.to_vec();
    trailing_row.push(0);
    assert!(FeeAssessmentSetRowCandidateV0::decode_exact(&trailing_row).is_err());
    let expected_digest = fixed::<32>(FEE_ASSESSMENT_SET_DIGEST_HEX);
    assert_sha256_vector(
        FEE_ASSESSMENT_SET_PREIMAGE_HEX,
        FEE_ASSESSMENT_SET_DIGEST_HEX,
    );
    assert_eq!(
        compute_fee_assessment_set_root(&[row]).unwrap(),
        expected_digest
    );

    let changed = FeeAssessmentSetRowCandidateV0 {
        assessment_digest: [0xcc; 32],
        ..row
    };
    assert_ne!(
        compute_fee_assessment_set_root(&[changed]).unwrap(),
        expected_digest
    );
}

#[test]
fn evidence_preimages_match_frozen_vectors() {
    let expected_core = fixed::<32>(CORE_VERIFIED_DIGEST_HEX);
    assert_sha256_vector(CORE_VERIFIED_PREIMAGE_HEX, CORE_VERIFIED_DIGEST_HEX);
    let core_inputs = CoreVerifiedEvidenceDigestInputs {
        core_program: &[1; 32],
        market_binding_digest: &[2; 32],
        loader_state_snapshot_digest: &[3; 32],
        intent_set_digest: &[4; 32],
        domain_set_digest: &[5; 32],
        protected_execution_root: &[6; 32],
        opaque_capability_root: &[7; 32],
        request_digest: &[8; 32],
        effect_digest: &[9; 32],
        fee_assessment_set_root: &[10; 32],
        observed_delta_root: &[11; 32],
    };
    assert_eq!(
        compute_core_verified_evidence_digest(core_inputs).unwrap(),
        expected_core
    );
    assert_ne!(
        compute_core_verified_evidence_digest(CoreVerifiedEvidenceDigestInputs {
            observed_delta_root: &[12; 32],
            ..core_inputs
        })
        .unwrap(),
        expected_core
    );

    let expected_engine = fixed::<32>(ENGINE_ATTESTED_DIGEST_HEX);
    assert_sha256_vector(ENGINE_ATTESTED_PREIMAGE_HEX, ENGINE_ATTESTED_DIGEST_HEX);
    let engine_inputs = EngineAttestedEvidenceDigestInputs {
        engine_program: &[1; 32],
        engine_interface_id: &[2; 32],
        engine_instance_id: &[3; 32],
        request_digest: &[4; 32],
        engine_supplied_digest: &[5; 32],
    };
    assert_eq!(
        compute_engine_attested_evidence_digest(engine_inputs).unwrap(),
        expected_engine
    );
    assert_ne!(
        compute_engine_attested_evidence_digest(EngineAttestedEvidenceDigestInputs {
            engine_supplied_digest: &[6; 32],
            ..engine_inputs
        })
        .unwrap(),
        expected_engine
    );
}

fn direct_envelope() -> ExecuteEnvelopeCandidateV0 {
    let payload = vec![0xaa, 0xbb];
    ExecuteEnvelopeCandidateV0 {
        header: ExecuteEnvelopeHeaderCandidateV0 {
            wire_version: WIRE_VERSION,
            loader_policy_account_count: 0,
            domain_control_account_count: 0,
            authorization_account_count: 1,
            protected_profile_account_count: 0,
            fee_control_account_count: 0,
            settlement_capability_count: 2,
            opaque_capability_count: 0,
            domain_count: 0,
            intent_count: 1,
            inline_intent_row_count: 1,
            asset_count: 1,
            fee_shard_count: 0,
            authorization_snapshot_row_count: 1,
            maximum_engine_moves: 2,
            flags: 0,
            payload_len: payload.len() as u16,
            expires_at_slot_exclusive: 100,
            expected_engine_sequence: 7,
            intent_set_digest: [1; 32],
            domain_set_digest: [2; 32],
            protected_execution_root: [3; 32],
            expected_opaque_capability_root: [4; 32],
            fee_policy_digest: [5; 32],
            expected_engine_loader_state_snapshot_digest: [6; 32],
            payload_digest: compute_payload_digest(&payload).unwrap(),
        },
        domain_controls: vec![],
        authorization_snapshots: vec![AuthorizationSnapshotRowCandidateV0 {
            authorization_slot: 0,
            witness_kind: WITNESS_DIRECT_ACTOR,
            authorization_control_offset_or_none: 0,
            inline_identity_index_or_none: 0,
            expected_fill_sequence: 0,
        }],
        inline_intent_identities: vec![InlineIntentIdentityRowCandidateV0 {
            actor: [7; 32],
            engine_terms_commitment: [8; 32],
            authorization_nonce: 9,
            expires_at_slot_exclusive: 100,
        }],
        fee_shards: vec![],
        settlement_capabilities: vec![
            SettlementCapabilityRowCandidateV0 {
                asset_index: 0,
                domain_index_or_none: NONE_INDEX,
                authorization_slot_or_none: 0,
                intent_local_term_index_or_none: 0,
                authority_class: AUTHORITY_INTENT_FUNDED,
                fee_shard_index_or_none: NONE_INDEX,
                fee_class: FEE_CLASS_GROSS_DEBIT_RATE,
                flags: 0,
                rights_bits: RIGHT_DEBIT,
                domain_accounting_slot_or_none: NONE_INDEX,
                spend_authority_control_offset_or_none: NONE_INDEX,
                reserved_0: 0,
                maximum_engine_debit: 10,
                maximum_total_debit: 10,
                minimum_credit: 0,
                maximum_protocol_fee: 0,
            },
            SettlementCapabilityRowCandidateV0 {
                asset_index: 0,
                domain_index_or_none: NONE_INDEX,
                authorization_slot_or_none: 0,
                intent_local_term_index_or_none: 1,
                authority_class: AUTHORITY_EXACT_EXTERNAL_CREDIT,
                fee_shard_index_or_none: NONE_INDEX,
                fee_class: FEE_CLASS_NONE,
                flags: 0,
                rights_bits: RIGHT_EXACT_EXTERNAL_RECIPIENT | RIGHT_CREDIT,
                domain_accounting_slot_or_none: NONE_INDEX,
                spend_authority_control_offset_or_none: NONE_INDEX,
                reserved_0: 0,
                maximum_engine_debit: 0,
                maximum_total_debit: 0,
                minimum_credit: 9,
                maximum_protocol_fee: 0,
            },
        ],
        payload,
    }
}

fn domain_envelope(rights_bits: u16) -> ExecuteEnvelopeCandidateV0 {
    let mut envelope = direct_envelope();
    envelope.header.domain_control_account_count = 2;
    envelope.header.domain_count = 1;
    envelope.header.settlement_capability_count = 3;
    envelope.domain_controls = vec![DomainControlRowCandidateV0 {
        descriptor_control_offset: 0,
        admission_control_offset_or_none: NONE_INDEX,
        accounting_control_offset: 1,
        flags: 0,
    }];
    let is_debit = rights_bits == RIGHT_DOMAIN_ACCOUNTED | RIGHT_DEBIT;
    envelope
        .settlement_capabilities
        .push(SettlementCapabilityRowCandidateV0 {
            asset_index: 0,
            domain_index_or_none: 0,
            authorization_slot_or_none: NONE_INDEX,
            intent_local_term_index_or_none: NONE_INDEX,
            authority_class: AUTHORITY_DOMAIN_ACCOUNTED,
            fee_shard_index_or_none: NONE_INDEX,
            fee_class: FEE_CLASS_NONE,
            flags: 0,
            rights_bits,
            domain_accounting_slot_or_none: 0,
            spend_authority_control_offset_or_none: NONE_INDEX,
            reserved_0: 0,
            maximum_engine_debit: if is_debit { 10 } else { 0 },
            maximum_total_debit: if is_debit { 10 } else { 0 },
            minimum_credit: 0,
            maximum_protocol_fee: 0,
        });
    envelope
}

fn reserved_fee_envelope() -> ExecuteEnvelopeCandidateV0 {
    let mut envelope = direct_envelope();
    envelope.header.fee_control_account_count = 2;
    envelope.header.fee_shard_count = 1;
    envelope.header.settlement_capability_count = 3;
    envelope.fee_shards = vec![FeeShardRowCandidateV0 {
        descriptor_control_offset: 0,
        liability_control_offset: 1,
        vault_settlement_capability_index: 2,
        asset_index: 0,
        flags: 0,
    }];
    envelope
        .settlement_capabilities
        .push(SettlementCapabilityRowCandidateV0 {
            asset_index: 0,
            domain_index_or_none: NONE_INDEX,
            authorization_slot_or_none: NONE_INDEX,
            intent_local_term_index_or_none: NONE_INDEX,
            authority_class: AUTHORITY_CORE_RESERVED_FEE,
            fee_shard_index_or_none: 0,
            fee_class: FEE_CLASS_NONE,
            flags: 0,
            rights_bits: RIGHT_CORE_RESERVED_FEE | RIGHT_CREDIT,
            domain_accounting_slot_or_none: NONE_INDEX,
            spend_authority_control_offset_or_none: NONE_INDEX,
            reserved_0: 0,
            maximum_engine_debit: 0,
            maximum_total_debit: 0,
            minimum_credit: 0,
            maximum_protocol_fee: 0,
        });
    envelope
}

fn protected_domain(rights_bits: u16) -> ProtectedCapabilityDigestRowCandidateV0 {
    let is_debit = rights_bits == RIGHT_DOMAIN_ACCOUNTED | RIGHT_DEBIT;
    ProtectedCapabilityDigestRowCandidateV0 {
        capability_position: 0,
        asset_index: 0,
        domain_index_or_none: 0,
        authorization_slot_or_none: NONE_INDEX,
        authority_class: AUTHORITY_DOMAIN_ACCOUNTED,
        fee_class: FEE_CLASS_NONE,
        fee_shard_index_or_none: NONE_INDEX,
        flags: 0,
        rights_bits,
        domain_accounting_slot_or_none: 0,
        spend_control_offset_or_none: NONE_INDEX,
        endpoint_executable: false,
        effective_signer: false,
        effective_writable: true,
        endpoint_key: [0x20; 32],
        endpoint_owner: [0x21; 32],
        transfer_authority_key_or_zero: if is_debit { [0x22; 32] } else { [0; 32] },
        asset_identity: [0x23; 32],
        asset_program: [0x24; 32],
        settlement_profile_digest: [0x25; 32],
        domain_descriptor_or_zero: [0x26; 32],
        domain_admission_digest_or_zero: [0x27; 32],
        lifecycle_digest: [0x28; 32],
        domain_revision: 7,
        maximum_engine_debit: if is_debit { 10 } else { 0 },
        maximum_total_debit: if is_debit { 10 } else { 0 },
        minimum_credit: 0,
        maximum_protocol_fee: 0,
        fee_policy_revision: 0,
        accounted_before_or_zero: 100,
    }
}

fn protected_external_credit() -> ProtectedCapabilityDigestRowCandidateV0 {
    let mut row = protected_intent_debit();
    row.authority_class = AUTHORITY_EXACT_EXTERNAL_CREDIT;
    row.fee_class = FEE_CLASS_NONE;
    row.fee_shard_index_or_none = NONE_INDEX;
    row.flags = 0;
    row.rights_bits = RIGHT_EXACT_EXTERNAL_RECIPIENT | RIGHT_CREDIT;
    row.spend_control_offset_or_none = NONE_INDEX;
    row.transfer_authority_key_or_zero = [0; 32];
    row.maximum_engine_debit = 0;
    row.maximum_total_debit = 0;
    row.minimum_credit = 9;
    row.maximum_protocol_fee = 0;
    row.fee_policy_revision = 0;
    row
}

fn protected_reserved_fee() -> ProtectedCapabilityDigestRowCandidateV0 {
    let mut row = protected_intent_debit();
    row.authorization_slot_or_none = NONE_INDEX;
    row.authority_class = AUTHORITY_CORE_RESERVED_FEE;
    row.fee_class = FEE_CLASS_NONE;
    row.flags = 0;
    row.rights_bits = RIGHT_CORE_RESERVED_FEE | RIGHT_CREDIT;
    row.spend_control_offset_or_none = NONE_INDEX;
    row.transfer_authority_key_or_zero = [0; 32];
    row.maximum_engine_debit = 0;
    row.maximum_total_debit = 0;
    row.minimum_credit = 0;
    row.maximum_protocol_fee = 0;
    row.fee_policy_revision = 0;
    row
}

#[test]
fn non_intent_roles_reject_every_recognized_flag_bit() {
    let mut envelopes = [
        domain_envelope(RIGHT_DOMAIN_ACCOUNTED | RIGHT_DEBIT),
        domain_envelope(RIGHT_DOMAIN_ACCOUNTED | RIGHT_CREDIT),
        direct_envelope(),
        reserved_fee_envelope(),
    ];
    let capability_indices = [2, 2, 1, 2];
    for (envelope, index) in envelopes.iter_mut().zip(capability_indices) {
        assert!(envelope.encode().is_ok());
        for flag in [
            SETTLEMENT_FLAG_FEE_FUNDING,
            SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT,
        ] {
            envelope.settlement_capabilities[index].flags = flag;
            assert!(envelope.encode().is_err());
            envelope.settlement_capabilities[index].flags = 0;
        }
    }

    let mut protected_rows = [
        protected_domain(RIGHT_DOMAIN_ACCOUNTED | RIGHT_DEBIT),
        protected_domain(RIGHT_DOMAIN_ACCOUNTED | RIGHT_CREDIT),
        protected_external_credit(),
        protected_reserved_fee(),
    ];
    for row in &mut protected_rows {
        assert!(row.encode().is_ok());
        for flag in [
            SETTLEMENT_FLAG_FEE_FUNDING,
            SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT,
        ] {
            row.flags = flag;
            assert!(row.encode().is_err());
            row.flags = 0;
        }
    }
}

#[test]
fn unconstrained_stored_debit_flag_rejects_direct_and_exact_delegate_witnesses() {
    let mut direct = direct_envelope();
    assert!(direct.encode().is_ok());
    direct.settlement_capabilities[0].flags = SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT;
    assert!(direct.encode().is_err());

    let mut exact_delegate = direct_envelope();
    exact_delegate.authorization_snapshots[0].witness_kind = WITNESS_EXACT_DELEGATE;
    exact_delegate.authorization_snapshots[0].authorization_control_offset_or_none = NONE_INDEX;
    exact_delegate.settlement_capabilities[0].spend_authority_control_offset_or_none = 0;
    assert!(exact_delegate.encode().is_ok());
    exact_delegate.settlement_capabilities[0].flags =
        SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT;
    assert!(exact_delegate.encode().is_err());
}

#[test]
fn domain_credit_rejects_unused_minimum_in_every_wire_projection() {
    let mut envelope = domain_envelope(RIGHT_DOMAIN_ACCOUNTED | RIGHT_CREDIT);
    assert!(envelope.encode().is_ok());
    envelope.settlement_capabilities[2].minimum_credit = 1;
    assert!(envelope.encode().is_err());

    let mut protected = protected_domain(RIGHT_DOMAIN_ACCOUNTED | RIGHT_CREDIT);
    assert!(protected.encode().is_ok());
    protected.minimum_credit = 1;
    assert!(protected.encode().is_err());

    let mut context = EngineContextRowCandidateV0 {
        settlement_capability_index: 2,
        asset_index: 0,
        domain_index_or_none: 0,
        authorization_slot_or_none: NONE_INDEX,
        rights_bits: RIGHT_DOMAIN_ACCOUNTED | RIGHT_CREDIT,
        fee_class: FEE_CLASS_NONE,
        context_flags: 0,
        endpoint_key: [0x31; 32],
        observed_before: 100,
        accounted_before_or_zero: 100,
        remaining_maximum_engine_debit: 0,
        remaining_maximum_total_debit: 0,
        remaining_minimum_credit: 0,
        remaining_maximum_protocol_fee: 0,
    };
    assert!(context.encode().is_ok());
    context.remaining_minimum_credit = 1;
    assert!(context.encode().is_err());
}

#[test]
fn every_authorization_snapshot_must_be_structurally_referenced() {
    let mut envelope = direct_envelope();
    envelope.header.authorization_account_count = 2;
    envelope.header.intent_count = 2;
    envelope.header.inline_intent_row_count = 2;
    envelope.header.authorization_snapshot_row_count = 2;
    envelope
        .authorization_snapshots
        .push(AuthorizationSnapshotRowCandidateV0 {
            authorization_slot: 1,
            witness_kind: WITNESS_DIRECT_ACTOR,
            authorization_control_offset_or_none: 1,
            inline_identity_index_or_none: 1,
            expected_fill_sequence: 0,
        });
    envelope
        .inline_intent_identities
        .push(InlineIntentIdentityRowCandidateV0 {
            actor: [0x41; 32],
            engine_terms_commitment: [0x42; 32],
            authorization_nonce: 10,
            expires_at_slot_exclusive: 100,
        });

    assert!(envelope
        .authorization_snapshots
        .iter()
        .all(|row| row.encode().is_ok()));
    assert!(envelope.encode().is_err());
}
