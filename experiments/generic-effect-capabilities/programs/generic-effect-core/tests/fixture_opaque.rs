mod common;

use anchor_lang::{prelude::Pubkey, AccountSerialize, Space};
use common::{
    contains_program_path, fixture_keypair, install_anchor_account, install_raw_account,
    read_anchor_account, snapshot_accounts, DirectFixture, SbfArtifacts, DIRECT_DEFAULT_AMOUNT,
};
use effect_engine_probe::{
    plan::{
        encode_explicit_plan, PlannedMove, RECEIPT_ACCEPT, RECEIPT_DESCENDANT_SETTER,
        RECEIPT_LATE_FAILURE,
    },
    state::{EngineStateCandidateV0, ENGINE_STATE_LEN},
};
use generic_effect_private_wire::NONE_INDEX;
use litesvm_cpi_tree::CpiTreeExt;
use solana_loader_v3_interface::get_program_data_address;
use solana_message::AccountMeta;
use solana_signer::Signer;

#[test]
fn zero_one_and_many_engine_state_accounts_execute_through_one_opaque_tail_shape() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");

    for state_count in [0_usize, 1, 3] {
        let mut fixture = DirectFixture::state_only(&artifacts);
        let state_addresses = (0..state_count)
            .map(|position| {
                let address = fixture_keypair(
                    130_u8
                        .checked_add(u8::try_from(position).expect("bounded state position"))
                        .expect("fixture state tag fits u8"),
                )
                .pubkey();
                install_engine_state(&mut fixture, address);
                address
            })
            .collect::<Vec<_>>();
        let state_bitmap = state_addresses
            .iter()
            .enumerate()
            .fold(0_u8, |bitmap, (position, _)| bitmap | (1_u8 << position));
        let payload =
            encode_explicit_plan(RECEIPT_ACCEPT, state_bitmap, NONE_INDEX, NONE_INDEX, &[])
                .expect("encode zero/one/many-state plan");
        fixture.envelope.header.expected_engine_sequence = u64::from(state_count != 0);
        fixture.rebuild_payload_and_opaque(
            payload,
            state_addresses
                .iter()
                .copied()
                .map(|address| AccountMeta::new(address, false))
                .collect(),
        );

        assert_eq!(
            usize::from(fixture.engine_request.header.opaque_capability_count),
            state_count
        );
        let request_digest = fixture
            .engine_request
            .digest()
            .expect("canonical state fixture request digest");
        let protected = fixture.rollback_state_addresses();
        let protected_before = snapshot_accounts(&fixture.svm, &protected);
        let (transaction, _) = fixture.compile_v0();
        let metadata = fixture
            .svm
            .send_transaction(transaction)
            .unwrap_or_else(|failure| {
                panic!(
                    "{state_count}-state opaque execution failed: {:?}\n{}\n{}",
                    failure.err,
                    failure.meta.pretty_logs(),
                    failure.meta.pretty_cpi_tree(),
                )
            });

        assert!(contains_program_path(
            &metadata,
            &[
                programmable_generic_effect_core::ID,
                effect_engine_probe::ID,
            ]
        ));
        assert_eq!(
            snapshot_accounts(&fixture.svm, &protected),
            protected_before,
            "{state_count}-state opaque execution changed protected state"
        );
        for address in state_addresses {
            let state = read_engine_state(&fixture, address);
            assert_eq!(state.sequence, 1);
            assert_eq!(state.accumulator, 0);
            assert_eq!(state.last_request_digest, request_digest);
            assert_eq!(state.last_move_count, 0);
        }
    }
}

#[test]
fn selected_engine_forwards_the_readonly_callback_capability_to_an_opaque_helper() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = DirectFixture::state_only(&artifacts);
    let helper_state = fixture_keypair(140).pubkey();
    install_helper_state(&mut fixture, helper_state);
    let payload =
        encode_explicit_plan(RECEIPT_ACCEPT, 0, 0, 1, &[]).expect("encode helper-forwarding plan");
    fixture.rebuild_payload_and_opaque(
        payload,
        vec![
            AccountMeta::new_readonly(callback_capability_probe::ID, false),
            AccountMeta::new(helper_state, false),
        ],
    );
    bind_helper_to_callback(&mut fixture, helper_state);

    let protected = fixture.rollback_state_addresses();
    let protected_before = snapshot_accounts(&fixture.svm, &protected);
    let (transaction, _) = fixture.compile_v0();
    let metadata = fixture
        .svm
        .send_transaction(transaction)
        .unwrap_or_else(|failure| {
            panic!(
                "opaque helper forwarding failed: {:?}\n{}\n{}",
                failure.err,
                failure.meta.pretty_logs(),
                failure.meta.pretty_cpi_tree(),
            )
        });

    assert!(contains_program_path(
        &metadata,
        &[
            programmable_generic_effect_core::ID,
            effect_engine_probe::ID,
            callback_capability_probe::ID,
        ]
    ));
    let helper: callback_capability_probe::HelperState =
        read_anchor_account(&fixture.svm, &helper_state);
    assert_eq!(helper.allowed_callback, fixture.callback_authority);
    assert_eq!(helper.calls, 1);
    assert_eq!(helper.value, 1);
    assert_eq!(helper.descendant_receipt_sets, 0);
    assert_eq!(
        snapshot_accounts(&fixture.svm, &protected),
        protected_before,
        "helper-only state transition changed the protected plane"
    );
}

#[test]
fn descendant_return_data_setter_is_rejected_and_its_mutation_rolls_back() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = DirectFixture::state_only(&artifacts);
    let helper_state = fixture_keypair(141).pubkey();
    install_helper_state(&mut fixture, helper_state);
    let payload = encode_explicit_plan(RECEIPT_DESCENDANT_SETTER, 0, 0, 1, &[])
        .expect("encode descendant-setter plan");
    fixture.rebuild_payload_and_opaque(
        payload,
        vec![
            AccountMeta::new_readonly(callback_capability_probe::ID, false),
            AccountMeta::new(helper_state, false),
        ],
    );
    bind_helper_to_callback(&mut fixture, helper_state);

    let mut rollback_addresses = fixture.rollback_state_addresses().to_vec();
    rollback_addresses.push(helper_state);
    let before = snapshot_accounts(&fixture.svm, &rollback_addresses);
    let (transaction, _) = fixture.compile_v0();
    let failure = fixture
        .svm
        .send_transaction(transaction)
        .expect_err("Core accepted return data set by an engine descendant");

    assert!(program_invoked(&failure.meta.logs, effect_engine_probe::ID));
    assert!(program_invoked(
        &failure.meta.logs,
        callback_capability_probe::ID
    ));
    assert!(!program_invoked(
        &failure.meta.logs,
        litesvm_token::TOKEN_ID
    ));
    assert_eq!(
        snapshot_accounts(&fixture.svm, &rollback_addresses),
        before,
        "descendant setter failure did not roll back helper and protected state"
    );
}

#[test]
fn engine_and_helper_mutations_roll_back_on_deliberate_late_failure() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let mut fixture = DirectFixture::accepted(&artifacts, DIRECT_DEFAULT_AMOUNT);
    let engine_state = fixture_keypair(142).pubkey();
    let helper_state = fixture_keypair(143).pubkey();
    install_engine_state(&mut fixture, engine_state);
    install_helper_state(&mut fixture, helper_state);
    let payload = encode_explicit_plan(
        RECEIPT_LATE_FAILURE,
        1,
        1,
        2,
        &[PlannedMove {
            source_capability_index: 0,
            destination_capability_index: 1,
            amount: DIRECT_DEFAULT_AMOUNT,
        }],
    )
    .expect("encode mutating late-failure plan");
    fixture.rebuild_payload_and_opaque(
        payload,
        vec![
            AccountMeta::new(engine_state, false),
            AccountMeta::new_readonly(callback_capability_probe::ID, false),
            AccountMeta::new(helper_state, false),
        ],
    );
    bind_helper_to_callback(&mut fixture, helper_state);

    let mut rollback_addresses = fixture.rollback_state_addresses().to_vec();
    rollback_addresses.extend([engine_state, helper_state]);
    let before = snapshot_accounts(&fixture.svm, &rollback_addresses);
    let (transaction, _) = fixture.compile_v0();
    let failure = fixture
        .svm
        .send_transaction(transaction)
        .expect_err("deliberately failing engine committed opaque mutations");

    assert!(program_invoked(&failure.meta.logs, effect_engine_probe::ID));
    assert!(program_invoked(
        &failure.meta.logs,
        callback_capability_probe::ID
    ));
    assert!(!program_invoked(
        &failure.meta.logs,
        litesvm_token::TOKEN_ID
    ));
    assert_eq!(
        snapshot_accounts(&fixture.svm, &rollback_addresses),
        before,
        "late engine failure did not roll back engine, helper, and protected state"
    );
}

#[test]
fn virtual_core_and_programdata_identities_cannot_enter_the_opaque_tail() {
    let artifacts = SbfArtifacts::load_exact()
        .expect("run ./scripts/build-sbf.sh before exact-SBF integration tests");
    let core_program_data = get_program_data_address(&programmable_generic_effect_core::ID);

    for identity in ["core-program", "selected-programdata", "core-programdata"] {
        let mut fixture = DirectFixture::state_only(&artifacts);
        let alias = match identity {
            "core-program" => programmable_generic_effect_core::ID,
            "selected-programdata" => fixture.engine_program_data,
            "core-programdata" => core_program_data,
            _ => unreachable!(),
        };
        fixture.append_opaque_account(alias, false);
        let mut rollback_addresses = fixture.rollback_state_addresses().to_vec();
        rollback_addresses.push(alias);
        let before = snapshot_accounts(&fixture.svm, &rollback_addresses);
        let (transaction, _) = fixture.compile_v0();
        let failure = fixture
            .svm
            .send_transaction(transaction)
            .expect_err("virtual protected identity reached the selected engine");

        assert!(program_invoked(
            &failure.meta.logs,
            programmable_generic_effect_core::ID
        ));
        assert!(
            !program_invoked(&failure.meta.logs, effect_engine_probe::ID),
            "{identity} reached the selected engine:\n{}",
            failure.meta.pretty_logs()
        );
        assert_eq!(
            snapshot_accounts(&fixture.svm, &rollback_addresses),
            before,
            "{identity} rejection changed landed state"
        );
    }
}

fn install_engine_state(fixture: &mut DirectFixture, address: Pubkey) {
    install_raw_account(
        &mut fixture.svm,
        address,
        effect_engine_probe::ID,
        EngineStateCandidateV0::fresh().encode().to_vec(),
        false,
    );
}

fn read_engine_state(fixture: &DirectFixture, address: Pubkey) -> EngineStateCandidateV0 {
    let account = fixture
        .svm
        .get_account(&address)
        .unwrap_or_else(|| panic!("engine state {address} is absent"));
    assert_eq!(account.data.len(), ENGINE_STATE_LEN);
    EngineStateCandidateV0::decode_exact(&account.data).expect("decode exact engine state")
}

fn install_helper_state(fixture: &mut DirectFixture, address: Pubkey) {
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

fn program_invoked(logs: &[String], program_id: Pubkey) -> bool {
    logs.iter()
        .any(|line| line.starts_with(&format!("Program {program_id} invoke")))
}
