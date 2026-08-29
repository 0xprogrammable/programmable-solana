use anchor_lang::solana_program::program_option::COption;
use generic_effect_private_wire::{
    compute_asset_set_digest, compute_authorization_capability_state_root,
    compute_authorization_fee_state_root, compute_authorization_state_digest,
    compute_authorization_view_set_digest, compute_domain_set_digest,
    compute_exact_fee_recipient_policy_digest, compute_fee_policy_digest,
    compute_fee_shard_set_digest, compute_intent_capability_terms_root,
    compute_intent_core_terms_root, compute_intent_credit_constraints_root, compute_intent_digest,
    compute_intent_set_digest, compute_opaque_capability_root, compute_payload_digest,
    compute_protected_execution_root, derive_callback_authority_for_engine,
    AssetBindingRowCandidateV0, AuthorizationCapabilityStateRowCandidateV0,
    AuthorizationSnapshotRowCandidateV0, AuthorizationStateDigestInputs,
    AuthorizationViewRowCandidateV0, EngineAssetRowCandidateV0, EngineContextRowCandidateV0,
    EngineFeePolicyRowCandidateV0, EngineIntentRowCandidateV0, EngineRequestCandidateV0,
    EngineRequestHeaderCandidateV0, ExecuteEnvelopeCandidateV0, ExecuteEnvelopeHeaderCandidateV0,
    FeeShardDigestRowCandidateV0, FeeShardRowCandidateV0, InlineIntentIdentityRowCandidateV0,
    IntentCapabilityTermRowCandidateV0, IntentCoreTermsDigestInputs, IntentDigestInputs,
    IntentSetRowCandidateV0, MarketBindingRowCandidateV0, OpaqueCapabilityDescriptorCandidateV0,
    ProtectedExecutionRootInputs, SettlementCapabilityRowCandidateV0, AUTHORITY_CORE_RESERVED_FEE,
    AUTHORITY_EXACT_EXTERNAL_CREDIT, AUTHORITY_INTENT_FUNDED, AUTHORIZATION_LIFECYCLE_ACTIVE,
    ENGINE_REQUEST_MAGIC, FEE_CLASS_GROSS_DEBIT_RATE, FEE_CLASS_NONE, NONE_INDEX, PHASE_TRANSITION,
    RIGHT_CORE_RESERVED_FEE, RIGHT_CREDIT, RIGHT_DEBIT, RIGHT_EXACT_EXTERNAL_RECIPIENT,
    SETTLEMENT_FLAG_FEE_FUNDING, WIRE_VERSION, WITNESS_DIRECT_ACTOR, WITNESS_EXACT_DELEGATE,
};
use litesvm::LiteSVM;
use litesvm_token::Approve;
use solana_address_lookup_table_interface::state::AddressLookupTable;
use solana_clock::Clock;
use solana_keypair::Keypair;
use solana_loader_v3_interface::{get_program_data_address, state::UpgradeableLoaderState};
use solana_message::{AccountMeta, Instruction};
use solana_native_token::LAMPORTS_PER_SOL;
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

use programmable_generic_effect_core::{
    account_segments::EffectivePrivilege,
    authorization::derive_exact_spend_authority,
    capabilities::{
        validate_settlement_capabilities, AssetProfileIdentity, CapabilityValidationContext,
        SettlementCapability,
    },
    constants::{
        EXPERIMENTAL_MAJOR, POLICY_IMMUTABLE_DEPLOYMENT, POLICY_MUTABLE_CONTROLLER_RISK,
        ROUND_FLOOR,
    },
    engine_identity::{EngineAdmissionPolicyCandidateV0, EngineLoaderStateSnapshotCandidateV0},
    state::{
        CoreConfigurationCandidateV0, FeeLiabilityLedgerCandidateV0, FeePolicyCandidateV0,
        FeeShardDescriptorCandidateV0, ImmutableEngineReleaseCandidateV0,
        MarketDescriptorCandidateV0,
    },
    token_settlement::ClassicSplEndpointSnapshot,
};

use super::{
    advance_one_slot, build_core_execute_instruction, compile_v0_transaction_with_signers,
    deploy_fixed_id_mutable_program, fixture_keypair, install_anchor_account, install_fixture_mint,
    install_lookup_table, loader_v3_test_vm, lookup_candidates, mint_tokens, read_anchor_account,
    read_program_data_state, request_heap_frame_instruction, set_compute_unit_limit_instruction,
    token_state, CoreExecuteAccountClosure, SbfArtifacts, V0MessageResources,
    CONTROLLED_COMPUTE_UNIT_LIMIT, CONTROLLED_HEAP_FRAME_BYTES,
};

pub const DIRECT_SOURCE_BALANCE: u64 = 1_000_000;
pub const DIRECT_DEFAULT_AMOUNT: u64 = 37_000;
pub const DIRECT_FEE_RATE_NUMERATOR: u64 = 3;
pub const DIRECT_FEE_RATE_DENOMINATOR: u64 = 1_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EngineFixtureMode {
    CachedImmutable,
    LoaderV3Mutable,
    LoaderV3MutableSameSlot,
    LoaderV3PinnedRejected,
}

/// The complete first executable graph. Every commitment is reconstructed
/// from the same deterministic state that is installed in `svm`; no fixture
/// digest is a hand-written preimage.
pub struct DirectFixture {
    pub svm: LiteSVM,
    pub payer: Keypair,
    pub actor: Keypair,
    pub recipient_owner: Keypair,
    pub configuration: anchor_lang::prelude::Pubkey,
    pub market: anchor_lang::prelude::Pubkey,
    pub fee_policy: anchor_lang::prelude::Pubkey,
    pub fee_shard_descriptor: anchor_lang::prelude::Pubkey,
    pub fee_liability: anchor_lang::prelude::Pubkey,
    pub loader_policy_account: anchor_lang::prelude::Pubkey,
    pub engine_program_data: anchor_lang::prelude::Pubkey,
    pub engine_controller: Option<anchor_lang::prelude::Pubkey>,
    pub engine_programdata_slot: u64,
    pub mint: anchor_lang::prelude::Pubkey,
    pub source: anchor_lang::prelude::Pubkey,
    pub destination: anchor_lang::prelude::Pubkey,
    pub fee_vault: anchor_lang::prelude::Pubkey,
    pub callback_authority: anchor_lang::prelude::Pubkey,
    pub spend_authority: Option<anchor_lang::prelude::Pubkey>,
    pub envelope: ExecuteEnvelopeCandidateV0,
    pub engine_request: EngineRequestCandidateV0,
    pub instruction: Instruction,
    pub maximum_engine_debit: u64,
    pub maximum_protocol_fee: u64,
    pub transfer_amount: u64,
    pub protocol_fee: u64,
}

impl DirectFixture {
    pub fn accepted(artifacts: &SbfArtifacts, transfer_amount: u64) -> Self {
        let payload = effect_engine_probe::plan::encode_explicit_plan(
            effect_engine_probe::plan::RECEIPT_ACCEPT,
            0,
            NONE_INDEX,
            NONE_INDEX,
            &[effect_engine_probe::plan::PlannedMove {
                source_capability_index: 0,
                destination_capability_index: 1,
                amount: transfer_amount,
            }],
        )
        .expect("encode canonical direct engine plan");
        Self::with_payload(artifacts, transfer_amount, payload)
    }

    /// The one deliberately admitted no-effect shape: one on-curve,
    /// transaction-root DIRECT witness and a canonical empty Move list. The
    /// nonzero FeePolicy and fee-funded capability remain authenticated, but
    /// zero gross debit means Core must derive no fee.
    pub fn state_only(artifacts: &SbfArtifacts) -> Self {
        let payload = effect_engine_probe::plan::encode_explicit_plan(
            effect_engine_probe::plan::RECEIPT_ACCEPT,
            0,
            NONE_INDEX,
            NONE_INDEX,
            &[],
        )
        .expect("encode canonical state-only engine plan");
        Self::with_payload_and_limits(
            artifacts,
            0,
            DIRECT_DEFAULT_AMOUNT,
            0,
            payload,
            EngineFixtureMode::CachedImmutable,
        )
    }

    /// Same canonical no-effect graph, but the primary engine is installed by
    /// real loader-v3 deployment and admitted under explicit mutable-controller
    /// risk. This is the minimal execution probe for loader snapshot liveness.
    pub fn state_only_mutable(artifacts: &SbfArtifacts) -> Self {
        Self::state_only_with_engine_mode(artifacts, EngineFixtureMode::LoaderV3Mutable)
    }

    pub fn state_only_mutable_same_slot(artifacts: &SbfArtifacts) -> Self {
        Self::state_only_with_engine_mode(artifacts, EngineFixtureMode::LoaderV3MutableSameSlot)
    }

    pub fn state_only_pinned_mutable(artifacts: &SbfArtifacts) -> Self {
        Self::state_only_with_engine_mode(artifacts, EngineFixtureMode::LoaderV3PinnedRejected)
    }

    fn state_only_with_engine_mode(
        artifacts: &SbfArtifacts,
        engine_mode: EngineFixtureMode,
    ) -> Self {
        let payload = effect_engine_probe::plan::encode_explicit_plan(
            effect_engine_probe::plan::RECEIPT_ACCEPT,
            0,
            NONE_INDEX,
            NONE_INDEX,
            &[],
        )
        .expect("encode canonical mutable state-only engine plan");
        Self::with_payload_and_limits(artifacts, 0, DIRECT_DEFAULT_AMOUNT, 0, payload, engine_mode)
    }

    pub fn with_receipt_mode(
        artifacts: &SbfArtifacts,
        transfer_amount: u64,
        receipt_mode: u8,
    ) -> Self {
        let payload = effect_engine_probe::plan::encode_explicit_plan(
            receipt_mode,
            0,
            NONE_INDEX,
            NONE_INDEX,
            &[effect_engine_probe::plan::PlannedMove {
                source_capability_index: 0,
                destination_capability_index: 1,
                amount: transfer_amount,
            }],
        )
        .expect("encode direct engine failure plan");
        Self::with_payload(artifacts, transfer_amount, payload)
    }

    pub fn with_payload(artifacts: &SbfArtifacts, transfer_amount: u64, payload: Vec<u8>) -> Self {
        Self::with_payload_and_limits(
            artifacts,
            transfer_amount,
            transfer_amount,
            transfer_amount,
            payload,
            EngineFixtureMode::CachedImmutable,
        )
    }

    fn with_payload_and_limits(
        artifacts: &SbfArtifacts,
        transfer_amount: u64,
        maximum_engine_debit: u64,
        minimum_credit: u64,
        payload: Vec<u8>,
        engine_mode: EngineFixtureMode,
    ) -> Self {
        assert!(maximum_engine_debit != 0 && maximum_engine_debit <= DIRECT_SOURCE_BALANCE);
        assert!(transfer_amount <= maximum_engine_debit);
        assert!(minimum_credit <= maximum_engine_debit);
        let payer = fixture_keypair(20);
        let actor = fixture_keypair(21);
        let recipient_owner = fixture_keypair(22);
        let mut svm = match engine_mode {
            EngineFixtureMode::CachedImmutable => {
                let mut svm = LiteSVM::new();
                artifacts.install_cached_programs(&mut svm);
                svm.airdrop(&payer.pubkey(), 100 * LAMPORTS_PER_SOL)
                    .expect("fund immutable direct fixture payer");
                svm
            }
            EngineFixtureMode::LoaderV3Mutable
            | EngineFixtureMode::LoaderV3MutableSameSlot
            | EngineFixtureMode::LoaderV3PinnedRejected => {
                let mut svm = loader_v3_test_vm();
                svm.add_program(callback_capability_probe::ID, &artifacts.helper)
                    .expect("install callback fixture beside mutable Engine");
                svm.add_program(hostile_router_probe::ID, &artifacts.router)
                    .expect("install router fixture beside mutable Engine");
                svm.add_program(programmable_generic_effect_core::ID, &artifacts.core)
                    .expect("install exact Core beside mutable Engine");
                svm.airdrop(&payer.pubkey(), 50_000 * LAMPORTS_PER_SOL)
                    .expect("fund real mutable Engine deployment payer/controller");
                deploy_fixed_id_mutable_program(
                    &mut svm,
                    &payer,
                    effect_engine_probe::ID,
                    &artifacts.engine,
                    artifacts
                        .engine
                        .len()
                        .max(artifacts.replacement_engine.len()),
                    29,
                );
                svm
            }
        };
        svm.airdrop(&actor.pubkey(), LAMPORTS_PER_SOL)
            .expect("install direct actor as a real readonly signer account");

        let configuration = fixture_keypair(23).pubkey();
        let market = fixture_keypair(24).pubkey();
        let fee_policy = fixture_keypair(25).pubkey();
        let mint = install_fixture_mint(&mut svm, 26, payer.pubkey(), 6);
        let source = super::create_token_account(&mut svm, &payer, &mint, &actor.pubkey());
        let destination =
            super::create_token_account(&mut svm, &payer, &mint, &recipient_owner.pubkey());
        let fee_vault = super::create_token_account(&mut svm, &payer, &mint, &payer.pubkey());
        mint_tokens(&mut svm, &payer, &mint, &source, DIRECT_SOURCE_BALANCE);

        let engine_program = effect_engine_probe::ID;
        let loader_program = solana_sdk_ids::bpf_loader_upgradeable::id();
        let canonical_program_data = get_program_data_address(&engine_program);
        let program_data_account = svm
            .get_account(&canonical_program_data)
            .expect("LiteSVM cached engine has canonical ProgramData");
        let (captured_programdata_slot, observed_controller) =
            match read_program_data_state(&svm, &canonical_program_data) {
                UpgradeableLoaderState::ProgramData {
                    slot,
                    upgrade_authority_address,
                } => (slot, upgrade_authority_address),
                other => panic!("cached engine has unexpected ProgramData state: {other:?}"),
            };
        match engine_mode {
            EngineFixtureMode::CachedImmutable => assert_eq!(
                observed_controller, None,
                "cached engine fixture must be immutable"
            ),
            EngineFixtureMode::LoaderV3Mutable
            | EngineFixtureMode::LoaderV3MutableSameSlot
            | EngineFixtureMode::LoaderV3PinnedRejected => assert_eq!(
                observed_controller,
                Some(payer.pubkey()),
                "real mutable Engine must retain the fixture controller"
            ),
        }
        let captured_programdata_data_len =
            u64::try_from(program_data_account.data.len()).expect("ProgramData length fits u64");
        if engine_mode != EngineFixtureMode::LoaderV3MutableSameSlot {
            advance_one_slot(&mut svm);
            assert!(
                svm.get_sysvar::<Clock>().slot > captured_programdata_slot,
                "loader state cannot be consumed in its landing slot"
            );
        } else {
            assert_eq!(
                svm.get_sysvar::<Clock>().slot,
                captured_programdata_slot,
                "same-slot fixture drifted after deployment"
            );
        }

        let engine_interface_id = [0x31; 32];
        let engine_instance_id = [0x32; 32];
        let protected_profile_digest = [0x33; 32];
        let domain_admission_profile_digest = [0x34; 32];
        let opaque_schema_digest = [0x35; 32];

        let admission_policy = EngineAdmissionPolicyCandidateV0 {
            policy_kind: match engine_mode {
                EngineFixtureMode::CachedImmutable => POLICY_IMMUTABLE_DEPLOYMENT,
                EngineFixtureMode::LoaderV3Mutable | EngineFixtureMode::LoaderV3MutableSameSlot => {
                    POLICY_MUTABLE_CONTROLLER_RISK
                }
                EngineFixtureMode::LoaderV3PinnedRejected => {
                    programmable_generic_effect_core::constants::POLICY_PINNED_MUTABLE_DEPLOYMENT
                }
            },
            engine_program,
            loader_program,
            program_data_or_zero: canonical_program_data,
            expected_controller_or_zero: observed_controller.unwrap_or_default(),
            captured_programdata_slot_or_zero: match engine_mode {
                EngineFixtureMode::CachedImmutable => captured_programdata_slot,
                EngineFixtureMode::LoaderV3Mutable | EngineFixtureMode::LoaderV3MutableSameSlot => {
                    0
                }
                EngineFixtureMode::LoaderV3PinnedRejected => captured_programdata_slot,
            },
        };
        let engine_admission_policy_digest = admission_policy
            .digest()
            .expect("compute exact engine admission digest");
        let loader_snapshot = EngineLoaderStateSnapshotCandidateV0 {
            engine_program,
            loader_program,
            program_data_or_zero: canonical_program_data,
            observed_programdata_slot: captured_programdata_slot,
            observed_controller_or_zero: observed_controller.unwrap_or_default(),
        };
        let loader_state_snapshot_digest = loader_snapshot
            .digest()
            .expect("compute exact loader snapshot digest");

        let fee_policy_row = EngineFeePolicyRowCandidateV0 {
            wire_version: WIRE_VERSION,
            rounding_mode: ROUND_FLOOR,
            flags: 0,
            revision: 1,
            rate_numerator: DIRECT_FEE_RATE_NUMERATOR,
            nonzero_denominator: DIRECT_FEE_RATE_DENOMINATOR,
        };
        let core_program_bytes = programmable_generic_effect_core::ID.to_bytes();
        let fee_policy_digest = compute_fee_policy_digest(&core_program_bytes, &fee_policy_row)
            .expect("compute nonzero fee policy digest");
        let fee_policy_state = FeePolicyCandidateV0 {
            wire_version: WIRE_VERSION,
            rounding_mode: ROUND_FLOOR,
            reserved: [0; 6],
            policy_digest: fee_policy_digest,
            revision: fee_policy_row.revision,
            rate: fee_policy_row.rate_numerator,
            denominator: fee_policy_row.nonzero_denominator,
            fixed_fee_disabled: 0,
        };

        let market_binding_row = MarketBindingRowCandidateV0 {
            core_program: core_program_bytes,
            core_experimental_major: EXPERIMENTAL_MAJOR,
            market_descriptor_key: market.to_bytes(),
            market_descriptor_revision: 1,
            engine_program: engine_program.to_bytes(),
            engine_interface_id,
            engine_instance_id,
            engine_admission_policy_digest,
            domain_admission_profile_digest,
            protected_profile_digest,
            fee_policy_digest,
            opaque_schema_digest,
        };
        let market_binding_digest = market_binding_row
            .digest()
            .expect("compute market binding digest");
        let (fee_shard_descriptor, fee_shard_bump) = FeeShardDescriptorCandidateV0::address(
            &programmable_generic_effect_core::ID,
            &market_binding_digest,
            0,
        );
        let (fee_liability, fee_liability_bump) = FeeLiabilityLedgerCandidateV0::address(
            &programmable_generic_effect_core::ID,
            &fee_shard_descriptor,
            &market_binding_digest,
        );
        let fee_recipient_policy_digest = compute_exact_fee_recipient_policy_digest(
            &core_program_bytes,
            &market_binding_digest,
            &fee_vault.to_bytes(),
            &mint.to_bytes(),
            &litesvm_token::TOKEN_ID.to_bytes(),
            &protected_profile_digest,
        )
        .expect("compute exact fee-recipient policy digest");
        let mut fee_shard_state = FeeShardDescriptorCandidateV0 {
            wire_version: WIRE_VERSION,
            shard_index: 0,
            bump: fee_shard_bump,
            reserved: [0; 5],
            descriptor_digest: [0; 32],
            market_binding_digest,
            fee_policy_digest,
            fee_policy_revision: fee_policy_row.revision,
            asset_identity: mint,
            asset_program: litesvm_token::TOKEN_ID,
            settlement_profile_digest: protected_profile_digest,
            vault: fee_vault,
            liability_ledger: fee_liability,
            recipient_policy_digest: fee_recipient_policy_digest,
        };
        fee_shard_state.descriptor_digest = fee_shard_state
            .derive_descriptor_digest(&programmable_generic_effect_core::ID)
            .expect("derive canonical fee-shard descriptor digest");
        let fee_liability_state = FeeLiabilityLedgerCandidateV0 {
            wire_version: WIRE_VERSION,
            shard_index: 0,
            bump: fee_liability_bump,
            reserved: [0; 5],
            descriptor: fee_shard_descriptor,
            market_binding_digest,
            asset_identity: mint,
            settlement_profile_digest: protected_profile_digest,
            liability: 0,
        };
        let market_state = MarketDescriptorCandidateV0 {
            wire_version: WIRE_VERSION,
            experimental_major: EXPERIMENTAL_MAJOR,
            bump: 0,
            reserved: [0; 2],
            market_binding_digest,
            market_descriptor_revision: 1,
            engine_program,
            engine_interface_id,
            engine_instance_id,
            engine_admission_policy_digest,
            protected_profile_digest,
            domain_admission_profile_digest,
            fee_policy_digest,
            fee_policy_revision: fee_policy_row.revision,
            opaque_schema_digest,
        };
        let configuration_state = CoreConfigurationCandidateV0 {
            wire_version: WIRE_VERSION,
            experimental_major: EXPERIMENTAL_MAJOR,
            bump: 0,
            reserved: [0; 2],
            classic_spl_profile_digest: protected_profile_digest,
            supported_engine_interface_digest: engine_interface_id,
            fee_policy_root: fee_policy_digest,
        };

        let (immutable_release, release_bump) = ImmutableEngineReleaseCandidateV0::address(
            &programmable_generic_effect_core::ID,
            &engine_program,
        );
        let release_state = if engine_mode == EngineFixtureMode::CachedImmutable {
            let mut state = ImmutableEngineReleaseCandidateV0 {
                wire_version: WIRE_VERSION,
                bump: release_bump,
                reserved: [0; 6],
                engine_program,
                loader_program,
                canonical_program_data,
                captured_programdata_slot,
                observed_controller_or_zero: Default::default(),
                captured_programdata_data_len,
                engine_admission_policy_digest,
                loader_state_snapshot_digest,
                release_observation_digest: [0; 32],
            };
            state.release_observation_digest = state
                .derive_observation_digest_for_core(&programmable_generic_effect_core::ID)
                .expect("derive canonical immutable release observation");
            Some(state)
        } else {
            None
        };
        let loader_policy_account = release_state
            .as_ref()
            .map_or(canonical_program_data, |_| immutable_release);

        install_anchor_account(
            &mut svm,
            configuration,
            programmable_generic_effect_core::ID,
            &configuration_state,
            CoreConfigurationCandidateV0::SPACE,
        );
        install_anchor_account(
            &mut svm,
            market,
            programmable_generic_effect_core::ID,
            &market_state,
            MarketDescriptorCandidateV0::SPACE,
        );
        install_anchor_account(
            &mut svm,
            fee_policy,
            programmable_generic_effect_core::ID,
            &fee_policy_state,
            FeePolicyCandidateV0::SPACE,
        );
        if let Some(release_state) = &release_state {
            install_anchor_account(
                &mut svm,
                immutable_release,
                programmable_generic_effect_core::ID,
                release_state,
                ImmutableEngineReleaseCandidateV0::SPACE,
            );
        }
        install_anchor_account(
            &mut svm,
            fee_shard_descriptor,
            programmable_generic_effect_core::ID,
            &fee_shard_state,
            FeeShardDescriptorCandidateV0::SPACE,
        );
        install_anchor_account(
            &mut svm,
            fee_liability,
            programmable_generic_effect_core::ID,
            &fee_liability_state,
            FeeLiabilityLedgerCandidateV0::SPACE,
        );

        let source_before = endpoint_snapshot(&svm, source);
        let destination_before = endpoint_snapshot(&svm, destination);
        let fee_vault_before = endpoint_snapshot(&svm, fee_vault);
        let maximum_protocol_fee = u64::try_from(
            u128::from(maximum_engine_debit)
                .checked_mul(u128::from(DIRECT_FEE_RATE_NUMERATOR))
                .expect("direct maximum fee numerator multiplication")
                / u128::from(DIRECT_FEE_RATE_DENOMINATOR),
        )
        .expect("direct maximum protocol fee fits u64");
        assert!(
            maximum_protocol_fee > 0,
            "direct fee-funded capability requires a nonzero maximum fee"
        );
        let protocol_fee = u64::try_from(
            u128::from(transfer_amount)
                .checked_mul(u128::from(DIRECT_FEE_RATE_NUMERATOR))
                .expect("direct actual fee numerator multiplication")
                / u128::from(DIRECT_FEE_RATE_DENOMINATOR),
        )
        .expect("direct actual protocol fee fits u64");
        let asset_binding = AssetBindingRowCandidateV0 {
            wire_version: WIRE_VERSION,
            flags: 0,
            decimals: 6,
            reserved: 0,
            asset_identity: mint.to_bytes(),
            asset_program: litesvm_token::TOKEN_ID.to_bytes(),
            settlement_profile_digest: protected_profile_digest,
        };
        let asset_binding_digest = asset_binding.digest().expect("asset binding digest");
        let asset_set_digest =
            compute_asset_set_digest(&[asset_binding]).expect("asset set digest");

        let declarations = direct_settlement_declarations(
            maximum_engine_debit,
            minimum_credit,
            maximum_protocol_fee,
        );
        let asset = AssetProfileIdentity {
            asset_identity: mint,
            asset_program: litesvm_token::TOKEN_ID,
            settlement_profile_digest: protected_profile_digest,
        };
        let protected_capability_set_digest = validate_settlement_capabilities(
            &[
                SettlementCapability {
                    position: 0,
                    declaration: declarations[0],
                    core_program: programmable_generic_effect_core::ID,
                    experimental_major: EXPERIMENTAL_MAJOR,
                    market,
                    endpoint: token_effective_privilege(source),
                    transfer_authority_or_zero: actor.pubkey(),
                    asset,
                    domain: None,
                    fee_policy_revision: fee_policy_row.revision,
                    lifecycle_digest: source_before
                        .lifecycle_digest()
                        .expect("source lifecycle digest"),
                    accounted_before_or_zero: 0,
                },
                SettlementCapability {
                    position: 1,
                    declaration: declarations[1],
                    core_program: programmable_generic_effect_core::ID,
                    experimental_major: EXPERIMENTAL_MAJOR,
                    market,
                    endpoint: token_effective_privilege(destination),
                    transfer_authority_or_zero: Default::default(),
                    asset,
                    domain: None,
                    fee_policy_revision: fee_policy_row.revision,
                    lifecycle_digest: destination_before
                        .lifecycle_digest()
                        .expect("destination lifecycle digest"),
                    accounted_before_or_zero: 0,
                },
                SettlementCapability {
                    position: 2,
                    declaration: declarations[2],
                    core_program: programmable_generic_effect_core::ID,
                    experimental_major: EXPERIMENTAL_MAJOR,
                    market,
                    endpoint: token_effective_privilege(fee_vault),
                    transfer_authority_or_zero: Default::default(),
                    asset,
                    domain: None,
                    fee_policy_revision: fee_policy_row.revision,
                    lifecycle_digest: fee_vault_before
                        .lifecycle_digest()
                        .expect("fee-vault lifecycle digest"),
                    accounted_before_or_zero: 0,
                },
            ],
            CapabilityValidationContext {
                core_program: programmable_generic_effect_core::ID,
                market,
                classic_token_program: litesvm_token::TOKEN_ID,
                experimental_major: EXPERIMENTAL_MAJOR,
                intent_count: 1,
                asset_count: 1,
                domain_count: 0,
                fee_shard_count: 1,
                fee_policy_revision: fee_policy_row.revision,
            },
        )
        .expect("validate and hash protected direct capabilities");

        let domain_set_digest =
            compute_domain_set_digest(&market_binding_digest, &[]).expect("empty domain set");
        let capability_terms = declarations
            .iter()
            .enumerate()
            .filter(|(_, declaration)| declaration.authorization_slot_or_none != NONE_INDEX)
            .map(
                |(position, declaration)| IntentCapabilityTermRowCandidateV0 {
                    intent_local_term_index: declaration.intent_local_term_index_or_none,
                    authority_class: declaration.authority_class,
                    fee_class: declaration.fee_class,
                    flags: declaration.flags,
                    rights_bits: declaration.rights_bits,
                    endpoint_key: match position {
                        0 => source.to_bytes(),
                        1 => destination.to_bytes(),
                        _ => unreachable!("Core-reserved fee vault is not an intent term"),
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
        let capability_terms_root = compute_intent_capability_terms_root(&capability_terms)
            .expect("direct capability terms root");
        let credit_constraints_root =
            compute_intent_credit_constraints_root(&[]).expect("empty credit constraints root");
        let core_terms_root = compute_intent_core_terms_root(IntentCoreTermsDigestInputs {
            maximum_successful_fills: 1,
            capability_terms_root: &capability_terms_root,
            credit_constraints_root: &credit_constraints_root,
        })
        .expect("direct core terms root");

        let current_slot = svm.get_sysvar::<Clock>().slot;
        let identity = InlineIntentIdentityRowCandidateV0 {
            actor: actor.pubkey().to_bytes(),
            engine_terms_commitment: [0x36; 32],
            authorization_nonce: 1,
            expires_at_slot_exclusive: current_slot + 200,
        };
        let intent_digest = compute_intent_digest(IntentDigestInputs {
            core_program: &core_program_bytes,
            market_binding_digest: &market_binding_digest,
            loader_state_snapshot_digest: &loader_state_snapshot_digest,
            fee_policy_digest: &fee_policy_digest,
            identity: &identity,
            core_terms_root: &core_terms_root,
        })
        .expect("direct intent digest");
        let intent_set_digest = compute_intent_set_digest(
            &domain_set_digest,
            &[IntentSetRowCandidateV0 { intent_digest }],
        )
        .expect("direct intent set digest");

        let capability_state_root = compute_authorization_capability_state_root(&[
            AuthorizationCapabilityStateRowCandidateV0 {
                local_term_index: 0,
                reserved_0: 0,
                flags: declarations[0].flags,
                initial_maximum_engine_debit: maximum_engine_debit,
                initial_minimum_credit: 0,
                initial_maximum_total_debit: maximum_engine_debit
                    .checked_add(maximum_protocol_fee)
                    .expect("direct total debit ceiling"),
                remaining_total_debit: maximum_engine_debit
                    .checked_add(maximum_protocol_fee)
                    .expect("direct remaining total debit"),
                cumulative_engine_debit: 0,
                cumulative_fee_debit: 0,
                cumulative_credit: 0,
            },
            AuthorizationCapabilityStateRowCandidateV0 {
                local_term_index: 1,
                reserved_0: 0,
                flags: declarations[1].flags,
                initial_maximum_engine_debit: 0,
                initial_minimum_credit: minimum_credit,
                initial_maximum_total_debit: 0,
                remaining_total_debit: 0,
                cumulative_engine_debit: 0,
                cumulative_fee_debit: 0,
                cumulative_credit: 0,
            },
        ])
        .expect("direct authorization capability state root");
        let fee_state_root =
            compute_authorization_fee_state_root(&[]).expect("empty authorization fee state");
        let authorization_state_digest =
            compute_authorization_state_digest(AuthorizationStateDigestInputs {
                intent_digest: &intent_digest,
                lifecycle: AUTHORIZATION_LIFECYCLE_ACTIVE,
                fill_sequence: 0,
                successful_fills: 0,
                remaining_fills: 1,
                capability_state_root: &capability_state_root,
                fee_state_root: &fee_state_root,
                stored_authorization_key_or_zero: &[0; 32],
            })
            .expect("direct authorization state digest");
        let authorization_view_set_digest =
            compute_authorization_view_set_digest(&[AuthorizationViewRowCandidateV0 {
                authorization_slot: 0,
                intent_digest,
                authorization_state_digest,
            }])
            .expect("direct authorization view set");
        let fee_shard_digest_row = FeeShardDigestRowCandidateV0 {
            shard_index: 0,
            asset_index: 0,
            vault_settlement_capability_index: 2,
            flags: 0,
            descriptor_key: fee_shard_descriptor.to_bytes(),
            descriptor_digest: fee_shard_state.descriptor_digest,
            liability_key: fee_liability.to_bytes(),
            vault_key: fee_vault.to_bytes(),
            asset_binding_digest,
            fee_policy_digest,
            recipient_policy_digest: fee_recipient_policy_digest,
            fee_policy_revision: fee_policy_row.revision,
            liability_before: 0,
        };
        let fee_shard_set_digest = compute_fee_shard_set_digest(&[fee_shard_digest_row])
            .expect("single authenticated fee shard set");
        let protected_execution_root =
            compute_protected_execution_root(ProtectedExecutionRootInputs {
                core_program: &core_program_bytes,
                market_binding_digest: &market_binding_digest,
                engine_loader_state_snapshot_digest: &loader_state_snapshot_digest,
                domain_set_digest: &domain_set_digest,
                intent_set_digest: &intent_set_digest,
                fee_policy_digest: &fee_policy_digest,
                asset_set_digest: &asset_set_digest,
                authorization_view_set_digest: &authorization_view_set_digest,
                fee_shard_set_digest: &fee_shard_set_digest,
                protected_capability_set_digest: &protected_capability_set_digest,
            })
            .expect("protected direct execution root");
        let opaque_capability_root =
            compute_opaque_capability_root(&[]).expect("empty opaque capability root");

        let payload_len = u16::try_from(payload.len()).expect("bounded direct payload length");
        let engine_request = EngineRequestCandidateV0 {
            header: EngineRequestHeaderCandidateV0 {
                magic: ENGINE_REQUEST_MAGIC,
                wire_version: WIRE_VERSION,
                phase: PHASE_TRANSITION,
                settlement_capability_count: 3,
                opaque_capability_count: 0,
                intent_count: 1,
                domain_count: 0,
                asset_count: 1,
                context_row_count: 2,
                payload_len,
                maximum_engine_moves: 1,
                market_binding_digest,
                engine_instance_id,
                engine_interface_id,
                intent_set_digest,
                domain_set_digest,
                protected_execution_root,
                opaque_capability_root,
                engine_loader_state_snapshot_digest: loader_state_snapshot_digest,
                fee_policy_digest,
            },
            assets: vec![EngineAssetRowCandidateV0 {
                asset_index: 0,
                asset_flags: 0,
                decimals: 6,
                reserved: 0,
                asset_identity: mint.to_bytes(),
                asset_program: litesvm_token::TOKEN_ID.to_bytes(),
                settlement_profile_digest: protected_profile_digest,
            }],
            domains: vec![],
            intents: vec![EngineIntentRowCandidateV0 {
                authorization_slot: 0,
                identity,
                intent_digest,
            }],
            fee_policy: fee_policy_row,
            contexts: vec![
                engine_context(0, declarations[0], source, source_before.amount),
                engine_context(1, declarations[1], destination, destination_before.amount),
            ],
            payload: payload.clone(),
        };
        engine_request
            .validate()
            .expect("fixture request is canonical before deriving callback");
        let callback_authority =
            derive_callback_authority_for_engine(&engine_request, &engine_program)
                .expect("derive request-bound callback authority")
                .0;

        let envelope = ExecuteEnvelopeCandidateV0 {
            header: ExecuteEnvelopeHeaderCandidateV0 {
                wire_version: WIRE_VERSION,
                loader_policy_account_count: 1,
                domain_control_account_count: 0,
                authorization_account_count: 1,
                protected_profile_account_count: 2,
                fee_control_account_count: 2,
                settlement_capability_count: 3,
                opaque_capability_count: 0,
                domain_count: 0,
                intent_count: 1,
                inline_intent_row_count: 1,
                asset_count: 1,
                fee_shard_count: 1,
                authorization_snapshot_row_count: 1,
                maximum_engine_moves: 1,
                flags: 0,
                payload_len,
                expires_at_slot_exclusive: current_slot + 100,
                expected_engine_sequence: 0,
                intent_set_digest,
                domain_set_digest,
                protected_execution_root,
                expected_opaque_capability_root: opaque_capability_root,
                fee_policy_digest,
                expected_engine_loader_state_snapshot_digest: loader_state_snapshot_digest,
                payload_digest: compute_payload_digest(&payload).expect("canonical payload digest"),
            },
            domain_controls: vec![],
            authorization_snapshots: vec![AuthorizationSnapshotRowCandidateV0 {
                authorization_slot: 0,
                witness_kind: WITNESS_DIRECT_ACTOR,
                authorization_control_offset_or_none: 0,
                inline_identity_index_or_none: 0,
                expected_fill_sequence: 0,
            }],
            inline_intent_identities: vec![identity],
            fee_shards: vec![FeeShardRowCandidateV0 {
                descriptor_control_offset: 0,
                liability_control_offset: 1,
                vault_settlement_capability_index: 2,
                asset_index: 0,
                flags: 0,
            }],
            settlement_capabilities: declarations.to_vec(),
            payload,
        };
        let closure = CoreExecuteAccountClosure {
            configuration,
            market,
            fee_policy,
            engine_program,
            callback_authority,
            loader_policy: vec![loader_policy_account],
            domain_controls: vec![],
            authorization_controls: vec![AccountMeta::new_readonly(actor.pubkey(), true)],
            protected_profile: vec![litesvm_token::TOKEN_ID, mint],
            fee_controls: vec![
                AccountMeta::new_readonly(fee_shard_descriptor, false),
                AccountMeta::new(fee_liability, false),
            ],
            settlement: vec![
                AccountMeta::new(source, false),
                AccountMeta::new(destination, false),
                AccountMeta::new(fee_vault, false),
            ],
            opaque: vec![],
        };
        let instruction = build_core_execute_instruction(&envelope, &closure)
            .expect("build canonical direct Core instruction");

        Self {
            svm,
            payer,
            actor,
            recipient_owner,
            configuration,
            market,
            fee_policy,
            fee_shard_descriptor,
            fee_liability,
            loader_policy_account,
            engine_program_data: canonical_program_data,
            engine_controller: observed_controller,
            engine_programdata_slot: captured_programdata_slot,
            mint,
            source,
            destination,
            fee_vault,
            callback_authority,
            spend_authority: None,
            envelope,
            engine_request,
            instruction,
            maximum_engine_debit,
            maximum_protocol_fee,
            transfer_amount,
            protocol_fee,
        }
    }

    /// Convert the already canonical ephemeral intent from a transaction-root
    /// DIRECT witness to its exact one-shot delegate witness. The intent digest
    /// is unchanged because the spend-control position is execution wiring,
    /// not an immutable term. Every digest that commits the changed delegate
    /// lifecycle or effective authority is recomputed from installed state.
    pub fn into_exact_delegate(mut self, delegated_amount: u64) -> Self {
        assert!(
            delegated_amount != 0,
            "an exact delegate cannot approve zero"
        );
        assert!(
            delegated_amount <= DIRECT_SOURCE_BALANCE,
            "delegate amount exceeds fixture source balance"
        );
        let intent = self
            .engine_request
            .intents
            .first()
            .copied()
            .expect("direct fixture has one inline intent");
        let (spend_authority, _) = derive_exact_spend_authority(
            &programmable_generic_effect_core::ID,
            &intent.intent_digest,
            &self.source,
        )
        .expect("derive request-bound exact spend authority");
        Approve::new(
            &mut self.svm,
            &self.payer,
            &spend_authority,
            &self.source,
            delegated_amount,
        )
        .owner(&self.actor)
        .send()
        .expect("approve exact spend PDA through the real classic SPL program");

        let source_declaration = self
            .envelope
            .settlement_capabilities
            .first_mut()
            .expect("direct fixture has a source capability");
        source_declaration.spend_authority_control_offset_or_none = 0;
        let snapshot = self
            .envelope
            .authorization_snapshots
            .first_mut()
            .expect("direct fixture has one authorization snapshot");
        snapshot.witness_kind = WITNESS_EXACT_DELEGATE;
        snapshot.authorization_control_offset_or_none = NONE_INDEX;

        let asset_row = self
            .engine_request
            .assets
            .first()
            .copied()
            .expect("direct fixture has one asset row");
        let asset_binding = AssetBindingRowCandidateV0 {
            wire_version: WIRE_VERSION,
            flags: asset_row.asset_flags,
            decimals: asset_row.decimals,
            reserved: asset_row.reserved,
            asset_identity: asset_row.asset_identity,
            asset_program: asset_row.asset_program,
            settlement_profile_digest: asset_row.settlement_profile_digest,
        };
        let asset_binding_digest = asset_binding.digest().expect("asset binding digest");
        let asset_set_digest =
            compute_asset_set_digest(&[asset_binding]).expect("single asset set digest");
        let asset = AssetProfileIdentity {
            asset_identity: self.mint,
            asset_program: litesvm_token::TOKEN_ID,
            settlement_profile_digest: asset_row.settlement_profile_digest,
        };
        let declarations = &self.envelope.settlement_capabilities;
        let source_before = endpoint_snapshot(&self.svm, self.source);
        let destination_before = endpoint_snapshot(&self.svm, self.destination);
        let fee_vault_before = endpoint_snapshot(&self.svm, self.fee_vault);
        assert_eq!(source_before.delegate, Some(spend_authority));
        assert_eq!(source_before.delegated_amount, delegated_amount);
        let protected_capability_set_digest = validate_settlement_capabilities(
            &[
                SettlementCapability {
                    position: 0,
                    declaration: declarations[0],
                    core_program: programmable_generic_effect_core::ID,
                    experimental_major: EXPERIMENTAL_MAJOR,
                    market: self.market,
                    endpoint: token_effective_privilege(self.source),
                    transfer_authority_or_zero: spend_authority,
                    asset,
                    domain: None,
                    fee_policy_revision: self.engine_request.fee_policy.revision,
                    lifecycle_digest: source_before
                        .lifecycle_digest()
                        .expect("delegated source lifecycle digest"),
                    accounted_before_or_zero: 0,
                },
                SettlementCapability {
                    position: 1,
                    declaration: declarations[1],
                    core_program: programmable_generic_effect_core::ID,
                    experimental_major: EXPERIMENTAL_MAJOR,
                    market: self.market,
                    endpoint: token_effective_privilege(self.destination),
                    transfer_authority_or_zero: Default::default(),
                    asset,
                    domain: None,
                    fee_policy_revision: self.engine_request.fee_policy.revision,
                    lifecycle_digest: destination_before
                        .lifecycle_digest()
                        .expect("destination lifecycle digest"),
                    accounted_before_or_zero: 0,
                },
                SettlementCapability {
                    position: 2,
                    declaration: declarations[2],
                    core_program: programmable_generic_effect_core::ID,
                    experimental_major: EXPERIMENTAL_MAJOR,
                    market: self.market,
                    endpoint: token_effective_privilege(self.fee_vault),
                    transfer_authority_or_zero: Default::default(),
                    asset,
                    domain: None,
                    fee_policy_revision: self.engine_request.fee_policy.revision,
                    lifecycle_digest: fee_vault_before
                        .lifecycle_digest()
                        .expect("fee-vault lifecycle digest"),
                    accounted_before_or_zero: 0,
                },
            ],
            CapabilityValidationContext {
                core_program: programmable_generic_effect_core::ID,
                market: self.market,
                classic_token_program: litesvm_token::TOKEN_ID,
                experimental_major: EXPERIMENTAL_MAJOR,
                intent_count: 1,
                asset_count: 1,
                domain_count: 0,
                fee_shard_count: 1,
                fee_policy_revision: self.engine_request.fee_policy.revision,
            },
        )
        .expect("validate exact-delegate protected capabilities");

        let capability_state_root = compute_authorization_capability_state_root(
            &declarations
                .iter()
                .filter(|row| row.authorization_slot_or_none == 0)
                .map(|row| AuthorizationCapabilityStateRowCandidateV0 {
                    local_term_index: row.intent_local_term_index_or_none,
                    reserved_0: 0,
                    flags: row.flags,
                    initial_maximum_engine_debit: row.maximum_engine_debit,
                    initial_minimum_credit: row.minimum_credit,
                    initial_maximum_total_debit: row.maximum_total_debit,
                    remaining_total_debit: row.maximum_total_debit,
                    cumulative_engine_debit: 0,
                    cumulative_fee_debit: 0,
                    cumulative_credit: 0,
                })
                .collect::<Vec<_>>(),
        )
        .expect("exact-delegate capability state root");
        let fee_state_root =
            compute_authorization_fee_state_root(&[]).expect("empty ephemeral fee state");
        let authorization_state_digest =
            compute_authorization_state_digest(AuthorizationStateDigestInputs {
                intent_digest: &intent.intent_digest,
                lifecycle: AUTHORIZATION_LIFECYCLE_ACTIVE,
                fill_sequence: 0,
                successful_fills: 0,
                remaining_fills: 1,
                capability_state_root: &capability_state_root,
                fee_state_root: &fee_state_root,
                stored_authorization_key_or_zero: &[0; 32],
            })
            .expect("exact-delegate authorization state digest");
        let authorization_view_set_digest =
            compute_authorization_view_set_digest(&[AuthorizationViewRowCandidateV0 {
                authorization_slot: 0,
                intent_digest: intent.intent_digest,
                authorization_state_digest,
            }])
            .expect("exact-delegate authorization view set");

        let descriptor: FeeShardDescriptorCandidateV0 =
            read_anchor_account(&self.svm, &self.fee_shard_descriptor);
        let liability: FeeLiabilityLedgerCandidateV0 =
            read_anchor_account(&self.svm, &self.fee_liability);
        let fee_shard_set_digest = compute_fee_shard_set_digest(&[FeeShardDigestRowCandidateV0 {
            shard_index: 0,
            asset_index: 0,
            vault_settlement_capability_index: 2,
            flags: 0,
            descriptor_key: self.fee_shard_descriptor.to_bytes(),
            descriptor_digest: descriptor.descriptor_digest,
            liability_key: self.fee_liability.to_bytes(),
            vault_key: self.fee_vault.to_bytes(),
            asset_binding_digest,
            fee_policy_digest: descriptor.fee_policy_digest,
            recipient_policy_digest: descriptor.recipient_policy_digest,
            fee_policy_revision: descriptor.fee_policy_revision,
            liability_before: liability.liability,
        }])
        .expect("exact-delegate fee shard set");
        let header = self.engine_request.header;
        let protected_execution_root =
            compute_protected_execution_root(ProtectedExecutionRootInputs {
                core_program: &programmable_generic_effect_core::ID.to_bytes(),
                market_binding_digest: &header.market_binding_digest,
                engine_loader_state_snapshot_digest: &header.engine_loader_state_snapshot_digest,
                domain_set_digest: &header.domain_set_digest,
                intent_set_digest: &header.intent_set_digest,
                fee_policy_digest: &header.fee_policy_digest,
                asset_set_digest: &asset_set_digest,
                authorization_view_set_digest: &authorization_view_set_digest,
                fee_shard_set_digest: &fee_shard_set_digest,
                protected_capability_set_digest: &protected_capability_set_digest,
            })
            .expect("exact-delegate protected execution root");
        self.engine_request.header.protected_execution_root = protected_execution_root;
        self.engine_request
            .validate()
            .expect("exact-delegate request remains canonical");
        let callback_authority =
            derive_callback_authority_for_engine(&self.engine_request, &effect_engine_probe::ID)
                .expect("derive exact-delegate callback authority")
                .0;
        self.callback_authority = callback_authority;
        self.envelope.header.protected_execution_root = protected_execution_root;

        let closure = CoreExecuteAccountClosure {
            configuration: self.configuration,
            market: self.market,
            fee_policy: self.fee_policy,
            engine_program: effect_engine_probe::ID,
            callback_authority,
            loader_policy: vec![self.loader_policy_account],
            domain_controls: vec![],
            authorization_controls: vec![AccountMeta::new_readonly(spend_authority, false)],
            protected_profile: vec![litesvm_token::TOKEN_ID, self.mint],
            fee_controls: vec![
                AccountMeta::new_readonly(self.fee_shard_descriptor, false),
                AccountMeta::new(self.fee_liability, false),
            ],
            settlement: vec![
                AccountMeta::new(self.source, false),
                AccountMeta::new(self.destination, false),
                AccountMeta::new(self.fee_vault, false),
            ],
            opaque: vec![],
        };
        self.instruction = build_core_execute_instruction(&self.envelope, &closure)
            .expect("build canonical exact-delegate Core instruction");
        self.spend_authority = Some(spend_authority);
        self
    }

    /// Rebuild every loader-dependent intent/request commitment after a real
    /// mutable upgrade. The mutable admission policy stays controller-bound
    /// and slot-agnostic; only the exact observed loader snapshot, intent,
    /// callback, and their dependent roots advance.
    pub fn refresh_mutable_loader_snapshot(&mut self) {
        let (observed_slot, observed_controller) =
            match read_program_data_state(&self.svm, &self.engine_program_data) {
                UpgradeableLoaderState::ProgramData {
                    slot,
                    upgrade_authority_address,
                } => (slot, upgrade_authority_address),
                other => panic!("mutable refresh saw unexpected ProgramData: {other:?}"),
            };
        assert_eq!(
            observed_controller, self.engine_controller,
            "slot refresh cannot silently accept a controller change"
        );
        assert!(
            self.svm.get_sysvar::<Clock>().slot > observed_slot,
            "mutable loader refresh requires a later observation slot"
        );
        self.rebuild_mutable_loader_snapshot(
            observed_slot,
            observed_controller.expect("mutable Engine retains controller"),
        );
    }

    /// Pre-commit the next-slot mutable snapshot so a v0 lookup table can be
    /// warmed before a real loader mutation lands in that exact slot. The
    /// caller must prove the predicted slot against the resulting ProgramData
    /// state before sending the already-compiled transaction.
    pub fn prepare_next_slot_mutable_loader_snapshot(&mut self) -> u64 {
        let controller = self
            .engine_controller
            .expect("immutable fixtures cannot prepare a mutable loader snapshot");
        let current_slot = self.svm.get_sysvar::<Clock>().slot;
        let predicted_slot = current_slot
            .checked_add(1)
            .expect("fixture slot prediction overflowed");
        self.rebuild_mutable_loader_snapshot(predicted_slot, controller);
        predicted_slot
    }

    fn rebuild_mutable_loader_snapshot(
        &mut self,
        observed_slot: u64,
        observed_controller: anchor_lang::prelude::Pubkey,
    ) {
        assert!(
            self.engine_controller.is_some(),
            "immutable fixtures cannot rebuild a mutable loader snapshot"
        );
        assert_eq!(
            self.envelope.authorization_snapshots[0].witness_kind, WITNESS_DIRECT_ACTOR,
            "loader refresh helper is intentionally scoped to DIRECT"
        );
        let loader_snapshot = EngineLoaderStateSnapshotCandidateV0 {
            engine_program: effect_engine_probe::ID,
            loader_program: solana_sdk_ids::bpf_loader_upgradeable::id(),
            program_data_or_zero: self.engine_program_data,
            observed_programdata_slot: observed_slot,
            observed_controller_or_zero: observed_controller,
        };
        let loader_state_snapshot_digest = loader_snapshot
            .digest()
            .expect("refreshed loader snapshot digest");

        let asset_row = self.engine_request.assets[0];
        let asset_binding = AssetBindingRowCandidateV0 {
            wire_version: WIRE_VERSION,
            flags: asset_row.asset_flags,
            decimals: asset_row.decimals,
            reserved: asset_row.reserved,
            asset_identity: asset_row.asset_identity,
            asset_program: asset_row.asset_program,
            settlement_profile_digest: asset_row.settlement_profile_digest,
        };
        let asset_binding_digest = asset_binding.digest().expect("asset binding digest");
        let asset_set_digest =
            compute_asset_set_digest(&[asset_binding]).expect("single asset set digest");
        let declarations = &self.envelope.settlement_capabilities;
        let capability_terms = declarations
            .iter()
            .enumerate()
            .filter(|(_, row)| row.authorization_slot_or_none == 0)
            .map(|(position, row)| IntentCapabilityTermRowCandidateV0 {
                intent_local_term_index: row.intent_local_term_index_or_none,
                authority_class: row.authority_class,
                fee_class: row.fee_class,
                flags: row.flags,
                rights_bits: row.rights_bits,
                endpoint_key: match position {
                    0 => self.source.to_bytes(),
                    1 => self.destination.to_bytes(),
                    _ => unreachable!("fee vault is not an intent term"),
                },
                asset_binding_digest,
                required_domain_descriptor_digest_or_zero: [0; 32],
                maximum_engine_debit: row.maximum_engine_debit,
                maximum_total_debit: row.maximum_total_debit,
                minimum_credit: row.minimum_credit,
                maximum_protocol_fee: row.maximum_protocol_fee,
            })
            .collect::<Vec<_>>();
        let capability_terms_root = compute_intent_capability_terms_root(&capability_terms)
            .expect("refreshed capability terms root");
        let credit_constraints_root =
            compute_intent_credit_constraints_root(&[]).expect("empty credit constraints root");
        let core_terms_root = compute_intent_core_terms_root(IntentCoreTermsDigestInputs {
            maximum_successful_fills: 1,
            capability_terms_root: &capability_terms_root,
            credit_constraints_root: &credit_constraints_root,
        })
        .expect("refreshed core terms root");
        let identity = self.envelope.inline_intent_identities[0];
        let intent_digest = compute_intent_digest(IntentDigestInputs {
            core_program: &programmable_generic_effect_core::ID.to_bytes(),
            market_binding_digest: &self.engine_request.header.market_binding_digest,
            loader_state_snapshot_digest: &loader_state_snapshot_digest,
            fee_policy_digest: &self.engine_request.header.fee_policy_digest,
            identity: &identity,
            core_terms_root: &core_terms_root,
        })
        .expect("refreshed mutable intent digest");
        let intent_set_digest = compute_intent_set_digest(
            &self.engine_request.header.domain_set_digest,
            &[IntentSetRowCandidateV0 { intent_digest }],
        )
        .expect("refreshed mutable intent set");

        let capability_state_root = compute_authorization_capability_state_root(
            &declarations
                .iter()
                .filter(|row| row.authorization_slot_or_none == 0)
                .map(|row| AuthorizationCapabilityStateRowCandidateV0 {
                    local_term_index: row.intent_local_term_index_or_none,
                    reserved_0: 0,
                    flags: row.flags,
                    initial_maximum_engine_debit: row.maximum_engine_debit,
                    initial_minimum_credit: row.minimum_credit,
                    initial_maximum_total_debit: row.maximum_total_debit,
                    remaining_total_debit: row.maximum_total_debit,
                    cumulative_engine_debit: 0,
                    cumulative_fee_debit: 0,
                    cumulative_credit: 0,
                })
                .collect::<Vec<_>>(),
        )
        .expect("refreshed capability state root");
        let fee_state_root =
            compute_authorization_fee_state_root(&[]).expect("empty ephemeral fee state");
        let authorization_state_digest =
            compute_authorization_state_digest(AuthorizationStateDigestInputs {
                intent_digest: &intent_digest,
                lifecycle: AUTHORIZATION_LIFECYCLE_ACTIVE,
                fill_sequence: 0,
                successful_fills: 0,
                remaining_fills: 1,
                capability_state_root: &capability_state_root,
                fee_state_root: &fee_state_root,
                stored_authorization_key_or_zero: &[0; 32],
            })
            .expect("refreshed authorization state digest");
        let authorization_view_set_digest =
            compute_authorization_view_set_digest(&[AuthorizationViewRowCandidateV0 {
                authorization_slot: 0,
                intent_digest,
                authorization_state_digest,
            }])
            .expect("refreshed authorization view set");

        let asset = AssetProfileIdentity {
            asset_identity: self.mint,
            asset_program: litesvm_token::TOKEN_ID,
            settlement_profile_digest: asset_row.settlement_profile_digest,
        };
        let source_before = endpoint_snapshot(&self.svm, self.source);
        let destination_before = endpoint_snapshot(&self.svm, self.destination);
        let fee_vault_before = endpoint_snapshot(&self.svm, self.fee_vault);
        let protected_capability_set_digest = validate_settlement_capabilities(
            &[
                SettlementCapability {
                    position: 0,
                    declaration: declarations[0],
                    core_program: programmable_generic_effect_core::ID,
                    experimental_major: EXPERIMENTAL_MAJOR,
                    market: self.market,
                    endpoint: token_effective_privilege(self.source),
                    transfer_authority_or_zero: self.actor.pubkey(),
                    asset,
                    domain: None,
                    fee_policy_revision: self.engine_request.fee_policy.revision,
                    lifecycle_digest: source_before
                        .lifecycle_digest()
                        .expect("refreshed source lifecycle"),
                    accounted_before_or_zero: 0,
                },
                SettlementCapability {
                    position: 1,
                    declaration: declarations[1],
                    core_program: programmable_generic_effect_core::ID,
                    experimental_major: EXPERIMENTAL_MAJOR,
                    market: self.market,
                    endpoint: token_effective_privilege(self.destination),
                    transfer_authority_or_zero: Default::default(),
                    asset,
                    domain: None,
                    fee_policy_revision: self.engine_request.fee_policy.revision,
                    lifecycle_digest: destination_before
                        .lifecycle_digest()
                        .expect("refreshed destination lifecycle"),
                    accounted_before_or_zero: 0,
                },
                SettlementCapability {
                    position: 2,
                    declaration: declarations[2],
                    core_program: programmable_generic_effect_core::ID,
                    experimental_major: EXPERIMENTAL_MAJOR,
                    market: self.market,
                    endpoint: token_effective_privilege(self.fee_vault),
                    transfer_authority_or_zero: Default::default(),
                    asset,
                    domain: None,
                    fee_policy_revision: self.engine_request.fee_policy.revision,
                    lifecycle_digest: fee_vault_before
                        .lifecycle_digest()
                        .expect("refreshed fee-vault lifecycle"),
                    accounted_before_or_zero: 0,
                },
            ],
            CapabilityValidationContext {
                core_program: programmable_generic_effect_core::ID,
                market: self.market,
                classic_token_program: litesvm_token::TOKEN_ID,
                experimental_major: EXPERIMENTAL_MAJOR,
                intent_count: 1,
                asset_count: 1,
                domain_count: 0,
                fee_shard_count: 1,
                fee_policy_revision: self.engine_request.fee_policy.revision,
            },
        )
        .expect("refreshed protected capability set");
        let descriptor: FeeShardDescriptorCandidateV0 =
            read_anchor_account(&self.svm, &self.fee_shard_descriptor);
        let liability: FeeLiabilityLedgerCandidateV0 =
            read_anchor_account(&self.svm, &self.fee_liability);
        let fee_shard_set_digest = compute_fee_shard_set_digest(&[FeeShardDigestRowCandidateV0 {
            shard_index: 0,
            asset_index: 0,
            vault_settlement_capability_index: 2,
            flags: 0,
            descriptor_key: self.fee_shard_descriptor.to_bytes(),
            descriptor_digest: descriptor.descriptor_digest,
            liability_key: self.fee_liability.to_bytes(),
            vault_key: self.fee_vault.to_bytes(),
            asset_binding_digest,
            fee_policy_digest: descriptor.fee_policy_digest,
            recipient_policy_digest: descriptor.recipient_policy_digest,
            fee_policy_revision: descriptor.fee_policy_revision,
            liability_before: liability.liability,
        }])
        .expect("refreshed fee shard set");
        let protected_execution_root =
            compute_protected_execution_root(ProtectedExecutionRootInputs {
                core_program: &programmable_generic_effect_core::ID.to_bytes(),
                market_binding_digest: &self.engine_request.header.market_binding_digest,
                engine_loader_state_snapshot_digest: &loader_state_snapshot_digest,
                domain_set_digest: &self.engine_request.header.domain_set_digest,
                intent_set_digest: &intent_set_digest,
                fee_policy_digest: &self.engine_request.header.fee_policy_digest,
                asset_set_digest: &asset_set_digest,
                authorization_view_set_digest: &authorization_view_set_digest,
                fee_shard_set_digest: &fee_shard_set_digest,
                protected_capability_set_digest: &protected_capability_set_digest,
            })
            .expect("refreshed protected execution root");

        self.engine_request
            .header
            .engine_loader_state_snapshot_digest = loader_state_snapshot_digest;
        self.engine_request.header.intent_set_digest = intent_set_digest;
        self.engine_request.header.protected_execution_root = protected_execution_root;
        self.engine_request.intents[0].intent_digest = intent_digest;
        self.engine_request
            .validate()
            .expect("refreshed mutable request remains canonical");
        self.envelope
            .header
            .expected_engine_loader_state_snapshot_digest = loader_state_snapshot_digest;
        self.envelope.header.intent_set_digest = intent_set_digest;
        self.envelope.header.protected_execution_root = protected_execution_root;
        let callback_authority =
            derive_callback_authority_for_engine(&self.engine_request, &effect_engine_probe::ID)
                .expect("derive refreshed mutable callback authority")
                .0;
        self.callback_authority = callback_authority;
        let closure = CoreExecuteAccountClosure {
            configuration: self.configuration,
            market: self.market,
            fee_policy: self.fee_policy,
            engine_program: effect_engine_probe::ID,
            callback_authority,
            loader_policy: vec![self.loader_policy_account],
            domain_controls: vec![],
            authorization_controls: vec![AccountMeta::new_readonly(self.actor.pubkey(), true)],
            protected_profile: vec![litesvm_token::TOKEN_ID, self.mint],
            fee_controls: vec![
                AccountMeta::new_readonly(self.fee_shard_descriptor, false),
                AccountMeta::new(self.fee_liability, false),
            ],
            settlement: vec![
                AccountMeta::new(self.source, false),
                AccountMeta::new(self.destination, false),
                AccountMeta::new(self.fee_vault, false),
            ],
            opaque: vec![],
        };
        self.instruction = build_core_execute_instruction(&self.envelope, &closure)
            .expect("build refreshed mutable Core instruction");
        self.engine_programdata_slot = observed_slot;
    }

    /// Add one canonically committed opaque account. Hostile alias tests use
    /// this to prove virtual and physical protected identities are rejected by
    /// Core before the engine receives any AccountInfo.
    pub fn append_opaque_account(&mut self, key: anchor_lang::prelude::Pubkey, writable: bool) {
        let meta = if writable {
            AccountMeta::new(key, false)
        } else {
            AccountMeta::new_readonly(key, false)
        };
        self.rebuild_payload_and_opaque(self.envelope.payload.clone(), vec![meta]);
    }

    /// Canonically rebuild the untrusted payload and arbitrary ordered opaque
    /// tail. Account descriptors come from the VM's real owner/executable
    /// state plus the requested message privileges; no digest preimage is
    /// accepted from the caller.
    pub fn rebuild_payload_and_opaque(&mut self, payload: Vec<u8>, opaque: Vec<AccountMeta>) {
        let opaque_count = u8::try_from(opaque.len()).expect("opaque fixture count fits u8");
        let descriptors = opaque
            .iter()
            .enumerate()
            .map(|(position, meta)| {
                let account = self
                    .svm
                    .get_account(&meta.pubkey)
                    .unwrap_or_else(|| panic!("opaque fixture account {} is absent", meta.pubkey));
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
        let opaque_root =
            compute_opaque_capability_root(&descriptors).expect("ordered opaque capability root");
        let payload_len = u16::try_from(payload.len()).expect("bounded opaque payload length");
        self.envelope.header.opaque_capability_count = opaque_count;
        self.envelope.header.expected_opaque_capability_root = opaque_root;
        self.envelope.header.payload_len = payload_len;
        self.envelope.header.payload_digest =
            compute_payload_digest(&payload).expect("rebuilt payload digest");
        self.envelope.payload = payload.clone();
        self.engine_request.header.opaque_capability_count = opaque_count;
        self.engine_request.header.opaque_capability_root = opaque_root;
        self.engine_request.header.payload_len = payload_len;
        self.engine_request.payload = payload;
        self.engine_request
            .validate()
            .expect("rebuilt opaque engine request remains canonical");
        let callback_authority =
            derive_callback_authority_for_engine(&self.engine_request, &effect_engine_probe::ID)
                .expect("derive rebuilt opaque callback authority")
                .0;
        self.callback_authority = callback_authority;
        let authorization_controls =
            if self.envelope.authorization_snapshots[0].witness_kind == WITNESS_DIRECT_ACTOR {
                vec![AccountMeta::new_readonly(self.actor.pubkey(), true)]
            } else {
                vec![AccountMeta::new_readonly(
                    self.spend_authority
                        .expect("non-DIRECT fixture has exact spend authority"),
                    false,
                )]
            };
        let closure = CoreExecuteAccountClosure {
            configuration: self.configuration,
            market: self.market,
            fee_policy: self.fee_policy,
            engine_program: effect_engine_probe::ID,
            callback_authority,
            loader_policy: vec![self.loader_policy_account],
            domain_controls: vec![],
            authorization_controls,
            protected_profile: vec![litesvm_token::TOKEN_ID, self.mint],
            fee_controls: vec![
                AccountMeta::new_readonly(self.fee_shard_descriptor, false),
                AccountMeta::new(self.fee_liability, false),
            ],
            settlement: vec![
                AccountMeta::new(self.source, false),
                AccountMeta::new(self.destination, false),
                AccountMeta::new(self.fee_vault, false),
            ],
            opaque,
        };
        self.instruction = build_core_execute_instruction(&self.envelope, &closure)
            .expect("build rebuilt opaque Core instruction");
    }

    /// Every executable DIRECT fixture uses a real warmed lookup table. The
    /// actor stays a static readonly signer; it can never be hidden in an ALT.
    pub fn compile_v0(&mut self) -> (VersionedTransaction, V0MessageResources) {
        self.compile_custom_v0(self.instruction.clone())
    }

    pub fn compile_custom_v0(
        &mut self,
        instruction: Instruction,
    ) -> (VersionedTransaction, V0MessageResources) {
        self.compile_custom_v0_instructions(vec![instruction])
    }

    pub fn compile_custom_v0_instructions(
        &mut self,
        instructions: Vec<Instruction>,
    ) -> (VersionedTransaction, V0MessageResources) {
        assert!(!instructions.is_empty(), "v0 fixture transaction is empty");
        let mut instructions_with_budget = Vec::with_capacity(instructions.len() + 2);
        instructions_with_budget.push(set_compute_unit_limit_instruction(
            CONTROLLED_COMPUTE_UNIT_LIMIT,
        ));
        instructions_with_budget.push(request_heap_frame_instruction(CONTROLLED_HEAP_FRAME_BYTES));
        instructions_with_budget.extend(instructions);
        self.compile_raw_v0_instructions(instructions_with_budget, true)
    }

    /// Compiles the exact supplied top-level order without adding budget
    /// instructions. This exists only for negative runtime-contract fixtures.
    pub fn compile_raw_v0_instructions(
        &mut self,
        instructions: Vec<Instruction>,
        sign_direct_actor: bool,
    ) -> (VersionedTransaction, V0MessageResources) {
        assert!(!instructions.is_empty(), "v0 fixture transaction is empty");
        let candidates = lookup_candidates(&instructions, self.payer.pubkey());
        let table = install_lookup_table(&mut self.svm, &self.payer, candidates);
        let account = self
            .svm
            .get_account(&table.key)
            .expect("direct fixture ALT account exists");
        AddressLookupTable::deserialize(&account.data)
            .expect("direct fixture installs a real ALT state");
        if sign_direct_actor
            && self.envelope.authorization_snapshots[0].witness_kind == WITNESS_DIRECT_ACTOR
        {
            compile_v0_transaction_with_signers(
                &self.svm,
                &self.payer,
                &instructions,
                &[table],
                &[&self.actor],
            )
            .expect("compile and sign DIRECT v0 transaction")
        } else {
            compile_v0_transaction_with_signers(
                &self.svm,
                &self.payer,
                &instructions,
                &[table],
                &[],
            )
            .expect("compile exact-delegate v0 transaction")
        }
    }

    pub fn immutable_state_addresses(&self) -> [anchor_lang::prelude::Pubkey; 5] {
        [
            self.configuration,
            self.market,
            self.fee_policy,
            self.loader_policy_account,
            self.fee_shard_descriptor,
        ]
    }

    pub fn rollback_state_addresses(&self) -> [anchor_lang::prelude::Pubkey; 9] {
        [
            self.configuration,
            self.market,
            self.fee_policy,
            self.loader_policy_account,
            self.fee_shard_descriptor,
            self.fee_liability,
            self.source,
            self.destination,
            self.fee_vault,
        ]
    }
}

fn direct_settlement_declarations(
    maximum_engine_debit: u64,
    minimum_credit: u64,
    maximum_protocol_fee: u64,
) -> [SettlementCapabilityRowCandidateV0; 3] {
    [
        SettlementCapabilityRowCandidateV0 {
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
            maximum_engine_debit,
            maximum_total_debit: maximum_engine_debit
                .checked_add(maximum_protocol_fee)
                .expect("direct fee-inclusive total debit"),
            minimum_credit: 0,
            maximum_protocol_fee,
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
            rights_bits: RIGHT_CREDIT | RIGHT_EXACT_EXTERNAL_RECIPIENT,
            domain_accounting_slot_or_none: NONE_INDEX,
            spend_authority_control_offset_or_none: NONE_INDEX,
            reserved_0: 0,
            maximum_engine_debit: 0,
            maximum_total_debit: 0,
            minimum_credit,
            maximum_protocol_fee: 0,
        },
        SettlementCapabilityRowCandidateV0 {
            asset_index: 0,
            domain_index_or_none: NONE_INDEX,
            authorization_slot_or_none: NONE_INDEX,
            intent_local_term_index_or_none: NONE_INDEX,
            authority_class: AUTHORITY_CORE_RESERVED_FEE,
            fee_shard_index_or_none: 0,
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
        },
    ]
}

fn engine_context(
    position: u8,
    declaration: SettlementCapabilityRowCandidateV0,
    endpoint: anchor_lang::prelude::Pubkey,
    observed_before: u64,
) -> EngineContextRowCandidateV0 {
    EngineContextRowCandidateV0 {
        settlement_capability_index: position,
        asset_index: declaration.asset_index,
        domain_index_or_none: declaration.domain_index_or_none,
        authorization_slot_or_none: declaration.authorization_slot_or_none,
        rights_bits: declaration.rights_bits,
        fee_class: declaration.fee_class,
        context_flags: 0,
        endpoint_key: endpoint.to_bytes(),
        observed_before,
        accounted_before_or_zero: 0,
        remaining_maximum_engine_debit: declaration.maximum_engine_debit,
        remaining_maximum_total_debit: declaration.maximum_total_debit,
        remaining_minimum_credit: declaration.minimum_credit,
        remaining_maximum_protocol_fee: declaration.maximum_protocol_fee,
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
