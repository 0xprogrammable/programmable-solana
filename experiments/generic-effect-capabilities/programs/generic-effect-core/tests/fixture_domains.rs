mod common;

use common::{
    read_anchor_account, snapshot_accounts, token_balance, DomainFixture, FixtureDomainRule,
    SbfArtifacts, DIRECT_DEFAULT_AMOUNT, DIRECT_SOURCE_BALANCE, DOMAIN_ACCOUNTED_LIQUIDITY,
    DOMAIN_DONATION,
};
use litesvm_cpi_tree::CpiTreeExt;

use programmable_generic_effect_core::state::{
    DomainAccountingCandidateV0, DomainAdmissionAccountCandidateV0, FeeLiabilityLedgerCandidateV0,
};

#[test]
fn open_domain_admission_credits_only_accounted_liquidity_across_a_raw_donation() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture =
        DomainFixture::open_credit(&artifacts, DIRECT_DEFAULT_AMOUNT, DOMAIN_DONATION);

    assert_eq!(fixture.rule, FixtureDomainRule::Open);
    assert!(fixture.admission.is_none());
    assert_eq!(
        fixture.direct.envelope.header.domain_control_account_count, 2,
        "open admission uses only descriptor and accounting controls"
    );
    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.direct.destination),
        DOMAIN_DONATION,
        "raw token donation must exist before the committed execution"
    );
    let accounting_before: DomainAccountingCandidateV0 =
        read_anchor_account(&fixture.direct.svm, &fixture.accounting);
    assert_eq!(accounting_before.assets[0].accounted_amount, 0);

    let (transaction, _) = fixture.compile_v0();
    let metadata = fixture
        .direct
        .svm
        .send_transaction(transaction)
        .unwrap_or_else(|failure| {
            panic!(
                "exact open-domain execution failed: {:?}\n{}\n{}",
                failure.err,
                failure.meta.pretty_logs(),
                failure.meta.pretty_cpi_tree(),
            )
        });

    assert!(program_invoked(&metadata.logs, effect_engine_probe::ID));
    assert!(program_invoked(&metadata.logs, litesvm_token::TOKEN_ID));
    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.direct.source),
        DIRECT_SOURCE_BALANCE - DIRECT_DEFAULT_AMOUNT - fixture.direct.protocol_fee
    );
    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.direct.destination),
        DOMAIN_DONATION + DIRECT_DEFAULT_AMOUNT
    );
    let accounting_after: DomainAccountingCandidateV0 =
        read_anchor_account(&fixture.direct.svm, &fixture.accounting);
    assert_eq!(
        accounting_after.assets[0].accounted_amount,
        u128::from(DIRECT_DEFAULT_AMOUNT),
        "Core must account the authenticated credit without absorbing the pre-existing donation"
    );
    assert_eq!(
        u128::from(token_balance(
            &fixture.direct.svm,
            &fixture.direct.destination
        )) - accounting_after.assets[0].accounted_amount,
        u128::from(DOMAIN_DONATION),
        "raw excess remains outside domain-accounted liquidity"
    );
}

#[test]
fn closed_domain_exact_participating_admission_executes_and_updates_accounting() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = DomainFixture::closed_credit(&artifacts, DIRECT_DEFAULT_AMOUNT);

    assert_eq!(fixture.rule, FixtureDomainRule::Closed);
    let admission = fixture
        .admission
        .expect("closed domain must carry one participating admission");
    let admission_state: DomainAdmissionAccountCandidateV0 =
        read_anchor_account(&fixture.direct.svm, &admission);
    assert_eq!(
        admission_state.engine_program,
        effect_engine_probe::ID.to_bytes()
    );
    assert_eq!(
        fixture.direct.envelope.header.domain_control_account_count, 3,
        "closed admission uses descriptor, admission, and accounting controls"
    );

    let (transaction, _) = fixture.compile_v0();
    let metadata = fixture
        .direct
        .svm
        .send_transaction(transaction)
        .unwrap_or_else(|failure| {
            panic!(
                "exact closed-domain execution failed: {:?}\n{}\n{}",
                failure.err,
                failure.meta.pretty_logs(),
                failure.meta.pretty_cpi_tree(),
            )
        });

    assert!(program_invoked(&metadata.logs, effect_engine_probe::ID));
    assert!(program_invoked(&metadata.logs, litesvm_token::TOKEN_ID));
    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.direct.destination),
        DIRECT_DEFAULT_AMOUNT
    );
    let accounting: DomainAccountingCandidateV0 =
        read_anchor_account(&fixture.direct.svm, &fixture.accounting);
    assert_eq!(
        accounting.assets[0].accounted_amount,
        u128::from(DIRECT_DEFAULT_AMOUNT)
    );
    let liability: FeeLiabilityLedgerCandidateV0 =
        read_anchor_account(&fixture.direct.svm, &fixture.direct.fee_liability);
    assert_eq!(liability.liability, u128::from(fixture.direct.protocol_fee));
}

#[test]
fn closed_domain_rejects_a_canonical_admission_for_a_nonparticipating_engine() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = DomainFixture::closed_credit(&artifacts, DIRECT_DEFAULT_AMOUNT);
    let nonparticipating = fixture
        .nonparticipating_admission
        .expect("closed fixture installs a nonparticipating admission record");
    let nonparticipating_state: DomainAdmissionAccountCandidateV0 =
        read_anchor_account(&fixture.direct.svm, &nonparticipating);
    assert_eq!(
        nonparticipating_state.engine_program,
        hostile_router_probe::ID.to_bytes(),
        "negative evidence uses a canonical record for a real but nonparticipating program"
    );
    assert_ne!(
        nonparticipating_state.engine_program,
        effect_engine_probe::ID.to_bytes()
    );
    fixture.replace_with_nonparticipating_admission();
    let protected = fixture.protected_state_addresses();
    let before = snapshot_accounts(&fixture.direct.svm, &protected);

    let (transaction, _) = fixture.compile_v0();
    let failure = fixture
        .direct
        .svm
        .send_transaction(transaction)
        .expect_err("nonparticipating admission unexpectedly authorized the market Engine");

    assert!(
        !program_invoked(&failure.meta.logs, effect_engine_probe::ID),
        "wrong admission crossed the untrusted Engine boundary:\n{}",
        failure.meta.pretty_logs()
    );
    assert!(!program_invoked(
        &failure.meta.logs,
        litesvm_token::TOKEN_ID
    ));
    assert_eq!(
        snapshot_accounts(&fixture.direct.svm, &protected),
        before,
        "wrong admission changed protected state"
    );
}

#[test]
fn raw_donation_cannot_expand_domain_debit_authority_beyond_accounted_liquidity() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let attempted_debit = DOMAIN_ACCOUNTED_LIQUIDITY + 1;
    let mut fixture = DomainFixture::open_debit(
        &artifacts,
        attempted_debit,
        DOMAIN_ACCOUNTED_LIQUIDITY,
        DOMAIN_DONATION,
    );
    assert_eq!(fixture.accounted_before, DOMAIN_ACCOUNTED_LIQUIDITY);
    assert_eq!(fixture.donation_before, DOMAIN_DONATION);
    assert_eq!(
        token_balance(&fixture.direct.svm, &fixture.direct.source),
        DOMAIN_ACCOUNTED_LIQUIDITY + DOMAIN_DONATION,
        "raw balance is sufficient only because it includes unaccounted tokens"
    );
    let protected = fixture.protected_state_addresses();
    let before = snapshot_accounts(&fixture.direct.svm, &protected);

    let (transaction, _) = fixture.compile_v0();
    let failure = fixture
        .direct
        .svm
        .send_transaction(transaction)
        .expect_err("raw donation incorrectly expanded domain debit authority");

    assert!(
        program_invoked(&failure.meta.logs, effect_engine_probe::ID),
        "donation boundary must reject the Engine's otherwise valid move plan:\n{}",
        failure.meta.pretty_logs()
    );
    assert!(
        !program_invoked(&failure.meta.logs, litesvm_token::TOKEN_ID),
        "unaccounted donation reached token settlement:\n{}",
        failure.meta.pretty_logs()
    );
    assert_eq!(
        snapshot_accounts(&fixture.direct.svm, &protected),
        before,
        "rejected donation-funded debit changed protected state"
    );
}

fn program_invoked(logs: &[String], program_id: anchor_lang::prelude::Pubkey) -> bool {
    let needle = format!("Program {program_id} invoke");
    logs.iter().any(|line| line.starts_with(&needle))
}
