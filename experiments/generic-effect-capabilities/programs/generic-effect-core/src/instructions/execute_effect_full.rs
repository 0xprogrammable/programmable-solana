//! General private execute orchestration.
//!
//! This is the canonical multi-witness/domain/fee execution path. It consumes
//! the frozen private Wire envelope directly; it is not a compatibility layer.

use anchor_lang::prelude::*;
use anchor_spl::token::spl_token;
use generic_effect_private_wire::{
    compute_asset_set_digest, compute_canonical_effect_digest,
    compute_core_verified_evidence_digest, compute_engine_attested_evidence_digest,
    compute_fee_principal_digest, compute_fee_rounding_group_digest,
    compute_intent_capability_terms_root, compute_intent_core_terms_root,
    compute_intent_credit_constraints_root, compute_intent_spend_seed, decode_effect_receipt,
    decode_execute_envelope, AssetBindingRowCandidateV0,
    AuthorizationCapabilityStateRowCandidateV0, CoreVerifiedEvidenceDigestInputs,
    CreditConstraintRowCandidateV0, DomainExecutionRowCandidateV0, EngineAssetRowCandidateV0,
    EngineAttestedEvidenceDigestInputs, EngineContextRowCandidateV0, EngineDomainRowCandidateV0,
    EngineIntentRowCandidateV0, EngineRequestCandidateV0, EngineRequestHeaderCandidateV0,
    ExecuteEnvelopeCandidateV0, FeeRoundingGroupRowCandidateV0, FeeShardDigestRowCandidateV0,
    IntentCapabilityTermRowCandidateV0, IntentCoreTermsDigestInputs, ENGINE_REQUEST_MAGIC,
    PHASE_TRANSITION, WIRE_VERSION, WITNESS_DIRECT_ACTOR, WITNESS_EXACT_DELEGATE,
    WITNESS_STORED_AUTHORIZATION,
};

use super::execution_preflight::{
    core_derived_authority_denylist, load_core_account, opaque_protected_keys,
    requested_privileges, validate_account_closure, validate_configuration,
    validate_loader_policy_closure,
};
use crate::{
    account_segments::{
        load_all_top_level_account_metas, snapshot_and_union, AccountSegments, EffectivePrivilege,
        SegmentCounts,
    },
    authorization::{
        authorization_view_from_ephemeral, authorization_view_from_stored,
        derive_exact_spend_authority, normalize_authorization_set, preview_stored_fill,
        resolve_inline_identity, validate_authorization_snapshot_rows,
        validate_exact_delegate_consumption, validate_stored_delegate_fill, AuthorizationView,
        ExactDelegateObservation, InlineIntentTerms, IntentFundedAuthorizationControl,
        NormalizedAuthorizationSet, ProtectedExecutionInputs, StoredCapabilityFill,
        StoredDelegateObservation, StoredFeeFill, StoredFillPreview, INTENT_SPEND_SEED,
    },
    capabilities::{
        validate_intent_term_binding, validate_opaque_capabilities, validate_plane_disjointness,
        validate_required_domain_descriptor_bindings, validate_settlement_capabilities,
        AssetProfileIdentity, CapabilityValidationContext, SettlementCapability,
    },
    constants::{
        ABSENT_INDEX, AUTHORITY_CORE_RESERVED_FEE_CREDIT, AUTHORITY_DOMAIN_ACCOUNTED,
        AUTHORITY_EXACT_EXTERNAL_CREDIT, AUTHORITY_INTENT_FUNDED_DEBIT, CALLBACK_ACCOUNT_INDEX,
        CONFIG_ACCOUNT_INDEX, DOMAIN_ACCOUNTING_SEED, ENGINE_PROGRAM_ACCOUNT_INDEX,
        EXPERIMENTAL_MAJOR, FEE_POLICY_ACCOUNT_INDEX, INSTRUCTIONS_SYSVAR_ACCOUNT_INDEX,
        MARKET_ACCOUNT_INDEX, RIGHT_CORE_RESERVED_FEE, RIGHT_CREDIT, RIGHT_DEBIT,
    },
    engine_identity::ValidatedEngineIdentity,
    error::CoreError,
    events::{CoreVerifiedEvidenceCandidateV0, EngineAttestedEvidenceCandidateV0, EvidenceClass},
    fees::{
        aggregate_fee_bases, derive_fee_assessment, fee_assessment_set_root, fee_shard_set_root,
        update_fee_liability, validate_user_fee_and_total_debit, AggregatedFeeBucket,
        FeeAssessment, FeeAssessmentContext, FeeBasisContribution, FeeBucketKey,
        FeeCollectionRoute, RatePolicy, RoundingMode,
    },
    moves::{
        derive_domain_accounting, validate_move_normal_form, verify_exact_observed_deltas,
        CanonicalMove, DomainAccountingDelta, DomainAccountingState, ObservedProtectedBalance,
        ProtectedFeeTransfer, ValidatedMovePlan,
    },
    runtime::{
        authenticate_actor_invocation, invoke_engine_transition,
        require_actor_invocation_privileges, AuthenticatedActorInvocation,
    },
    state::{
        commit_verified_stored_authorization_execution_exact, read_stored_authorization_compact,
        reserve_stored_authorization_execution_exact, serialize_account_exact,
        verify_stored_authorization_execution_for_commit_exact,
        AuthorizationCapabilityStateCandidateV0, AuthorizationFeeStateCandidateV0,
        CoreConfigurationCandidateV0, DomainAccountingCandidateV0,
        DomainAdmissionAccountCandidateV0, DomainDescriptorAccountCandidateV0,
        FeeLiabilityLedgerCandidateV0, FeePolicyCandidateV0, FeeShardAuthenticationExpectedV0,
        FeeShardDescriptorCandidateV0, IntentIdentityCandidateV0, MarketDescriptorCandidateV0,
        VerifiedStoredAuthorizationCommitV0,
    },
    token_settlement::{
        execute_classic_spl_transfers, load_classic_spl_endpoint, load_classic_spl_mint,
        ClassicSplEndpointSnapshot, ClassicSplMintSnapshot, ClassicSplTransfer,
        ObservedClassicSplDelta,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecuteEffectOutcome {
    pub request_digest: [u8; 32],
    pub canonical_effect_digest: [u8; 32],
    pub observed_delta_root: [u8; 32],
    pub core_verified_evidence_digest: [u8; 32],
    pub engine_attested_evidence_digest: [u8; 32],
    pub move_count: u8,
}

#[derive(Clone, Copy, Debug)]
struct ResolvedAsset {
    mint_position: usize,
    mint: ClassicSplMintSnapshot,
    binding: AssetBindingRowCandidateV0,
    binding_digest: [u8; 32],
}

#[derive(Clone, Debug)]
struct ResolvedAuthorization {
    witness_kind: u8,
    identity: IntentIdentityCandidateV0,
    view: AuthorizationView,
    terms: Vec<IntentCapabilityTermRowCandidateV0>,
    constraints: Vec<CreditConstraintRowCandidateV0>,
    capability_states: Vec<AuthorizationCapabilityStateCandidateV0>,
    fee_states: Vec<AuthorizationFeeStateCandidateV0>,
    local_to_global: Vec<u8>,
    stored_control_position: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
struct StoredRuntime {
    authorization_slot: u8,
    account_position: usize,
}

#[derive(Debug)]
struct ResolvedDomain {
    descriptor_position: usize,
    accounting_position: usize,
    descriptor: DomainDescriptorAccountCandidateV0,
    descriptor_digest: [u8; 32],
    admission_digest: [u8; 32],
    execution_row: DomainExecutionRowCandidateV0,
    accounting: Box<DomainAccountingCandidateV0>,
}

#[derive(Debug)]
struct ResolvedFeeShard {
    descriptor_position: usize,
    liability_position: usize,
    vault_capability_index: u8,
    asset_index: u8,
    descriptor: FeeShardDescriptorCandidateV0,
    liability: FeeLiabilityLedgerCandidateV0,
    digest_row: FeeShardDigestRowCandidateV0,
}

#[derive(Clone, Copy, Debug)]
struct SignerMaterial {
    prefix: &'static [u8],
    seed: [u8; 32],
    bump: [u8; 1],
}

#[derive(Clone, Copy, Debug)]
struct TransferPlanRow {
    source_capability_index: u8,
    destination_capability_index: u8,
    amount: u64,
    authority_position: usize,
    signer_material_index: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
struct DerivedFeeRuntime {
    assessment: FeeAssessment,
    authorization_slot: u8,
    funding_local_term_index: u8,
    source_capability_index: u8,
    destination_capability_index: u8,
}

#[derive(Debug)]
struct StoredPreviewRuntime {
    account_position: usize,
    verified_commit: VerifiedStoredAuthorizationCommitV0,
    preview: StoredFillPreview,
}

struct ValidatedEffectInputs<'a, 'info> {
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'info>],
    segments: &'a AccountSegments,
    effective: &'a [EffectivePrivilege],
    envelope: &'a ExecuteEnvelopeCandidateV0,
    market: &'a MarketDescriptorCandidateV0,
    fee_policy: &'a FeePolicyCandidateV0,
    validated_engine: &'a ValidatedEngineIdentity,
    assets: &'a [ResolvedAsset],
    endpoints: &'a [ClassicSplEndpointSnapshot],
    domains: &'a [ResolvedDomain],
    authorizations: &'a [ResolvedAuthorization],
    stored: &'a [StoredRuntime],
    fee_shards: &'a [ResolvedFeeShard],
    normalized: &'a NormalizedAuthorizationSet,
    market_binding_digest: &'a [u8; 32],
    domain_set_digest: &'a [u8; 32],
    opaque_capability_root: &'a [u8; 32],
    protected_execution_root: &'a [u8; 32],
    capabilities: &'a [SettlementCapability],
    current_slot: u64,
}

/// Executes the complete frozen generic-effect profile.
pub fn handle_execute_effect_full(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    complete_instruction_data: &[u8],
) -> Result<ExecuteEffectOutcome> {
    require_keys_eq!(*program_id, crate::ID, CoreError::InvalidWireEncoding);
    let envelope = decode_execute_envelope(complete_instruction_data)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    validate_base_shape(&envelope)?;
    let segments = AccountSegments::parse(
        SegmentCounts {
            loader_policy: envelope.header.loader_policy_account_count,
            domain_controls: envelope.header.domain_control_account_count,
            authorization_controls: envelope.header.authorization_account_count,
            protected_profile: envelope.header.protected_profile_account_count,
            fee_controls: envelope.header.fee_control_account_count,
            settlement: envelope.header.settlement_capability_count,
            opaque: envelope.header.opaque_capability_count,
        },
        accounts.len(),
    )?;

    let direct_actor_positions = envelope
        .authorization_snapshots
        .iter()
        .filter(|snapshot| snapshot.witness_kind == WITNESS_DIRECT_ACTOR)
        .map(|snapshot| {
            segments.authorization_controls.start
                + usize::from(snapshot.authorization_control_offset_or_none)
        })
        .collect::<Vec<_>>();
    let effective = if !direct_actor_positions.is_empty() {
        let forbidden = core_derived_authority_denylist(
            program_id,
            accounts,
            segments,
            &envelope,
            &direct_actor_positions,
        );
        let expected = requested_privileges(accounts, segments, &envelope)?;
        let mut transaction_root = None;
        for actor_position in &direct_actor_positions {
            let authenticated = authenticate_actor_invocation(
                program_id,
                accounts,
                complete_instruction_data,
                &accounts[INSTRUCTIONS_SYSVAR_ACCOUNT_INDEX],
                *actor_position,
                &forbidden,
            )?;
            require_actor_invocation_privileges(&authenticated, accounts, &expected)?;
            match authenticated {
                AuthenticatedActorInvocation::TransactionRoot(call) => {
                    if let Some(existing) = &transaction_root {
                        require!(
                            existing == &call,
                            CoreError::DirectAuthorizationNotTransactionRoot
                        );
                    } else {
                        transaction_root = Some(call);
                    }
                }
                AuthenticatedActorInvocation::ProgramActor { .. } => require!(
                    transaction_root.is_none(),
                    CoreError::DirectAuthorizationNotTransactionRoot
                ),
            }
        }
        if let Some(authenticated) = transaction_root {
            snapshot_and_union(accounts, &authenticated.effective_accounts)?
        } else {
            snapshot_and_union(accounts, &[])?
        }
    } else {
        snapshot_and_union(accounts, &[])?
    };
    validate_account_closure(program_id, accounts, &effective, segments, &envelope)?;
    let all_transaction_metas =
        load_all_top_level_account_metas(&accounts[INSTRUCTIONS_SYSVAR_ACCOUNT_INDEX])?;
    let loader_effective = snapshot_and_union(accounts, &all_transaction_metas)?;

    let config = load_core_account::<CoreConfigurationCandidateV0>(
        program_id,
        &accounts[CONFIG_ACCOUNT_INDEX],
        CoreConfigurationCandidateV0::SPACE,
    )?;
    validate_configuration(&config)?;
    let market = load_core_account::<MarketDescriptorCandidateV0>(
        program_id,
        &accounts[MARKET_ACCOUNT_INDEX],
        MarketDescriptorCandidateV0::SPACE,
    )?;
    let market_binding = market.binding_row(program_id, accounts[MARKET_ACCOUNT_INDEX].key)?;
    let market_binding_digest = market_binding
        .digest()
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    let fee_policy = load_core_account::<FeePolicyCandidateV0>(
        program_id,
        &accounts[FEE_POLICY_ACCOUNT_INDEX],
        FeePolicyCandidateV0::SPACE,
    )?;
    fee_policy.validate_digest(program_id)?;
    require!(
        fee_policy.policy_digest == market.fee_policy_digest
            && fee_policy.policy_digest == envelope.header.fee_policy_digest
            && fee_policy.revision == market.fee_policy_revision
            && config.classic_spl_profile_digest == market.protected_profile_digest
            && config.supported_engine_interface_digest == market.engine_interface_id
            && config.fee_policy_root == fee_policy.policy_digest,
        CoreError::InvalidWireEncoding
    );
    require_keys_eq!(
        *accounts[ENGINE_PROGRAM_ACCOUNT_INDEX].key,
        market.engine_program,
        CoreError::EngineAdmissionPolicyMismatch
    );
    let current_slot = Clock::get()?.slot;
    let validated_engine = validate_loader_policy_closure(
        program_id,
        accounts,
        &loader_effective,
        segments,
        &market,
        &envelope.header.expected_engine_loader_state_snapshot_digest,
        current_slot,
    )?;

    require!(
        envelope.header.expires_at_slot_exclusive != 0
            && current_slot < envelope.header.expires_at_slot_exclusive,
        CoreError::AuthorizationExpired
    );
    let assets = resolve_assets(accounts, segments, &envelope, &config)?;
    let asset_rows = assets.iter().map(|asset| asset.binding).collect::<Vec<_>>();
    let asset_set_digest = compute_asset_set_digest(&asset_rows)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    let endpoints = resolve_endpoints(accounts, segments, &envelope, &assets)?;
    let domains = resolve_domains(
        program_id,
        accounts,
        segments,
        &envelope,
        &market,
        &config,
        current_slot,
        market_binding_digest,
    )?;
    let domain_descriptor_digests = domains
        .iter()
        .map(|domain| domain.descriptor_digest)
        .collect::<Vec<_>>();
    let (authorizations, stored) = resolve_authorizations(
        program_id,
        accounts,
        segments,
        &envelope,
        &assets,
        &endpoints,
        &domain_descriptor_digests,
        market_binding_digest,
        validated_engine.loader_state_snapshot_digest,
        fee_policy.policy_digest,
        current_slot,
    )?;
    let capabilities = build_capabilities(
        program_id,
        accounts,
        segments,
        &effective,
        &envelope,
        &assets,
        &endpoints,
        &domains,
        &authorizations,
        fee_policy.revision,
    )?;
    let fee_shards = resolve_fee_shards(
        program_id, accounts, segments, &envelope, &market, &assets, &endpoints,
    )?;
    let protected_capability_set_digest = validate_settlement_capabilities(
        &capabilities,
        CapabilityValidationContext {
            core_program: *program_id,
            market: *accounts[MARKET_ACCOUNT_INDEX].key,
            classic_token_program: spl_token::ID,
            experimental_major: EXPERIMENTAL_MAJOR,
            intent_count: envelope.header.intent_count,
            asset_count: envelope.header.asset_count,
            domain_count: envelope.header.domain_count,
            fee_shard_count: envelope.header.fee_shard_count,
            fee_policy_revision: fee_policy.revision,
        },
    )?;
    validate_fee_funding_groups(&capabilities)?;
    validate_all_intent_term_mappings(&capabilities, &assets, &domains, &authorizations)?;
    let domain_rows = domains
        .iter()
        .map(|domain| domain.execution_row)
        .collect::<Vec<_>>();
    let domain_set_digest = generic_effect_private_wire::compute_domain_set_digest(
        &market_binding_digest,
        &domain_rows,
    )
    .map_err(|_| error!(CoreError::InvalidSettlementDomain))?;
    require!(
        domain_set_digest == envelope.header.domain_set_digest,
        CoreError::InvalidSettlementDomain
    );
    let normalized = normalize_authorization_set(
        &authorizations
            .iter()
            .map(|authorization| authorization.view.clone())
            .collect::<Vec<_>>(),
        &domain_set_digest,
    )?;
    require!(
        normalized.intent_set_digest == envelope.header.intent_set_digest,
        CoreError::AuthorizationIdentityMismatch
    );
    let fee_shard_rows = fee_shards
        .iter()
        .map(|shard| shard.digest_row)
        .collect::<Vec<_>>();
    let fee_shard_set_digest = fee_shard_set_root(&fee_shard_rows)?;
    let protected_execution_root =
        crate::authorization::derive_protected_execution_root(ProtectedExecutionInputs {
            core_program: *program_id,
            market_binding_digest,
            loader_state_snapshot_digest: validated_engine.loader_state_snapshot_digest,
            domain_set_digest,
            intent_set_digest: normalized.intent_set_digest,
            fee_policy_digest: fee_policy.policy_digest,
            asset_set_digest,
            authorization_view_set_digest: normalized.authorization_view_set_digest,
            fee_shard_set_digest,
            protected_capability_set_digest,
        })?;
    require!(
        protected_execution_root == envelope.header.protected_execution_root,
        CoreError::AuthorizationSnapshotMismatch
    );
    let protected_keys = opaque_protected_keys(accounts, segments, program_id, &validated_engine);
    let (opaque_capabilities, opaque_capability_root) = validate_opaque_capabilities(
        &effective[segments.opaque.start..segments.opaque.end],
        &protected_keys,
        program_id,
        &spl_token::ID,
        &anchor_spl::token_2022::ID,
    )?;
    validate_plane_disjointness(&capabilities, &opaque_capabilities)?;
    require!(
        opaque_capability_root == envelope.header.expected_opaque_capability_root,
        CoreError::InvalidWireEncoding
    );

    let validated = ValidatedEffectInputs {
        program_id,
        accounts,
        segments: &segments,
        effective: &effective,
        envelope: &envelope,
        market: &market,
        fee_policy: &fee_policy,
        validated_engine: &validated_engine,
        assets: &assets,
        endpoints: &endpoints,
        domains: &domains,
        authorizations: &authorizations,
        stored: &stored,
        fee_shards: &fee_shards,
        normalized: &normalized,
        market_binding_digest: &market_binding_digest,
        domain_set_digest: &domain_set_digest,
        opaque_capability_root: &opaque_capability_root,
        protected_execution_root: &protected_execution_root,
        capabilities: &capabilities,
        current_slot,
    };
    execute_validated_effect(&validated)
}

fn validate_base_shape(envelope: &ExecuteEnvelopeCandidateV0) -> Result<()> {
    let header = &envelope.header;
    require_eq!(
        header.loader_policy_account_count,
        1,
        CoreError::AccountSegmentLengthMismatch
    );
    require!(
        header.intent_count != 0
            && header.asset_count != 0
            && header.settlement_capability_count != 0
            && header.maximum_engine_moves != 0,
        CoreError::ExperimentLimitExceeded
    );
    let expected_profile_count = usize::from(header.asset_count)
        .checked_add(1)
        .ok_or(CoreError::ArithmeticOverflow)?;
    require_eq!(
        usize::from(header.protected_profile_account_count),
        expected_profile_count,
        CoreError::MoveAssetProfileMismatch
    );
    Ok(())
}

fn resolve_assets(
    accounts: &[AccountInfo<'_>],
    segments: AccountSegments,
    envelope: &ExecuteEnvelopeCandidateV0,
    config: &CoreConfigurationCandidateV0,
) -> Result<Vec<ResolvedAsset>> {
    let token_program_position = segments.protected_profile.start;
    require_keys_eq!(
        *accounts[token_program_position].key,
        spl_token::ID,
        CoreError::MoveAssetProfileMismatch
    );
    let mut assets: Vec<ResolvedAsset> =
        Vec::with_capacity(usize::from(envelope.header.asset_count));
    for asset_index in 0..usize::from(envelope.header.asset_count) {
        let mint_position = token_program_position + 1 + asset_index;
        let mint = load_classic_spl_mint(&accounts[mint_position])?;
        let binding = AssetBindingRowCandidateV0 {
            wire_version: WIRE_VERSION,
            flags: 0,
            decimals: mint.decimals,
            reserved: 0,
            asset_identity: mint.key.to_bytes(),
            asset_program: spl_token::ID.to_bytes(),
            settlement_profile_digest: config.classic_spl_profile_digest,
        };
        let binding_digest = binding
            .digest()
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        require!(
            assets
                .last()
                .is_none_or(|previous| previous.binding_digest < binding_digest),
            CoreError::InvalidWireEncoding
        );
        assets.push(ResolvedAsset {
            mint_position,
            mint,
            binding,
            binding_digest,
        });
    }
    Ok(assets)
}

fn resolve_endpoints(
    accounts: &[AccountInfo<'_>],
    segments: AccountSegments,
    envelope: &ExecuteEnvelopeCandidateV0,
    assets: &[ResolvedAsset],
) -> Result<Vec<ClassicSplEndpointSnapshot>> {
    envelope
        .settlement_capabilities
        .iter()
        .enumerate()
        .map(|(capability_index, declaration)| {
            let endpoint =
                load_classic_spl_endpoint(&accounts[segments.settlement.start + capability_index])?;
            let asset = assets
                .get(usize::from(declaration.asset_index))
                .ok_or(CoreError::MoveAssetProfileMismatch)?;
            require_keys_eq!(
                endpoint.mint,
                asset.mint.key,
                CoreError::MoveAssetProfileMismatch
            );
            Ok(endpoint)
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn resolve_domains(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    segments: AccountSegments,
    envelope: &ExecuteEnvelopeCandidateV0,
    market: &MarketDescriptorCandidateV0,
    config: &CoreConfigurationCandidateV0,
    current_slot: u64,
    market_binding_digest: [u8; 32],
) -> Result<Vec<ResolvedDomain>> {
    let mut domains: Vec<ResolvedDomain> = Vec::with_capacity(envelope.domain_controls.len());
    for (domain_index, control) in envelope.domain_controls.iter().enumerate() {
        let domain = resolve_one_domain(
            program_id,
            accounts,
            segments,
            domain_index,
            control,
            market,
            config,
            current_slot,
            market_binding_digest,
        )?;
        require!(
            domains
                .last()
                .is_none_or(|previous| previous.descriptor_digest < domain.descriptor_digest),
            CoreError::InvalidSettlementDomain
        );
        domains.push(domain);
    }
    Ok(domains)
}

#[allow(clippy::too_many_arguments)]
#[inline(never)]
fn resolve_one_domain(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    segments: AccountSegments,
    domain_index: usize,
    control: &generic_effect_private_wire::DomainControlRowCandidateV0,
    market: &MarketDescriptorCandidateV0,
    config: &CoreConfigurationCandidateV0,
    current_slot: u64,
    market_binding_digest: [u8; 32],
) -> Result<ResolvedDomain> {
    let descriptor_position =
        segments.domain_controls.start + usize::from(control.descriptor_control_offset);
    let accounting_position =
        segments.domain_controls.start + usize::from(control.accounting_control_offset);
    let descriptor_key = *accounts[descriptor_position].key;
    let accounting_key = *accounts[accounting_position].key;
    let descriptor = load_core_account::<DomainDescriptorAccountCandidateV0>(
        program_id,
        &accounts[descriptor_position],
        DomainDescriptorAccountCandidateV0::SPACE,
    )?;
    let descriptor_digest = descriptor.digest(program_id)?;
    require!(
        descriptor_digest != [0; 32]
            && descriptor.protected_profile_digest == config.classic_spl_profile_digest
            && descriptor.protected_profile_digest == market.protected_profile_digest,
        CoreError::InvalidSettlementDomain
    );
    let accounting = Box::new(load_core_account::<DomainAccountingCandidateV0>(
        program_id,
        &accounts[accounting_position],
        DomainAccountingCandidateV0::SPACE,
    )?);
    accounting.validate_authenticated(
        program_id,
        &accounting_key,
        &descriptor_key,
        descriptor.domain_revision,
    )?;

    let (admission_account_or_zero, admission_digest) = resolve_domain_admission(
        program_id,
        accounts,
        segments,
        control,
        &descriptor,
        descriptor_key,
        descriptor_digest,
        market,
        config,
        current_slot,
        market_binding_digest,
    )?;
    let execution_row = DomainExecutionRowCandidateV0 {
        domain_index: u8::try_from(domain_index).map_err(|_| CoreError::ExperimentLimitExceeded)?,
        admission_kind: descriptor.rule_kind,
        domain_descriptor_key: descriptor_key.to_bytes(),
        domain_descriptor_digest: descriptor_digest,
        domain_revision: descriptor.domain_revision,
        admission_account_or_zero,
        admission_digest,
        accounting_account: accounting_key.to_bytes(),
        accounting_profile_digest: descriptor.accounting_profile_digest,
    };
    execution_row
        .digest(&market_binding_digest)
        .map_err(|_| error!(CoreError::InvalidSettlementDomain))?;
    Ok(ResolvedDomain {
        descriptor_position,
        accounting_position,
        descriptor,
        descriptor_digest,
        admission_digest,
        execution_row,
        accounting,
    })
}

#[allow(clippy::too_many_arguments)]
#[inline(never)]
fn resolve_domain_admission(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    segments: AccountSegments,
    control: &generic_effect_private_wire::DomainControlRowCandidateV0,
    descriptor: &DomainDescriptorAccountCandidateV0,
    descriptor_key: Pubkey,
    descriptor_digest: [u8; 32],
    market: &MarketDescriptorCandidateV0,
    config: &CoreConfigurationCandidateV0,
    current_slot: u64,
    market_binding_digest: [u8; 32],
) -> Result<([u8; 32], [u8; 32])> {
    match descriptor.rule_kind {
        generic_effect_private_wire::DOMAIN_RULE_OPEN => {
            require_eq!(
                control.admission_control_offset_or_none,
                ABSENT_INDEX,
                CoreError::InvalidSettlementDomain
            );
            let open_rule = generic_effect_private_wire::compute_open_domain_rule_digest()
                .map_err(|_| error!(CoreError::InvalidSettlementDomain))?;
            require!(
                descriptor.admission_rule_digest == open_rule,
                CoreError::InvalidSettlementDomain
            );
            let admission_digest =
                generic_effect_private_wire::compute_open_domain_admission_digest(
                    &descriptor_digest,
                    &market_binding_digest,
                )
                .map_err(|_| error!(CoreError::InvalidSettlementDomain))?;
            Ok(([0; 32], admission_digest))
        }
        generic_effect_private_wire::DOMAIN_RULE_CLOSED => {
            require!(
                control.admission_control_offset_or_none != ABSENT_INDEX,
                CoreError::InvalidSettlementDomain
            );
            let admission_position = segments.domain_controls.start
                + usize::from(control.admission_control_offset_or_none);
            let admission_key = *accounts[admission_position].key;
            let admission = load_core_account::<DomainAdmissionAccountCandidateV0>(
                program_id,
                &accounts[admission_position],
                DomainAdmissionAccountCandidateV0::SPACE,
            )?;
            let open_rule = generic_effect_private_wire::compute_open_domain_rule_digest()
                .map_err(|_| error!(CoreError::InvalidSettlementDomain))?;
            require!(
                descriptor.admission_rule_digest != [0; 32]
                    && descriptor.admission_rule_digest != open_rule,
                CoreError::InvalidSettlementDomain
            );
            let admission_digest = admission.validate_authenticated(
                program_id,
                &admission_key,
                &descriptor_key,
                descriptor,
                accounts[MARKET_ACCOUNT_INDEX].key,
                market,
                &config.classic_spl_profile_digest,
                current_slot,
            )?;
            Ok((admission_key.to_bytes(), admission_digest))
        }
        _ => err!(CoreError::InvalidSettlementDomain),
    }
}

fn resolve_fee_shards(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    segments: AccountSegments,
    envelope: &ExecuteEnvelopeCandidateV0,
    market: &MarketDescriptorCandidateV0,
    assets: &[ResolvedAsset],
    endpoints: &[ClassicSplEndpointSnapshot],
) -> Result<Vec<ResolvedFeeShard>> {
    let mut shards = Vec::with_capacity(envelope.fee_shards.len());
    for (shard_index, row) in envelope.fee_shards.iter().enumerate() {
        let descriptor_position =
            segments.fee_controls.start + usize::from(row.descriptor_control_offset);
        let liability_position =
            segments.fee_controls.start + usize::from(row.liability_control_offset);
        let vault_capability_index = usize::from(row.vault_settlement_capability_index);
        let asset = assets
            .get(usize::from(row.asset_index))
            .ok_or(CoreError::InvalidSettlementFeeShard)?;
        let vault = endpoints
            .get(vault_capability_index)
            .ok_or(CoreError::InvalidSettlementFeeShard)?;
        let descriptor = load_core_account::<FeeShardDescriptorCandidateV0>(
            program_id,
            &accounts[descriptor_position],
            FeeShardDescriptorCandidateV0::SPACE,
        )?;
        let liability = load_core_account::<FeeLiabilityLedgerCandidateV0>(
            program_id,
            &accounts[liability_position],
            FeeLiabilityLedgerCandidateV0::SPACE,
        )?;
        let descriptor_row = descriptor.validate_authenticated(
            program_id,
            accounts[descriptor_position].key,
            accounts[MARKET_ACCOUNT_INDEX].key,
            market,
            accounts[liability_position].key,
            &liability,
            &FeeShardAuthenticationExpectedV0 {
                shard_index: u8::try_from(shard_index)
                    .map_err(|_| CoreError::ExperimentLimitExceeded)?,
                asset_identity: asset.mint.key,
                asset_program: spl_token::ID,
                settlement_profile_digest: asset.binding.settlement_profile_digest,
                vault: vault.key,
            },
        )?;
        let digest_row = FeeShardDigestRowCandidateV0 {
            shard_index: u8::try_from(shard_index)
                .map_err(|_| CoreError::ExperimentLimitExceeded)?,
            asset_index: row.asset_index,
            vault_settlement_capability_index: row.vault_settlement_capability_index,
            flags: 0,
            descriptor_key: accounts[descriptor_position].key.to_bytes(),
            descriptor_digest: descriptor.descriptor_digest,
            liability_key: accounts[liability_position].key.to_bytes(),
            vault_key: vault.key.to_bytes(),
            asset_binding_digest: asset.binding_digest,
            fee_policy_digest: descriptor_row.fee_policy_digest,
            recipient_policy_digest: descriptor_row.recipient_policy_digest,
            fee_policy_revision: descriptor_row.fee_policy_revision,
            liability_before: liability.liability,
        };
        digest_row
            .encode()
            .map_err(|_| error!(CoreError::InvalidSettlementFeeShard))?;
        shards.push(ResolvedFeeShard {
            descriptor_position,
            liability_position,
            vault_capability_index: row.vault_settlement_capability_index,
            asset_index: row.asset_index,
            descriptor,
            liability,
            digest_row,
        });
    }
    Ok(shards)
}

#[allow(clippy::too_many_arguments)]
fn resolve_authorizations(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    segments: AccountSegments,
    envelope: &ExecuteEnvelopeCandidateV0,
    assets: &[ResolvedAsset],
    endpoints: &[ClassicSplEndpointSnapshot],
    authenticated_domain_descriptor_digests: &[[u8; 32]],
    market_binding_digest: [u8; 32],
    loader_state_snapshot_digest: [u8; 32],
    fee_policy_digest: [u8; 32],
    current_slot: u64,
) -> Result<(Vec<ResolvedAuthorization>, Vec<StoredRuntime>)> {
    let source_controls = envelope
        .settlement_capabilities
        .iter()
        .filter(|row| row.authority_class == generic_effect_private_wire::AUTHORITY_INTENT_FUNDED)
        .map(|row| IntentFundedAuthorizationControl {
            authorization_slot: row.authorization_slot_or_none,
            spend_authority_control_offset_or_none: row.spend_authority_control_offset_or_none,
            settlement_flags: row.flags,
        })
        .collect::<Vec<_>>();
    validate_authorization_snapshot_rows(
        &envelope.authorization_snapshots,
        &envelope.inline_intent_identities,
        segments.authorization_controls.len(),
        &source_controls,
    )?;

    let mut resolved = Vec::with_capacity(envelope.authorization_snapshots.len());
    let mut stored = Vec::new();
    for snapshot in &envelope.authorization_snapshots {
        let slot = snapshot.authorization_slot;
        match snapshot.witness_kind {
            WITNESS_DIRECT_ACTOR | WITNESS_EXACT_DELEGATE => {
                let inline = *envelope
                    .inline_intent_identities
                    .get(usize::from(snapshot.inline_identity_index_or_none))
                    .ok_or(CoreError::InvalidAuthorizationSlots)?;
                let (terms, local_to_global) = reconstruct_ephemeral_terms(
                    slot,
                    envelope,
                    assets,
                    endpoints,
                    authenticated_domain_descriptor_digests,
                )?;
                validate_required_domain_descriptor_bindings(
                    &terms,
                    authenticated_domain_descriptor_digests,
                )?;
                let capability_terms_root = compute_intent_capability_terms_root(&terms)
                    .map_err(|_| error!(CoreError::AuthorizationIdentityMismatch))?;
                let constraints = Vec::<CreditConstraintRowCandidateV0>::new();
                let credit_constraints_root = compute_intent_credit_constraints_root(&constraints)
                    .map_err(|_| error!(CoreError::AuthorizationIdentityMismatch))?;
                let core_terms_root = compute_intent_core_terms_root(IntentCoreTermsDigestInputs {
                    maximum_successful_fills: 1,
                    capability_terms_root: &capability_terms_root,
                    credit_constraints_root: &credit_constraints_root,
                })
                .map_err(|_| error!(CoreError::AuthorizationIdentityMismatch))?;
                let identity = resolve_inline_identity(
                    program_id,
                    &inline,
                    InlineIntentTerms {
                        experimental_major: crate::constants::EXPERIMENTAL_MAJOR,
                        market_binding_digest,
                        loader_state_snapshot_digest,
                        fee_policy_digest,
                        core_terms_root,
                        max_fills: 1,
                    },
                )?;
                require!(
                    envelope.header.expires_at_slot_exclusive <= identity.expires_at_slot_exclusive,
                    CoreError::AuthorizationExpired
                );
                if snapshot.witness_kind == WITNESS_DIRECT_ACTOR {
                    let actor_position = segments.authorization_controls.start
                        + usize::from(snapshot.authorization_control_offset_or_none);
                    require_keys_eq!(
                        *accounts[actor_position].key,
                        identity.actor,
                        CoreError::AuthorizationIdentityMismatch
                    );
                }
                let capability_states = terms
                    .iter()
                    .map(|term| AuthorizationCapabilityStateRowCandidateV0 {
                        local_term_index: term.intent_local_term_index,
                        reserved_0: 0,
                        flags: term.flags,
                        initial_maximum_engine_debit: term.maximum_engine_debit,
                        initial_minimum_credit: term.minimum_credit,
                        initial_maximum_total_debit: term.maximum_total_debit,
                        remaining_total_debit: term.maximum_total_debit,
                        cumulative_engine_debit: 0,
                        cumulative_fee_debit: 0,
                        cumulative_credit: 0,
                    })
                    .collect::<Vec<_>>();
                let local_endpoints = terms
                    .iter()
                    .map(|term| Pubkey::new_from_array(term.endpoint_key))
                    .collect::<Vec<_>>();
                let view = authorization_view_from_ephemeral(
                    program_id,
                    &identity,
                    &capability_states,
                    &[],
                    &local_endpoints,
                    current_slot,
                )?;
                resolved.push(ResolvedAuthorization {
                    witness_kind: snapshot.witness_kind,
                    identity,
                    view,
                    terms,
                    constraints,
                    capability_states: capability_states
                        .into_iter()
                        .map(AuthorizationCapabilityStateCandidateV0::from_wire_row)
                        .collect(),
                    fee_states: Vec::new(),
                    local_to_global,
                    stored_control_position: None,
                });
            }
            WITNESS_STORED_AUTHORIZATION => {
                let account_position = segments.authorization_controls.start
                    + usize::from(snapshot.authorization_control_offset_or_none);
                let state =
                    read_stored_authorization_compact(&accounts[account_position], program_id)?;
                let view = authorization_view_from_stored(
                    program_id,
                    &state,
                    accounts[account_position].key,
                    current_slot,
                )?;
                require_eq!(
                    state.header.fill_sequence,
                    snapshot.expected_fill_sequence,
                    CoreError::AuthorizationFillSequenceMismatch
                );
                require!(
                    state.identity.market_binding_digest == market_binding_digest
                        && state.identity.loader_state_snapshot_digest
                            == loader_state_snapshot_digest
                        && state.identity.fee_policy_digest == fee_policy_digest
                        && envelope.header.expires_at_slot_exclusive
                            <= state.identity.expires_at_slot_exclusive,
                    CoreError::AuthorizationIdentityMismatch
                );
                let terms = state
                    .immutable_terms
                    .iter()
                    .map(|term| term.wire_row())
                    .collect::<Result<Vec<_>>>()?;
                let constraints = state
                    .credit_constraints
                    .iter()
                    .map(|constraint| constraint.wire_row())
                    .collect::<Result<Vec<_>>>()?;
                validate_required_domain_descriptor_bindings(
                    &terms,
                    authenticated_domain_descriptor_digests,
                )?;
                let local_to_global = map_stored_terms(slot, &terms, envelope)?;
                resolved.push(ResolvedAuthorization {
                    witness_kind: snapshot.witness_kind,
                    identity: state.identity,
                    view,
                    terms,
                    constraints,
                    capability_states: state.capabilities,
                    fee_states: state.fee_states,
                    local_to_global,
                    stored_control_position: Some(account_position),
                });
                stored.push(StoredRuntime {
                    authorization_slot: slot,
                    account_position,
                });
            }
            _ => return err!(CoreError::UnsupportedAuthorizationWitness),
        }
    }
    Ok((resolved, stored))
}

fn reconstruct_ephemeral_terms(
    authorization_slot: u8,
    envelope: &ExecuteEnvelopeCandidateV0,
    assets: &[ResolvedAsset],
    endpoints: &[ClassicSplEndpointSnapshot],
    authenticated_domain_descriptor_digests: &[[u8; 32]],
) -> Result<(Vec<IntentCapabilityTermRowCandidateV0>, Vec<u8>)> {
    let mut mapped = envelope
        .settlement_capabilities
        .iter()
        .enumerate()
        .filter(|(_, row)| row.authorization_slot_or_none == authorization_slot)
        .map(|(global_index, row)| (row.intent_local_term_index_or_none, global_index, row))
        .collect::<Vec<_>>();
    mapped.sort_unstable_by_key(|(local_index, _, _)| *local_index);
    require!(!mapped.is_empty(), CoreError::InvalidAuthorizationSlots);
    let mut terms = Vec::with_capacity(mapped.len());
    let mut local_to_global = Vec::with_capacity(mapped.len());
    for (position, (local_index, global_index, row)) in mapped.iter().enumerate() {
        require_eq!(
            usize::from(*local_index),
            position,
            CoreError::InvalidAuthorizationSlots
        );
        let asset = assets
            .get(usize::from(row.asset_index))
            .ok_or(CoreError::MoveAssetProfileMismatch)?;
        let endpoint = endpoints
            .get(*global_index)
            .ok_or(CoreError::InvalidAuthorizationSlots)?;
        let required_domain_descriptor_digest_or_zero = if row.domain_index_or_none == ABSENT_INDEX
        {
            [0; 32]
        } else {
            *authenticated_domain_descriptor_digests
                .get(usize::from(row.domain_index_or_none))
                .ok_or(CoreError::InvalidSettlementDomain)?
        };
        terms.push(IntentCapabilityTermRowCandidateV0 {
            intent_local_term_index: *local_index,
            authority_class: row.authority_class,
            fee_class: row.fee_class,
            flags: row.flags,
            rights_bits: row.rights_bits,
            endpoint_key: endpoint.key.to_bytes(),
            asset_binding_digest: asset.binding_digest,
            required_domain_descriptor_digest_or_zero,
            maximum_engine_debit: row.maximum_engine_debit,
            maximum_total_debit: row.maximum_total_debit,
            minimum_credit: row.minimum_credit,
            maximum_protocol_fee: row.maximum_protocol_fee,
        });
        local_to_global
            .push(u8::try_from(*global_index).map_err(|_| CoreError::ExperimentLimitExceeded)?);
    }
    Ok((terms, local_to_global))
}

fn map_stored_terms(
    authorization_slot: u8,
    terms: &[IntentCapabilityTermRowCandidateV0],
    envelope: &ExecuteEnvelopeCandidateV0,
) -> Result<Vec<u8>> {
    let mut mapping = Vec::with_capacity(terms.len());
    for (local_index, _) in terms.iter().enumerate() {
        let matches = envelope
            .settlement_capabilities
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                row.authorization_slot_or_none == authorization_slot
                    && usize::from(row.intent_local_term_index_or_none) == local_index
            })
            .map(|(global_index, _)| global_index)
            .collect::<Vec<_>>();
        require_eq!(matches.len(), 1, CoreError::InvalidAuthorizationSlots);
        mapping.push(u8::try_from(matches[0]).map_err(|_| CoreError::ExperimentLimitExceeded)?);
    }
    require_eq!(
        envelope
            .settlement_capabilities
            .iter()
            .filter(|row| row.authorization_slot_or_none == authorization_slot)
            .count(),
        terms.len(),
        CoreError::InvalidAuthorizationSlots
    );
    Ok(mapping)
}

#[allow(clippy::too_many_arguments)]
fn build_capabilities(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    segments: AccountSegments,
    effective: &[EffectivePrivilege],
    envelope: &ExecuteEnvelopeCandidateV0,
    assets: &[ResolvedAsset],
    endpoints: &[ClassicSplEndpointSnapshot],
    domains: &[ResolvedDomain],
    authorizations: &[ResolvedAuthorization],
    fee_policy_revision: u64,
) -> Result<Vec<SettlementCapability>> {
    envelope
        .settlement_capabilities
        .iter()
        .enumerate()
        .map(|(position, declaration)| {
            let endpoint = *endpoints
                .get(position)
                .ok_or(CoreError::InvalidSettlementRights)?;
            let asset = assets
                .get(usize::from(declaration.asset_index))
                .ok_or(CoreError::MoveAssetProfileMismatch)?;
            let domain = if declaration.domain_index_or_none == ABSENT_INDEX {
                None
            } else {
                let resolved = domains
                    .get(usize::from(declaration.domain_index_or_none))
                    .ok_or(CoreError::InvalidSettlementDomain)?;
                Some(crate::capabilities::DomainCapabilityIdentity {
                    domain_index: declaration.domain_index_or_none,
                    domain_descriptor: *accounts[resolved.descriptor_position].key,
                    domain_revision: resolved.descriptor.domain_revision,
                    admission_digest: resolved.admission_digest,
                    accounting_slot: declaration.domain_accounting_slot_or_none,
                })
            };
            let mut accounted_before_or_zero = 0_u128;
            let transfer_authority_or_zero = match declaration.authority_class {
                AUTHORITY_INTENT_FUNDED_DEBIT => {
                    let slot = usize::from(declaration.authorization_slot_or_none);
                    let authorization = authorizations
                        .get(slot)
                        .ok_or(CoreError::InvalidAuthorizationSlots)?;
                    require_keys_eq!(
                        endpoint.authority,
                        authorization.identity.actor,
                        CoreError::AuthorizationIdentityMismatch
                    );
                    let snapshot = envelope
                        .authorization_snapshots
                        .get(slot)
                        .ok_or(CoreError::InvalidAuthorizationSlots)?;
                    match snapshot.witness_kind {
                        WITNESS_DIRECT_ACTOR => {
                            require_eq!(
                                declaration.spend_authority_control_offset_or_none,
                                ABSENT_INDEX,
                                CoreError::InvalidAuthorizationSlots
                            );
                            let actor_position = segments.authorization_controls.start
                                + usize::from(snapshot.authorization_control_offset_or_none);
                            require_keys_eq!(
                                *accounts[actor_position].key,
                                authorization.identity.actor,
                                CoreError::AuthorizationIdentityMismatch
                            );
                            authorization.identity.actor
                        }
                        WITNESS_EXACT_DELEGATE | WITNESS_STORED_AUTHORIZATION => {
                            let control_position = segments.authorization_controls.start
                                + usize::from(declaration.spend_authority_control_offset_or_none);
                            let (expected, _) = derive_exact_spend_authority(
                                program_id,
                                &authorization.identity.intent_digest,
                                &endpoint.key,
                            )?;
                            require_keys_eq!(
                                *accounts[control_position].key,
                                expected,
                                CoreError::ExactDelegateConsumptionMismatch
                            );
                            require!(
                                endpoint.delegate == Some(expected)
                                    && endpoint.delegated_amount != 0,
                                CoreError::ExactDelegateConsumptionMismatch
                            );
                            expected
                        }
                        _ => return err!(CoreError::UnsupportedAuthorizationWitness),
                    }
                }
                AUTHORITY_EXACT_EXTERNAL_CREDIT => Pubkey::default(),
                AUTHORITY_DOMAIN_ACCOUNTED => {
                    let resolved = domains
                        .get(usize::from(declaration.domain_index_or_none))
                        .ok_or(CoreError::InvalidSettlementDomain)?;
                    let accounting_slot = resolved
                        .accounting
                        .assets
                        .get(usize::from(declaration.domain_accounting_slot_or_none))
                        .filter(|_| {
                            usize::from(declaration.domain_accounting_slot_or_none)
                                < usize::from(resolved.accounting.asset_count)
                        })
                        .ok_or(CoreError::InvalidSettlementDomain)?;
                    require!(
                        accounting_slot.asset_identity == asset.mint.key
                            && accounting_slot.asset_program == spl_token::ID
                            && accounting_slot.settlement_profile_digest
                                == asset.binding.settlement_profile_digest,
                        CoreError::MoveAssetProfileMismatch
                    );
                    require_keys_eq!(
                        endpoint.authority,
                        *accounts[resolved.accounting_position].key,
                        CoreError::InvalidSettlementDomain
                    );
                    require!(
                        endpoint.delegate.is_none() && endpoint.close_authority.is_none(),
                        CoreError::InvalidSettlementDomain
                    );
                    accounted_before_or_zero = accounting_slot.accounted_amount;
                    if declaration.rights_bits & RIGHT_DEBIT != 0 {
                        *accounts[resolved.accounting_position].key
                    } else {
                        Pubkey::default()
                    }
                }
                _ => Pubkey::default(),
            };
            Ok(SettlementCapability {
                position: u8::try_from(position).map_err(|_| CoreError::ExperimentLimitExceeded)?,
                declaration: *declaration,
                core_program: *program_id,
                experimental_major: EXPERIMENTAL_MAJOR,
                market: *accounts[MARKET_ACCOUNT_INDEX].key,
                endpoint: effective[segments.settlement.start + position],
                transfer_authority_or_zero,
                asset: AssetProfileIdentity {
                    asset_identity: asset.mint.key,
                    asset_program: spl_token::ID,
                    settlement_profile_digest: asset.binding.settlement_profile_digest,
                },
                domain,
                fee_policy_revision,
                lifecycle_digest: endpoint.lifecycle_digest()?,
                accounted_before_or_zero,
            })
        })
        .collect()
}

fn validate_all_intent_term_mappings(
    capabilities: &[SettlementCapability],
    assets: &[ResolvedAsset],
    domains: &[ResolvedDomain],
    authorizations: &[ResolvedAuthorization],
) -> Result<()> {
    for (slot, authorization) in authorizations.iter().enumerate() {
        require_eq!(
            authorization.terms.len(),
            authorization.local_to_global.len(),
            CoreError::InvalidAuthorizationSlots
        );
        for (local_index, global_index) in authorization.local_to_global.iter().enumerate() {
            let capability = capabilities
                .get(usize::from(*global_index))
                .ok_or(CoreError::InvalidAuthorizationSlots)?;
            require_eq!(
                usize::from(capability.declaration.authorization_slot_or_none),
                slot,
                CoreError::InvalidAuthorizationSlots
            );
            require_eq!(
                usize::from(capability.declaration.intent_local_term_index_or_none),
                local_index,
                CoreError::InvalidAuthorizationSlots
            );
            let asset = assets
                .get(usize::from(capability.declaration.asset_index))
                .ok_or(CoreError::MoveAssetProfileMismatch)?;
            let term = authorization
                .terms
                .get(local_index)
                .ok_or(CoreError::InvalidAuthorizationSlots)?;
            let required_domain_descriptor_digest_or_zero =
                if capability.declaration.domain_index_or_none == ABSENT_INDEX {
                    [0; 32]
                } else {
                    domains
                        .get(usize::from(capability.declaration.domain_index_or_none))
                        .ok_or(CoreError::InvalidSettlementDomain)?
                        .descriptor_digest
                };
            validate_intent_term_binding(
                capability,
                &asset.binding_digest,
                &required_domain_descriptor_digest_or_zero,
                term,
            )?;
        }
    }
    Ok(())
}

fn validate_fee_funding_groups(capabilities: &[SettlementCapability]) -> Result<()> {
    for capability in capabilities.iter().filter(|capability| {
        capability.declaration.authority_class == AUTHORITY_INTENT_FUNDED_DEBIT
    }) {
        let funding_count = capabilities
            .iter()
            .filter(|candidate| {
                candidate.declaration.authority_class == AUTHORITY_INTENT_FUNDED_DEBIT
                    && candidate.authorization_slot() == capability.authorization_slot()
                    && candidate.asset == capability.asset
                    && candidate.declaration.fee_class == capability.declaration.fee_class
                    && candidate.is_fee_funding()
            })
            .count();
        require_eq!(funding_count, 1, CoreError::InvalidSettlementFeeShard);
    }
    Ok(())
}

fn execute_validated_effect(
    inputs: &ValidatedEffectInputs<'_, '_>,
) -> Result<ExecuteEffectOutcome> {
    let program_id = inputs.program_id;
    let accounts = inputs.accounts;
    let segments = *inputs.segments;
    let effective = inputs.effective;
    let envelope = inputs.envelope;
    let market = inputs.market;
    let fee_policy = inputs.fee_policy;
    let validated_engine = inputs.validated_engine;
    let assets = inputs.assets;
    let endpoints = inputs.endpoints;
    let domains = inputs.domains;
    let authorizations = inputs.authorizations;
    let stored = inputs.stored;
    let fee_shards = inputs.fee_shards;
    let normalized = inputs.normalized;
    let market_binding_digest = *inputs.market_binding_digest;
    let domain_set_digest = *inputs.domain_set_digest;
    let opaque_capability_root = *inputs.opaque_capability_root;
    let protected_execution_root = *inputs.protected_execution_root;
    let capabilities = inputs.capabilities;
    let current_slot = inputs.current_slot;
    let request = build_engine_request(
        envelope,
        market,
        fee_policy,
        validated_engine,
        assets,
        endpoints,
        domains,
        authorizations,
        capabilities,
        market_binding_digest,
        domain_set_digest,
        opaque_capability_root,
        protected_execution_root,
    )?;
    let (engine_data, request_digest, callback_seed) = request
        .encode_digest_and_callback_seed(accounts[ENGINE_PROGRAM_ACCOUNT_INDEX].key)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;

    // The request commits the Active pre-state. Persist every replay lock
    // before crossing the untrusted callback boundary, then drop all account
    // borrows before CPI.
    for stored_runtime in stored {
        let snapshot = envelope
            .authorization_snapshots
            .get(usize::from(stored_runtime.authorization_slot))
            .ok_or(CoreError::InvalidAuthorizationSlots)?;
        reserve_stored_authorization_execution_exact(
            &accounts[stored_runtime.account_position],
            program_id,
            current_slot,
            snapshot.expected_fill_sequence,
            request_digest,
        )?;
    }

    let (expected_callback, callback_bump) =
        Pubkey::find_program_address(&[&callback_seed], program_id);
    require_keys_eq!(
        *accounts[CALLBACK_ACCOUNT_INDEX].key,
        expected_callback,
        CoreError::InvalidWireEncoding
    );
    let callback_bump_seed = [callback_bump];
    let callback_signer_seeds = [callback_seed.as_ref(), callback_bump_seed.as_ref()];
    let receipt_data = invoke_engine_transition(
        &accounts[ENGINE_PROGRAM_ACCOUNT_INDEX],
        &accounts[CALLBACK_ACCOUNT_INDEX],
        &accounts[segments.opaque.start..segments.opaque.end],
        &effective[segments.opaque.start..segments.opaque.end],
        engine_data,
        &callback_signer_seeds,
    )?;

    require_callback_preserved_protected_prestate(
        program_id, accounts, segments, endpoints, domains, fee_shards,
    )?;
    let receipt =
        decode_effect_receipt(&receipt_data).map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    require!(
        receipt.request_digest == request_digest
            && receipt.intent_set_digest == normalized.intent_set_digest
            && receipt.protected_execution_root == protected_execution_root
            && receipt.engine_sequence == envelope.header.expected_engine_sequence
            && receipt.engine_supplied_evidence_digest != [0; 32]
            && receipt.moves.len() <= usize::from(envelope.header.maximum_engine_moves),
        CoreError::InvalidWireEncoding
    );
    let canonical_moves = receipt
        .moves
        .iter()
        .map(|movement| CanonicalMove {
            source_capability_index: movement.source_capability_index,
            destination_capability_index: movement.destination_capability_index,
            amount: movement.amount,
        })
        .collect::<Vec<_>>();
    let plan = validate_move_normal_form(&canonical_moves, capabilities)?;
    let canonical_effect_digest =
        compute_canonical_effect_digest(&request_digest, &protected_execution_root, &receipt.moves)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;

    let domain_accounting = derive_all_domain_accounting(capabilities, endpoints, domains, &plan)?;
    let derived_fees = derive_all_fees(
        program_id,
        authorizations,
        capabilities,
        fee_shards,
        fee_policy,
        &plan,
        market_binding_digest,
        normalized.intent_set_digest,
        protected_execution_root,
        canonical_effect_digest,
    )?;
    let fee_assessment_set_digest = fee_assessment_set_root(
        &derived_fees
            .iter()
            .map(|runtime| runtime.assessment)
            .collect::<Vec<_>>(),
    )?;
    let stored_previews = validate_authorization_effects(
        program_id,
        accounts,
        authorizations,
        &plan,
        &derived_fees,
        &request_digest,
    )?;

    let (transfer_rows, signer_materials) = build_transfer_plan(
        program_id,
        accounts,
        segments,
        envelope,
        authorizations,
        domains,
        &plan,
        &derived_fees,
    )?;
    let seed_sets = signer_materials
        .iter()
        .map(|material| {
            [
                material.prefix,
                material.seed.as_ref(),
                material.bump.as_ref(),
            ]
        })
        .collect::<Vec<_>>();
    let transfers = transfer_rows
        .iter()
        .map(|row| {
            let source = usize::from(row.source_capability_index);
            let destination = usize::from(row.destination_capability_index);
            let asset = assets
                .get(usize::from(capabilities[source].declaration.asset_index))
                .ok_or(CoreError::MoveAssetProfileMismatch)?;
            Ok(ClassicSplTransfer {
                source: &accounts[segments.settlement.start + source],
                destination: &accounts[segments.settlement.start + destination],
                mint: &accounts[asset.mint_position],
                authority: &accounts[row.authority_position],
                amount: row.amount,
                authority_signer_seeds: row
                    .signer_material_index
                    .map(|index| seed_sets[index].as_slice()),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let observed_transfers =
        execute_classic_spl_transfers(&accounts[segments.protected_profile.start], &transfers)?;

    let mut protected_fee_transfers = derived_fees
        .iter()
        .filter(|runtime| runtime.assessment.total_fee != 0)
        .map(|runtime| ProtectedFeeTransfer {
            source_capability_index: runtime.source_capability_index,
            destination_capability_index: runtime.destination_capability_index,
            amount: runtime.assessment.total_fee,
        })
        .collect::<Vec<_>>();
    protected_fee_transfers.sort_unstable_by_key(|transfer| {
        (
            transfer.source_capability_index,
            transfer.destination_capability_index,
        )
    });
    let observed_balances = changed_observed_balances(
        capabilities,
        endpoints,
        &plan,
        &protected_fee_transfers,
        &observed_transfers,
    )?;
    let observed_delta_root =
        verify_exact_observed_deltas(&plan, &protected_fee_transfers, &observed_balances)?;

    validate_delegate_postconditions(
        program_id,
        accounts,
        segments,
        authorizations,
        endpoints,
        capabilities,
        &plan,
        &derived_fees,
    )?;
    commit_domain_accounting(
        program_id,
        accounts,
        domains,
        capabilities,
        endpoints,
        &domain_accounting,
        &observed_transfers,
    )?;
    commit_fee_liabilities(
        program_id,
        accounts,
        fee_shards,
        endpoints,
        &derived_fees,
        &observed_transfers,
    )?;
    for runtime in stored_previews {
        commit_verified_stored_authorization_execution_exact(
            &accounts[runtime.account_position],
            program_id,
            &request_digest,
            &runtime.verified_commit,
            &runtime.preview.next_capabilities,
            &runtime.preview.next_fee_states,
            runtime.preview.terminal,
        )?;
    }

    emit_execution_evidence(
        program_id,
        market,
        validated_engine,
        envelope,
        &receipt,
        market_binding_digest,
        domain_set_digest,
        opaque_capability_root,
        protected_execution_root,
        request_digest,
        canonical_effect_digest,
        fee_assessment_set_digest,
        observed_delta_root,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_engine_request(
    envelope: &ExecuteEnvelopeCandidateV0,
    market: &MarketDescriptorCandidateV0,
    fee_policy: &FeePolicyCandidateV0,
    validated_engine: &ValidatedEngineIdentity,
    assets: &[ResolvedAsset],
    endpoints: &[ClassicSplEndpointSnapshot],
    domains: &[ResolvedDomain],
    authorizations: &[ResolvedAuthorization],
    capabilities: &[SettlementCapability],
    market_binding_digest: [u8; 32],
    domain_set_digest: [u8; 32],
    opaque_capability_root: [u8; 32],
    protected_execution_root: [u8; 32],
) -> Result<EngineRequestCandidateV0> {
    let asset_rows = assets
        .iter()
        .enumerate()
        .map(|(index, asset)| {
            Ok(EngineAssetRowCandidateV0 {
                asset_index: u8::try_from(index).map_err(|_| CoreError::ExperimentLimitExceeded)?,
                asset_flags: 0,
                decimals: asset.mint.decimals,
                reserved: 0,
                asset_identity: asset.mint.key.to_bytes(),
                asset_program: spl_token::ID.to_bytes(),
                settlement_profile_digest: asset.binding.settlement_profile_digest,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let domain_rows = domains
        .iter()
        .enumerate()
        .map(|(index, domain)| {
            Ok(EngineDomainRowCandidateV0 {
                domain_index: u8::try_from(index)
                    .map_err(|_| CoreError::ExperimentLimitExceeded)?,
                domain_descriptor: domain.execution_row.domain_descriptor_key,
                domain_revision: domain.descriptor.domain_revision,
                admission_digest: domain.admission_digest,
                accounting_profile_digest: domain.descriptor.accounting_profile_digest,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let intent_rows = authorizations
        .iter()
        .enumerate()
        .map(|(slot, authorization)| {
            Ok(EngineIntentRowCandidateV0 {
                authorization_slot: u8::try_from(slot)
                    .map_err(|_| CoreError::ExperimentLimitExceeded)?,
                identity: authorization.identity.inline_identity(),
                intent_digest: authorization.identity.intent_digest,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let contexts = envelope
        .settlement_capabilities
        .iter()
        .enumerate()
        .filter(|(_, declaration)| {
            declaration.authority_class != AUTHORITY_CORE_RESERVED_FEE_CREDIT
                && declaration.rights_bits & RIGHT_CORE_RESERVED_FEE == 0
        })
        .map(|(index, declaration)| {
            engine_context_for_capability(
                u8::try_from(index).map_err(|_| CoreError::ExperimentLimitExceeded)?,
                *declaration,
                endpoints[index],
                authorizations,
                capabilities[index].accounted_before_or_zero,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let context_row_count =
        u8::try_from(contexts.len()).map_err(|_| CoreError::ExperimentLimitExceeded)?;
    let request = EngineRequestCandidateV0 {
        header: EngineRequestHeaderCandidateV0 {
            magic: ENGINE_REQUEST_MAGIC,
            wire_version: WIRE_VERSION,
            phase: PHASE_TRANSITION,
            settlement_capability_count: envelope.header.settlement_capability_count,
            opaque_capability_count: envelope.header.opaque_capability_count,
            intent_count: envelope.header.intent_count,
            domain_count: envelope.header.domain_count,
            asset_count: envelope.header.asset_count,
            context_row_count,
            payload_len: envelope.header.payload_len,
            maximum_engine_moves: envelope.header.maximum_engine_moves,
            market_binding_digest,
            engine_instance_id: market.engine_instance_id,
            engine_interface_id: market.engine_interface_id,
            intent_set_digest: envelope.header.intent_set_digest,
            domain_set_digest,
            protected_execution_root,
            opaque_capability_root,
            engine_loader_state_snapshot_digest: validated_engine.loader_state_snapshot_digest,
            fee_policy_digest: fee_policy.policy_digest,
        },
        assets: asset_rows,
        domains: domain_rows,
        intents: intent_rows,
        fee_policy: fee_policy.engine_row()?,
        contexts,
        payload: envelope.payload.clone(),
    };
    Ok(request)
}

fn engine_context_for_capability(
    capability_index: u8,
    declaration: generic_effect_private_wire::SettlementCapabilityRowCandidateV0,
    endpoint: ClassicSplEndpointSnapshot,
    authorizations: &[ResolvedAuthorization],
    accounted_before_or_zero: u128,
) -> Result<EngineContextRowCandidateV0> {
    let mut remaining_maximum_engine_debit = declaration.maximum_engine_debit;
    let mut remaining_maximum_total_debit = declaration.maximum_total_debit;
    let mut remaining_minimum_credit = declaration.minimum_credit;
    let mut remaining_maximum_protocol_fee = declaration.maximum_protocol_fee;
    if declaration.authorization_slot_or_none != ABSENT_INDEX {
        let authorization = authorizations
            .get(usize::from(declaration.authorization_slot_or_none))
            .ok_or(CoreError::InvalidAuthorizationSlots)?;
        let state = authorization
            .capability_states
            .get(usize::from(declaration.intent_local_term_index_or_none))
            .ok_or(CoreError::InvalidAuthorizationSlots)?;
        let remaining_engine = u128::from(state.initial_maximum_engine_debit)
            .checked_sub(state.cumulative_engine_debit)
            .ok_or(CoreError::AuthorizationBoundExceeded)?;
        remaining_maximum_total_debit = state.remaining_total_debit;
        remaining_maximum_engine_debit = u64::try_from(remaining_engine)
            .map_err(|_| CoreError::AmountConversionFailed)?
            .min(remaining_maximum_total_debit);
        remaining_minimum_credit =
            if state.cumulative_credit >= u128::from(state.initial_minimum_credit) {
                0
            } else {
                u64::try_from(u128::from(state.initial_minimum_credit) - state.cumulative_credit)
                    .map_err(|_| CoreError::AmountConversionFailed)?
            };
        let remaining_fee = u128::from(declaration.maximum_protocol_fee)
            .checked_sub(state.cumulative_fee_debit)
            .ok_or(CoreError::FeeCeilingExceeded)?;
        remaining_maximum_protocol_fee = u64::try_from(remaining_fee)
            .map_err(|_| CoreError::AmountConversionFailed)?
            .min(remaining_maximum_total_debit);
    }
    Ok(EngineContextRowCandidateV0 {
        settlement_capability_index: capability_index,
        asset_index: declaration.asset_index,
        domain_index_or_none: declaration.domain_index_or_none,
        authorization_slot_or_none: declaration.authorization_slot_or_none,
        rights_bits: declaration.rights_bits,
        fee_class: declaration.fee_class,
        context_flags: 0,
        endpoint_key: endpoint.key.to_bytes(),
        observed_before: endpoint.amount,
        accounted_before_or_zero: u64::try_from(
            if declaration.authority_class == AUTHORITY_DOMAIN_ACCOUNTED {
                // The protected capability root already authenticated this exact
                // u128 value. The engine row is an explicitly checked projection.
                accounted_before_or_zero
            } else {
                0
            },
        )
        .map_err(|_| CoreError::AmountConversionFailed)?,
        remaining_maximum_engine_debit,
        remaining_maximum_total_debit,
        remaining_minimum_credit,
        remaining_maximum_protocol_fee,
    })
}

#[allow(clippy::too_many_arguments)]
fn require_callback_preserved_protected_prestate(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    segments: AccountSegments,
    endpoints: &[ClassicSplEndpointSnapshot],
    domains: &[ResolvedDomain],
    fee_shards: &[ResolvedFeeShard],
) -> Result<()> {
    for (index, before) in endpoints.iter().enumerate() {
        let after = load_classic_spl_endpoint(&accounts[segments.settlement.start + index])?;
        require!(after == *before, CoreError::ObservedProtectedDeltaMismatch);
    }
    // Stored tombstones are reborrowed and fully authenticated by
    // `validate_authorization_effects` below, still before any protected token
    // transfer. Keeping that single post-callback decode avoids retaining or
    // recreating a second complete heap-backed stored view here.
    for domain in domains {
        let current = load_core_account::<DomainAccountingCandidateV0>(
            program_id,
            &accounts[domain.accounting_position],
            DomainAccountingCandidateV0::SPACE,
        )?;
        current.validate_authenticated(
            program_id,
            accounts[domain.accounting_position].key,
            accounts[domain.descriptor_position].key,
            domain.descriptor.domain_revision,
        )?;
        require!(
            current.wire_version == domain.accounting.wire_version
                && current.asset_count == domain.accounting.asset_count
                && current.bump == domain.accounting.bump
                && current.reserved == domain.accounting.reserved
                && current.domain_descriptor == domain.accounting.domain_descriptor
                && current.domain_revision == domain.accounting.domain_revision
                && current.assets == domain.accounting.assets,
            CoreError::AuthorizationSnapshotMismatch
        );
    }
    for shard in fee_shards {
        let current = load_core_account::<FeeLiabilityLedgerCandidateV0>(
            program_id,
            &accounts[shard.liability_position],
            FeeLiabilityLedgerCandidateV0::SPACE,
        )?;
        current.validate_partition(
            program_id,
            accounts[shard.liability_position].key,
            &shard.descriptor,
            accounts[shard.descriptor_position].key,
            &shard.descriptor.market_binding_digest,
        )?;
        require!(
            current.wire_version == shard.liability.wire_version
                && current.shard_index == shard.liability.shard_index
                && current.bump == shard.liability.bump
                && current.reserved == shard.liability.reserved
                && current.descriptor == shard.liability.descriptor
                && current.market_binding_digest == shard.liability.market_binding_digest
                && current.asset_identity == shard.liability.asset_identity
                && current.settlement_profile_digest == shard.liability.settlement_profile_digest
                && current.liability == shard.liability.liability,
            CoreError::AuthorizationSnapshotMismatch
        );
    }
    Ok(())
}

fn derive_all_domain_accounting(
    capabilities: &[SettlementCapability],
    endpoints: &[ClassicSplEndpointSnapshot],
    domains: &[ResolvedDomain],
    plan: &ValidatedMovePlan,
) -> Result<Vec<DomainAccountingDelta>> {
    let mut states = Vec::new();
    for (index, capability) in capabilities.iter().enumerate() {
        if capability.declaration.authority_class != AUTHORITY_DOMAIN_ACCOUNTED {
            continue;
        }
        let identity = capability
            .domain
            .ok_or(CoreError::InvalidSettlementDomain)?;
        let domain = domains
            .get(usize::from(identity.domain_index))
            .ok_or(CoreError::InvalidSettlementDomain)?;
        let slot = domain
            .accounting
            .assets
            .get(usize::from(identity.accounting_slot))
            .filter(|_| {
                usize::from(identity.accounting_slot) < usize::from(domain.accounting.asset_count)
            })
            .ok_or(CoreError::InvalidSettlementDomain)?;
        states.push(DomainAccountingState {
            domain_index: identity.domain_index,
            accounting_slot: identity.accounting_slot,
            asset: capability.asset,
            accounted_before: slot.accounted_amount,
            raw_balance_before: endpoints[index].amount,
        });
    }
    derive_domain_accounting(plan, capabilities, &states)
}

#[allow(clippy::too_many_arguments)]
fn derive_all_fees(
    program_id: &Pubkey,
    authorizations: &[ResolvedAuthorization],
    capabilities: &[SettlementCapability],
    fee_shards: &[ResolvedFeeShard],
    fee_policy: &FeePolicyCandidateV0,
    plan: &ValidatedMovePlan,
    market_binding_digest: [u8; 32],
    intent_set_digest: [u8; 32],
    protected_execution_root: [u8; 32],
    effect_digest: [u8; 32],
) -> Result<Vec<DerivedFeeRuntime>> {
    let mut contributions = Vec::new();
    for (index, capability) in capabilities.iter().enumerate() {
        if capability.declaration.authority_class != AUTHORITY_INTENT_FUNDED_DEBIT {
            continue;
        }
        let slot = capability
            .authorization_slot()
            .ok_or(CoreError::InvalidSettlementAuthorization)?;
        let authorization = authorizations
            .get(usize::from(slot))
            .ok_or(CoreError::InvalidAuthorizationSlots)?;
        let collection_route = if capability.is_fee_funding() {
            let shard_index = capability
                .fee_shard_index()
                .ok_or(CoreError::InvalidSettlementFeeShard)?;
            let shard = fee_shards
                .get(usize::from(shard_index))
                .ok_or(CoreError::InvalidSettlementFeeShard)?;
            require_eq!(
                shard.asset_index,
                capability.declaration.asset_index,
                CoreError::InvalidSettlementFeeShard
            );
            Some(FeeCollectionRoute {
                funding_capability_index: capability.position,
                designated_endpoint: capability.endpoint.key,
                fee_shard_index: shard_index,
                maximum_protocol_fee: capability.declaration.maximum_protocol_fee,
                maximum_total_debit: capability.declaration.maximum_total_debit,
            })
        } else {
            None
        };
        contributions.push(FeeBasisContribution {
            key: FeeBucketKey {
                actor: authorization.identity.actor,
                intent_digest: authorization.identity.intent_digest,
                fee_policy_digest: fee_policy.policy_digest,
                asset: capability.asset,
                fee_class: capability.declaration.fee_class,
                fee_policy_revision: fee_policy.revision,
            },
            basis: plan.gross_debits[index],
            collection_route,
        });
    }

    // Preserve a zero-basis FEE_FUNDING row when another source in the same
    // principal group has basis, but omit all-zero groups altogether.
    let mut active_keys = Vec::new();
    for contribution in &contributions {
        if contribution.basis != 0 && !active_keys.contains(&contribution.key) {
            active_keys.push(contribution.key);
        }
    }
    contributions.retain(|contribution| active_keys.contains(&contribution.key));
    let buckets = if contributions.is_empty() {
        Vec::<AggregatedFeeBucket>::new()
    } else {
        aggregate_fee_bases(&contributions)?
    };
    let rate_policy = RatePolicy {
        rate: fee_policy.rate,
        denominator: fee_policy.denominator,
        rounding: RoundingMode::decode(fee_policy.rounding_mode)?,
    };
    let mut output = Vec::with_capacity(buckets.len());
    for bucket in buckets {
        let authorization_slot = authorizations
            .iter()
            .position(|authorization| {
                authorization.identity.intent_digest == bucket.key.intent_digest
                    && authorization.identity.actor == bucket.key.actor
            })
            .ok_or(CoreError::InvalidAuthorizationSlots)?;
        let authorization = &authorizations[authorization_slot];
        let funding_capability = capabilities
            .get(usize::from(
                bucket.collection_route.funding_capability_index,
            ))
            .filter(|capability| {
                capability.is_fee_funding()
                    && capability.authorization_slot() == u8::try_from(authorization_slot).ok()
                    && capability.endpoint.key == bucket.collection_route.designated_endpoint
            })
            .ok_or(CoreError::InvalidSettlementFeeShard)?;
        let funding_local_term_index = funding_capability
            .declaration
            .intent_local_term_index_or_none;
        let rounding_group_digest = fee_rounding_group_digest(&bucket.key)?;
        let existing_fee_state = authorization
            .fee_states
            .iter()
            .find(|state| state.rounding_group_digest == rounding_group_digest);
        let (cumulative_basis_before, cumulative_assessed_before) =
            if let Some(state) = existing_fee_state {
                require!(
                    authorization.witness_kind == WITNESS_STORED_AUTHORIZATION
                        && state.funding_local_term_index == funding_local_term_index
                        && state.fee_class == bucket.key.fee_class
                        && state.maximum_fee == bucket.collection_route.maximum_protocol_fee,
                    CoreError::AuthorizationIdentityMismatch
                );
                (state.cumulative_basis, state.cumulative_assessed_fee)
            } else {
                (0, 0)
            };
        let assessment = derive_fee_assessment(
            FeeAssessmentContext {
                core_program: *program_id,
                experimental_major: EXPERIMENTAL_MAJOR,
                market_binding_digest,
                policy_digest: fee_policy.policy_digest,
                policy_revision: fee_policy.revision,
                intent_set_digest,
                protected_execution_root,
                effect_digest,
                fill_sequence: authorization.view.fill_sequence,
            },
            bucket,
            cumulative_basis_before,
            cumulative_assessed_before,
            rate_policy,
        )?;
        validate_user_fee_and_total_debit(
            plan.gross_debits[usize::from(assessment.collection_route.funding_capability_index)],
            &assessment,
            funding_capability.declaration.maximum_total_debit,
            funding_capability.declaration.maximum_protocol_fee,
        )?;
        let shard = fee_shards
            .get(usize::from(assessment.collection_route.fee_shard_index))
            .ok_or(CoreError::InvalidSettlementFeeShard)?;
        let destination_capability = capabilities
            .get(usize::from(shard.vault_capability_index))
            .filter(|capability| {
                capability.declaration.authority_class == AUTHORITY_CORE_RESERVED_FEE_CREDIT
                    && capability.fee_shard_index()
                        == Some(assessment.collection_route.fee_shard_index)
                    && capability.asset == funding_capability.asset
            })
            .ok_or(CoreError::InvalidSettlementFeeShard)?;
        output.push(DerivedFeeRuntime {
            assessment,
            authorization_slot: u8::try_from(authorization_slot)
                .map_err(|_| CoreError::ExperimentLimitExceeded)?,
            funding_local_term_index,
            source_capability_index: funding_capability.position,
            destination_capability_index: destination_capability.position,
        });
    }
    output.sort_unstable_by_key(|runtime| runtime.assessment.rounding_group_digest);
    require!(
        output.windows(2).all(|pair| {
            pair[0].assessment.rounding_group_digest < pair[1].assessment.rounding_group_digest
        }),
        CoreError::DuplicateFeeAssessment
    );
    Ok(output)
}

fn fee_rounding_group_digest(key: &FeeBucketKey) -> Result<[u8; 32]> {
    let principal_digest = compute_fee_principal_digest(&key.actor.to_bytes(), &key.intent_digest)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    compute_fee_rounding_group_digest(&FeeRoundingGroupRowCandidateV0 {
        fee_principal_digest: principal_digest,
        fee_policy_digest: key.fee_policy_digest,
        asset_identity: key.asset.asset_identity.to_bytes(),
        asset_program: key.asset.asset_program.to_bytes(),
        settlement_profile_digest: key.asset.settlement_profile_digest,
        fee_class: key.fee_class,
        fee_policy_revision: key.fee_policy_revision,
    })
    .map_err(|_| error!(CoreError::InvalidWireEncoding))
}

#[allow(clippy::too_many_arguments)]
fn validate_authorization_effects(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    authorizations: &[ResolvedAuthorization],
    plan: &ValidatedMovePlan,
    derived_fees: &[DerivedFeeRuntime],
    request_digest: &[u8; 32],
) -> Result<Vec<StoredPreviewRuntime>> {
    let mut previews = Vec::new();
    for (slot, authorization) in authorizations.iter().enumerate() {
        let mut fills = Vec::with_capacity(authorization.local_to_global.len());
        let mut positive_non_fee_delta = false;
        for (local_index, global_index) in authorization.local_to_global.iter().enumerate() {
            let global_index_usize = usize::from(*global_index);
            let engine_debit = u64::try_from(plan.gross_debits[global_index_usize])
                .map_err(|_| CoreError::AmountConversionFailed)?;
            let credit = u64::try_from(plan.gross_credits[global_index_usize])
                .map_err(|_| CoreError::AmountConversionFailed)?;
            let rate_fee_debit = fee_for_source(*global_index, derived_fees)?;
            positive_non_fee_delta |= engine_debit != 0 || credit != 0;
            let term = authorization
                .terms
                .get(local_index)
                .ok_or(CoreError::InvalidAuthorizationSlots)?;
            if term.rights_bits & RIGHT_DEBIT != 0 {
                let total = engine_debit
                    .checked_add(rate_fee_debit)
                    .ok_or(CoreError::ArithmeticOverflow)?;
                require!(
                    engine_debit <= term.maximum_engine_debit
                        && rate_fee_debit <= term.maximum_protocol_fee
                        && total <= term.maximum_total_debit,
                    CoreError::AuthorizationBoundExceeded
                );
            } else {
                require_eq!(rate_fee_debit, 0, CoreError::FeeCeilingExceeded);
            }
            fills.push(StoredCapabilityFill {
                local_term_index: u8::try_from(local_index)
                    .map_err(|_| CoreError::ExperimentLimitExceeded)?,
                engine_debit,
                rate_fee_debit,
                credit,
            });
        }
        require!(
            positive_non_fee_delta
                || (authorizations.len() == 1
                    && authorization.witness_kind == WITNESS_DIRECT_ACTOR
                    && authorization.identity.actor.is_on_curve()
                    && anchor_lang::solana_program::instruction::get_stack_height()
                        == anchor_lang::solana_program::instruction::TRANSACTION_LEVEL_STACK_HEIGHT
                    && plan.moves.is_empty() && derived_fees.is_empty()),
            CoreError::AuthorizationBoundExceeded
        );

        if authorization.witness_kind == WITNESS_STORED_AUTHORIZATION {
            let account_position = authorization
                .stored_control_position
                .ok_or(CoreError::InvalidAuthorizationSlots)?;
            let (compact, verified_commit) =
                verify_stored_authorization_execution_for_commit_exact(
                    &accounts[account_position],
                    program_id,
                    request_digest,
                )?;
            let fee_fills = derived_fees
                .iter()
                .filter(|runtime| usize::from(runtime.authorization_slot) == slot)
                .map(|runtime| StoredFeeFill {
                    rounding_group_digest: runtime.assessment.rounding_group_digest,
                    funding_local_term_index: runtime.funding_local_term_index,
                    fee_class: runtime.assessment.key.fee_class,
                    maximum_fee: runtime.assessment.collection_route.maximum_protocol_fee,
                    fill_basis: runtime.assessment.fill_basis,
                    assessed_fee: runtime.assessment.total_fee,
                })
                .collect::<Vec<_>>();
            let preview = preview_stored_fill(
                program_id,
                accounts[account_position].key,
                &compact,
                &fills,
                &fee_fills,
            )?;
            previews.push(StoredPreviewRuntime {
                account_position,
                verified_commit,
                preview,
            });
            continue;
        }

        require!(
            authorization.constraints.is_empty(),
            CoreError::AuthorizationBoundExceeded
        );
        for (local_index, global_index) in authorization.local_to_global.iter().enumerate() {
            let term = authorization
                .terms
                .get(local_index)
                .ok_or(CoreError::InvalidAuthorizationSlots)?;
            let global = usize::from(*global_index);
            if term.rights_bits & RIGHT_CREDIT != 0 {
                require!(
                    plan.gross_credits[global] >= u128::from(term.minimum_credit),
                    CoreError::CapabilityMinimumCreditNotMet
                );
            }
        }
    }
    Ok(previews)
}

fn fee_for_source(source_capability_index: u8, derived_fees: &[DerivedFeeRuntime]) -> Result<u64> {
    Ok(derived_fees
        .iter()
        .filter(|runtime| runtime.source_capability_index == source_capability_index)
        .try_fold(0_u64, |sum, runtime| {
            sum.checked_add(runtime.assessment.total_fee)
                .ok_or(CoreError::ArithmeticOverflow)
        })?)
}

#[allow(clippy::too_many_arguments)]
fn build_transfer_plan(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    segments: AccountSegments,
    envelope: &ExecuteEnvelopeCandidateV0,
    authorizations: &[ResolvedAuthorization],
    domains: &[ResolvedDomain],
    plan: &ValidatedMovePlan,
    derived_fees: &[DerivedFeeRuntime],
) -> Result<(Vec<TransferPlanRow>, Vec<SignerMaterial>)> {
    let mut rows = Vec::with_capacity(plan.moves.len() + derived_fees.len());
    let mut signer_materials = Vec::with_capacity(rows.capacity());
    for movement in &plan.moves {
        let (authority_position, signer_material_index) = transfer_authority_for_source(
            program_id,
            accounts,
            segments,
            envelope,
            authorizations,
            domains,
            movement.source_capability_index,
            &mut signer_materials,
        )?;
        rows.push(TransferPlanRow {
            source_capability_index: movement.source_capability_index,
            destination_capability_index: movement.destination_capability_index,
            amount: movement.amount,
            authority_position,
            signer_material_index,
        });
    }
    let mut fee_rows = derived_fees
        .iter()
        .filter(|runtime| runtime.assessment.total_fee != 0)
        .map(|runtime| {
            let (authority_position, signer_material_index) = transfer_authority_for_source(
                program_id,
                accounts,
                segments,
                envelope,
                authorizations,
                domains,
                runtime.source_capability_index,
                &mut signer_materials,
            )?;
            Ok(TransferPlanRow {
                source_capability_index: runtime.source_capability_index,
                destination_capability_index: runtime.destination_capability_index,
                amount: runtime.assessment.total_fee,
                authority_position,
                signer_material_index,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    fee_rows.sort_unstable_by_key(|row| {
        (
            row.source_capability_index,
            row.destination_capability_index,
        )
    });
    require!(
        fee_rows.windows(2).all(|pair| {
            (
                pair[0].source_capability_index,
                pair[0].destination_capability_index,
            ) < (
                pair[1].source_capability_index,
                pair[1].destination_capability_index,
            )
        }),
        CoreError::NonCanonicalMoveOrder
    );
    rows.extend(fee_rows);
    Ok((rows, signer_materials))
}

#[allow(clippy::too_many_arguments)]
fn transfer_authority_for_source(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    segments: AccountSegments,
    envelope: &ExecuteEnvelopeCandidateV0,
    authorizations: &[ResolvedAuthorization],
    domains: &[ResolvedDomain],
    source_capability_index: u8,
    signer_materials: &mut Vec<SignerMaterial>,
) -> Result<(usize, Option<usize>)> {
    let declaration = *envelope
        .settlement_capabilities
        .get(usize::from(source_capability_index))
        .ok_or(CoreError::MoveCapabilityIndexOutOfRange)?;
    match declaration.authority_class {
        AUTHORITY_INTENT_FUNDED_DEBIT => {
            let authorization = authorizations
                .get(usize::from(declaration.authorization_slot_or_none))
                .ok_or(CoreError::InvalidAuthorizationSlots)?;
            if authorization.witness_kind == WITNESS_DIRECT_ACTOR {
                let snapshot = envelope
                    .authorization_snapshots
                    .get(usize::from(declaration.authorization_slot_or_none))
                    .ok_or(CoreError::InvalidAuthorizationSlots)?;
                let position = segments.authorization_controls.start
                    + usize::from(snapshot.authorization_control_offset_or_none);
                require_keys_eq!(
                    *accounts[position].key,
                    authorization.identity.actor,
                    CoreError::AuthorizationIdentityMismatch
                );
                require!(
                    accounts[position].is_signer,
                    CoreError::DirectAuthorizationNotTransactionRoot
                );
                return Ok((position, None));
            }
            require!(
                matches!(
                    authorization.witness_kind,
                    WITNESS_EXACT_DELEGATE | WITNESS_STORED_AUTHORIZATION
                ) && declaration.spend_authority_control_offset_or_none != ABSENT_INDEX,
                CoreError::UnsupportedAuthorizationWitness
            );
            let position = segments.authorization_controls.start
                + usize::from(declaration.spend_authority_control_offset_or_none);
            let source =
                accounts[segments.settlement.start + usize::from(source_capability_index)].key;
            let seed = compute_intent_spend_seed(
                &authorization.identity.intent_digest,
                &source.to_bytes(),
            )
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
            let (authority, bump) = derive_exact_spend_authority(
                program_id,
                &authorization.identity.intent_digest,
                source,
            )?;
            require_keys_eq!(
                *accounts[position].key,
                authority,
                CoreError::ExactDelegateConsumptionMismatch
            );
            signer_materials.push(SignerMaterial {
                prefix: INTENT_SPEND_SEED,
                seed,
                bump: [bump],
            });
            Ok((position, Some(signer_materials.len() - 1)))
        }
        AUTHORITY_DOMAIN_ACCOUNTED => {
            let domain = domains
                .get(usize::from(declaration.domain_index_or_none))
                .ok_or(CoreError::InvalidSettlementDomain)?;
            let descriptor_key = *accounts[domain.descriptor_position].key;
            let (authority, bump) =
                DomainAccountingCandidateV0::address(program_id, &descriptor_key);
            require_keys_eq!(
                *accounts[domain.accounting_position].key,
                authority,
                CoreError::InvalidSettlementDomain
            );
            signer_materials.push(SignerMaterial {
                prefix: DOMAIN_ACCOUNTING_SEED,
                seed: descriptor_key.to_bytes(),
                bump: [bump],
            });
            Ok((domain.accounting_position, Some(signer_materials.len() - 1)))
        }
        _ => err!(CoreError::MoveRightMissing),
    }
}

fn changed_observed_balances(
    capabilities: &[SettlementCapability],
    endpoints: &[ClassicSplEndpointSnapshot],
    plan: &ValidatedMovePlan,
    fee_transfers: &[ProtectedFeeTransfer],
    observed: &[ObservedClassicSplDelta],
) -> Result<Vec<ObservedProtectedBalance>> {
    let mut debits = plan.gross_debits.clone();
    let mut credits = plan.gross_credits.clone();
    for transfer in fee_transfers {
        let source = usize::from(transfer.source_capability_index);
        let destination = usize::from(transfer.destination_capability_index);
        debits[source] = debits[source]
            .checked_add(u128::from(transfer.amount))
            .ok_or(CoreError::ArithmeticOverflow)?;
        credits[destination] = credits[destination]
            .checked_add(u128::from(transfer.amount))
            .ok_or(CoreError::ArithmeticOverflow)?;
    }
    let mut output = Vec::new();
    for index in 0..capabilities.len() {
        if debits[index] == 0 && credits[index] == 0 {
            continue;
        }
        let delta = observed
            .iter()
            .find(|delta| delta.key == endpoints[index].key)
            .ok_or(CoreError::ObservedProtectedDeltaMismatch)?;
        require!(
            delta.amount_before == endpoints[index].amount
                && delta.expected_debit == debits[index]
                && delta.expected_credit == credits[index],
            CoreError::ObservedProtectedDeltaMismatch
        );
        output.push(ObservedProtectedBalance {
            capability_index: u8::try_from(index)
                .map_err(|_| CoreError::ExperimentLimitExceeded)?,
            before: delta.amount_before,
            after: delta.amount_after,
        });
    }
    Ok(output)
}

#[allow(clippy::too_many_arguments)]
fn validate_delegate_postconditions(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    segments: AccountSegments,
    authorizations: &[ResolvedAuthorization],
    endpoints: &[ClassicSplEndpointSnapshot],
    capabilities: &[SettlementCapability],
    plan: &ValidatedMovePlan,
    derived_fees: &[DerivedFeeRuntime],
) -> Result<()> {
    for authorization in authorizations {
        if authorization.witness_kind == WITNESS_DIRECT_ACTOR {
            continue;
        }
        let mut exact_observations = Vec::new();
        for global_index in &authorization.local_to_global {
            let index = usize::from(*global_index);
            if capabilities[index].declaration.authority_class != AUTHORITY_INTENT_FUNDED_DEBIT {
                continue;
            }
            let before = endpoints[index];
            let after = load_classic_spl_endpoint(&accounts[segments.settlement.start + index])?;
            let fee_debit = u128::from(fee_for_source(*global_index, derived_fees)?);
            if authorization.witness_kind == WITNESS_EXACT_DELEGATE {
                exact_observations.push(ExactDelegateObservation {
                    source: before.key,
                    token_owner: before.authority,
                    spend_authority: before.delegate.unwrap_or_default(),
                    delegated_amount_before: before.delegated_amount,
                    delegated_amount_after: after.delegated_amount,
                    delegate_present_after: after.delegate.is_some(),
                    observed_engine_source_debit: plan.gross_debits[index],
                    observed_rate_fee_debit: fee_debit,
                });
            } else if plan.gross_debits[index] != 0 || fee_debit != 0 {
                validate_stored_delegate_fill(
                    program_id,
                    &authorization.identity,
                    &StoredDelegateObservation {
                        source: before.key,
                        token_owner: before.authority,
                        spend_authority_before: before.delegate.unwrap_or_default(),
                        delegated_amount_before: before.delegated_amount,
                        delegate_after_or_zero: after.delegate.unwrap_or_default(),
                        delegated_amount_after: after.delegated_amount,
                        observed_engine_source_debit: plan.gross_debits[index],
                        observed_rate_fee_debit: fee_debit,
                    },
                )?;
            }
        }
        if authorization.witness_kind == WITNESS_EXACT_DELEGATE {
            validate_exact_delegate_consumption(
                program_id,
                &authorization.identity,
                &exact_observations,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn commit_domain_accounting(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    domains: &[ResolvedDomain],
    capabilities: &[SettlementCapability],
    endpoints: &[ClassicSplEndpointSnapshot],
    deltas: &[DomainAccountingDelta],
    observed: &[ObservedClassicSplDelta],
) -> Result<()> {
    for (domain_index, domain) in domains.iter().enumerate() {
        let mut current = load_core_account::<DomainAccountingCandidateV0>(
            program_id,
            &accounts[domain.accounting_position],
            DomainAccountingCandidateV0::SPACE,
        )?;
        current.validate_authenticated(
            program_id,
            accounts[domain.accounting_position].key,
            accounts[domain.descriptor_position].key,
            domain.descriptor.domain_revision,
        )?;
        require!(
            current.assets == domain.accounting.assets,
            CoreError::AuthorizationSnapshotMismatch
        );
        let mut changed = false;
        for delta in deltas
            .iter()
            .filter(|delta| usize::from(delta.domain_index) == domain_index)
        {
            if delta.local_debits == 0 && delta.local_credits == 0 {
                require_eq!(
                    delta.accounted_after,
                    delta.accounted_before,
                    CoreError::InvalidSettlementDomain
                );
                continue;
            }
            let capability_index = capabilities
                .iter()
                .position(|capability| {
                    capability.declaration.authority_class == AUTHORITY_DOMAIN_ACCOUNTED
                        && capability.domain.is_some_and(|identity| {
                            identity.domain_index == delta.domain_index
                                && identity.accounting_slot == delta.accounting_slot
                                && capability.asset == delta.asset
                        })
                })
                .ok_or(CoreError::InvalidSettlementDomain)?;
            let endpoint = endpoints[capability_index];
            let observation = observed
                .iter()
                .find(|observation| observation.key == endpoint.key)
                .ok_or(CoreError::ObservedProtectedDeltaMismatch)?;
            require_eq!(
                observation.amount_after,
                delta.expected_raw_after,
                CoreError::ObservedProtectedDeltaMismatch
            );
            let slot = current
                .assets
                .get_mut(usize::from(delta.accounting_slot))
                .filter(|_| usize::from(delta.accounting_slot) < usize::from(current.asset_count))
                .ok_or(CoreError::InvalidSettlementDomain)?;
            require_eq!(
                slot.accounted_amount,
                delta.accounted_before,
                CoreError::AuthorizationSnapshotMismatch
            );
            slot.accounted_amount = delta.accounted_after;
            changed = true;
        }
        if changed {
            let mut data = accounts[domain.accounting_position]
                .try_borrow_mut_data()
                .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
            serialize_account_exact(&current, &mut data, DomainAccountingCandidateV0::SPACE)?;
        }
    }
    Ok(())
}

fn commit_fee_liabilities(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    fee_shards: &[ResolvedFeeShard],
    endpoints: &[ClassicSplEndpointSnapshot],
    derived_fees: &[DerivedFeeRuntime],
    observed: &[ObservedClassicSplDelta],
) -> Result<()> {
    for (shard_index, shard) in fee_shards.iter().enumerate() {
        let expected_credit = derived_fees
            .iter()
            .filter(|runtime| {
                usize::from(runtime.assessment.collection_route.fee_shard_index) == shard_index
            })
            .try_fold(0_u64, |sum, runtime| {
                sum.checked_add(runtime.assessment.total_fee)
                    .ok_or(CoreError::ArithmeticOverflow)
            })?;
        if expected_credit == 0 {
            continue;
        }
        let vault_before = endpoints
            .get(usize::from(shard.vault_capability_index))
            .ok_or(CoreError::InvalidSettlementFeeShard)?;
        let vault_observation = observed
            .iter()
            .find(|observation| observation.key == vault_before.key)
            .ok_or(CoreError::FeeLiabilityMismatch)?;
        let observed_credit = vault_observation
            .amount_after
            .checked_sub(vault_observation.amount_before)
            .ok_or(CoreError::FeeLiabilityMismatch)?;
        require_eq!(
            observed_credit,
            expected_credit,
            CoreError::FeeLiabilityMismatch
        );
        let mut current = load_core_account::<FeeLiabilityLedgerCandidateV0>(
            program_id,
            &accounts[shard.liability_position],
            FeeLiabilityLedgerCandidateV0::SPACE,
        )?;
        current.validate_partition(
            program_id,
            accounts[shard.liability_position].key,
            &shard.descriptor,
            accounts[shard.descriptor_position].key,
            &shard.descriptor.market_binding_digest,
        )?;
        require_eq!(
            current.liability,
            shard.liability.liability,
            CoreError::AuthorizationSnapshotMismatch
        );
        current.liability = update_fee_liability(current.liability, observed_credit)?;
        let mut data = accounts[shard.liability_position]
            .try_borrow_mut_data()
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        serialize_account_exact(&current, &mut data, FeeLiabilityLedgerCandidateV0::SPACE)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_execution_evidence(
    program_id: &Pubkey,
    market: &MarketDescriptorCandidateV0,
    validated_engine: &ValidatedEngineIdentity,
    envelope: &ExecuteEnvelopeCandidateV0,
    receipt: &generic_effect_private_wire::EffectReceiptCandidateV0,
    market_binding_digest: [u8; 32],
    domain_set_digest: [u8; 32],
    opaque_capability_root: [u8; 32],
    protected_execution_root: [u8; 32],
    request_digest: [u8; 32],
    canonical_effect_digest: [u8; 32],
    fee_assessment_set_digest: [u8; 32],
    observed_delta_root: [u8; 32],
) -> Result<ExecuteEffectOutcome> {
    let core_verified_evidence_digest =
        compute_core_verified_evidence_digest(CoreVerifiedEvidenceDigestInputs {
            core_program: &program_id.to_bytes(),
            market_binding_digest: &market_binding_digest,
            loader_state_snapshot_digest: &validated_engine.loader_state_snapshot_digest,
            intent_set_digest: &envelope.header.intent_set_digest,
            domain_set_digest: &domain_set_digest,
            protected_execution_root: &protected_execution_root,
            opaque_capability_root: &opaque_capability_root,
            request_digest: &request_digest,
            effect_digest: &canonical_effect_digest,
            fee_assessment_set_root: &fee_assessment_set_digest,
            observed_delta_root: &observed_delta_root,
        })
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    let engine_attested_evidence_digest =
        compute_engine_attested_evidence_digest(EngineAttestedEvidenceDigestInputs {
            engine_program: &market.engine_program.to_bytes(),
            engine_interface_id: &market.engine_interface_id,
            engine_instance_id: &market.engine_instance_id,
            request_digest: &request_digest,
            engine_supplied_digest: &receipt.engine_supplied_evidence_digest,
        })
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    let move_count =
        u8::try_from(receipt.moves.len()).map_err(|_| CoreError::ExperimentLimitExceeded)?;
    let routed = anchor_lang::solana_program::instruction::get_stack_height()
        != anchor_lang::solana_program::instruction::TRANSACTION_LEVEL_STACK_HEIGHT;
    let core_event = CoreVerifiedEvidenceCandidateV0 {
        evidence_class: EvidenceClass::CoreVerified.encode(),
        routed,
        move_count,
        intent_count: envelope.header.intent_count,
        domain_count: envelope.header.domain_count,
        reserved: [0; 3],
        core_program: *program_id,
        market_binding_digest,
        loader_state_snapshot_digest: validated_engine.loader_state_snapshot_digest,
        intent_set_digest: envelope.header.intent_set_digest,
        domain_set_digest,
        protected_execution_root,
        opaque_capability_root,
        request_digest,
        effect_digest: canonical_effect_digest,
        fee_assessment_set_root: fee_assessment_set_digest,
        observed_delta_root,
        evidence_digest: core_verified_evidence_digest,
    };
    core_event.validate()?;
    emit!(core_event);
    let engine_event = EngineAttestedEvidenceCandidateV0 {
        evidence_class: EvidenceClass::EngineAttested.encode(),
        reserved: [0; 7],
        engine_program: market.engine_program,
        engine_interface_id: market.engine_interface_id,
        engine_instance_id: market.engine_instance_id,
        request_digest,
        engine_supplied_digest: receipt.engine_supplied_evidence_digest,
        evidence_digest: engine_attested_evidence_digest,
    };
    engine_event.validate()?;
    emit!(engine_event);
    Ok(ExecuteEffectOutcome {
        request_digest,
        canonical_effect_digest,
        observed_delta_root,
        core_verified_evidence_digest,
        engine_attested_evidence_digest,
        move_count,
    })
}
