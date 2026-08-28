use std::path::{Path, PathBuf};

use anchor_lang::{
    prelude::Pubkey, AccountDeserialize, AccountSerialize, InstructionData, ToAccountMetas,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use generated_plan_engine::{
    accounts as engine_accounts, encode_helper_payload, instruction as engine_instruction,
    quote_exact_in, EngineState, CAPABILITY_AUTHORITY_SEED, MODE_ACCEPT,
    MODE_HOSTILE_READONLY_ESCALATION, MODE_MALFORMED_RECEIPT, MODE_MISSING_RECEIPT,
    MODE_OVERSIZED_OUTPUT, MODE_TRAILING_RECEIPT_BYTE, MODE_WRONG_RECEIPT_MAGIC,
    MODE_WRONG_RECEIPT_VERSION, MODE_WRONG_REQUEST_HASH, MODE_ZERO_OUTPUT,
};
use generated_settlement_probe_wire::{
    compute_capability_hash, decode_request, encode_request, CapabilityDescriptor, EngineRequest,
    ENGINE_RECEIPT_LEN, ENGINE_REQUEST_LEN, MAX_OPAQUE_ACCOUNTS, MAX_OPAQUE_PAYLOAD_LEN,
};
use litesvm::{types::TransactionMetadata, LiteSVM};
use litesvm_cpi_tree::{CpiFrame, CpiTreeExt};
use litesvm_token::{
    get_spl_account, spl_token::state::Account as SplTokenAccount, CreateAssociatedTokenAccount,
    CreateMint, MintTo, Transfer,
};
use opaque_capability_probe::{
    accounts as helper_accounts, instruction as helper_instruction, HelperState,
};
use programmable_generated_settlement_core::{
    accounts as core_accounts,
    constants::{
        ASSET_A_INDEX_V0, ASSET_A_SEED_V0, ASSET_B_INDEX_V0, ASSET_B_SEED_V0, DOMAIN_SEED_V0,
        FEE_LEDGER_SEED_V0, FEE_VAULT_SEED_V0, INSTRUCTIONS_SYSVAR_ID, MARKET_SEED_V0,
        PROTOCOL_FEE_BPS_V0, VAULT_SEED_V0,
    },
    instruction as core_instruction, DepositV0Args, DomainV0, ExecuteEngineGeneratedProbeV0Args,
    FeeLedgerV0, InitializeMarketDomainV0Args, MarketV0,
};
use solana_keypair::Keypair;
use solana_message::{AccountMeta, Instruction, Message};
use solana_native_token::LAMPORTS_PER_SOL;
use solana_signer::Signer;
use solana_transaction::Transaction;

const MARKET_ID: [u8; 32] = [0x42; 32];
const ENGINE_REVISION: u64 = 1;
const TOKEN_DECIMALS: u8 = 6;
const INITIAL_MINT_AMOUNT: u64 = 2_000_000;
const INITIAL_LIQUIDITY: u64 = 1_000_000;
const TRADE_INPUT: u64 = 100_000;
const ENGINE_LP_FEE_BPS: u16 = 30;
const ENGINE_OUTPUT: u64 = 90_661;
const CORE_FEE: u64 = 300;
const LEGACY_PACKET_LIMIT: usize = 1_232;
const DIRECT_PACKET_CEILING: usize = 900;
const OPAQUE_PACKET_CEILING: usize = 1_000;
const EXECUTE_CU_CEILING: u64 = 200_000;

struct Fixture {
    svm: LiteSVM,
    authority: Keypair,
    mint_a: Pubkey,
    mint_b: Pubkey,
    user_source_a: Pubkey,
    provider_source_b: Pubkey,
    user_destination_b: Pubkey,
    market: Pubkey,
    domain: Pubkey,
    fee_ledger: Pubkey,
    vault_a: Pubkey,
    vault_b: Pubkey,
    fee_vault: Pubkey,
    engine_state: Pubkey,
    capability_authority: Pubkey,
    helper_state: Pubkey,
}

#[derive(Debug, PartialEq, Eq)]
struct EconomicSnapshot {
    accounts: Vec<AccountSnapshot>,
}

#[derive(Debug, PartialEq, Eq)]
struct AccountSnapshot {
    lamports: u64,
    data: Vec<u8>,
    owner: Pubkey,
    executable: bool,
    rent_epoch: u64,
}

impl Fixture {
    fn new() -> Self {
        Self::new_with_lp_fee(ENGINE_LP_FEE_BPS)
    }

    fn new_with_lp_fee(lp_fee_bps: u16) -> Self {
        let mut svm = LiteSVM::new();
        load_program(
            &mut svm,
            programmable_generated_settlement_core::ID,
            "programmable_generated_settlement_core.so",
        );
        load_program(
            &mut svm,
            generated_plan_engine::ID,
            "generated_plan_engine.so",
        );
        load_program(
            &mut svm,
            opaque_capability_probe::ID,
            "opaque_capability_probe.so",
        );

        let authority = Keypair::new();
        svm.airdrop(&authority.pubkey(), 100 * LAMPORTS_PER_SOL)
            .unwrap();

        let mint_a = CreateMint::new(&mut svm, &authority)
            .decimals(TOKEN_DECIMALS)
            .send()
            .unwrap();
        let mint_b = CreateMint::new(&mut svm, &authority)
            .decimals(TOKEN_DECIMALS)
            .send()
            .unwrap();
        let user_source_a = CreateAssociatedTokenAccount::new(&mut svm, &authority, &mint_a)
            .send()
            .unwrap();
        let provider_source_b = CreateAssociatedTokenAccount::new(&mut svm, &authority, &mint_b)
            .send()
            .unwrap();
        let recipient = Keypair::new();
        let user_destination_b = CreateAssociatedTokenAccount::new(&mut svm, &authority, &mint_b)
            .owner(&recipient.pubkey())
            .send()
            .unwrap();

        MintTo::new(
            &mut svm,
            &authority,
            &mint_a,
            &user_source_a,
            INITIAL_MINT_AMOUNT,
        )
        .send()
        .unwrap();
        MintTo::new(
            &mut svm,
            &authority,
            &mint_b,
            &provider_source_b,
            INITIAL_MINT_AMOUNT,
        )
        .send()
        .unwrap();

        let authority_key = authority.pubkey();
        let (market, _) = Pubkey::find_program_address(
            &[MARKET_SEED_V0, authority_key.as_ref(), &MARKET_ID],
            &programmable_generated_settlement_core::ID,
        );
        let (domain, _) = Pubkey::find_program_address(
            &[DOMAIN_SEED_V0, market.as_ref()],
            &programmable_generated_settlement_core::ID,
        );
        let (fee_ledger, _) = Pubkey::find_program_address(
            &[FEE_LEDGER_SEED_V0, market.as_ref(), mint_a.as_ref()],
            &programmable_generated_settlement_core::ID,
        );
        let (vault_a, _) = Pubkey::find_program_address(
            &[VAULT_SEED_V0, domain.as_ref(), ASSET_A_SEED_V0],
            &programmable_generated_settlement_core::ID,
        );
        let (vault_b, _) = Pubkey::find_program_address(
            &[VAULT_SEED_V0, domain.as_ref(), ASSET_B_SEED_V0],
            &programmable_generated_settlement_core::ID,
        );
        let (fee_vault, _) = Pubkey::find_program_address(
            &[FEE_VAULT_SEED_V0, fee_ledger.as_ref()],
            &programmable_generated_settlement_core::ID,
        );

        let engine_state_keypair = Keypair::new();
        let engine_state = engine_state_keypair.pubkey();
        let (capability_authority, _) = Pubkey::find_program_address(
            &[CAPABILITY_AUTHORITY_SEED, engine_state.as_ref()],
            &generated_plan_engine::ID,
        );
        let initialize_engine = Instruction {
            program_id: generated_plan_engine::ID,
            accounts: engine_accounts::Initialize {
                engine_state,
                capability_authority,
                authority: authority_key,
                system_program: anchor_lang::system_program::ID,
            }
            .to_account_metas(None),
            data: engine_instruction::Initialize {
                market,
                revision: ENGINE_REVISION,
                lp_fee_bps,
            }
            .data(),
        };
        must_send(
            &mut svm,
            &authority,
            initialize_engine,
            &[&engine_state_keypair],
        );

        let helper_state_keypair = Keypair::new();
        let helper_state = helper_state_keypair.pubkey();
        let initialize_helper = Instruction {
            program_id: opaque_capability_probe::ID,
            accounts: helper_accounts::Initialize {
                helper_state,
                payer: authority_key,
                system_program: anchor_lang::system_program::ID,
            }
            .to_account_metas(None),
            data: helper_instruction::Initialize {
                authority: capability_authority,
            }
            .data(),
        };
        must_send(
            &mut svm,
            &authority,
            initialize_helper,
            &[&helper_state_keypair],
        );

        let initialize_core = Instruction {
            program_id: programmable_generated_settlement_core::ID,
            accounts: core_accounts::InitializeMarketDomainV0 {
                initializer: authority_key,
                market,
                domain,
                fee_ledger,
                mint_a,
                mint_b,
                vault_a,
                vault_b,
                fee_vault,
                engine_program: generated_plan_engine::ID,
                engine_state,
                token_program: litesvm_token::TOKEN_ID,
                system_program: anchor_lang::system_program::ID,
            }
            .to_account_metas(None),
            data: core_instruction::InitializeMarketDomain {
                args: InitializeMarketDomainV0Args {
                    market_id: MARKET_ID,
                    engine_revision: ENGINE_REVISION,
                },
            }
            .data(),
        };
        must_send(&mut svm, &authority, initialize_core, &[]);

        deposit(
            &mut svm,
            &authority,
            market,
            domain,
            mint_a,
            user_source_a,
            vault_a,
            ASSET_A_INDEX_V0,
        );
        deposit(
            &mut svm,
            &authority,
            market,
            domain,
            mint_b,
            provider_source_b,
            vault_b,
            ASSET_B_INDEX_V0,
        );

        let fixture = Self {
            svm,
            authority,
            mint_a,
            mint_b,
            user_source_a,
            provider_source_b,
            user_destination_b,
            market,
            domain,
            fee_ledger,
            vault_a,
            vault_b,
            fee_vault,
            engine_state,
            capability_authority,
            helper_state,
        };
        let domain_state: DomainV0 = fixture.read_anchor(fixture.domain);
        assert_eq!(domain_state.accounted_a, INITIAL_LIQUIDITY);
        assert_eq!(domain_state.accounted_b, INITIAL_LIQUIDITY);
        assert_eq!(fixture.token_balance(fixture.vault_a), INITIAL_LIQUIDITY);
        assert_eq!(fixture.token_balance(fixture.vault_b), INITIAL_LIQUIDITY);
        fixture
    }

    fn execute_instruction(
        &self,
        mut args: ExecuteEngineGeneratedProbeV0Args,
        opaque_accounts: &[AccountMeta],
    ) -> Instruction {
        args.expected_capability_hash = self.observed_capability_hash(opaque_accounts);
        self.execute_instruction_with_expected_capability_hash(args, opaque_accounts)
    }

    fn execute_instruction_with_expected_capability_hash(
        &self,
        args: ExecuteEngineGeneratedProbeV0Args,
        opaque_accounts: &[AccountMeta],
    ) -> Instruction {
        let mut accounts = core_accounts::ExecuteEngineGeneratedProbeV0 {
            user: self.authority.pubkey(),
            market: self.market,
            domain: self.domain,
            fee_ledger: self.fee_ledger,
            mint_a: self.mint_a,
            mint_b: self.mint_b,
            user_source_a: self.user_source_a,
            user_destination_b: self.user_destination_b,
            vault_a: self.vault_a,
            vault_b: self.vault_b,
            fee_vault: self.fee_vault,
            engine_program: generated_plan_engine::ID,
            engine_state: self.engine_state,
            instructions_sysvar: INSTRUCTIONS_SYSVAR_ID,
            token_program: litesvm_token::TOKEN_ID,
        }
        .to_account_metas(None);
        accounts.extend_from_slice(opaque_accounts);
        Instruction {
            program_id: programmable_generated_settlement_core::ID,
            accounts,
            data: core_instruction::ExecuteEngineGeneratedProbe { args }.data(),
        }
    }

    fn observed_capability_hash(&self, opaque_accounts: &[AccountMeta]) -> [u8; 32] {
        let engine_state = self.svm.get_account(&self.engine_state).unwrap();
        let mut descriptors = Vec::with_capacity(2 + opaque_accounts.len());
        descriptors.push(CapabilityDescriptor {
            key: self.engine_state,
            owner: engine_state.owner,
            is_writable: true,
            is_signer: false,
            is_executable: engine_state.executable,
        });
        descriptors.push(CapabilityDescriptor {
            key: INSTRUCTIONS_SYSVAR_ID,
            owner: Pubkey::from_str_const("Sysvar1111111111111111111111111111111111111"),
            is_writable: false,
            is_signer: false,
            is_executable: false,
        });
        for meta in opaque_accounts {
            let (owner, is_executable) = match self.svm.get_account(&meta.pubkey) {
                Some(account) => (account.owner, account.executable),
                None if meta.pubkey == INSTRUCTIONS_SYSVAR_ID => (
                    Pubkey::from_str_const("Sysvar1111111111111111111111111111111111111"),
                    false,
                ),
                None if meta.pubkey == programmable_generated_settlement_core::ID => {
                    (Pubkey::default(), true)
                }
                None => panic!("missing opaque test account {}", meta.pubkey),
            };
            descriptors.push(CapabilityDescriptor {
                key: meta.pubkey,
                owner,
                is_writable: opaque_accounts
                    .iter()
                    .any(|candidate| candidate.pubkey == meta.pubkey && candidate.is_writable),
                is_signer: opaque_accounts
                    .iter()
                    .any(|candidate| candidate.pubkey == meta.pubkey && candidate.is_signer),
                is_executable,
            });
        }

        compute_capability_hash(&generated_plan_engine::ID, &descriptors).unwrap_or([0; 32])
    }

    fn execute_transaction(
        &self,
        args: ExecuteEngineGeneratedProbeV0Args,
        opaque_accounts: &[AccountMeta],
        additional_signers: &[&Keypair],
    ) -> Transaction {
        let mut signers = Vec::with_capacity(additional_signers.len() + 1);
        signers.push(&self.authority);
        signers.extend_from_slice(additional_signers);
        Transaction::new(
            &signers,
            Message::new(
                &[self.execute_instruction(args, opaque_accounts)],
                Some(&self.authority.pubkey()),
            ),
            self.svm.latest_blockhash(),
        )
    }

    fn opaque_helper_accounts(&self) -> [AccountMeta; 3] {
        [
            AccountMeta::new_readonly(opaque_capability_probe::ID, false),
            AccountMeta::new(self.helper_state, false),
            AccountMeta::new_readonly(self.capability_authority, false),
        ]
    }

    fn set_engine_mode(&mut self, mode: u8) {
        let instruction = Instruction {
            program_id: generated_plan_engine::ID,
            accounts: engine_accounts::SetMode {
                engine_state: self.engine_state,
                authority: self.authority.pubkey(),
            }
            .to_account_metas(None),
            data: engine_instruction::SetMode { mode }.data(),
        };
        must_send(&mut self.svm, &self.authority, instruction, &[]);
    }

    fn read_anchor<T: AccountDeserialize>(&self, address: Pubkey) -> T {
        let account = self.svm.get_account(&address).unwrap();
        let mut data = account.data.as_slice();
        T::try_deserialize(&mut data).unwrap()
    }

    fn token_balance(&self, address: Pubkey) -> u64 {
        let account: SplTokenAccount = get_spl_account(&self.svm, &address).unwrap();
        account.amount
    }

    fn snapshot(&self) -> EconomicSnapshot {
        EconomicSnapshot {
            accounts: self
                .snapshot_addresses()
                .iter()
                .map(|address| self.account_snapshot(*address))
                .collect(),
        }
    }

    fn snapshot_addresses(&self) -> [Pubkey; 11] {
        [
            self.market,
            self.domain,
            self.fee_ledger,
            self.engine_state,
            self.capability_authority,
            self.helper_state,
            self.user_source_a,
            self.user_destination_b,
            self.vault_a,
            self.vault_b,
            self.fee_vault,
        ]
    }

    fn changed_snapshot_addresses(&self, before: &EconomicSnapshot) -> Vec<Pubkey> {
        self.snapshot_addresses()
            .into_iter()
            .zip(before.accounts.iter())
            .filter_map(|(address, old)| {
                let current = self.account_snapshot(address);
                (&current != old).then_some(address)
            })
            .collect()
    }

    fn overwrite_helper_state(&mut self, calls: u64, value: u64) {
        let mut account = self.svm.get_account(&self.helper_state).unwrap();
        let replacement = HelperState {
            authority: self.capability_authority,
            calls,
            value,
        };
        let mut data = Vec::with_capacity(account.data.len());
        replacement.try_serialize(&mut data).unwrap();
        assert_eq!(data.len(), account.data.len());
        account.data = data;
        self.svm.set_account(self.helper_state, account).unwrap();
    }

    fn clone_account_with_owner(&mut self, source: Pubkey, owner: Pubkey) -> Pubkey {
        let key = Pubkey::new_unique();
        let mut account = self.svm.get_account(&source).unwrap();
        account.owner = owner;
        account.executable = false;
        self.svm.set_account(key, account).unwrap();
        key
    }

    fn account_snapshot(&self, address: Pubkey) -> AccountSnapshot {
        let account = self.svm.get_account(&address).unwrap();
        AccountSnapshot {
            lamports: account.lamports,
            data: account.data,
            owner: account.owner,
            executable: account.executable,
            rent_epoch: account.rent_epoch,
        }
    }
}

#[test]
fn happy_exact_in_uses_engine_output_and_preserves_resource_headroom() {
    let mut fixture = Fixture::new();
    let before = fixture.snapshot();
    let transaction = fixture.execute_transaction(valid_args(1, Vec::new()), &[], &[]);
    let packet_bytes = wincode::serialize(&transaction).unwrap().len();
    let account_count = transaction.message.account_keys.len();
    let writable_accounts = writable_account_count(&transaction.message);
    let message_keys = transaction.message.account_keys.clone();

    assert!(packet_bytes < LEGACY_PACKET_LIMIT);
    assert!(packet_bytes <= DIRECT_PACKET_CEILING);
    assert_eq!(account_count, 16);
    assert_eq!(writable_accounts, 9);

    let metadata = send_success(&mut fixture.svm, transaction, "direct exact-in");
    assert!(metadata.compute_units_consumed <= EXECUTE_CU_CEILING);
    assert_execution_shape(&metadata, &message_keys, fixture.engine_state, &[], false);

    let market: MarketV0 = fixture.read_anchor(fixture.market);
    let domain: DomainV0 = fixture.read_anchor(fixture.domain);
    let fees: FeeLedgerV0 = fixture.read_anchor(fixture.fee_ledger);
    let engine: EngineState = fixture.read_anchor(fixture.engine_state);
    let helper: HelperState = fixture.read_anchor(fixture.helper_state);

    assert_eq!(market.fee_bps, PROTOCOL_FEE_BPS_V0);
    assert_eq!(domain.accounted_a, INITIAL_LIQUIDITY + TRADE_INPUT);
    assert_eq!(domain.accounted_b, INITIAL_LIQUIDITY - ENGINE_OUTPUT);
    assert_eq!(fees.accounted_fee_a, CORE_FEE);
    assert_eq!(engine.lp_fee_bps, ENGINE_LP_FEE_BPS);
    assert_eq!(engine.sequence, 1);
    assert_eq!(engine.last_amount_out, ENGINE_OUTPUT);
    assert_ne!(engine.last_request_hash, [0; 32]);
    assert_eq!(helper.calls, 0);
    assert_eq!(helper.value, 0);

    assert_eq!(
        fixture.token_balance(fixture.user_source_a),
        INITIAL_MINT_AMOUNT - INITIAL_LIQUIDITY - TRADE_INPUT - CORE_FEE
    );
    assert_eq!(
        fixture.token_balance(fixture.user_destination_b),
        ENGINE_OUTPUT
    );
    assert_eq!(
        fixture.token_balance(fixture.vault_a),
        INITIAL_LIQUIDITY + TRADE_INPUT
    );
    assert_eq!(
        fixture.token_balance(fixture.vault_b),
        INITIAL_LIQUIDITY - ENGINE_OUTPUT
    );
    assert_eq!(fixture.token_balance(fixture.fee_vault), CORE_FEE);
    assert_eq!(
        fixture.changed_snapshot_addresses(&before),
        [
            fixture.domain,
            fixture.fee_ledger,
            fixture.engine_state,
            fixture.user_source_a,
            fixture.user_destination_b,
            fixture.vault_a,
            fixture.vault_b,
            fixture.fee_vault,
        ],
        "a successful direct settlement changed an unpredicted account"
    );

    eprintln!(
        "generated direct metrics: {packet_bytes} bytes, {account_count} accounts, \
         {writable_accounts} writable, {} CU",
        metadata.compute_units_consumed
    );
}

#[test]
fn opaque_helper_cpi_mutates_only_declared_state_and_engine_return_wins() {
    let mut fixture = Fixture::new();
    let helper_increment = 7;
    let opaque_accounts = fixture.opaque_helper_accounts();
    let transaction = fixture.execute_transaction(
        valid_args(1, encode_helper_payload(helper_increment).to_vec()),
        &opaque_accounts,
        &[],
    );
    let packet_bytes = wincode::serialize(&transaction).unwrap().len();
    let account_count = transaction.message.account_keys.len();
    let writable_accounts = writable_account_count(&transaction.message);
    let message_keys = transaction.message.account_keys.clone();

    assert!(packet_bytes < LEGACY_PACKET_LIMIT);
    assert!(packet_bytes <= OPAQUE_PACKET_CEILING);
    assert_eq!(account_count, 19);
    assert_eq!(writable_accounts, 10);

    let metadata = send_success(&mut fixture.svm, transaction, "opaque helper exact-in");
    assert!(metadata.compute_units_consumed <= EXECUTE_CU_CEILING);
    let expected_opaque_keys = [
        opaque_capability_probe::ID,
        fixture.helper_state,
        fixture.capability_authority,
    ];
    assert_execution_shape(
        &metadata,
        &message_keys,
        fixture.engine_state,
        &expected_opaque_keys,
        true,
    );

    let engine: EngineState = fixture.read_anchor(fixture.engine_state);
    let helper: HelperState = fixture.read_anchor(fixture.helper_state);
    assert_eq!(engine.sequence, 1);
    assert_eq!(engine.last_amount_out, ENGINE_OUTPUT);
    assert_eq!(helper.calls, 1);
    assert_eq!(helper.value, helper_increment);
    assert_eq!(
        fixture.token_balance(fixture.user_destination_b),
        ENGINE_OUTPUT
    );

    eprintln!(
        "generated opaque metrics: {packet_bytes} bytes, {account_count} accounts, \
         {writable_accounts} writable, {} CU, depth 3",
        metadata.compute_units_consumed
    );
}

#[test]
fn omitted_capability_and_failing_custom_program_cannot_leave_state() {
    let mut omitted = Fixture::new();
    let before = omitted.snapshot();
    let transaction =
        omitted.execute_transaction(valid_args(1, encode_helper_payload(1).to_vec()), &[], &[]);
    let failure = omitted.svm.send_transaction(transaction).unwrap_err();
    assert_log_contains(&failure.meta, "InvalidHelperCapabilityClosure");
    assert_eq!(omitted.snapshot(), before);
    assert_engine_reached_without_token_cpi(&failure.meta, false);

    let mut failing_helper = Fixture::new();
    failing_helper.overwrite_helper_state(u64::MAX, 0);
    let before = failing_helper.snapshot();
    let opaque_accounts = failing_helper.opaque_helper_accounts();
    let transaction = failing_helper.execute_transaction(
        valid_args(1, encode_helper_payload(1).to_vec()),
        &opaque_accounts,
        &[],
    );
    let failure = failing_helper
        .svm
        .send_transaction(transaction)
        .unwrap_err();
    assert_log_contains(&failure.meta, "ArithmeticOverflow");
    assert_eq!(failing_helper.snapshot(), before);
    assert_engine_reached_without_token_cpi(&failure.meta, true);
}

#[test]
fn changing_only_engine_lp_fee_changes_the_generated_output() {
    let mut thirty_bps = Fixture::new_with_lp_fee(30);
    let thirty_tx = thirty_bps.execute_transaction(valid_args(1, Vec::new()), &[], &[]);
    send_success(&mut thirty_bps.svm, thirty_tx, "30 bps engine");

    let mut hundred_bps = Fixture::new_with_lp_fee(100);
    let hundred_tx = hundred_bps.execute_transaction(valid_args(1, Vec::new()), &[], &[]);
    send_success(&mut hundred_bps.svm, hundred_tx, "100 bps engine");

    let thirty_engine: EngineState = thirty_bps.read_anchor(thirty_bps.engine_state);
    let hundred_engine: EngineState = hundred_bps.read_anchor(hundred_bps.engine_state);
    let thirty_fees: FeeLedgerV0 = thirty_bps.read_anchor(thirty_bps.fee_ledger);
    let hundred_fees: FeeLedgerV0 = hundred_bps.read_anchor(hundred_bps.fee_ledger);

    assert_eq!(thirty_engine.last_amount_out, ENGINE_OUTPUT);
    assert_eq!(hundred_engine.last_amount_out, 90_081);
    assert_ne!(
        thirty_engine.last_amount_out,
        hundred_engine.last_amount_out
    );
    assert_eq!(thirty_fees.accounted_fee_a, CORE_FEE);
    assert_eq!(hundred_fees.accounted_fee_a, CORE_FEE);
}

#[test]
fn a_conservative_but_destructive_engine_quote_is_explicit_engine_risk() {
    let mut fixture = Fixture::new_with_lp_fee(9_999);
    let expected_output = quote_exact_in(INITIAL_LIQUIDITY, INITIAL_LIQUIDITY, TRADE_INPUT, 9_999)
        .unwrap()
        .amount_out;
    assert_eq!(expected_output, 9);

    let transaction = fixture.execute_transaction(valid_args(1, Vec::new()), &[], &[]);
    send_success(
        &mut fixture.svm,
        transaction,
        "bounded but economically destructive engine quote",
    );

    let engine: EngineState = fixture.read_anchor(fixture.engine_state);
    let domain: DomainV0 = fixture.read_anchor(fixture.domain);
    assert_eq!(engine.last_amount_out, expected_output);
    assert_eq!(
        fixture.token_balance(fixture.user_destination_b),
        expected_output
    );
    assert_eq!(domain.accounted_b, INITIAL_LIQUIDITY - expected_output);
    assert_eq!(
        fixture.token_balance(fixture.user_source_a),
        INITIAL_MINT_AMOUNT - INITIAL_LIQUIDITY - TRADE_INPUT - CORE_FEE
    );
}

#[test]
fn opaque_capability_gate_rejects_authority_escalation_before_engine_cpi() {
    let non_signer_cases = [
        ("OpaqueFixedRoleAlias", 0_u8),
        ("OpaqueProtectedTokenAccountWritable", 1),
        ("OpaqueCoreOwnedAccount", 2),
    ];

    for (expected_error, kind) in non_signer_cases {
        let mut fixture = Fixture::new();
        let opaque_accounts = match kind {
            0 => vec![AccountMeta::new_readonly(fixture.market, false)],
            1 => vec![AccountMeta::new(fixture.provider_source_b, false)],
            2 => {
                let opaque_core_account = Pubkey::new_unique();
                let cloned_core_account = fixture.svm.get_account(&fixture.market).unwrap();
                fixture
                    .svm
                    .set_account(opaque_core_account, cloned_core_account)
                    .unwrap();
                vec![AccountMeta::new_readonly(opaque_core_account, false)]
            }
            _ => unreachable!(),
        };
        let before = fixture.snapshot();
        let transaction =
            fixture.execute_transaction(valid_args(1, Vec::new()), &opaque_accounts, &[]);
        let failure = fixture.svm.send_transaction(transaction).unwrap_err();

        assert_log_contains(&failure.meta, expected_error);
        assert_eq!(fixture.snapshot(), before, "gate case {expected_error}");
        assert_no_child_cpi(&failure.meta);
    }

    let mut fixture = Fixture::new();
    let opaque_signer = Keypair::new();
    fixture
        .svm
        .airdrop(&opaque_signer.pubkey(), LAMPORTS_PER_SOL)
        .unwrap();
    let before = fixture.snapshot();
    let transaction = fixture.execute_transaction(
        valid_args(1, Vec::new()),
        &[AccountMeta::new_readonly(opaque_signer.pubkey(), true)],
        &[&opaque_signer],
    );
    let failure = fixture.svm.send_transaction(transaction).unwrap_err();

    assert_log_contains(&failure.meta, "OpaqueSignerForbidden");
    assert_eq!(fixture.snapshot(), before);
    assert_no_child_cpi(&failure.meta);
}

#[test]
fn duplicate_privileges_are_normalized_at_the_eight_position_sbf_boundary() {
    let mut accepted = Fixture::new();
    let accepted_accounts =
        vec![AccountMeta::new_readonly(opaque_capability_probe::ID, false); MAX_OPAQUE_ACCOUNTS];
    let accepted_keys = vec![opaque_capability_probe::ID; MAX_OPAQUE_ACCOUNTS];
    let transaction =
        accepted.execute_transaction(valid_args(1, Vec::new()), &accepted_accounts, &[]);
    let message_keys = transaction.message.account_keys.clone();
    let metadata = send_success(
        &mut accepted.svm,
        transaction,
        "eight duplicate read-only capabilities",
    );
    assert_execution_shape(
        &metadata,
        &message_keys,
        accepted.engine_state,
        &accepted_keys,
        false,
    );

    let mut executable = Fixture::new();
    let mut executable_accounts =
        vec![AccountMeta::new_readonly(opaque_capability_probe::ID, false); MAX_OPAQUE_ACCOUNTS];
    executable_accounts[MAX_OPAQUE_ACCOUNTS - 1] =
        AccountMeta::new(opaque_capability_probe::ID, false);
    let before = executable.snapshot();
    let transaction =
        executable.execute_transaction(valid_args(1, Vec::new()), &executable_accounts, &[]);
    let failure = executable.svm.send_transaction(transaction).unwrap_err();
    assert_log_contains(&failure.meta, "OpaqueExecutableWritable");
    assert_eq!(executable.snapshot(), before);
    assert_no_child_cpi(&failure.meta);

    let mut classic_token = Fixture::new();
    let mut classic_accounts =
        vec![
            AccountMeta::new_readonly(classic_token.provider_source_b, false);
            MAX_OPAQUE_ACCOUNTS
        ];
    classic_accounts[MAX_OPAQUE_ACCOUNTS / 2] =
        AccountMeta::new(classic_token.provider_source_b, false);
    let before = classic_token.snapshot();
    let transaction =
        classic_token.execute_transaction(valid_args(1, Vec::new()), &classic_accounts, &[]);
    let failure = classic_token.svm.send_transaction(transaction).unwrap_err();
    assert_log_contains(&failure.meta, "OpaqueProtectedTokenAccountWritable");
    assert_eq!(classic_token.snapshot(), before);
    assert_no_child_cpi(&failure.meta);

    let mut token_2022 = Fixture::new();
    let token_2022_owned =
        token_2022.clone_account_with_owner(token_2022.helper_state, anchor_spl::token_2022::ID);
    let mut token_2022_accounts =
        vec![AccountMeta::new_readonly(token_2022_owned, false); MAX_OPAQUE_ACCOUNTS];
    token_2022_accounts[1] = AccountMeta::new(token_2022_owned, false);
    let before = token_2022.snapshot();
    let transaction =
        token_2022.execute_transaction(valid_args(1, Vec::new()), &token_2022_accounts, &[]);
    let failure = token_2022.svm.send_transaction(transaction).unwrap_err();
    assert_log_contains(&failure.meta, "OpaqueProtectedTokenAccountWritable");
    assert_eq!(token_2022.snapshot(), before);
    assert_no_child_cpi(&failure.meta);
}

#[test]
fn substituted_or_omitted_fixed_accounts_fail_before_the_engine() {
    let mut fixture = Fixture::new();
    let baseline = fixture.snapshot();
    let substitutions = [
        ("market", 1, fixture.domain),
        ("domain", 2, fixture.fee_ledger),
        ("fee ledger", 3, fixture.domain),
        ("mint A", 4, fixture.mint_b),
        ("mint B", 5, fixture.mint_a),
        ("user source", 6, fixture.provider_source_b),
        ("recipient", 7, fixture.user_source_a),
        ("input vault", 8, fixture.user_source_a),
        ("output vault", 9, fixture.provider_source_b),
        ("fee vault", 10, fixture.user_source_a),
        ("engine program", 11, opaque_capability_probe::ID),
        ("engine state", 12, fixture.helper_state),
        ("instructions sysvar", 13, fixture.capability_authority),
        ("token program", 14, opaque_capability_probe::ID),
    ];

    for (label, index, replacement) in substitutions {
        let mut instruction = fixture.execute_instruction(valid_args(1, Vec::new()), &[]);
        instruction.accounts[index].pubkey = replacement;
        let transaction = Transaction::new(
            &[&fixture.authority],
            Message::new(&[instruction], Some(&fixture.authority.pubkey())),
            fixture.svm.latest_blockhash(),
        );
        let failure = fixture.svm.send_transaction(transaction).unwrap_err();
        assert_eq!(fixture.snapshot(), baseline, "substitution mutated {label}");
        assert_no_child_cpi(&failure.meta);
    }

    let mut omitted = fixture.execute_instruction(valid_args(1, Vec::new()), &[]);
    assert_eq!(omitted.accounts.len(), 15);
    omitted.accounts.pop();
    let transaction = Transaction::new(
        &[&fixture.authority],
        Message::new(&[omitted], Some(&fixture.authority.pubkey())),
        fixture.svm.latest_blockhash(),
    );
    let failure = fixture.svm.send_transaction(transaction).unwrap_err();
    assert_eq!(
        fixture.snapshot(),
        baseline,
        "omitted fixed account mutated state"
    );
    assert_no_child_cpi(&failure.meta);
}

#[test]
fn external_readonly_duplicates_are_accepted_and_order_bound() {
    let mut fixture = Fixture::new();
    let baseline = fixture.svm.clone();
    let first_order = [
        AccountMeta::new_readonly(opaque_capability_probe::ID, false),
        AccountMeta::new_readonly(fixture.helper_state, false),
        AccountMeta::new_readonly(opaque_capability_probe::ID, false),
    ];
    let first_tx = fixture.execute_transaction(valid_args(1, Vec::new()), &first_order, &[]);
    let first_keys = first_tx.message.account_keys.clone();
    let first_metadata = send_success(&mut fixture.svm, first_tx, "first duplicate order");
    let first_engine: EngineState = fixture.read_anchor(fixture.engine_state);
    assert_execution_shape(
        &first_metadata,
        &first_keys,
        fixture.engine_state,
        &[
            opaque_capability_probe::ID,
            fixture.helper_state,
            opaque_capability_probe::ID,
        ],
        false,
    );

    fixture.svm = baseline;
    let second_order = [
        AccountMeta::new_readonly(opaque_capability_probe::ID, false),
        AccountMeta::new_readonly(opaque_capability_probe::ID, false),
        AccountMeta::new_readonly(fixture.helper_state, false),
    ];
    let second_tx = fixture.execute_transaction(valid_args(1, Vec::new()), &second_order, &[]);
    let second_metadata = send_success(&mut fixture.svm, second_tx, "second duplicate order");
    let second_engine: EngineState = fixture.read_anchor(fixture.engine_state);

    assert_eq!(first_engine.last_amount_out, ENGINE_OUTPUT);
    assert_eq!(second_engine.last_amount_out, ENGINE_OUTPUT);
    assert_ne!(
        first_engine.last_request_hash,
        second_engine.last_request_hash
    );
    assert_eq!(
        fixture
            .read_anchor::<HelperState>(fixture.helper_state)
            .calls,
        0
    );
    assert_eq!(frame_depth(&second_metadata.cpi_tree()[0]), 2);
}

#[test]
fn invalid_engine_results_and_post_engine_bounds_roll_back_every_account() {
    let cases = [
        (MODE_WRONG_REQUEST_HASH, 1, "EngineReceiptRequestMismatch"),
        (MODE_MALFORMED_RECEIPT, 1, "InvalidEngineReceipt"),
        (MODE_WRONG_RECEIPT_MAGIC, 1, "InvalidEngineReceipt"),
        (MODE_WRONG_RECEIPT_VERSION, 1, "InvalidEngineReceipt"),
        (MODE_TRAILING_RECEIPT_BYTE, 1, "InvalidEngineReceipt"),
        (MODE_ZERO_OUTPUT, 1, "ZeroAmount"),
        (MODE_OVERSIZED_OUTPUT, 1, "InsufficientAccountedLiquidity"),
        (MODE_ACCEPT, ENGINE_OUTPUT + 1, "OutputBelowUserMinimum"),
    ];

    for (mode, minimum_output, expected_error) in cases {
        let mut fixture = Fixture::new();
        fixture.set_engine_mode(mode);
        let before = fixture.snapshot();
        let opaque_accounts = fixture.opaque_helper_accounts();
        let transaction = fixture.execute_transaction(
            valid_args(minimum_output, encode_helper_payload(5).to_vec()),
            &opaque_accounts,
            &[],
        );
        let failure = fixture.svm.send_transaction(transaction).unwrap_err();

        assert_log_contains(&failure.meta, expected_error);
        assert_eq!(
            fixture.snapshot(),
            before,
            "engine failure case {expected_error}"
        );
        let engine: EngineState = fixture.read_anchor(fixture.engine_state);
        let helper: HelperState = fixture.read_anchor(fixture.helper_state);
        assert_eq!(engine.sequence, 0, "engine case {expected_error}");
        assert_eq!(engine.mode, mode, "engine case {expected_error}");
        assert_eq!(helper.calls, 0, "helper case {expected_error}");
        assert_engine_and_helper_reached_without_token_cpi(&failure.meta);
    }

    let mut fixture = Fixture::new();
    fixture.set_engine_mode(MODE_HOSTILE_READONLY_ESCALATION);
    let before = fixture.snapshot();
    let opaque_accounts = fixture.opaque_helper_accounts();
    let transaction = fixture.execute_transaction(
        valid_args(1, encode_helper_payload(5).to_vec()),
        &opaque_accounts,
        &[],
    );
    let failure = fixture.svm.send_transaction(transaction).unwrap_err();
    assert!(
        failure.meta.logs.iter().any(|line| {
            line.contains("writable privilege escalated")
                || line.contains("unauthorized signer or writable account")
        }),
        "missing privilege-escalation evidence:\n{}",
        failure.meta.pretty_logs()
    );
    assert_eq!(fixture.snapshot(), before);
    assert_engine_and_helper_reached_without_token_cpi(&failure.meta);
}

#[test]
fn absent_return_data_is_distinct_from_a_wrong_receipt_setter() {
    let mut missing = Fixture::new();
    missing.set_engine_mode(MODE_MISSING_RECEIPT);
    let before = missing.snapshot();
    let transaction = missing.execute_transaction(valid_args(1, Vec::new()), &[], &[]);
    let failure = missing.svm.send_transaction(transaction).unwrap_err();
    assert_log_contains(&failure.meta, "MissingEngineReceipt");
    assert_eq!(missing.snapshot(), before);
    assert_engine_reached_without_token_cpi(&failure.meta, false);

    let mut wrong_setter = Fixture::new();
    wrong_setter.set_engine_mode(MODE_MISSING_RECEIPT);
    let before = wrong_setter.snapshot();
    let opaque_accounts = wrong_setter.opaque_helper_accounts();
    let transaction = wrong_setter.execute_transaction(
        valid_args(1, encode_helper_payload(1).to_vec()),
        &opaque_accounts,
        &[],
    );
    let failure = wrong_setter.svm.send_transaction(transaction).unwrap_err();
    assert_log_contains(&failure.meta, "InvalidEngineReceiptSetter");
    assert_eq!(wrong_setter.snapshot(), before);
    assert_engine_reached_without_token_cpi(&failure.meta, true);
}

#[test]
fn signed_expiry_and_fee_bounds_fail_before_untrusted_code() {
    let mut fixture = Fixture::new();
    fixture.svm.warp_to_slot(1);

    let mut expired = valid_args(1, Vec::new());
    expired.expires_at_slot = 0;
    let mut fee_above_maximum = valid_args(1, Vec::new());
    fee_above_maximum.max_protocol_fee = CORE_FEE - 1;
    let mut debit_above_maximum = valid_args(1, Vec::new());
    debit_above_maximum.max_total_input_debit = TRADE_INPUT + CORE_FEE - 1;
    let mut zero_input = valid_args(1, Vec::new());
    zero_input.amount_in = 0;

    let cases = [
        ("RequestExpired", expired),
        ("ProtocolFeeAboveUserMaximum", fee_above_maximum),
        ("TotalDebitAboveUserMaximum", debit_above_maximum),
        ("ZeroAmount", zero_input),
    ];
    let baseline = fixture.snapshot();
    for (expected_error, args) in cases {
        let transaction = fixture.execute_transaction(args, &[], &[]);
        let failure = fixture.svm.send_transaction(transaction).unwrap_err();
        assert_log_contains(&failure.meta, expected_error);
        assert_eq!(fixture.snapshot(), baseline, "bound case {expected_error}");
        assert_no_child_cpi(&failure.meta);
    }
}

#[test]
fn wrong_or_stale_capability_expectations_fail_before_the_engine() {
    let mut wrong = Fixture::new();
    let before = wrong.snapshot();
    let mut args = valid_args(1, Vec::new());
    args.expected_capability_hash = [0x5a; 32];
    let instruction = wrong.execute_instruction_with_expected_capability_hash(args, &[]);
    let transaction = Transaction::new(
        &[&wrong.authority],
        Message::new(&[instruction], Some(&wrong.authority.pubkey())),
        wrong.svm.latest_blockhash(),
    );
    let failure = wrong.svm.send_transaction(transaction).unwrap_err();
    assert_log_contains(&failure.meta, "CapabilityHashExpectationMismatch");
    assert_eq!(wrong.snapshot(), before);
    assert_no_child_cpi(&failure.meta);

    let mut stale = Fixture::new();
    let opaque_accounts = [AccountMeta::new_readonly(stale.helper_state, false)];
    let stale_expectation = stale.observed_capability_hash(&opaque_accounts);
    let mut account = stale.svm.get_account(&stale.helper_state).unwrap();
    account.owner = anchor_lang::system_program::ID;
    stale.svm.set_account(stale.helper_state, account).unwrap();
    let before = stale.snapshot();
    let mut args = valid_args(1, Vec::new());
    args.expected_capability_hash = stale_expectation;
    let instruction =
        stale.execute_instruction_with_expected_capability_hash(args, &opaque_accounts);
    let transaction = Transaction::new(
        &[&stale.authority],
        Message::new(&[instruction], Some(&stale.authority.pubkey())),
        stale.svm.latest_blockhash(),
    );
    let failure = stale.svm.send_transaction(transaction).unwrap_err();
    assert_log_contains(&failure.meta, "CapabilityHashExpectationMismatch");
    assert_eq!(stale.snapshot(), before);
    assert_no_child_cpi(&failure.meta);
}

#[test]
fn identical_user_terms_bind_new_accounted_state_on_a_second_execution() {
    let mut fixture = Fixture::new();
    let first_transaction = fixture.execute_transaction(valid_args(1, Vec::new()), &[], &[]);
    send_success(
        &mut fixture.svm,
        first_transaction,
        "first state-bound execution",
    );
    let first_engine: EngineState = fixture.read_anchor(fixture.engine_state);
    let first_domain: DomainV0 = fixture.read_anchor(fixture.domain);

    fixture.svm.expire_blockhash();
    let second_transaction = fixture.execute_transaction(valid_args(1, Vec::new()), &[], &[]);
    send_success(
        &mut fixture.svm,
        second_transaction,
        "second state-bound execution",
    );
    let second_engine: EngineState = fixture.read_anchor(fixture.engine_state);
    let second_domain: DomainV0 = fixture.read_anchor(fixture.domain);
    let expected_second = quote_exact_in(
        first_domain.accounted_a,
        first_domain.accounted_b,
        TRADE_INPUT,
        ENGINE_LP_FEE_BPS,
    )
    .unwrap();

    assert_eq!(first_engine.sequence, 1);
    assert_eq!(second_engine.sequence, 2);
    assert_ne!(
        first_engine.last_request_hash,
        second_engine.last_request_hash
    );
    assert_eq!(second_engine.last_amount_out, expected_second.amount_out);
    assert_eq!(
        second_domain.accounted_a,
        first_domain.accounted_a + TRADE_INPUT
    );
    assert_eq!(
        second_domain.accounted_b,
        first_domain.accounted_b - expected_second.amount_out
    );
}

#[test]
fn insufficient_user_source_fails_before_the_engine() {
    let mut fixture = Fixture::new();
    let source_balance = fixture.token_balance(fixture.user_source_a);
    assert_eq!(source_balance, INITIAL_MINT_AMOUNT - INITIAL_LIQUIDITY);
    let fee = protocol_fee(source_balance);
    let before = fixture.snapshot();
    let args = ExecuteEngineGeneratedProbeV0Args {
        amount_in: source_balance,
        max_total_input_debit: source_balance + fee,
        min_output_credit: 1,
        max_protocol_fee: fee,
        expires_at_slot: u64::MAX,
        expected_capability_hash: [0; 32],
        opaque_payload: Vec::new(),
    };
    let transaction = fixture.execute_transaction(args, &[], &[]);
    let failure = fixture.svm.send_transaction(transaction).unwrap_err();

    assert_log_contains(&failure.meta, "InsufficientUserSourceBalance");
    assert_eq!(fixture.snapshot(), before);
    assert_no_child_cpi(&failure.meta);
}

#[test]
fn raw_vault_donation_does_not_change_the_engine_quote() {
    let mut fixture = Fixture::new();
    let donation = 500_000;
    Transfer::new(
        &mut fixture.svm,
        &fixture.authority,
        &fixture.mint_b,
        &fixture.vault_b,
        donation,
    )
    .source(&fixture.provider_source_b)
    .send()
    .unwrap();

    let domain_before: DomainV0 = fixture.read_anchor(fixture.domain);
    assert_eq!(domain_before.accounted_b, INITIAL_LIQUIDITY);
    assert_eq!(
        fixture.token_balance(fixture.vault_b),
        INITIAL_LIQUIDITY + donation
    );

    let transaction = fixture.execute_transaction(valid_args(1, Vec::new()), &[], &[]);
    send_success(&mut fixture.svm, transaction, "donation-independent quote");

    let engine: EngineState = fixture.read_anchor(fixture.engine_state);
    let domain_after: DomainV0 = fixture.read_anchor(fixture.domain);
    assert_eq!(engine.last_amount_out, ENGINE_OUTPUT);
    assert_eq!(domain_after.accounted_b, INITIAL_LIQUIDITY - ENGINE_OUTPUT);
    assert_eq!(
        fixture.token_balance(fixture.vault_b),
        INITIAL_LIQUIDITY + donation - ENGINE_OUTPUT
    );
}

#[test]
fn input_and_fee_vault_donations_never_become_accounting_or_quote_inputs() {
    let mut fixture = Fixture::new();
    let input_donation = 80_000;
    let fee_donation = 20_000;
    Transfer::new(
        &mut fixture.svm,
        &fixture.authority,
        &fixture.mint_a,
        &fixture.vault_a,
        input_donation,
    )
    .source(&fixture.user_source_a)
    .send()
    .unwrap();
    Transfer::new(
        &mut fixture.svm,
        &fixture.authority,
        &fixture.mint_a,
        &fixture.fee_vault,
        fee_donation,
    )
    .source(&fixture.user_source_a)
    .send()
    .unwrap();

    let domain_before: DomainV0 = fixture.read_anchor(fixture.domain);
    let fees_before: FeeLedgerV0 = fixture.read_anchor(fixture.fee_ledger);
    assert_eq!(domain_before.accounted_a, INITIAL_LIQUIDITY);
    assert_eq!(fees_before.accounted_fee_a, 0);
    assert_eq!(
        fixture.token_balance(fixture.vault_a),
        INITIAL_LIQUIDITY + input_donation
    );
    assert_eq!(fixture.token_balance(fixture.fee_vault), fee_donation);

    let transaction = fixture.execute_transaction(valid_args(1, Vec::new()), &[], &[]);
    send_success(
        &mut fixture.svm,
        transaction,
        "input and fee donation invariance",
    );

    let domain_after: DomainV0 = fixture.read_anchor(fixture.domain);
    let fees_after: FeeLedgerV0 = fixture.read_anchor(fixture.fee_ledger);
    let engine: EngineState = fixture.read_anchor(fixture.engine_state);
    assert_eq!(engine.last_amount_out, ENGINE_OUTPUT);
    assert_eq!(domain_after.accounted_a, INITIAL_LIQUIDITY + TRADE_INPUT);
    assert_eq!(fees_after.accounted_fee_a, CORE_FEE);
    assert_eq!(
        fixture.token_balance(fixture.vault_a),
        INITIAL_LIQUIDITY + input_donation + TRADE_INPUT
    );
    assert_eq!(
        fixture.token_balance(fixture.fee_vault),
        fee_donation + CORE_FEE
    );
}

#[test]
fn maximum_opaque_closure_payload_and_nested_helper_respect_resource_bounds() {
    let mut fixture = Fixture::new();
    let helper_increment = 7;
    let mut opaque_accounts = fixture.opaque_helper_accounts().to_vec();
    let additional_account_count = MAX_OPAQUE_ACCOUNTS - opaque_accounts.len();
    opaque_accounts.extend(existing_readonly_external_accounts(
        &mut fixture,
        additional_account_count,
    ));
    assert_eq!(opaque_accounts.len(), MAX_OPAQUE_ACCOUNTS);
    let opaque_keys: Vec<_> = opaque_accounts.iter().map(|meta| meta.pubkey).collect();
    let mut payload = vec![0x7f; MAX_OPAQUE_PAYLOAD_LEN];
    let helper_payload = encode_helper_payload(helper_increment);
    payload[..helper_payload.len()].copy_from_slice(&helper_payload);
    let transaction = fixture.execute_transaction(valid_args(1, payload), &opaque_accounts, &[]);
    let packet_bytes = wincode::serialize(&transaction).unwrap().len();
    let account_count = transaction.message.account_keys.len();
    let writable_accounts = writable_account_count(&transaction.message);
    let message_keys = transaction.message.account_keys.clone();

    assert!(packet_bytes < LEGACY_PACKET_LIMIT);
    assert_eq!(account_count, 16 + MAX_OPAQUE_ACCOUNTS);
    assert_eq!(writable_accounts, 10);

    let metadata = send_success(
        &mut fixture.svm,
        transaction,
        "maximum opaque closure with nested helper",
    );
    assert!(metadata.compute_units_consumed <= EXECUTE_CU_CEILING);
    assert_engine_receipt_log(&metadata);
    assert_execution_shape(
        &metadata,
        &message_keys,
        fixture.engine_state,
        &opaque_keys,
        true,
    );
    let engine: EngineState = fixture.read_anchor(fixture.engine_state);
    let helper: HelperState = fixture.read_anchor(fixture.helper_state);
    assert_eq!(engine.sequence, 1);
    assert_eq!(engine.last_amount_out, ENGINE_OUTPUT);
    assert_eq!(helper.calls, 1);
    assert_eq!(helper.value, helper_increment);
    assert_eq!(
        fixture.token_balance(fixture.user_destination_b),
        ENGINE_OUTPUT
    );

    eprintln!(
        "generated max-closure metrics: {packet_bytes} bytes, {account_count} accounts, \
         {writable_accounts} writable, {} CU, depth 3",
        metadata.compute_units_consumed
    );

    let mut fixture = Fixture::new();
    let too_many_accounts =
        existing_readonly_external_accounts(&mut fixture, MAX_OPAQUE_ACCOUNTS + 1);
    let before = fixture.snapshot();
    let transaction =
        fixture.execute_transaction(valid_args(1, Vec::new()), &too_many_accounts, &[]);
    let failure = fixture.svm.send_transaction(transaction).unwrap_err();
    assert_log_contains(&failure.meta, "TooManyOpaqueAccounts");
    assert_eq!(fixture.snapshot(), before);
    assert_no_child_cpi(&failure.meta);

    let mut fixture = Fixture::new();
    let before = fixture.snapshot();
    let oversized_payload = vec![0x7f; MAX_OPAQUE_PAYLOAD_LEN + 1];
    let transaction = fixture.execute_transaction(valid_args(1, oversized_payload), &[], &[]);
    let failure = fixture.svm.send_transaction(transaction).unwrap_err();
    assert_log_contains(&failure.meta, "OpaquePayloadTooLarge");
    assert_eq!(fixture.snapshot(), before);
    assert_no_child_cpi(&failure.meta);
}

#[test]
fn every_fixed_envelope_key_is_rejected_as_an_opaque_alias_before_engine_cpi() {
    let mut fixture = Fixture::new();
    let fixed_envelope = [
        ("user", fixture.authority.pubkey(), "OpaqueSignerForbidden"),
        ("market", fixture.market, "OpaqueFixedRoleAlias"),
        ("domain", fixture.domain, "OpaqueFixedRoleAlias"),
        ("fee ledger", fixture.fee_ledger, "OpaqueFixedRoleAlias"),
        ("mint A", fixture.mint_a, "OpaqueFixedRoleAlias"),
        ("mint B", fixture.mint_b, "OpaqueFixedRoleAlias"),
        (
            "user source A",
            fixture.user_source_a,
            "OpaqueFixedRoleAlias",
        ),
        (
            "user destination B",
            fixture.user_destination_b,
            "OpaqueFixedRoleAlias",
        ),
        ("vault A", fixture.vault_a, "OpaqueFixedRoleAlias"),
        ("vault B", fixture.vault_b, "OpaqueFixedRoleAlias"),
        ("fee vault", fixture.fee_vault, "OpaqueFixedRoleAlias"),
        (
            "engine program",
            generated_plan_engine::ID,
            "OpaqueFixedRoleAlias",
        ),
        ("engine state", fixture.engine_state, "OpaqueFixedRoleAlias"),
        (
            "instructions sysvar",
            INSTRUCTIONS_SYSVAR_ID,
            "OpaqueFixedRoleAlias",
        ),
        (
            "token program",
            litesvm_token::TOKEN_ID,
            "OpaqueFixedRoleAlias",
        ),
        (
            "Core program",
            programmable_generated_settlement_core::ID,
            "OpaqueFixedRoleAlias",
        ),
    ];
    let before = fixture.snapshot();

    let representative_positions = [0_usize, MAX_OPAQUE_ACCOUNTS / 2, MAX_OPAQUE_ACCOUNTS - 1];
    for (case_index, (label, key, expected_error)) in fixed_envelope.into_iter().enumerate() {
        let opaque_position = representative_positions[case_index % representative_positions.len()];
        let mut opaque_accounts = vec![
            AccountMeta::new_readonly(opaque_capability_probe::ID, false);
            MAX_OPAQUE_ACCOUNTS
        ];
        opaque_accounts[opaque_position] = AccountMeta::new_readonly(key, false);
        let transaction =
            fixture.execute_transaction(valid_args(1, Vec::new()), &opaque_accounts, &[]);
        let failure = fixture.svm.send_transaction(transaction).unwrap_err();

        assert_log_contains(&failure.meta, expected_error);
        assert_eq!(
            fixture.snapshot(),
            before,
            "fixed alias case {label} at opaque position {opaque_position}"
        );
        assert_no_child_cpi(&failure.meta);
    }
}

#[test]
fn engine_evaluate_rejects_top_level_invocation_without_mutation() {
    let mut fixture = Fixture::new();
    let engine_state_account = fixture.svm.get_account(&fixture.engine_state).unwrap();
    let descriptors = [
        CapabilityDescriptor {
            key: fixture.engine_state,
            owner: engine_state_account.owner,
            is_writable: true,
            is_signer: false,
            is_executable: engine_state_account.executable,
        },
        CapabilityDescriptor {
            key: INSTRUCTIONS_SYSVAR_ID,
            owner: Pubkey::from_str_const("Sysvar1111111111111111111111111111111111111"),
            is_writable: false,
            is_signer: false,
            is_executable: false,
        },
    ];
    let capability_hash =
        compute_capability_hash(&generated_plan_engine::ID, &descriptors).unwrap();
    let request = EngineRequest::new(
        [0x51; 32],
        fixture.market,
        fixture.domain,
        ENGINE_REVISION,
        TRADE_INPUT,
        INITIAL_LIQUIDITY,
        INITIAL_LIQUIDITY,
        0,
        capability_hash,
        &[],
    )
    .unwrap();
    let wire_request = encode_request(&request).unwrap();
    assert_eq!(ENGINE_REQUEST_LEN, 293);
    assert_eq!(wire_request.len(), ENGINE_REQUEST_LEN);
    assert_eq!(decode_request(&wire_request).unwrap(), request);

    let instruction = Instruction {
        program_id: generated_plan_engine::ID,
        accounts: engine_accounts::Evaluate {
            engine_state: fixture.engine_state,
            instructions_sysvar: INSTRUCTIONS_SYSVAR_ID,
        }
        .to_account_metas(None),
        data: engine_instruction::Evaluate { wire_request }.data(),
    };
    let transaction = Transaction::new(
        &[&fixture.authority],
        Message::new(&[instruction], Some(&fixture.authority.pubkey())),
        fixture.svm.latest_blockhash(),
    );
    let packet_bytes = wincode::serialize(&transaction).unwrap().len();
    let before = fixture.snapshot();
    let failure = fixture.svm.send_transaction(transaction).unwrap_err();

    assert_log_contains(&failure.meta, "InvalidInvocationDepth");
    assert_eq!(fixture.snapshot(), before);
    let roots = failure.meta.cpi_tree();
    assert_eq!(roots.len(), 1, "{}", failure.meta.pretty_cpi_tree());
    assert_eq!(roots[0].program_id, generated_plan_engine::ID);
    assert!(roots[0].children.is_empty());
    eprintln!(
        "top-level engine rejection metrics: {packet_bytes} bytes, {} CU",
        failure.meta.compute_units_consumed
    );
}

#[allow(clippy::too_many_arguments)]
fn deposit(
    svm: &mut LiteSVM,
    authority: &Keypair,
    market: Pubkey,
    domain: Pubkey,
    mint: Pubkey,
    provider_source: Pubkey,
    domain_vault: Pubkey,
    asset_index: u8,
) {
    let instruction = Instruction {
        program_id: programmable_generated_settlement_core::ID,
        accounts: core_accounts::DepositV0 {
            provider: authority.pubkey(),
            market,
            domain,
            mint,
            provider_source,
            domain_vault,
            token_program: litesvm_token::TOKEN_ID,
        }
        .to_account_metas(None),
        data: core_instruction::Deposit {
            args: DepositV0Args {
                asset_index,
                amount: INITIAL_LIQUIDITY,
            },
        }
        .data(),
    };
    must_send(svm, authority, instruction, &[]);
}

fn valid_args(
    min_output_credit: u64,
    opaque_payload: Vec<u8>,
) -> ExecuteEngineGeneratedProbeV0Args {
    ExecuteEngineGeneratedProbeV0Args {
        amount_in: TRADE_INPUT,
        max_total_input_debit: TRADE_INPUT + CORE_FEE,
        min_output_credit,
        max_protocol_fee: CORE_FEE,
        expires_at_slot: u64::MAX,
        expected_capability_hash: [0; 32],
        opaque_payload,
    }
}

fn existing_readonly_external_accounts(fixture: &mut Fixture, count: usize) -> Vec<AccountMeta> {
    (0..count)
        .map(|_| {
            let key = Pubkey::new_unique();
            fixture.svm.airdrop(&key, LAMPORTS_PER_SOL).unwrap();
            assert!(fixture.svm.get_account(&key).is_some());
            AccountMeta::new_readonly(key, false)
        })
        .collect()
}

fn protocol_fee(amount: u64) -> u64 {
    let numerator = u128::from(amount) * u128::from(PROTOCOL_FEE_BPS_V0);
    numerator.div_ceil(10_000).try_into().unwrap()
}

fn must_send(
    svm: &mut LiteSVM,
    payer: &Keypair,
    instruction: Instruction,
    additional_signers: &[&Keypair],
) -> TransactionMetadata {
    let mut signers = Vec::with_capacity(additional_signers.len() + 1);
    signers.push(payer);
    signers.extend_from_slice(additional_signers);
    let transaction = Transaction::new(
        &signers,
        Message::new(&[instruction], Some(&payer.pubkey())),
        svm.latest_blockhash(),
    );
    send_success(svm, transaction, "fixture instruction")
}

fn send_success(svm: &mut LiteSVM, transaction: Transaction, label: &str) -> TransactionMetadata {
    svm.send_transaction(transaction).unwrap_or_else(|failure| {
        panic!(
            "{label} failed: {:?}\n{}\n{}",
            failure.err,
            failure.meta.pretty_logs(),
            failure.meta.pretty_cpi_tree(),
        )
    })
}

fn load_program(svm: &mut LiteSVM, program_id: Pubkey, file_name: &str) {
    let path = program_artifact(file_name);
    assert!(
        path.is_file(),
        "missing {}; run `./scripts/build-sbf.sh` before this test",
        path.display()
    );
    svm.add_program_from_file(program_id, path).unwrap();
}

fn program_artifact(file_name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/deploy")
        .join(file_name)
}

fn writable_account_count(message: &Message) -> usize {
    let required_signatures = usize::from(message.header.num_required_signatures);
    let writable_signers =
        required_signatures - usize::from(message.header.num_readonly_signed_accounts);
    let unsigned_accounts = message.account_keys.len() - required_signatures;
    let writable_unsigned =
        unsigned_accounts - usize::from(message.header.num_readonly_unsigned_accounts);
    writable_signers + writable_unsigned
}

fn assert_execution_shape(
    metadata: &TransactionMetadata,
    message_keys: &[Pubkey],
    engine_state: Pubkey,
    opaque_keys: &[Pubkey],
    expect_helper_cpi: bool,
) {
    let roots = metadata.cpi_tree();
    assert_eq!(roots.len(), 1, "{}", metadata.pretty_cpi_tree());
    let root = &roots[0];
    assert_eq!(root.program_id, programmable_generated_settlement_core::ID);
    assert_eq!(frame_depth(root), if expect_helper_cpi { 3 } else { 2 });
    assert_eq!(frame_count(root), if expect_helper_cpi { 6 } else { 5 });
    assert_eq!(root.children.len(), 4, "{}", metadata.pretty_cpi_tree());
    assert_eq!(root.children[0].program_id, generated_plan_engine::ID);
    assert!(root.children[1..]
        .iter()
        .all(|frame| frame.program_id == litesvm_token::TOKEN_ID));
    if expect_helper_cpi {
        assert_eq!(root.children[0].children.len(), 1);
        assert_eq!(
            root.children[0].children[0].program_id,
            opaque_capability_probe::ID
        );
    } else {
        assert!(root.children[0].children.is_empty());
    }

    let inner = metadata.inner_instructions.first().unwrap();
    let engine_call = inner
        .iter()
        .find(|entry| {
            message_keys[usize::from(entry.instruction.program_id_index)]
                == generated_plan_engine::ID
                && entry.stack_height == 2
        })
        .expect("missing generated engine CPI");
    let engine_accounts: Vec<_> = engine_call
        .instruction
        .accounts
        .iter()
        .map(|index| message_keys[usize::from(*index)])
        .collect();
    let mut expected_engine_accounts = vec![engine_state, INSTRUCTIONS_SYSVAR_ID];
    expected_engine_accounts.extend_from_slice(opaque_keys);
    assert_eq!(engine_accounts, expected_engine_accounts);

    if expect_helper_cpi {
        let helper_call = inner
            .iter()
            .find(|entry| {
                message_keys[usize::from(entry.instruction.program_id_index)]
                    == opaque_capability_probe::ID
                    && entry.stack_height == 3
            })
            .expect("missing opaque helper CPI");
        let helper_accounts: Vec<_> = helper_call
            .instruction
            .accounts
            .iter()
            .map(|index| message_keys[usize::from(*index)])
            .collect();
        assert_eq!(helper_accounts, [opaque_keys[1], opaque_keys[2]]);
    }
}

fn assert_no_child_cpi(metadata: &TransactionMetadata) {
    let roots = metadata.cpi_tree();
    assert_eq!(roots.len(), 1, "{}", metadata.pretty_cpi_tree());
    assert_eq!(
        roots[0].program_id,
        programmable_generated_settlement_core::ID
    );
    assert!(
        roots[0].children.is_empty(),
        "failure reached a CPI:\n{}",
        metadata.pretty_cpi_tree()
    );
}

fn assert_engine_and_helper_reached_without_token_cpi(metadata: &TransactionMetadata) {
    assert_engine_reached_without_token_cpi(metadata, true);
}

fn assert_engine_reached_without_token_cpi(
    metadata: &TransactionMetadata,
    expect_helper_cpi: bool,
) {
    let roots = metadata.cpi_tree();
    assert_eq!(roots.len(), 1, "{}", metadata.pretty_cpi_tree());
    let root = &roots[0];
    assert_eq!(root.program_id, programmable_generated_settlement_core::ID);
    assert!(
        root.children
            .iter()
            .any(|frame| frame.program_id == generated_plan_engine::ID),
        "engine was not reached:\n{}",
        metadata.pretty_cpi_tree()
    );
    let helper_reached = root.children.iter().any(|frame| {
        frame.program_id == generated_plan_engine::ID
            && frame
                .children
                .iter()
                .any(|child| child.program_id == opaque_capability_probe::ID)
    });
    assert_eq!(
        helper_reached,
        expect_helper_cpi,
        "unexpected helper CPI shape:\n{}",
        metadata.pretty_cpi_tree()
    );
    assert!(!root
        .children
        .iter()
        .any(|frame| frame.program_id == litesvm_token::TOKEN_ID));
}

fn frame_count(frame: &CpiFrame) -> usize {
    1 + frame.children.iter().map(frame_count).sum::<usize>()
}

fn frame_depth(frame: &CpiFrame) -> usize {
    1 + frame.children.iter().map(frame_depth).max().unwrap_or(0)
}

fn assert_log_contains(metadata: &TransactionMetadata, expected: &str) {
    assert!(
        metadata.logs.iter().any(|line| line.contains(expected)),
        "missing `{expected}` in transaction logs:\n{}",
        metadata.pretty_logs()
    );
}

fn assert_engine_receipt_log(metadata: &TransactionMetadata) {
    let prefix = format!("Program return: {} ", generated_plan_engine::ID);
    let encoded = metadata
        .logs
        .iter()
        .find_map(|line| line.strip_prefix(&prefix))
        .expect("missing generated engine return-data log");
    let receipt = BASE64_STANDARD
        .decode(encoded)
        .expect("engine return-data log is not valid base64");
    assert_eq!(receipt.len(), ENGINE_RECEIPT_LEN);
}
