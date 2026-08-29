use std::collections::BTreeMap;

use anchor_lang::solana_program::program_option::COption;
use generic_effect_private_wire::{
    compute_asset_set_digest, compute_authorization_state_digest,
    compute_authorization_view_set_digest, compute_domain_set_digest,
    compute_exact_fee_recipient_policy_digest, compute_fee_shard_set_digest,
    compute_intent_capability_terms_root, compute_intent_core_terms_root,
    compute_intent_credit_constraints_root, compute_intent_debit_group_root, compute_intent_digest,
    compute_intent_set_digest, compute_opaque_capability_root,
    compute_open_domain_admission_digest, compute_open_domain_rule_digest, compute_payload_digest,
    compute_protected_execution_root, derive_callback_authority_for_engine,
    AssetBindingRowCandidateV0, AuthorizationSnapshotRowCandidateV0,
    AuthorizationStateDigestInputs, AuthorizationViewRowCandidateV0,
    CoreControlInstructionCandidateV0, CreditConstraintRowCandidateV0, DomainControlRowCandidateV0,
    DomainExecutionRowCandidateV0, EngineAssetRowCandidateV0, EngineContextRowCandidateV0,
    EngineDomainRowCandidateV0, EngineIntentRowCandidateV0, EngineRequestCandidateV0,
    EngineRequestHeaderCandidateV0, FeeShardDigestRowCandidateV0, FeeShardRowCandidateV0,
    InitializeStoredAuthorizationArgsCandidateV0, InlineIntentIdentityRowCandidateV0,
    IntentCapabilityTermRowCandidateV0, IntentCoreTermsDigestInputs, IntentDigestInputs,
    IntentSetRowCandidateV0, OpaqueCapabilityDescriptorCandidateV0, ProtectedExecutionRootInputs,
    SettlementCapabilityRowCandidateV0, StoredAuthorizationChunkCandidateV0,
    StoredAuthorizationChunkHeaderCandidateV0, StoredAuthorizationChunkRowsCandidateV0,
    ADMISSION_OPEN, AUTHORITY_CORE_RESERVED_FEE, AUTHORITY_DOMAIN_ACCOUNTED,
    AUTHORITY_EXACT_EXTERNAL_CREDIT, AUTHORITY_INTENT_FUNDED, ENGINE_REQUEST_MAGIC,
    FEE_CLASS_GROSS_DEBIT_RATE, FEE_CLASS_NONE, NONE_INDEX, PHASE_TRANSITION,
    RIGHT_CORE_RESERVED_FEE, RIGHT_CREDIT, RIGHT_DEBIT, RIGHT_DOMAIN_ACCOUNTED,
    RIGHT_EXACT_EXTERNAL_RECIPIENT, SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT,
    SETTLEMENT_FLAG_FEE_FUNDING, STORED_AUTHORIZATION_CHUNK_KIND_CONSTRAINT,
    STORED_AUTHORIZATION_CHUNK_KIND_TERM, WIRE_VERSION, WITNESS_STORED_AUTHORIZATION,
};
use litesvm::types::TransactionMetadata;
use litesvm_cpi_tree::CpiTreeExt;
use litesvm_token::Approve;
use solana_clock::Clock;
use solana_keypair::Keypair;
use solana_message::{AccountMeta, Instruction};
use solana_native_token::LAMPORTS_PER_SOL;
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

use programmable_generic_effect_core::{
    account_segments::EffectivePrivilege,
    capabilities::{
        validate_settlement_capabilities, AssetProfileIdentity, CapabilityValidationContext,
        DomainCapabilityIdentity, SettlementCapability,
    },
    constants::{EXPERIMENTAL_MAJOR, MAX_ASSETS},
    state::{
        DomainAccountingAssetSlotCandidateV0, DomainAccountingCandidateV0,
        DomainDescriptorAccountCandidateV0, FeeLiabilityLedgerCandidateV0,
        FeeShardDescriptorCandidateV0, MarketDescriptorCandidateV0, StoredAuthorizationCandidateV0,
        StoredAuthorizationLifecycle,
    },
    token_settlement::ClassicSplEndpointSnapshot,
};

use super::{
    build_core_execute_instruction, create_token_account, fixture_keypair, install_anchor_account,
    install_fixture_mint, install_raw_account, mint_tokens, must_send_legacy, read_anchor_account,
    snapshot_accounts, token_state, AccountSnapshot, CoreExecuteAccountClosure, DirectFixture,
    ExecutionResources, SbfArtifacts, V0MessageResources, DIRECT_FEE_RATE_DENOMINATOR,
    DIRECT_FEE_RATE_NUMERATOR,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReferenceAssetSpec {
    pub decimals: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReferenceIntentSpec {
    pub maximum_successful_fills: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceCreditConstraintSpec {
    pub authorization_slot: u8,
    pub credit_local_term_index: u8,
    pub debit_local_term_indices: Vec<u8>,
    pub minimum_credit_numerator: u64,
    pub nonzero_debit_denominator: u64,
    pub terminal_absolute_minimum: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReferenceCapabilityKind {
    IntentDebit {
        authorization_slot: u8,
        maximum_engine_debit: u64,
    },
    ExactCredit {
        authorization_slot: u8,
        minimum_credit: u64,
    },
    DomainDebit {
        maximum_engine_debit: u64,
        accounted_before: u64,
    },
    DomainCredit {
        accounted_before: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReferenceCapabilitySpec {
    pub asset_index: u8,
    pub initial_balance: u64,
    pub kind: ReferenceCapabilityKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceOpaqueSpec {
    pub address_tag: u8,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReferencePlanSpec {
    Explicit(Vec<effect_engine_probe::plan::PlannedMove>),
    Weighted(effect_engine_probe::plan::WeightedAllocationPlan),
    ConstantProduct(effect_engine_probe::plan::ConstantProductPlan),
    PartialAuction(effect_engine_probe::plan::PartialAuctionPlan),
    BatchClearing(effect_engine_probe::plan::BatchClearingPlan),
    InventoryDistribution(effect_engine_probe::plan::InventoryDistributionPlan),
}

impl ReferencePlanSpec {
    fn encode(&self, receipt_mode: u8) -> Vec<u8> {
        match self {
            Self::Explicit(moves) => effect_engine_probe::plan::encode_explicit_plan(
                receipt_mode,
                0,
                NONE_INDEX,
                NONE_INDEX,
                moves,
            ),
            Self::Weighted(plan) => {
                effect_engine_probe::plan::encode_weighted_allocation_plan(receipt_mode, *plan)
            }
            Self::ConstantProduct(plan) => {
                effect_engine_probe::plan::encode_constant_product_plan(receipt_mode, *plan)
            }
            Self::PartialAuction(plan) => {
                effect_engine_probe::plan::encode_partial_auction_plan(receipt_mode, *plan)
            }
            Self::BatchClearing(plan) => {
                effect_engine_probe::plan::encode_batch_clearing_plan(receipt_mode, *plan)
            }
            Self::InventoryDistribution(plan) => {
                effect_engine_probe::plan::encode_inventory_distribution_plan(receipt_mode, *plan)
            }
        }
        .expect("encode reference semantic plan")
    }

    fn expected_sequence(&self, fixture: &ReferenceFixtureCompiler) -> u64 {
        match self {
            Self::ConstantProduct(plan) => fixture
                .constant_product_state(plan.state_position)
                .sequence
                .checked_add(1)
                .expect("constant-product sequence remains bounded"),
            Self::PartialAuction(plan) => fixture
                .auction_state(plan.auction_state_position)
                .sequence
                .checked_add(1)
                .expect("auction sequence remains bounded"),
            Self::Explicit(_)
            | Self::Weighted(_)
            | Self::BatchClearing(_)
            | Self::InventoryDistribution(_) => 0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceFixtureSpec {
    pub label: &'static str,
    pub assets: Vec<ReferenceAssetSpec>,
    pub intents: Vec<ReferenceIntentSpec>,
    pub capabilities: Vec<ReferenceCapabilitySpec>,
    pub credit_constraints: Vec<ReferenceCreditConstraintSpec>,
    pub opaque: Vec<ReferenceOpaqueSpec>,
    pub plan: ReferencePlanSpec,
    pub receipt_mode: u8,
}

#[derive(Clone, Copy, Debug)]
struct CompiledAsset {
    mint: anchor_lang::prelude::Pubkey,
    binding: AssetBindingRowCandidateV0,
    binding_digest: [u8; 32],
}

#[derive(Clone, Copy, Debug)]
struct CompiledFeeShard {
    shard_index: u8,
    asset_index: u8,
    descriptor: anchor_lang::prelude::Pubkey,
    liability: anchor_lang::prelude::Pubkey,
    vault: anchor_lang::prelude::Pubkey,
    vault_capability_index: u8,
}

#[derive(Clone, Debug)]
struct CompiledDomain {
    descriptor: anchor_lang::prelude::Pubkey,
    accounting: anchor_lang::prelude::Pubkey,
    descriptor_digest: [u8; 32],
    admission_digest: [u8; 32],
    accounting_profile_digest: [u8; 32],
    asset_slots: BTreeMap<u8, u8>,
}

/// Generic exact-SBF compiler for reference semantics. Product labels never
/// enter Core or Wire: the compiler reduces one small test specification to
/// their existing typed assets, domains, authorizations and capabilities.
pub struct ReferenceFixtureCompiler {
    pub base: DirectFixture,
    pub spec: ReferenceFixtureSpec,
    pub endpoints: Vec<anchor_lang::prelude::Pubkey>,
    pub authorizations: Vec<anchor_lang::prelude::Pubkey>,
    pub spend_authorities: Vec<Option<anchor_lang::prelude::Pubkey>>,
    pub opaque_accounts: Vec<anchor_lang::prelude::Pubkey>,
    actors: Vec<Keypair>,
    assets: Vec<CompiledAsset>,
    declarations: Vec<SettlementCapabilityRowCandidateV0>,
    fee_shards: Vec<CompiledFeeShard>,
    domain: Option<CompiledDomain>,
}

impl ReferenceFixtureCompiler {
    pub fn new(artifacts: &SbfArtifacts, spec: ReferenceFixtureSpec) -> Self {
        validate_spec(&spec);
        let mut base = DirectFixture::state_only(artifacts);
        let market: MarketDescriptorCandidateV0 = read_anchor_account(&base.svm, &base.market);
        let assets = compile_assets(&mut base, &market, &spec.assets);
        let actors = compile_actors(&mut base, spec.intents.len());
        let domain_seed = spec
            .capabilities
            .iter()
            .any(|capability| {
                matches!(
                    capability.kind,
                    ReferenceCapabilityKind::DomainDebit { .. }
                        | ReferenceCapabilityKind::DomainCredit { .. }
                )
            })
            .then(|| compile_domain_seed(&base, &market));
        let endpoints = compile_endpoints(
            &mut base,
            &spec.capabilities,
            &assets,
            &actors,
            domain_seed.as_ref().map(|domain| domain.accounting),
        );
        let domain = domain_seed
            .map(|seed| finish_domain(&mut base, &market, seed, &spec.capabilities, &assets));
        let (mut declarations, debit_assets) = compile_capability_declarations(&spec);
        let fee_shards = compile_fee_shards(
            &mut base,
            &market,
            &assets,
            &debit_assets,
            declarations.len(),
        );
        bind_local_capability_indices(&mut declarations, &fee_shards, domain.as_ref());
        declarations.extend(
            fee_shards
                .iter()
                .map(|shard| fee_vault_declaration(shard.asset_index, shard.shard_index)),
        );

        let (authorizations, spend_authorities) = compile_stored_authorizations(
            &mut base,
            &spec,
            &assets,
            &endpoints,
            &mut declarations,
            &actors,
            domain.as_ref(),
        );
        let opaque_accounts = compile_opaque_accounts(&mut base, &spec.opaque);

        let mut compiler = Self {
            base,
            spec,
            endpoints,
            authorizations,
            spend_authorities,
            opaque_accounts,
            actors,
            assets,
            declarations,
            fee_shards,
            domain,
        };
        compiler.prepare();
        compiler
    }

    pub fn prepare(&mut self) {
        let payload = self.spec.plan.encode(self.spec.receipt_mode);
        let payload_len = u16::try_from(payload.len()).expect("reference payload fits u16");
        let intent_states = self
            .authorizations
            .iter()
            .map(|authorization| {
                read_anchor_account::<StoredAuthorizationCandidateV0>(&self.base.svm, authorization)
            })
            .collect::<Vec<_>>();
        for state in &intent_states {
            assert_eq!(state.lifecycle, StoredAuthorizationLifecycle::ACTIVE);
        }

        let domain_set_digest = self.domain_set_digest();
        let intent_set_digest = compute_intent_set_digest(
            &domain_set_digest,
            &intent_states
                .iter()
                .map(|state| IntentSetRowCandidateV0 {
                    intent_digest: state.identity.intent_digest,
                })
                .collect::<Vec<_>>(),
        )
        .expect("reference intent set digest");
        let authorization_view_set_digest = compute_authorization_view_set_digest(
            &intent_states
                .iter()
                .enumerate()
                .map(|(slot, state)| AuthorizationViewRowCandidateV0 {
                    authorization_slot: u8::try_from(slot).expect("authorization slot fits u8"),
                    intent_digest: state.identity.intent_digest,
                    authorization_state_digest: authorization_state_digest(
                        state,
                        self.authorizations[slot],
                    ),
                })
                .collect::<Vec<_>>(),
        )
        .expect("reference authorization view set");
        let asset_set_digest = compute_asset_set_digest(
            &self
                .assets
                .iter()
                .map(|asset| asset.binding)
                .collect::<Vec<_>>(),
        )
        .expect("reference asset set digest");
        let protected_capability_set_digest = self.protected_capability_set_digest(&intent_states);
        let fee_shard_set_digest = self.fee_shard_set_digest();
        let protected_execution_root =
            compute_protected_execution_root(ProtectedExecutionRootInputs {
                core_program: &programmable_generic_effect_core::ID.to_bytes(),
                market_binding_digest: &self.base.engine_request.header.market_binding_digest,
                engine_loader_state_snapshot_digest: &self
                    .base
                    .engine_request
                    .header
                    .engine_loader_state_snapshot_digest,
                domain_set_digest: &domain_set_digest,
                intent_set_digest: &intent_set_digest,
                fee_policy_digest: &self.base.engine_request.header.fee_policy_digest,
                asset_set_digest: &asset_set_digest,
                authorization_view_set_digest: &authorization_view_set_digest,
                fee_shard_set_digest: &fee_shard_set_digest,
                protected_capability_set_digest: &protected_capability_set_digest,
            })
            .expect("reference protected execution root");
        let opaque_descriptors = self.opaque_descriptors();
        let opaque_root = compute_opaque_capability_root(&opaque_descriptors)
            .expect("reference opaque capability root");
        let contexts = self.engine_contexts(&intent_states);
        let engine_domains = self.engine_domains();
        let expected_sequence = self.spec.plan.expected_sequence(self);
        let move_count = effect_engine_probe::plan::EnginePlan::decode_exact(&payload)
            .expect("decode encoded reference plan")
            .move_count;

        self.base.engine_request = EngineRequestCandidateV0 {
            header: EngineRequestHeaderCandidateV0 {
                magic: ENGINE_REQUEST_MAGIC,
                wire_version: WIRE_VERSION,
                phase: PHASE_TRANSITION,
                settlement_capability_count: u8::try_from(self.declarations.len())
                    .expect("settlement count fits u8"),
                opaque_capability_count: u8::try_from(self.opaque_accounts.len())
                    .expect("opaque count fits u8"),
                intent_count: u8::try_from(intent_states.len()).expect("intent count fits u8"),
                domain_count: u8::from(self.domain.is_some()),
                asset_count: u8::try_from(self.assets.len()).expect("asset count fits u8"),
                context_row_count: u8::try_from(contexts.len()).expect("context count fits u8"),
                payload_len,
                maximum_engine_moves: move_count,
                market_binding_digest: self.base.engine_request.header.market_binding_digest,
                engine_instance_id: self.base.engine_request.header.engine_instance_id,
                engine_interface_id: self.base.engine_request.header.engine_interface_id,
                intent_set_digest,
                domain_set_digest,
                protected_execution_root,
                opaque_capability_root: opaque_root,
                engine_loader_state_snapshot_digest: self
                    .base
                    .engine_request
                    .header
                    .engine_loader_state_snapshot_digest,
                fee_policy_digest: self.base.engine_request.header.fee_policy_digest,
            },
            assets: self
                .assets
                .iter()
                .enumerate()
                .map(|(index, asset)| EngineAssetRowCandidateV0 {
                    asset_index: u8::try_from(index).expect("asset index fits u8"),
                    asset_flags: asset.binding.flags,
                    decimals: asset.binding.decimals,
                    reserved: asset.binding.reserved,
                    asset_identity: asset.binding.asset_identity,
                    asset_program: asset.binding.asset_program,
                    settlement_profile_digest: asset.binding.settlement_profile_digest,
                })
                .collect(),
            domains: engine_domains,
            intents: intent_states
                .iter()
                .enumerate()
                .map(|(slot, state)| EngineIntentRowCandidateV0 {
                    authorization_slot: u8::try_from(slot).expect("intent slot fits u8"),
                    identity: state.identity.inline_identity(),
                    intent_digest: state.identity.intent_digest,
                })
                .collect(),
            fee_policy: self.base.engine_request.fee_policy,
            contexts,
            payload: payload.clone(),
        };
        self.base
            .engine_request
            .validate()
            .expect("compiled reference engine request is canonical");
        let callback_authority = derive_callback_authority_for_engine(
            &self.base.engine_request,
            &effect_engine_probe::ID,
        )
        .expect("derive reference callback authority")
        .0;
        self.base.callback_authority = callback_authority;

        let authorization_snapshots = intent_states
            .iter()
            .enumerate()
            .map(|(slot, state)| AuthorizationSnapshotRowCandidateV0 {
                authorization_slot: u8::try_from(slot).expect("authorization slot fits u8"),
                witness_kind: WITNESS_STORED_AUTHORIZATION,
                authorization_control_offset_or_none: u8::try_from(slot)
                    .expect("authorization control offset fits u8"),
                inline_identity_index_or_none: NONE_INDEX,
                expected_fill_sequence: state.fill_sequence,
            })
            .collect::<Vec<_>>();
        let domain_controls = self.domain_controls();
        let authorization_controls = self.authorization_controls();
        let fee_rows = self
            .fee_shards
            .iter()
            .enumerate()
            .map(|(index, shard)| FeeShardRowCandidateV0 {
                descriptor_control_offset: u8::try_from(index * 2)
                    .expect("fee descriptor offset fits u8"),
                liability_control_offset: u8::try_from(index * 2 + 1)
                    .expect("fee liability offset fits u8"),
                vault_settlement_capability_index: shard.vault_capability_index,
                asset_index: shard.asset_index,
                flags: 0,
            })
            .collect::<Vec<_>>();
        self.base.envelope = generic_effect_private_wire::ExecuteEnvelopeCandidateV0 {
            header: generic_effect_private_wire::ExecuteEnvelopeHeaderCandidateV0 {
                wire_version: WIRE_VERSION,
                loader_policy_account_count: 1,
                domain_control_account_count: u8::try_from(domain_controls.len())
                    .expect("domain control count fits u8"),
                authorization_account_count: u8::try_from(authorization_controls.len())
                    .expect("authorization control count fits u8"),
                protected_profile_account_count: u8::try_from(1 + self.assets.len())
                    .expect("profile count fits u8"),
                fee_control_account_count: u8::try_from(self.fee_shards.len() * 2)
                    .expect("fee control count fits u8"),
                settlement_capability_count: u8::try_from(self.declarations.len())
                    .expect("settlement count fits u8"),
                opaque_capability_count: u8::try_from(self.opaque_accounts.len())
                    .expect("opaque count fits u8"),
                domain_count: u8::from(self.domain.is_some()),
                intent_count: u8::try_from(intent_states.len()).expect("intent count fits u8"),
                inline_intent_row_count: 0,
                asset_count: u8::try_from(self.assets.len()).expect("asset count fits u8"),
                fee_shard_count: u8::try_from(self.fee_shards.len())
                    .expect("fee shard count fits u8"),
                authorization_snapshot_row_count: u8::try_from(authorization_snapshots.len())
                    .expect("authorization snapshot count fits u8"),
                maximum_engine_moves: move_count,
                flags: 0,
                payload_len,
                expires_at_slot_exclusive: self.base.svm.get_sysvar::<Clock>().slot + 100,
                expected_engine_sequence: expected_sequence,
                intent_set_digest,
                domain_set_digest,
                protected_execution_root,
                expected_opaque_capability_root: opaque_root,
                fee_policy_digest: self.base.engine_request.header.fee_policy_digest,
                expected_engine_loader_state_snapshot_digest: self
                    .base
                    .engine_request
                    .header
                    .engine_loader_state_snapshot_digest,
                payload_digest: compute_payload_digest(&payload).expect("reference payload digest"),
            },
            domain_controls: self.domain_control_rows(),
            authorization_snapshots,
            inline_intent_identities: vec![],
            fee_shards: fee_rows,
            settlement_capabilities: self.declarations.clone(),
            payload,
        };
        let closure = CoreExecuteAccountClosure {
            configuration: self.base.configuration,
            market: self.base.market,
            fee_policy: self.base.fee_policy,
            engine_program: effect_engine_probe::ID,
            callback_authority,
            loader_policy: vec![self.base.loader_policy_account],
            domain_controls,
            authorization_controls,
            protected_profile: std::iter::once(litesvm_token::TOKEN_ID)
                .chain(self.assets.iter().map(|asset| asset.mint))
                .collect(),
            fee_controls: self
                .fee_shards
                .iter()
                .flat_map(|shard| {
                    [
                        AccountMeta::new_readonly(shard.descriptor, false),
                        AccountMeta::new(shard.liability, false),
                    ]
                })
                .collect(),
            settlement: self
                .endpoints
                .iter()
                .copied()
                .map(|endpoint| AccountMeta::new(endpoint, false))
                .chain(
                    self.fee_shards
                        .iter()
                        .map(|shard| AccountMeta::new(shard.vault, false)),
                )
                .collect(),
            opaque: self
                .opaque_accounts
                .iter()
                .copied()
                .map(|account| AccountMeta::new(account, false))
                .collect(),
        };
        self.base.instruction = build_core_execute_instruction(&self.base.envelope, &closure)
            .expect("build compiled reference Core instruction");
    }

    pub fn set_plan(&mut self, plan: ReferencePlanSpec, receipt_mode: u8) {
        self.spec.plan = plan;
        self.spec.receipt_mode = receipt_mode;
        self.prepare();
    }

    pub fn compile_v0(&mut self) -> (VersionedTransaction, V0MessageResources) {
        self.base.compile_v0()
    }

    pub fn send_success(
        &mut self,
    ) -> (TransactionMetadata, V0MessageResources, ExecutionResources) {
        let (transaction, message) = self.compile_v0();
        let metadata = self
            .base
            .svm
            .send_transaction(transaction)
            .unwrap_or_else(|failure| {
                panic!(
                    "{} execution failed: {:?}\n{}\n{}",
                    self.spec.label,
                    failure.err,
                    failure.meta.pretty_logs(),
                    failure.meta.pretty_cpi_tree(),
                )
            });
        let top_level = [
            super::set_compute_unit_limit_instruction(super::CONTROLLED_COMPUTE_UNIT_LIMIT),
            super::request_heap_frame_instruction(super::CONTROLLED_HEAP_FRAME_BYTES),
            self.base.instruction.clone(),
        ];
        let execution = super::measure_execution(&metadata, &top_level);
        (metadata, message, execution)
    }

    pub fn stored_state(&self, slot: usize) -> StoredAuthorizationCandidateV0 {
        read_anchor_account(&self.base.svm, &self.authorizations[slot])
    }

    pub fn endpoint_balance(&self, capability_index: usize) -> u64 {
        super::token_balance(&self.base.svm, &self.endpoints[capability_index])
    }

    pub fn fee_vault_balance(&self, shard_index: usize) -> u64 {
        super::token_balance(&self.base.svm, &self.fee_shards[shard_index].vault)
    }

    pub fn fee_liability(&self, shard_index: usize) -> u128 {
        read_anchor_account::<FeeLiabilityLedgerCandidateV0>(
            &self.base.svm,
            &self.fee_shards[shard_index].liability,
        )
        .liability
    }

    pub fn asset_mint(&self, asset_index: usize) -> anchor_lang::prelude::Pubkey {
        self.assets[asset_index].mint
    }

    pub fn domain_accounted(&self, asset_index: u8) -> u128 {
        let domain = self
            .domain
            .as_ref()
            .expect("reference fixture has a domain");
        let state: DomainAccountingCandidateV0 =
            read_anchor_account(&self.base.svm, &domain.accounting);
        let slot = domain.asset_slots[&asset_index];
        state.assets[usize::from(slot)].accounted_amount
    }

    pub fn constant_product_state(
        &self,
        position: u8,
    ) -> effect_engine_probe::reference_state::ConstantProductStateCandidateV0 {
        let account = self
            .base
            .svm
            .get_account(&self.opaque_accounts[usize::from(position)])
            .expect("constant-product opaque account exists");
        effect_engine_probe::reference_state::ConstantProductStateCandidateV0::decode_exact(
            &account.data,
        )
        .expect("decode exact constant-product state")
    }

    pub fn auction_state(
        &self,
        position: u8,
    ) -> effect_engine_probe::reference_state::AuctionStateCandidateV0 {
        let account = self
            .base
            .svm
            .get_account(&self.opaque_accounts[usize::from(position)])
            .expect("auction opaque account exists");
        effect_engine_probe::reference_state::AuctionStateCandidateV0::decode_exact(&account.data)
            .expect("decode exact auction state")
    }

    pub fn order_state(
        &self,
        position: u8,
    ) -> effect_engine_probe::reference_state::OrderStateCandidateV0 {
        let account = self
            .base
            .svm
            .get_account(&self.opaque_accounts[usize::from(position)])
            .expect("order opaque account exists");
        effect_engine_probe::reference_state::OrderStateCandidateV0::decode_exact(&account.data)
            .expect("decode exact order state")
    }

    pub fn rollback_addresses(&self) -> Vec<anchor_lang::prelude::Pubkey> {
        let mut addresses = vec![
            self.base.configuration,
            self.base.market,
            self.base.fee_policy,
            self.base.loader_policy_account,
        ];
        addresses.extend(self.assets.iter().map(|asset| asset.mint));
        addresses.extend(self.endpoints.iter().copied());
        addresses.extend(self.authorizations.iter().copied());
        for shard in &self.fee_shards {
            addresses.extend([shard.descriptor, shard.liability, shard.vault]);
        }
        if let Some(domain) = &self.domain {
            addresses.extend([domain.descriptor, domain.accounting]);
        }
        addresses.extend(self.opaque_accounts.iter().copied());
        addresses.sort_unstable();
        addresses.dedup();
        addresses
    }

    pub fn rollback_snapshot(&self) -> Vec<AccountSnapshot> {
        snapshot_accounts(&self.base.svm, &self.rollback_addresses())
    }

    pub fn actor_count(&self) -> usize {
        self.actors.len()
    }

    fn asset_set_digest(&self) -> [u8; 32] {
        compute_asset_set_digest(
            &self
                .assets
                .iter()
                .map(|asset| asset.binding)
                .collect::<Vec<_>>(),
        )
        .expect("reference asset set")
    }

    fn domain_set_digest(&self) -> [u8; 32] {
        let rows = self
            .domain
            .as_ref()
            .map(|domain| {
                vec![DomainExecutionRowCandidateV0 {
                    domain_index: 0,
                    admission_kind: ADMISSION_OPEN,
                    domain_descriptor_key: domain.descriptor.to_bytes(),
                    domain_descriptor_digest: domain.descriptor_digest,
                    domain_revision: 1,
                    admission_account_or_zero: [0; 32],
                    admission_digest: domain.admission_digest,
                    accounting_account: domain.accounting.to_bytes(),
                    accounting_profile_digest: domain.accounting_profile_digest,
                }]
            })
            .unwrap_or_default();
        compute_domain_set_digest(
            &self.base.engine_request.header.market_binding_digest,
            &rows,
        )
        .expect("reference domain set digest")
    }

    fn engine_domains(&self) -> Vec<EngineDomainRowCandidateV0> {
        self.domain
            .as_ref()
            .map(|domain| {
                vec![EngineDomainRowCandidateV0 {
                    domain_index: 0,
                    domain_descriptor: domain.descriptor.to_bytes(),
                    domain_revision: 1,
                    admission_digest: domain.admission_digest,
                    accounting_profile_digest: domain.accounting_profile_digest,
                }]
            })
            .unwrap_or_default()
    }

    fn domain_control_rows(&self) -> Vec<DomainControlRowCandidateV0> {
        self.domain
            .as_ref()
            .map(|_| {
                vec![DomainControlRowCandidateV0 {
                    descriptor_control_offset: 0,
                    admission_control_offset_or_none: NONE_INDEX,
                    accounting_control_offset: 1,
                    flags: 0,
                }]
            })
            .unwrap_or_default()
    }

    fn domain_controls(&self) -> Vec<AccountMeta> {
        self.domain
            .as_ref()
            .map(|domain| {
                vec![
                    AccountMeta::new_readonly(domain.descriptor, false),
                    AccountMeta::new(domain.accounting, false),
                ]
            })
            .unwrap_or_default()
    }

    fn authorization_controls(&self) -> Vec<AccountMeta> {
        self.authorizations
            .iter()
            .copied()
            .map(|authorization| AccountMeta::new(authorization, false))
            .chain(
                self.spend_authorities
                    .iter()
                    .flatten()
                    .copied()
                    .map(|authority| AccountMeta::new_readonly(authority, false)),
            )
            .collect()
    }

    fn opaque_descriptors(&self) -> Vec<OpaqueCapabilityDescriptorCandidateV0> {
        self.opaque_accounts
            .iter()
            .enumerate()
            .map(|(position, key)| {
                let account = self
                    .base
                    .svm
                    .get_account(key)
                    .expect("reference opaque account exists");
                OpaqueCapabilityDescriptorCandidateV0 {
                    position: u8::try_from(position).expect("opaque position fits u8"),
                    key: key.to_bytes(),
                    owner: account.owner.to_bytes(),
                    executable: account.executable,
                    effective_signer: false,
                    effective_writable: true,
                }
            })
            .collect()
    }

    fn fee_shard_set_digest(&self) -> [u8; 32] {
        compute_fee_shard_set_digest(
            &self
                .fee_shards
                .iter()
                .map(|shard| {
                    let descriptor: FeeShardDescriptorCandidateV0 =
                        read_anchor_account(&self.base.svm, &shard.descriptor);
                    let liability: FeeLiabilityLedgerCandidateV0 =
                        read_anchor_account(&self.base.svm, &shard.liability);
                    FeeShardDigestRowCandidateV0 {
                        shard_index: shard.shard_index,
                        asset_index: shard.asset_index,
                        vault_settlement_capability_index: shard.vault_capability_index,
                        flags: 0,
                        descriptor_key: shard.descriptor.to_bytes(),
                        descriptor_digest: descriptor.descriptor_digest,
                        liability_key: shard.liability.to_bytes(),
                        vault_key: shard.vault.to_bytes(),
                        asset_binding_digest: self.assets[usize::from(shard.asset_index)]
                            .binding_digest,
                        fee_policy_digest: descriptor.fee_policy_digest,
                        recipient_policy_digest: descriptor.recipient_policy_digest,
                        fee_policy_revision: descriptor.fee_policy_revision,
                        liability_before: liability.liability,
                    }
                })
                .collect::<Vec<_>>(),
        )
        .expect("reference fee shard set")
    }

    fn protected_capability_set_digest(
        &self,
        intent_states: &[StoredAuthorizationCandidateV0],
    ) -> [u8; 32] {
        let accounting = self.domain.as_ref().map(|domain| {
            read_anchor_account::<DomainAccountingCandidateV0>(&self.base.svm, &domain.accounting)
        });
        let capabilities = self
            .declarations
            .iter()
            .enumerate()
            .map(|(position, declaration)| {
                let endpoint = if position < self.endpoints.len() {
                    self.endpoints[position]
                } else {
                    self.fee_shards[position - self.endpoints.len()].vault
                };
                let asset = self.assets[usize::from(declaration.asset_index)];
                let domain = self.domain.as_ref().and_then(|domain| {
                    (declaration.domain_index_or_none != NONE_INDEX).then_some({
                        DomainCapabilityIdentity {
                            domain_index: 0,
                            domain_descriptor: domain.descriptor,
                            domain_revision: 1,
                            admission_digest: domain.admission_digest,
                            accounting_slot: declaration.domain_accounting_slot_or_none,
                        }
                    })
                });
                let transfer_authority_or_zero = match declaration.authority_class {
                    AUTHORITY_INTENT_FUNDED => self.spend_authorities
                        [usize::from(declaration.authorization_slot_or_none)]
                    .expect("intent debit has spend authority"),
                    AUTHORITY_DOMAIN_ACCOUNTED
                        if declaration.rights_bits == (RIGHT_DOMAIN_ACCOUNTED | RIGHT_DEBIT) =>
                    {
                        self.domain
                            .as_ref()
                            .expect("domain debit capability has domain")
                            .accounting
                    }
                    AUTHORITY_DOMAIN_ACCOUNTED => Default::default(),
                    AUTHORITY_EXACT_EXTERNAL_CREDIT | AUTHORITY_CORE_RESERVED_FEE => {
                        Default::default()
                    }
                    _ => unreachable!("reference compiler emitted known authority classes"),
                };
                let accounted_before_or_zero = domain.map_or(0, |_| {
                    accounting
                        .as_ref()
                        .expect("domain accounting loaded")
                        .assets[usize::from(declaration.domain_accounting_slot_or_none)]
                    .accounted_amount
                });
                SettlementCapability {
                    position: u8::try_from(position).expect("capability position fits u8"),
                    declaration: *declaration,
                    core_program: programmable_generic_effect_core::ID,
                    experimental_major: EXPERIMENTAL_MAJOR,
                    market: self.base.market,
                    endpoint: token_effective_privilege(endpoint),
                    transfer_authority_or_zero,
                    asset: AssetProfileIdentity {
                        asset_identity: asset.mint,
                        asset_program: litesvm_token::TOKEN_ID,
                        settlement_profile_digest: asset.binding.settlement_profile_digest,
                    },
                    domain,
                    fee_policy_revision: self.base.engine_request.fee_policy.revision,
                    lifecycle_digest: endpoint_snapshot(&self.base.svm, endpoint)
                        .lifecycle_digest()
                        .expect("reference endpoint lifecycle"),
                    accounted_before_or_zero,
                }
            })
            .collect::<Vec<_>>();
        validate_settlement_capabilities(
            &capabilities,
            CapabilityValidationContext {
                core_program: programmable_generic_effect_core::ID,
                market: self.base.market,
                classic_token_program: litesvm_token::TOKEN_ID,
                experimental_major: EXPERIMENTAL_MAJOR,
                intent_count: u8::try_from(intent_states.len()).expect("intent count fits u8"),
                asset_count: u8::try_from(self.assets.len()).expect("asset count fits u8"),
                domain_count: u8::from(self.domain.is_some()),
                fee_shard_count: u8::try_from(self.fee_shards.len())
                    .expect("fee shard count fits u8"),
                fee_policy_revision: self.base.engine_request.fee_policy.revision,
            },
        )
        .expect("validate reference protected capabilities")
    }

    fn engine_contexts(
        &self,
        intent_states: &[StoredAuthorizationCandidateV0],
    ) -> Vec<EngineContextRowCandidateV0> {
        let accounting = self.domain.as_ref().map(|domain| {
            read_anchor_account::<DomainAccountingCandidateV0>(&self.base.svm, &domain.accounting)
        });
        self.spec
            .capabilities
            .iter()
            .enumerate()
            .map(|(position, _)| {
                let declaration = self.declarations[position];
                let endpoint = endpoint_snapshot(&self.base.svm, self.endpoints[position]);
                let (remaining_engine, remaining_total, remaining_credit, remaining_fee) =
                    if declaration.authorization_slot_or_none != NONE_INDEX {
                        let state =
                            &intent_states[usize::from(declaration.authorization_slot_or_none)];
                        let bound = state.capabilities
                            [usize::from(declaration.intent_local_term_index_or_none)];
                        (
                            u64::try_from(
                                u128::from(bound.initial_maximum_engine_debit)
                                    .checked_sub(bound.cumulative_engine_debit)
                                    .expect("stored cumulative engine debit is bounded"),
                            )
                            .expect("remaining engine debit fits u64")
                            .min(bound.remaining_total_debit),
                            bound.remaining_total_debit,
                            u64::try_from(
                                u128::from(bound.initial_minimum_credit)
                                    .saturating_sub(bound.cumulative_credit),
                            )
                            .expect("remaining credit fits u64"),
                            u64::try_from(
                                u128::from(declaration.maximum_protocol_fee)
                                    .checked_sub(bound.cumulative_fee_debit)
                                    .expect("stored cumulative fee is bounded"),
                            )
                            .expect("remaining fee fits u64")
                            .min(bound.remaining_total_debit),
                        )
                    } else {
                        (
                            declaration.maximum_engine_debit,
                            declaration.maximum_total_debit,
                            declaration.minimum_credit,
                            declaration.maximum_protocol_fee,
                        )
                    };
                let accounted_before_or_zero = if declaration.domain_index_or_none == NONE_INDEX {
                    0
                } else {
                    u64::try_from(
                        accounting
                            .as_ref()
                            .expect("domain accounting loaded")
                            .assets[usize::from(declaration.domain_accounting_slot_or_none)]
                        .accounted_amount,
                    )
                    .expect("reference accounted amount fits u64")
                };
                EngineContextRowCandidateV0 {
                    settlement_capability_index: u8::try_from(position)
                        .expect("context capability index fits u8"),
                    asset_index: declaration.asset_index,
                    domain_index_or_none: declaration.domain_index_or_none,
                    authorization_slot_or_none: declaration.authorization_slot_or_none,
                    rights_bits: declaration.rights_bits,
                    fee_class: declaration.fee_class,
                    context_flags: 0,
                    endpoint_key: endpoint.key.to_bytes(),
                    observed_before: endpoint.amount,
                    accounted_before_or_zero,
                    remaining_maximum_engine_debit: remaining_engine,
                    remaining_maximum_total_debit: remaining_total,
                    remaining_minimum_credit: remaining_credit,
                    remaining_maximum_protocol_fee: remaining_fee,
                }
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug)]
struct DomainSeed {
    descriptor: anchor_lang::prelude::Pubkey,
    accounting: anchor_lang::prelude::Pubkey,
    accounting_bump: u8,
    descriptor_digest: [u8; 32],
    admission_digest: [u8; 32],
    accounting_profile_digest: [u8; 32],
}

fn validate_spec(spec: &ReferenceFixtureSpec) {
    assert!(!spec.assets.is_empty() && spec.assets.len() <= 2);
    assert_eq!(
        spec.assets[0].decimals, 6,
        "asset zero reuses the base mint"
    );
    assert!(!spec.intents.is_empty());
    assert!(!spec.capabilities.is_empty());
    assert!(spec
        .intents
        .iter()
        .all(|intent| intent.maximum_successful_fills != 0));
    for constraint in &spec.credit_constraints {
        assert!(usize::from(constraint.authorization_slot) < spec.intents.len());
        assert!(!constraint.debit_local_term_indices.is_empty());
        assert!(constraint
            .debit_local_term_indices
            .windows(2)
            .all(|pair| pair[0] < pair[1]));
        assert!(constraint.nonzero_debit_denominator != 0);
        assert!(
            constraint.minimum_credit_numerator != 0 || constraint.terminal_absolute_minimum != 0
        );
    }
    for capability in &spec.capabilities {
        assert!(usize::from(capability.asset_index) < spec.assets.len());
        match capability.kind {
            ReferenceCapabilityKind::IntentDebit {
                authorization_slot,
                maximum_engine_debit,
            } => {
                assert!(usize::from(authorization_slot) < spec.intents.len());
                assert!(maximum_engine_debit != 0);
                assert!(capability.initial_balance >= maximum_engine_debit);
            }
            ReferenceCapabilityKind::ExactCredit {
                authorization_slot,
                minimum_credit,
            } => {
                assert!(usize::from(authorization_slot) < spec.intents.len());
                assert!(minimum_credit != 0);
            }
            ReferenceCapabilityKind::DomainDebit {
                maximum_engine_debit,
                accounted_before,
            } => {
                assert!(maximum_engine_debit != 0);
                assert!(accounted_before != 0 && accounted_before <= capability.initial_balance);
            }
            ReferenceCapabilityKind::DomainCredit { accounted_before } => {
                assert!(accounted_before <= capability.initial_balance);
            }
        }
    }
}

fn compile_assets(
    base: &mut DirectFixture,
    market: &MarketDescriptorCandidateV0,
    specs: &[ReferenceAssetSpec],
) -> Vec<CompiledAsset> {
    let base_binding = AssetBindingRowCandidateV0 {
        wire_version: WIRE_VERSION,
        flags: 0,
        decimals: 6,
        reserved: 0,
        asset_identity: base.mint.to_bytes(),
        asset_program: litesvm_token::TOKEN_ID.to_bytes(),
        settlement_profile_digest: market.protected_profile_digest,
    };
    let mut assets = vec![CompiledAsset {
        mint: base.mint,
        binding: base_binding,
        binding_digest: base_binding.digest().expect("base asset binding digest"),
    }];
    if specs.len() == 2 {
        let previous = assets[0].binding_digest;
        let (tag, binding) = (140_u8..=239)
            .find_map(|tag| {
                let mint = anchor_lang::prelude::Pubkey::new_from_array([tag; 32]);
                let binding = AssetBindingRowCandidateV0 {
                    wire_version: WIRE_VERSION,
                    flags: 0,
                    decimals: specs[1].decimals,
                    reserved: 0,
                    asset_identity: mint.to_bytes(),
                    asset_program: litesvm_token::TOKEN_ID.to_bytes(),
                    settlement_profile_digest: market.protected_profile_digest,
                };
                (binding.digest().ok()? > previous).then_some((tag, binding))
            })
            .expect("find deterministic second mint preserving binding-digest order");
        let mint = install_fixture_mint(&mut base.svm, tag, base.payer.pubkey(), specs[1].decimals);
        assert_eq!(mint.to_bytes(), binding.asset_identity);
        assets.push(CompiledAsset {
            mint,
            binding,
            binding_digest: binding.digest().expect("second asset binding digest"),
        });
    }
    assets
}

fn compile_actors(base: &mut DirectFixture, count: usize) -> Vec<Keypair> {
    (0..count)
        .map(|slot| {
            let actor = fixture_keypair(
                100_u8
                    .checked_add(u8::try_from(slot).expect("bounded actor slot"))
                    .expect("actor tag fits u8"),
            );
            base.svm
                .airdrop(&actor.pubkey(), LAMPORTS_PER_SOL)
                .expect("fund reference stored actor");
            actor
        })
        .collect()
}

fn compile_domain_seed(base: &DirectFixture, market: &MarketDescriptorCandidateV0) -> DomainSeed {
    let descriptor = fixture_keypair(170).pubkey();
    let (accounting, accounting_bump) =
        DomainAccountingCandidateV0::address(&programmable_generic_effect_core::ID, &descriptor);
    let accounting_profile_digest = [0x71; 32];
    let admission_rule_digest = compute_open_domain_rule_digest().expect("open domain rule digest");
    let descriptor_state = DomainDescriptorAccountCandidateV0 {
        wire_version: WIRE_VERSION,
        rule_kind: generic_effect_private_wire::DOMAIN_RULE_OPEN,
        reserved: [0; 6],
        controller_program: callback_capability_probe::ID,
        controller_identity: fixture_keypair(171).pubkey(),
        domain_revision: 1,
        namespace_or_instance: [0x72; 32],
        custody_profile_digest: [0x73; 32],
        asset_profile_digest: [0x74; 32],
        accounting_profile_digest,
        exit_class_digest: [0x75; 32],
        admission_rule_digest,
        protected_profile_digest: market.protected_profile_digest,
    };
    let descriptor_digest = descriptor_state
        .digest(&programmable_generic_effect_core::ID)
        .expect("reference domain descriptor digest");
    let admission_digest = compute_open_domain_admission_digest(
        &descriptor_digest,
        &base.engine_request.header.market_binding_digest,
    )
    .expect("reference open-domain admission digest");
    DomainSeed {
        descriptor,
        accounting,
        accounting_bump,
        descriptor_digest,
        admission_digest,
        accounting_profile_digest,
    }
}

fn compile_endpoints(
    base: &mut DirectFixture,
    capabilities: &[ReferenceCapabilitySpec],
    assets: &[CompiledAsset],
    actors: &[Keypair],
    domain_accounting: Option<anchor_lang::prelude::Pubkey>,
) -> Vec<anchor_lang::prelude::Pubkey> {
    capabilities
        .iter()
        .enumerate()
        .map(|(position, capability)| {
            let owner = match capability.kind {
                ReferenceCapabilityKind::IntentDebit {
                    authorization_slot, ..
                } => actors[usize::from(authorization_slot)].pubkey(),
                ReferenceCapabilityKind::ExactCredit { .. } => fixture_keypair(
                    180_u8
                        .checked_add(u8::try_from(position).expect("bounded endpoint position"))
                        .expect("endpoint-owner tag fits u8"),
                )
                .pubkey(),
                ReferenceCapabilityKind::DomainDebit { .. }
                | ReferenceCapabilityKind::DomainCredit { .. } => {
                    domain_accounting.expect("domain capability has accounting PDA")
                }
            };
            let mint = assets[usize::from(capability.asset_index)].mint;
            let endpoint = create_token_account(&mut base.svm, &base.payer, &mint, &owner);
            if capability.initial_balance != 0 {
                mint_tokens(
                    &mut base.svm,
                    &base.payer,
                    &mint,
                    &endpoint,
                    capability.initial_balance,
                );
            }
            endpoint
        })
        .collect()
}

fn finish_domain(
    base: &mut DirectFixture,
    market: &MarketDescriptorCandidateV0,
    seed: DomainSeed,
    capabilities: &[ReferenceCapabilitySpec],
    assets: &[CompiledAsset],
) -> CompiledDomain {
    let descriptor_state = DomainDescriptorAccountCandidateV0 {
        wire_version: WIRE_VERSION,
        rule_kind: generic_effect_private_wire::DOMAIN_RULE_OPEN,
        reserved: [0; 6],
        controller_program: callback_capability_probe::ID,
        controller_identity: fixture_keypair(171).pubkey(),
        domain_revision: 1,
        namespace_or_instance: [0x72; 32],
        custody_profile_digest: [0x73; 32],
        asset_profile_digest: [0x74; 32],
        accounting_profile_digest: seed.accounting_profile_digest,
        exit_class_digest: [0x75; 32],
        admission_rule_digest: compute_open_domain_rule_digest().expect("open domain rule"),
        protected_profile_digest: market.protected_profile_digest,
    };
    install_anchor_account(
        &mut base.svm,
        seed.descriptor,
        programmable_generic_effect_core::ID,
        &descriptor_state,
        DomainDescriptorAccountCandidateV0::SPACE,
    );
    let mut accounted_by_asset = BTreeMap::new();
    for capability in capabilities {
        let accounted = match capability.kind {
            ReferenceCapabilityKind::DomainDebit {
                accounted_before, ..
            }
            | ReferenceCapabilityKind::DomainCredit { accounted_before } => accounted_before,
            ReferenceCapabilityKind::IntentDebit { .. }
            | ReferenceCapabilityKind::ExactCredit { .. } => continue,
        };
        assert!(
            accounted_by_asset
                .insert(capability.asset_index, accounted)
                .is_none(),
            "one domain endpoint per asset keeps accounting unambiguous"
        );
    }
    let mut slots = [DomainAccountingAssetSlotCandidateV0::default(); MAX_ASSETS];
    let mut asset_slots = BTreeMap::new();
    for (slot, (asset_index, accounted)) in accounted_by_asset.iter().enumerate() {
        let asset = assets[usize::from(*asset_index)];
        slots[slot] = DomainAccountingAssetSlotCandidateV0 {
            domain_asset_slot: u8::try_from(slot).expect("domain slot fits u8"),
            reserved: [0; 7],
            asset_identity: asset.mint,
            asset_program: litesvm_token::TOKEN_ID,
            settlement_profile_digest: asset.binding.settlement_profile_digest,
            accounted_amount: u128::from(*accounted),
        };
        asset_slots.insert(
            *asset_index,
            u8::try_from(slot).expect("domain slot fits u8"),
        );
    }
    let accounting_state = DomainAccountingCandidateV0 {
        wire_version: WIRE_VERSION,
        asset_count: u8::try_from(accounted_by_asset.len()).expect("domain asset count fits u8"),
        bump: seed.accounting_bump,
        reserved: [0; 5],
        domain_descriptor: seed.descriptor,
        domain_revision: 1,
        assets: slots,
    };
    accounting_state
        .validate_authenticated(
            &programmable_generic_effect_core::ID,
            &seed.accounting,
            &seed.descriptor,
            1,
        )
        .expect("validate reference domain accounting");
    install_anchor_account(
        &mut base.svm,
        seed.accounting,
        programmable_generic_effect_core::ID,
        &accounting_state,
        DomainAccountingCandidateV0::SPACE,
    );
    CompiledDomain {
        descriptor: seed.descriptor,
        accounting: seed.accounting,
        descriptor_digest: seed.descriptor_digest,
        admission_digest: seed.admission_digest,
        accounting_profile_digest: seed.accounting_profile_digest,
        asset_slots,
    }
}

fn compile_capability_declarations(
    spec: &ReferenceFixtureSpec,
) -> (Vec<SettlementCapabilityRowCandidateV0>, Vec<u8>) {
    let mut local_terms = vec![0_u8; spec.intents.len()];
    let mut debit_assets = Vec::new();
    let mut declarations = spec
        .capabilities
        .iter()
        .map(|capability| match capability.kind {
            ReferenceCapabilityKind::IntentDebit {
                authorization_slot,
                maximum_engine_debit,
            } => {
                let local = local_terms[usize::from(authorization_slot)];
                local_terms[usize::from(authorization_slot)] += 1;
                if !debit_assets.contains(&capability.asset_index) {
                    debit_assets.push(capability.asset_index);
                }
                let maximum_protocol_fee = protocol_fee(maximum_engine_debit);
                assert!(maximum_protocol_fee != 0);
                SettlementCapabilityRowCandidateV0 {
                    asset_index: capability.asset_index,
                    domain_index_or_none: NONE_INDEX,
                    authorization_slot_or_none: authorization_slot,
                    intent_local_term_index_or_none: local,
                    authority_class: AUTHORITY_INTENT_FUNDED,
                    fee_shard_index_or_none: capability.asset_index,
                    fee_class: FEE_CLASS_GROSS_DEBIT_RATE,
                    flags: SETTLEMENT_FLAG_FEE_FUNDING,
                    rights_bits: RIGHT_DEBIT,
                    domain_accounting_slot_or_none: NONE_INDEX,
                    spend_authority_control_offset_or_none: NONE_INDEX,
                    reserved_0: 0,
                    maximum_engine_debit,
                    maximum_total_debit: maximum_engine_debit
                        .checked_add(maximum_protocol_fee)
                        .expect("reference total debit bound"),
                    minimum_credit: 0,
                    maximum_protocol_fee,
                }
            }
            ReferenceCapabilityKind::ExactCredit {
                authorization_slot,
                minimum_credit,
            } => {
                let local = local_terms[usize::from(authorization_slot)];
                local_terms[usize::from(authorization_slot)] += 1;
                SettlementCapabilityRowCandidateV0 {
                    asset_index: capability.asset_index,
                    domain_index_or_none: NONE_INDEX,
                    authorization_slot_or_none: authorization_slot,
                    intent_local_term_index_or_none: local,
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
                    minimum_credit,
                    maximum_protocol_fee: 0,
                }
            }
            ReferenceCapabilityKind::DomainDebit {
                maximum_engine_debit,
                ..
            } => SettlementCapabilityRowCandidateV0 {
                asset_index: capability.asset_index,
                domain_index_or_none: 0,
                authorization_slot_or_none: NONE_INDEX,
                intent_local_term_index_or_none: NONE_INDEX,
                authority_class: AUTHORITY_DOMAIN_ACCOUNTED,
                fee_shard_index_or_none: NONE_INDEX,
                fee_class: FEE_CLASS_NONE,
                flags: 0,
                rights_bits: RIGHT_DOMAIN_ACCOUNTED | RIGHT_DEBIT,
                domain_accounting_slot_or_none: capability.asset_index,
                spend_authority_control_offset_or_none: NONE_INDEX,
                reserved_0: 0,
                maximum_engine_debit,
                maximum_total_debit: maximum_engine_debit,
                minimum_credit: 0,
                maximum_protocol_fee: 0,
            },
            ReferenceCapabilityKind::DomainCredit { .. } => SettlementCapabilityRowCandidateV0 {
                asset_index: capability.asset_index,
                domain_index_or_none: 0,
                authorization_slot_or_none: NONE_INDEX,
                intent_local_term_index_or_none: NONE_INDEX,
                authority_class: AUTHORITY_DOMAIN_ACCOUNTED,
                fee_shard_index_or_none: NONE_INDEX,
                fee_class: FEE_CLASS_NONE,
                flags: 0,
                rights_bits: RIGHT_DOMAIN_ACCOUNTED | RIGHT_CREDIT,
                domain_accounting_slot_or_none: capability.asset_index,
                spend_authority_control_offset_or_none: NONE_INDEX,
                reserved_0: 0,
                maximum_engine_debit: 0,
                maximum_total_debit: 0,
                minimum_credit: 0,
                maximum_protocol_fee: 0,
            },
        })
        .collect::<Vec<SettlementCapabilityRowCandidateV0>>();
    for declaration in &mut declarations {
        if declaration.authority_class != AUTHORITY_INTENT_FUNDED {
            continue;
        }
        let constrained = spec.credit_constraints.iter().any(|constraint| {
            constraint.authorization_slot == declaration.authorization_slot_or_none
                && constraint
                    .debit_local_term_indices
                    .contains(&declaration.intent_local_term_index_or_none)
        });
        if !constrained {
            declaration.flags |= SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT;
        }
    }
    debit_assets.sort_unstable();
    (declarations, debit_assets)
}

fn compile_fee_shards(
    base: &mut DirectFixture,
    market: &MarketDescriptorCandidateV0,
    assets: &[CompiledAsset],
    debit_assets: &[u8],
    first_vault_capability: usize,
) -> Vec<CompiledFeeShard> {
    debit_assets
        .iter()
        .enumerate()
        .map(|(shard_position, asset_index)| {
            let shard_index = u8::try_from(shard_position).expect("fee shard index fits u8");
            let asset = assets[usize::from(*asset_index)];
            let vault = if shard_index == 0 && *asset_index == 0 {
                base.fee_vault
            } else {
                create_token_account(
                    &mut base.svm,
                    &base.payer,
                    &asset.mint,
                    &base.payer.pubkey(),
                )
            };
            let (descriptor, descriptor_bump) = FeeShardDescriptorCandidateV0::address(
                &programmable_generic_effect_core::ID,
                &market.market_binding_digest,
                shard_index,
            );
            let (liability, liability_bump) = FeeLiabilityLedgerCandidateV0::address(
                &programmable_generic_effect_core::ID,
                &descriptor,
                &market.market_binding_digest,
            );
            if !(shard_index == 0 && *asset_index == 0) {
                let recipient_policy_digest = compute_exact_fee_recipient_policy_digest(
                    &programmable_generic_effect_core::ID.to_bytes(),
                    &market.market_binding_digest,
                    &vault.to_bytes(),
                    &asset.mint.to_bytes(),
                    &litesvm_token::TOKEN_ID.to_bytes(),
                    &market.protected_profile_digest,
                )
                .expect("reference fee recipient policy");
                let mut descriptor_state = FeeShardDescriptorCandidateV0 {
                    wire_version: WIRE_VERSION,
                    shard_index,
                    bump: descriptor_bump,
                    reserved: [0; 5],
                    descriptor_digest: [0; 32],
                    market_binding_digest: market.market_binding_digest,
                    fee_policy_digest: market.fee_policy_digest,
                    fee_policy_revision: market.fee_policy_revision,
                    asset_identity: asset.mint,
                    asset_program: litesvm_token::TOKEN_ID,
                    settlement_profile_digest: market.protected_profile_digest,
                    vault,
                    liability_ledger: liability,
                    recipient_policy_digest,
                };
                descriptor_state.descriptor_digest = descriptor_state
                    .derive_descriptor_digest(&programmable_generic_effect_core::ID)
                    .expect("reference fee descriptor digest");
                let liability_state = FeeLiabilityLedgerCandidateV0 {
                    wire_version: WIRE_VERSION,
                    shard_index,
                    bump: liability_bump,
                    reserved: [0; 5],
                    descriptor,
                    market_binding_digest: market.market_binding_digest,
                    asset_identity: asset.mint,
                    settlement_profile_digest: market.protected_profile_digest,
                    liability: 0,
                };
                install_anchor_account(
                    &mut base.svm,
                    descriptor,
                    programmable_generic_effect_core::ID,
                    &descriptor_state,
                    FeeShardDescriptorCandidateV0::SPACE,
                );
                install_anchor_account(
                    &mut base.svm,
                    liability,
                    programmable_generic_effect_core::ID,
                    &liability_state,
                    FeeLiabilityLedgerCandidateV0::SPACE,
                );
            } else {
                assert_eq!(descriptor, base.fee_shard_descriptor);
                assert_eq!(liability, base.fee_liability);
            }
            CompiledFeeShard {
                shard_index,
                asset_index: *asset_index,
                descriptor,
                liability,
                vault,
                vault_capability_index: u8::try_from(first_vault_capability + shard_position)
                    .expect("fee vault capability index fits u8"),
            }
        })
        .collect()
}

fn bind_local_capability_indices(
    declarations: &mut [SettlementCapabilityRowCandidateV0],
    fee_shards: &[CompiledFeeShard],
    domain: Option<&CompiledDomain>,
) {
    for declaration in declarations {
        if declaration.authority_class == AUTHORITY_INTENT_FUNDED {
            declaration.fee_shard_index_or_none = fee_shards
                .iter()
                .find(|shard| shard.asset_index == declaration.asset_index)
                .map(|shard| shard.shard_index)
                .expect("every intent debit asset has one compiled fee shard");
        }
        if declaration.domain_index_or_none != NONE_INDEX {
            declaration.domain_accounting_slot_or_none = *domain
                .expect("domain capability has compiled domain")
                .asset_slots
                .get(&declaration.asset_index)
                .expect("domain capability asset has an accounting slot");
        }
    }
}

fn compile_stored_authorizations(
    base: &mut DirectFixture,
    spec: &ReferenceFixtureSpec,
    assets: &[CompiledAsset],
    endpoints: &[anchor_lang::prelude::Pubkey],
    declarations: &mut [SettlementCapabilityRowCandidateV0],
    actors: &[Keypair],
    domain: Option<&CompiledDomain>,
) -> (
    Vec<anchor_lang::prelude::Pubkey>,
    Vec<Option<anchor_lang::prelude::Pubkey>>,
) {
    let mut authorizations = Vec::with_capacity(spec.intents.len());
    let mut spend_authorities = vec![None; spec.intents.len()];
    for (slot, intent) in spec.intents.iter().enumerate() {
        let slot_u8 = u8::try_from(slot).expect("authorization slot fits u8");
        let terms = declarations
            .iter()
            .enumerate()
            .filter(|(_, declaration)| declaration.authorization_slot_or_none == slot_u8)
            .map(
                |(position, declaration)| IntentCapabilityTermRowCandidateV0 {
                    intent_local_term_index: declaration.intent_local_term_index_or_none,
                    authority_class: declaration.authority_class,
                    fee_class: declaration.fee_class,
                    flags: declaration.flags,
                    rights_bits: declaration.rights_bits,
                    endpoint_key: endpoints[position].to_bytes(),
                    asset_binding_digest: assets[usize::from(declaration.asset_index)]
                        .binding_digest,
                    required_domain_descriptor_digest_or_zero: domain
                        .filter(|_| declaration.domain_index_or_none != NONE_INDEX)
                        .map_or([0; 32], |domain| domain.descriptor_digest),
                    maximum_engine_debit: declaration.maximum_engine_debit,
                    maximum_total_debit: declaration.maximum_total_debit,
                    minimum_credit: declaration.minimum_credit,
                    maximum_protocol_fee: declaration.maximum_protocol_fee,
                },
            )
            .collect::<Vec<_>>();
        assert!(
            !terms.is_empty(),
            "every reference intent owns at least one term"
        );
        let constraints = spec
            .credit_constraints
            .iter()
            .filter(|constraint| constraint.authorization_slot == slot_u8)
            .enumerate()
            .map(|(constraint_index, constraint)| {
                let credit = terms
                    .get(usize::from(constraint.credit_local_term_index))
                    .expect("constraint credit local term exists");
                assert_eq!(
                    credit.authority_class, AUTHORITY_EXACT_EXTERNAL_CREDIT,
                    "constraint credit local term is an exact credit"
                );
                let mut debit_source_bitmap = 0_u16;
                for source in &constraint.debit_local_term_indices {
                    let debit = terms
                        .get(usize::from(*source))
                        .expect("constraint debit local term exists");
                    assert_eq!(
                        debit.authority_class, AUTHORITY_INTENT_FUNDED,
                        "constraint source local term is an intent debit"
                    );
                    debit_source_bitmap |= 1_u16 << source;
                }
                CreditConstraintRowCandidateV0 {
                    constraint_index: u8::try_from(constraint_index)
                        .expect("constraint index fits u8"),
                    credit_local_term_index: constraint.credit_local_term_index,
                    flags: 0,
                    debit_source_bitmap,
                    debit_group_root: compute_intent_debit_group_root(
                        &constraint.debit_local_term_indices,
                    )
                    .expect("reference debit group root"),
                    minimum_credit_numerator: constraint.minimum_credit_numerator,
                    nonzero_debit_denominator: constraint.nonzero_debit_denominator,
                    terminal_absolute_minimum: constraint.terminal_absolute_minimum,
                }
            })
            .collect::<Vec<_>>();
        let capability_terms_root = compute_intent_capability_terms_root(&terms)
            .expect("reference stored capability terms root");
        let credit_constraints_root = compute_intent_credit_constraints_root(&constraints)
            .expect("reference credit constraints root");
        let core_terms_root = compute_intent_core_terms_root(IntentCoreTermsDigestInputs {
            maximum_successful_fills: intent.maximum_successful_fills,
            capability_terms_root: &capability_terms_root,
            credit_constraints_root: &credit_constraints_root,
        })
        .expect("reference stored Core terms root");
        let identity = InlineIntentIdentityRowCandidateV0 {
            actor: actors[slot].pubkey().to_bytes(),
            engine_terms_commitment: [0x80_u8
                .checked_add(slot_u8)
                .expect("engine terms marker fits u8"); 32],
            authorization_nonce: 1 + u64::try_from(slot).expect("slot fits u64"),
            expires_at_slot_exclusive: base.svm.get_sysvar::<Clock>().slot + 500,
        };
        let intent_digest = compute_intent_digest(IntentDigestInputs {
            core_program: &programmable_generic_effect_core::ID.to_bytes(),
            market_binding_digest: &base.engine_request.header.market_binding_digest,
            loader_state_snapshot_digest: &base
                .engine_request
                .header
                .engine_loader_state_snapshot_digest,
            fee_policy_digest: &base.engine_request.header.fee_policy_digest,
            identity: &identity,
            core_terms_root: &core_terms_root,
        })
        .expect("reference stored intent digest");
        let args = InitializeStoredAuthorizationArgsCandidateV0 {
            wire_version: WIRE_VERSION,
            term_count: u8::try_from(terms.len()).expect("stored term count fits u8"),
            constraint_count: u8::try_from(constraints.len()).expect("constraint count fits u8"),
            flags: 0,
            maximum_successful_fills: intent.maximum_successful_fills,
            identity,
            market_binding_digest: base.engine_request.header.market_binding_digest,
            engine_loader_state_snapshot_digest: base
                .engine_request
                .header
                .engine_loader_state_snapshot_digest,
            fee_policy_digest: base.engine_request.header.fee_policy_digest,
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
        let initialize = Instruction {
            program_id: programmable_generic_effect_core::ID,
            accounts: vec![
                AccountMeta::new(base.payer.pubkey(), true),
                AccountMeta::new_readonly(actors[slot].pubkey(), true),
                AccountMeta::new(authorization, false),
                AccountMeta::new_readonly(solana_sdk_ids::system_program::id(), false),
                AccountMeta::new_readonly(solana_sdk_ids::sysvar::instructions::id(), false),
            ],
            data: CoreControlInstructionCandidateV0::InitializeStoredAuthorization(args)
                .encode()
                .expect("encode reference stored initializer"),
        };
        must_send_legacy(
            &mut base.svm,
            &base.payer,
            &[initialize],
            &[&actors[slot]],
            "initialize reference stored authorization",
        );
        let write = stored_control_instruction(
            actors[slot].pubkey(),
            authorization,
            CoreControlInstructionCandidateV0::WriteStoredAuthorizationChunk(
                StoredAuthorizationChunkCandidateV0 {
                    header: StoredAuthorizationChunkHeaderCandidateV0 {
                        wire_version: WIRE_VERSION,
                        chunk_kind: STORED_AUTHORIZATION_CHUNK_KIND_TERM,
                        start_index: 0,
                        row_count: u8::try_from(terms.len()).expect("stored term count fits u8"),
                    },
                    rows: StoredAuthorizationChunkRowsCandidateV0::Terms(terms),
                },
            ),
        );
        must_send_legacy(
            &mut base.svm,
            &base.payer,
            &[write],
            &[&actors[slot]],
            "write reference stored terms",
        );
        if !constraints.is_empty() {
            let write_constraints = stored_control_instruction(
                actors[slot].pubkey(),
                authorization,
                CoreControlInstructionCandidateV0::WriteStoredAuthorizationChunk(
                    StoredAuthorizationChunkCandidateV0 {
                        header: StoredAuthorizationChunkHeaderCandidateV0 {
                            wire_version: WIRE_VERSION,
                            chunk_kind: STORED_AUTHORIZATION_CHUNK_KIND_CONSTRAINT,
                            start_index: 0,
                            row_count: u8::try_from(constraints.len())
                                .expect("stored constraint count fits u8"),
                        },
                        rows: StoredAuthorizationChunkRowsCandidateV0::Constraints(constraints),
                    },
                ),
            );
            must_send_legacy(
                &mut base.svm,
                &base.payer,
                &[write_constraints],
                &[&actors[slot]],
                "write reference stored constraints",
            );
        }
        let activate = stored_control_instruction(
            actors[slot].pubkey(),
            authorization,
            CoreControlInstructionCandidateV0::ActivateStoredAuthorization,
        );
        must_send_legacy(
            &mut base.svm,
            &base.payer,
            &[
                super::set_compute_unit_limit_instruction(super::CONTROLLED_COMPUTE_UNIT_LIMIT),
                super::request_heap_frame_instruction(super::CONTROLLED_HEAP_FRAME_BYTES),
                activate,
            ],
            &[&actors[slot]],
            "activate reference stored authorization",
        );
        let debit_position = declarations.iter().position(|declaration| {
            declaration.authorization_slot_or_none == slot_u8
                && declaration.authority_class == AUTHORITY_INTENT_FUNDED
        });
        if let Some(position) = debit_position {
            assert!(
                !declarations.iter().enumerate().any(|(other, declaration)| {
                    other != position
                        && declaration.authorization_slot_or_none == slot_u8
                        && declaration.authority_class == AUTHORITY_INTENT_FUNDED
                }),
                "reference intents support one debit source each"
            );
            let (spend_authority, _) =
                programmable_generic_effect_core::authorization::derive_exact_spend_authority(
                    &programmable_generic_effect_core::ID,
                    &intent_digest,
                    &endpoints[position],
                )
                .expect("derive reference stored spend authority");
            Approve::new(
                &mut base.svm,
                &base.payer,
                &spend_authority,
                &endpoints[position],
                declarations[position].maximum_total_debit,
            )
            .owner(&actors[slot])
            .send()
            .expect("approve reference stored spend authority");
            spend_authorities[slot] = Some(spend_authority);
        }
        authorizations.push(authorization);
    }
    let mut entries = authorizations
        .into_iter()
        .zip(spend_authorities)
        .enumerate()
        .map(|(old_slot, (authorization, spend_authority))| {
            let state: StoredAuthorizationCandidateV0 =
                read_anchor_account(&base.svm, &authorization);
            (
                old_slot,
                state.identity.intent_digest,
                authorization,
                spend_authority,
            )
        })
        .collect::<Vec<_>>();
    entries.sort_unstable_by_key(|entry| entry.1);
    assert!(entries.windows(2).all(|pair| pair[0].1 < pair[1].1));

    let mut slot_remap = vec![0_u8; entries.len()];
    for (new_slot, (old_slot, _, _, _)) in entries.iter().enumerate() {
        slot_remap[*old_slot] = u8::try_from(new_slot).expect("authorization slot fits u8");
    }
    for declaration in declarations.iter_mut() {
        if declaration.authorization_slot_or_none != NONE_INDEX {
            declaration.authorization_slot_or_none =
                slot_remap[usize::from(declaration.authorization_slot_or_none)];
        }
    }

    let authorizations = entries
        .iter()
        .map(|(_, _, authorization, _)| *authorization)
        .collect::<Vec<_>>();
    let spend_authorities = entries
        .iter()
        .map(|(_, _, _, spend_authority)| *spend_authority)
        .collect::<Vec<_>>();
    let authorization_count = authorizations.len();
    let mut spend_control_offsets = vec![NONE_INDEX; authorization_count];
    let mut next_control = authorization_count;
    for (slot, spend_authority) in spend_authorities.iter().enumerate() {
        if spend_authority.is_some() {
            spend_control_offsets[slot] =
                u8::try_from(next_control).expect("spend control offset fits u8");
            next_control += 1;
        }
    }
    for declaration in declarations.iter_mut() {
        if declaration.authority_class == AUTHORITY_INTENT_FUNDED {
            declaration.spend_authority_control_offset_or_none =
                spend_control_offsets[usize::from(declaration.authorization_slot_or_none)];
            assert_ne!(
                declaration.spend_authority_control_offset_or_none,
                NONE_INDEX
            );
        }
    }
    (authorizations, spend_authorities)
}

fn compile_opaque_accounts(
    base: &mut DirectFixture,
    opaque: &[ReferenceOpaqueSpec],
) -> Vec<anchor_lang::prelude::Pubkey> {
    opaque
        .iter()
        .map(|spec| {
            let address = fixture_keypair(spec.address_tag).pubkey();
            install_raw_account(
                &mut base.svm,
                address,
                effect_engine_probe::ID,
                spec.data.clone(),
                false,
            );
            address
        })
        .collect()
}

fn fee_vault_declaration(
    asset_index: u8,
    fee_shard_index: u8,
) -> SettlementCapabilityRowCandidateV0 {
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

fn authorization_state_digest(
    state: &StoredAuthorizationCandidateV0,
    authorization: anchor_lang::prelude::Pubkey,
) -> [u8; 32] {
    compute_authorization_state_digest(AuthorizationStateDigestInputs {
        intent_digest: &state.identity.intent_digest,
        lifecycle: state.lifecycle,
        fill_sequence: state.fill_sequence,
        successful_fills: state.fill_sequence,
        remaining_fills: state
            .identity
            .max_fills
            .checked_sub(state.fill_sequence)
            .expect("stored fill sequence remains bounded"),
        capability_state_root: &state
            .capability_state_root()
            .expect("reference stored capability root"),
        fee_state_root: &state.fee_state_root().expect("reference stored fee root"),
        stored_authorization_key_or_zero: &authorization.to_bytes(),
    })
    .expect("reference stored authorization digest")
}

fn endpoint_snapshot(
    svm: &litesvm::LiteSVM,
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

fn protocol_fee(amount: u64) -> u64 {
    u64::try_from(
        u128::from(amount) * u128::from(DIRECT_FEE_RATE_NUMERATOR)
            / u128::from(DIRECT_FEE_RATE_DENOMINATOR),
    )
    .expect("reference protocol fee fits u64")
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
        data: control.encode().expect("encode reference stored control"),
    }
}
