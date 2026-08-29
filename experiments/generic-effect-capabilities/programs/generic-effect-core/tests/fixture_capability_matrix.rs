mod common;

use std::collections::{BTreeMap, BTreeSet, HashSet};

use anchor_lang::{prelude::Pubkey, AccountSerialize, Space};
use common::{
    compile_v0_transaction_with_signers, fixture_keypair, install_anchor_account,
    install_lookup_table, install_raw_account, lookup_candidates, read_anchor_account,
    request_heap_frame_instruction, set_compute_unit_limit_instruction, snapshot_accounts,
    DirectFixture, DomainFixture, SbfArtifacts, V0MessageResources, CONTROLLED_COMPUTE_UNIT_LIMIT,
    CONTROLLED_HEAP_FRAME_BYTES, DIRECT_DEFAULT_AMOUNT,
};
use effect_engine_probe::{
    plan::{encode_explicit_plan, RECEIPT_ACCEPT},
    state::{EngineStateCandidateV0, ENGINE_STATE_LEN},
};
use generic_effect_private_wire::{
    compute_opaque_capability_root, compute_payload_digest, derive_callback_authority_for_engine,
    OpaqueCapabilityDescriptorCandidateV0,
};
use litesvm_cpi_tree::CpiTreeExt;
use programmable_generic_effect_core::error::CoreError;
use solana_message::{AccountMeta, VersionedMessage};
use solana_signer::Signer;
use solana_transaction::{versioned::VersionedTransaction, InstructionError, TransactionError};

const CORE_INSTRUCTION_INDEX: u8 = 2;
const OPAQUE_DECOY_TAG: u8 = 240;
const OPAQUE_SECOND_TAG: u8 = 241;
const OPAQUE_ADDITION_TAG: u8 = 242;
const HELPER_STATE_TAG: u8 = 243;
const OPAQUE_SIGNER_TAG: u8 = 244;
const EXPECTED_ALIAS_ROLES: usize = 18;
const EXPECTED_ALIAS_CELLS: usize = 71;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProtectedRole {
    Configuration,
    Market,
    FeePolicy,
    EngineProgram,
    CallbackAuthority,
    InstructionsSysvar,
    LoaderPolicy,
    DomainDescriptor,
    DomainAdmission,
    DomainAccounting,
    AuthorizationActor,
    ClassicTokenProgram,
    AssetMint,
    FeeShardDescriptor,
    FeeLiability,
    SettlementSource,
    SettlementDestination,
    SettlementFeeVault,
}

#[derive(Clone, Copy, Debug)]
struct AliasCase {
    family: &'static str,
    role: &'static str,
    protected_role: ProtectedRole,
    alt_supported: bool,
}

const ALIAS_CASES: [AliasCase; EXPECTED_ALIAS_ROLES] = [
    AliasCase {
        family: "fixed",
        role: "configuration",
        protected_role: ProtectedRole::Configuration,
        alt_supported: true,
    },
    AliasCase {
        family: "fixed",
        role: "market",
        protected_role: ProtectedRole::Market,
        alt_supported: true,
    },
    AliasCase {
        family: "fixed",
        role: "fee_policy",
        protected_role: ProtectedRole::FeePolicy,
        alt_supported: true,
    },
    AliasCase {
        family: "fixed",
        role: "engine_program",
        protected_role: ProtectedRole::EngineProgram,
        alt_supported: true,
    },
    AliasCase {
        family: "fixed",
        role: "callback_authority",
        protected_role: ProtectedRole::CallbackAuthority,
        alt_supported: true,
    },
    AliasCase {
        family: "fixed",
        role: "instructions_sysvar",
        protected_role: ProtectedRole::InstructionsSysvar,
        alt_supported: true,
    },
    AliasCase {
        family: "loader",
        role: "loader_policy",
        protected_role: ProtectedRole::LoaderPolicy,
        alt_supported: true,
    },
    AliasCase {
        family: "domain",
        role: "descriptor",
        protected_role: ProtectedRole::DomainDescriptor,
        alt_supported: true,
    },
    AliasCase {
        family: "domain",
        role: "admission",
        protected_role: ProtectedRole::DomainAdmission,
        alt_supported: true,
    },
    AliasCase {
        family: "domain",
        role: "accounting",
        protected_role: ProtectedRole::DomainAccounting,
        alt_supported: true,
    },
    AliasCase {
        family: "authorization",
        role: "direct_actor",
        protected_role: ProtectedRole::AuthorizationActor,
        // Solana signers are static message keys; an ALT can never carry this
        // globally signer-unioned duplicate identity.
        alt_supported: false,
    },
    AliasCase {
        family: "protected_profile",
        role: "classic_token_program",
        protected_role: ProtectedRole::ClassicTokenProgram,
        alt_supported: true,
    },
    AliasCase {
        family: "protected_profile",
        role: "asset_mint",
        protected_role: ProtectedRole::AssetMint,
        alt_supported: true,
    },
    AliasCase {
        family: "fee_control",
        role: "fee_shard_descriptor",
        protected_role: ProtectedRole::FeeShardDescriptor,
        alt_supported: true,
    },
    AliasCase {
        family: "fee_control",
        role: "fee_liability",
        protected_role: ProtectedRole::FeeLiability,
        alt_supported: true,
    },
    AliasCase {
        family: "settlement",
        role: "source",
        protected_role: ProtectedRole::SettlementSource,
        alt_supported: true,
    },
    AliasCase {
        family: "settlement",
        role: "destination",
        protected_role: ProtectedRole::SettlementDestination,
        alt_supported: true,
    },
    AliasCase {
        family: "settlement",
        role: "fee_vault",
        protected_role: ProtectedRole::SettlementFeeVault,
        alt_supported: true,
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OpaquePrivilegeVariant {
    Readonly,
    Writable,
    ExplicitSigner,
}

impl OpaquePrivilegeVariant {
    fn label(self) -> &'static str {
        match self {
            Self::Readonly => "readonly",
            Self::Writable => "writable",
            Self::ExplicitSigner => "explicit_signer",
        }
    }

    fn meta(self, key: Pubkey) -> AccountMeta {
        match self {
            Self::Readonly => AccountMeta::new_readonly(key, false),
            Self::Writable => AccountMeta::new(key, false),
            Self::ExplicitSigner => AccountMeta::new_readonly(key, true),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Placement {
    Static,
    Alt,
}

impl Placement {
    fn label(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Alt => "alt",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ObservedLocation {
    Static,
    Alt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ObservedPrivilege {
    location: ObservedLocation,
    signer: bool,
    compiled_writable: bool,
    writable: bool,
    message_index: u8,
}

#[derive(Clone, Copy)]
struct CommittedPrivilege {
    signer: bool,
    writable: bool,
}

struct BuiltAliasFixture {
    direct: DirectFixture,
    alias: Pubkey,
    protected_signer: bool,
    protected_writable: bool,
    rollback: Vec<Pubkey>,
}

#[test]
fn protected_roles_cannot_alias_opaque_across_static_alt_and_privilege_union() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut seen_roles = BTreeSet::new();
    let mut cells_by_family = BTreeMap::<&str, usize>::new();
    let mut executed_cells = 0_usize;
    let mut signer_union_cells = 0_usize;
    let mut writable_union_cells = 0_usize;
    let mut static_cells = 0_usize;
    let mut alt_cells = 0_usize;

    for case in ALIAS_CASES {
        assert!(
            seen_roles.insert((case.family, case.role)),
            "duplicate alias-matrix role: {}.{}",
            case.family,
            case.role
        );
        let variants: &[OpaquePrivilegeVariant] =
            if case.protected_role == ProtectedRole::AuthorizationActor {
                &[
                    OpaquePrivilegeVariant::Readonly,
                    OpaquePrivilegeVariant::Writable,
                    OpaquePrivilegeVariant::ExplicitSigner,
                ]
            } else {
                &[
                    OpaquePrivilegeVariant::Readonly,
                    OpaquePrivilegeVariant::Writable,
                ]
            };
        let placements: &[Placement] = if case.alt_supported {
            &[Placement::Static, Placement::Alt]
        } else {
            &[Placement::Static]
        };

        for variant in variants {
            for placement in placements {
                let label = format!(
                    "{}.{}:{}:{}",
                    case.family,
                    case.role,
                    variant.label(),
                    placement.label()
                );
                let mut built = build_alias_fixture(&artifacts, case, *variant);
                let expected_signer =
                    built.protected_signer || *variant == OpaquePrivilegeVariant::ExplicitSigner;
                let expected_compiled_writable =
                    built.protected_writable || *variant == OpaquePrivilegeVariant::Writable;
                // Reserved sysvars are runtime-demoted even when the compiled
                // header carries a writable request. Do not claim a positional
                // writable privilege Solana cannot expose to AccountInfo.
                let expected_writable = case.protected_role != ProtectedRole::InstructionsSysvar
                    && (built.protected_writable || *variant == OpaquePrivilegeVariant::Writable);
                let before = snapshot_accounts(&built.direct.svm, &built.rollback);
                let (transaction, resources, observed) =
                    compile_with_placement(&mut built.direct, built.alias, *placement, &[]);

                assert_eq!(
                    observed.signer, expected_signer,
                    "{label}: signer privilege was not globally unioned as expected"
                );
                assert_eq!(
                    observed.compiled_writable, expected_compiled_writable,
                    "{label}: compiled writable union changed"
                );
                assert_eq!(
                    observed.writable, expected_writable,
                    "{label}: writable privilege was not globally unioned as expected"
                );
                assert_eq!(
                    observed.location,
                    match placement {
                        Placement::Static => ObservedLocation::Static,
                        Placement::Alt => ObservedLocation::Alt,
                    },
                    "{label}: message compiler placed the alias in the wrong key plane"
                );
                assert_duplicate_positions_share_message_index(
                    &transaction,
                    &built.direct.instruction,
                    built.alias,
                    observed.message_index,
                    &label,
                );
                assert_eq!(
                    resources
                        .resolved_unique_keys
                        .iter()
                        .filter(|key| **key == built.alias)
                        .count(),
                    1,
                    "{label}: one public key must resolve to one message key"
                );

                let failure = built
                    .direct
                    .svm
                    .send_transaction(transaction)
                    .expect_err("protected identity entered the opaque plane");
                assert_eq!(
                    failure.err,
                    TransactionError::InstructionError(
                        CORE_INSTRUCTION_INDEX,
                        core_instruction_error(expected_alias_barrier(
                            case.protected_role,
                            built.protected_signer,
                            built.protected_writable,
                            observed,
                        )),
                    ),
                    "{label}: alias stopped at an unexpected validation barrier\n{}",
                    failure.meta.pretty_logs()
                );
                assert!(program_invoked(
                    &failure.meta.logs,
                    programmable_generic_effect_core::ID
                ));
                assert!(!program_invoked(
                    &failure.meta.logs,
                    effect_engine_probe::ID
                ));
                assert!(!program_invoked(
                    &failure.meta.logs,
                    callback_capability_probe::ID
                ));
                assert_eq!(
                    snapshot_accounts(&built.direct.svm, &built.rollback),
                    before,
                    "{label}: rejected alias changed landed state"
                );

                executed_cells += 1;
                *cells_by_family.entry(case.family).or_default() += 1;
                signer_union_cells += usize::from(observed.signer);
                writable_union_cells += usize::from(
                    observed.writable
                        && (!built.protected_writable
                            || *variant == OpaquePrivilegeVariant::Readonly),
                );
                match observed.location {
                    ObservedLocation::Static => static_cells += 1,
                    ObservedLocation::Alt => alt_cells += 1,
                }
            }
        }
    }

    assert_eq!(seen_roles.len(), EXPECTED_ALIAS_ROLES);
    assert_eq!(executed_cells, EXPECTED_ALIAS_CELLS);
    assert_eq!(
        cells_by_family,
        BTreeMap::from([
            ("authorization", 3),
            ("domain", 12),
            ("fee_control", 8),
            ("fixed", 24),
            ("loader", 4),
            ("protected_profile", 8),
            ("settlement", 12),
        ])
    );
    assert_eq!(signer_union_cells, 3);
    assert_eq!(writable_union_cells, 33);
    assert_eq!(static_cells, 37);
    assert_eq!(alt_cells, 34);
    eprintln!(
        "capability alias matrix: {executed_cells} cells / {} roles / {} families; {static_cells} static / {alt_cells} ALT; {signer_union_cells} signer-union / {writable_union_cells} writable-union cells",
        seen_roles.len(),
        cells_by_family.len(),
    );
}

#[test]
fn duplicate_opaque_positions_succeed_static_and_alt_with_ordered_multiplicity() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");

    for placement in [Placement::Static, Placement::Alt] {
        let mut fixture = DirectFixture::state_only(&artifacts);
        let duplicate = install_opaque_state(&mut fixture, OPAQUE_DECOY_TAG);
        let helper_state = install_helper_state(&mut fixture);
        let payload = helper_payload(2, 3);
        let opaque = vec![
            AccountMeta::new_readonly(duplicate, false),
            AccountMeta::new_readonly(duplicate, false),
            AccountMeta::new_readonly(callback_capability_probe::ID, false),
            AccountMeta::new(helper_state, false),
        ];
        rebuild_opaque_preserving_closure(
            &mut fixture,
            payload,
            opaque,
            &[
                CommittedPrivilege {
                    signer: false,
                    writable: false,
                },
                CommittedPrivilege {
                    signer: false,
                    writable: false,
                },
                CommittedPrivilege {
                    signer: false,
                    writable: false,
                },
                CommittedPrivilege {
                    signer: false,
                    writable: true,
                },
            ],
        );
        bind_helper_to_callback(&mut fixture, helper_state);
        let mut unchanged = fixture.rollback_state_addresses().to_vec();
        unchanged.push(duplicate);
        let unchanged = unique_addresses(unchanged);
        let before = snapshot_accounts(&fixture.svm, &unchanged);
        let (transaction, _, observed) =
            compile_with_placement(&mut fixture, duplicate, placement, &[]);
        assert!(!observed.signer && !observed.compiled_writable && !observed.writable);
        assert_duplicate_opaque_indices(&transaction, 4, 0, 1);

        let metadata = fixture
            .svm
            .send_transaction(transaction)
            .unwrap_or_else(|failure| {
                panic!(
                    "duplicate opaque positions failed for {placement:?}: {:?}\n{}\n{}",
                    failure.err,
                    failure.meta.pretty_logs(),
                    failure.meta.pretty_cpi_tree(),
                )
            });
        assert!(program_invoked(
            &metadata.logs,
            programmable_generic_effect_core::ID
        ));
        assert!(program_invoked(&metadata.logs, effect_engine_probe::ID));
        assert!(program_invoked(
            &metadata.logs,
            callback_capability_probe::ID
        ));
        assert_eq!(snapshot_accounts(&fixture.svm, &unchanged), before);
        let helper: callback_capability_probe::HelperState =
            read_anchor_account(&fixture.svm, &helper_state);
        assert_eq!(helper.calls, 1);
        assert_eq!(helper.value, 1);
        assert_eq!(helper.descendant_receipt_sets, 0);
    }
}

#[test]
fn opaque_order_count_and_multiplicity_tampering_fail_before_engine_and_roll_back() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");

    for tamper in ["reorder", "omission", "addition", "multiplicity"] {
        let mut fixture = DirectFixture::state_only(&artifacts);
        let first = install_opaque_state(&mut fixture, OPAQUE_DECOY_TAG);
        let second = install_opaque_state(&mut fixture, OPAQUE_SECOND_TAG);
        let addition = install_opaque_state(&mut fixture, OPAQUE_ADDITION_TAG);
        let helper_state = install_helper_state(&mut fixture);
        let committed_keys = if tamper == "multiplicity" {
            [first, first]
        } else {
            [first, second]
        };
        let opaque = vec![
            AccountMeta::new_readonly(committed_keys[0], false),
            AccountMeta::new_readonly(committed_keys[1], false),
            AccountMeta::new_readonly(callback_capability_probe::ID, false),
            AccountMeta::new(helper_state, false),
        ];
        rebuild_opaque_preserving_closure(
            &mut fixture,
            helper_payload(2, 3),
            opaque,
            &[
                CommittedPrivilege {
                    signer: false,
                    writable: false,
                },
                CommittedPrivilege {
                    signer: false,
                    writable: false,
                },
                CommittedPrivilege {
                    signer: false,
                    writable: false,
                },
                CommittedPrivilege {
                    signer: false,
                    writable: true,
                },
            ],
        );
        bind_helper_to_callback(&mut fixture, helper_state);

        let opaque_start = fixture
            .instruction
            .accounts
            .len()
            .checked_sub(4)
            .expect("four-account opaque tail");
        match tamper {
            "reorder" => fixture
                .instruction
                .accounts
                .swap(opaque_start, opaque_start + 1),
            "omission" => {
                fixture.instruction.accounts.remove(opaque_start + 1);
            }
            "addition" => fixture
                .instruction
                .accounts
                .push(AccountMeta::new_readonly(addition, false)),
            "multiplicity" => {
                assert_eq!(
                    fixture.instruction.accounts[opaque_start].pubkey,
                    fixture.instruction.accounts[opaque_start + 1].pubkey
                );
                fixture.instruction.accounts[opaque_start + 1].pubkey = second;
            }
            _ => unreachable!(),
        }

        let mut rollback = fixture.rollback_state_addresses().to_vec();
        rollback.extend([first, second, addition, helper_state]);
        let rollback = unique_addresses(rollback);
        let before = snapshot_accounts(&fixture.svm, &rollback);
        let (transaction, _) = fixture.compile_v0();
        let failure = fixture
            .svm
            .send_transaction(transaction)
            .expect_err("opaque tail tampering reached the Engine");
        let expected_error = match tamper {
            "reorder" | "multiplicity" => CoreError::InvalidWireEncoding,
            "omission" | "addition" => CoreError::AccountSegmentLengthMismatch,
            _ => unreachable!(),
        };
        assert_eq!(
            failure.err,
            TransactionError::InstructionError(
                CORE_INSTRUCTION_INDEX,
                core_instruction_error(expected_error),
            ),
            "{tamper}: opaque-tail mutation stopped at the wrong gate"
        );
        assert!(program_invoked(
            &failure.meta.logs,
            programmable_generic_effect_core::ID
        ));
        assert!(!program_invoked(
            &failure.meta.logs,
            effect_engine_probe::ID
        ));
        assert!(!program_invoked(
            &failure.meta.logs,
            callback_capability_probe::ID
        ));
        assert_eq!(
            snapshot_accounts(&fixture.svm, &rollback),
            before,
            "{tamper}: rejected opaque-tail mutation changed landed state"
        );
    }
}

#[test]
fn duplicate_opaque_writable_union_must_be_bound_at_every_position() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");

    for bind_global_union in [false, true] {
        let mut fixture = DirectFixture::state_only(&artifacts);
        let duplicate = install_opaque_state(&mut fixture, OPAQUE_DECOY_TAG);
        let helper_state = install_helper_state(&mut fixture);
        let opaque = vec![
            AccountMeta::new_readonly(duplicate, false),
            AccountMeta::new(duplicate, false),
            AccountMeta::new_readonly(callback_capability_probe::ID, false),
            AccountMeta::new(helper_state, false),
        ];
        rebuild_opaque_preserving_closure(
            &mut fixture,
            helper_payload(2, 3),
            opaque,
            &[
                CommittedPrivilege {
                    signer: false,
                    writable: bind_global_union,
                },
                CommittedPrivilege {
                    signer: false,
                    writable: true,
                },
                CommittedPrivilege {
                    signer: false,
                    writable: false,
                },
                CommittedPrivilege {
                    signer: false,
                    writable: true,
                },
            ],
        );
        bind_helper_to_callback(&mut fixture, helper_state);
        let mut rollback = fixture.rollback_state_addresses().to_vec();
        rollback.extend([duplicate, helper_state]);
        let rollback = unique_addresses(rollback);
        let before = snapshot_accounts(&fixture.svm, &rollback);
        let (transaction, _, observed) =
            compile_with_placement(&mut fixture, duplicate, Placement::Alt, &[]);
        assert!(!observed.signer && observed.compiled_writable && observed.writable);
        assert_duplicate_opaque_indices(&transaction, 4, 0, 1);

        if bind_global_union {
            let metadata = fixture
                .svm
                .send_transaction(transaction)
                .unwrap_or_else(|failure| {
                    panic!(
                        "union-bound duplicate opaque execution failed: {:?}\n{}\n{}",
                        failure.err,
                        failure.meta.pretty_logs(),
                        failure.meta.pretty_cpi_tree(),
                    )
                });
            assert!(program_invoked(&metadata.logs, effect_engine_probe::ID));
            assert!(program_invoked(
                &metadata.logs,
                callback_capability_probe::ID
            ));
            let helper: callback_capability_probe::HelperState =
                read_anchor_account(&fixture.svm, &helper_state);
            assert_eq!(helper.calls, 1);
            assert_eq!(helper.value, 1);
        } else {
            let failure = fixture
                .svm
                .send_transaction(transaction)
                .expect_err("per-position requested flags hid the writable union");
            assert_eq!(
                failure.err,
                TransactionError::InstructionError(
                    CORE_INSTRUCTION_INDEX,
                    core_instruction_error(CoreError::InvalidWireEncoding),
                )
            );
            assert!(!program_invoked(
                &failure.meta.logs,
                effect_engine_probe::ID
            ));
            assert!(!program_invoked(
                &failure.meta.logs,
                callback_capability_probe::ID
            ));
            assert_eq!(snapshot_accounts(&fixture.svm, &rollback), before);
        }
    }
}

#[test]
fn duplicate_opaque_signer_union_is_static_and_rejected_before_engine() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = DirectFixture::state_only(&artifacts);
    let opaque_signer = fixture_keypair(OPAQUE_SIGNER_TAG);
    install_raw_account(
        &mut fixture.svm,
        opaque_signer.pubkey(),
        effect_engine_probe::ID,
        EngineStateCandidateV0::fresh().encode().to_vec(),
        false,
    );
    let helper_state = install_helper_state(&mut fixture);
    let opaque = vec![
        AccountMeta::new_readonly(opaque_signer.pubkey(), true),
        AccountMeta::new_readonly(opaque_signer.pubkey(), false),
        AccountMeta::new_readonly(callback_capability_probe::ID, false),
        AccountMeta::new(helper_state, false),
    ];
    rebuild_opaque_preserving_closure(
        &mut fixture,
        helper_payload(2, 3),
        opaque,
        &[
            CommittedPrivilege {
                signer: true,
                writable: false,
            },
            CommittedPrivilege {
                signer: true,
                writable: false,
            },
            CommittedPrivilege {
                signer: false,
                writable: false,
            },
            CommittedPrivilege {
                signer: false,
                writable: true,
            },
        ],
    );
    bind_helper_to_callback(&mut fixture, helper_state);
    let mut rollback = fixture.rollback_state_addresses().to_vec();
    rollback.extend([opaque_signer.pubkey(), helper_state]);
    let rollback = unique_addresses(rollback);
    let before = snapshot_accounts(&fixture.svm, &rollback);
    let (transaction, _, observed) = compile_with_placement(
        &mut fixture,
        opaque_signer.pubkey(),
        Placement::Static,
        &[&opaque_signer],
    );
    assert_eq!(observed.location, ObservedLocation::Static);
    assert!(observed.signer && !observed.compiled_writable && !observed.writable);
    assert_duplicate_opaque_indices(&transaction, 4, 0, 1);

    let failure = fixture
        .svm
        .send_transaction(transaction)
        .expect_err("globally signer-unioned opaque duplicate reached the Engine");
    assert_eq!(
        failure.err,
        TransactionError::InstructionError(
            CORE_INSTRUCTION_INDEX,
            core_instruction_error(CoreError::DirectAuthorizationNotTransactionRoot),
        )
    );
    assert!(program_invoked(
        &failure.meta.logs,
        programmable_generic_effect_core::ID
    ));
    assert!(!program_invoked(
        &failure.meta.logs,
        effect_engine_probe::ID
    ));
    assert!(!program_invoked(
        &failure.meta.logs,
        callback_capability_probe::ID
    ));
    assert_eq!(snapshot_accounts(&fixture.svm, &rollback), before);
}

fn build_alias_fixture(
    artifacts: &SbfArtifacts,
    case: AliasCase,
    variant: OpaquePrivilegeVariant,
) -> BuiltAliasFixture {
    let (mut direct, mut alias, mut rollback) = match case.protected_role {
        ProtectedRole::DomainDescriptor
        | ProtectedRole::DomainAdmission
        | ProtectedRole::DomainAccounting => {
            let domain = DomainFixture::closed_credit(artifacts, DIRECT_DEFAULT_AMOUNT);
            let alias = match case.protected_role {
                ProtectedRole::DomainDescriptor => domain.descriptor,
                ProtectedRole::DomainAdmission => {
                    domain.admission.expect("closed domain has admission state")
                }
                ProtectedRole::DomainAccounting => domain.accounting,
                _ => unreachable!(),
            };
            let rollback = domain.protected_state_addresses();
            (domain.direct, alias, rollback)
        }
        role => {
            let direct = DirectFixture::state_only(artifacts);
            let alias = protected_key(&direct, role);
            let rollback = direct.rollback_state_addresses().to_vec();
            (direct, alias, rollback)
        }
    };
    let decoy = install_opaque_state(&mut direct, OPAQUE_DECOY_TAG);
    let helper_state = install_helper_state(&mut direct);
    let decoy_meta = variant.meta(decoy);
    let decoy_privilege = CommittedPrivilege {
        signer: decoy_meta.is_signer,
        writable: decoy_meta.is_writable,
    };
    rebuild_opaque_preserving_closure(
        &mut direct,
        helper_payload(1, 2),
        vec![
            decoy_meta,
            AccountMeta::new_readonly(callback_capability_probe::ID, false),
            AccountMeta::new(helper_state, false),
        ],
        &[
            decoy_privilege,
            CommittedPrivilege {
                signer: false,
                writable: false,
            },
            CommittedPrivilege {
                signer: false,
                writable: true,
            },
        ],
    );
    bind_helper_to_callback(&mut direct, helper_state);
    if case.protected_role == ProtectedRole::CallbackAuthority {
        // The callback PDA binds the rebuilt opaque root. Select the final
        // protected callback only after the canonical decoy tail is complete.
        alias = direct.callback_authority;
    }
    let protected_occurrences = direct
        .instruction
        .accounts
        .iter()
        .filter(|meta| meta.pubkey == alias)
        .collect::<Vec<_>>();
    assert_eq!(
        protected_occurrences.len(),
        1,
        "{}.{} baseline must contain exactly one protected occurrence",
        case.family,
        case.role
    );
    let protected_signer = protected_occurrences[0].is_signer;
    let protected_writable = protected_occurrences[0].is_writable;
    let opaque_start = direct
        .instruction
        .accounts
        .len()
        .checked_sub(3)
        .expect("three-account opaque alias tail");
    assert_eq!(direct.instruction.accounts[opaque_start].pubkey, decoy);
    direct.instruction.accounts[opaque_start].pubkey = alias;

    rollback.extend([alias, decoy, helper_state]);
    BuiltAliasFixture {
        direct,
        alias,
        protected_signer,
        protected_writable,
        rollback: unique_addresses(rollback),
    }
}

fn protected_key(direct: &DirectFixture, role: ProtectedRole) -> Pubkey {
    match role {
        ProtectedRole::Configuration => direct.configuration,
        ProtectedRole::Market => direct.market,
        ProtectedRole::FeePolicy => direct.fee_policy,
        ProtectedRole::EngineProgram => effect_engine_probe::ID,
        ProtectedRole::CallbackAuthority => direct.callback_authority,
        ProtectedRole::InstructionsSysvar => solana_sdk_ids::sysvar::instructions::id(),
        ProtectedRole::LoaderPolicy => direct.loader_policy_account,
        ProtectedRole::AuthorizationActor => direct.actor.pubkey(),
        ProtectedRole::ClassicTokenProgram => litesvm_token::TOKEN_ID,
        ProtectedRole::AssetMint => direct.mint,
        ProtectedRole::FeeShardDescriptor => direct.fee_shard_descriptor,
        ProtectedRole::FeeLiability => direct.fee_liability,
        ProtectedRole::SettlementSource => direct.source,
        ProtectedRole::SettlementDestination => direct.destination,
        ProtectedRole::SettlementFeeVault => direct.fee_vault,
        ProtectedRole::DomainDescriptor
        | ProtectedRole::DomainAdmission
        | ProtectedRole::DomainAccounting => panic!("domain role requires DomainFixture"),
    }
}

fn expected_alias_barrier(
    _role: ProtectedRole,
    protected_signer: bool,
    protected_writable: bool,
    observed: ObservedPrivilege,
) -> CoreError {
    if protected_signer || observed.writable != protected_writable {
        CoreError::DirectAuthorizationNotTransactionRoot
    } else {
        CoreError::OpaqueProtectedAlias
    }
}

fn rebuild_opaque_preserving_closure(
    fixture: &mut DirectFixture,
    payload: Vec<u8>,
    opaque: Vec<AccountMeta>,
    committed: &[CommittedPrivilege],
) {
    assert_eq!(opaque.len(), committed.len());
    let previous_count = usize::from(fixture.envelope.header.opaque_capability_count);
    let closure_end = fixture
        .instruction
        .accounts
        .len()
        .checked_sub(previous_count)
        .expect("previous opaque count fits landed closure");
    fixture.instruction.accounts.truncate(closure_end);

    let descriptors = opaque
        .iter()
        .zip(committed)
        .enumerate()
        .map(|(position, (meta, privilege))| {
            let account = fixture
                .svm
                .get_account(&meta.pubkey)
                .unwrap_or_else(|| panic!("opaque fixture account {} is absent", meta.pubkey));
            OpaqueCapabilityDescriptorCandidateV0 {
                position: u8::try_from(position).expect("opaque position fits u8"),
                key: meta.pubkey.to_bytes(),
                owner: account.owner.to_bytes(),
                executable: account.executable,
                effective_signer: privilege.signer,
                effective_writable: privilege.writable,
            }
        })
        .collect::<Vec<_>>();
    let opaque_root =
        compute_opaque_capability_root(&descriptors).expect("canonical opaque capability root");
    let opaque_count = u8::try_from(opaque.len()).expect("opaque count fits u8");
    let payload_len = u16::try_from(payload.len()).expect("opaque payload length fits u16");
    fixture.envelope.header.opaque_capability_count = opaque_count;
    fixture.envelope.header.expected_opaque_capability_root = opaque_root;
    fixture.envelope.header.payload_len = payload_len;
    fixture.envelope.header.payload_digest =
        compute_payload_digest(&payload).expect("canonical opaque payload digest");
    fixture.envelope.payload = payload.clone();
    fixture.engine_request.header.opaque_capability_count = opaque_count;
    fixture.engine_request.header.opaque_capability_root = opaque_root;
    fixture.engine_request.header.payload_len = payload_len;
    fixture.engine_request.payload = payload;
    fixture
        .engine_request
        .validate()
        .expect("rebuilt Engine request remains canonical");
    fixture.callback_authority =
        derive_callback_authority_for_engine(&fixture.engine_request, &effect_engine_probe::ID)
            .expect("derive rebuilt callback authority")
            .0;
    fixture.instruction.accounts[4].pubkey = fixture.callback_authority;
    fixture.instruction.accounts.extend(opaque);
    fixture.instruction.data = fixture
        .envelope
        .encode()
        .expect("encode rebuilt Core envelope");
}

fn compile_with_placement(
    fixture: &mut DirectFixture,
    observed_key: Pubkey,
    placement: Placement,
    extra_signers: &[&solana_keypair::Keypair],
) -> (VersionedTransaction, V0MessageResources, ObservedPrivilege) {
    let instructions = vec![
        set_compute_unit_limit_instruction(CONTROLLED_COMPUTE_UNIT_LIMIT),
        request_heap_frame_instruction(CONTROLLED_HEAP_FRAME_BYTES),
        fixture.instruction.clone(),
    ];
    let mut candidates = lookup_candidates(&instructions, fixture.payer.pubkey());
    match placement {
        Placement::Static => candidates.retain(|key| *key != observed_key),
        Placement::Alt => assert!(
            candidates.contains(&observed_key),
            "ALT-requested key is not a lookup candidate"
        ),
    }
    let table = install_lookup_table(&mut fixture.svm, &fixture.payer, candidates);
    let mut signers = Vec::with_capacity(1 + extra_signers.len());
    signers.push(&fixture.actor);
    signers.extend_from_slice(extra_signers);
    let (transaction, resources) = compile_v0_transaction_with_signers(
        &fixture.svm,
        &fixture.payer,
        &instructions,
        &[table],
        &signers,
    )
    .expect("compile capability-matrix v0 transaction");
    let observed = observed_privilege(&transaction, &resources, observed_key);
    (transaction, resources, observed)
}

fn observed_privilege(
    transaction: &VersionedTransaction,
    resources: &V0MessageResources,
    key: Pubkey,
) -> ObservedPrivilege {
    let VersionedMessage::V0(message) = &transaction.message else {
        panic!("capability matrix must compile a v0 message");
    };
    if let Some(index) = message
        .account_keys
        .iter()
        .position(|candidate| *candidate == key)
    {
        let required = usize::from(message.header.num_required_signatures);
        let signer = index < required;
        let writable = if signer {
            index
                < required
                    .checked_sub(usize::from(message.header.num_readonly_signed_accounts))
                    .expect("valid signed-account header")
        } else {
            index
                < message
                    .account_keys
                    .len()
                    .checked_sub(usize::from(message.header.num_readonly_unsigned_accounts))
                    .expect("valid unsigned-account header")
        };
        return ObservedPrivilege {
            location: ObservedLocation::Static,
            signer,
            compiled_writable: writable,
            writable: runtime_effective_writable(key, writable),
            message_index: u8::try_from(index).expect("message index fits u8"),
        };
    }
    if let Some(index) = resources
        .loaded_writable_keys
        .iter()
        .position(|candidate| *candidate == key)
    {
        return ObservedPrivilege {
            location: ObservedLocation::Alt,
            signer: false,
            compiled_writable: true,
            writable: runtime_effective_writable(key, true),
            message_index: u8::try_from(message.account_keys.len() + index)
                .expect("message index fits u8"),
        };
    }
    if let Some(index) = resources
        .loaded_readonly_keys
        .iter()
        .position(|candidate| *candidate == key)
    {
        return ObservedPrivilege {
            location: ObservedLocation::Alt,
            signer: false,
            compiled_writable: false,
            writable: false,
            message_index: u8::try_from(
                message.account_keys.len() + resources.loaded_writable_keys.len() + index,
            )
            .expect("message index fits u8"),
        };
    }
    panic!("observed key {key} is absent from the resolved v0 message");
}

fn runtime_effective_writable(key: Pubkey, compiled_writable: bool) -> bool {
    compiled_writable && key != solana_sdk_ids::sysvar::instructions::id()
}

fn assert_duplicate_positions_share_message_index(
    transaction: &VersionedTransaction,
    source: &solana_message::Instruction,
    key: Pubkey,
    expected_index: u8,
    label: &str,
) {
    let VersionedMessage::V0(message) = &transaction.message else {
        panic!("capability matrix must compile a v0 message");
    };
    let compiled = &message.instructions[usize::from(CORE_INSTRUCTION_INDEX)];
    let positions = source
        .accounts
        .iter()
        .enumerate()
        .filter_map(|(position, meta)| (meta.pubkey == key).then_some(position))
        .collect::<Vec<_>>();
    assert_eq!(
        positions.len(),
        2,
        "{label}: alias fixture must preserve two positional occurrences"
    );
    for position in positions {
        assert_eq!(
            compiled.accounts[position], expected_index,
            "{label}: duplicate position did not resolve to the global message key"
        );
    }
}

fn assert_duplicate_opaque_indices(
    transaction: &VersionedTransaction,
    opaque_count: usize,
    first: usize,
    second: usize,
) {
    let VersionedMessage::V0(message) = &transaction.message else {
        panic!("capability matrix must compile a v0 message");
    };
    let compiled = &message.instructions[usize::from(CORE_INSTRUCTION_INDEX)];
    let start = compiled
        .accounts
        .len()
        .checked_sub(opaque_count)
        .expect("opaque count fits compiled Core account list");
    assert_eq!(
        compiled.accounts[start + first],
        compiled.accounts[start + second]
    );
}

fn helper_payload(program_position: u8, state_position: u8) -> Vec<u8> {
    encode_explicit_plan(RECEIPT_ACCEPT, 0, program_position, state_position, &[])
        .expect("encode helper-forwarding capability plan")
}

fn install_opaque_state(fixture: &mut DirectFixture, tag: u8) -> Pubkey {
    let address = fixture_keypair(tag).pubkey();
    install_raw_account(
        &mut fixture.svm,
        address,
        effect_engine_probe::ID,
        EngineStateCandidateV0::fresh().encode().to_vec(),
        false,
    );
    assert_eq!(
        fixture
            .svm
            .get_account(&address)
            .expect("opaque state")
            .data
            .len(),
        ENGINE_STATE_LEN
    );
    address
}

fn install_helper_state(fixture: &mut DirectFixture) -> Pubkey {
    let address = fixture_keypair(HELPER_STATE_TAG).pubkey();
    install_anchor_account(
        &mut fixture.svm,
        address,
        callback_capability_probe::ID,
        &callback_capability_probe::HelperState {
            allowed_callback: fixture.callback_authority,
            calls: 0,
            value: 0,
            descendant_receipt_sets: 0,
        },
        8 + callback_capability_probe::HelperState::INIT_SPACE,
    );
    address
}

fn bind_helper_to_callback(fixture: &mut DirectFixture, address: Pubkey) {
    let mut account = fixture
        .svm
        .get_account(&address)
        .unwrap_or_else(|| panic!("helper state {address} is absent"));
    let state = callback_capability_probe::HelperState {
        allowed_callback: fixture.callback_authority,
        calls: 0,
        value: 0,
        descendant_receipt_sets: 0,
    };
    let mut data = Vec::with_capacity(account.data.len());
    state
        .try_serialize(&mut data)
        .expect("serialize callback-bound helper state");
    assert!(data.len() <= account.data.len());
    data.resize(account.data.len(), 0);
    account.data = data;
    fixture
        .svm
        .set_account(address, account)
        .unwrap_or_else(|error| panic!("bind helper state {address} to callback: {error}"));
}

fn unique_addresses(addresses: Vec<Pubkey>) -> Vec<Pubkey> {
    let mut seen = HashSet::with_capacity(addresses.len());
    addresses
        .into_iter()
        .filter(|address| seen.insert(*address))
        .collect()
}

fn program_invoked(logs: &[String], program_id: Pubkey) -> bool {
    logs.iter()
        .any(|line| line.starts_with(&format!("Program {program_id} invoke")))
}

fn core_instruction_error(error: CoreError) -> InstructionError {
    InstructionError::Custom(anchor_lang::error::ERROR_CODE_OFFSET + error as u32)
}
