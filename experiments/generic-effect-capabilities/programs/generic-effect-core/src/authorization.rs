//! Witness-neutral authorization normalization and explicit replay state.

use anchor_lang::prelude::*;
use generic_effect_private_wire::{
    compute_authorization_capability_state_root, compute_authorization_fee_state_root,
    compute_authorization_state_digest, compute_authorization_view_set_digest,
    compute_intent_set_digest, compute_intent_spend_seed, compute_protected_execution_root,
    AuthorizationCapabilityStateRowCandidateV0, AuthorizationFeeStateRowCandidateV0,
    AuthorizationSnapshotRowCandidateV0, AuthorizationStateDigestInputs,
    AuthorizationViewRowCandidateV0, InlineIntentIdentityRowCandidateV0, IntentSetRowCandidateV0,
    ProtectedExecutionRootInputs, MAX_INLINE_INTENTS,
    SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT,
};

use crate::{
    account_segments::{
        has_transaction_signer, require_exact_transaction_root_invocation, EffectivePrivilege,
        TopLevelAccountMeta, TopLevelInstructionView,
    },
    constants::{
        ABSENT_INDEX, EXPERIMENTAL_MAJOR, MAX_AUTHORIZATION_CONTROL_ACCOUNTS, MAX_INTENTS,
        RIGHT_CREDIT, RIGHT_DEBIT, WITNESS_DIRECT_ACTOR, WITNESS_EXACT_ONE_SHOT_DELEGATE,
        WITNESS_STORED_AUTHORIZATION,
    },
    error::CoreError,
    fees::U256,
    state::{
        commit_stored_authorization_execution_exact, read_stored_authorization_compact,
        AuthorizationCapabilityStateCandidateV0, AuthorizationFeeStateCandidateV0,
        IntentIdentityCandidateV0, StoredAuthorizationCompactCandidateV0,
        StoredAuthorizationLifecycle, MAX_STORED_FEE_STATES,
    },
};

pub const INTENT_SPEND_SEED: &[u8] = b"intent-spend-v0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InlineIntentTerms {
    pub experimental_major: u32,
    pub market_binding_digest: [u8; 32],
    pub loader_state_snapshot_digest: [u8; 32],
    pub fee_policy_digest: [u8; 32],
    pub core_terms_root: [u8; 32],
    pub max_fills: u32,
}

pub fn resolve_inline_identity(
    core_program: &Pubkey,
    row: &InlineIntentIdentityRowCandidateV0,
    terms: InlineIntentTerms,
) -> Result<IntentIdentityCandidateV0> {
    require_eq!(
        terms.experimental_major,
        EXPERIMENTAL_MAJOR,
        CoreError::AuthorizationIdentityMismatch
    );
    require_eq!(terms.max_fills, 1, CoreError::AuthorizationIdentityMismatch);
    let mut identity = IntentIdentityCandidateV0 {
        experimental_major: terms.experimental_major,
        core_program: *core_program,
        actor: Pubkey::new_from_array(row.actor),
        authorization_nonce: row.authorization_nonce,
        market_binding_digest: terms.market_binding_digest,
        loader_state_snapshot_digest: terms.loader_state_snapshot_digest,
        fee_policy_digest: terms.fee_policy_digest,
        engine_terms_commitment: row.engine_terms_commitment,
        core_terms_root: terms.core_terms_root,
        reserved_digest: [0; 32],
        expires_at_slot_exclusive: row.expires_at_slot_exclusive,
        max_fills: terms.max_fills,
        intent_digest: [0; 32],
    };
    identity.intent_digest = identity.compute_intent_digest(core_program)?;
    identity.validate(core_program)?;
    Ok(identity)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IntentFundedAuthorizationControl {
    pub authorization_slot: u8,
    pub spend_authority_control_offset_or_none: u8,
    pub settlement_flags: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AuthorizationControlUse {
    DirectActor(Pubkey),
    Exclusive,
}

/// Validates the exact per-fill witness table. Inline identities and control
/// accounts are consumed exactly once, except that several direct intents from
/// the same actor may intentionally share that actor's signer control.
pub fn validate_authorization_snapshot_rows(
    snapshots: &[AuthorizationSnapshotRowCandidateV0],
    inline_identities: &[InlineIntentIdentityRowCandidateV0],
    authorization_control_count: usize,
    intent_funded_sources: &[IntentFundedAuthorizationControl],
) -> Result<()> {
    require!(
        !snapshots.is_empty() && snapshots.len() <= MAX_INTENTS,
        CoreError::ExperimentLimitExceeded
    );
    require!(
        inline_identities.len() <= MAX_INLINE_INTENTS
            && authorization_control_count <= MAX_AUTHORIZATION_CONTROL_ACCOUNTS,
        CoreError::ExperimentLimitExceeded
    );
    let mut used_inline = vec![false; inline_identities.len()];
    let mut used_controls = vec![None; authorization_control_count];
    for (position, snapshot) in snapshots.iter().enumerate() {
        require_eq!(
            usize::from(snapshot.authorization_slot),
            position,
            CoreError::InvalidAuthorizationSlots
        );
        match snapshot.witness_kind {
            WITNESS_DIRECT_ACTOR => {
                let inline = usize::from(snapshot.inline_identity_index_or_none);
                require!(
                    inline < used_inline.len() && !used_inline[inline],
                    CoreError::InvalidAuthorizationSlots
                );
                used_inline[inline] = true;
                require_eq!(
                    snapshot.expected_fill_sequence,
                    0,
                    CoreError::AuthorizationFillSequenceMismatch
                );
                let control = usize::from(snapshot.authorization_control_offset_or_none);
                require!(
                    control < used_controls.len(),
                    CoreError::InvalidAuthorizationSlots
                );
                let actor = Pubkey::new_from_array(inline_identities[inline].actor);
                match used_controls[control] {
                    None => {
                        used_controls[control] = Some(AuthorizationControlUse::DirectActor(actor));
                    }
                    Some(AuthorizationControlUse::DirectActor(existing)) => {
                        require_keys_eq!(existing, actor, CoreError::InvalidAuthorizationSlots)
                    }
                    Some(AuthorizationControlUse::Exclusive) => {
                        return err!(CoreError::InvalidAuthorizationSlots);
                    }
                }
            }
            WITNESS_EXACT_ONE_SHOT_DELEGATE => {
                let inline = usize::from(snapshot.inline_identity_index_or_none);
                require!(
                    inline < used_inline.len() && !used_inline[inline],
                    CoreError::InvalidAuthorizationSlots
                );
                used_inline[inline] = true;
                require_eq!(
                    snapshot.authorization_control_offset_or_none,
                    ABSENT_INDEX,
                    CoreError::InvalidAuthorizationSlots
                );
                require_eq!(
                    snapshot.expected_fill_sequence,
                    0,
                    CoreError::AuthorizationFillSequenceMismatch
                );
            }
            WITNESS_STORED_AUTHORIZATION => {
                require_eq!(
                    snapshot.inline_identity_index_or_none,
                    ABSENT_INDEX,
                    CoreError::InvalidAuthorizationSlots
                );
                let control = usize::from(snapshot.authorization_control_offset_or_none);
                require!(
                    control < used_controls.len() && used_controls[control].is_none(),
                    CoreError::InvalidAuthorizationSlots
                );
                used_controls[control] = Some(AuthorizationControlUse::Exclusive);
            }
            _ => return err!(CoreError::UnsupportedAuthorizationWitness),
        }
    }
    for source in intent_funded_sources {
        let snapshot = snapshots
            .get(usize::from(source.authorization_slot))
            .ok_or(CoreError::InvalidAuthorizationSlots)?;
        let allows_unconstrained =
            source.settlement_flags & SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT != 0;
        require!(
            !allows_unconstrained || snapshot.witness_kind == WITNESS_STORED_AUTHORIZATION,
            CoreError::InvalidSettlementAuthorization
        );
        if matches!(
            snapshot.witness_kind,
            WITNESS_EXACT_ONE_SHOT_DELEGATE | WITNESS_STORED_AUTHORIZATION
        ) {
            let control = usize::from(source.spend_authority_control_offset_or_none);
            require!(
                control < used_controls.len() && used_controls[control].is_none(),
                CoreError::InvalidAuthorizationSlots
            );
            used_controls[control] = Some(AuthorizationControlUse::Exclusive);
        } else {
            require_eq!(
                source.spend_authority_control_offset_or_none,
                ABSENT_INDEX,
                CoreError::InvalidAuthorizationSlots
            );
        }
    }
    require!(
        used_inline.iter().all(|used| *used) && used_controls.iter().all(Option::is_some),
        CoreError::InvalidAuthorizationSlots
    );
    Ok(())
}

pub fn validate_direct_witness(
    core_program: &Pubkey,
    top_level: &TopLevelInstructionView,
    expected_top_level_accounts: &[TopLevelAccountMeta],
    expected_instruction_data: &[u8],
    actor: &EffectivePrivilege,
    identity: &IntentIdentityCandidateV0,
    current_slot: u64,
) -> Result<()> {
    identity.validate(core_program)?;
    require_eq!(
        identity.max_fills,
        1,
        CoreError::AuthorizationIdentityMismatch
    );
    require_exact_transaction_root_invocation(
        top_level,
        core_program,
        expected_top_level_accounts,
        expected_instruction_data,
    )?;
    require_keys_eq!(
        actor.key,
        identity.actor,
        CoreError::AuthorizationIdentityMismatch
    );
    require!(
        actor.signer,
        CoreError::DirectAuthorizationNotTransactionRoot
    );
    require!(
        has_transaction_signer(&identity.actor, &top_level.accounts),
        CoreError::DirectAuthorizationNotTransactionRoot
    );
    require!(
        current_slot < identity.expires_at_slot_exclusive,
        CoreError::AuthorizationExpired
    );
    Ok(())
}

pub fn derive_exact_spend_authority(
    core_program: &Pubkey,
    intent_digest: &[u8; 32],
    source: &Pubkey,
) -> Result<(Pubkey, u8)> {
    let seed = compute_intent_spend_seed(intent_digest, &source.to_bytes())
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    Ok(Pubkey::find_program_address(
        &[INTENT_SPEND_SEED, &seed],
        core_program,
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactDelegateObservation {
    pub source: Pubkey,
    pub token_owner: Pubkey,
    pub spend_authority: Pubkey,
    pub delegated_amount_before: u64,
    pub delegated_amount_after: u64,
    pub delegate_present_after: bool,
    pub observed_engine_source_debit: u128,
    pub observed_rate_fee_debit: u128,
}

pub fn validate_exact_delegate_consumption(
    core_program: &Pubkey,
    identity: &IntentIdentityCandidateV0,
    observations: &[ExactDelegateObservation],
) -> Result<()> {
    require!(
        identity.max_fills == 1 && !observations.is_empty(),
        CoreError::ExactDelegateConsumptionMismatch
    );
    for (position, observation) in observations.iter().enumerate() {
        require_keys_eq!(
            observation.token_owner,
            identity.actor,
            CoreError::ExactDelegateConsumptionMismatch
        );
        require!(
            observations[..position]
                .iter()
                .all(|earlier| earlier.source != observation.source),
            CoreError::CrossAuthorizationFundingAlias
        );
        let (expected_authority, _) = derive_exact_spend_authority(
            core_program,
            &identity.intent_digest,
            &observation.source,
        )?;
        require_keys_eq!(
            observation.spend_authority,
            expected_authority,
            CoreError::ExactDelegateConsumptionMismatch
        );
        require!(
            observation.delegated_amount_before != 0,
            CoreError::ExactDelegateConsumptionMismatch
        );
        let total = observation
            .observed_engine_source_debit
            .checked_add(observation.observed_rate_fee_debit)
            .ok_or(CoreError::ArithmeticOverflow)?;
        require_eq!(
            total,
            u128::from(observation.delegated_amount_before),
            CoreError::ExactDelegateConsumptionMismatch
        );
        require!(
            observation.delegated_amount_after == 0 && !observation.delegate_present_after,
            CoreError::ExactDelegateConsumptionMismatch
        );
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoredDelegateObservation {
    pub source: Pubkey,
    pub token_owner: Pubkey,
    pub spend_authority_before: Pubkey,
    pub delegated_amount_before: u64,
    pub delegate_after_or_zero: Pubkey,
    pub delegated_amount_after: u64,
    pub observed_engine_source_debit: u128,
    pub observed_rate_fee_debit: u128,
}

/// Validates the asset-program authority used by one stored fill. Remaining
/// delegation is allowed, but it remains subordinate to the Core-owned replay
/// state and must decrease by the exact protected debit of this fill.
pub fn validate_stored_delegate_fill(
    core_program: &Pubkey,
    identity: &IntentIdentityCandidateV0,
    observation: &StoredDelegateObservation,
) -> Result<()> {
    require_keys_eq!(
        observation.token_owner,
        identity.actor,
        CoreError::ExactDelegateConsumptionMismatch
    );
    let (expected_authority, _) =
        derive_exact_spend_authority(core_program, &identity.intent_digest, &observation.source)?;
    require_keys_eq!(
        observation.spend_authority_before,
        expected_authority,
        CoreError::ExactDelegateConsumptionMismatch
    );
    let consumed = observation
        .observed_engine_source_debit
        .checked_add(observation.observed_rate_fee_debit)
        .ok_or(CoreError::ArithmeticOverflow)?;
    let consumed = u64::try_from(consumed).map_err(|_| CoreError::AmountConversionFailed)?;
    require!(consumed != 0, CoreError::ExactDelegateConsumptionMismatch);
    let expected_after = observation
        .delegated_amount_before
        .checked_sub(consumed)
        .ok_or(CoreError::ExactDelegateConsumptionMismatch)?;
    require_eq!(
        observation.delegated_amount_after,
        expected_after,
        CoreError::ExactDelegateConsumptionMismatch
    );
    if expected_after == 0 {
        require_keys_eq!(
            observation.delegate_after_or_zero,
            Pubkey::default(),
            CoreError::ExactDelegateConsumptionMismatch
        );
    } else {
        require_keys_eq!(
            observation.delegate_after_or_zero,
            expected_authority,
            CoreError::ExactDelegateConsumptionMismatch
        );
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizationView {
    pub intent_digest: [u8; 32],
    pub actor: Pubkey,
    pub core_terms_root: [u8; 32],
    pub fill_sequence: u32,
    pub successful_fills: u32,
    pub remaining_fills: u32,
    pub expires_at_slot_exclusive: u64,
    pub lifecycle: u8,
    pub capability_state_root: [u8; 32],
    pub fee_state_root: [u8; 32],
    pub stored_authorization_key_or_zero: Pubkey,
    /// Exact local-term endpoints used only for complete mapping and alias
    /// checks. They are already committed by `core_terms_root`.
    pub endpoints: Vec<Pubkey>,
}

impl AuthorizationView {
    pub fn state_digest(&self) -> Result<[u8; 32]> {
        require_eq!(
            self.successful_fills,
            self.fill_sequence,
            CoreError::AuthorizationFillSequenceMismatch
        );
        compute_authorization_state_digest(AuthorizationStateDigestInputs {
            intent_digest: &self.intent_digest,
            lifecycle: self.lifecycle,
            fill_sequence: self.fill_sequence,
            successful_fills: self.successful_fills,
            remaining_fills: self.remaining_fills,
            capability_state_root: &self.capability_state_root,
            fee_state_root: &self.fee_state_root,
            stored_authorization_key_or_zero: &self.stored_authorization_key_or_zero.to_bytes(),
        })
        .map_err(|_| error!(CoreError::InvalidWireEncoding))
    }
}

pub fn authorization_view_from_stored(
    core_program: &Pubkey,
    state: &StoredAuthorizationCompactCandidateV0,
    stored_authorization_key: &Pubkey,
    current_slot: u64,
) -> Result<AuthorizationView> {
    state.validate_account(core_program, stored_authorization_key)?;
    require!(
        state.lifecycle()? == StoredAuthorizationLifecycle::Active,
        CoreError::AuthorizationUnavailable
    );
    require!(
        current_slot < state.identity.expires_at_slot_exclusive,
        CoreError::AuthorizationExpired
    );
    let remaining_fills = state
        .identity
        .max_fills
        .checked_sub(state.header.fill_sequence)
        .ok_or(CoreError::AuthorizationFillSequenceMismatch)?;
    Ok(AuthorizationView {
        intent_digest: state.identity.intent_digest,
        actor: state.identity.actor,
        core_terms_root: state.identity.core_terms_root,
        fill_sequence: state.header.fill_sequence,
        successful_fills: state.header.fill_sequence,
        remaining_fills,
        expires_at_slot_exclusive: state.identity.expires_at_slot_exclusive,
        lifecycle: state.header.lifecycle,
        capability_state_root: state.capability_state_root()?,
        fee_state_root: state.fee_state_root()?,
        stored_authorization_key_or_zero: *stored_authorization_key,
        endpoints: state
            .immutable_terms
            .iter()
            .map(|term| term.endpoint_key)
            .collect(),
    })
}

pub fn authorization_view_from_ephemeral(
    core_program: &Pubkey,
    identity: &IntentIdentityCandidateV0,
    capability_states: &[AuthorizationCapabilityStateRowCandidateV0],
    fee_states: &[AuthorizationFeeStateRowCandidateV0],
    endpoints: &[Pubkey],
    current_slot: u64,
) -> Result<AuthorizationView> {
    identity.validate(core_program)?;
    require_eq!(
        identity.max_fills,
        1,
        CoreError::AuthorizationIdentityMismatch
    );
    require!(
        current_slot < identity.expires_at_slot_exclusive,
        CoreError::AuthorizationExpired
    );
    require_eq!(
        capability_states.len(),
        endpoints.len(),
        CoreError::AuthorizationIdentityMismatch
    );
    let capability_state_root = compute_authorization_capability_state_root(capability_states)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    let fee_state_root = compute_authorization_fee_state_root(fee_states)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    Ok(AuthorizationView {
        intent_digest: identity.intent_digest,
        actor: identity.actor,
        core_terms_root: identity.core_terms_root,
        fill_sequence: 0,
        successful_fills: 0,
        remaining_fills: 1,
        expires_at_slot_exclusive: identity.expires_at_slot_exclusive,
        lifecycle: StoredAuthorizationLifecycle::ACTIVE,
        capability_state_root,
        fee_state_root,
        stored_authorization_key_or_zero: Pubkey::default(),
        endpoints: endpoints.to_vec(),
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedAuthorizationSet {
    pub views: Vec<AuthorizationView>,
    pub intent_set_digest: [u8; 32],
    pub authorization_view_set_digest: [u8; 32],
}

pub fn normalize_authorization_set(
    views_in_declared_slot_order: &[AuthorizationView],
    domain_set_digest: &[u8; 32],
) -> Result<NormalizedAuthorizationSet> {
    require!(
        !views_in_declared_slot_order.is_empty()
            && views_in_declared_slot_order.len() <= MAX_INTENTS,
        CoreError::InvalidAuthorizationSlots
    );
    for (position, view) in views_in_declared_slot_order.iter().enumerate() {
        if position != 0 {
            require!(
                views_in_declared_slot_order[position - 1].intent_digest < view.intent_digest,
                CoreError::InvalidAuthorizationSlots
            );
        }
        for earlier in &views_in_declared_slot_order[..position] {
            require!(
                earlier.stored_authorization_key_or_zero == Pubkey::default()
                    || view.stored_authorization_key_or_zero == Pubkey::default()
                    || earlier.stored_authorization_key_or_zero
                        != view.stored_authorization_key_or_zero,
                CoreError::DuplicateIntentDigest
            );
            require!(
                earlier
                    .endpoints
                    .iter()
                    .all(|left| view.endpoints.iter().all(|right| left != right)),
                CoreError::CrossAuthorizationFundingAlias
            );
        }
    }
    let intent_rows = views_in_declared_slot_order
        .iter()
        .map(|view| IntentSetRowCandidateV0 {
            intent_digest: view.intent_digest,
        })
        .collect::<Vec<_>>();
    let intent_set_digest = compute_intent_set_digest(domain_set_digest, &intent_rows)
        .map_err(|_| error!(CoreError::InvalidAuthorizationSlots))?;
    let view_rows = views_in_declared_slot_order
        .iter()
        .enumerate()
        .map(|(position, view)| {
            Ok(AuthorizationViewRowCandidateV0 {
                authorization_slot: u8::try_from(position)
                    .map_err(|_| CoreError::ExperimentLimitExceeded)?,
                intent_digest: view.intent_digest,
                authorization_state_digest: view.state_digest()?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let authorization_view_set_digest = compute_authorization_view_set_digest(&view_rows)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    Ok(NormalizedAuthorizationSet {
        views: views_in_declared_slot_order.to_vec(),
        intent_set_digest,
        authorization_view_set_digest,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtectedExecutionInputs {
    pub core_program: Pubkey,
    pub market_binding_digest: [u8; 32],
    pub loader_state_snapshot_digest: [u8; 32],
    pub domain_set_digest: [u8; 32],
    pub intent_set_digest: [u8; 32],
    pub fee_policy_digest: [u8; 32],
    pub asset_set_digest: [u8; 32],
    pub authorization_view_set_digest: [u8; 32],
    pub fee_shard_set_digest: [u8; 32],
    pub protected_capability_set_digest: [u8; 32],
}

pub fn derive_protected_execution_root(inputs: ProtectedExecutionInputs) -> Result<[u8; 32]> {
    compute_protected_execution_root(ProtectedExecutionRootInputs {
        core_program: &inputs.core_program.to_bytes(),
        market_binding_digest: &inputs.market_binding_digest,
        engine_loader_state_snapshot_digest: &inputs.loader_state_snapshot_digest,
        domain_set_digest: &inputs.domain_set_digest,
        intent_set_digest: &inputs.intent_set_digest,
        fee_policy_digest: &inputs.fee_policy_digest,
        asset_set_digest: &inputs.asset_set_digest,
        authorization_view_set_digest: &inputs.authorization_view_set_digest,
        fee_shard_set_digest: &inputs.fee_shard_set_digest,
        protected_capability_set_digest: &inputs.protected_capability_set_digest,
    })
    .map_err(|_| error!(CoreError::InvalidWireEncoding))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoredCapabilityFill {
    pub local_term_index: u8,
    pub engine_debit: u64,
    pub rate_fee_debit: u64,
    /// Authoritative only for a credit local term; debit rows must keep it zero.
    pub credit: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoredFeeFill {
    pub rounding_group_digest: [u8; 32],
    pub funding_local_term_index: u8,
    pub fee_class: u8,
    pub maximum_fee: u64,
    pub fill_basis: u128,
    pub assessed_fee: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredFillPreview {
    pub next_capabilities: Vec<AuthorizationCapabilityStateCandidateV0>,
    pub next_fee_states: Vec<AuthorizationFeeStateCandidateV0>,
    pub terminal: bool,
}

pub fn preview_stored_fill(
    core_program: &Pubkey,
    stored_authorization_key: &Pubkey,
    state: &StoredAuthorizationCompactCandidateV0,
    capability_fills: &[StoredCapabilityFill],
    fee_fills: &[StoredFeeFill],
) -> Result<StoredFillPreview> {
    state.validate_account(core_program, stored_authorization_key)?;
    require!(
        state.lifecycle()? == StoredAuthorizationLifecycle::Executing,
        CoreError::AuthorizationUnavailable
    );
    preview_stored_fill_validated(state, capability_fills, fee_fills)
}

fn preview_stored_fill_validated(
    state: &StoredAuthorizationCompactCandidateV0,
    capability_fills: &[StoredCapabilityFill],
    fee_fills: &[StoredFeeFill],
) -> Result<StoredFillPreview> {
    require!(
        state.lifecycle()? == StoredAuthorizationLifecycle::Executing,
        CoreError::AuthorizationUnavailable
    );
    let term_count = state.immutable_terms.len();
    let mut next_capabilities = state.capabilities.clone();
    let mut next_fee_states = state.fee_states.clone();
    let mut has_positive_non_fee_delta = false;
    for (position, fill) in capability_fills.iter().enumerate() {
        require!(
            usize::from(fill.local_term_index) < term_count
                && (position == 0
                    || capability_fills[position - 1].local_term_index < fill.local_term_index),
            CoreError::AuthorizationBoundExceeded
        );
        let index = usize::from(fill.local_term_index);
        let term = &state.immutable_terms[index];
        let bound = &mut next_capabilities[index];
        require_eq!(
            bound.local_term_index,
            fill.local_term_index,
            CoreError::AuthorizationIdentityMismatch
        );
        if term.rights_bits & RIGHT_DEBIT != 0 {
            require_eq!(fill.credit, 0, CoreError::AuthorizationBoundExceeded);
            has_positive_non_fee_delta |= fill.engine_debit != 0;
            let total_debit = fill
                .engine_debit
                .checked_add(fill.rate_fee_debit)
                .ok_or(CoreError::ArithmeticOverflow)?;
            require!(
                total_debit <= bound.remaining_total_debit,
                CoreError::AuthorizationBoundExceeded
            );
            if fill.rate_fee_debit != 0 {
                require!(
                    bound.flags
                        & generic_effect_private_wire::AUTHORIZATION_CAPABILITY_STATE_FLAG_FEE_FUNDING
                        != 0,
                    CoreError::FeeCeilingExceeded
                );
            }
            bound.remaining_total_debit -= total_debit;
            bound.cumulative_engine_debit = bound
                .cumulative_engine_debit
                .checked_add(u128::from(fill.engine_debit))
                .ok_or(CoreError::ArithmeticOverflow)?;
            bound.cumulative_fee_debit = bound
                .cumulative_fee_debit
                .checked_add(u128::from(fill.rate_fee_debit))
                .ok_or(CoreError::ArithmeticOverflow)?;
            require!(
                bound.cumulative_engine_debit <= u128::from(bound.initial_maximum_engine_debit),
                CoreError::AuthorizationBoundExceeded
            );
        } else if term.rights_bits & RIGHT_CREDIT != 0 {
            require!(
                fill.engine_debit == 0 && fill.rate_fee_debit == 0,
                CoreError::AuthorizationBoundExceeded
            );
            bound.cumulative_credit = bound
                .cumulative_credit
                .checked_add(u128::from(fill.credit))
                .ok_or(CoreError::ArithmeticOverflow)?;
            has_positive_non_fee_delta |= fill.credit != 0;
        } else {
            return err!(CoreError::AuthorizationBoundExceeded);
        }
    }
    require!(
        has_positive_non_fee_delta,
        CoreError::AuthorizationBoundExceeded
    );

    for (position, fill) in fee_fills.iter().enumerate() {
        require!(
            fill.rounding_group_digest != [0; 32]
                && fill.fill_basis != 0
                && (position == 0
                    || fee_fills[position - 1].rounding_group_digest < fill.rounding_group_digest),
            CoreError::AuthorizationBoundExceeded
        );
        let funding_index = usize::from(fill.funding_local_term_index);
        let funding_term = state
            .immutable_terms
            .get(funding_index)
            .filter(|_| funding_index < term_count)
            .ok_or(CoreError::AuthorizationBoundExceeded)?;
        require!(
            funding_term.flags
                & generic_effect_private_wire::INTENT_CAPABILITY_TERM_FLAG_FEE_FUNDING
                != 0
                && funding_term.fee_class == fill.fee_class
                && funding_term.maximum_protocol_fee == fill.maximum_fee,
            CoreError::FeeCeilingExceeded
        );
        let bucket_index = match next_fee_states
            .binary_search_by_key(&fill.rounding_group_digest, |bucket| {
                bucket.rounding_group_digest
            }) {
            Ok(index) => {
                let existing = &next_fee_states[index];
                require!(
                    existing.funding_local_term_index == fill.funding_local_term_index
                        && existing.fee_class == fill.fee_class
                        && existing.maximum_fee == fill.maximum_fee,
                    CoreError::AuthorizationIdentityMismatch
                );
                index
            }
            Err(index) => {
                require!(
                    next_fee_states.len() < MAX_STORED_FEE_STATES,
                    CoreError::ExperimentLimitExceeded
                );
                next_fee_states.insert(
                    index,
                    AuthorizationFeeStateCandidateV0 {
                        rounding_group_digest: fill.rounding_group_digest,
                        funding_local_term_index: fill.funding_local_term_index,
                        fee_class: fill.fee_class,
                        flags: 0,
                        reserved: [0; 5],
                        cumulative_basis: 0,
                        cumulative_assessed_fee: 0,
                        maximum_fee: fill.maximum_fee,
                    },
                );
                index
            }
        };
        let bucket = &mut next_fee_states[bucket_index];
        bucket.cumulative_basis = bucket
            .cumulative_basis
            .checked_add(fill.fill_basis)
            .ok_or(CoreError::ArithmeticOverflow)?;
        bucket.cumulative_assessed_fee = bucket
            .cumulative_assessed_fee
            .checked_add(u128::from(fill.assessed_fee))
            .ok_or(CoreError::ArithmeticOverflow)?;
        require!(
            bucket.cumulative_assessed_fee <= u128::from(bucket.maximum_fee),
            CoreError::FeeCeilingExceeded
        );
    }

    for (local_index, (next_capability, previous_capability)) in next_capabilities[..term_count]
        .iter()
        .zip(&state.capabilities[..term_count])
        .enumerate()
    {
        let capability_fee_delta = next_capability
            .cumulative_fee_debit
            .checked_sub(previous_capability.cumulative_fee_debit)
            .ok_or(CoreError::ArithmeticOverflow)?;
        let assessed_fee_delta = fee_fills
            .iter()
            .filter(|fill| usize::from(fill.funding_local_term_index) == local_index)
            .try_fold(0_u128, |sum, fill| {
                sum.checked_add(u128::from(fill.assessed_fee))
                    .ok_or(CoreError::ArithmeticOverflow)
            })?;
        require_eq!(
            capability_fee_delta,
            assessed_fee_delta,
            CoreError::FeeLiabilityMismatch
        );
    }

    let next_capability_rows = next_capabilities
        .iter()
        .map(AuthorizationCapabilityStateCandidateV0::wire_row)
        .collect::<Result<Vec<_>>>()?;
    compute_authorization_capability_state_root(&next_capability_rows)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    let next_fee_rows = next_fee_states
        .iter()
        .map(AuthorizationFeeStateCandidateV0::wire_row)
        .collect::<Result<Vec<_>>>()?;
    compute_authorization_fee_state_root(&next_fee_rows)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;

    validate_all_cumulative_constraints(state, &next_capabilities, false)?;
    let active = next_capabilities.as_slice();
    let debit_terms = state.immutable_terms[..term_count]
        .iter()
        .zip(active)
        .filter(|(term, _)| term.rights_bits & RIGHT_DEBIT != 0)
        .collect::<Vec<_>>();
    let economic_debits_exhausted = !debit_terms.is_empty()
        && debit_terms.iter().all(|(_, bound)| {
            bound.remaining_total_debit == 0
                || bound.cumulative_engine_debit == u128::from(bound.initial_maximum_engine_debit)
        });
    let reaches_max_fills =
        state.header.fill_sequence.saturating_add(1) >= state.identity.max_fills;
    let terminal = economic_debits_exhausted || reaches_max_fills;
    if terminal {
        validate_all_cumulative_constraints(state, &next_capabilities, true)?;
    }
    Ok(StoredFillPreview {
        next_capabilities,
        next_fee_states,
        terminal,
    })
}

pub fn finalize_stored_fill_exact(
    account: &AccountInfo<'_>,
    core_program: &Pubkey,
    execution_digest: &[u8; 32],
    capability_fills: &[StoredCapabilityFill],
    fee_fills: &[StoredFeeFill],
) -> Result<bool> {
    let state = read_stored_authorization_compact(account, core_program)?;
    let preview = preview_stored_fill(
        core_program,
        account.key,
        &state,
        capability_fills,
        fee_fills,
    )?;
    commit_stored_authorization_execution_exact(
        account,
        core_program,
        execution_digest,
        &preview.next_capabilities,
        &preview.next_fee_states,
        preview.terminal,
    )?;
    Ok(preview.terminal)
}

fn validate_all_cumulative_constraints(
    state: &StoredAuthorizationCompactCandidateV0,
    capabilities: &[AuthorizationCapabilityStateCandidateV0],
    terminal: bool,
) -> Result<()> {
    for constraint in &state.credit_constraints {
        let credit_index = usize::from(constraint.credit_local_term_index);
        let credit_state = capabilities
            .get(credit_index)
            .filter(|_| credit_index < state.immutable_terms.len())
            .ok_or(CoreError::AuthorizationBoundExceeded)?;
        require!(
            credit_state.cumulative_engine_debit == 0 && credit_state.cumulative_fee_debit == 0,
            CoreError::AuthorizationBoundExceeded
        );
        let mut cumulative_group_debit = 0_u128;
        for (source_index, source) in capabilities.iter().enumerate().filter(|(source_index, _)| {
            constraint.debit_source_bitmap & (1_u16 << source_index) != 0
        }) {
            require!(
                state.immutable_terms[source_index].rights_bits & RIGHT_DEBIT != 0,
                CoreError::AuthorizationBoundExceeded
            );
            require_eq!(
                source.cumulative_credit,
                0,
                CoreError::AuthorizationBoundExceeded
            );
            cumulative_group_debit = cumulative_group_debit
                .checked_add(source.cumulative_engine_debit)
                .ok_or(CoreError::ArithmeticOverflow)?;
        }
        let numerator =
            U256::from(cumulative_group_debit) * U256::from(constraint.minimum_credit_numerator);
        require!(
            constraint.nonzero_debit_denominator != 0,
            CoreError::AuthorizationBoundExceeded
        );
        let denominator = U256::from(constraint.nonzero_debit_denominator);
        let required_prefix = if numerator.is_zero() {
            U256::zero()
        } else {
            numerator
                .checked_add(denominator - U256::one())
                .ok_or(CoreError::ArithmeticOverflow)?
                / denominator
        };
        require!(
            U256::from(credit_state.cumulative_credit) >= required_prefix,
            CoreError::CapabilityMinimumCreditNotMet
        );
        if terminal {
            let signed_minimum = state.immutable_terms[credit_index].minimum_credit;
            let required_terminal = u128::from(signed_minimum)
                .checked_add(u128::from(constraint.terminal_absolute_minimum))
                .ok_or(CoreError::ArithmeticOverflow)?;
            require!(
                credit_state.cumulative_credit >= required_terminal,
                CoreError::CapabilityMinimumCreditNotMet
            );
        }
    }
    if terminal {
        for (credit_index, term) in state
            .immutable_terms
            .iter()
            .enumerate()
            .filter(|(_, term)| term.rights_bits & RIGHT_CREDIT != 0)
        {
            if state
                .credit_constraints
                .iter()
                .any(|constraint| usize::from(constraint.credit_local_term_index) == credit_index)
            {
                continue;
            }
            require!(
                capabilities[credit_index].cumulative_credit >= u128::from(term.minimum_credit),
                CoreError::CapabilityMinimumCreditNotMet
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use generic_effect_private_wire::StoredAuthorizationHeaderCandidateV0;

    use crate::state::{StoredCreditConstraintCandidateV0, StoredIntentCapabilityTermCandidateV0};

    fn compact_executing_state(
        term_count: usize,
        constraint_count: usize,
    ) -> StoredAuthorizationCompactCandidateV0 {
        StoredAuthorizationCompactCandidateV0 {
            header: StoredAuthorizationHeaderCandidateV0 {
                wire_version: 0,
                lifecycle: StoredAuthorizationLifecycle::EXECUTING,
                bump: 0,
                term_count: u8::try_from(term_count).unwrap(),
                constraint_count: u8::try_from(constraint_count).unwrap(),
                fee_state_count: 0,
                flags: 0,
                reserved: 0,
                term_written_bitmap: if term_count == 0 {
                    0
                } else {
                    (1_u16 << term_count) - 1
                },
                constraint_written_bitmap: if constraint_count == 0 {
                    0
                } else {
                    (1_u16 << constraint_count) - 1
                },
                fill_sequence: 0,
            },
            identity: IntentIdentityCandidateV0::default(),
            pending_execution_digest: [1; 32],
            immutable_terms: vec![StoredIntentCapabilityTermCandidateV0::default(); term_count],
            credit_constraints: vec![
                StoredCreditConstraintCandidateV0::default();
                constraint_count
            ],
            capabilities: vec![AuthorizationCapabilityStateCandidateV0::default(); term_count],
            fee_states: Vec::new(),
        }
    }

    #[test]
    fn spend_authority_is_bound_per_source() {
        let core = Pubkey::new_unique();
        let intent = [7; 32];
        let first = Pubkey::new_unique();
        let second = Pubkey::new_unique();
        assert_ne!(
            derive_exact_spend_authority(&core, &intent, &first)
                .unwrap()
                .0,
            derive_exact_spend_authority(&core, &intent, &second)
                .unwrap()
                .0
        );
    }

    #[test]
    fn exact_delegate_cannot_consume_a_zero_effect() {
        let core = Pubkey::new_unique();
        let source = Pubkey::new_unique();
        let actor = Pubkey::new_unique();
        let identity = IntentIdentityCandidateV0 {
            actor,
            max_fills: 1,
            intent_digest: [7; 32],
            ..Default::default()
        };
        let (spend_authority, _) =
            derive_exact_spend_authority(&core, &identity.intent_digest, &source).unwrap();
        let observation = ExactDelegateObservation {
            source,
            token_owner: actor,
            spend_authority,
            delegated_amount_before: 0,
            delegated_amount_after: 0,
            delegate_present_after: false,
            observed_engine_source_debit: 0,
            observed_rate_fee_debit: 0,
        };
        assert!(validate_exact_delegate_consumption(&core, &identity, &[observation]).is_err());
    }

    #[test]
    fn unconstrained_debit_flag_is_stored_only() {
        let inline = InlineIntentIdentityRowCandidateV0 {
            actor: [7; 32],
            engine_terms_commitment: [8; 32],
            authorization_nonce: 1,
            expires_at_slot_exclusive: 10,
        };
        let direct = AuthorizationSnapshotRowCandidateV0 {
            authorization_slot: 0,
            witness_kind: WITNESS_DIRECT_ACTOR,
            authorization_control_offset_or_none: 0,
            inline_identity_index_or_none: 0,
            expected_fill_sequence: 0,
        };
        let flagged_direct_source = IntentFundedAuthorizationControl {
            authorization_slot: 0,
            spend_authority_control_offset_or_none: ABSENT_INDEX,
            settlement_flags: SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT,
        };
        assert!(validate_authorization_snapshot_rows(
            &[direct],
            &[inline],
            1,
            &[flagged_direct_source],
        )
        .is_err());

        let stored = AuthorizationSnapshotRowCandidateV0 {
            authorization_slot: 0,
            witness_kind: WITNESS_STORED_AUTHORIZATION,
            authorization_control_offset_or_none: 0,
            inline_identity_index_or_none: ABSENT_INDEX,
            expected_fill_sequence: 0,
        };
        let flagged_stored_source = IntentFundedAuthorizationControl {
            authorization_slot: 0,
            spend_authority_control_offset_or_none: 1,
            settlement_flags: SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT,
        };
        assert!(
            validate_authorization_snapshot_rows(&[stored], &[], 2, &[flagged_stored_source])
                .is_ok()
        );
    }

    #[test]
    fn two_sources_share_one_credit_without_double_counting() {
        let mut state = compact_executing_state(3, 1);
        state.identity.max_fills = 2;
        state.credit_constraints[0] = StoredCreditConstraintCandidateV0 {
            constraint_index: 0,
            credit_local_term_index: 2,
            debit_source_bitmap: 0b11,
            nonzero_debit_denominator: 10,
            minimum_credit_numerator: 6,
            ..Default::default()
        };
        state.immutable_terms[0].rights_bits = RIGHT_DEBIT;
        state.immutable_terms[1].rights_bits = RIGHT_DEBIT;
        state.immutable_terms[2].rights_bits = RIGHT_CREDIT;
        state.capabilities[0] = AuthorizationCapabilityStateCandidateV0 {
            local_term_index: 0,
            reserved_0: 0,
            cumulative_engine_debit: 20,
            ..Default::default()
        };
        state.capabilities[1] = AuthorizationCapabilityStateCandidateV0 {
            local_term_index: 1,
            reserved_0: 0,
            cumulative_engine_debit: 30,
            ..Default::default()
        };
        state.capabilities[2] = AuthorizationCapabilityStateCandidateV0 {
            local_term_index: 2,
            reserved_0: 0,
            cumulative_credit: 29,
            ..Default::default()
        };
        assert!(validate_all_cumulative_constraints(&state, &state.capabilities, false).is_err());
        state.capabilities[2].cumulative_credit = 30;
        assert!(validate_all_cumulative_constraints(&state, &state.capabilities, false).is_ok());

        // The same credit may satisfy several independently signed relations,
        // and one source may participate in more than one relation. Required
        // credit is checked per constraint; it is never double-counted by
        // duplicating a source inside one bitmap.
        state.header.constraint_count = 2;
        state.header.constraint_written_bitmap = 0b11;
        state
            .credit_constraints
            .push(StoredCreditConstraintCandidateV0 {
                constraint_index: 1,
                credit_local_term_index: 2,
                debit_source_bitmap: 0b01,
                nonzero_debit_denominator: 1,
                minimum_credit_numerator: 1,
                ..Default::default()
            });
        assert!(validate_all_cumulative_constraints(&state, &state.capabilities, false).is_ok());
    }

    #[test]
    fn fee_state_is_inserted_lazily_and_pure_noop_is_rejected() {
        let mut state = compact_executing_state(1, 0);
        state.identity.max_fills = 2;
        state.immutable_terms[0] = crate::state::StoredIntentCapabilityTermCandidateV0 {
            intent_local_term_index: 0,
            authority_class: crate::constants::AUTHORITY_INTENT_FUNDED_DEBIT,
            fee_class: crate::constants::FEE_CLASS_GROSS_DEBIT_RATE,
            flags: generic_effect_private_wire::INTENT_CAPABILITY_TERM_FLAG_FEE_FUNDING,
            rights_bits: RIGHT_DEBIT,
            endpoint_key: Pubkey::new_unique(),
            asset_binding_digest: [2; 32],
            maximum_engine_debit: 100,
            maximum_total_debit: 110,
            maximum_protocol_fee: 10,
            ..Default::default()
        };
        state.capabilities[0] = AuthorizationCapabilityStateCandidateV0 {
            local_term_index: 0,
            reserved_0: 0,
            flags: generic_effect_private_wire::AUTHORIZATION_CAPABILITY_STATE_FLAG_FEE_FUNDING,
            initial_maximum_engine_debit: 100,
            initial_minimum_credit: 0,
            initial_maximum_total_debit: 110,
            remaining_total_debit: 110,
            cumulative_engine_debit: 0,
            cumulative_fee_debit: 0,
            cumulative_credit: 0,
            reserved: [0; 5],
        };

        assert!(preview_stored_fill_validated(&state, &[], &[]).is_err());

        let preview = preview_stored_fill_validated(
            &state,
            &[StoredCapabilityFill {
                local_term_index: 0,
                engine_debit: 50,
                rate_fee_debit: 5,
                credit: 0,
            }],
            &[StoredFeeFill {
                rounding_group_digest: [3; 32],
                funding_local_term_index: 0,
                fee_class: crate::constants::FEE_CLASS_GROSS_DEBIT_RATE,
                maximum_fee: 10,
                fill_basis: 50,
                assessed_fee: 5,
            }],
        )
        .unwrap();
        assert_eq!(preview.next_fee_states.len(), 1);
        assert_eq!(preview.next_fee_states[0].cumulative_basis, 50);
        assert_eq!(preview.next_fee_states[0].cumulative_assessed_fee, 5);
        assert_eq!(preview.next_capabilities[0].remaining_total_debit, 55);
    }

    #[test]
    fn combined_debit_exhaustion_enforces_terminal_credit_minimum() {
        let mut state = compact_executing_state(2, 1);
        state.identity.max_fills = 2;
        state.immutable_terms[0] = StoredIntentCapabilityTermCandidateV0 {
            intent_local_term_index: 0,
            authority_class: crate::constants::AUTHORITY_INTENT_FUNDED_DEBIT,
            fee_class: crate::constants::FEE_CLASS_GROSS_DEBIT_RATE,
            flags: generic_effect_private_wire::INTENT_CAPABILITY_TERM_FLAG_FEE_FUNDING,
            rights_bits: RIGHT_DEBIT,
            endpoint_key: Pubkey::new_unique(),
            asset_binding_digest: [2; 32],
            maximum_engine_debit: 100,
            maximum_total_debit: 100,
            maximum_protocol_fee: 10,
            ..Default::default()
        };
        state.immutable_terms[1] = StoredIntentCapabilityTermCandidateV0 {
            intent_local_term_index: 1,
            authority_class: crate::constants::AUTHORITY_EXACT_EXTERNAL_CREDIT,
            fee_class: crate::constants::FEE_CLASS_NONE,
            rights_bits: RIGHT_CREDIT,
            endpoint_key: Pubkey::new_unique(),
            asset_binding_digest: [4; 32],
            ..Default::default()
        };
        state.credit_constraints[0] = StoredCreditConstraintCandidateV0 {
            constraint_index: 0,
            credit_local_term_index: 1,
            debit_source_bitmap: 0b01,
            debit_group_root: generic_effect_private_wire::compute_intent_debit_group_root(&[0])
                .unwrap(),
            minimum_credit_numerator: 1,
            nonzero_debit_denominator: 1,
            terminal_absolute_minimum: 100,
            ..Default::default()
        };
        state.capabilities[0] = AuthorizationCapabilityStateCandidateV0 {
            local_term_index: 0,
            flags: generic_effect_private_wire::AUTHORIZATION_CAPABILITY_STATE_FLAG_FEE_FUNDING,
            initial_maximum_engine_debit: 100,
            initial_maximum_total_debit: 100,
            remaining_total_debit: 100,
            ..Default::default()
        };
        state.capabilities[1] = AuthorizationCapabilityStateCandidateV0 {
            local_term_index: 1,
            ..Default::default()
        };
        let fee_fill = [StoredFeeFill {
            rounding_group_digest: [3; 32],
            funding_local_term_index: 0,
            fee_class: crate::constants::FEE_CLASS_GROSS_DEBIT_RATE,
            maximum_fee: 10,
            fill_basis: 90,
            assessed_fee: 10,
        }];
        let fills = |credit| {
            [
                StoredCapabilityFill {
                    local_term_index: 0,
                    engine_debit: 90,
                    rate_fee_debit: 10,
                    credit: 0,
                },
                StoredCapabilityFill {
                    local_term_index: 1,
                    engine_debit: 0,
                    rate_fee_debit: 0,
                    credit,
                },
            ]
        };

        let insufficient_terminal_credit =
            preview_stored_fill_validated(&state, &fills(90), &fee_fill).unwrap_err();
        match insufficient_terminal_credit {
            anchor_lang::error::Error::AnchorError(error) => {
                assert_eq!(error.error_name, "CapabilityMinimumCreditNotMet");
            }
            other => panic!("unexpected terminal-credit error: {other:?}"),
        }

        let preview = preview_stored_fill_validated(&state, &fills(100), &fee_fill).unwrap();
        assert!(preview.terminal);
        assert_eq!(preview.next_capabilities[0].remaining_total_debit, 0);
        assert_eq!(preview.next_capabilities[0].cumulative_engine_debit, 90);
        assert_eq!(preview.next_capabilities[0].cumulative_fee_debit, 10);
        assert_eq!(preview.next_capabilities[1].cumulative_credit, 100);
    }

    #[test]
    fn credit_only_authorization_remains_active_until_fill_limit() {
        let mut state = compact_executing_state(1, 0);
        state.identity.max_fills = 2;
        state.immutable_terms[0].rights_bits = RIGHT_CREDIT;
        state.capabilities[0].local_term_index = 0;

        let fill = [StoredCapabilityFill {
            local_term_index: 0,
            engine_debit: 0,
            rate_fee_debit: 0,
            credit: 1,
        }];
        let first = preview_stored_fill_validated(&state, &fill, &[]).unwrap();
        assert!(!first.terminal);

        state.header.fill_sequence = 1;
        let second = preview_stored_fill_validated(&state, &fill, &[]).unwrap();
        assert!(second.terminal);
    }
}
