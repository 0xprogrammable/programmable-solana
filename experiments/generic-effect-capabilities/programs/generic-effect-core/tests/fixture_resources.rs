mod common;

use anchor_lang::{prelude::Pubkey, InstructionData};
use common::{
    assert_controlled_resource_headroom, build_core_execute_instruction, compile_v0_transaction,
    contains_program_path, fixture_keypair, install_lookup_table, install_raw_account,
    lookup_candidates, measure_execution, request_heap_frame_instruction,
    set_compute_unit_limit_instruction, snapshot_accounts, token_balance,
    CoreExecuteAccountClosure, ExecutionResources, ResourceFixture, SbfArtifacts,
    V0MessageResources, COMPUTE_ACCEPTANCE_CEILING, CONTROLLED_COMPUTE_UNIT_LIMIT,
    CONTROLLED_HEAP_FRAME_BYTES, CPI_ACCOUNT_POSITION_ACCEPTANCE_CEILING,
    INSTRUCTION_DATA_ACCEPTANCE_CEILING, INSTRUCTION_TRACE_ACCEPTANCE_CEILING,
    LOADED_ACCOUNT_DATA_ACCEPTANCE_CEILING, PACKET_ACCEPTANCE_CEILING,
    RESOURCE_CORE_ACCOUNT_POSITIONS, RESOURCE_CORE_INSTRUCTION_BYTES,
    RESOURCE_ENGINE_REQUEST_BYTES, RESOURCE_FEE_A, RESOURCE_FEE_B,
    RESOURCE_FROZEN_MEASURED_PEAK_BUMP_BYTES, RESOURCE_HEAP_MEASUREMENT_ARTIFACT_SHA256,
    RESOURCE_MOVE_A, RESOURCE_MOVE_B, RETURN_DATA_ACCEPTANCE_CEILING,
    STACK_HEIGHT_ACCEPTANCE_CEILING, UNIQUE_LOCK_ACCEPTANCE_CEILING,
};
use generic_effect_private_wire::{
    compute_payload_digest, AuthorizationSnapshotRowCandidateV0, DomainControlRowCandidateV0,
    ExecuteEnvelopeCandidateV0, ExecuteEnvelopeHeaderCandidateV0, FeeShardRowCandidateV0,
    SettlementCapabilityRowCandidateV0, AUTHORITY_CORE_RESERVED_FEE,
    AUTHORITY_EXACT_EXTERNAL_CREDIT, AUTHORITY_INTENT_FUNDED, AUTHORIZATION_SNAPSHOT_ROW_LEN,
    CORE_EXECUTE_EFFECT_DISCRIMINATOR, DOMAIN_CONTROL_ROW_LEN, EXECUTE_ENVELOPE_HEADER_LEN,
    FEE_CLASS_GROSS_DEBIT_RATE, FEE_CLASS_NONE, FEE_SHARD_ROW_LEN, INLINE_INTENT_IDENTITY_ROW_LEN,
    MAX_ASSETS, MAX_AUTHORIZATION_ACCOUNTS, MAX_DOMAINS, MAX_DOMAIN_CONTROL_ACCOUNTS,
    MAX_ENGINE_MOVES, MAX_EXECUTE_ENVELOPE_LEN, MAX_FEE_CONTROL_ACCOUNTS, MAX_FEE_SHARDS,
    MAX_INLINE_INTENTS, MAX_INTENTS, MAX_LOADER_POLICY_ACCOUNTS, MAX_OPAQUE_CAPABILITIES,
    MAX_OPAQUE_PAYLOAD_LEN, MAX_PROTECTED_PROFILE_ACCOUNTS, MAX_SETTLEMENT_CAPABILITIES,
    NONE_INDEX, RIGHT_CORE_RESERVED_FEE, RIGHT_CREDIT, RIGHT_DEBIT, RIGHT_EXACT_EXTERNAL_RECIPIENT,
    SETTLEMENT_CAPABILITY_ROW_LEN, SETTLEMENT_FLAG_FEE_FUNDING, WIRE_VERSION,
    WITNESS_STORED_AUTHORIZATION,
};
use litesvm::LiteSVM;
use litesvm_cpi_tree::CpiTreeExt;
use solana_message::{AccountMeta, Instruction};
use solana_native_token::LAMPORTS_PER_SOL;
use solana_signer::Signer;

use programmable_generic_effect_core::state::StoredAuthorizationLifecycle;

const LEGACY_V0_PACKET_LIMIT: usize = 1_232;
const REDUCED_CORE_ACCOUNT_POSITIONS: usize = 6 + 1 + 6 + 4 + 3 + 4 + 6 + 4;
const REDUCED_CORE_INSTRUCTION_BYTES: usize = 8
    + EXECUTE_ENVELOPE_HEADER_LEN
    + 2 * DOMAIN_CONTROL_ROW_LEN
    + 2 * AUTHORIZATION_SNAPSHOT_ROW_LEN
    + 2 * FEE_SHARD_ROW_LEN
    + 6 * SETTLEMENT_CAPABILITY_ROW_LEN;
const _: () = assert!(REDUCED_CORE_ACCOUNT_POSITIONS == 34);
const _: () = assert!(REDUCED_CORE_INSTRUCTION_BYTES == 608);
const _: () = assert!(MAX_EXECUTE_ENVELOPE_LEN > LEGACY_V0_PACKET_LIMIT);

/// Freeze the exact predeclared AC13 wire shape independently of any product
/// semantics. The rows are canonical enough to pass the private Wire codec;
/// the account contents installed below deliberately are not protocol state.
fn impossible_six_move_envelope() -> ExecuteEnvelopeCandidateV0 {
    let payload = Vec::new();
    ExecuteEnvelopeCandidateV0 {
        header: ExecuteEnvelopeHeaderCandidateV0 {
            wire_version: WIRE_VERSION,
            loader_policy_account_count: 1,
            domain_control_account_count: 6,
            authorization_account_count: 4,
            protected_profile_account_count: 3,
            fee_control_account_count: 4,
            settlement_capability_count: 6,
            opaque_capability_count: 4,
            domain_count: 2,
            intent_count: 2,
            inline_intent_row_count: 0,
            asset_count: 2,
            fee_shard_count: 2,
            authorization_snapshot_row_count: 2,
            maximum_engine_moves: 6,
            flags: 0,
            payload_len: 0,
            expires_at_slot_exclusive: 0,
            expected_engine_sequence: 0,
            intent_set_digest: [0x11; 32],
            domain_set_digest: [0x12; 32],
            protected_execution_root: [0x13; 32],
            expected_opaque_capability_root: [0x14; 32],
            fee_policy_digest: [0x15; 32],
            expected_engine_loader_state_snapshot_digest: [0x16; 32],
            payload_digest: compute_payload_digest(&payload).expect("hash empty reduced payload"),
        },
        domain_controls: vec![
            DomainControlRowCandidateV0 {
                descriptor_control_offset: 0,
                admission_control_offset_or_none: 1,
                accounting_control_offset: 2,
                flags: 0,
            },
            DomainControlRowCandidateV0 {
                descriptor_control_offset: 3,
                admission_control_offset_or_none: 4,
                accounting_control_offset: 5,
                flags: 0,
            },
        ],
        authorization_snapshots: vec![
            AuthorizationSnapshotRowCandidateV0 {
                authorization_slot: 0,
                witness_kind: WITNESS_STORED_AUTHORIZATION,
                authorization_control_offset_or_none: 0,
                inline_identity_index_or_none: NONE_INDEX,
                expected_fill_sequence: 0,
            },
            AuthorizationSnapshotRowCandidateV0 {
                authorization_slot: 1,
                witness_kind: WITNESS_STORED_AUTHORIZATION,
                authorization_control_offset_or_none: 1,
                inline_identity_index_or_none: NONE_INDEX,
                expected_fill_sequence: 0,
            },
        ],
        inline_intent_identities: vec![],
        fee_shards: vec![
            FeeShardRowCandidateV0 {
                descriptor_control_offset: 0,
                liability_control_offset: 1,
                vault_settlement_capability_index: 4,
                asset_index: 0,
                flags: 0,
            },
            FeeShardRowCandidateV0 {
                descriptor_control_offset: 2,
                liability_control_offset: 3,
                vault_settlement_capability_index: 5,
                asset_index: 1,
                flags: 0,
            },
        ],
        settlement_capabilities: vec![
            intent_debit(0, 0, 0, 2),
            exact_credit(1, 0, 0),
            intent_debit(1, 1, 1, 3),
            exact_credit(0, 1, 1),
            fee_vault(0, 0),
            fee_vault(1, 1),
        ],
        payload,
    }
}

fn intent_debit(
    asset_index: u8,
    domain_index: u8,
    authorization_slot: u8,
    spend_authority_control_offset: u8,
) -> SettlementCapabilityRowCandidateV0 {
    SettlementCapabilityRowCandidateV0 {
        asset_index,
        domain_index_or_none: domain_index,
        authorization_slot_or_none: authorization_slot,
        intent_local_term_index_or_none: 0,
        authority_class: AUTHORITY_INTENT_FUNDED,
        fee_shard_index_or_none: asset_index,
        fee_class: FEE_CLASS_GROSS_DEBIT_RATE,
        flags: SETTLEMENT_FLAG_FEE_FUNDING,
        rights_bits: RIGHT_DEBIT,
        domain_accounting_slot_or_none: NONE_INDEX,
        spend_authority_control_offset_or_none: spend_authority_control_offset,
        reserved_0: 0,
        maximum_engine_debit: 10,
        maximum_total_debit: 11,
        minimum_credit: 0,
        maximum_protocol_fee: 1,
    }
}

fn exact_credit(
    asset_index: u8,
    domain_index: u8,
    authorization_slot: u8,
) -> SettlementCapabilityRowCandidateV0 {
    SettlementCapabilityRowCandidateV0 {
        asset_index,
        domain_index_or_none: domain_index,
        authorization_slot_or_none: authorization_slot,
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
    }
}

fn fee_vault(asset_index: u8, fee_shard_index: u8) -> SettlementCapabilityRowCandidateV0 {
    SettlementCapabilityRowCandidateV0 {
        asset_index,
        domain_index_or_none: NONE_INDEX,
        authorization_slot_or_none: NONE_INDEX,
        intent_local_term_index_or_none: NONE_INDEX,
        authority_class: AUTHORITY_CORE_RESERVED_FEE,
        fee_shard_index_or_none: fee_shard_index,
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
    }
}

fn next_key(tags: &mut impl Iterator<Item = u8>) -> Pubkey {
    fixture_keypair(tags.next().expect("enough deterministic fixture tags")).pubkey()
}

fn impossible_six_move_core_instruction() -> Instruction {
    let mut tags = 32_u8..96;
    let configuration = next_key(&mut tags);
    let market = next_key(&mut tags);
    let fee_policy = next_key(&mut tags);
    let callback_authority = next_key(&mut tags);
    let loader_policy = next_key(&mut tags);

    let domain_controls = vec![
        AccountMeta::new_readonly(next_key(&mut tags), false),
        AccountMeta::new_readonly(next_key(&mut tags), false),
        AccountMeta::new(next_key(&mut tags), false),
        AccountMeta::new_readonly(next_key(&mut tags), false),
        AccountMeta::new_readonly(next_key(&mut tags), false),
        AccountMeta::new(next_key(&mut tags), false),
    ];
    let authorization_controls = vec![
        AccountMeta::new(next_key(&mut tags), false),
        AccountMeta::new(next_key(&mut tags), false),
        AccountMeta::new_readonly(next_key(&mut tags), false),
        AccountMeta::new_readonly(next_key(&mut tags), false),
    ];
    let protected_profile = (0..3).map(|_| next_key(&mut tags)).collect();
    let fee_controls = vec![
        AccountMeta::new_readonly(next_key(&mut tags), false),
        AccountMeta::new(next_key(&mut tags), false),
        AccountMeta::new_readonly(next_key(&mut tags), false),
        AccountMeta::new(next_key(&mut tags), false),
    ];
    let settlement = (0..6)
        .map(|_| AccountMeta::new(next_key(&mut tags), false))
        .collect();
    let opaque = vec![
        AccountMeta::new_readonly(callback_capability_probe::ID, false),
        AccountMeta::new(next_key(&mut tags), false),
        AccountMeta::new_readonly(next_key(&mut tags), false),
        AccountMeta::new_readonly(next_key(&mut tags), false),
    ];

    let instruction = build_core_execute_instruction(
        &impossible_six_move_envelope(),
        &CoreExecuteAccountClosure {
            configuration,
            market,
            fee_policy,
            engine_program: effect_engine_probe::ID,
            callback_authority,
            loader_policy: vec![loader_policy],
            domain_controls,
            authorization_controls,
            protected_profile,
            fee_controls,
            settlement,
            opaque,
        },
    )
    .expect("encode the canonical reduced resource shape");
    assert_eq!(instruction.accounts.len(), REDUCED_CORE_ACCOUNT_POSITIONS);
    assert_eq!(instruction.data.len(), REDUCED_CORE_INSTRUCTION_BYTES);
    instruction
}

fn forward_once(core: &Instruction) -> Instruction {
    let core_account_count =
        u8::try_from(core.accounts.len()).expect("reduced Core position count fits u8");
    let mut accounts = Vec::with_capacity(1 + core.accounts.len());
    accounts.push(AccountMeta::new_readonly(
        programmable_generic_effect_core::ID,
        false,
    ));
    accounts.extend(core.accounts.iter().cloned());
    Instruction {
        program_id: hostile_router_probe::ID,
        accounts,
        data: hostile_router_probe::instruction::Route {
            args: hostile_router_probe::RouteProbeArgs {
                core_account_count,
                mode: hostile_router_probe::RouterMode::ForwardExactOnce,
                core_instruction_data: core.data.clone(),
            },
        }
        .data(),
    }
}

fn install_nonprogram_fixture_accounts(svm: &mut LiteSVM, core: &Instruction) {
    for (position, meta) in core.accounts.iter().enumerate() {
        if solana_sdk_ids::sysvar::instructions::check_id(&meta.pubkey)
            || svm.get_account(&meta.pubkey).is_some()
        {
            continue;
        }
        // Variable but deterministic lengths keep loaded-data accounting real
        // without pretending these system-owned bytes are valid protocol state.
        install_raw_account(
            svm,
            meta.pubkey,
            solana_sdk_ids::system_program::id(),
            vec![position as u8; (position % 4) * 16],
            false,
        );
    }
}

fn assert_prefix_failure_headroom(message: &V0MessageResources, execution: &ExecutionResources) {
    // This deliberately mirrors the private 20% falsification thresholds but
    // is not `assert_controlled_resource_headroom`: the measured execution is
    // an expected early Core failure, not the accepted six-move AC13 path.
    assert!(message.packet_bytes <= PACKET_ACCEPTANCE_CEILING);
    assert!(message.unique_locks <= UNIQUE_LOCK_ACCEPTANCE_CEILING);
    assert!(message.writable_locks <= message.unique_locks);
    assert!(message.loaded_account_data_bytes <= LOADED_ACCOUNT_DATA_ACCEPTANCE_CEILING);
    assert!(execution.compute_units <= COMPUTE_ACCEPTANCE_CEILING);
    assert!(execution.instruction_trace_len <= INSTRUCTION_TRACE_ACCEPTANCE_CEILING);
    assert!(execution.cpi_tree_frames <= INSTRUCTION_TRACE_ACCEPTANCE_CEILING);
    assert!(execution.maximum_stack_height <= STACK_HEIGHT_ACCEPTANCE_CEILING);
    assert!(execution.cpi_tree_depth <= usize::from(STACK_HEIGHT_ACCEPTANCE_CEILING));
    assert!(execution.maximum_cpi_account_positions <= CPI_ACCOUNT_POSITION_ACCEPTANCE_CEILING);
    assert!(execution.maximum_instruction_data_bytes <= INSTRUCTION_DATA_ACCEPTANCE_CEILING);
    assert!(execution.return_data_bytes <= RETURN_DATA_ACCEPTANCE_CEILING);
}

#[test]
fn frozen_608_byte_six_move_tuple_is_impossible_under_move_normal_form() {
    let envelope = impossible_six_move_envelope();
    assert_eq!(envelope.header.settlement_capability_count, 6);
    assert_eq!(envelope.header.fee_shard_count, 2);
    assert_eq!(envelope.header.asset_count, 2);
    assert_eq!(envelope.header.maximum_engine_moves, 6);

    let mut reserved = Vec::new();
    for (shard_index, shard) in envelope.fee_shards.iter().enumerate() {
        let capability_index = usize::from(shard.vault_settlement_capability_index);
        let capability = envelope
            .settlement_capabilities
            .get(capability_index)
            .expect("fee shard names an existing settlement capability");
        assert_eq!(capability.authority_class, AUTHORITY_CORE_RESERVED_FEE);
        assert_eq!(capability.fee_shard_index_or_none, shard_index as u8);
        assert_eq!(capability.asset_index, shard.asset_index);
        assert!(reserved.iter().all(|earlier| *earlier != capability_index));
        reserved.push(capability_index);
    }
    assert_eq!(
        reserved.len(),
        2,
        "two shards consume two distinct fee caps"
    );

    let move_eligible = envelope
        .settlement_capabilities
        .iter()
        .enumerate()
        .filter(|(index, _)| !reserved.contains(index))
        .map(|(_, capability)| capability)
        .collect::<Vec<_>>();
    assert_eq!(move_eligible.len(), 4);

    let per_asset = (0..envelope.header.asset_count)
        .map(|asset_index| {
            let rows = move_eligible
                .iter()
                .copied()
                .filter(|capability| capability.asset_index == asset_index)
                .collect::<Vec<_>>();
            let sources = rows
                .iter()
                .filter(|capability| capability.rights_bits == RIGHT_DEBIT)
                .count();
            let destinations = rows
                .iter()
                .filter(|capability| capability.rights_bits & RIGHT_CREDIT != 0)
                .count();
            assert!(sources != 0 && destinations != 0, "asset is actively moved");
            (sources, destinations)
        })
        .collect::<Vec<_>>();
    assert_eq!(per_asset, vec![(1, 1), (1, 1)]);

    // Move normal form makes each capability exclusively source or
    // destination and forbids duplicate (source,destination) pairs. For s
    // capabilities of one asset, the maximum is floor(s^2/4). With four
    // eligible capabilities split across two active assets, only 2+2 is
    // possible, so the global maximum is 1+1=2.
    let maximum_normal_form_moves = per_asset
        .iter()
        .map(|(sources, destinations)| sources * destinations)
        .sum::<usize>();
    assert_eq!(maximum_normal_form_moves, 2);
    assert!(usize::from(envelope.header.maximum_engine_moves) > maximum_normal_form_moves);
}

#[test]
fn corrected_636_byte_resource_graph_executes_routed_with_exact_state_and_measured_headroom() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = ResourceFixture::new(&artifacts);
    let core_positions = fixture.core_instruction().accounts.len();
    let core_data_bytes = fixture.core_instruction().data.len();

    assert_eq!(core_positions, RESOURCE_CORE_ACCOUNT_POSITIONS);
    assert_eq!(core_data_bytes, RESOURCE_CORE_INSTRUCTION_BYTES);
    assert_eq!(fixture.direct.envelope.header.maximum_engine_moves, 6);
    assert_eq!(fixture.direct.envelope.payload.len(), 28);
    assert_eq!(
        fixture
            .direct
            .engine_request
            .encode()
            .expect("encode resource request for its exact length")
            .len(),
        RESOURCE_ENGINE_REQUEST_BYTES,
    );
    assert_eq!(
        fixture.direct.envelope.settlement_capabilities[0].flags, SETTLEMENT_FLAG_FEE_FUNDING,
        "corrected stored debit must not use the unconstrained-debit escape",
    );
    assert_eq!(
        fixture.direct.envelope.settlement_capabilities[2].flags, SETTLEMENT_FLAG_FEE_FUNDING,
        "corrected stored debit must not use the unconstrained-debit escape",
    );

    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.sources[0]),
        1_000_000
    );
    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.sources[1]),
        1_000_000
    );
    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.recipients[0]),
        0
    );
    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.recipients[1]),
        0
    );
    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.fee_vaults[0]),
        0
    );
    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.fee_vaults[1]),
        0
    );
    assert_eq!(fixture.fee_liability_state(0).liability, 0);
    assert_eq!(fixture.fee_liability_state(1).liability, 0);
    for index in 0..2 {
        let authorization = fixture.authorization_state(index);
        assert_eq!(
            authorization.lifecycle,
            StoredAuthorizationLifecycle::ACTIVE
        );
        assert_eq!(authorization.fill_sequence, 0);
        assert_eq!(authorization.identity.max_fills, 1);
        assert_eq!(authorization.constraint_count, 1);
        assert_eq!(authorization.pending_execution_digest, [0; 32]);
    }
    let domain_before = snapshot_accounts(&fixture.direct.svm, &fixture.domain_accounting);

    let (transaction, message, instructions) = fixture.compile_routed_v0();
    eprintln!(
        "RESOURCE corrected-routed-preflight: core_positions={} core_data={} engine_request={} packet={} static={} alt_tables={} alt_writable={} alt_readonly={} locks={} writable={} loaded_data={}",
        core_positions,
        core_data_bytes,
        RESOURCE_ENGINE_REQUEST_BYTES,
        message.packet_bytes,
        message.static_keys.len(),
        message.lookup_table_keys.len(),
        message.loaded_writable_keys.len(),
        message.loaded_readonly_keys.len(),
        message.unique_locks,
        message.writable_locks,
        message.loaded_account_data_bytes,
    );
    assert_eq!(message.packet_bytes, 974);
    assert_eq!(message.static_keys.len(), 3);
    assert_eq!(message.lookup_table_keys.len(), 1);
    assert_eq!(message.loaded_writable_keys.len(), 15);
    assert_eq!(message.loaded_readonly_keys.len(), 20);
    assert_eq!(message.unique_locks, 38);
    assert_eq!(message.writable_locks, 16);

    let metadata = fixture
        .direct
        .svm
        .send_transaction(transaction)
        .unwrap_or_else(|failure| {
            panic!(
                "corrected routed resource execution failed: {:?}\n{}\n{}",
                failure.err,
                failure.meta.pretty_logs(),
                failure.meta.pretty_cpi_tree(),
            )
        });
    let execution = measure_execution(&metadata, &instructions);
    eprintln!(
        "RESOURCE corrected-routed-success: packet={} locks={} writable={} loaded_data={} compute={} trace={} frames={} stack={} tree_depth={} cpi_positions={} instruction_data={} return_data={} frozen_measured_heap_peak={} heap_frame={} measurement_artifact_sha256={}",
        message.packet_bytes,
        message.unique_locks,
        message.writable_locks,
        message.loaded_account_data_bytes,
        execution.compute_units,
        execution.instruction_trace_len,
        execution.cpi_tree_frames,
        execution.maximum_stack_height,
        execution.cpi_tree_depth,
        execution.maximum_cpi_account_positions,
        execution.maximum_instruction_data_bytes,
        execution.return_data_bytes,
        RESOURCE_FROZEN_MEASURED_PEAK_BUMP_BYTES,
        CONTROLLED_HEAP_FRAME_BYTES,
        RESOURCE_HEAP_MEASUREMENT_ARTIFACT_SHA256,
    );
    eprintln!("{}", metadata.pretty_cpi_tree());

    assert!(contains_program_path(
        &metadata,
        &[
            hostile_router_probe::ID,
            programmable_generic_effect_core::ID,
            effect_engine_probe::ID,
            callback_capability_probe::ID,
        ]
    ));
    assert_eq!(execution.instruction_trace_len, 10);
    assert_eq!(execution.cpi_tree_frames, 10);
    assert_eq!(execution.maximum_stack_height, 4);
    assert_eq!(execution.cpi_tree_depth, 4);
    assert_eq!(
        execution.maximum_cpi_account_positions,
        RESOURCE_CORE_ACCOUNT_POSITIONS,
    );
    assert_eq!(
        execution.maximum_instruction_data_bytes,
        RESOURCE_ENGINE_REQUEST_BYTES,
    );
    assert_eq!(execution.return_data_bytes, 0);

    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.sources[0]),
        1_000_000 - RESOURCE_MOVE_A - RESOURCE_FEE_A,
    );
    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.sources[1]),
        1_000_000 - RESOURCE_MOVE_B - RESOURCE_FEE_B,
    );
    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.recipients[0]),
        RESOURCE_MOVE_A,
    );
    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.recipients[1]),
        RESOURCE_MOVE_B,
    );
    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.fee_vaults[0]),
        RESOURCE_FEE_A,
    );
    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.fee_vaults[1]),
        RESOURCE_FEE_B,
    );
    assert_eq!(
        fixture.fee_liability_state(0).liability,
        u128::from(RESOURCE_FEE_A),
    );
    assert_eq!(
        fixture.fee_liability_state(1).liability,
        u128::from(RESOURCE_FEE_B),
    );
    assert_eq!(
        snapshot_accounts(&fixture.direct.svm, &fixture.domain_accounting),
        domain_before,
        "closed admission-only domain accounting must stay byte-identical",
    );

    let expected = [
        (RESOURCE_MOVE_A, RESOURCE_FEE_A, RESOURCE_MOVE_B),
        (RESOURCE_MOVE_B, RESOURCE_FEE_B, RESOURCE_MOVE_A),
    ];
    for (index, (debit, fee, credit)) in expected.into_iter().enumerate() {
        let authorization = fixture.authorization_state(index);
        assert_eq!(
            authorization.lifecycle,
            StoredAuthorizationLifecycle::CONSUMED
        );
        assert_eq!(authorization.fill_sequence, 1);
        assert_eq!(
            authorization.identity.max_fills - authorization.fill_sequence,
            0
        );
        assert_eq!(authorization.pending_execution_digest, [0; 32]);
        assert_eq!(authorization.fee_state_count, 1);
        assert_eq!(authorization.capabilities[0].remaining_total_debit, 0);
        assert_eq!(
            authorization.capabilities[0].cumulative_engine_debit,
            u128::from(debit)
        );
        assert_eq!(
            authorization.capabilities[0].cumulative_fee_debit,
            u128::from(fee)
        );
        assert_eq!(authorization.capabilities[0].cumulative_credit, 0);
        assert_eq!(authorization.capabilities[1].cumulative_engine_debit, 0);
        assert_eq!(authorization.capabilities[1].cumulative_fee_debit, 0);
        assert_eq!(
            authorization.capabilities[1].cumulative_credit,
            u128::from(credit)
        );
        assert_eq!(authorization.fee_states[0].funding_local_term_index, 0);
        assert_eq!(
            authorization.fee_states[0].fee_class,
            FEE_CLASS_GROSS_DEBIT_RATE
        );
        assert_eq!(
            authorization.fee_states[0].cumulative_basis,
            u128::from(debit)
        );
        assert_eq!(
            authorization.fee_states[0].cumulative_assessed_fee,
            u128::from(fee)
        );
        assert_eq!(authorization.fee_states[0].maximum_fee, fee);
    }

    for index in 0..2 {
        let state = fixture.engine_state(index);
        assert_eq!(state.sequence, 1);
        assert_eq!(state.accumulator, RESOURCE_MOVE_A + RESOURCE_MOVE_B);
        assert_eq!(state.last_request_digest, fixture.request_digest);
        assert_eq!(state.last_move_count, 2);
    }
    let helper = fixture.helper();
    assert_eq!(helper.allowed_callback, fixture.direct.callback_authority);
    assert_eq!(helper.calls, 1);
    assert_eq!(helper.value, RESOURCE_MOVE_A + RESOURCE_MOVE_B);
    assert_eq!(helper.descendant_receipt_sets, 0);

    assert!(
        u128::from(RESOURCE_FROZEN_MEASURED_PEAK_BUMP_BYTES) * 5
            <= u128::from(CONTROLLED_HEAP_FRAME_BYTES) * 4,
        "frozen measured heap peak must leave at least 20% of the authenticated frame free",
    );
    assert_controlled_resource_headroom("corrected-routed", &message, &execution);
}

#[test]
fn reduced_shape_uses_real_v0_alt_and_measures_the_routed_core_prefix_without_claiming_acceptance()
{
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut svm = LiteSVM::new();
    artifacts.install_cached_programs(&mut svm);
    let payer = fixture_keypair(31);
    svm.airdrop(&payer.pubkey(), 10 * LAMPORTS_PER_SOL)
        .expect("fund reduced resource payer");

    let core = impossible_six_move_core_instruction();
    install_nonprogram_fixture_accounts(&mut svm, &core);
    let routed = forward_once(&core);
    assert_eq!(routed.accounts.len(), 1 + REDUCED_CORE_ACCOUNT_POSITIONS);

    let compute_limit = set_compute_unit_limit_instruction(CONTROLLED_COMPUTE_UNIT_LIMIT);
    let heap = request_heap_frame_instruction(CONTROLLED_HEAP_FRAME_BYTES);
    let instructions = vec![compute_limit, heap, routed];
    let lookup = install_lookup_table(
        &mut svm,
        &payer,
        lookup_candidates(&instructions, payer.pubkey()),
    );
    let (transaction, message) = compile_v0_transaction(&svm, &payer, &instructions, &[lookup])
        .expect("compile and sign the reduced routed v0 transaction");

    assert_eq!(message.lookup_table_keys.len(), 1);
    assert!(!message.loaded_writable_keys.is_empty());
    assert!(!message.loaded_readonly_keys.is_empty());
    assert_eq!(
        message.unique_locks,
        message.resolved_unique_keys.len(),
        "resolved unique keys, not Core positions, are lock evidence"
    );
    assert!(message.packet_bytes <= LEGACY_V0_PACKET_LIMIT);

    // The accounts above intentionally have system-owned placeholder bytes.
    // Reaching Core proves the 34-position routed CPI shape, while the expected
    // decode failure prevents this prefix-only fixture from being cited as a
    // successful two-domain/two-intent/six-move AC13 execution.
    let failure = svm
        .send_transaction(transaction)
        .expect_err("placeholder protocol state unexpectedly executed");
    let execution = measure_execution(&failure.meta, &instructions);
    assert!(failure
        .meta
        .logs
        .iter()
        .any(|line| { line.starts_with(&format!("Program {} invoke", hostile_router_probe::ID)) }));
    assert!(failure.meta.logs.iter().any(|line| {
        line.starts_with(&format!(
            "Program {} invoke",
            programmable_generic_effect_core::ID
        ))
    }));
    assert!(!failure
        .meta
        .logs
        .iter()
        .any(|line| line.starts_with(&format!("Program {} invoke", effect_engine_probe::ID))));
    assert_eq!(
        execution.maximum_cpi_account_positions,
        REDUCED_CORE_ACCOUNT_POSITIONS
    );
    assert!(execution.maximum_instruction_data_bytes >= REDUCED_CORE_INSTRUCTION_BYTES);
    assert_prefix_failure_headroom(&message, &execution);

    eprintln!(
        "RESOURCE reduced-routed-prefix-EXPECTED-FAILURE: core_positions={} core_data={} packet={} static={} alt_tables={} alt_writable={} alt_readonly={} locks={} writable={} loaded_data={} compute={} trace={} frames={} stack={} tree_depth={} cpi_positions={} instruction_data={} return_data={}",
        core.accounts.len(),
        core.data.len(),
        message.packet_bytes,
        message.static_keys.len(),
        message.lookup_table_keys.len(),
        message.loaded_writable_keys.len(),
        message.loaded_readonly_keys.len(),
        message.unique_locks,
        message.writable_locks,
        message.loaded_account_data_bytes,
        execution.compute_units,
        execution.instruction_trace_len,
        execution.cpi_tree_frames,
        execution.maximum_stack_height,
        execution.cpi_tree_depth,
        execution.maximum_cpi_account_positions,
        execution.maximum_instruction_data_bytes,
        execution.return_data_bytes,
    );
    eprintln!(
        "RESOURCE reduced-routed-prefix static_keys={:?}",
        message.static_keys
    );
    eprintln!(
        "RESOURCE reduced-routed-prefix alt_writable_keys={:?}",
        message.loaded_writable_keys
    );
    eprintln!(
        "RESOURCE reduced-routed-prefix alt_readonly_keys={:?}",
        message.loaded_readonly_keys
    );
    eprintln!(
        "RESOURCE reduced-routed-prefix resolved_unique_keys={:?}",
        message.resolved_unique_keys
    );
    eprintln!(
        "RESOURCE reduced-routed-prefix expected_failure={:?}",
        failure.err
    );
    eprintln!("{}", failure.meta.pretty_cpi_tree());
}

#[test]
fn cartesian_1424_byte_wire_length_cannot_fit_even_a_minimal_top_level_v0_packet() {
    let cartesian_len = 8
        + EXECUTE_ENVELOPE_HEADER_LEN
        + MAX_DOMAINS * DOMAIN_CONTROL_ROW_LEN
        + MAX_INTENTS * AUTHORIZATION_SNAPSHOT_ROW_LEN
        + MAX_INLINE_INTENTS * INLINE_INTENT_IDENTITY_ROW_LEN
        + MAX_FEE_SHARDS * FEE_SHARD_ROW_LEN
        + MAX_SETTLEMENT_CAPABILITIES * SETTLEMENT_CAPABILITY_ROW_LEN
        + MAX_OPAQUE_PAYLOAD_LEN;
    assert_eq!(cartesian_len, MAX_EXECUTE_ENVELOPE_LEN);
    assert_eq!(cartesian_len, 1_424);

    let cartesian_payload = vec![0; MAX_OPAQUE_PAYLOAD_LEN];
    let header = ExecuteEnvelopeHeaderCandidateV0 {
        wire_version: WIRE_VERSION,
        loader_policy_account_count: MAX_LOADER_POLICY_ACCOUNTS as u8,
        domain_control_account_count: MAX_DOMAIN_CONTROL_ACCOUNTS as u8,
        authorization_account_count: MAX_AUTHORIZATION_ACCOUNTS as u8,
        protected_profile_account_count: MAX_PROTECTED_PROFILE_ACCOUNTS as u8,
        fee_control_account_count: MAX_FEE_CONTROL_ACCOUNTS as u8,
        settlement_capability_count: MAX_SETTLEMENT_CAPABILITIES as u8,
        opaque_capability_count: MAX_OPAQUE_CAPABILITIES as u8,
        domain_count: MAX_DOMAINS as u8,
        intent_count: MAX_INTENTS as u8,
        inline_intent_row_count: MAX_INLINE_INTENTS as u8,
        asset_count: MAX_ASSETS as u8,
        fee_shard_count: MAX_FEE_SHARDS as u8,
        authorization_snapshot_row_count: MAX_INTENTS as u8,
        maximum_engine_moves: MAX_ENGINE_MOVES as u8,
        flags: 0,
        payload_len: MAX_OPAQUE_PAYLOAD_LEN as u16,
        expires_at_slot_exclusive: 0,
        expected_engine_sequence: 0,
        intent_set_digest: [0x21; 32],
        domain_set_digest: [0x22; 32],
        protected_execution_root: [0x23; 32],
        expected_opaque_capability_root: [0x24; 32],
        fee_policy_digest: [0x25; 32],
        expected_engine_loader_state_snapshot_digest: [0x26; 32],
        payload_digest: compute_payload_digest(&cartesian_payload).expect("hash Cartesian payload"),
    };
    let mut bytes = Vec::with_capacity(cartesian_len);
    bytes.extend_from_slice(&CORE_EXECUTE_EFFECT_DISCRIMINATOR);
    bytes.extend_from_slice(&header.encode().expect("encode all-axis header"));
    bytes.resize(cartesian_len - cartesian_payload.len(), 0);
    bytes.extend_from_slice(&cartesian_payload);
    assert_eq!(bytes.len(), cartesian_len);

    // The body is only a packet-size witness, not a claim that the independent
    // maxima form one semantically valid envelope. Using zero account metas is
    // the strongest possible lower bound: every real Core closure only adds
    // message bytes, while the instruction data already exceeds 1,232 bytes.
    let instruction = Instruction {
        program_id: programmable_generic_effect_core::ID,
        accounts: vec![],
        data: bytes,
    };
    let mut svm = LiteSVM::new();
    let payer = fixture_keypair(30);
    svm.airdrop(&payer.pubkey(), LAMPORTS_PER_SOL)
        .expect("fund Cartesian packet witness payer");
    let (_, message) = compile_v0_transaction(&svm, &payer, &[instruction], &[])
        .expect("serialize minimal Cartesian v0 packet witness");

    assert!(message.packet_bytes > LEGACY_V0_PACKET_LIMIT);
    eprintln!(
        "RESOURCE cartesian-expected-packet-failure: core_data={} minimal_v0_packet={} packet_limit={}",
        MAX_EXECUTE_ENVELOPE_LEN, message.packet_bytes, LEGACY_V0_PACKET_LIMIT
    );
}
