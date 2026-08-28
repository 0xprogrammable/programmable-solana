use std::path::{Path, PathBuf};

use anchor_lang::{prelude::Pubkey, AccountDeserialize, InstructionData, ToAccountMetas};
use engine_probe_interface::{encode_request, EngineRequest};
use litesvm::{types::TransactionMetadata, LiteSVM};
use litesvm_cpi_tree::{CpiFrame, CpiTreeExt};
use litesvm_token::{
    get_spl_account, spl_token::state::Account as SplTokenAccount, CreateAssociatedTokenAccount,
    CreateMint, MintTo, Transfer,
};
use programmable_core::{
    accounts as core_accounts,
    constants::{
        ASSET_A_SEED_V0, ASSET_B_INDEX_V0, ASSET_B_SEED_V0, DOMAIN_SEED_V0, FEE_LEDGER_SEED_V0,
        FEE_VAULT_SEED_V0, INSTRUCTIONS_SYSVAR_ID, MARKET_SEED_V0, PROTOCOL_FEE_BPS_V0,
        VAULT_SEED_V0,
    },
    instruction as core_instruction, DepositV0Args, DomainV0, ExecuteEngineProbeV0Args,
    FeeLedgerV0, InitializeMarketDomainV0Args, MarketV0,
};
use programmable_spike_engine::{
    accounts as engine_accounts, instruction as engine_instruction, EngineState,
    MODE_HOSTILE_READONLY_ESCALATION, MODE_MALFORMED_RECEIPT, MODE_MISSING_RECEIPT,
    MODE_WRONG_PLAN_HASH,
};
use solana_keypair::Keypair;
use solana_message::{Instruction, Message};
use solana_native_token::LAMPORTS_PER_SOL;
use solana_signer::Signer;
use solana_transaction::Transaction;

const MARKET_ID: [u8; 32] = [0x42; 32];
const VICTIM_MARKET_ID: [u8; 32] = [0x24; 32];
const ENGINE_REVISION: u64 = 1;
const TOKEN_DECIMALS: u8 = 6;
const INITIAL_A: u64 = 2_000_000;
const INITIAL_B: u64 = 2_000_000;
const DEPOSIT_B: u64 = 1_000_000;
const LEGACY_PACKET_LIMIT: usize = 1_232;
const EXECUTE_PACKET_CEILING: usize = 900;
const EXECUTE_CU_CEILING: u64 = 200_000;
const EXECUTE_ACCOUNT_COUNT: usize = 16;
const EXECUTE_WRITABLE_ACCOUNT_COUNT: usize = 9;

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
    victim: Option<MarketInstance>,
}

#[derive(Clone, Copy)]
struct MarketInstance {
    market: Pubkey,
    domain: Pubkey,
    fee_ledger: Pubkey,
    vault_a: Pubkey,
    vault_b: Pubkey,
    fee_vault: Pubkey,
    engine_state: Pubkey,
}

#[derive(Default)]
struct ExecuteAccountOverrides {
    domain: Option<Pubkey>,
    fee_ledger: Option<Pubkey>,
    user_destination_b: Option<Pubkey>,
    vault_a: Option<Pubkey>,
    vault_b: Option<Pubkey>,
    fee_vault: Option<Pubkey>,
    engine_state: Option<Pubkey>,
}

#[derive(Clone, Copy, Debug)]
enum CrossMarketSubstitution {
    Domain,
    FeeLedger,
    VaultA,
    VaultB,
    FeeVault,
    EngineState,
}

impl CrossMarketSubstitution {
    const CASES: [Self; 6] = [
        Self::Domain,
        Self::FeeLedger,
        Self::VaultA,
        Self::VaultB,
        Self::FeeVault,
        Self::EngineState,
    ];

    fn expected_error(self) -> &'static str {
        match self {
            Self::Domain | Self::FeeLedger | Self::VaultA | Self::VaultB | Self::FeeVault => {
                "ConstraintSeeds"
            }
            Self::EngineState => "InvalidEngineState",
        }
    }

    fn override_with(self, victim: MarketInstance) -> ExecuteAccountOverrides {
        let mut overrides = ExecuteAccountOverrides::default();
        match self {
            Self::Domain => overrides.domain = Some(victim.domain),
            Self::FeeLedger => overrides.fee_ledger = Some(victim.fee_ledger),
            Self::VaultA => overrides.vault_a = Some(victim.vault_a),
            Self::VaultB => overrides.vault_b = Some(victim.vault_b),
            Self::FeeVault => overrides.fee_vault = Some(victim.fee_vault),
            Self::EngineState => overrides.engine_state = Some(victim.engine_state),
        }
        overrides
    }
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
        let mut svm = LiteSVM::new();
        load_program(&mut svm, programmable_core::ID, "programmable_core.so");
        load_program(
            &mut svm,
            programmable_spike_engine::ID,
            "programmable_spike_engine.so",
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

        MintTo::new(&mut svm, &authority, &mint_a, &user_source_a, INITIAL_A)
            .send()
            .unwrap();
        MintTo::new(&mut svm, &authority, &mint_b, &provider_source_b, INITIAL_B)
            .send()
            .unwrap();

        let authority_key = authority.pubkey();
        let (market, _) = Pubkey::find_program_address(
            &[MARKET_SEED_V0, authority_key.as_ref(), &MARKET_ID],
            &programmable_core::ID,
        );
        let (domain, _) = Pubkey::find_program_address(
            &[DOMAIN_SEED_V0, market.as_ref()],
            &programmable_core::ID,
        );
        let (fee_ledger, _) = Pubkey::find_program_address(
            &[FEE_LEDGER_SEED_V0, market.as_ref(), mint_a.as_ref()],
            &programmable_core::ID,
        );
        let (vault_a, _) = Pubkey::find_program_address(
            &[VAULT_SEED_V0, domain.as_ref(), ASSET_A_SEED_V0],
            &programmable_core::ID,
        );
        let (vault_b, _) = Pubkey::find_program_address(
            &[VAULT_SEED_V0, domain.as_ref(), ASSET_B_SEED_V0],
            &programmable_core::ID,
        );
        let (fee_vault, _) = Pubkey::find_program_address(
            &[FEE_VAULT_SEED_V0, fee_ledger.as_ref()],
            &programmable_core::ID,
        );

        let engine_state_keypair = Keypair::new();
        let engine_state = engine_state_keypair.pubkey();
        let initialize_engine = Instruction {
            program_id: programmable_spike_engine::ID,
            accounts: engine_accounts::Initialize {
                engine_state,
                authority: authority_key,
                system_program: anchor_lang::system_program::ID,
            }
            .to_account_metas(None),
            data: engine_instruction::Initialize {
                market,
                revision: ENGINE_REVISION,
            }
            .data(),
        };
        must_send(
            &mut svm,
            &authority,
            initialize_engine,
            &[&engine_state_keypair],
        );

        let initialize_core = Instruction {
            program_id: programmable_core::ID,
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
                engine_program: programmable_spike_engine::ID,
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

        let deposit = Instruction {
            program_id: programmable_core::ID,
            accounts: core_accounts::DepositV0 {
                provider: authority_key,
                market,
                domain,
                mint: mint_b,
                provider_source: provider_source_b,
                domain_vault: vault_b,
                token_program: litesvm_token::TOKEN_ID,
            }
            .to_account_metas(None),
            data: core_instruction::Deposit {
                args: DepositV0Args {
                    asset_index: ASSET_B_INDEX_V0,
                    amount: DEPOSIT_B,
                },
            }
            .data(),
        };
        must_send(&mut svm, &authority, deposit, &[]);

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
            victim: None,
        };

        let domain_state: DomainV0 = fixture.read_anchor(fixture.domain);
        assert_eq!(domain_state.accounted_a, 0);
        assert_eq!(domain_state.accounted_b, DEPOSIT_B);
        assert_eq!(fixture.token_balance(fixture.vault_a), 0);
        assert_eq!(fixture.token_balance(fixture.vault_b), DEPOSIT_B);
        fixture
    }

    fn new_two_markets() -> Self {
        let mut fixture = Self::new();
        let victim = initialize_market_instance(
            &mut fixture.svm,
            &fixture.authority,
            fixture.mint_a,
            fixture.mint_b,
            fixture.provider_source_b,
            VICTIM_MARKET_ID,
        );

        let victim_domain: DomainV0 = fixture.read_anchor(victim.domain);
        assert_eq!(victim_domain.accounted_a, 0);
        assert_eq!(victim_domain.accounted_b, DEPOSIT_B);
        assert_eq!(fixture.token_balance(victim.vault_a), 0);
        assert_eq!(fixture.token_balance(victim.vault_b), DEPOSIT_B);
        fixture.victim = Some(victim);
        fixture
    }

    fn victim(&self) -> MarketInstance {
        self.victim.expect("two-market fixture has a victim market")
    }

    fn execute_instruction(&self, args: ExecuteEngineProbeV0Args) -> Instruction {
        self.execute_instruction_with_overrides(args, ExecuteAccountOverrides::default())
    }

    fn execute_instruction_with_overrides(
        &self,
        args: ExecuteEngineProbeV0Args,
        overrides: ExecuteAccountOverrides,
    ) -> Instruction {
        Instruction {
            program_id: programmable_core::ID,
            accounts: core_accounts::ExecuteEngineProbeV0 {
                user: self.authority.pubkey(),
                market: self.market,
                domain: overrides.domain.unwrap_or(self.domain),
                fee_ledger: overrides.fee_ledger.unwrap_or(self.fee_ledger),
                mint_a: self.mint_a,
                mint_b: self.mint_b,
                user_source_a: self.user_source_a,
                user_destination_b: overrides
                    .user_destination_b
                    .unwrap_or(self.user_destination_b),
                vault_a: overrides.vault_a.unwrap_or(self.vault_a),
                vault_b: overrides.vault_b.unwrap_or(self.vault_b),
                fee_vault: overrides.fee_vault.unwrap_or(self.fee_vault),
                engine_program: programmable_spike_engine::ID,
                engine_state: overrides.engine_state.unwrap_or(self.engine_state),
                instructions_sysvar: INSTRUCTIONS_SYSVAR_ID,
                token_program: litesvm_token::TOKEN_ID,
            }
            .to_account_metas(None),
            data: core_instruction::ExecuteEngineProbe { args }.data(),
        }
    }

    fn execute_transaction(&self, args: ExecuteEngineProbeV0Args) -> Transaction {
        Transaction::new(
            &[&self.authority],
            Message::new(
                &[self.execute_instruction(args)],
                Some(&self.authority.pubkey()),
            ),
            self.svm.latest_blockhash(),
        )
    }

    fn execute_transaction_with_overrides(
        &self,
        args: ExecuteEngineProbeV0Args,
        overrides: ExecuteAccountOverrides,
    ) -> Transaction {
        Transaction::new(
            &[&self.authority],
            Message::new(
                &[self.execute_instruction_with_overrides(args, overrides)],
                Some(&self.authority.pubkey()),
            ),
            self.svm.latest_blockhash(),
        )
    }

    fn set_engine_mode(&mut self, mode: u8) {
        let instruction = Instruction {
            program_id: programmable_spike_engine::ID,
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
        let addresses = [
            self.market,
            self.domain,
            self.fee_ledger,
            self.engine_state,
            self.user_source_a,
            self.user_destination_b,
            self.vault_a,
            self.vault_b,
            self.fee_vault,
        ];
        EconomicSnapshot {
            accounts: addresses
                .iter()
                .map(|address| self.account_snapshot(*address))
                .collect(),
        }
    }

    fn two_market_snapshot(&self) -> EconomicSnapshot {
        let victim = self.victim();
        let addresses = [
            self.market,
            self.domain,
            self.fee_ledger,
            self.engine_state,
            victim.market,
            victim.domain,
            victim.fee_ledger,
            victim.engine_state,
            self.mint_a,
            self.mint_b,
            self.user_source_a,
            self.provider_source_b,
            self.user_destination_b,
            self.vault_a,
            self.vault_b,
            self.fee_vault,
            victim.vault_a,
            victim.vault_b,
            victim.fee_vault,
        ];
        EconomicSnapshot {
            accounts: addresses
                .iter()
                .map(|address| self.account_snapshot(*address))
                .collect(),
        }
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

fn initialize_market_instance(
    svm: &mut LiteSVM,
    authority: &Keypair,
    mint_a: Pubkey,
    mint_b: Pubkey,
    provider_source_b: Pubkey,
    market_id: [u8; 32],
) -> MarketInstance {
    let authority_key = authority.pubkey();
    let (market, _) = Pubkey::find_program_address(
        &[MARKET_SEED_V0, authority_key.as_ref(), &market_id],
        &programmable_core::ID,
    );
    let (domain, _) =
        Pubkey::find_program_address(&[DOMAIN_SEED_V0, market.as_ref()], &programmable_core::ID);
    let (fee_ledger, _) = Pubkey::find_program_address(
        &[FEE_LEDGER_SEED_V0, market.as_ref(), mint_a.as_ref()],
        &programmable_core::ID,
    );
    let (vault_a, _) = Pubkey::find_program_address(
        &[VAULT_SEED_V0, domain.as_ref(), ASSET_A_SEED_V0],
        &programmable_core::ID,
    );
    let (vault_b, _) = Pubkey::find_program_address(
        &[VAULT_SEED_V0, domain.as_ref(), ASSET_B_SEED_V0],
        &programmable_core::ID,
    );
    let (fee_vault, _) = Pubkey::find_program_address(
        &[FEE_VAULT_SEED_V0, fee_ledger.as_ref()],
        &programmable_core::ID,
    );

    let engine_state_keypair = Keypair::new();
    let engine_state = engine_state_keypair.pubkey();
    let initialize_engine = Instruction {
        program_id: programmable_spike_engine::ID,
        accounts: engine_accounts::Initialize {
            engine_state,
            authority: authority_key,
            system_program: anchor_lang::system_program::ID,
        }
        .to_account_metas(None),
        data: engine_instruction::Initialize {
            market,
            revision: ENGINE_REVISION,
        }
        .data(),
    };
    must_send(svm, authority, initialize_engine, &[&engine_state_keypair]);

    let initialize_core = Instruction {
        program_id: programmable_core::ID,
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
            engine_program: programmable_spike_engine::ID,
            engine_state,
            token_program: litesvm_token::TOKEN_ID,
            system_program: anchor_lang::system_program::ID,
        }
        .to_account_metas(None),
        data: core_instruction::InitializeMarketDomain {
            args: InitializeMarketDomainV0Args {
                market_id,
                engine_revision: ENGINE_REVISION,
            },
        }
        .data(),
    };
    must_send(svm, authority, initialize_core, &[]);

    let deposit = Instruction {
        program_id: programmable_core::ID,
        accounts: core_accounts::DepositV0 {
            provider: authority_key,
            market,
            domain,
            mint: mint_b,
            provider_source: provider_source_b,
            domain_vault: vault_b,
            token_program: litesvm_token::TOKEN_ID,
        }
        .to_account_metas(None),
        data: core_instruction::Deposit {
            args: DepositV0Args {
                asset_index: ASSET_B_INDEX_V0,
                amount: DEPOSIT_B,
            },
        }
        .data(),
    };
    must_send(svm, authority, deposit, &[]);

    MarketInstance {
        market,
        domain,
        fee_ledger,
        vault_a,
        vault_b,
        fee_vault,
        engine_state,
    }
}

#[test]
fn happy_path_proves_exact_accounting_and_bounded_execution_shape() {
    let mut fixture = Fixture::new();
    let amount_in = 100_001;
    let amount_out = 250_000;
    let fee = protocol_fee(amount_in);
    assert_eq!(fee, 301);

    let transaction = fixture.execute_transaction(valid_args(amount_in, amount_out));
    let packet_bytes = wincode::serialize(&transaction).unwrap().len();
    let account_count = transaction.message.account_keys.len();
    let writable_account_count = writable_account_count(&transaction.message);
    let message_keys = transaction.message.account_keys.clone();

    assert!(packet_bytes < LEGACY_PACKET_LIMIT);
    assert!(packet_bytes <= EXECUTE_PACKET_CEILING);
    assert_eq!(account_count, EXECUTE_ACCOUNT_COUNT);
    assert_eq!(writable_account_count, EXECUTE_WRITABLE_ACCOUNT_COUNT);

    let metadata = fixture
        .svm
        .send_transaction(transaction)
        .unwrap_or_else(|failure| {
            panic!(
                "happy-path transaction failed: {:?}\n{}\n{}",
                failure.err,
                failure.meta.pretty_logs(),
                failure.meta.pretty_cpi_tree(),
            )
        });

    assert!(metadata.compute_units_consumed <= EXECUTE_CU_CEILING);
    assert_engine_cpi_shape(&metadata, &message_keys, fixture.engine_state);

    let market: MarketV0 = fixture.read_anchor(fixture.market);
    let domain: DomainV0 = fixture.read_anchor(fixture.domain);
    let fees: FeeLedgerV0 = fixture.read_anchor(fixture.fee_ledger);
    let engine: EngineState = fixture.read_anchor(fixture.engine_state);

    assert_eq!(market.fee_bps, PROTOCOL_FEE_BPS_V0);
    assert_eq!(domain.accounted_a, amount_in);
    assert_eq!(domain.accounted_b, DEPOSIT_B - amount_out);
    assert_eq!(fees.accounted_fee_a, fee);
    assert_eq!(engine.sequence, 1);
    assert_ne!(engine.last_plan_hash, [0; 32]);

    assert_eq!(
        fixture.token_balance(fixture.user_source_a),
        INITIAL_A - amount_in - fee
    );
    assert_eq!(
        fixture.token_balance(fixture.user_destination_b),
        amount_out
    );
    assert_eq!(fixture.token_balance(fixture.vault_a), amount_in);
    assert_eq!(
        fixture.token_balance(fixture.vault_b),
        DEPOSIT_B - amount_out
    );
    assert_eq!(fixture.token_balance(fixture.fee_vault), fee);

    eprintln!(
        "execute metrics: {packet_bytes} bytes, {account_count} accounts, \
         {writable_account_count} writable, {} CU",
        metadata.compute_units_consumed
    );
}

#[test]
fn fee_ceiling_failure_rolls_back_every_economic_account() {
    let mut fixture = Fixture::new();
    let amount_in = 100_001;
    let amount_out = 10_000;
    let fee = protocol_fee(amount_in);
    let before = fixture.snapshot();

    let mut args = valid_args(amount_in, amount_out);
    args.max_protocol_fee = fee - 1;
    let transaction = fixture.execute_transaction(args);
    let failure = fixture.svm.send_transaction(transaction).unwrap_err();

    assert_log_contains(&failure.meta, "ProtocolFeeAboveUserMaximum");
    assert_eq!(fixture.snapshot(), before);
    let engine: EngineState = fixture.read_anchor(fixture.engine_state);
    assert_eq!(engine.sequence, 0);
}

#[test]
fn signed_bound_failures_stop_before_the_engine_and_preserve_state() {
    let mut fixture = Fixture::new();
    fixture.svm.warp_to_slot(1);
    let amount_in = 100_000;
    let amount_out = 10_000;
    let fee = protocol_fee(amount_in);

    let mut expired = valid_args(amount_in, amount_out);
    expired.expires_at_slot = 0;
    let mut output_below_minimum = valid_args(amount_in, amount_out);
    output_below_minimum.min_output_credit = amount_out + 1;
    let mut debit_above_maximum = valid_args(amount_in, amount_out);
    debit_above_maximum.max_total_input_debit = amount_in + fee - 1;

    let cases = [
        ("RequestExpired", expired),
        ("OutputBelowUserMinimum", output_below_minimum),
        ("TotalDebitAboveUserMaximum", debit_above_maximum),
    ];

    for (expected_error, args) in cases {
        let before = fixture.snapshot();
        let transaction = fixture.execute_transaction(args);
        let failure = fixture.svm.send_transaction(transaction).unwrap_err();

        assert_log_contains(&failure.meta, expected_error);
        assert_eq!(fixture.snapshot(), before, "bound case {expected_error}");

        let roots = failure.meta.cpi_tree();
        assert_eq!(roots.len(), 1, "{}", failure.meta.pretty_cpi_tree());
        assert_eq!(roots[0].program_id, programmable_core::ID);
        assert!(
            roots[0].children.is_empty(),
            "bound case {expected_error} reached a CPI:\n{}",
            failure.meta.pretty_cpi_tree()
        );
    }
}

#[test]
fn raw_vault_donation_never_becomes_accounted_or_spendable() {
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

    let domain: DomainV0 = fixture.read_anchor(fixture.domain);
    assert_eq!(domain.accounted_b, DEPOSIT_B);
    assert_eq!(fixture.token_balance(fixture.vault_b), DEPOSIT_B + donation);

    let before = fixture.snapshot();
    let amount_out = DEPOSIT_B + donation / 2;
    let transaction = fixture.execute_transaction(valid_args(100_000, amount_out));
    let failure = fixture.svm.send_transaction(transaction).unwrap_err();

    assert_log_contains(&failure.meta, "InsufficientAccountedLiquidity");
    assert_eq!(fixture.snapshot(), before);
    assert_eq!(fixture.token_balance(fixture.vault_b), DEPOSIT_B + donation);
}

#[test]
fn raw_fee_vault_donation_never_becomes_protocol_liability() {
    let mut fixture = Fixture::new();
    let donation = 123;
    let amount_in = 100_000;
    let amount_out = 10_000;
    let fee = protocol_fee(amount_in);

    Transfer::new(
        &mut fixture.svm,
        &fixture.authority,
        &fixture.mint_a,
        &fixture.fee_vault,
        donation,
    )
    .source(&fixture.user_source_a)
    .send()
    .unwrap();

    let fees_before: FeeLedgerV0 = fixture.read_anchor(fixture.fee_ledger);
    assert_eq!(fees_before.accounted_fee_a, 0);
    assert_eq!(fixture.token_balance(fixture.fee_vault), donation);

    let transaction = fixture.execute_transaction(valid_args(amount_in, amount_out));
    fixture
        .svm
        .send_transaction(transaction)
        .unwrap_or_else(|failure| {
            panic!(
                "fee-donation transaction failed: {:?}\n{}\n{}",
                failure.err,
                failure.meta.pretty_logs(),
                failure.meta.pretty_cpi_tree(),
            )
        });

    let fees_after: FeeLedgerV0 = fixture.read_anchor(fixture.fee_ledger);
    assert_eq!(fees_after.accounted_fee_a, fee);
    assert_eq!(fixture.token_balance(fixture.fee_vault), donation + fee);
    assert_eq!(
        fixture.token_balance(fixture.user_source_a),
        INITIAL_A - donation - amount_in - fee
    );
}

#[test]
fn hostile_engine_mutation_and_privilege_escalation_fully_roll_back() {
    let mut fixture = Fixture::new();
    fixture.set_engine_mode(MODE_HOSTILE_READONLY_ESCALATION);
    let before = fixture.snapshot();

    let transaction = fixture.execute_transaction(valid_args(100_000, 10_000));
    let failure = fixture.svm.send_transaction(transaction).unwrap_err();

    assert!(
        failure.meta.logs.iter().any(|line| {
            line.contains("writable privilege escalated")
                || line.contains("unauthorized signer or writable account")
        }),
        "missing runtime privilege-escalation evidence:\n{}",
        failure.meta.pretty_logs()
    );
    assert_eq!(fixture.snapshot(), before);

    let engine: EngineState = fixture.read_anchor(fixture.engine_state);
    assert_eq!(engine.sequence, 0);
    assert_eq!(engine.last_plan_hash, [0; 32]);
    assert_eq!(engine.mode, MODE_HOSTILE_READONLY_ESCALATION);

    let roots = failure.meta.cpi_tree();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].program_id, programmable_core::ID);
    assert!(
        roots[0]
            .children
            .iter()
            .any(|frame| frame.program_id == programmable_spike_engine::ID),
        "hostile engine was not reached:\n{}",
        failure.meta.pretty_cpi_tree()
    );
}

#[test]
fn accepted_engine_and_first_transfer_roll_back_when_fee_transfer_fails() {
    let mut fixture = Fixture::new();
    let amount_in = INITIAL_A;
    let amount_out = 10_000;
    let before = fixture.snapshot();

    let transaction = fixture.execute_transaction(valid_args(amount_in, amount_out));
    let failure = fixture.svm.send_transaction(transaction).unwrap_err();

    assert!(
        failure
            .meta
            .logs
            .iter()
            .any(|line| line.contains("insufficient funds")),
        "missing failed fee-transfer evidence:\n{}",
        failure.meta.pretty_logs()
    );
    assert_eq!(fixture.snapshot(), before);

    let engine: EngineState = fixture.read_anchor(fixture.engine_state);
    assert_eq!(engine.sequence, 0);
    assert_eq!(engine.last_plan_hash, [0; 32]);

    let roots = failure.meta.cpi_tree();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].program_id, programmable_core::ID);
    assert!(
        roots[0]
            .children
            .iter()
            .any(|frame| frame.program_id == programmable_spike_engine::ID),
        "engine was not reached:\n{}",
        failure.meta.pretty_cpi_tree()
    );
    assert_eq!(
        roots[0]
            .children
            .iter()
            .filter(|frame| frame.program_id == litesvm_token::TOKEN_ID)
            .count(),
        2,
        "the successful input transfer and failed fee transfer were not both reached:\n{}",
        failure.meta.pretty_cpi_tree()
    );
}

#[test]
fn cross_market_account_substitutions_fail_before_settlement_and_preserve_both_markets() {
    let mut fixture = Fixture::new_two_markets();
    let victim = fixture.victim();
    let initial = fixture.two_market_snapshot();
    let mut rejected_cases = 0;

    for substitution in CrossMarketSubstitution::CASES {
        let before = fixture.two_market_snapshot();
        assert_eq!(before, initial, "dirty setup for {substitution:?}");

        let transaction = fixture.execute_transaction_with_overrides(
            valid_args(100_000, 10_000),
            substitution.override_with(victim),
        );
        let failure = fixture.svm.send_transaction(transaction).unwrap_err();

        assert!(
            failure
                .meta
                .logs
                .iter()
                .any(|line| line.contains(substitution.expected_error())),
            "missing exact {:?} rejection `{}`:\n{}",
            substitution,
            substitution.expected_error(),
            failure.meta.pretty_logs()
        );
        assert_eq!(
            fixture.two_market_snapshot(),
            before,
            "{substitution:?} mutated a market, engine state, mint, or token account"
        );

        let roots = failure.meta.cpi_tree();
        assert_eq!(
            roots.len(),
            1,
            "{substitution:?}:\n{}",
            failure.meta.pretty_cpi_tree()
        );
        assert_eq!(roots[0].program_id, programmable_core::ID);
        assert!(
            roots[0].children.is_empty(),
            "{substitution:?} reached a CPI before rejection:\n{}",
            failure.meta.pretty_cpi_tree()
        );
        rejected_cases += 1;
    }

    let attacker_engine: EngineState = fixture.read_anchor(fixture.engine_state);
    let victim_engine: EngineState = fixture.read_anchor(victim.engine_state);
    assert_eq!(attacker_engine.sequence, 0);
    assert_eq!(attacker_engine.last_plan_hash, [0; 32]);
    assert_eq!(victim_engine.sequence, 0);
    assert_eq!(victim_engine.last_plan_hash, [0; 32]);
    assert_eq!(rejected_cases, CrossMarketSubstitution::CASES.len());

    eprintln!(
        "cross-market isolation: {rejected_cases} substitutions rejected, 0 engine CPIs, 0 token CPIs"
    );
}

#[test]
fn cross_market_recipient_credit_is_only_an_unaccounted_donation() {
    let mut fixture = Fixture::new_two_markets();
    let victim = fixture.victim();
    let amount_in = 100_000;
    let amount_out = 10_000;

    let victim_market_before = fixture.account_snapshot(victim.market);
    let victim_domain_before = fixture.account_snapshot(victim.domain);
    let victim_fee_ledger_before = fixture.account_snapshot(victim.fee_ledger);
    let victim_engine_before = fixture.account_snapshot(victim.engine_state);
    let victim_vault_a_before = fixture.account_snapshot(victim.vault_a);
    let victim_fee_vault_before = fixture.account_snapshot(victim.fee_vault);
    let victim_raw_b_before = fixture.token_balance(victim.vault_b);

    let transaction = fixture.execute_transaction_with_overrides(
        valid_args(amount_in, amount_out),
        ExecuteAccountOverrides {
            user_destination_b: Some(victim.vault_b),
            ..ExecuteAccountOverrides::default()
        },
    );
    fixture
        .svm
        .send_transaction(transaction)
        .unwrap_or_else(|failure| {
            panic!(
                "cross-market donation transaction failed: {:?}\n{}\n{}",
                failure.err,
                failure.meta.pretty_logs(),
                failure.meta.pretty_cpi_tree(),
            )
        });

    let attacker_domain: DomainV0 = fixture.read_anchor(fixture.domain);
    let victim_domain: DomainV0 = fixture.read_anchor(victim.domain);
    let victim_engine: EngineState = fixture.read_anchor(victim.engine_state);

    assert_eq!(attacker_domain.accounted_b, DEPOSIT_B - amount_out);
    assert_eq!(victim_domain.accounted_b, DEPOSIT_B);
    assert_eq!(
        fixture.token_balance(victim.vault_b),
        victim_raw_b_before + amount_out
    );
    assert_eq!(victim_engine.sequence, 0);
    assert_eq!(victim_engine.last_plan_hash, [0; 32]);
    assert_eq!(
        fixture.account_snapshot(victim.market),
        victim_market_before
    );
    assert_eq!(
        fixture.account_snapshot(victim.domain),
        victim_domain_before
    );
    assert_eq!(
        fixture.account_snapshot(victim.fee_ledger),
        victim_fee_ledger_before
    );
    assert_eq!(
        fixture.account_snapshot(victim.engine_state),
        victim_engine_before
    );
    assert_eq!(
        fixture.account_snapshot(victim.vault_a),
        victim_vault_a_before
    );
    assert_eq!(
        fixture.account_snapshot(victim.fee_vault),
        victim_fee_vault_before
    );
}

#[test]
fn invalid_engine_receipts_roll_back_every_economic_account() {
    let cases = [
        (MODE_MISSING_RECEIPT, "MissingEngineReceipt"),
        (MODE_WRONG_PLAN_HASH, "EngineReceiptPlanMismatch"),
        (MODE_MALFORMED_RECEIPT, "InvalidEngineReceipt"),
    ];

    for (mode, expected_error) in cases {
        let mut fixture = Fixture::new();
        fixture.set_engine_mode(mode);
        let before = fixture.snapshot();

        let transaction = fixture.execute_transaction(valid_args(100_000, 10_000));
        let failure = fixture.svm.send_transaction(transaction).unwrap_err();

        assert_log_contains(&failure.meta, expected_error);
        assert_eq!(fixture.snapshot(), before, "receipt mode {mode}");

        let engine: EngineState = fixture.read_anchor(fixture.engine_state);
        assert_eq!(engine.sequence, 0, "receipt mode {mode}");
        assert_eq!(engine.last_plan_hash, [0; 32], "receipt mode {mode}");
        assert_eq!(engine.mode, mode, "receipt mode {mode}");
    }
}

#[test]
fn direct_top_level_engine_invocation_is_rejected_without_mutation() {
    let mut fixture = Fixture::new();
    let before = fixture.snapshot();
    let wire_request = encode_request(&EngineRequest {
        plan_hash: [0x5a; 32],
        market: fixture.market,
        domain: fixture.domain,
        engine_revision: ENGINE_REVISION,
        amount_in: 100_000,
        amount_out: 10_000,
        protocol_fee: protocol_fee(100_000),
        accounted_input_before: 0,
        accounted_output_before: DEPOSIT_B,
        accounted_fee_before: 0,
    });
    let instruction = Instruction {
        program_id: programmable_spike_engine::ID,
        accounts: engine_accounts::Evaluate {
            engine_state: fixture.engine_state,
            instructions: INSTRUCTIONS_SYSVAR_ID,
        }
        .to_account_metas(None),
        data: engine_instruction::Evaluate { wire_request }.data(),
    };
    let transaction = Transaction::new(
        &[&fixture.authority],
        Message::new(&[instruction], Some(&fixture.authority.pubkey())),
        fixture.svm.latest_blockhash(),
    );

    let failure = fixture.svm.send_transaction(transaction).unwrap_err();

    assert_log_contains(&failure.meta, "InvalidInvocationDepth");
    assert_eq!(fixture.snapshot(), before);
    let roots = failure.meta.cpi_tree();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].program_id, programmable_spike_engine::ID);
    assert!(roots[0].children.is_empty());
}

fn valid_args(amount_in: u64, amount_out: u64) -> ExecuteEngineProbeV0Args {
    let fee = protocol_fee(amount_in);
    ExecuteEngineProbeV0Args {
        amount_in,
        amount_out,
        max_total_input_debit: amount_in + fee,
        min_output_credit: amount_out,
        max_protocol_fee: fee,
        expires_at_slot: u64::MAX,
    }
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
    match svm.send_transaction(transaction) {
        Ok(metadata) => metadata,
        Err(failure) => panic!(
            "fixture transaction failed: {:?}\n{}\n{}",
            failure.err,
            failure.meta.pretty_logs(),
            failure.meta.pretty_cpi_tree(),
        ),
    }
}

fn load_program(svm: &mut LiteSVM, program_id: Pubkey, file_name: &str) {
    let path = program_artifact(file_name);
    assert!(
        path.is_file(),
        "missing {}; run `./scripts/build-spike-sbf.sh` before this test",
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

fn assert_engine_cpi_shape(
    metadata: &TransactionMetadata,
    message_keys: &[Pubkey],
    engine_state: Pubkey,
) {
    let roots = metadata.cpi_tree();
    assert_eq!(roots.len(), 1, "{}", metadata.pretty_cpi_tree());
    let root = &roots[0];
    assert_eq!(root.program_id, programmable_core::ID);
    assert_eq!(frame_count(root), 5, "{}", metadata.pretty_cpi_tree());
    assert_eq!(frame_depth(root), 2, "{}", metadata.pretty_cpi_tree());
    assert_eq!(root.children.len(), 4, "{}", metadata.pretty_cpi_tree());
    assert_eq!(root.children[0].program_id, programmable_spike_engine::ID);
    assert!(root.children[1..]
        .iter()
        .all(|frame| frame.program_id == litesvm_token::TOKEN_ID));

    let inner = metadata.inner_instructions.first().unwrap();
    let engine_calls: Vec<_> = inner
        .iter()
        .filter(|entry| {
            message_keys[usize::from(entry.instruction.program_id_index)]
                == programmable_spike_engine::ID
        })
        .collect();
    assert_eq!(engine_calls.len(), 1);
    let engine_call = engine_calls[0];
    assert_eq!(engine_call.stack_height, 2);
    let engine_accounts: Vec<_> = engine_call
        .instruction
        .accounts
        .iter()
        .map(|index| message_keys[usize::from(*index)])
        .collect();
    assert_eq!(engine_accounts, [engine_state, INSTRUCTIONS_SYSVAR_ID]);
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
