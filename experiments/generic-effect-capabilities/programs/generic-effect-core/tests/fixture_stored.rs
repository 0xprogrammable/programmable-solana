mod common;

use anchor_lang::solana_program::program_option::COption;
use common::{
    build_core_execute_instruction, cpi_program_paths, must_send_legacy, read_anchor_account,
    send_legacy_failure, snapshot_accounts, token_balance, token_state, CoreExecuteAccountClosure,
    DirectFixture, SbfArtifacts, DIRECT_DEFAULT_AMOUNT, DIRECT_FEE_RATE_DENOMINATOR,
    DIRECT_FEE_RATE_NUMERATOR, DIRECT_SOURCE_BALANCE,
};
use generic_effect_private_wire::{
    compute_asset_set_digest, compute_authorization_state_digest,
    compute_authorization_view_set_digest, compute_fee_shard_set_digest,
    compute_intent_capability_terms_root, compute_intent_core_terms_root,
    compute_intent_credit_constraints_root, compute_intent_digest, compute_intent_set_digest,
    compute_payload_digest, compute_protected_execution_root, AssetBindingRowCandidateV0,
    AuthorizationSnapshotRowCandidateV0, AuthorizationStateDigestInputs,
    AuthorizationViewRowCandidateV0, CoreControlInstructionCandidateV0,
    EngineContextRowCandidateV0, EngineIntentRowCandidateV0, FeeShardDigestRowCandidateV0,
    InitializeStoredAuthorizationArgsCandidateV0, IntentCapabilityTermRowCandidateV0,
    IntentCoreTermsDigestInputs, IntentDigestInputs, IntentSetRowCandidateV0,
    ProtectedExecutionRootInputs, StoredAuthorizationChunkCandidateV0,
    StoredAuthorizationChunkHeaderCandidateV0, StoredAuthorizationChunkRowsCandidateV0, NONE_INDEX,
    SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT, STORED_AUTHORIZATION_CHUNK_KIND_TERM,
    WIRE_VERSION, WITNESS_STORED_AUTHORIZATION,
};
use litesvm::LiteSVM;
use litesvm_cpi_tree::CpiTreeExt;
use litesvm_token::Approve;
use solana_message::{AccountMeta, Instruction};
use solana_signer::Signer;
use solana_transaction::{InstructionError, TransactionError};

use programmable_generic_effect_core::{
    account_segments::EffectivePrivilege,
    authorization::derive_exact_spend_authority,
    capabilities::{
        validate_settlement_capabilities, AssetProfileIdentity, CapabilityValidationContext,
        SettlementCapability,
    },
    constants::EXPERIMENTAL_MAJOR,
    error::CoreError,
    state::{
        FeeLiabilityLedgerCandidateV0, FeeShardDescriptorCandidateV0,
        StoredAuthorizationCandidateV0, StoredAuthorizationLifecycle,
    },
    token_settlement::ClassicSplEndpointSnapshot,
};

const FIRST_PARTIAL_FILL: u64 = 10_000;

#[test]
fn stored_partial_fill_rejects_stale_replay_then_commits_one_terminal_tombstone() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = StoredFixture::new(
        &artifacts,
        2,
        FIRST_PARTIAL_FILL,
        effect_engine_probe::plan::RECEIPT_ACCEPT,
    );
    let stale_fill = fixture.direct.instruction.clone();
    let delegated_total = fixture.maximum_total_debit();

    let (transaction, _) = fixture.direct.compile_v0();
    let metadata = fixture
        .direct
        .svm
        .send_transaction(transaction)
        .unwrap_or_else(|failure| {
            panic!(
                "first exact stored fill failed: {:?}\n{}\n{}",
                failure.err,
                failure.meta.pretty_logs(),
                failure.meta.pretty_cpi_tree(),
            )
        });
    assert!(cpi_program_paths(&metadata).contains(&vec![
        programmable_generic_effect_core::ID,
        effect_engine_probe::ID,
    ]));
    assert!(cpi_program_paths(&metadata).contains(&vec![
        programmable_generic_effect_core::ID,
        litesvm_token::TOKEN_ID,
    ]));

    let partial = fixture.state();
    assert_eq!(partial.lifecycle, StoredAuthorizationLifecycle::ACTIVE);
    assert_eq!(partial.fill_sequence, 1);
    assert_eq!(partial.pending_execution_digest, [0; 32]);
    assert_eq!(
        partial.capabilities[0].cumulative_engine_debit,
        u128::from(FIRST_PARTIAL_FILL)
    );
    assert_eq!(partial.capabilities[0].cumulative_fee_debit, 30);
    assert_eq!(partial.capabilities[1].cumulative_credit, 10_000);
    assert_eq!(partial.fee_state_count, 1);
    assert_eq!(partial.fee_states[0].cumulative_basis, 10_000);
    assert_eq!(partial.fee_states[0].cumulative_assessed_fee, 30);
    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.direct.source),
        DIRECT_SOURCE_BALANCE - FIRST_PARTIAL_FILL - 30
    );
    assert_eq!(
        token_state(&fixture.direct.svm, &fixture.direct.source).delegated_amount,
        delegated_total - FIRST_PARTIAL_FILL - 30
    );

    let before_stale_replay = fixture.rollback_snapshot();
    let (transaction, _) = fixture.direct.compile_custom_v0(stale_fill);
    let stale_failure = fixture
        .direct
        .svm
        .send_transaction(transaction)
        .expect_err("stale fill sequence unexpectedly replayed");
    assert_eq!(
        stale_failure.err,
        TransactionError::InstructionError(
            2,
            InstructionError::Custom(
                anchor_lang::error::ERROR_CODE_OFFSET
                    + CoreError::AuthorizationFillSequenceMismatch as u32,
            ),
        )
    );
    assert!(!program_invoked(
        &stale_failure.meta.logs,
        effect_engine_probe::ID
    ));
    assert_eq!(fixture.rollback_snapshot(), before_stale_replay);

    let terminal_fill = DIRECT_DEFAULT_AMOUNT - FIRST_PARTIAL_FILL;
    fixture.prepare_fill(terminal_fill, effect_engine_probe::plan::RECEIPT_ACCEPT);
    let terminal_instruction = fixture.direct.instruction.clone();
    let (transaction, _) = fixture.direct.compile_v0();
    fixture
        .direct
        .svm
        .send_transaction(transaction)
        .unwrap_or_else(|failure| {
            panic!(
                "terminal exact stored fill failed: {:?}\n{}\n{}",
                failure.err,
                failure.meta.pretty_logs(),
                failure.meta.pretty_cpi_tree(),
            )
        });

    let terminal = fixture.state();
    assert_eq!(terminal.lifecycle, StoredAuthorizationLifecycle::CONSUMED);
    assert_eq!(terminal.fill_sequence, 2);
    assert_eq!(terminal.pending_execution_digest, [0; 32]);
    assert_eq!(
        terminal.capabilities[0].cumulative_engine_debit,
        u128::from(DIRECT_DEFAULT_AMOUNT)
    );
    assert_eq!(
        terminal.capabilities[0].cumulative_fee_debit,
        u128::from(fixture.maximum_protocol_fee())
    );
    assert_eq!(terminal.capabilities[0].remaining_total_debit, 0);
    assert_eq!(
        terminal.capabilities[1].cumulative_credit,
        u128::from(DIRECT_DEFAULT_AMOUNT)
    );
    assert_eq!(terminal.fee_states[0].cumulative_basis, 37_000);
    assert_eq!(terminal.fee_states[0].cumulative_assessed_fee, 111);
    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.direct.source),
        DIRECT_SOURCE_BALANCE - delegated_total
    );
    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.direct.destination),
        DIRECT_DEFAULT_AMOUNT
    );
    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.direct.fee_vault),
        fixture.maximum_protocol_fee()
    );
    let source = token_state(&fixture.direct.svm, &fixture.direct.source);
    assert_eq!(source.delegate, COption::None);
    assert_eq!(source.delegated_amount, 0);

    let terminal_tombstone = fixture.rollback_snapshot();
    let (transaction, _) = fixture.direct.compile_custom_v0(terminal_instruction);
    let consumed_failure = fixture
        .direct
        .svm
        .send_transaction(transaction)
        .expect_err("consumed stored tombstone executed again");
    assert_eq!(
        consumed_failure.err,
        TransactionError::InstructionError(
            2,
            InstructionError::Custom(
                anchor_lang::error::ERROR_CODE_OFFSET + CoreError::AuthorizationUnavailable as u32,
            ),
        )
    );
    assert!(!program_invoked(
        &consumed_failure.meta.logs,
        effect_engine_probe::ID
    ));
    assert_eq!(fixture.rollback_snapshot(), terminal_tombstone);

    let cancel = fixture.cancel_instruction();
    send_legacy_failure(
        &mut fixture.direct.svm,
        &fixture.direct.payer,
        &[cancel],
        &[&fixture.direct.actor],
    );
    assert_eq!(fixture.rollback_snapshot(), terminal_tombstone);
}

#[test]
fn active_cancel_retains_exact_tombstone_and_blocks_execution_and_reinitialization() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = StoredFixture::new(
        &artifacts,
        3,
        FIRST_PARTIAL_FILL,
        effect_engine_probe::plan::RECEIPT_ACCEPT,
    );
    let prepared_execution = fixture.direct.instruction.clone();
    let authorization_before = fixture
        .direct
        .svm
        .get_account(&fixture.authorization)
        .expect("active stored authorization exists");
    let non_authorization_addresses = fixture.direct.rollback_state_addresses();
    let protected_before_cancel =
        snapshot_accounts(&fixture.direct.svm, &non_authorization_addresses);

    let cancel = fixture.cancel_instruction();
    must_send_legacy(
        &mut fixture.direct.svm,
        &fixture.direct.payer,
        &[cancel],
        &[&fixture.direct.actor],
        "cancel active exact stored authorization",
    );
    let cancelled = fixture.state();
    assert_eq!(cancelled.lifecycle, StoredAuthorizationLifecycle::CANCELLED);
    assert_eq!(cancelled.fill_sequence, 0);
    assert_eq!(cancelled.term_bitmap, 0b11);
    assert_eq!(cancelled.pending_execution_digest, [0; 32]);
    assert_eq!(
        snapshot_accounts(&fixture.direct.svm, &non_authorization_addresses),
        protected_before_cancel,
        "cancel may only advance the permanent authorization tombstone"
    );
    let authorization_after = fixture
        .direct
        .svm
        .get_account(&fixture.authorization)
        .expect("cancelled authorization tombstone remains allocated");
    assert_eq!(authorization_after.owner, authorization_before.owner);
    assert_eq!(authorization_after.lamports, authorization_before.lamports);
    assert_eq!(
        authorization_after.data.len(),
        authorization_before.data.len()
    );

    let cancelled_tombstone = fixture.rollback_snapshot();
    let (transaction, _) = fixture.direct.compile_custom_v0(prepared_execution);
    let execute_failure = fixture
        .direct
        .svm
        .send_transaction(transaction)
        .expect_err("cancelled authorization executed");
    assert_eq!(
        execute_failure.err,
        TransactionError::InstructionError(
            2,
            InstructionError::Custom(
                anchor_lang::error::ERROR_CODE_OFFSET + CoreError::AuthorizationUnavailable as u32,
            ),
        )
    );
    assert!(!program_invoked(
        &execute_failure.meta.logs,
        effect_engine_probe::ID
    ));
    assert_eq!(fixture.rollback_snapshot(), cancelled_tombstone);

    send_legacy_failure(
        &mut fixture.direct.svm,
        &fixture.direct.payer,
        std::slice::from_ref(&fixture.initialize_instruction),
        &[&fixture.direct.actor],
    );
    assert_eq!(
        fixture.rollback_snapshot(),
        cancelled_tombstone,
        "the final intent PDA cannot be closed and recreated after cancellation"
    );
}

#[test]
fn late_engine_failure_rolls_back_the_executing_lock_and_every_protected_write() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = StoredFixture::new(
        &artifacts,
        2,
        FIRST_PARTIAL_FILL,
        effect_engine_probe::plan::RECEIPT_LATE_FAILURE,
    );
    let before = fixture.rollback_snapshot();
    let source_before = token_state(&fixture.direct.svm, &fixture.direct.source);
    let (transaction, _) = fixture.direct.compile_v0();
    let failure = fixture
        .direct
        .svm
        .send_transaction(transaction)
        .expect_err("engine late failure unexpectedly committed a stored fill");

    assert!(program_invoked(&failure.meta.logs, effect_engine_probe::ID));
    assert!(!program_invoked(
        &failure.meta.logs,
        litesvm_token::TOKEN_ID
    ));
    assert_eq!(fixture.rollback_snapshot(), before);
    assert_eq!(
        token_state(&fixture.direct.svm, &fixture.direct.source),
        source_before
    );
    let rolled_back = fixture.state();
    assert_eq!(rolled_back.lifecycle, StoredAuthorizationLifecycle::ACTIVE);
    assert_eq!(rolled_back.fill_sequence, 0);
    assert_eq!(rolled_back.pending_execution_digest, [0; 32]);
    assert_eq!(rolled_back.fee_state_count, 0);
    assert_eq!(rolled_back.capabilities[0].cumulative_engine_debit, 0);
    assert_eq!(rolled_back.capabilities[0].cumulative_fee_debit, 0);
}

struct StoredFixture {
    direct: DirectFixture,
    authorization: anchor_lang::prelude::Pubkey,
    spend_authority: anchor_lang::prelude::Pubkey,
    initialize_instruction: Instruction,
}

impl StoredFixture {
    fn new(
        artifacts: &SbfArtifacts,
        maximum_successful_fills: u32,
        fill_amount: u64,
        receipt_mode: u8,
    ) -> Self {
        assert!(maximum_successful_fills > 1);
        let mut direct = DirectFixture::state_only(artifacts);
        let mut declarations = direct.envelope.settlement_capabilities.clone();
        declarations[0].flags |= SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT;
        declarations[0].spend_authority_control_offset_or_none = 1;

        let asset_binding = asset_binding(&direct);
        let asset_binding_digest = asset_binding.digest().expect("stored asset binding digest");
        let terms = declarations
            .iter()
            .enumerate()
            .filter(|(_, declaration)| declaration.authorization_slot_or_none == 0)
            .map(
                |(position, declaration)| IntentCapabilityTermRowCandidateV0 {
                    intent_local_term_index: declaration.intent_local_term_index_or_none,
                    authority_class: declaration.authority_class,
                    fee_class: declaration.fee_class,
                    flags: declaration.flags,
                    rights_bits: declaration.rights_bits,
                    endpoint_key: match position {
                        0 => direct.source.to_bytes(),
                        1 => direct.destination.to_bytes(),
                        _ => unreachable!("only source and destination are intent terms"),
                    },
                    asset_binding_digest,
                    required_domain_descriptor_digest_or_zero: [0; 32],
                    maximum_engine_debit: declaration.maximum_engine_debit,
                    maximum_total_debit: declaration.maximum_total_debit,
                    minimum_credit: declaration.minimum_credit,
                    maximum_protocol_fee: declaration.maximum_protocol_fee,
                },
            )
            .collect::<Vec<_>>();
        let capability_terms_root =
            compute_intent_capability_terms_root(&terms).expect("stored capability terms root");
        let credit_constraints_root =
            compute_intent_credit_constraints_root(&[]).expect("empty stored constraints root");
        let core_terms_root = compute_intent_core_terms_root(IntentCoreTermsDigestInputs {
            maximum_successful_fills,
            capability_terms_root: &capability_terms_root,
            credit_constraints_root: &credit_constraints_root,
        })
        .expect("stored Core terms root");
        let inline_identity = direct.envelope.inline_intent_identities[0];
        let core_program = programmable_generic_effect_core::ID.to_bytes();
        let intent_digest = compute_intent_digest(IntentDigestInputs {
            core_program: &core_program,
            market_binding_digest: &direct.engine_request.header.market_binding_digest,
            loader_state_snapshot_digest: &direct
                .engine_request
                .header
                .engine_loader_state_snapshot_digest,
            fee_policy_digest: &direct.engine_request.header.fee_policy_digest,
            identity: &inline_identity,
            core_terms_root: &core_terms_root,
        })
        .expect("stored intent digest");
        let args = InitializeStoredAuthorizationArgsCandidateV0 {
            wire_version: WIRE_VERSION,
            term_count: u8::try_from(terms.len()).expect("stored term count fits u8"),
            constraint_count: 0,
            flags: 0,
            maximum_successful_fills,
            identity: inline_identity,
            market_binding_digest: direct.engine_request.header.market_binding_digest,
            engine_loader_state_snapshot_digest: direct
                .engine_request
                .header
                .engine_loader_state_snapshot_digest,
            fee_policy_digest: direct.engine_request.header.fee_policy_digest,
            intent_capability_terms_root: capability_terms_root,
            credit_constraints_root,
            core_terms_root,
            intent_digest,
        };
        let authorization = StoredAuthorizationCandidateV0::address(
            &programmable_generic_effect_core::ID,
            &intent_digest,
        )
        .0;
        let initialize_instruction = Instruction {
            program_id: programmable_generic_effect_core::ID,
            accounts: vec![
                AccountMeta::new(direct.payer.pubkey(), true),
                AccountMeta::new_readonly(direct.actor.pubkey(), true),
                AccountMeta::new(authorization, false),
                AccountMeta::new_readonly(solana_sdk_ids::system_program::id(), false),
                AccountMeta::new_readonly(solana_sdk_ids::sysvar::instructions::id(), false),
            ],
            data: CoreControlInstructionCandidateV0::InitializeStoredAuthorization(args)
                .encode()
                .expect("encode stored initializer"),
        };
        must_send_legacy(
            &mut direct.svm,
            &direct.payer,
            std::slice::from_ref(&initialize_instruction),
            &[&direct.actor],
            "initialize exact stored authorization",
        );
        let write = stored_control_instruction(
            direct.actor.pubkey(),
            authorization,
            CoreControlInstructionCandidateV0::WriteStoredAuthorizationChunk(
                StoredAuthorizationChunkCandidateV0 {
                    header: StoredAuthorizationChunkHeaderCandidateV0 {
                        wire_version: WIRE_VERSION,
                        chunk_kind: STORED_AUTHORIZATION_CHUNK_KIND_TERM,
                        start_index: 0,
                        row_count: u8::try_from(terms.len()).expect("stored chunk size fits u8"),
                    },
                    rows: StoredAuthorizationChunkRowsCandidateV0::Terms(terms),
                },
            ),
        );
        must_send_legacy(
            &mut direct.svm,
            &direct.payer,
            &[write],
            &[&direct.actor],
            "write exact stored authorization terms",
        );
        let activate = stored_control_instruction(
            direct.actor.pubkey(),
            authorization,
            CoreControlInstructionCandidateV0::ActivateStoredAuthorization,
        );
        must_send_legacy(
            &mut direct.svm,
            &direct.payer,
            &[activate],
            &[&direct.actor],
            "activate exact stored authorization",
        );

        let (spend_authority, _) = derive_exact_spend_authority(
            &programmable_generic_effect_core::ID,
            &intent_digest,
            &direct.source,
        )
        .expect("derive stored spend authority");
        Approve::new(
            &mut direct.svm,
            &direct.payer,
            &spend_authority,
            &direct.source,
            declarations[0].maximum_total_debit,
        )
        .owner(&direct.actor)
        .send()
        .expect("approve multi-fill stored spend authority");

        direct.envelope.settlement_capabilities = declarations;
        direct.envelope.header.authorization_account_count = 2;
        direct.envelope.header.inline_intent_row_count = 0;
        direct.envelope.authorization_snapshots = vec![AuthorizationSnapshotRowCandidateV0 {
            authorization_slot: 0,
            witness_kind: WITNESS_STORED_AUTHORIZATION,
            authorization_control_offset_or_none: 0,
            inline_identity_index_or_none: NONE_INDEX,
            expected_fill_sequence: 0,
        }];
        direct.envelope.inline_intent_identities.clear();
        direct.spend_authority = Some(spend_authority);

        let mut fixture = Self {
            direct,
            authorization,
            spend_authority,
            initialize_instruction,
        };
        fixture.prepare_fill(fill_amount, receipt_mode);
        fixture
    }

    fn prepare_fill(&mut self, fill_amount: u64, receipt_mode: u8) {
        assert!(fill_amount != 0);
        let state = self.state();
        assert_eq!(state.lifecycle, StoredAuthorizationLifecycle::ACTIVE);
        let debit_state = state.capabilities[0];
        let remaining_engine_debit = u64::try_from(
            u128::from(debit_state.initial_maximum_engine_debit)
                - debit_state.cumulative_engine_debit,
        )
        .expect("remaining stored engine debit fits u64");
        assert!(fill_amount <= remaining_engine_debit);

        let payload = effect_engine_probe::plan::encode_explicit_plan(
            receipt_mode,
            0,
            NONE_INDEX,
            NONE_INDEX,
            &[effect_engine_probe::plan::PlannedMove {
                source_capability_index: 0,
                destination_capability_index: 1,
                amount: fill_amount,
            }],
        )
        .expect("encode exact stored engine plan");
        let payload_len = u16::try_from(payload.len()).expect("stored payload length fits u16");
        let asset_binding = asset_binding(&self.direct);
        let asset_binding_digest = asset_binding.digest().expect("stored asset binding digest");
        let asset_set_digest =
            compute_asset_set_digest(&[asset_binding]).expect("stored asset set digest");
        let endpoints = [
            endpoint_snapshot(&self.direct.svm, self.direct.source),
            endpoint_snapshot(&self.direct.svm, self.direct.destination),
            endpoint_snapshot(&self.direct.svm, self.direct.fee_vault),
        ];
        let declarations = &self.direct.envelope.settlement_capabilities;
        let asset = AssetProfileIdentity {
            asset_identity: self.direct.mint,
            asset_program: litesvm_token::TOKEN_ID,
            settlement_profile_digest: asset_binding.settlement_profile_digest,
        };
        let protected_capability_set_digest = validate_settlement_capabilities(
            &[
                SettlementCapability {
                    position: 0,
                    declaration: declarations[0],
                    core_program: programmable_generic_effect_core::ID,
                    experimental_major: EXPERIMENTAL_MAJOR,
                    market: self.direct.market,
                    endpoint: token_effective_privilege(self.direct.source),
                    transfer_authority_or_zero: self.spend_authority,
                    asset,
                    domain: None,
                    fee_policy_revision: self.direct.engine_request.fee_policy.revision,
                    lifecycle_digest: endpoints[0]
                        .lifecycle_digest()
                        .expect("stored source lifecycle digest"),
                    accounted_before_or_zero: 0,
                },
                SettlementCapability {
                    position: 1,
                    declaration: declarations[1],
                    core_program: programmable_generic_effect_core::ID,
                    experimental_major: EXPERIMENTAL_MAJOR,
                    market: self.direct.market,
                    endpoint: token_effective_privilege(self.direct.destination),
                    transfer_authority_or_zero: Default::default(),
                    asset,
                    domain: None,
                    fee_policy_revision: self.direct.engine_request.fee_policy.revision,
                    lifecycle_digest: endpoints[1]
                        .lifecycle_digest()
                        .expect("stored destination lifecycle digest"),
                    accounted_before_or_zero: 0,
                },
                SettlementCapability {
                    position: 2,
                    declaration: declarations[2],
                    core_program: programmable_generic_effect_core::ID,
                    experimental_major: EXPERIMENTAL_MAJOR,
                    market: self.direct.market,
                    endpoint: token_effective_privilege(self.direct.fee_vault),
                    transfer_authority_or_zero: Default::default(),
                    asset,
                    domain: None,
                    fee_policy_revision: self.direct.engine_request.fee_policy.revision,
                    lifecycle_digest: endpoints[2]
                        .lifecycle_digest()
                        .expect("stored fee-vault lifecycle digest"),
                    accounted_before_or_zero: 0,
                },
            ],
            CapabilityValidationContext {
                core_program: programmable_generic_effect_core::ID,
                market: self.direct.market,
                classic_token_program: litesvm_token::TOKEN_ID,
                experimental_major: EXPERIMENTAL_MAJOR,
                intent_count: 1,
                asset_count: 1,
                domain_count: 0,
                fee_shard_count: 1,
                fee_policy_revision: self.direct.engine_request.fee_policy.revision,
            },
        )
        .expect("validate stored protected capabilities");

        let remaining_fills = state
            .identity
            .max_fills
            .checked_sub(state.fill_sequence)
            .expect("stored fill sequence is bounded");
        let authorization_state_digest =
            compute_authorization_state_digest(AuthorizationStateDigestInputs {
                intent_digest: &state.identity.intent_digest,
                lifecycle: state.lifecycle,
                fill_sequence: state.fill_sequence,
                successful_fills: state.fill_sequence,
                remaining_fills,
                capability_state_root: &state
                    .capability_state_root()
                    .expect("stored capability state root"),
                fee_state_root: &state.fee_state_root().expect("stored fee state root"),
                stored_authorization_key_or_zero: &self.authorization.to_bytes(),
            })
            .expect("stored authorization state digest");
        let authorization_view_set_digest =
            compute_authorization_view_set_digest(&[AuthorizationViewRowCandidateV0 {
                authorization_slot: 0,
                intent_digest: state.identity.intent_digest,
                authorization_state_digest,
            }])
            .expect("stored authorization view set");
        let intent_set_digest = compute_intent_set_digest(
            &self.direct.engine_request.header.domain_set_digest,
            &[IntentSetRowCandidateV0 {
                intent_digest: state.identity.intent_digest,
            }],
        )
        .expect("stored intent set digest");
        let descriptor: FeeShardDescriptorCandidateV0 =
            read_anchor_account(&self.direct.svm, &self.direct.fee_shard_descriptor);
        let liability: FeeLiabilityLedgerCandidateV0 =
            read_anchor_account(&self.direct.svm, &self.direct.fee_liability);
        let fee_shard_set_digest = compute_fee_shard_set_digest(&[FeeShardDigestRowCandidateV0 {
            shard_index: 0,
            asset_index: 0,
            vault_settlement_capability_index: 2,
            flags: 0,
            descriptor_key: self.direct.fee_shard_descriptor.to_bytes(),
            descriptor_digest: descriptor.descriptor_digest,
            liability_key: self.direct.fee_liability.to_bytes(),
            vault_key: self.direct.fee_vault.to_bytes(),
            asset_binding_digest,
            fee_policy_digest: descriptor.fee_policy_digest,
            recipient_policy_digest: descriptor.recipient_policy_digest,
            fee_policy_revision: descriptor.fee_policy_revision,
            liability_before: liability.liability,
        }])
        .expect("stored fee shard set digest");
        let request_header = self.direct.engine_request.header;
        let protected_execution_root =
            compute_protected_execution_root(ProtectedExecutionRootInputs {
                core_program: &programmable_generic_effect_core::ID.to_bytes(),
                market_binding_digest: &request_header.market_binding_digest,
                engine_loader_state_snapshot_digest: &request_header
                    .engine_loader_state_snapshot_digest,
                domain_set_digest: &request_header.domain_set_digest,
                intent_set_digest: &intent_set_digest,
                fee_policy_digest: &request_header.fee_policy_digest,
                asset_set_digest: &asset_set_digest,
                authorization_view_set_digest: &authorization_view_set_digest,
                fee_shard_set_digest: &fee_shard_set_digest,
                protected_capability_set_digest: &protected_capability_set_digest,
            })
            .expect("stored protected execution root");

        self.direct.engine_request.header.payload_len = payload_len;
        self.direct.engine_request.header.intent_set_digest = intent_set_digest;
        self.direct.engine_request.header.protected_execution_root = protected_execution_root;
        self.direct.engine_request.intents = vec![EngineIntentRowCandidateV0 {
            authorization_slot: 0,
            identity: state.identity.inline_identity(),
            intent_digest: state.identity.intent_digest,
        }];
        self.direct.engine_request.contexts = vec![
            engine_context(0, declarations[0], endpoints[0], &state),
            engine_context(1, declarations[1], endpoints[1], &state),
        ];
        self.direct.engine_request.payload = payload.clone();
        self.direct
            .engine_request
            .validate()
            .expect("stored engine request remains canonical");
        let callback_authority = generic_effect_private_wire::derive_callback_authority_for_engine(
            &self.direct.engine_request,
            &effect_engine_probe::ID,
        )
        .expect("derive stored callback authority")
        .0;

        self.direct.envelope.header.payload_len = payload_len;
        self.direct.envelope.header.intent_set_digest = intent_set_digest;
        self.direct.envelope.header.protected_execution_root = protected_execution_root;
        self.direct.envelope.header.payload_digest =
            compute_payload_digest(&payload).expect("stored payload digest");
        self.direct.envelope.authorization_snapshots[0].expected_fill_sequence =
            state.fill_sequence;
        self.direct.envelope.payload = payload;
        self.direct.callback_authority = callback_authority;
        let closure = CoreExecuteAccountClosure {
            configuration: self.direct.configuration,
            market: self.direct.market,
            fee_policy: self.direct.fee_policy,
            engine_program: effect_engine_probe::ID,
            callback_authority,
            loader_policy: vec![self.direct.loader_policy_account],
            domain_controls: vec![],
            authorization_controls: vec![
                AccountMeta::new(self.authorization, false),
                AccountMeta::new_readonly(self.spend_authority, false),
            ],
            protected_profile: vec![litesvm_token::TOKEN_ID, self.direct.mint],
            fee_controls: vec![
                AccountMeta::new_readonly(self.direct.fee_shard_descriptor, false),
                AccountMeta::new(self.direct.fee_liability, false),
            ],
            settlement: vec![
                AccountMeta::new(self.direct.source, false),
                AccountMeta::new(self.direct.destination, false),
                AccountMeta::new(self.direct.fee_vault, false),
            ],
            opaque: vec![],
        };
        self.direct.instruction = build_core_execute_instruction(&self.direct.envelope, &closure)
            .expect("build canonical stored Core instruction");
        self.direct.transfer_amount = fill_amount;
        self.direct.protocol_fee = fee_delta(&state, fill_amount);
    }

    fn state(&self) -> StoredAuthorizationCandidateV0 {
        read_anchor_account(&self.direct.svm, &self.authorization)
    }

    fn maximum_protocol_fee(&self) -> u64 {
        self.direct.envelope.settlement_capabilities[0].maximum_protocol_fee
    }

    fn maximum_total_debit(&self) -> u64 {
        self.direct.envelope.settlement_capabilities[0].maximum_total_debit
    }

    fn rollback_addresses(&self) -> Vec<anchor_lang::prelude::Pubkey> {
        let mut addresses = self.direct.rollback_state_addresses().to_vec();
        addresses.push(self.authorization);
        addresses
    }

    fn rollback_snapshot(&self) -> Vec<common::AccountSnapshot> {
        snapshot_accounts(&self.direct.svm, &self.rollback_addresses())
    }

    fn cancel_instruction(&self) -> Instruction {
        stored_control_instruction(
            self.direct.actor.pubkey(),
            self.authorization,
            CoreControlInstructionCandidateV0::CancelStoredAuthorization,
        )
    }
}

fn asset_binding(direct: &DirectFixture) -> AssetBindingRowCandidateV0 {
    let row = direct.engine_request.assets[0];
    AssetBindingRowCandidateV0 {
        wire_version: WIRE_VERSION,
        flags: row.asset_flags,
        decimals: row.decimals,
        reserved: row.reserved,
        asset_identity: row.asset_identity,
        asset_program: row.asset_program,
        settlement_profile_digest: row.settlement_profile_digest,
    }
}

fn endpoint_snapshot(
    svm: &LiteSVM,
    key: anchor_lang::prelude::Pubkey,
) -> ClassicSplEndpointSnapshot {
    let state = token_state(svm, &key);
    ClassicSplEndpointSnapshot {
        key,
        mint: state.mint,
        authority: state.owner,
        amount: state.amount,
        delegate: coption(state.delegate),
        delegated_amount: state.delegated_amount,
        close_authority: coption(state.close_authority),
    }
}

fn coption<T>(value: COption<T>) -> Option<T> {
    match value {
        COption::Some(value) => Some(value),
        COption::None => None,
    }
}

fn token_effective_privilege(key: anchor_lang::prelude::Pubkey) -> EffectivePrivilege {
    EffectivePrivilege {
        key,
        owner: litesvm_token::TOKEN_ID,
        executable: false,
        signer: false,
        writable: true,
    }
}

fn engine_context(
    position: u8,
    declaration: generic_effect_private_wire::SettlementCapabilityRowCandidateV0,
    endpoint: ClassicSplEndpointSnapshot,
    state: &StoredAuthorizationCandidateV0,
) -> EngineContextRowCandidateV0 {
    let bound = state.capabilities[usize::from(declaration.intent_local_term_index_or_none)];
    let remaining_engine = u64::try_from(
        u128::from(bound.initial_maximum_engine_debit) - bound.cumulative_engine_debit,
    )
    .expect("remaining engine bound fits u64")
    .min(bound.remaining_total_debit);
    let remaining_credit = u64::try_from(
        u128::from(bound.initial_minimum_credit).saturating_sub(bound.cumulative_credit),
    )
    .expect("remaining credit bound fits u64");
    let remaining_fee =
        u64::try_from(u128::from(declaration.maximum_protocol_fee) - bound.cumulative_fee_debit)
            .expect("remaining fee bound fits u64")
            .min(bound.remaining_total_debit);
    EngineContextRowCandidateV0 {
        settlement_capability_index: position,
        asset_index: declaration.asset_index,
        domain_index_or_none: declaration.domain_index_or_none,
        authorization_slot_or_none: declaration.authorization_slot_or_none,
        rights_bits: declaration.rights_bits,
        fee_class: declaration.fee_class,
        context_flags: 0,
        endpoint_key: endpoint.key.to_bytes(),
        observed_before: endpoint.amount,
        accounted_before_or_zero: 0,
        remaining_maximum_engine_debit: remaining_engine,
        remaining_maximum_total_debit: bound.remaining_total_debit,
        remaining_minimum_credit: remaining_credit,
        remaining_maximum_protocol_fee: remaining_fee,
    }
}

fn fee_delta(state: &StoredAuthorizationCandidateV0, fill_amount: u64) -> u64 {
    let previous_basis = state.capabilities[0].cumulative_engine_debit;
    let previous_fee = state.capabilities[0].cumulative_fee_debit;
    let next_basis = previous_basis + u128::from(fill_amount);
    u64::try_from(
        next_basis * u128::from(DIRECT_FEE_RATE_NUMERATOR)
            / u128::from(DIRECT_FEE_RATE_DENOMINATOR)
            - previous_fee,
    )
    .expect("stored fee delta fits u64")
}

fn stored_control_instruction(
    actor: anchor_lang::prelude::Pubkey,
    authorization: anchor_lang::prelude::Pubkey,
    control: CoreControlInstructionCandidateV0,
) -> Instruction {
    Instruction {
        program_id: programmable_generic_effect_core::ID,
        accounts: vec![
            AccountMeta::new_readonly(actor, true),
            AccountMeta::new(authorization, false),
            AccountMeta::new_readonly(solana_sdk_ids::sysvar::instructions::id(), false),
        ],
        data: control
            .encode()
            .expect("encode exact stored authorization control"),
    }
}

fn program_invoked(logs: &[String], program_id: anchor_lang::prelude::Pubkey) -> bool {
    logs.iter()
        .any(|line| line.starts_with(&format!("Program {program_id} invoke")))
}
