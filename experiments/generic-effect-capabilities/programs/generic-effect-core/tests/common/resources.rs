use anchor_lang::{
    solana_program::program_option::COption, AccountSerialize, InstructionData, Space,
};
use effect_engine_probe::{
    plan::{encode_explicit_plan, PlannedMove, RECEIPT_ACCEPT},
    state::{EngineStateCandidateV0, ENGINE_STATE_LEN},
};
use generic_effect_private_wire::{
    compute_asset_set_digest, compute_authorization_state_digest,
    compute_authorization_view_set_digest, compute_domain_set_digest,
    compute_exact_fee_recipient_policy_digest, compute_fee_shard_set_digest,
    compute_intent_capability_terms_root, compute_intent_core_terms_root,
    compute_intent_credit_constraints_root, compute_intent_debit_group_root, compute_intent_digest,
    compute_intent_set_digest, compute_opaque_capability_root, compute_payload_digest,
    compute_protected_execution_root, derive_callback_authority_for_engine,
    AssetBindingRowCandidateV0, AuthorizationSnapshotRowCandidateV0,
    AuthorizationStateDigestInputs, AuthorizationViewRowCandidateV0,
    CoreControlInstructionCandidateV0, CreditConstraintRowCandidateV0, DomainAdmissionCandidateV0,
    DomainControlRowCandidateV0, DomainExecutionRowCandidateV0, EngineAssetRowCandidateV0,
    EngineContextRowCandidateV0, EngineDomainRowCandidateV0, EngineIntentRowCandidateV0,
    EngineRequestCandidateV0, EngineRequestHeaderCandidateV0, ExecuteEnvelopeCandidateV0,
    ExecuteEnvelopeHeaderCandidateV0, FeeShardDigestRowCandidateV0, FeeShardRowCandidateV0,
    InitializeStoredAuthorizationArgsCandidateV0, InlineIntentIdentityRowCandidateV0,
    IntentCapabilityTermRowCandidateV0, IntentCoreTermsDigestInputs, IntentDigestInputs,
    IntentSetRowCandidateV0, OpaqueCapabilityDescriptorCandidateV0, ProtectedExecutionRootInputs,
    SettlementCapabilityRowCandidateV0, StoredAuthorizationChunkCandidateV0,
    StoredAuthorizationChunkHeaderCandidateV0, StoredAuthorizationChunkRowsCandidateV0,
    ADMISSION_CLOSED, AUTHORITY_CORE_RESERVED_FEE, AUTHORITY_EXACT_EXTERNAL_CREDIT,
    AUTHORITY_INTENT_FUNDED, AUTHORIZATION_LIFECYCLE_ACTIVE, AUTHORIZATION_SNAPSHOT_ROW_LEN,
    DOMAIN_CONTROL_ROW_LEN, DOMAIN_RULE_CLOSED, ENGINE_REQUEST_MAGIC, EXECUTE_ENVELOPE_HEADER_LEN,
    FEE_CLASS_GROSS_DEBIT_RATE, FEE_CLASS_NONE, FEE_SHARD_ROW_LEN, NONE_INDEX, PHASE_TRANSITION,
    RIGHT_CORE_RESERVED_FEE, RIGHT_CREDIT, RIGHT_DEBIT, RIGHT_EXACT_EXTERNAL_RECIPIENT,
    SETTLEMENT_CAPABILITY_ROW_LEN, SETTLEMENT_FLAG_FEE_FUNDING,
    STORED_AUTHORIZATION_CHUNK_KIND_CONSTRAINT, STORED_AUTHORIZATION_CHUNK_KIND_TERM, WIRE_VERSION,
    WITNESS_STORED_AUTHORIZATION,
};
use litesvm::LiteSVM;
use litesvm_token::Approve;
use solana_clock::Clock;
use solana_keypair::Keypair;
use solana_message::{AccountMeta, Instruction};
use solana_native_token::LAMPORTS_PER_SOL;
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

use programmable_generic_effect_core::{
    account_segments::EffectivePrivilege,
    authorization::derive_exact_spend_authority,
    capabilities::{
        validate_settlement_capabilities, AssetProfileIdentity, CapabilityValidationContext,
        DomainCapabilityIdentity, SettlementCapability,
    },
    constants::{EXPERIMENTAL_MAJOR, MAX_ASSETS},
    state::{
        DomainAccountingAssetSlotCandidateV0, DomainAccountingCandidateV0,
        DomainAdmissionAccountCandidateV0, DomainDescriptorAccountCandidateV0,
        FeeLiabilityLedgerCandidateV0, FeeShardDescriptorCandidateV0, MarketDescriptorCandidateV0,
        StoredAuthorizationCandidateV0,
    },
    token_settlement::ClassicSplEndpointSnapshot,
};

use super::{
    build_core_execute_instruction, compile_v0_transaction, create_token_account, fixture_keypair,
    install_anchor_account, install_fixture_mint, install_lookup_table, install_raw_account,
    lookup_candidates, mint_tokens, must_send_legacy, read_anchor_account,
    request_heap_frame_instruction, set_compute_unit_limit_instruction, token_state,
    CoreExecuteAccountClosure, DirectFixture, SbfArtifacts, V0MessageResources,
    CONTROLLED_COMPUTE_UNIT_LIMIT, CONTROLLED_HEAP_FRAME_BYTES, DIRECT_FEE_RATE_DENOMINATOR,
    DIRECT_FEE_RATE_NUMERATOR, DIRECT_SOURCE_BALANCE,
};

pub const RESOURCE_MOVE_A: u64 = 100_000;
pub const RESOURCE_MOVE_B: u64 = 200_000;
pub const RESOURCE_FEE_A: u64 = 300;
pub const RESOURCE_FEE_B: u64 = 600;
pub const RESOURCE_PAYLOAD_BYTES: usize = 28;
pub const RESOURCE_CORE_ACCOUNT_POSITIONS: usize = 6 + 1 + 6 + 4 + 3 + 4 + 6 + 4;
pub const RESOURCE_CORE_INSTRUCTION_BYTES: usize = 8
    + EXECUTE_ENVELOPE_HEADER_LEN
    + 2 * DOMAIN_CONTROL_ROW_LEN
    + 2 * AUTHORIZATION_SNAPSHOT_ROW_LEN
    + 2 * FEE_SHARD_ROW_LEN
    + 6 * SETTLEMENT_CAPABILITY_ROW_LEN
    + RESOURCE_PAYLOAD_BYTES;
pub const RESOURCE_ENGINE_REQUEST_BYTES: usize = 1_396;
pub const RESOURCE_FROZEN_MEASURED_PEAK_BUMP_BYTES: u64 = 158_408;
pub const RESOURCE_HEAP_MEASUREMENT_ARTIFACT_SHA256: &str =
    "e9f9cb6fbaf17bba498d02d20791874c8f3eddb5814173442d66441c9151ad1d";

const _: () = assert!(RESOURCE_CORE_ACCOUNT_POSITIONS == 34);
const _: () = assert!(RESOURCE_CORE_INSTRUCTION_BYTES == 636);

#[derive(Clone, Copy)]
struct ResourceDomain {
    descriptor: anchor_lang::prelude::Pubkey,
    descriptor_digest: [u8; 32],
    revision: u64,
    accounting: anchor_lang::prelude::Pubkey,
    admission: anchor_lang::prelude::Pubkey,
    admission_digest: [u8; 32],
    accounting_profile_digest: [u8; 32],
}

#[derive(Clone)]
struct AuthorizationPlan {
    identity: InlineIntentIdentityRowCandidateV0,
    intent_digest: [u8; 32],
    market_binding_digest: [u8; 32],
    loader_state_snapshot_digest: [u8; 32],
    fee_policy_digest: [u8; 32],
    capability_terms_root: [u8; 32],
    credit_constraints_root: [u8; 32],
    core_terms_root: [u8; 32],
    terms: Vec<IntentCapabilityTermRowCandidateV0>,
    constraint: CreditConstraintRowCandidateV0,
}

/// Executable corrected AC13 resource graph. The established Direct fixture
/// supplies the exact immutable loader/market/configuration facts and the
/// first classic-SPL asset/shard. Everything execution-specific is rebuilt
/// from installed state for the two-asset, two-domain, two-authorization case.
pub struct ResourceFixture {
    pub direct: DirectFixture,
    pub mints: [anchor_lang::prelude::Pubkey; 2],
    pub sources: [anchor_lang::prelude::Pubkey; 2],
    pub recipients: [anchor_lang::prelude::Pubkey; 2],
    pub fee_vaults: [anchor_lang::prelude::Pubkey; 2],
    pub fee_descriptors: [anchor_lang::prelude::Pubkey; 2],
    pub fee_liabilities: [anchor_lang::prelude::Pubkey; 2],
    pub domain_descriptors: [anchor_lang::prelude::Pubkey; 2],
    pub domain_admissions: [anchor_lang::prelude::Pubkey; 2],
    pub domain_accounting: [anchor_lang::prelude::Pubkey; 2],
    pub authorizations: [anchor_lang::prelude::Pubkey; 2],
    pub spend_authorities: [anchor_lang::prelude::Pubkey; 2],
    pub engine_states: [anchor_lang::prelude::Pubkey; 2],
    pub helper_state: anchor_lang::prelude::Pubkey,
    pub request_digest: [u8; 32],
}

impl ResourceFixture {
    pub fn new(artifacts: &SbfArtifacts) -> Self {
        assert_eq!(
            RESOURCE_MOVE_A * DIRECT_FEE_RATE_NUMERATOR / DIRECT_FEE_RATE_DENOMINATOR,
            RESOURCE_FEE_A,
        );
        assert_eq!(
            RESOURCE_MOVE_B * DIRECT_FEE_RATE_NUMERATOR / DIRECT_FEE_RATE_DENOMINATOR,
            RESOURCE_FEE_B,
        );
        let mut direct = DirectFixture::state_only(artifacts);
        let actor_b = fixture_keypair(180);
        let recipient_b_owner = fixture_keypair(181);
        direct
            .svm
            .airdrop(&actor_b.pubkey(), LAMPORTS_PER_SOL)
            .expect("install the second stored actor as a signer account");

        let protected_profile_digest = direct.engine_request.assets[0].settlement_profile_digest;
        let asset_a_binding = asset_binding_from_engine(direct.engine_request.assets[0]);
        let asset_a_digest = asset_a_binding
            .digest()
            .expect("derive the first resource asset binding");
        let mint_b_tag = (190_u8..=250)
            .find(|tag| {
                let candidate = AssetBindingRowCandidateV0 {
                    wire_version: WIRE_VERSION,
                    flags: 0,
                    decimals: 6,
                    reserved: 0,
                    asset_identity: anchor_lang::prelude::Pubkey::new_from_array([*tag; 32])
                        .to_bytes(),
                    asset_program: litesvm_token::TOKEN_ID.to_bytes(),
                    settlement_profile_digest: protected_profile_digest,
                };
                candidate
                    .digest()
                    .is_ok_and(|digest| asset_a_digest < digest)
            })
            .expect("find a deterministic second mint after the first binding digest");
        let mint_b = install_fixture_mint(&mut direct.svm, mint_b_tag, direct.payer.pubkey(), 6);
        let source_b =
            create_token_account(&mut direct.svm, &direct.payer, &mint_b, &actor_b.pubkey());
        let recipient_b = create_token_account(
            &mut direct.svm,
            &direct.payer,
            &mint_b,
            &recipient_b_owner.pubkey(),
        );
        let fee_vault_b = create_token_account(
            &mut direct.svm,
            &direct.payer,
            &mint_b,
            &direct.payer.pubkey(),
        );
        mint_tokens(
            &mut direct.svm,
            &direct.payer,
            &mint_b,
            &source_b,
            DIRECT_SOURCE_BALANCE,
        );

        let asset_b_binding = AssetBindingRowCandidateV0 {
            wire_version: WIRE_VERSION,
            flags: 0,
            decimals: 6,
            reserved: 0,
            asset_identity: mint_b.to_bytes(),
            asset_program: litesvm_token::TOKEN_ID.to_bytes(),
            settlement_profile_digest: protected_profile_digest,
        };
        let asset_b_digest = asset_b_binding
            .digest()
            .expect("derive the second resource asset binding");
        assert!(asset_a_digest < asset_b_digest);
        let asset_bindings = [asset_a_binding, asset_b_binding];
        let asset_set_digest =
            compute_asset_set_digest(&asset_bindings).expect("derive the resource asset set");

        let (fee_descriptor_b, fee_liability_b) =
            install_second_fee_shard(&mut direct, mint_b, fee_vault_b, asset_b_digest);
        let fee_descriptors = [direct.fee_shard_descriptor, fee_descriptor_b];
        let fee_liabilities = [direct.fee_liability, fee_liability_b];
        let mints = [direct.mint, mint_b];
        let sources = [direct.source, source_b];
        let recipients = [direct.destination, recipient_b];
        let fee_vaults = [direct.fee_vault, fee_vault_b];

        let current_slot = direct.svm.get_sysvar::<Clock>().slot;
        let mut domains = vec![
            install_closed_domain(&mut direct, 160, 0, current_slot),
            install_closed_domain(&mut direct, 161, 1, current_slot),
        ];
        domains.sort_by_key(|domain| domain.descriptor_digest);
        assert!(domains[0].descriptor_digest < domains[1].descriptor_digest);
        let domains: [ResourceDomain; 2] = domains
            .try_into()
            .unwrap_or_else(|_| unreachable!("resource fixture installs two domains"));
        let domain_rows = domains
            .iter()
            .enumerate()
            .map(|(index, domain)| DomainExecutionRowCandidateV0 {
                domain_index: u8::try_from(index).expect("domain index fits u8"),
                admission_kind: ADMISSION_CLOSED,
                domain_descriptor_key: domain.descriptor.to_bytes(),
                domain_descriptor_digest: domain.descriptor_digest,
                domain_revision: domain.revision,
                admission_account_or_zero: domain.admission.to_bytes(),
                admission_digest: domain.admission_digest,
                accounting_account: domain.accounting.to_bytes(),
                accounting_profile_digest: domain.accounting_profile_digest,
            })
            .collect::<Vec<_>>();
        let market_binding_digest = direct.engine_request.header.market_binding_digest;
        let domain_set_digest = compute_domain_set_digest(&market_binding_digest, &domain_rows)
            .expect("derive the resource domain set");

        let declarations = resource_declarations();
        let auth_a_terms = vec![
            intent_term(
                declarations[0],
                sources[0],
                asset_a_digest,
                domains[0].descriptor_digest,
            ),
            intent_term(
                declarations[1],
                recipients[1],
                asset_b_digest,
                domains[0].descriptor_digest,
            ),
        ];
        let auth_b_terms = vec![
            intent_term(
                declarations[2],
                sources[1],
                asset_b_digest,
                domains[1].descriptor_digest,
            ),
            intent_term(
                declarations[3],
                recipients[0],
                asset_a_digest,
                domains[1].descriptor_digest,
            ),
        ];
        let auth_a_constraint = credit_constraint(2, 1, RESOURCE_MOVE_B);
        let auth_b_constraint = credit_constraint(1, 2, RESOURCE_MOVE_A);
        let auth_b_plan = authorization_plan(
            &direct,
            actor_b.pubkey(),
            2,
            [0x92; 32],
            current_slot + 200,
            auth_b_terms,
            auth_b_constraint,
        );
        let auth_a_plan = (0_u16..=u16::MAX)
            .find_map(|marker| {
                let mut commitment = [0x91; 32];
                commitment[..2].copy_from_slice(&marker.to_le_bytes());
                let candidate = authorization_plan(
                    &direct,
                    direct.actor.pubkey(),
                    1,
                    commitment,
                    current_slot + 200,
                    auth_a_terms.clone(),
                    auth_a_constraint,
                );
                (candidate.intent_digest < auth_b_plan.intent_digest).then_some(candidate)
            })
            .expect("find deterministic resource intent ordering");
        assert!(auth_a_plan.intent_digest < auth_b_plan.intent_digest);

        let (authorization_a, spend_a) = install_stored_authorization(
            &mut direct.svm,
            &direct.payer,
            &direct.actor,
            &auth_a_plan,
            sources[0],
            declarations[0].maximum_total_debit,
        );
        let (authorization_b, spend_b) = install_stored_authorization(
            &mut direct.svm,
            &direct.payer,
            &actor_b,
            &auth_b_plan,
            sources[1],
            declarations[2].maximum_total_debit,
        );
        let authorizations = [authorization_a, authorization_b];
        let spend_authorities = [spend_a, spend_b];
        let authorization_states: [StoredAuthorizationCandidateV0; 2] = [
            read_anchor_account(&direct.svm, &authorization_a),
            read_anchor_account(&direct.svm, &authorization_b),
        ];

        let intent_set_digest = compute_intent_set_digest(
            &domain_set_digest,
            &[
                IntentSetRowCandidateV0 {
                    intent_digest: auth_a_plan.intent_digest,
                },
                IntentSetRowCandidateV0 {
                    intent_digest: auth_b_plan.intent_digest,
                },
            ],
        )
        .expect("derive the ordered resource intent set");
        let authorization_view_set_digest = compute_authorization_view_set_digest(&[
            authorization_view(0, authorization_a, &authorization_states[0]),
            authorization_view(1, authorization_b, &authorization_states[1]),
        ])
        .expect("derive resource authorization views");

        let endpoint_snapshots = [
            endpoint_snapshot(&direct.svm, sources[0]),
            endpoint_snapshot(&direct.svm, recipients[1]),
            endpoint_snapshot(&direct.svm, sources[1]),
            endpoint_snapshot(&direct.svm, recipients[0]),
            endpoint_snapshot(&direct.svm, fee_vaults[0]),
            endpoint_snapshot(&direct.svm, fee_vaults[1]),
        ];
        let domain_identities = [
            capability_domain(0, domains[0]),
            capability_domain(1, domains[1]),
        ];
        let asset_identities = [
            asset_identity(mints[0], protected_profile_digest),
            asset_identity(mints[1], protected_profile_digest),
        ];
        let protected_capability_set_digest = validate_settlement_capabilities(
            &[
                protected_capability(
                    0,
                    declarations[0],
                    direct.market,
                    endpoint_snapshots[0],
                    spend_a,
                    asset_identities[0],
                    Some(domain_identities[0]),
                    direct.engine_request.fee_policy.revision,
                ),
                protected_capability(
                    1,
                    declarations[1],
                    direct.market,
                    endpoint_snapshots[1],
                    Default::default(),
                    asset_identities[1],
                    Some(domain_identities[0]),
                    direct.engine_request.fee_policy.revision,
                ),
                protected_capability(
                    2,
                    declarations[2],
                    direct.market,
                    endpoint_snapshots[2],
                    spend_b,
                    asset_identities[1],
                    Some(domain_identities[1]),
                    direct.engine_request.fee_policy.revision,
                ),
                protected_capability(
                    3,
                    declarations[3],
                    direct.market,
                    endpoint_snapshots[3],
                    Default::default(),
                    asset_identities[0],
                    Some(domain_identities[1]),
                    direct.engine_request.fee_policy.revision,
                ),
                protected_capability(
                    4,
                    declarations[4],
                    direct.market,
                    endpoint_snapshots[4],
                    Default::default(),
                    asset_identities[0],
                    None,
                    direct.engine_request.fee_policy.revision,
                ),
                protected_capability(
                    5,
                    declarations[5],
                    direct.market,
                    endpoint_snapshots[5],
                    Default::default(),
                    asset_identities[1],
                    None,
                    direct.engine_request.fee_policy.revision,
                ),
            ],
            CapabilityValidationContext {
                core_program: programmable_generic_effect_core::ID,
                market: direct.market,
                classic_token_program: litesvm_token::TOKEN_ID,
                experimental_major: EXPERIMENTAL_MAJOR,
                intent_count: 2,
                asset_count: 2,
                domain_count: 2,
                fee_shard_count: 2,
                fee_policy_revision: direct.engine_request.fee_policy.revision,
            },
        )
        .expect("validate the resource protected capabilities");

        let fee_shard_set_digest = resource_fee_shard_set_digest(
            &direct,
            fee_descriptors,
            fee_liabilities,
            fee_vaults,
            [asset_a_digest, asset_b_digest],
        );
        let protected_execution_root =
            compute_protected_execution_root(ProtectedExecutionRootInputs {
                core_program: &programmable_generic_effect_core::ID.to_bytes(),
                market_binding_digest: &market_binding_digest,
                engine_loader_state_snapshot_digest: &direct
                    .engine_request
                    .header
                    .engine_loader_state_snapshot_digest,
                domain_set_digest: &domain_set_digest,
                intent_set_digest: &intent_set_digest,
                fee_policy_digest: &direct.engine_request.header.fee_policy_digest,
                asset_set_digest: &asset_set_digest,
                authorization_view_set_digest: &authorization_view_set_digest,
                fee_shard_set_digest: &fee_shard_set_digest,
                protected_capability_set_digest: &protected_capability_set_digest,
            })
            .expect("derive the resource protected execution root");

        let engine_states = [fixture_keypair(170).pubkey(), fixture_keypair(171).pubkey()];
        for state in engine_states {
            install_raw_account(
                &mut direct.svm,
                state,
                effect_engine_probe::ID,
                EngineStateCandidateV0::fresh().encode().to_vec(),
                false,
            );
        }
        let helper_state = fixture_keypair(172).pubkey();
        install_anchor_account(
            &mut direct.svm,
            helper_state,
            callback_capability_probe::ID,
            &callback_capability_probe::HelperState {
                allowed_callback: fixture_keypair(173).pubkey(),
                calls: 0,
                value: 0,
                descendant_receipt_sets: 0,
            },
            8 + callback_capability_probe::HelperState::INIT_SPACE,
        );
        let opaque_metas = vec![
            AccountMeta::new(engine_states[0], false),
            AccountMeta::new(engine_states[1], false),
            AccountMeta::new_readonly(callback_capability_probe::ID, false),
            AccountMeta::new(helper_state, false),
        ];
        let opaque_root = opaque_root(&direct.svm, &opaque_metas);
        let payload = encode_explicit_plan(
            RECEIPT_ACCEPT,
            0b0000_0011,
            2,
            3,
            &[
                PlannedMove {
                    source_capability_index: 0,
                    destination_capability_index: 3,
                    amount: RESOURCE_MOVE_A,
                },
                PlannedMove {
                    source_capability_index: 2,
                    destination_capability_index: 1,
                    amount: RESOURCE_MOVE_B,
                },
            ],
        )
        .expect("encode the explicit corrected resource plan");
        assert_eq!(payload.len(), RESOURCE_PAYLOAD_BYTES);

        let engine_request = EngineRequestCandidateV0 {
            header: EngineRequestHeaderCandidateV0 {
                magic: ENGINE_REQUEST_MAGIC,
                wire_version: WIRE_VERSION,
                phase: PHASE_TRANSITION,
                settlement_capability_count: 6,
                opaque_capability_count: 4,
                intent_count: 2,
                domain_count: 2,
                asset_count: 2,
                context_row_count: 4,
                payload_len: RESOURCE_PAYLOAD_BYTES as u16,
                maximum_engine_moves: 6,
                market_binding_digest,
                engine_instance_id: direct.engine_request.header.engine_instance_id,
                engine_interface_id: direct.engine_request.header.engine_interface_id,
                intent_set_digest,
                domain_set_digest,
                protected_execution_root,
                opaque_capability_root: opaque_root,
                engine_loader_state_snapshot_digest: direct
                    .engine_request
                    .header
                    .engine_loader_state_snapshot_digest,
                fee_policy_digest: direct.engine_request.header.fee_policy_digest,
            },
            assets: asset_bindings
                .iter()
                .enumerate()
                .map(|(index, binding)| EngineAssetRowCandidateV0 {
                    asset_index: u8::try_from(index).expect("asset index fits u8"),
                    asset_flags: binding.flags,
                    decimals: binding.decimals,
                    reserved: binding.reserved,
                    asset_identity: binding.asset_identity,
                    asset_program: binding.asset_program,
                    settlement_profile_digest: binding.settlement_profile_digest,
                })
                .collect(),
            domains: domains
                .iter()
                .enumerate()
                .map(|(index, domain)| EngineDomainRowCandidateV0 {
                    domain_index: u8::try_from(index).expect("domain index fits u8"),
                    domain_descriptor: domain.descriptor.to_bytes(),
                    domain_revision: domain.revision,
                    admission_digest: domain.admission_digest,
                    accounting_profile_digest: domain.accounting_profile_digest,
                })
                .collect(),
            intents: vec![
                EngineIntentRowCandidateV0 {
                    authorization_slot: 0,
                    identity: authorization_states[0].identity.inline_identity(),
                    intent_digest: auth_a_plan.intent_digest,
                },
                EngineIntentRowCandidateV0 {
                    authorization_slot: 1,
                    identity: authorization_states[1].identity.inline_identity(),
                    intent_digest: auth_b_plan.intent_digest,
                },
            ],
            fee_policy: direct.engine_request.fee_policy,
            contexts: vec![
                stored_engine_context(
                    0,
                    declarations[0],
                    endpoint_snapshots[0],
                    &authorization_states[0],
                ),
                stored_engine_context(
                    1,
                    declarations[1],
                    endpoint_snapshots[1],
                    &authorization_states[0],
                ),
                stored_engine_context(
                    2,
                    declarations[2],
                    endpoint_snapshots[2],
                    &authorization_states[1],
                ),
                stored_engine_context(
                    3,
                    declarations[3],
                    endpoint_snapshots[3],
                    &authorization_states[1],
                ),
            ],
            payload: payload.clone(),
        };
        engine_request
            .validate()
            .expect("the corrected resource request is canonical");
        assert_eq!(
            engine_request
                .encode()
                .expect("encode the corrected resource request")
                .len(),
            RESOURCE_ENGINE_REQUEST_BYTES,
        );
        let request_digest = engine_request
            .digest()
            .expect("derive the corrected resource request digest");
        let callback_authority =
            derive_callback_authority_for_engine(&engine_request, &effect_engine_probe::ID)
                .expect("derive the corrected resource callback")
                .0;
        bind_helper_state(&mut direct.svm, helper_state, callback_authority);

        let envelope = ExecuteEnvelopeCandidateV0 {
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
                payload_len: RESOURCE_PAYLOAD_BYTES as u16,
                expires_at_slot_exclusive: current_slot + 100,
                expected_engine_sequence: 1,
                intent_set_digest,
                domain_set_digest,
                protected_execution_root,
                expected_opaque_capability_root: opaque_root,
                fee_policy_digest: direct.engine_request.header.fee_policy_digest,
                expected_engine_loader_state_snapshot_digest: direct
                    .engine_request
                    .header
                    .engine_loader_state_snapshot_digest,
                payload_digest: compute_payload_digest(&payload)
                    .expect("derive the corrected resource payload digest"),
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
            settlement_capabilities: declarations.to_vec(),
            payload,
        };
        let domain_controls = domains
            .iter()
            .flat_map(|domain| {
                [
                    AccountMeta::new_readonly(domain.descriptor, false),
                    AccountMeta::new_readonly(domain.admission, false),
                    AccountMeta::new(domain.accounting, false),
                ]
            })
            .collect();
        let instruction = build_core_execute_instruction(
            &envelope,
            &CoreExecuteAccountClosure {
                configuration: direct.configuration,
                market: direct.market,
                fee_policy: direct.fee_policy,
                engine_program: effect_engine_probe::ID,
                callback_authority,
                loader_policy: vec![direct.loader_policy_account],
                domain_controls,
                authorization_controls: vec![
                    AccountMeta::new(authorization_a, false),
                    AccountMeta::new(authorization_b, false),
                    AccountMeta::new_readonly(spend_a, false),
                    AccountMeta::new_readonly(spend_b, false),
                ],
                protected_profile: vec![litesvm_token::TOKEN_ID, mints[0], mints[1]],
                fee_controls: vec![
                    AccountMeta::new_readonly(fee_descriptors[0], false),
                    AccountMeta::new(fee_liabilities[0], false),
                    AccountMeta::new_readonly(fee_descriptors[1], false),
                    AccountMeta::new(fee_liabilities[1], false),
                ],
                settlement: vec![
                    AccountMeta::new(sources[0], false),
                    AccountMeta::new(recipients[1], false),
                    AccountMeta::new(sources[1], false),
                    AccountMeta::new(recipients[0], false),
                    AccountMeta::new(fee_vaults[0], false),
                    AccountMeta::new(fee_vaults[1], false),
                ],
                opaque: opaque_metas,
            },
        )
        .expect("build the corrected resource Core instruction");
        assert_eq!(instruction.accounts.len(), RESOURCE_CORE_ACCOUNT_POSITIONS);
        assert_eq!(instruction.data.len(), RESOURCE_CORE_INSTRUCTION_BYTES);

        direct.envelope = envelope;
        direct.engine_request = engine_request;
        direct.callback_authority = callback_authority;
        direct.instruction = instruction;

        Self {
            direct,
            mints,
            sources,
            recipients,
            fee_vaults,
            fee_descriptors,
            fee_liabilities,
            domain_descriptors: [domains[0].descriptor, domains[1].descriptor],
            domain_admissions: [domains[0].admission, domains[1].admission],
            domain_accounting: [domains[0].accounting, domains[1].accounting],
            authorizations,
            spend_authorities,
            engine_states,
            helper_state,
            request_digest,
        }
    }

    pub fn core_instruction(&self) -> &Instruction {
        &self.direct.instruction
    }

    pub fn compile_routed_v0(
        &mut self,
    ) -> (VersionedTransaction, V0MessageResources, Vec<Instruction>) {
        let routed = forward_exact_once(&self.direct.instruction);
        let instructions = vec![
            set_compute_unit_limit_instruction(CONTROLLED_COMPUTE_UNIT_LIMIT),
            request_heap_frame_instruction(CONTROLLED_HEAP_FRAME_BYTES),
            routed,
        ];
        let lookup = install_lookup_table(
            &mut self.direct.svm,
            &self.direct.payer,
            lookup_candidates(&instructions, self.direct.payer.pubkey()),
        );
        let (transaction, resources) = compile_v0_transaction(
            &self.direct.svm,
            &self.direct.payer,
            &instructions,
            &[lookup],
        )
        .expect("compile and sign the corrected routed resource transaction");
        (transaction, resources, instructions)
    }

    pub fn authorization_state(&self, index: usize) -> StoredAuthorizationCandidateV0 {
        read_anchor_account(&self.direct.svm, &self.authorizations[index])
    }

    pub fn fee_liability_state(&self, index: usize) -> FeeLiabilityLedgerCandidateV0 {
        read_anchor_account(&self.direct.svm, &self.fee_liabilities[index])
    }

    pub fn engine_state(&self, index: usize) -> EngineStateCandidateV0 {
        let account = self
            .direct
            .svm
            .get_account(&self.engine_states[index])
            .expect("resource engine state exists");
        assert_eq!(account.data.len(), ENGINE_STATE_LEN);
        EngineStateCandidateV0::decode_exact(&account.data)
            .expect("decode the exact resource engine state")
    }

    pub fn helper(&self) -> callback_capability_probe::HelperState {
        read_anchor_account(&self.direct.svm, &self.helper_state)
    }
}

fn install_second_fee_shard(
    direct: &mut DirectFixture,
    mint: anchor_lang::prelude::Pubkey,
    vault: anchor_lang::prelude::Pubkey,
    _asset_binding_digest: [u8; 32],
) -> (anchor_lang::prelude::Pubkey, anchor_lang::prelude::Pubkey) {
    let market_binding_digest = direct.engine_request.header.market_binding_digest;
    let fee_policy_digest = direct.engine_request.header.fee_policy_digest;
    let profile = direct.engine_request.assets[0].settlement_profile_digest;
    let revision = direct.engine_request.fee_policy.revision;
    let (descriptor, descriptor_bump) = FeeShardDescriptorCandidateV0::address(
        &programmable_generic_effect_core::ID,
        &market_binding_digest,
        1,
    );
    let (liability, liability_bump) = FeeLiabilityLedgerCandidateV0::address(
        &programmable_generic_effect_core::ID,
        &descriptor,
        &market_binding_digest,
    );
    let recipient_policy_digest = compute_exact_fee_recipient_policy_digest(
        &programmable_generic_effect_core::ID.to_bytes(),
        &market_binding_digest,
        &vault.to_bytes(),
        &mint.to_bytes(),
        &litesvm_token::TOKEN_ID.to_bytes(),
        &profile,
    )
    .expect("derive the second resource fee recipient policy");
    let mut descriptor_state = FeeShardDescriptorCandidateV0 {
        wire_version: WIRE_VERSION,
        shard_index: 1,
        bump: descriptor_bump,
        reserved: [0; 5],
        descriptor_digest: [0; 32],
        market_binding_digest,
        fee_policy_digest,
        fee_policy_revision: revision,
        asset_identity: mint,
        asset_program: litesvm_token::TOKEN_ID,
        settlement_profile_digest: profile,
        vault,
        liability_ledger: liability,
        recipient_policy_digest,
    };
    descriptor_state.descriptor_digest = descriptor_state
        .derive_descriptor_digest(&programmable_generic_effect_core::ID)
        .expect("derive the second resource fee descriptor digest");
    let liability_state = FeeLiabilityLedgerCandidateV0 {
        wire_version: WIRE_VERSION,
        shard_index: 1,
        bump: liability_bump,
        reserved: [0; 5],
        descriptor,
        market_binding_digest,
        asset_identity: mint,
        settlement_profile_digest: profile,
        liability: 0,
    };
    install_anchor_account(
        &mut direct.svm,
        descriptor,
        programmable_generic_effect_core::ID,
        &descriptor_state,
        FeeShardDescriptorCandidateV0::SPACE,
    );
    install_anchor_account(
        &mut direct.svm,
        liability,
        programmable_generic_effect_core::ID,
        &liability_state,
        FeeLiabilityLedgerCandidateV0::SPACE,
    );
    (descriptor, liability)
}

fn install_closed_domain(
    direct: &mut DirectFixture,
    descriptor_tag: u8,
    marker: u8,
    current_slot: u64,
) -> ResourceDomain {
    let market: MarketDescriptorCandidateV0 = read_anchor_account(&direct.svm, &direct.market);
    let descriptor = fixture_keypair(descriptor_tag).pubkey();
    let accounting_profile_digest = [0x60_u8 + marker; 32];
    let admission_rule_digest = [0x70_u8 + marker; 32];
    let descriptor_state = DomainDescriptorAccountCandidateV0 {
        wire_version: WIRE_VERSION,
        rule_kind: DOMAIN_RULE_CLOSED,
        reserved: [0; 6],
        controller_program: callback_capability_probe::ID,
        controller_identity: fixture_keypair(162 + marker).pubkey(),
        domain_revision: 1 + u64::from(marker),
        namespace_or_instance: [0x40_u8 + marker; 32],
        custody_profile_digest: [0x42_u8 + marker; 32],
        asset_profile_digest: [0x44_u8 + marker; 32],
        accounting_profile_digest,
        exit_class_digest: [0x46_u8 + marker; 32],
        admission_rule_digest,
        protected_profile_digest: market.protected_profile_digest,
    };
    let descriptor_digest = descriptor_state
        .digest(&programmable_generic_effect_core::ID)
        .expect("derive a resource domain descriptor digest");
    let (accounting, accounting_bump) =
        DomainAccountingCandidateV0::address(&programmable_generic_effect_core::ID, &descriptor);
    let accounting_state = DomainAccountingCandidateV0 {
        wire_version: WIRE_VERSION,
        asset_count: 0,
        bump: accounting_bump,
        reserved: [0; 5],
        domain_descriptor: descriptor,
        domain_revision: descriptor_state.domain_revision,
        assets: [DomainAccountingAssetSlotCandidateV0::default(); MAX_ASSETS],
    };
    accounting_state
        .validate_authenticated(
            &programmable_generic_effect_core::ID,
            &accounting,
            &descriptor,
            descriptor_state.domain_revision,
        )
        .expect("validate zero-asset resource domain accounting");
    let admission_row = DomainAdmissionCandidateV0 {
        wire_version: WIRE_VERSION,
        domain_descriptor: descriptor.to_bytes(),
        domain_revision: descriptor_state.domain_revision,
        market: direct.market.to_bytes(),
        engine_program: market.engine_program.to_bytes(),
        engine_interface_id: market.engine_interface_id,
        engine_instance_policy_digest: market
            .exact_engine_instance_policy_digest(
                &programmable_generic_effect_core::ID,
                &direct.market,
            )
            .expect("derive exact Engine policy for a resource domain"),
        engine_admission_policy_digest: market.engine_admission_policy_digest,
        settlement_profile_digest: market.protected_profile_digest,
        admission_rule_digest,
        active_from_slot: current_slot,
        expires_at_slot_or_zero: 0,
        revoked_at_slot_or_zero: 0,
    };
    let admission = DomainAdmissionAccountCandidateV0::address(
        &programmable_generic_effect_core::ID,
        &admission_row,
    )
    .expect("derive a resource domain admission PDA")
    .0;
    let admission_digest = admission_row
        .digest()
        .expect("derive a resource domain admission digest");
    install_anchor_account(
        &mut direct.svm,
        descriptor,
        programmable_generic_effect_core::ID,
        &descriptor_state,
        DomainDescriptorAccountCandidateV0::SPACE,
    );
    install_anchor_account(
        &mut direct.svm,
        accounting,
        programmable_generic_effect_core::ID,
        &accounting_state,
        DomainAccountingCandidateV0::SPACE,
    );
    let admission_state = admission_state(admission_row);
    install_anchor_account(
        &mut direct.svm,
        admission,
        programmable_generic_effect_core::ID,
        &admission_state,
        DomainAdmissionAccountCandidateV0::SPACE,
    );
    ResourceDomain {
        descriptor,
        descriptor_digest,
        revision: descriptor_state.domain_revision,
        accounting,
        admission,
        admission_digest,
        accounting_profile_digest,
    }
}

fn admission_state(row: DomainAdmissionCandidateV0) -> DomainAdmissionAccountCandidateV0 {
    DomainAdmissionAccountCandidateV0 {
        wire_version: row.wire_version,
        reserved: [0; 7],
        domain_descriptor: row.domain_descriptor,
        domain_revision: row.domain_revision,
        market: row.market,
        engine_program: row.engine_program,
        engine_interface_id: row.engine_interface_id,
        engine_instance_policy_digest: row.engine_instance_policy_digest,
        engine_admission_policy_digest: row.engine_admission_policy_digest,
        settlement_profile_digest: row.settlement_profile_digest,
        admission_rule_digest: row.admission_rule_digest,
        active_from_slot: row.active_from_slot,
        expires_at_slot_or_zero: row.expires_at_slot_or_zero,
        revoked_at_slot_or_zero: row.revoked_at_slot_or_zero,
    }
}

fn authorization_plan(
    direct: &DirectFixture,
    actor: anchor_lang::prelude::Pubkey,
    nonce: u64,
    engine_terms_commitment: [u8; 32],
    expires_at_slot_exclusive: u64,
    terms: Vec<IntentCapabilityTermRowCandidateV0>,
    constraint: CreditConstraintRowCandidateV0,
) -> AuthorizationPlan {
    let capability_terms_root =
        compute_intent_capability_terms_root(&terms).expect("derive stored resource terms root");
    let credit_constraints_root = compute_intent_credit_constraints_root(&[constraint])
        .expect("derive stored resource credit constraints root");
    let core_terms_root = compute_intent_core_terms_root(IntentCoreTermsDigestInputs {
        maximum_successful_fills: 1,
        capability_terms_root: &capability_terms_root,
        credit_constraints_root: &credit_constraints_root,
    })
    .expect("derive stored resource Core terms root");
    let identity = InlineIntentIdentityRowCandidateV0 {
        actor: actor.to_bytes(),
        engine_terms_commitment,
        authorization_nonce: nonce,
        expires_at_slot_exclusive,
    };
    let intent_digest = compute_intent_digest(IntentDigestInputs {
        core_program: &programmable_generic_effect_core::ID.to_bytes(),
        market_binding_digest: &direct.engine_request.header.market_binding_digest,
        loader_state_snapshot_digest: &direct
            .engine_request
            .header
            .engine_loader_state_snapshot_digest,
        fee_policy_digest: &direct.engine_request.header.fee_policy_digest,
        identity: &identity,
        core_terms_root: &core_terms_root,
    })
    .expect("derive stored resource intent digest");
    AuthorizationPlan {
        identity,
        intent_digest,
        market_binding_digest: direct.engine_request.header.market_binding_digest,
        loader_state_snapshot_digest: direct
            .engine_request
            .header
            .engine_loader_state_snapshot_digest,
        fee_policy_digest: direct.engine_request.header.fee_policy_digest,
        capability_terms_root,
        credit_constraints_root,
        core_terms_root,
        terms,
        constraint,
    }
}

fn install_stored_authorization(
    svm: &mut LiteSVM,
    payer: &Keypair,
    actor: &Keypair,
    plan: &AuthorizationPlan,
    source: anchor_lang::prelude::Pubkey,
    delegated_amount: u64,
) -> (anchor_lang::prelude::Pubkey, anchor_lang::prelude::Pubkey) {
    let authorization = StoredAuthorizationCandidateV0::address(
        &programmable_generic_effect_core::ID,
        &plan.intent_digest,
    )
    .0;
    let initialize = Instruction {
        program_id: programmable_generic_effect_core::ID,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new_readonly(actor.pubkey(), true),
            AccountMeta::new(authorization, false),
            AccountMeta::new_readonly(solana_sdk_ids::system_program::id(), false),
            AccountMeta::new_readonly(solana_sdk_ids::sysvar::instructions::id(), false),
        ],
        data: CoreControlInstructionCandidateV0::InitializeStoredAuthorization(
            InitializeStoredAuthorizationArgsCandidateV0 {
                wire_version: WIRE_VERSION,
                term_count: u8::try_from(plan.terms.len()).expect("resource term count fits u8"),
                constraint_count: 1,
                flags: 0,
                maximum_successful_fills: 1,
                identity: plan.identity,
                market_binding_digest: plan.market_binding_digest,
                engine_loader_state_snapshot_digest: plan.loader_state_snapshot_digest,
                fee_policy_digest: plan.fee_policy_digest,
                intent_capability_terms_root: plan.capability_terms_root,
                credit_constraints_root: plan.credit_constraints_root,
                core_terms_root: plan.core_terms_root,
                intent_digest: plan.intent_digest,
            },
        )
        .encode()
        .expect("encode a resource stored initializer"),
    };
    must_send_legacy(
        svm,
        payer,
        &[initialize],
        &[actor],
        "initialize a resource stored authorization",
    );
    let terms = stored_control_instruction(
        actor.pubkey(),
        authorization,
        CoreControlInstructionCandidateV0::WriteStoredAuthorizationChunk(
            StoredAuthorizationChunkCandidateV0 {
                header: StoredAuthorizationChunkHeaderCandidateV0 {
                    wire_version: WIRE_VERSION,
                    chunk_kind: STORED_AUTHORIZATION_CHUNK_KIND_TERM,
                    start_index: 0,
                    row_count: u8::try_from(plan.terms.len())
                        .expect("resource term chunk count fits u8"),
                },
                rows: StoredAuthorizationChunkRowsCandidateV0::Terms(plan.terms.clone()),
            },
        ),
    );
    must_send_legacy(
        svm,
        payer,
        &[terms],
        &[actor],
        "write resource stored authorization terms",
    );
    let constraint = stored_control_instruction(
        actor.pubkey(),
        authorization,
        CoreControlInstructionCandidateV0::WriteStoredAuthorizationChunk(
            StoredAuthorizationChunkCandidateV0 {
                header: StoredAuthorizationChunkHeaderCandidateV0 {
                    wire_version: WIRE_VERSION,
                    chunk_kind: STORED_AUTHORIZATION_CHUNK_KIND_CONSTRAINT,
                    start_index: 0,
                    row_count: 1,
                },
                rows: StoredAuthorizationChunkRowsCandidateV0::Constraints(vec![plan.constraint]),
            },
        ),
    );
    must_send_legacy(
        svm,
        payer,
        &[constraint],
        &[actor],
        "write resource stored authorization constraint",
    );
    let activate = stored_control_instruction(
        actor.pubkey(),
        authorization,
        CoreControlInstructionCandidateV0::ActivateStoredAuthorization,
    );
    must_send_legacy(
        svm,
        payer,
        &[activate],
        &[actor],
        "activate a resource stored authorization",
    );
    let spend_authority = derive_exact_spend_authority(
        &programmable_generic_effect_core::ID,
        &plan.intent_digest,
        &source,
    )
    .expect("derive a resource stored spend authority")
    .0;
    Approve::new(svm, payer, &spend_authority, &source, delegated_amount)
        .owner(actor)
        .send()
        .expect("approve a resource stored spend authority");
    (authorization, spend_authority)
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
            .expect("encode a resource stored control instruction"),
    }
}

fn resource_declarations() -> [SettlementCapabilityRowCandidateV0; 6] {
    [
        intent_debit(0, 0, 0, 0, 2, RESOURCE_MOVE_A, RESOURCE_FEE_A),
        exact_credit(1, 0, 0, 1),
        intent_debit(1, 1, 1, 0, 3, RESOURCE_MOVE_B, RESOURCE_FEE_B),
        exact_credit(0, 1, 1, 1),
        fee_vault(0, 0),
        fee_vault(1, 1),
    ]
}

fn intent_debit(
    asset_index: u8,
    domain_index: u8,
    authorization_slot: u8,
    local_term: u8,
    spend_offset: u8,
    amount: u64,
    fee: u64,
) -> SettlementCapabilityRowCandidateV0 {
    SettlementCapabilityRowCandidateV0 {
        asset_index,
        domain_index_or_none: domain_index,
        authorization_slot_or_none: authorization_slot,
        intent_local_term_index_or_none: local_term,
        authority_class: AUTHORITY_INTENT_FUNDED,
        fee_shard_index_or_none: asset_index,
        fee_class: FEE_CLASS_GROSS_DEBIT_RATE,
        flags: SETTLEMENT_FLAG_FEE_FUNDING,
        rights_bits: RIGHT_DEBIT,
        domain_accounting_slot_or_none: NONE_INDEX,
        spend_authority_control_offset_or_none: spend_offset,
        reserved_0: 0,
        maximum_engine_debit: amount,
        maximum_total_debit: amount + fee,
        minimum_credit: 0,
        maximum_protocol_fee: fee,
    }
}

fn exact_credit(
    asset_index: u8,
    domain_index: u8,
    authorization_slot: u8,
    local_term: u8,
) -> SettlementCapabilityRowCandidateV0 {
    SettlementCapabilityRowCandidateV0 {
        asset_index,
        domain_index_or_none: domain_index,
        authorization_slot_or_none: authorization_slot,
        intent_local_term_index_or_none: local_term,
        authority_class: AUTHORITY_EXACT_EXTERNAL_CREDIT,
        fee_shard_index_or_none: NONE_INDEX,
        fee_class: FEE_CLASS_NONE,
        flags: 0,
        rights_bits: RIGHT_CREDIT | RIGHT_EXACT_EXTERNAL_RECIPIENT,
        domain_accounting_slot_or_none: NONE_INDEX,
        spend_authority_control_offset_or_none: NONE_INDEX,
        reserved_0: 0,
        maximum_engine_debit: 0,
        maximum_total_debit: 0,
        minimum_credit: 0,
        maximum_protocol_fee: 0,
    }
}

fn fee_vault(asset_index: u8, shard_index: u8) -> SettlementCapabilityRowCandidateV0 {
    SettlementCapabilityRowCandidateV0 {
        asset_index,
        domain_index_or_none: NONE_INDEX,
        authorization_slot_or_none: NONE_INDEX,
        intent_local_term_index_or_none: NONE_INDEX,
        authority_class: AUTHORITY_CORE_RESERVED_FEE,
        fee_shard_index_or_none: shard_index,
        fee_class: FEE_CLASS_NONE,
        flags: 0,
        rights_bits: RIGHT_CREDIT | RIGHT_CORE_RESERVED_FEE,
        domain_accounting_slot_or_none: NONE_INDEX,
        spend_authority_control_offset_or_none: NONE_INDEX,
        reserved_0: 0,
        maximum_engine_debit: 0,
        maximum_total_debit: 0,
        minimum_credit: 0,
        maximum_protocol_fee: 0,
    }
}

fn intent_term(
    declaration: SettlementCapabilityRowCandidateV0,
    endpoint: anchor_lang::prelude::Pubkey,
    asset_binding_digest: [u8; 32],
    domain_descriptor_digest: [u8; 32],
) -> IntentCapabilityTermRowCandidateV0 {
    IntentCapabilityTermRowCandidateV0 {
        intent_local_term_index: declaration.intent_local_term_index_or_none,
        authority_class: declaration.authority_class,
        fee_class: declaration.fee_class,
        flags: declaration.flags,
        rights_bits: declaration.rights_bits,
        endpoint_key: endpoint.to_bytes(),
        asset_binding_digest,
        required_domain_descriptor_digest_or_zero: domain_descriptor_digest,
        maximum_engine_debit: declaration.maximum_engine_debit,
        maximum_total_debit: declaration.maximum_total_debit,
        minimum_credit: declaration.minimum_credit,
        maximum_protocol_fee: declaration.maximum_protocol_fee,
    }
}

fn credit_constraint(
    numerator: u64,
    denominator: u64,
    terminal_minimum: u64,
) -> CreditConstraintRowCandidateV0 {
    CreditConstraintRowCandidateV0 {
        constraint_index: 0,
        credit_local_term_index: 1,
        flags: 0,
        debit_source_bitmap: 0b1,
        debit_group_root: compute_intent_debit_group_root(&[0])
            .expect("derive the resource debit group root"),
        minimum_credit_numerator: numerator,
        nonzero_debit_denominator: denominator,
        terminal_absolute_minimum: terminal_minimum,
    }
}

fn authorization_view(
    slot: u8,
    authorization: anchor_lang::prelude::Pubkey,
    state: &StoredAuthorizationCandidateV0,
) -> AuthorizationViewRowCandidateV0 {
    AuthorizationViewRowCandidateV0 {
        authorization_slot: slot,
        intent_digest: state.identity.intent_digest,
        authorization_state_digest: compute_authorization_state_digest(
            AuthorizationStateDigestInputs {
                intent_digest: &state.identity.intent_digest,
                lifecycle: AUTHORIZATION_LIFECYCLE_ACTIVE,
                fill_sequence: state.fill_sequence,
                successful_fills: state.fill_sequence,
                remaining_fills: state.identity.max_fills - state.fill_sequence,
                capability_state_root: &state
                    .capability_state_root()
                    .expect("derive resource authorization capability state"),
                fee_state_root: &state
                    .fee_state_root()
                    .expect("derive resource authorization fee state"),
                stored_authorization_key_or_zero: &authorization.to_bytes(),
            },
        )
        .expect("derive a resource authorization state digest"),
    }
}

fn capability_domain(index: u8, domain: ResourceDomain) -> DomainCapabilityIdentity {
    DomainCapabilityIdentity {
        domain_index: index,
        domain_descriptor: domain.descriptor,
        domain_revision: domain.revision,
        admission_digest: domain.admission_digest,
        accounting_slot: NONE_INDEX,
    }
}

fn asset_identity(mint: anchor_lang::prelude::Pubkey, profile: [u8; 32]) -> AssetProfileIdentity {
    AssetProfileIdentity {
        asset_identity: mint,
        asset_program: litesvm_token::TOKEN_ID,
        settlement_profile_digest: profile,
    }
}

#[allow(clippy::too_many_arguments)]
fn protected_capability(
    position: u8,
    declaration: SettlementCapabilityRowCandidateV0,
    market: anchor_lang::prelude::Pubkey,
    endpoint: ClassicSplEndpointSnapshot,
    transfer_authority_or_zero: anchor_lang::prelude::Pubkey,
    asset: AssetProfileIdentity,
    domain: Option<DomainCapabilityIdentity>,
    fee_policy_revision: u64,
) -> SettlementCapability {
    SettlementCapability {
        position,
        declaration,
        core_program: programmable_generic_effect_core::ID,
        experimental_major: EXPERIMENTAL_MAJOR,
        market,
        endpoint: token_effective_privilege(endpoint.key),
        transfer_authority_or_zero,
        asset,
        domain,
        fee_policy_revision,
        lifecycle_digest: endpoint
            .lifecycle_digest()
            .expect("derive a resource endpoint lifecycle digest"),
        accounted_before_or_zero: 0,
    }
}

fn resource_fee_shard_set_digest(
    direct: &DirectFixture,
    descriptors: [anchor_lang::prelude::Pubkey; 2],
    liabilities: [anchor_lang::prelude::Pubkey; 2],
    vaults: [anchor_lang::prelude::Pubkey; 2],
    asset_binding_digests: [[u8; 32]; 2],
) -> [u8; 32] {
    let rows = (0..2)
        .map(|index| {
            let descriptor: FeeShardDescriptorCandidateV0 =
                read_anchor_account(&direct.svm, &descriptors[index]);
            let liability: FeeLiabilityLedgerCandidateV0 =
                read_anchor_account(&direct.svm, &liabilities[index]);
            FeeShardDigestRowCandidateV0 {
                shard_index: index as u8,
                asset_index: index as u8,
                vault_settlement_capability_index: 4 + index as u8,
                flags: 0,
                descriptor_key: descriptors[index].to_bytes(),
                descriptor_digest: descriptor.descriptor_digest,
                liability_key: liabilities[index].to_bytes(),
                vault_key: vaults[index].to_bytes(),
                asset_binding_digest: asset_binding_digests[index],
                fee_policy_digest: descriptor.fee_policy_digest,
                recipient_policy_digest: descriptor.recipient_policy_digest,
                fee_policy_revision: descriptor.fee_policy_revision,
                liability_before: liability.liability,
            }
        })
        .collect::<Vec<_>>();
    compute_fee_shard_set_digest(&rows).expect("derive the resource fee-shard set")
}

fn stored_engine_context(
    position: u8,
    declaration: SettlementCapabilityRowCandidateV0,
    endpoint: ClassicSplEndpointSnapshot,
    state: &StoredAuthorizationCandidateV0,
) -> EngineContextRowCandidateV0 {
    let bound = state.capabilities[usize::from(declaration.intent_local_term_index_or_none)];
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
        remaining_maximum_engine_debit: bound.initial_maximum_engine_debit,
        remaining_maximum_total_debit: bound.remaining_total_debit,
        remaining_minimum_credit: bound.initial_minimum_credit,
        remaining_maximum_protocol_fee: declaration.maximum_protocol_fee,
    }
}

fn asset_binding_from_engine(row: EngineAssetRowCandidateV0) -> AssetBindingRowCandidateV0 {
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

fn opaque_root(svm: &LiteSVM, metas: &[AccountMeta]) -> [u8; 32] {
    let descriptors = metas
        .iter()
        .enumerate()
        .map(|(position, meta)| {
            let account = svm
                .get_account(&meta.pubkey)
                .unwrap_or_else(|| panic!("resource opaque account {} exists", meta.pubkey));
            OpaqueCapabilityDescriptorCandidateV0 {
                position: u8::try_from(position).expect("opaque position fits u8"),
                key: meta.pubkey.to_bytes(),
                owner: account.owner.to_bytes(),
                executable: account.executable,
                effective_signer: meta.is_signer,
                effective_writable: meta.is_writable,
            }
        })
        .collect::<Vec<_>>();
    compute_opaque_capability_root(&descriptors).expect("derive the resource opaque root")
}

fn bind_helper_state(
    svm: &mut LiteSVM,
    address: anchor_lang::prelude::Pubkey,
    callback: anchor_lang::prelude::Pubkey,
) {
    let mut account = svm
        .get_account(&address)
        .expect("resource helper state exists before callback binding");
    let state = callback_capability_probe::HelperState {
        allowed_callback: callback,
        calls: 0,
        value: 0,
        descendant_receipt_sets: 0,
    };
    let mut data = Vec::with_capacity(account.data.len());
    state
        .try_serialize(&mut data)
        .expect("serialize callback-bound resource helper state");
    data.resize(account.data.len(), 0);
    account.data = data;
    svm.set_account(address, account)
        .expect("bind the resource helper state to the callback");
}

fn forward_exact_once(core: &Instruction) -> Instruction {
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
                core_account_count: u8::try_from(core.accounts.len())
                    .expect("resource Core account position count fits u8"),
                mode: hostile_router_probe::RouterMode::ForwardExactOnce,
                core_instruction_data: core.data.clone(),
            },
        }
        .data(),
    }
}
