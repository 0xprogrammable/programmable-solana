mod common;

use anchor_lang::{prelude::Pubkey, solana_program::program_option::COption, AccountSerialize};
use common::{
    build_core_execute_instruction, create_token_account, fixture_keypair, install_raw_account,
    mint_tokens, read_anchor_account, snapshot_accounts, token_balance, token_state,
    CoreExecuteAccountClosure, DomainFixture, ResourceFixture, SbfArtifacts, DIRECT_DEFAULT_AMOUNT,
};
use effect_engine_probe::plan::{encode_explicit_plan, PlannedMove, RECEIPT_ACCEPT};
use generic_effect_private_wire::{
    compute_asset_set_digest, compute_authorization_state_digest,
    compute_authorization_view_set_digest, compute_fee_shard_set_digest, compute_payload_digest,
    compute_protected_execution_root, derive_callback_authority_for_engine,
    AssetBindingRowCandidateV0, AuthorizationStateDigestInputs, AuthorizationViewRowCandidateV0,
    EngineContextRowCandidateV0, FeeShardDigestRowCandidateV0, ProtectedExecutionRootInputs,
    SettlementCapabilityRowCandidateV0, AUTHORITY_DOMAIN_ACCOUNTED, AUTHORIZATION_LIFECYCLE_ACTIVE,
    FEE_CLASS_NONE, NONE_INDEX, RIGHT_CREDIT, RIGHT_DEBIT, RIGHT_DOMAIN_ACCOUNTED, WIRE_VERSION,
};
use litesvm_cpi_tree::CpiTreeExt;
use solana_message::AccountMeta;
use solana_signer::Signer;
use solana_transaction::{InstructionError, TransactionError};

use programmable_generic_effect_core::{
    account_segments::EffectivePrivilege,
    capabilities::{
        validate_settlement_capabilities, AssetProfileIdentity, CapabilityValidationContext,
        DomainCapabilityIdentity, SettlementCapability,
    },
    constants::{EXPERIMENTAL_MAJOR, MAX_ASSETS},
    error::CoreError,
    state::{
        DomainAccountingAssetSlotCandidateV0, DomainAccountingCandidateV0,
        FeeLiabilityLedgerCandidateV0, FeeShardDescriptorCandidateV0,
        StoredAuthorizationCandidateV0,
    },
    token_settlement::ClassicSplEndpointSnapshot,
};

#[test]
fn exact_closed_domain_debit_uses_only_its_authenticated_local_accounting() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let accounted_before = 80_000;
    let donation_before = 11_000;
    let mut fixture = DomainFixture::closed_debit(
        &artifacts,
        DIRECT_DEFAULT_AMOUNT,
        accounted_before,
        donation_before,
    );
    let admission = fixture
        .admission
        .expect("closed-domain debit carries an exact admission account");

    let (transaction, _) = fixture.compile_v0();
    let metadata = fixture
        .direct
        .svm
        .send_transaction(transaction)
        .unwrap_or_else(|failure| {
            panic!(
                "exact closed-domain debit failed: {:?}\n{}\n{}",
                failure.err,
                failure.meta.pretty_logs(),
                failure.meta.pretty_cpi_tree(),
            )
        });

    assert!(program_invoked(&metadata.logs, effect_engine_probe::ID));
    assert!(program_invoked(&metadata.logs, litesvm_token::TOKEN_ID));
    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.direct.source),
        accounted_before + donation_before - DIRECT_DEFAULT_AMOUNT,
    );
    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.direct.destination),
        DIRECT_DEFAULT_AMOUNT,
    );
    let accounting: DomainAccountingCandidateV0 =
        read_anchor_account(&fixture.direct.svm, &fixture.accounting);
    assert_eq!(
        accounting.assets[0].accounted_amount,
        u128::from(accounted_before - DIRECT_DEFAULT_AMOUNT),
        "a successful debit must decrease only the authenticated domain-local ledger",
    );
    let liability: FeeLiabilityLedgerCandidateV0 =
        read_anchor_account(&fixture.direct.svm, &fixture.direct.fee_liability);
    assert_eq!(liability.liability, 0);
    assert!(fixture.direct.svm.get_account(&admission).is_some());
}

#[test]
fn closed_domain_missing_foreign_and_self_declared_admissions_fail_before_engine() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");

    for case in AdmissionFailureCase::ALL {
        let mut fixture = DomainFixture::closed_credit(&artifacts, DIRECT_DEFAULT_AMOUNT);
        let participating = fixture
            .admission
            .expect("closed fixture has a participating admission");
        let mut extra_snapshot = None;
        let expected = match case {
            AdmissionFailureCase::Missing => {
                let absent = fixture_keypair(201).pubkey();
                assert!(fixture.direct.svm.get_account(&absent).is_none());
                replace_account_meta(
                    &mut fixture.direct.instruction.accounts,
                    participating,
                    absent,
                );
                extra_snapshot = Some(absent);
                CoreError::InvalidWireEncoding
            }
            AdmissionFailureCase::ForeignEngine => {
                fixture.replace_with_nonparticipating_admission();
                CoreError::InvalidSettlementDomain
            }
            AdmissionFailureCase::SelfDeclaredClone => {
                let counterfeit = fixture_keypair(202).pubkey();
                let admitted = fixture
                    .direct
                    .svm
                    .get_account(&participating)
                    .expect("participating admission exists");
                install_raw_account(
                    &mut fixture.direct.svm,
                    counterfeit,
                    programmable_generic_effect_core::ID,
                    admitted.data,
                    false,
                );
                replace_account_meta(
                    &mut fixture.direct.instruction.accounts,
                    participating,
                    counterfeit,
                );
                extra_snapshot = Some(counterfeit);
                CoreError::InvalidSettlementDomain
            }
        };

        let mut protected = fixture.protected_state_addresses();
        if let Some(extra) = extra_snapshot {
            protected.push(extra);
        }
        protected.sort_unstable();
        protected.dedup();
        let before = snapshot_accounts(&fixture.direct.svm, &protected);
        let (transaction, _) = fixture.compile_v0();
        let failure = fixture
            .direct
            .svm
            .send_transaction(transaction)
            .expect_err("invalid closed-domain admission unexpectedly reached the Engine");

        assert_eq!(
            failure.err,
            TransactionError::InstructionError(2, core_instruction_error(expected)),
            "typed error for {case:?}",
        );
        assert!(
            !program_invoked(&failure.meta.logs, effect_engine_probe::ID),
            "{case:?} crossed the untrusted Engine boundary:\n{}",
            failure.meta.pretty_logs(),
        );
        assert!(!program_invoked(
            &failure.meta.logs,
            litesvm_token::TOKEN_ID
        ));
        assert_eq!(
            snapshot_accounts(&fixture.direct.svm, &protected),
            before,
            "{case:?} changed protected state",
        );
    }
}

#[test]
fn hostile_engine_move_normal_form_matrix_fails_typed_after_engine_and_before_settlement() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let cases: &[(&str, &[PlannedMove], u8, InstructionError)] = &[
        (
            "zero",
            &[PlannedMove {
                source_capability_index: 0,
                destination_capability_index: 1,
                amount: 0,
            }],
            1,
            core_instruction_error(CoreError::ZeroMoveAmount),
        ),
        (
            "self",
            &[PlannedMove {
                source_capability_index: 0,
                destination_capability_index: 0,
                amount: 1,
            }],
            1,
            core_instruction_error(CoreError::MoveEndpointsIdentical),
        ),
        (
            "out-of-range",
            &[PlannedMove {
                source_capability_index: 3,
                destination_capability_index: 1,
                amount: 1,
            }],
            1,
            core_instruction_error(CoreError::MoveCapabilityIndexOutOfRange),
        ),
        (
            "duplicate-pair-order",
            &[
                PlannedMove {
                    source_capability_index: 0,
                    destination_capability_index: 1,
                    amount: 1,
                },
                PlannedMove {
                    source_capability_index: 0,
                    destination_capability_index: 1,
                    amount: 1,
                },
            ],
            2,
            core_instruction_error(CoreError::NonCanonicalMoveOrder),
        ),
        (
            "cycle-attempt",
            // Canonical capability shapes are directional. Therefore a cycle
            // attempt loses its source right before the redundant graph-side
            // guard could become the first observable error.
            &[
                PlannedMove {
                    source_capability_index: 0,
                    destination_capability_index: 1,
                    amount: 1,
                },
                PlannedMove {
                    source_capability_index: 1,
                    destination_capability_index: 0,
                    amount: 1,
                },
            ],
            2,
            core_instruction_error(CoreError::MoveRightMissing),
        ),
        (
            "authorized-bound-plus-one",
            &[PlannedMove {
                source_capability_index: 0,
                destination_capability_index: 1,
                amount: DIRECT_DEFAULT_AMOUNT + 1,
            }],
            1,
            core_instruction_error(CoreError::CapabilityMaximumDebitExceeded),
        ),
    ];

    for (label, moves, maximum_engine_moves, expected) in cases {
        let mut fixture = common::DirectFixture::accepted(&artifacts, DIRECT_DEFAULT_AMOUNT);
        fixture.envelope.header.maximum_engine_moves = *maximum_engine_moves;
        fixture.engine_request.header.maximum_engine_moves = *maximum_engine_moves;
        let payload = encode_explicit_plan(RECEIPT_ACCEPT, 0, NONE_INDEX, NONE_INDEX, moves)
            .expect("encode deliberately hostile explicit Move receipt");
        fixture.rebuild_payload_and_opaque(payload, vec![]);
        let protected = fixture.rollback_state_addresses();
        let before = snapshot_accounts(&fixture.svm, &protected);

        let (transaction, _) = fixture.compile_v0();
        let failure = fixture
            .svm
            .send_transaction(transaction)
            .expect_err("hostile Move unexpectedly settled");

        assert_eq!(
            failure.err,
            TransactionError::InstructionError(2, expected.clone()),
            "typed Move rejection for {label}",
        );
        assert!(
            program_invoked(&failure.meta.logs, effect_engine_probe::ID),
            "{label} did not cross the authenticated Engine CPI boundary:\n{}",
            failure.meta.pretty_logs(),
        );
        assert!(
            !program_invoked(&failure.meta.logs, litesvm_token::TOKEN_ID),
            "{label} reached token settlement:\n{}",
            failure.meta.pretty_logs(),
        );
        assert_eq!(
            snapshot_accounts(&fixture.svm, &protected),
            before,
            "{label} changed protected state",
        );
    }
}

#[test]
fn hostile_cross_asset_receipt_is_rejected_after_engine_with_full_two_domain_rollback() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = ResourceFixture::new(&artifacts);
    let payload = encode_explicit_plan(
        RECEIPT_ACCEPT,
        0,
        NONE_INDEX,
        NONE_INDEX,
        &[PlannedMove {
            source_capability_index: 0,
            destination_capability_index: 1,
            amount: 1,
        }],
    )
    .expect("encode cross-asset hostile receipt");
    rebuild_resource_payload(&mut fixture, payload);

    let protected = resource_rollback_addresses(&fixture);
    let before = snapshot_accounts(&fixture.direct.svm, &protected);
    let (transaction, _) = fixture.direct.compile_v0();
    let failure = fixture
        .direct
        .svm
        .send_transaction(transaction)
        .expect_err("cross-asset receipt unexpectedly settled");

    assert_eq!(
        failure.err,
        TransactionError::InstructionError(
            2,
            core_instruction_error(CoreError::MoveAssetProfileMismatch),
        ),
    );
    assert!(program_invoked(&failure.meta.logs, effect_engine_probe::ID));
    assert!(!program_invoked(
        &failure.meta.logs,
        litesvm_token::TOKEN_ID
    ));
    assert_eq!(
        snapshot_accounts(&fixture.direct.svm, &protected),
        before,
        "cross-asset receipt changed a protected, domain, authorization, fee, or Engine account",
    );
}

#[test]
fn two_domain_accounting_ledgers_reject_cross_netting_before_any_token_cpi() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = ResourceFixture::new(&artifacts);
    let domain_endpoints = extend_resource_with_domain_accounted_cross_net(&mut fixture);
    let mut protected = resource_rollback_addresses(&fixture);
    protected.extend(domain_endpoints);
    protected.sort_unstable();
    protected.dedup();
    let before = snapshot_accounts(&fixture.direct.svm, &protected);

    let (transaction, _) = fixture.direct.compile_v0();
    let failure = fixture
        .direct
        .svm
        .send_transaction(transaction)
        .expect_err("domain-one credit incorrectly funded domain-zero's local debit");

    assert_eq!(
        failure.err,
        TransactionError::InstructionError(
            2,
            core_instruction_error(CoreError::DomainAccountedLiquidityExceeded),
        ),
    );
    assert!(program_invoked(&failure.meta.logs, effect_engine_probe::ID));
    assert!(
        !program_invoked(&failure.meta.logs, litesvm_token::TOKEN_ID),
        "cross-domain netting reached token settlement:\n{}",
        failure.meta.pretty_logs(),
    );
    assert_eq!(
        snapshot_accounts(&fixture.direct.svm, &protected),
        before,
        "cross-domain netting rejection did not roll back the complete state closure",
    );
}

#[derive(Clone, Copy, Debug)]
enum AdmissionFailureCase {
    Missing,
    ForeignEngine,
    SelfDeclaredClone,
}

impl AdmissionFailureCase {
    const ALL: [Self; 3] = [Self::Missing, Self::ForeignEngine, Self::SelfDeclaredClone];
}

fn replace_account_meta(accounts: &mut [solana_message::AccountMeta], from: Pubkey, to: Pubkey) {
    let meta = accounts
        .iter_mut()
        .find(|meta| meta.pubkey == from)
        .expect("fixture account meta to replace exists");
    assert!(!meta.is_signer && !meta.is_writable);
    meta.pubkey = to;
}

fn rebuild_resource_payload(fixture: &mut ResourceFixture, payload: Vec<u8>) {
    let payload_len = u16::try_from(payload.len()).expect("bounded hostile resource payload");
    fixture.direct.envelope.header.expected_engine_sequence = 0;
    fixture.direct.envelope.header.payload_len = payload_len;
    fixture.direct.envelope.header.payload_digest =
        compute_payload_digest(&payload).expect("derive hostile resource payload digest");
    fixture.direct.envelope.payload = payload.clone();
    fixture.direct.engine_request.header.payload_len = payload_len;
    fixture.direct.engine_request.payload = payload;
    fixture
        .direct
        .engine_request
        .validate()
        .expect("hostile resource request remains structurally canonical");
    let callback_authority = derive_callback_authority_for_engine(
        &fixture.direct.engine_request,
        &effect_engine_probe::ID,
    )
    .expect("derive hostile resource callback authority")
    .0;
    fixture.direct.callback_authority = callback_authority;
    fixture.direct.instruction = build_resource_instruction(fixture, callback_authority, vec![]);
}

fn extend_resource_with_domain_accounted_cross_net(fixture: &mut ResourceFixture) -> [Pubkey; 2] {
    const DOMAIN_ZERO_ACCOUNTED: u64 = 5;
    const DOMAIN_ZERO_RAW_BALANCE: u64 = 20;
    const ATTEMPTED_DEBIT: u64 = 10;

    let asset = fixture.mints[0];
    let profile = fixture.direct.engine_request.assets[0].settlement_profile_digest;
    let domain_zero_source = create_token_account(
        &mut fixture.direct.svm,
        &fixture.direct.payer,
        &asset,
        &fixture.domain_accounting[0],
    );
    let domain_one_destination = create_token_account(
        &mut fixture.direct.svm,
        &fixture.direct.payer,
        &asset,
        &fixture.domain_accounting[1],
    );
    mint_tokens(
        &mut fixture.direct.svm,
        &fixture.direct.payer,
        &asset,
        &domain_zero_source,
        DOMAIN_ZERO_RAW_BALANCE,
    );

    for (index, accounted_amount) in [DOMAIN_ZERO_ACCOUNTED, 0].into_iter().enumerate() {
        let descriptor = fixture.domain_descriptors[index];
        let accounting = fixture.domain_accounting[index];
        let (expected, bump) = DomainAccountingCandidateV0::address(
            &programmable_generic_effect_core::ID,
            &descriptor,
        );
        assert_eq!(accounting, expected);
        let mut assets = [DomainAccountingAssetSlotCandidateV0::default(); MAX_ASSETS];
        assets[0] = DomainAccountingAssetSlotCandidateV0 {
            domain_asset_slot: 0,
            reserved: [0; 7],
            asset_identity: asset,
            asset_program: litesvm_token::TOKEN_ID,
            settlement_profile_digest: profile,
            accounted_amount: u128::from(accounted_amount),
        };
        let state = DomainAccountingCandidateV0 {
            wire_version: WIRE_VERSION,
            asset_count: 1,
            bump,
            reserved: [0; 5],
            domain_descriptor: descriptor,
            domain_revision: fixture.direct.engine_request.domains[index].domain_revision,
            assets,
        };
        state
            .validate_authenticated(
                &programmable_generic_effect_core::ID,
                &accounting,
                &descriptor,
                fixture.direct.engine_request.domains[index].domain_revision,
            )
            .expect("validate the test's exact local accounting state");
        overwrite_anchor_account(
            &mut fixture.direct.svm,
            accounting,
            &state,
            DomainAccountingCandidateV0::SPACE,
        );
    }

    let mut declarations = fixture.direct.envelope.settlement_capabilities.clone();
    declarations.push(domain_accounted_declaration(
        0,
        RIGHT_DEBIT,
        ATTEMPTED_DEBIT,
    ));
    declarations.push(domain_accounted_declaration(1, RIGHT_CREDIT, 0));
    assert_eq!(declarations.len(), 8);

    let endpoint_keys = [
        fixture.sources[0],
        fixture.recipients[1],
        fixture.sources[1],
        fixture.recipients[0],
        fixture.fee_vaults[0],
        fixture.fee_vaults[1],
        domain_zero_source,
        domain_one_destination,
    ];
    let endpoints = endpoint_keys.map(|key| endpoint_snapshot(&fixture.direct.svm, key));
    let asset_bindings = fixture
        .direct
        .engine_request
        .assets
        .iter()
        .copied()
        .map(|row| AssetBindingRowCandidateV0 {
            wire_version: WIRE_VERSION,
            flags: row.asset_flags,
            decimals: row.decimals,
            reserved: row.reserved,
            asset_identity: row.asset_identity,
            asset_program: row.asset_program,
            settlement_profile_digest: row.settlement_profile_digest,
        })
        .collect::<Vec<_>>();
    let asset_binding_digests = asset_bindings
        .iter()
        .map(|binding| binding.digest().expect("derive resource asset binding"))
        .collect::<Vec<_>>();
    let asset_set_digest =
        compute_asset_set_digest(&asset_bindings).expect("derive resource asset set");
    let asset_identities = fixture
        .direct
        .engine_request
        .assets
        .iter()
        .map(|row| AssetProfileIdentity {
            asset_identity: Pubkey::new_from_array(row.asset_identity),
            asset_program: Pubkey::new_from_array(row.asset_program),
            settlement_profile_digest: row.settlement_profile_digest,
        })
        .collect::<Vec<_>>();
    let domain_predicates = fixture
        .direct
        .engine_request
        .domains
        .iter()
        .enumerate()
        .map(|(index, row)| DomainCapabilityIdentity {
            domain_index: u8::try_from(index).expect("bounded resource domain index"),
            domain_descriptor: fixture.domain_descriptors[index],
            domain_revision: row.domain_revision,
            admission_digest: row.admission_digest,
            accounting_slot: NONE_INDEX,
        })
        .collect::<Vec<_>>();
    let fee_policy_revision = fixture.direct.engine_request.fee_policy.revision;
    let capabilities = declarations
        .iter()
        .copied()
        .enumerate()
        .map(|(position, declaration)| {
            let domain = if declaration.domain_index_or_none == NONE_INDEX {
                None
            } else {
                let mut identity = domain_predicates[usize::from(declaration.domain_index_or_none)];
                if declaration.authority_class == AUTHORITY_DOMAIN_ACCOUNTED {
                    identity.accounting_slot = 0;
                }
                Some(identity)
            };
            let transfer_authority_or_zero = match position {
                0 => fixture.spend_authorities[0],
                2 => fixture.spend_authorities[1],
                6 => fixture.domain_accounting[0],
                _ => Pubkey::default(),
            };
            SettlementCapability {
                position: u8::try_from(position).expect("bounded settlement capability index"),
                declaration,
                core_program: programmable_generic_effect_core::ID,
                experimental_major: EXPERIMENTAL_MAJOR,
                market: fixture.direct.market,
                endpoint: token_effective_privilege(endpoint_keys[position]),
                transfer_authority_or_zero,
                asset: asset_identities[usize::from(declaration.asset_index)],
                domain,
                fee_policy_revision,
                lifecycle_digest: endpoints[position]
                    .lifecycle_digest()
                    .expect("derive exact endpoint lifecycle"),
                accounted_before_or_zero: if position == 6 {
                    u128::from(DOMAIN_ZERO_ACCOUNTED)
                } else {
                    0
                },
            }
        })
        .collect::<Vec<_>>();
    let protected_capability_set_digest = validate_settlement_capabilities(
        &capabilities,
        CapabilityValidationContext {
            core_program: programmable_generic_effect_core::ID,
            market: fixture.direct.market,
            classic_token_program: litesvm_token::TOKEN_ID,
            experimental_major: EXPERIMENTAL_MAJOR,
            intent_count: 2,
            asset_count: 2,
            domain_count: 2,
            fee_shard_count: 2,
            fee_policy_revision,
        },
    )
    .expect("validate the cross-net settlement capability set");

    let authorization_view_set_digest = compute_authorization_view_set_digest(
        &(0..2)
            .map(|index| {
                let state: StoredAuthorizationCandidateV0 =
                    read_anchor_account(&fixture.direct.svm, &fixture.authorizations[index]);
                AuthorizationViewRowCandidateV0 {
                    authorization_slot: u8::try_from(index).expect("bounded authorization slot"),
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
                                .expect("derive resource capability state"),
                            fee_state_root: &state
                                .fee_state_root()
                                .expect("derive resource fee state"),
                            stored_authorization_key_or_zero: &fixture.authorizations[index]
                                .to_bytes(),
                        },
                    )
                    .expect("derive resource authorization state digest"),
                }
            })
            .collect::<Vec<_>>(),
    )
    .expect("derive resource authorization view set");
    let fee_shard_set_digest = compute_fee_shard_set_digest(
        &(0..2)
            .map(|index| {
                let descriptor: FeeShardDescriptorCandidateV0 =
                    read_anchor_account(&fixture.direct.svm, &fixture.fee_descriptors[index]);
                let liability: FeeLiabilityLedgerCandidateV0 =
                    read_anchor_account(&fixture.direct.svm, &fixture.fee_liabilities[index]);
                FeeShardDigestRowCandidateV0 {
                    shard_index: u8::try_from(index).expect("bounded fee shard"),
                    asset_index: u8::try_from(index).expect("bounded fee asset"),
                    vault_settlement_capability_index: 4 + u8::try_from(index)
                        .expect("bounded fee capability"),
                    flags: 0,
                    descriptor_key: fixture.fee_descriptors[index].to_bytes(),
                    descriptor_digest: descriptor.descriptor_digest,
                    liability_key: fixture.fee_liabilities[index].to_bytes(),
                    vault_key: fixture.fee_vaults[index].to_bytes(),
                    asset_binding_digest: asset_binding_digests[index],
                    fee_policy_digest: descriptor.fee_policy_digest,
                    recipient_policy_digest: descriptor.recipient_policy_digest,
                    fee_policy_revision: descriptor.fee_policy_revision,
                    liability_before: liability.liability,
                }
            })
            .collect::<Vec<_>>(),
    )
    .expect("derive resource fee-shard set");
    let header = fixture.direct.engine_request.header;
    let protected_execution_root = compute_protected_execution_root(ProtectedExecutionRootInputs {
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
    .expect("derive cross-net protected execution root");

    let payload = encode_explicit_plan(
        RECEIPT_ACCEPT,
        0,
        NONE_INDEX,
        NONE_INDEX,
        &[PlannedMove {
            source_capability_index: 6,
            destination_capability_index: 7,
            amount: ATTEMPTED_DEBIT,
        }],
    )
    .expect("encode cross-net domain receipt");
    let payload_len = u16::try_from(payload.len()).expect("bounded cross-net payload");
    fixture.direct.envelope.header.settlement_capability_count = 8;
    fixture.direct.envelope.header.expected_engine_sequence = 0;
    fixture.direct.envelope.header.payload_len = payload_len;
    fixture.direct.envelope.header.protected_execution_root = protected_execution_root;
    fixture.direct.envelope.header.payload_digest =
        compute_payload_digest(&payload).expect("derive cross-net payload digest");
    fixture.direct.envelope.settlement_capabilities = declarations.clone();
    fixture.direct.envelope.payload = payload.clone();

    fixture
        .direct
        .engine_request
        .contexts
        .push(domain_engine_context(
            6,
            declarations[6],
            endpoints[6],
            DOMAIN_ZERO_ACCOUNTED,
        ));
    fixture
        .direct
        .engine_request
        .contexts
        .push(domain_engine_context(7, declarations[7], endpoints[7], 0));
    fixture
        .direct
        .engine_request
        .header
        .settlement_capability_count = 8;
    fixture.direct.engine_request.header.context_row_count = 6;
    fixture.direct.engine_request.header.payload_len = payload_len;
    fixture
        .direct
        .engine_request
        .header
        .protected_execution_root = protected_execution_root;
    fixture.direct.engine_request.payload = payload;
    fixture
        .direct
        .engine_request
        .validate()
        .expect("cross-net Engine request remains canonical");
    let callback_authority = derive_callback_authority_for_engine(
        &fixture.direct.engine_request,
        &effect_engine_probe::ID,
    )
    .expect("derive cross-net callback authority")
    .0;
    fixture.direct.callback_authority = callback_authority;
    fixture.direct.instruction = build_resource_instruction(
        fixture,
        callback_authority,
        vec![domain_zero_source, domain_one_destination],
    );

    assert_eq!(token_balance(&fixture.direct.svm, &domain_zero_source), 20);
    assert_eq!(
        token_balance(&fixture.direct.svm, &domain_one_destination),
        0
    );
    [domain_zero_source, domain_one_destination]
}

fn domain_accounted_declaration(
    domain_index: u8,
    directional_right: u16,
    maximum_debit: u64,
) -> SettlementCapabilityRowCandidateV0 {
    SettlementCapabilityRowCandidateV0 {
        asset_index: 0,
        domain_index_or_none: domain_index,
        authorization_slot_or_none: NONE_INDEX,
        intent_local_term_index_or_none: NONE_INDEX,
        authority_class: AUTHORITY_DOMAIN_ACCOUNTED,
        fee_shard_index_or_none: NONE_INDEX,
        fee_class: FEE_CLASS_NONE,
        flags: 0,
        rights_bits: directional_right | RIGHT_DOMAIN_ACCOUNTED,
        domain_accounting_slot_or_none: 0,
        spend_authority_control_offset_or_none: NONE_INDEX,
        reserved_0: 0,
        maximum_engine_debit: maximum_debit,
        maximum_total_debit: maximum_debit,
        minimum_credit: 0,
        maximum_protocol_fee: 0,
    }
}

fn domain_engine_context(
    position: u8,
    declaration: SettlementCapabilityRowCandidateV0,
    endpoint: ClassicSplEndpointSnapshot,
    accounted_before: u64,
) -> EngineContextRowCandidateV0 {
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
        accounted_before_or_zero: accounted_before,
        remaining_maximum_engine_debit: declaration.maximum_engine_debit,
        remaining_maximum_total_debit: declaration.maximum_total_debit,
        remaining_minimum_credit: declaration.minimum_credit,
        remaining_maximum_protocol_fee: declaration.maximum_protocol_fee,
    }
}

fn build_resource_instruction(
    fixture: &ResourceFixture,
    callback_authority: Pubkey,
    extra_settlement: Vec<Pubkey>,
) -> solana_message::Instruction {
    let mut settlement = vec![
        AccountMeta::new(fixture.sources[0], false),
        AccountMeta::new(fixture.recipients[1], false),
        AccountMeta::new(fixture.sources[1], false),
        AccountMeta::new(fixture.recipients[0], false),
        AccountMeta::new(fixture.fee_vaults[0], false),
        AccountMeta::new(fixture.fee_vaults[1], false),
    ];
    settlement.extend(
        extra_settlement
            .into_iter()
            .map(|key| AccountMeta::new(key, false)),
    );
    build_core_execute_instruction(
        &fixture.direct.envelope,
        &CoreExecuteAccountClosure {
            configuration: fixture.direct.configuration,
            market: fixture.direct.market,
            fee_policy: fixture.direct.fee_policy,
            engine_program: effect_engine_probe::ID,
            callback_authority,
            loader_policy: vec![fixture.direct.loader_policy_account],
            domain_controls: fixture
                .domain_descriptors
                .iter()
                .zip(fixture.domain_admissions)
                .zip(fixture.domain_accounting)
                .flat_map(|((descriptor, admission), accounting)| {
                    [
                        AccountMeta::new_readonly(*descriptor, false),
                        AccountMeta::new_readonly(admission, false),
                        AccountMeta::new(accounting, false),
                    ]
                })
                .collect(),
            authorization_controls: vec![
                AccountMeta::new(fixture.authorizations[0], false),
                AccountMeta::new(fixture.authorizations[1], false),
                AccountMeta::new_readonly(fixture.spend_authorities[0], false),
                AccountMeta::new_readonly(fixture.spend_authorities[1], false),
            ],
            protected_profile: vec![litesvm_token::TOKEN_ID, fixture.mints[0], fixture.mints[1]],
            fee_controls: vec![
                AccountMeta::new_readonly(fixture.fee_descriptors[0], false),
                AccountMeta::new(fixture.fee_liabilities[0], false),
                AccountMeta::new_readonly(fixture.fee_descriptors[1], false),
                AccountMeta::new(fixture.fee_liabilities[1], false),
            ],
            settlement,
            opaque: vec![
                AccountMeta::new(fixture.engine_states[0], false),
                AccountMeta::new(fixture.engine_states[1], false),
                AccountMeta::new_readonly(callback_capability_probe::ID, false),
                AccountMeta::new(fixture.helper_state, false),
            ],
        },
    )
    .expect("build resource-derived Core instruction")
}

fn overwrite_anchor_account<T: AccountSerialize>(
    svm: &mut litesvm::LiteSVM,
    address: Pubkey,
    state: &T,
    exact_space: usize,
) {
    let mut account = svm
        .get_account(&address)
        .expect("Anchor account exists before exact overwrite");
    assert_eq!(account.owner, programmable_generic_effect_core::ID);
    let mut data = Vec::with_capacity(exact_space);
    state
        .try_serialize(&mut data)
        .expect("serialize overwritten Anchor account");
    assert!(data.len() <= exact_space);
    data.resize(exact_space, 0);
    account.data = data;
    svm.set_account(address, account)
        .expect("overwrite exact Anchor account");
}

fn endpoint_snapshot(svm: &litesvm::LiteSVM, key: Pubkey) -> ClassicSplEndpointSnapshot {
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

fn token_effective_privilege(key: Pubkey) -> EffectivePrivilege {
    EffectivePrivilege {
        key,
        owner: litesvm_token::TOKEN_ID,
        executable: false,
        signer: false,
        writable: true,
    }
}

fn coption<T>(value: COption<T>) -> Option<T> {
    match value {
        COption::Some(value) => Some(value),
        COption::None => None,
    }
}

fn resource_rollback_addresses(fixture: &ResourceFixture) -> Vec<Pubkey> {
    let mut addresses = fixture.direct.rollback_state_addresses().to_vec();
    addresses.extend(fixture.sources);
    addresses.extend(fixture.recipients);
    addresses.extend(fixture.fee_vaults);
    addresses.extend(fixture.fee_liabilities);
    addresses.extend(fixture.domain_descriptors);
    addresses.extend(fixture.domain_admissions);
    addresses.extend(fixture.domain_accounting);
    addresses.extend(fixture.authorizations);
    addresses.extend(fixture.engine_states);
    addresses.push(fixture.helper_state);
    addresses.sort_unstable();
    addresses.dedup();
    addresses
}

fn core_instruction_error(error: CoreError) -> InstructionError {
    InstructionError::Custom(anchor_lang::error::ERROR_CODE_OFFSET + error as u32)
}

fn program_invoked(logs: &[String], program_id: Pubkey) -> bool {
    let needle = format!("Program {program_id} invoke");
    logs.iter().any(|line| line.starts_with(&needle))
}
