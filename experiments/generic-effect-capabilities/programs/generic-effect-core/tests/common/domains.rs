use anchor_lang::solana_program::program_option::COption;
use generic_effect_private_wire::{
    compute_asset_set_digest, compute_authorization_capability_state_root,
    compute_authorization_fee_state_root, compute_authorization_state_digest,
    compute_authorization_view_set_digest, compute_domain_set_digest,
    compute_exact_engine_instance_policy_digest, compute_fee_shard_set_digest,
    compute_intent_capability_terms_root, compute_intent_core_terms_root,
    compute_intent_credit_constraints_root, compute_intent_digest, compute_intent_set_digest,
    compute_open_domain_admission_digest, compute_open_domain_rule_digest,
    compute_protected_execution_root, derive_callback_authority_for_engine,
    AssetBindingRowCandidateV0, AuthorizationCapabilityStateRowCandidateV0,
    AuthorizationStateDigestInputs, AuthorizationViewRowCandidateV0, DomainAdmissionCandidateV0,
    DomainControlRowCandidateV0, DomainExecutionRowCandidateV0, EngineContextRowCandidateV0,
    EngineDomainRowCandidateV0, FeeShardDigestRowCandidateV0, IntentCapabilityTermRowCandidateV0,
    IntentCoreTermsDigestInputs, IntentDigestInputs, IntentSetRowCandidateV0,
    ProtectedExecutionRootInputs, SettlementCapabilityRowCandidateV0, ADMISSION_CLOSED,
    ADMISSION_OPEN, AUTHORITY_DOMAIN_ACCOUNTED, AUTHORITY_EXACT_EXTERNAL_CREDIT,
    AUTHORIZATION_LIFECYCLE_ACTIVE, DOMAIN_RULE_CLOSED, DOMAIN_RULE_OPEN, FEE_CLASS_NONE,
    NONE_INDEX, RIGHT_CREDIT, RIGHT_DEBIT, RIGHT_DOMAIN_ACCOUNTED, WIRE_VERSION,
};
use solana_clock::Clock;
use solana_message::AccountMeta;
use solana_signer::Signer;

use programmable_generic_effect_core::{
    account_segments::EffectivePrivilege,
    capabilities::{
        validate_settlement_capabilities, AssetProfileIdentity, CapabilityValidationContext,
        DomainCapabilityIdentity, SettlementCapability,
    },
    constants::{EXPERIMENTAL_MAJOR, MAX_ASSETS},
    state::{
        DomainAccountingAssetSlotCandidateV0, DomainAccountingCandidateV0,
        DomainAdmissionAccountCandidateV0, DomainDescriptorAccountCandidateV0,
        FeeLiabilityLedgerCandidateV0, FeeShardDescriptorCandidateV0, MarketDescriptorCandidateV0,
    },
    token_settlement::ClassicSplEndpointSnapshot,
};

use super::{
    build_core_execute_instruction, create_token_account, fixture_keypair, install_anchor_account,
    mint_tokens, read_anchor_account, token_state, CoreExecuteAccountClosure, DirectFixture,
    SbfArtifacts,
};

pub const DOMAIN_DONATION: u64 = 11_000;
pub const DOMAIN_ACCOUNTED_LIQUIDITY: u64 = 13_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixtureDomainRule {
    Open,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FixtureDomainFlow {
    Credit {
        donation_before: u64,
    },
    Debit {
        accounted_before: u64,
        donation_before: u64,
    },
}

/// Exact-SBF fixture for one authenticated domain and one classic-SPL asset.
///
/// The underlying DIRECT graph supplies the immutable Engine, market, fee,
/// authorization, ALT, and transaction-root machinery. This adapter replaces
/// exactly one settlement endpoint with a DomainAccounting-owned endpoint and
/// reconstructs every commitment from the installed account state.
pub struct DomainFixture {
    pub direct: DirectFixture,
    pub rule: FixtureDomainRule,
    pub descriptor: anchor_lang::prelude::Pubkey,
    pub accounting: anchor_lang::prelude::Pubkey,
    pub admission: Option<anchor_lang::prelude::Pubkey>,
    pub nonparticipating_admission: Option<anchor_lang::prelude::Pubkey>,
    pub accounted_before: u64,
    pub donation_before: u64,
}

impl DomainFixture {
    pub fn open_credit(
        artifacts: &SbfArtifacts,
        transfer_amount: u64,
        donation_before: u64,
    ) -> Self {
        Self::new(
            artifacts,
            FixtureDomainRule::Open,
            transfer_amount,
            FixtureDomainFlow::Credit { donation_before },
        )
    }

    pub fn closed_credit(artifacts: &SbfArtifacts, transfer_amount: u64) -> Self {
        Self::new(
            artifacts,
            FixtureDomainRule::Closed,
            transfer_amount,
            FixtureDomainFlow::Credit { donation_before: 0 },
        )
    }

    pub fn closed_debit(
        artifacts: &SbfArtifacts,
        transfer_amount: u64,
        accounted_before: u64,
        donation_before: u64,
    ) -> Self {
        assert!(
            transfer_amount <= accounted_before,
            "successful closed-domain debit must fit its local accounted liquidity"
        );
        Self::new(
            artifacts,
            FixtureDomainRule::Closed,
            transfer_amount,
            FixtureDomainFlow::Debit {
                accounted_before,
                donation_before,
            },
        )
    }

    pub fn open_debit(
        artifacts: &SbfArtifacts,
        transfer_amount: u64,
        accounted_before: u64,
        donation_before: u64,
    ) -> Self {
        assert!(
            transfer_amount > accounted_before,
            "donation-boundary fixture must attempt more than accounted liquidity"
        );
        Self::new(
            artifacts,
            FixtureDomainRule::Open,
            transfer_amount,
            FixtureDomainFlow::Debit {
                accounted_before,
                donation_before,
            },
        )
    }

    fn new(
        artifacts: &SbfArtifacts,
        rule: FixtureDomainRule,
        transfer_amount: u64,
        flow: FixtureDomainFlow,
    ) -> Self {
        let mut direct = DirectFixture::accepted(artifacts, transfer_amount);
        let market: MarketDescriptorCandidateV0 = read_anchor_account(&direct.svm, &direct.market);
        let descriptor = fixture_keypair(match rule {
            FixtureDomainRule::Open => 70,
            FixtureDomainRule::Closed => 71,
        })
        .pubkey();
        let (accounting, accounting_bump) = DomainAccountingCandidateV0::address(
            &programmable_generic_effect_core::ID,
            &descriptor,
        );
        let protected_profile_digest = market.protected_profile_digest;
        let accounting_profile_digest = [0x47; 32];
        let admission_rule_digest = match rule {
            FixtureDomainRule::Open => {
                compute_open_domain_rule_digest().expect("derive canonical open-domain rule")
            }
            FixtureDomainRule::Closed => [0x48; 32],
        };
        let descriptor_state = DomainDescriptorAccountCandidateV0 {
            wire_version: WIRE_VERSION,
            rule_kind: match rule {
                FixtureDomainRule::Open => DOMAIN_RULE_OPEN,
                FixtureDomainRule::Closed => DOMAIN_RULE_CLOSED,
            },
            reserved: [0; 6],
            controller_program: callback_capability_probe::ID,
            controller_identity: fixture_keypair(72).pubkey(),
            domain_revision: 1,
            namespace_or_instance: [0x41; 32],
            custody_profile_digest: [0x42; 32],
            asset_profile_digest: [0x43; 32],
            accounting_profile_digest,
            exit_class_digest: [0x44; 32],
            admission_rule_digest,
            protected_profile_digest,
        };
        let descriptor_digest = descriptor_state
            .digest(&programmable_generic_effect_core::ID)
            .expect("derive installed domain descriptor digest");

        let (accounted_before, donation_before, domain_endpoint) = match flow {
            FixtureDomainFlow::Credit { donation_before } => {
                let endpoint =
                    create_token_account(&mut direct.svm, &direct.payer, &direct.mint, &accounting);
                if donation_before != 0 {
                    mint_tokens(
                        &mut direct.svm,
                        &direct.payer,
                        &direct.mint,
                        &endpoint,
                        donation_before,
                    );
                }
                direct.destination = endpoint;
                (0, donation_before, endpoint)
            }
            FixtureDomainFlow::Debit {
                accounted_before,
                donation_before,
            } => {
                let endpoint =
                    create_token_account(&mut direct.svm, &direct.payer, &direct.mint, &accounting);
                let raw_balance = accounted_before
                    .checked_add(donation_before)
                    .expect("domain raw fixture balance");
                mint_tokens(
                    &mut direct.svm,
                    &direct.payer,
                    &direct.mint,
                    &endpoint,
                    raw_balance,
                );
                direct.source = endpoint;
                direct.protocol_fee = 0;
                direct.maximum_protocol_fee = 0;
                (accounted_before, donation_before, endpoint)
            }
        };

        let mut accounting_assets = [DomainAccountingAssetSlotCandidateV0::default(); MAX_ASSETS];
        accounting_assets[0] = DomainAccountingAssetSlotCandidateV0 {
            domain_asset_slot: 0,
            reserved: [0; 7],
            asset_identity: direct.mint,
            asset_program: litesvm_token::TOKEN_ID,
            settlement_profile_digest: protected_profile_digest,
            accounted_amount: u128::from(accounted_before),
        };
        let accounting_state = DomainAccountingCandidateV0 {
            wire_version: WIRE_VERSION,
            asset_count: 1,
            bump: accounting_bump,
            reserved: [0; 5],
            domain_descriptor: descriptor,
            domain_revision: descriptor_state.domain_revision,
            assets: accounting_assets,
        };
        accounting_state
            .validate_authenticated(
                &programmable_generic_effect_core::ID,
                &accounting,
                &descriptor,
                descriptor_state.domain_revision,
            )
            .expect("validate installed domain accounting state");

        let current_slot = direct.svm.get_sysvar::<Clock>().slot;
        let (admission, admission_digest, nonparticipating_admission) = match rule {
            FixtureDomainRule::Open => (
                None,
                compute_open_domain_admission_digest(
                    &descriptor_digest,
                    &direct.engine_request.header.market_binding_digest,
                )
                .expect("derive canonical open-domain admission"),
                None,
            ),
            FixtureDomainRule::Closed => {
                let exact_instance_policy = market
                    .exact_engine_instance_policy_digest(
                        &programmable_generic_effect_core::ID,
                        &direct.market,
                    )
                    .expect("derive market's exact Engine instance policy");
                let row = DomainAdmissionCandidateV0 {
                    wire_version: WIRE_VERSION,
                    domain_descriptor: descriptor.to_bytes(),
                    domain_revision: descriptor_state.domain_revision,
                    market: direct.market.to_bytes(),
                    engine_program: market.engine_program.to_bytes(),
                    engine_interface_id: market.engine_interface_id,
                    engine_instance_policy_digest: exact_instance_policy,
                    engine_admission_policy_digest: market.engine_admission_policy_digest,
                    settlement_profile_digest: protected_profile_digest,
                    admission_rule_digest,
                    active_from_slot: current_slot,
                    expires_at_slot_or_zero: 0,
                    revoked_at_slot_or_zero: 0,
                };
                let state = admission_state(row);
                let (key, _) = DomainAdmissionAccountCandidateV0::address(
                    &programmable_generic_effect_core::ID,
                    &row,
                )
                .expect("derive participating admission account");
                let digest = row.digest().expect("derive participating admission digest");
                install_anchor_account(
                    &mut direct.svm,
                    key,
                    programmable_generic_effect_core::ID,
                    &state,
                    DomainAdmissionAccountCandidateV0::SPACE,
                );

                // A fully canonical record and address for another Engine is
                // stronger evidence than a random wrong account: it must still
                // fail the participating market's exact admission relation.
                let nonparticipating_engine = hostile_router_probe::ID;
                let nonparticipating_row = DomainAdmissionCandidateV0 {
                    engine_program: nonparticipating_engine.to_bytes(),
                    engine_instance_policy_digest: compute_exact_engine_instance_policy_digest(
                        &programmable_generic_effect_core::ID.to_bytes(),
                        &nonparticipating_engine.to_bytes(),
                        &market.engine_interface_id,
                        &market.engine_instance_id,
                    )
                    .expect("derive nonparticipating Engine instance policy"),
                    engine_admission_policy_digest: [0x49; 32],
                    ..row
                };
                let nonparticipating_state = admission_state(nonparticipating_row);
                let (nonparticipating_key, _) = DomainAdmissionAccountCandidateV0::address(
                    &programmable_generic_effect_core::ID,
                    &nonparticipating_row,
                )
                .expect("derive nonparticipating admission account");
                install_anchor_account(
                    &mut direct.svm,
                    nonparticipating_key,
                    programmable_generic_effect_core::ID,
                    &nonparticipating_state,
                    DomainAdmissionAccountCandidateV0::SPACE,
                );
                (Some(key), digest, Some(nonparticipating_key))
            }
        };

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

        let domain_execution = DomainExecutionRowCandidateV0 {
            domain_index: 0,
            admission_kind: match rule {
                FixtureDomainRule::Open => ADMISSION_OPEN,
                FixtureDomainRule::Closed => ADMISSION_CLOSED,
            },
            domain_descriptor_key: descriptor.to_bytes(),
            domain_descriptor_digest: descriptor_digest,
            domain_revision: descriptor_state.domain_revision,
            admission_account_or_zero: admission.map_or([0; 32], |key| key.to_bytes()),
            admission_digest,
            accounting_account: accounting.to_bytes(),
            accounting_profile_digest,
        };
        let domain_set_digest = compute_domain_set_digest(
            &direct.engine_request.header.market_binding_digest,
            &[domain_execution],
        )
        .expect("derive one-domain execution set");

        let declarations = domain_declarations(&direct, flow, transfer_amount);
        let asset_row = direct.engine_request.assets[0];
        let asset_binding = AssetBindingRowCandidateV0 {
            wire_version: WIRE_VERSION,
            flags: asset_row.asset_flags,
            decimals: asset_row.decimals,
            reserved: asset_row.reserved,
            asset_identity: asset_row.asset_identity,
            asset_program: asset_row.asset_program,
            settlement_profile_digest: asset_row.settlement_profile_digest,
        };
        let asset_binding_digest = asset_binding.digest().expect("domain asset binding digest");
        let asset_set_digest =
            compute_asset_set_digest(&[asset_binding]).expect("one domain asset set");
        let identity = direct.envelope.inline_intent_identities[0];
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
                    0 => direct.source.to_bytes(),
                    1 => direct.destination.to_bytes(),
                    _ => unreachable!("Core-reserved fee capability is not an intent term"),
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
            .expect("single domain fixture intent term root");
        let credit_constraints_root = compute_intent_credit_constraints_root(&[])
            .expect("empty domain fixture credit constraints");
        let core_terms_root = compute_intent_core_terms_root(IntentCoreTermsDigestInputs {
            maximum_successful_fills: 1,
            capability_terms_root: &capability_terms_root,
            credit_constraints_root: &credit_constraints_root,
        })
        .expect("domain fixture intent core terms");
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
        .expect("domain fixture intent digest");
        let intent_set_digest = compute_intent_set_digest(
            &domain_set_digest,
            &[IntentSetRowCandidateV0 { intent_digest }],
        )
        .expect("domain-bound intent set");

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
        .expect("domain fixture authorization capability state");
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
            .expect("domain fixture authorization state digest");
        let authorization_view_set_digest =
            compute_authorization_view_set_digest(&[AuthorizationViewRowCandidateV0 {
                authorization_slot: 0,
                intent_digest,
                authorization_state_digest,
            }])
            .expect("domain fixture authorization view");

        let asset = AssetProfileIdentity {
            asset_identity: direct.mint,
            asset_program: litesvm_token::TOKEN_ID,
            settlement_profile_digest: protected_profile_digest,
        };
        let domain_identity = DomainCapabilityIdentity {
            domain_index: 0,
            domain_descriptor: descriptor,
            domain_revision: descriptor_state.domain_revision,
            admission_digest,
            accounting_slot: 0,
        };
        let source_before = endpoint_snapshot(&direct, direct.source);
        let destination_before = endpoint_snapshot(&direct, direct.destination);
        let fee_vault_before = endpoint_snapshot(&direct, direct.fee_vault);
        let capabilities = [
            SettlementCapability {
                position: 0,
                declaration: declarations[0],
                core_program: programmable_generic_effect_core::ID,
                experimental_major: EXPERIMENTAL_MAJOR,
                market: direct.market,
                endpoint: token_effective_privilege(direct.source),
                transfer_authority_or_zero: match flow {
                    FixtureDomainFlow::Credit { .. } => direct.actor.pubkey(),
                    FixtureDomainFlow::Debit { .. } => accounting,
                },
                asset,
                domain: matches!(flow, FixtureDomainFlow::Debit { .. }).then_some(domain_identity),
                fee_policy_revision: direct.engine_request.fee_policy.revision,
                lifecycle_digest: source_before
                    .lifecycle_digest()
                    .expect("domain fixture source lifecycle"),
                accounted_before_or_zero: match flow {
                    FixtureDomainFlow::Credit { .. } => 0,
                    FixtureDomainFlow::Debit { .. } => u128::from(accounted_before),
                },
            },
            SettlementCapability {
                position: 1,
                declaration: declarations[1],
                core_program: programmable_generic_effect_core::ID,
                experimental_major: EXPERIMENTAL_MAJOR,
                market: direct.market,
                endpoint: token_effective_privilege(direct.destination),
                transfer_authority_or_zero: Default::default(),
                asset,
                domain: matches!(flow, FixtureDomainFlow::Credit { .. }).then_some(domain_identity),
                fee_policy_revision: direct.engine_request.fee_policy.revision,
                lifecycle_digest: destination_before
                    .lifecycle_digest()
                    .expect("domain fixture destination lifecycle"),
                accounted_before_or_zero: 0,
            },
            SettlementCapability {
                position: 2,
                declaration: declarations[2],
                core_program: programmable_generic_effect_core::ID,
                experimental_major: EXPERIMENTAL_MAJOR,
                market: direct.market,
                endpoint: token_effective_privilege(direct.fee_vault),
                transfer_authority_or_zero: Default::default(),
                asset,
                domain: None,
                fee_policy_revision: direct.engine_request.fee_policy.revision,
                lifecycle_digest: fee_vault_before
                    .lifecycle_digest()
                    .expect("domain fixture fee-vault lifecycle"),
                accounted_before_or_zero: 0,
            },
        ];
        let protected_capability_set_digest = validate_settlement_capabilities(
            &capabilities,
            CapabilityValidationContext {
                core_program: programmable_generic_effect_core::ID,
                market: direct.market,
                classic_token_program: litesvm_token::TOKEN_ID,
                experimental_major: EXPERIMENTAL_MAJOR,
                intent_count: 1,
                asset_count: 1,
                domain_count: 1,
                fee_shard_count: 1,
                fee_policy_revision: direct.engine_request.fee_policy.revision,
            },
        )
        .expect("validate domain fixture protected capabilities");

        let shard: FeeShardDescriptorCandidateV0 =
            read_anchor_account(&direct.svm, &direct.fee_shard_descriptor);
        let liability: FeeLiabilityLedgerCandidateV0 =
            read_anchor_account(&direct.svm, &direct.fee_liability);
        let fee_shard_set_digest = compute_fee_shard_set_digest(&[FeeShardDigestRowCandidateV0 {
            shard_index: 0,
            asset_index: 0,
            vault_settlement_capability_index: 2,
            flags: 0,
            descriptor_key: direct.fee_shard_descriptor.to_bytes(),
            descriptor_digest: shard.descriptor_digest,
            liability_key: direct.fee_liability.to_bytes(),
            vault_key: direct.fee_vault.to_bytes(),
            asset_binding_digest,
            fee_policy_digest: shard.fee_policy_digest,
            recipient_policy_digest: shard.recipient_policy_digest,
            fee_policy_revision: shard.fee_policy_revision,
            liability_before: liability.liability,
        }])
        .expect("domain fixture fee-shard set");
        let protected_execution_root =
            compute_protected_execution_root(ProtectedExecutionRootInputs {
                core_program: &programmable_generic_effect_core::ID.to_bytes(),
                market_binding_digest: &direct.engine_request.header.market_binding_digest,
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
            .expect("domain fixture protected execution root");

        direct.engine_request.header.domain_count = 1;
        direct.engine_request.header.domain_set_digest = domain_set_digest;
        direct.engine_request.header.intent_set_digest = intent_set_digest;
        direct.engine_request.header.protected_execution_root = protected_execution_root;
        direct.engine_request.domains = vec![EngineDomainRowCandidateV0 {
            domain_index: 0,
            domain_descriptor: descriptor.to_bytes(),
            domain_revision: descriptor_state.domain_revision,
            admission_digest,
            accounting_profile_digest,
        }];
        direct.engine_request.intents[0].intent_digest = intent_digest;
        direct.engine_request.contexts = vec![
            engine_context(
                0,
                declarations[0],
                direct.source,
                source_before.amount,
                match flow {
                    FixtureDomainFlow::Credit { .. } => 0,
                    FixtureDomainFlow::Debit { .. } => accounted_before,
                },
            ),
            engine_context(
                1,
                declarations[1],
                direct.destination,
                destination_before.amount,
                0,
            ),
        ];
        direct
            .engine_request
            .validate()
            .expect("domain fixture Engine request is canonical");
        direct.callback_authority =
            derive_callback_authority_for_engine(&direct.engine_request, &effect_engine_probe::ID)
                .expect("derive domain fixture callback authority")
                .0;

        let domain_control = DomainControlRowCandidateV0 {
            descriptor_control_offset: 0,
            admission_control_offset_or_none: match rule {
                FixtureDomainRule::Open => NONE_INDEX,
                FixtureDomainRule::Closed => 1,
            },
            accounting_control_offset: match rule {
                FixtureDomainRule::Open => 1,
                FixtureDomainRule::Closed => 2,
            },
            flags: 0,
        };
        direct.envelope.header.domain_control_account_count = match rule {
            FixtureDomainRule::Open => 2,
            FixtureDomainRule::Closed => 3,
        };
        direct.envelope.header.domain_count = 1;
        direct.envelope.header.intent_set_digest = intent_set_digest;
        direct.envelope.header.domain_set_digest = domain_set_digest;
        direct.envelope.header.protected_execution_root = protected_execution_root;
        direct.envelope.domain_controls = vec![domain_control];
        direct.envelope.settlement_capabilities = declarations.to_vec();

        let mut domain_controls = vec![AccountMeta::new_readonly(descriptor, false)];
        if let Some(key) = admission {
            domain_controls.push(AccountMeta::new_readonly(key, false));
        }
        domain_controls.push(AccountMeta::new(accounting, false));
        let closure = CoreExecuteAccountClosure {
            configuration: direct.configuration,
            market: direct.market,
            fee_policy: direct.fee_policy,
            engine_program: effect_engine_probe::ID,
            callback_authority: direct.callback_authority,
            loader_policy: vec![direct.loader_policy_account],
            domain_controls,
            authorization_controls: vec![AccountMeta::new_readonly(direct.actor.pubkey(), true)],
            protected_profile: vec![litesvm_token::TOKEN_ID, direct.mint],
            fee_controls: vec![
                AccountMeta::new_readonly(direct.fee_shard_descriptor, false),
                AccountMeta::new(direct.fee_liability, false),
            ],
            settlement: vec![
                AccountMeta::new(direct.source, false),
                AccountMeta::new(direct.destination, false),
                AccountMeta::new(direct.fee_vault, false),
            ],
            opaque: vec![],
        };
        direct.instruction = build_core_execute_instruction(&direct.envelope, &closure)
            .expect("build canonical domain Core instruction");

        assert_eq!(
            token_state(&direct.svm, &domain_endpoint).owner,
            accounting,
            "domain settlement endpoint must be controlled only by its accounting PDA"
        );
        Self {
            direct,
            rule,
            descriptor,
            accounting,
            admission,
            nonparticipating_admission,
            accounted_before,
            donation_before,
        }
    }

    pub fn replace_with_nonparticipating_admission(&mut self) {
        let participating = self
            .admission
            .expect("only a closed-domain fixture has an admission account");
        let nonparticipating = self
            .nonparticipating_admission
            .expect("closed-domain fixture installs a nonparticipating record");
        let meta = self
            .direct
            .instruction
            .accounts
            .iter_mut()
            .find(|meta| meta.pubkey == participating)
            .expect("participating admission meta exists");
        assert!(!meta.is_signer && !meta.is_writable);
        meta.pubkey = nonparticipating;
    }

    pub fn protected_state_addresses(&self) -> Vec<anchor_lang::prelude::Pubkey> {
        let mut addresses = self.direct.rollback_state_addresses().to_vec();
        addresses.push(self.descriptor);
        addresses.push(self.accounting);
        if let Some(key) = self.admission {
            addresses.push(key);
        }
        if let Some(key) = self.nonparticipating_admission {
            addresses.push(key);
        }
        addresses
    }

    pub fn compile_v0(
        &mut self,
    ) -> (
        solana_transaction::versioned::VersionedTransaction,
        super::V0MessageResources,
    ) {
        self.direct.compile_v0()
    }
}

fn domain_declarations(
    direct: &DirectFixture,
    flow: FixtureDomainFlow,
    transfer_amount: u64,
) -> [SettlementCapabilityRowCandidateV0; 3] {
    let mut declarations: [SettlementCapabilityRowCandidateV0; 3] = direct
        .envelope
        .settlement_capabilities
        .clone()
        .try_into()
        .expect("direct fixture has exactly three settlement capabilities");
    match flow {
        FixtureDomainFlow::Credit { .. } => {
            declarations[1] = SettlementCapabilityRowCandidateV0 {
                asset_index: 0,
                domain_index_or_none: 0,
                authorization_slot_or_none: NONE_INDEX,
                intent_local_term_index_or_none: NONE_INDEX,
                authority_class: AUTHORITY_DOMAIN_ACCOUNTED,
                fee_shard_index_or_none: NONE_INDEX,
                fee_class: FEE_CLASS_NONE,
                flags: 0,
                rights_bits: RIGHT_CREDIT | RIGHT_DOMAIN_ACCOUNTED,
                domain_accounting_slot_or_none: 0,
                spend_authority_control_offset_or_none: NONE_INDEX,
                reserved_0: 0,
                maximum_engine_debit: 0,
                maximum_total_debit: 0,
                minimum_credit: 0,
                maximum_protocol_fee: 0,
            };
        }
        FixtureDomainFlow::Debit { .. } => {
            declarations[0] = SettlementCapabilityRowCandidateV0 {
                asset_index: 0,
                domain_index_or_none: 0,
                authorization_slot_or_none: NONE_INDEX,
                intent_local_term_index_or_none: NONE_INDEX,
                authority_class: AUTHORITY_DOMAIN_ACCOUNTED,
                fee_shard_index_or_none: NONE_INDEX,
                fee_class: FEE_CLASS_NONE,
                flags: 0,
                rights_bits: RIGHT_DEBIT | RIGHT_DOMAIN_ACCOUNTED,
                domain_accounting_slot_or_none: 0,
                spend_authority_control_offset_or_none: NONE_INDEX,
                reserved_0: 0,
                maximum_engine_debit: transfer_amount,
                maximum_total_debit: transfer_amount,
                minimum_credit: 0,
                maximum_protocol_fee: 0,
            };
            declarations[1].intent_local_term_index_or_none = 0;
            declarations[1].authority_class = AUTHORITY_EXACT_EXTERNAL_CREDIT;
            declarations[1].minimum_credit = transfer_amount;
        }
    }
    declarations
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

fn engine_context(
    position: u8,
    declaration: SettlementCapabilityRowCandidateV0,
    endpoint: anchor_lang::prelude::Pubkey,
    observed_before: u64,
    accounted_before_or_zero: u64,
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
        accounted_before_or_zero,
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
    direct: &DirectFixture,
    key: anchor_lang::prelude::Pubkey,
) -> ClassicSplEndpointSnapshot {
    let state = token_state(&direct.svm, &key);
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
