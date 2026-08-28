use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anchor_lang::{prelude::Pubkey, AccountDeserialize, InstructionData, ToAccountMetas};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use callback_capability_probe::{
    accounts as helper_accounts, instruction as helper_instruction, HelperState,
};
use hostile_router_probe::{
    accounts as router_accounts, instruction as router_instruction, RouteProbeArgs, RouterMode,
};
use litesvm::{
    types::{FailedTransactionMetadata, TransactionMetadata},
    LiteSVM,
};
use litesvm_cpi_tree::{CpiFrame, CpiTreeExt};
use litesvm_token::{
    get_spl_account,
    spl_token::state::{Account as SplTokenAccount, Mint as SplMint},
    ApproveChecked, CreateAssociatedTokenAccount, MintTo, Revoke,
};
use programmable_routed_callback_core::{
    accounts as core_accounts,
    constants::{
        ASSET_A_INDEX_V0, ASSET_A_SEED_V0, ASSET_B_INDEX_V0, ASSET_B_SEED_V0, DOMAIN_SEED_V0,
        FEE_LEDGER_SEED_V0, FEE_POLICY_REVISION_V0, FEE_VAULT_SEED_V0, MARKET_SEED_V0,
        PROTOCOL_FEE_BPS_V0, SPEND_AUTHORITY_SEED_V0, VAULT_SEED_V0,
    },
    error::CoreError,
    instruction as core_instruction, AuthorizeSpendV0Args, DepositV0Args, DomainV0,
    ExecuteCallbackAuthenticatedProbeV0Args, FeeLedgerV0, InitializeMarketDomainV0Args,
};
use routed_callback_probe_wire::{
    compute_capability_hash, compute_intent_digest, compute_payload_hash, encode_engine_request,
    encode_intent_binding, CapabilityDescriptor, EngineRequest, ExecutionBinding, IntentBinding,
    CALLBACK_AUTHORITY_SEED, ENGINE_INSTRUCTION_LEN, ENGINE_RECEIPT_LEN, MAX_OPAQUE_ACCOUNTS,
    MAX_OPAQUE_PAYLOAD_LEN, PHASE_COMMIT, PHASE_PREPARE, PHASE_TRANSITION, TIMING_PREPARE_COMMIT,
    TIMING_SINGLE,
};
use routed_plan_engine::{
    accounts as engine_accounts, encode_helper_payload, instruction as engine_instruction,
    quote_exact_in, EngineState, MODE_LATE_COMMIT_FAILURE, MODE_MALFORMED_RECEIPT,
    MODE_MISSING_RECEIPT, MODE_OVERSIZED_OUTPUT, MODE_TRAILING_RECEIPT_BYTE,
    MODE_WRONG_EXECUTION_DIGEST, MODE_WRONG_INTENT_DIGEST, MODE_WRONG_RECEIPT_MAGIC,
    MODE_WRONG_RECEIPT_PHASE, MODE_WRONG_RECEIPT_VERSION, MODE_ZERO_OUTPUT,
};
use solana_account::Account;
use solana_address_lookup_table_interface::instruction::{
    create_lookup_table, extend_lookup_table,
};
use solana_keypair::Keypair;
use solana_message::{
    v0::Message as MessageV0, AccountMeta, AddressLookupTableAccount, Instruction, Message,
    VersionedMessage,
};
use solana_native_token::LAMPORTS_PER_SOL;
use solana_program_pack::Pack;
use solana_signer::Signer;
use solana_transaction::{versioned::VersionedTransaction, Transaction};

const MARKET_ID: [u8; 32] = [0x42; 32];
const ENGINE_REVISION: u64 = 1;
const TOKEN_DECIMALS: u8 = 6;
const INITIAL_MINT_AMOUNT: u64 = 2_000_000;
const INITIAL_LIQUIDITY: u64 = 1_000_000;
const TRADE_INPUT: u64 = 100_000;
const ENGINE_LP_FEE_BPS: u16 = 30;
const CORE_FEE: u64 = 300;
const MIN_OUTPUT: u64 = 90_000;
const HELPER_INCREMENT: u64 = 7;
const PACKET_LIMIT: usize = 1_232;
const CU_LIMIT: u64 = 250_000;

fn fixture_keypair(tag: u8) -> Keypair {
    // Public, valueless test identities only. No fixture key is a deployment
    // authority or accepted by any cluster program.
    Keypair::new_from_array([tag; Keypair::SECRET_KEY_LENGTH])
}

fn install_fixture_mint(
    svm: &mut LiteSVM,
    address_tag: u8,
    authority: Pubkey,
    decimals: u8,
) -> Pubkey {
    let address = Pubkey::new_from_array([address_tag; 32]);
    let mut data = vec![0; SplMint::LEN];
    SplMint::pack(
        SplMint {
            mint_authority: Some(authority).into(),
            supply: 0,
            decimals,
            is_initialized: true,
            freeze_authority: None.into(),
        },
        &mut data,
    )
    .unwrap();
    svm.set_account(
        address,
        Account {
            lamports: svm.minimum_balance_for_rent_exemption(SplMint::LEN),
            data,
            owner: litesvm_token::TOKEN_ID,
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();
    address
}

#[derive(Clone)]
struct TradePlan {
    args: ExecuteCallbackAuthenticatedProbeV0Args,
    intent_binding: IntentBinding,
    opaque: Vec<AccountMeta>,
    intent_digest: [u8; 32],
    spend_authority: Pubkey,
    primary_callback: Pubkey,
    commit_callback: Pubkey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot(Vec<(Pubkey, AccountSnapshot)>);

#[derive(Clone, Debug, PartialEq, Eq)]
struct AccountSnapshot {
    lamports: u64,
    data: Vec<u8>,
    owner: Pubkey,
    executable: bool,
    rent_epoch: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Economics {
    source: u64,
    destination: u64,
    vault_a: u64,
    vault_b: u64,
    fee_vault: u64,
    accounted_a: u64,
    accounted_b: u64,
    accounted_fee_a: u64,
}

struct Fixture {
    svm: LiteSVM,
    authority: Keypair,
    relayer: Keypair,
    mint_a: Pubkey,
    mint_b: Pubkey,
    user_source_a: Pubkey,
    provider_source_b: Pubkey,
    user_destination_b: Pubkey,
    attacker_destination_a: Pubkey,
    market: Pubkey,
    domain: Pubkey,
    fee_ledger: Pubkey,
    vault_a: Pubkey,
    vault_b: Pubkey,
    fee_vault: Pubkey,
    engine_state: Pubkey,
    helper_state: Pubkey,
    helper_keypair: Option<Keypair>,
}

impl Fixture {
    fn new(timing_mode: u8) -> Self {
        let mut svm = LiteSVM::new();
        load_program(
            &mut svm,
            programmable_routed_callback_core::ID,
            "programmable_routed_callback_core.so",
        );
        load_program(&mut svm, routed_plan_engine::ID, "routed_plan_engine.so");
        load_program(
            &mut svm,
            hostile_router_probe::ID,
            "hostile_router_probe.so",
        );
        load_program(
            &mut svm,
            callback_capability_probe::ID,
            "callback_capability_probe.so",
        );

        let authority = fixture_keypair(1);
        svm.airdrop(&authority.pubkey(), 100 * LAMPORTS_PER_SOL)
            .unwrap();
        let relayer = fixture_keypair(2);
        svm.airdrop(&relayer.pubkey(), 10 * LAMPORTS_PER_SOL)
            .unwrap();
        let mint_a = install_fixture_mint(&mut svm, 101, authority.pubkey(), TOKEN_DECIMALS);
        let mint_b = install_fixture_mint(&mut svm, 102, authority.pubkey(), TOKEN_DECIMALS);
        let user_source_a = CreateAssociatedTokenAccount::new(&mut svm, &authority, &mint_a)
            .send()
            .unwrap();
        let provider_source_b = CreateAssociatedTokenAccount::new(&mut svm, &authority, &mint_b)
            .send()
            .unwrap();
        let recipient = fixture_keypair(3);
        let user_destination_b = CreateAssociatedTokenAccount::new(&mut svm, &authority, &mint_b)
            .owner(&recipient.pubkey())
            .send()
            .unwrap();
        let attacker = fixture_keypair(4);
        let attacker_destination_a =
            CreateAssociatedTokenAccount::new(&mut svm, &authority, &mint_a)
                .owner(&attacker.pubkey())
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
            &programmable_routed_callback_core::ID,
        );
        let (domain, _) = Pubkey::find_program_address(
            &[DOMAIN_SEED_V0, market.as_ref()],
            &programmable_routed_callback_core::ID,
        );
        let (fee_ledger, _) = Pubkey::find_program_address(
            &[FEE_LEDGER_SEED_V0, market.as_ref(), mint_a.as_ref()],
            &programmable_routed_callback_core::ID,
        );
        let (vault_a, _) = Pubkey::find_program_address(
            &[VAULT_SEED_V0, domain.as_ref(), ASSET_A_SEED_V0],
            &programmable_routed_callback_core::ID,
        );
        let (vault_b, _) = Pubkey::find_program_address(
            &[VAULT_SEED_V0, domain.as_ref(), ASSET_B_SEED_V0],
            &programmable_routed_callback_core::ID,
        );
        let (fee_vault, _) = Pubkey::find_program_address(
            &[FEE_VAULT_SEED_V0, fee_ledger.as_ref()],
            &programmable_routed_callback_core::ID,
        );

        let engine_keypair = fixture_keypair(5);
        let engine_state = engine_keypair.pubkey();
        must_send(
            &mut svm,
            &authority,
            vec![Instruction {
                program_id: routed_plan_engine::ID,
                accounts: engine_accounts::Initialize {
                    engine_state,
                    authority: authority_key,
                    system_program: anchor_lang::system_program::ID,
                }
                .to_account_metas(None),
                data: engine_instruction::Initialize {
                    market,
                    revision: ENGINE_REVISION,
                    lp_fee_bps: ENGINE_LP_FEE_BPS,
                    timing_mode,
                }
                .data(),
            }],
            &[&engine_keypair],
            "initialize engine",
        );
        must_send(
            &mut svm,
            &authority,
            vec![Instruction {
                program_id: programmable_routed_callback_core::ID,
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
                    engine_program: routed_plan_engine::ID,
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
            }],
            &[],
            "initialize core",
        );
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

        let helper_keypair = fixture_keypair(6);
        let helper_state = helper_keypair.pubkey();
        let fixture = Self {
            svm,
            authority,
            relayer,
            mint_a,
            mint_b,
            user_source_a,
            provider_source_b,
            user_destination_b,
            attacker_destination_a,
            market,
            domain,
            fee_ledger,
            vault_a,
            vault_b,
            fee_vault,
            engine_state,
            helper_state,
            helper_keypair: Some(helper_keypair),
        };
        assert_eq!(fixture.economics(), initial_economics());
        fixture
    }

    fn fork(&self) -> Self {
        Self {
            svm: self.svm.clone(),
            authority: self.authority.insecure_clone(),
            relayer: self.relayer.insecure_clone(),
            mint_a: self.mint_a,
            mint_b: self.mint_b,
            user_source_a: self.user_source_a,
            provider_source_b: self.provider_source_b,
            user_destination_b: self.user_destination_b,
            attacker_destination_a: self.attacker_destination_a,
            market: self.market,
            domain: self.domain,
            fee_ledger: self.fee_ledger,
            vault_a: self.vault_a,
            vault_b: self.vault_b,
            fee_vault: self.fee_vault,
            engine_state: self.engine_state,
            helper_state: self.helper_state,
            helper_keypair: self.helper_keypair.as_ref().map(Keypair::insecure_clone),
        }
    }

    fn plan(
        &self,
        timing_mode: u8,
        nonce_byte: u8,
        opaque: Vec<AccountMeta>,
        payload: Vec<u8>,
    ) -> TradePlan {
        self.plan_with_expiry(
            timing_mode,
            nonce_byte,
            opaque,
            payload,
            self.current_slot().saturating_add(1_000),
        )
    }

    fn plan_with_expiry(
        &self,
        timing_mode: u8,
        nonce_byte: u8,
        opaque: Vec<AccountMeta>,
        payload: Vec<u8>,
        expires_at_slot: u64,
    ) -> TradePlan {
        assert!(opaque.len() <= MAX_OPAQUE_ACCOUNTS);
        assert!(payload.len() <= MAX_OPAQUE_PAYLOAD_LEN);
        let capability_hash = self.capability_hash(&opaque, true);
        let payload_hash = compute_payload_hash(&payload).unwrap();
        let fee = protocol_fee(TRADE_INPUT);
        let intent_binding = IntentBinding {
            timing_mode,
            core_program: programmable_routed_callback_core::ID,
            market: self.market,
            domain: self.domain,
            engine_program: routed_plan_engine::ID,
            engine_state: self.engine_state,
            user_authority: self.authority.pubkey(),
            user_input: self.user_source_a,
            user_output: self.user_destination_b,
            mint_in: self.mint_a,
            mint_out: self.mint_b,
            domain_input_vault: self.vault_a,
            domain_output_vault: self.vault_b,
            protocol_fee_vault: self.fee_vault,
            fee_ledger: self.fee_ledger,
            token_program: litesvm_token::TOKEN_ID,
            engine_revision: ENGINE_REVISION,
            fee_policy_revision: FEE_POLICY_REVISION_V0,
            amount_in: TRADE_INPUT,
            protocol_fee: fee,
            max_total_input_debit: TRADE_INPUT + fee,
            min_output_credit: MIN_OUTPUT,
            max_protocol_fee: fee,
            expires_at_slot,
            authorization_nonce: [nonce_byte; 32],
            authorized_capability_hash: capability_hash,
            payload_hash,
        };
        let intent_digest = compute_intent_digest(&intent_binding).unwrap();
        let (spend_authority, _) = Pubkey::find_program_address(
            &[
                SPEND_AUTHORITY_SEED_V0,
                self.user_source_a.as_ref(),
                intent_digest.as_ref(),
            ],
            &programmable_routed_callback_core::ID,
        );
        let primary_phase = match timing_mode {
            TIMING_SINGLE => PHASE_TRANSITION,
            TIMING_PREPARE_COMMIT => PHASE_PREPARE,
            _ => panic!("unsupported fixture timing mode"),
        };
        let primary_callback = self.callback(intent_digest, primary_phase);
        let commit_callback = self.callback(intent_digest, PHASE_COMMIT);
        TradePlan {
            args: ExecuteCallbackAuthenticatedProbeV0Args {
                amount_in: TRADE_INPUT,
                max_total_input_debit: TRADE_INPUT + fee,
                min_output_credit: MIN_OUTPUT,
                max_protocol_fee: fee,
                expires_at_slot,
                authorization_nonce: [nonce_byte; 32],
                expected_engine_sequence: 0,
                timing_mode,
                expected_capability_hash: capability_hash,
                opaque_payload: payload,
            },
            intent_binding,
            opaque,
            intent_digest,
            spend_authority,
            primary_callback,
            commit_callback,
        }
    }

    fn callback(&self, intent_digest: [u8; 32], phase: u8) -> Pubkey {
        Pubkey::find_program_address(
            &[
                CALLBACK_AUTHORITY_SEED,
                routed_plan_engine::ID.as_ref(),
                self.engine_state.as_ref(),
                self.market.as_ref(),
                self.domain.as_ref(),
                intent_digest.as_ref(),
                &[phase],
            ],
            &programmable_routed_callback_core::ID,
        )
        .0
    }

    fn helper_opaque(&self) -> Vec<AccountMeta> {
        vec![
            AccountMeta::new_readonly(callback_capability_probe::ID, false),
            AccountMeta::new(self.helper_state, false),
        ]
    }

    fn initialize_helper(&mut self, allowed_callback: Pubkey) {
        let helper_keypair = self
            .helper_keypair
            .take()
            .expect("helper can be initialized only once");
        let instruction = Instruction {
            program_id: callback_capability_probe::ID,
            accounts: helper_accounts::Initialize {
                helper_state: self.helper_state,
                payer: self.authority.pubkey(),
                system_program: anchor_lang::system_program::ID,
            }
            .to_account_metas(None),
            data: helper_instruction::Initialize { allowed_callback }.data(),
        };
        must_send(
            &mut self.svm,
            &self.authority,
            vec![instruction],
            &[&helper_keypair],
            "initialize helper",
        );
    }

    fn capability_hash(&self, opaque: &[AccountMeta], writable: bool) -> [u8; 32] {
        let engine = self.svm.get_account(&self.engine_state).unwrap();
        let mut descriptors = Vec::with_capacity(1 + opaque.len());
        descriptors.push(CapabilityDescriptor {
            key: self.engine_state,
            owner: engine.owner,
            is_writable: writable,
            is_signer: false,
            is_executable: false,
        });
        for meta in opaque {
            let (owner, executable) = if meta.pubkey == self.helper_state
                && self.svm.get_account(&meta.pubkey).is_none()
            {
                (callback_capability_probe::ID, false)
            } else {
                let account = self
                    .svm
                    .get_account(&meta.pubkey)
                    .unwrap_or_else(|| panic!("missing capability {}", meta.pubkey));
                (account.owner, account.executable)
            };
            let effective_writable = opaque
                .iter()
                .any(|candidate| candidate.pubkey == meta.pubkey && candidate.is_writable);
            let effective_signer = opaque
                .iter()
                .any(|candidate| candidate.pubkey == meta.pubkey && candidate.is_signer);
            descriptors.push(CapabilityDescriptor {
                key: meta.pubkey,
                owner,
                is_writable: writable && effective_writable,
                is_signer: writable && effective_signer,
                is_executable: executable,
            });
        }
        compute_capability_hash(&routed_plan_engine::ID, &descriptors).unwrap()
    }

    fn authorize_instruction(&self, plan: &TradePlan) -> Instruction {
        self.authorize_instruction_with_spend(plan, plan.spend_authority)
    }

    fn authorize_instruction_with_spend(
        &self,
        plan: &TradePlan,
        spend_authority: Pubkey,
    ) -> Instruction {
        Instruction {
            program_id: programmable_routed_callback_core::ID,
            accounts: core_accounts::AuthorizeSpendV0 {
                user: self.authority.pubkey(),
                source: self.user_source_a,
                mint: self.mint_a,
                spend_authority,
                token_program: litesvm_token::TOKEN_ID,
            }
            .to_account_metas(None),
            data: core_instruction::AuthorizeSpendV0 {
                args: AuthorizeSpendV0Args {
                    wire_intent: encode_intent_binding(&plan.intent_binding).unwrap(),
                },
            }
            .data(),
        }
    }

    fn execute_instruction(&self, plan: &TradePlan) -> Instruction {
        self.execute_instruction_with(plan, plan.args.clone(), plan.opaque.clone())
    }

    fn execute_instruction_with(
        &self,
        plan: &TradePlan,
        args: ExecuteCallbackAuthenticatedProbeV0Args,
        opaque: Vec<AccountMeta>,
    ) -> Instruction {
        let mut accounts = core_accounts::ExecuteCallbackAuthenticatedProbeV0 {
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
            engine_program: routed_plan_engine::ID,
            engine_state: self.engine_state,
            spend_authority: plan.spend_authority,
            primary_callback: plan.primary_callback,
            commit_callback: plan.commit_callback,
            token_program: litesvm_token::TOKEN_ID,
        }
        .to_account_metas(None);
        accounts.extend(opaque);
        Instruction {
            program_id: programmable_routed_callback_core::ID,
            accounts,
            data: core_instruction::ExecuteCallbackAuthenticatedProbeV0 { args }.data(),
        }
    }

    fn route_instruction(&self, mode: RouterMode, core: Instruction) -> Instruction {
        let mut accounts = router_accounts::Route {
            core_program: programmable_routed_callback_core::ID,
        }
        .to_account_metas(None);
        accounts.extend(core.accounts);
        Instruction {
            program_id: hostile_router_probe::ID,
            accounts,
            data: router_instruction::Route {
                args: RouteProbeArgs {
                    mode,
                    core_instruction_data: core.data,
                },
            }
            .data(),
        }
    }

    fn authorize(&mut self, plan: &TradePlan) -> TransactionMetadata {
        let instruction = self.authorize_instruction(plan);
        must_send(
            &mut self.svm,
            &self.authority,
            vec![instruction],
            &[],
            "authorize spend",
        )
    }

    fn send_success(
        &mut self,
        instructions: Vec<Instruction>,
        additional_signers: &[&Keypair],
        label: &str,
    ) -> TransactionMetadata {
        must_send(
            &mut self.svm,
            &self.authority,
            instructions,
            additional_signers,
            label,
        )
    }

    fn send_success_as_relayer(
        &mut self,
        instructions: Vec<Instruction>,
        label: &str,
    ) -> TransactionMetadata {
        must_send(&mut self.svm, &self.relayer, instructions, &[], label)
    }

    fn send_failure(
        &mut self,
        instructions: Vec<Instruction>,
        additional_signers: &[&Keypair],
        label: &str,
    ) -> FailedTransactionMetadata {
        let transaction = signed_transaction(
            &self.authority,
            instructions,
            additional_signers,
            self.svm.latest_blockhash(),
        );
        match self.svm.send_transaction(transaction) {
            Ok(metadata) => panic!(
                "{label} unexpectedly succeeded\n{}\n{}",
                metadata.pretty_logs(),
                metadata.pretty_cpi_tree(),
            ),
            Err(failure) => failure,
        }
    }

    fn send_failure_as_relayer(
        &mut self,
        instructions: Vec<Instruction>,
        label: &str,
    ) -> FailedTransactionMetadata {
        let transaction = signed_transaction(
            &self.relayer,
            instructions,
            &[],
            self.svm.latest_blockhash(),
        );
        match self.svm.send_transaction(transaction) {
            Ok(metadata) => panic!(
                "{label} unexpectedly succeeded\n{}\n{}",
                metadata.pretty_logs(),
                metadata.pretty_cpi_tree(),
            ),
            Err(failure) => failure,
        }
    }

    fn set_engine_mode(&mut self, mode: u8) {
        self.send_success(
            vec![Instruction {
                program_id: routed_plan_engine::ID,
                accounts: engine_accounts::SetMode {
                    engine_state: self.engine_state,
                    authority: self.authority.pubkey(),
                }
                .to_account_metas(None),
                data: engine_instruction::SetMode { mode }.data(),
            }],
            &[],
            "set engine mode",
        );
    }

    fn current_slot(&self) -> u64 {
        self.svm.get_sysvar::<solana_clock::Clock>().slot
    }

    fn read_anchor<T: AccountDeserialize>(&self, address: Pubkey) -> T {
        let account = self.svm.get_account(&address).unwrap();
        let mut data = account.data.as_slice();
        T::try_deserialize(&mut data).unwrap()
    }

    fn token(&self, address: Pubkey) -> SplTokenAccount {
        get_spl_account(&self.svm, &address).unwrap()
    }

    fn token_balance(&self, address: Pubkey) -> u64 {
        self.token(address).amount
    }

    fn economics(&self) -> Economics {
        let domain: DomainV0 = self.read_anchor(self.domain);
        let ledger: FeeLedgerV0 = self.read_anchor(self.fee_ledger);
        Economics {
            source: self.token_balance(self.user_source_a),
            destination: self.token_balance(self.user_destination_b),
            vault_a: self.token_balance(self.vault_a),
            vault_b: self.token_balance(self.vault_b),
            fee_vault: self.token_balance(self.fee_vault),
            accounted_a: domain.accounted_a,
            accounted_b: domain.accounted_b,
            accounted_fee_a: ledger.accounted_fee_a,
        }
    }

    fn snapshot(&self) -> Snapshot {
        let addresses = [
            self.market,
            self.domain,
            self.fee_ledger,
            self.engine_state,
            self.helper_state,
            self.user_source_a,
            self.user_destination_b,
            self.attacker_destination_a,
            self.vault_a,
            self.vault_b,
            self.fee_vault,
        ];
        Snapshot(
            addresses
                .into_iter()
                .filter_map(|address| {
                    self.svm.get_account(&address).map(|account| {
                        (
                            address,
                            AccountSnapshot {
                                lamports: account.lamports,
                                data: account.data,
                                owner: account.owner,
                                executable: account.executable,
                                rent_epoch: account.rent_epoch,
                            },
                        )
                    })
                })
                .collect(),
        )
    }

    fn readonly_accounts(&mut self, count: usize) -> Vec<AccountMeta> {
        (0..count)
            .map(|index| {
                let tag = u8::try_from(index).unwrap().checked_add(128).unwrap();
                let key = Pubkey::new_from_array([tag; 32]);
                self.svm.airdrop(&key, LAMPORTS_PER_SOL).unwrap();
                AccountMeta::new_readonly(key, false)
            })
            .collect()
    }
}

#[test]
fn direct_and_permissionless_routed_single_execution_have_identical_economics() {
    let base = Fixture::new(TIMING_SINGLE);
    let mut direct = base.fork();
    let mut routed = base.fork();
    let direct_plan = direct.plan(TIMING_SINGLE, 1, vec![], vec![]);
    let authorization = vec![direct.authorize_instruction(&direct_plan)];
    let (authorization_packet, authorization_locks, authorization_writable) = legacy_resources(
        &direct.authority,
        &authorization,
        direct.svm.latest_blockhash(),
    );
    let authorization_meta = direct.send_success(authorization, &[], "top-level authorization");
    assert_resources(
        "authorization",
        &authorization_meta,
        authorization_packet,
        authorization_locks,
        authorization_writable,
        1,
    );
    let direct_instructions = vec![direct.execute_instruction(&direct_plan)];
    let (direct_packet, direct_locks, direct_writable) = legacy_resources(
        &direct.authority,
        &direct_instructions,
        direct.svm.latest_blockhash(),
    );
    let direct_meta = direct.send_success(direct_instructions, &[], "direct single execution");
    assert_success_economics(&direct, None);
    assert_delegate_cleared(&direct);
    assert_single_shape(&direct_meta, false, false);
    assert_resources(
        "direct-single",
        &direct_meta,
        direct_packet,
        direct_locks,
        direct_writable,
        1,
    );

    let routed_plan = routed.plan(TIMING_SINGLE, 1, vec![], vec![]);
    assert_eq!(direct_plan.intent_digest, routed_plan.intent_digest);
    assert_eq!(direct_plan.spend_authority, routed_plan.spend_authority);
    assert_eq!(direct_plan.primary_callback, routed_plan.primary_callback);
    let core_execute = routed.execute_instruction(&routed_plan);
    let route = routed.route_instruction(RouterMode::ForwardOnce, core_execute);
    assert!(
        route
            .accounts
            .iter()
            .all(|meta| meta.pubkey != routed.authority.pubkey()),
        "the router execution closure must not contain the user authority"
    );
    assert!(route
        .accounts
        .iter()
        .all(|meta| meta.pubkey != routed.relayer.pubkey() && !meta.is_signer));
    routed.authorize(&routed_plan);
    let routed_instructions = vec![route];
    let (routed_packet, routed_locks, routed_writable) = legacy_resources(
        &routed.relayer,
        &routed_instructions,
        routed.svm.latest_blockhash(),
    );
    let routed_meta =
        routed.send_success_as_relayer(routed_instructions, "routed single execution");
    assert_success_economics(&routed, None);
    assert_delegate_cleared(&routed);
    assert_single_shape(&routed_meta, true, false);
    assert_resources(
        "routed-single",
        &routed_meta,
        routed_packet,
        routed_locks,
        routed_writable,
        1,
    );

    assert_eq!(direct.economics(), routed.economics());
    let direct_engine: EngineState = direct.read_anchor(direct.engine_state);
    let routed_engine: EngineState = routed.read_anchor(routed.engine_state);
    assert_eq!(
        direct_engine.last_intent_digest,
        routed_engine.last_intent_digest
    );
    assert_eq!(
        direct_engine.last_execution_digest,
        routed_engine.last_execution_digest
    );
    assert_eq!(direct_engine.last_amount_out, routed_engine.last_amount_out);
    assert_eq!(direct.snapshot(), routed.snapshot());
}

#[test]
fn direct_owner_approval_and_core_authorization_are_semantically_equivalent() {
    let mut fixture = Fixture::new(TIMING_SINGLE);
    let plan = fixture.plan(TIMING_SINGLE, 2, vec![], vec![]);
    ApproveChecked::new(
        &mut fixture.svm,
        &fixture.authority,
        &plan.spend_authority,
        &fixture.mint_a,
        TRADE_INPUT + CORE_FEE,
    )
    .source(&fixture.user_source_a)
    .send()
    .unwrap();
    let source = fixture.token(fixture.user_source_a);
    assert_eq!(source.delegate, Some(plan.spend_authority).into());
    assert_eq!(source.delegated_amount, TRADE_INPUT + CORE_FEE);

    fixture.send_success(
        vec![fixture.execute_instruction(&plan)],
        &[],
        "execute after direct classic SPL approval",
    );
    assert_success_economics(&fixture, None);
    assert_delegate_cleared(&fixture);
}

#[test]
fn callback_signer_is_forwarded_only_during_engine_cpi() {
    let mut fixture = Fixture::new(TIMING_SINGLE);
    let plan = fixture.plan(
        TIMING_SINGLE,
        3,
        fixture.helper_opaque(),
        encode_helper_payload(HELPER_INCREMENT).to_vec(),
    );
    fixture.initialize_helper(plan.primary_callback);
    fixture.authorize(&plan);
    let metadata = fixture.send_success(
        vec![fixture.execute_instruction(&plan)],
        &[],
        "single execution with callback helper",
    );
    assert_success_economics(&fixture, Some((1, HELPER_INCREMENT)));
    assert_single_shape(&metadata, false, true);
    let engine: EngineState = fixture.read_anchor(fixture.engine_state);
    assert_eq!(engine.sequence, 1);
    assert_eq!(engine.last_phase, PHASE_TRANSITION);
    assert_eq!(engine.last_phase_context_digest, [0; 32]);
}

#[test]
fn two_phase_prepare_is_readonly_and_commit_authenticates_settlement() {
    let mut fixture = Fixture::new(TIMING_PREPARE_COMMIT);
    let plan = fixture.plan(
        TIMING_PREPARE_COMMIT,
        4,
        fixture.helper_opaque(),
        encode_helper_payload(HELPER_INCREMENT).to_vec(),
    );
    fixture.initialize_helper(plan.commit_callback);
    fixture.authorize(&plan);
    let instructions = vec![fixture.execute_instruction(&plan)];
    let (packet, locks, writable) = legacy_resources(
        &fixture.authority,
        &instructions,
        fixture.svm.latest_blockhash(),
    );
    let metadata = fixture.send_success(instructions, &[], "prepare and commit execution");
    assert_success_economics(&fixture, Some((1, HELPER_INCREMENT)));
    assert_two_phase_shape(&metadata, false);
    assert_resources(
        "direct-prepare-commit",
        &metadata,
        packet,
        locks,
        writable,
        1,
    );

    let engine: EngineState = fixture.read_anchor(fixture.engine_state);
    assert_eq!(engine.sequence, 1, "PREPARE must not advance sequence");
    assert_eq!(engine.last_phase, PHASE_COMMIT);
    assert_ne!(
        engine.last_phase_context_digest, [0; 32],
        "COMMIT must persist Core's nonzero settlement binding"
    );
}

#[test]
fn authorization_must_be_top_level_and_exact() {
    let mut via_router = Fixture::new(TIMING_SINGLE);
    let plan = via_router.plan(TIMING_SINGLE, 5, vec![], vec![]);
    let routed_authorize = via_router.route_instruction(
        RouterMode::ForwardOnce,
        via_router.authorize_instruction(&plan),
    );
    let before = via_router.snapshot();
    let failure = via_router.send_failure(vec![routed_authorize], &[], "CPI authorization");
    assert_log_contains(&failure.meta, "DirectInvocationRequired");
    assert_eq!(via_router.snapshot(), before);

    let mut wrong_pda = Fixture::new(TIMING_SINGLE);
    let plan = wrong_pda.plan(TIMING_SINGLE, 6, vec![], vec![]);
    let failure = wrong_pda.send_failure(
        vec![wrong_pda.authorize_instruction_with_spend(&plan, Pubkey::new_unique())],
        &[],
        "wrong spend PDA",
    );
    assert_log_contains(&failure.meta, "spend authority");
    assert!(wrong_pda.token(wrong_pda.user_source_a).delegate.is_none());

    let mut existing = Fixture::new(TIMING_SINGLE);
    let plan = existing.plan(TIMING_SINGLE, 7, vec![], vec![]);
    let preexisting_delegate = Pubkey::new_unique();
    ApproveChecked::new(
        &mut existing.svm,
        &existing.authority,
        &preexisting_delegate,
        &existing.mint_a,
        1,
    )
    .source(&existing.user_source_a)
    .send()
    .unwrap();
    let failure = existing.send_failure(
        vec![existing.authorize_instruction(&plan)],
        &[],
        "preexisting delegate",
    );
    assert_log_contains(&failure.meta, "delegate");
    let source = existing.token(existing.user_source_a);
    assert_eq!(source.delegate, Some(preexisting_delegate).into());
    assert_eq!(source.delegated_amount, 1);

    let mut wrong_user = Fixture::new(TIMING_SINGLE);
    let plan = wrong_user.plan(TIMING_SINGLE, 55, vec![], vec![]);
    let mut wrong_binding = plan.intent_binding;
    wrong_binding.user_authority = wrong_user.relayer.pubkey();
    let wrong_digest = compute_intent_digest(&wrong_binding).unwrap();
    let wrong_spend = Pubkey::find_program_address(
        &[
            SPEND_AUTHORITY_SEED_V0,
            wrong_user.user_source_a.as_ref(),
            wrong_digest.as_ref(),
        ],
        &programmable_routed_callback_core::ID,
    )
    .0;
    let instruction = Instruction {
        program_id: programmable_routed_callback_core::ID,
        accounts: core_accounts::AuthorizeSpendV0 {
            user: wrong_user.relayer.pubkey(),
            source: wrong_user.user_source_a,
            mint: wrong_user.mint_a,
            spend_authority: wrong_spend,
            token_program: litesvm_token::TOKEN_ID,
        }
        .to_account_metas(None),
        data: core_instruction::AuthorizeSpendV0 {
            args: AuthorizeSpendV0Args {
                wire_intent: encode_intent_binding(&wrong_binding).unwrap(),
            },
        }
        .data(),
    };
    let before = wrong_user.snapshot();
    let failure =
        wrong_user.send_failure_as_relayer(vec![instruction], "wrong token owner authorization");
    assert_log_contains(&failure.meta, "ConstraintTokenOwner");
    assert_eq!(wrong_user.snapshot(), before);
    assert!(wrong_user
        .token(wrong_user.user_source_a)
        .delegate
        .is_none());
}

#[test]
fn delegated_amount_must_match_the_authorized_total_exactly() {
    for delegated_amount in [TRADE_INPUT + CORE_FEE - 1, TRADE_INPUT + CORE_FEE + 1] {
        let mut fixture = Fixture::new(TIMING_SINGLE);
        let plan = fixture.plan(TIMING_SINGLE, 8, vec![], vec![]);
        fixture.authorize(&plan);
        ApproveChecked::new(
            &mut fixture.svm,
            &fixture.authority,
            &plan.spend_authority,
            &fixture.mint_a,
            delegated_amount,
        )
        .source(&fixture.user_source_a)
        .send()
        .unwrap();
        let before = fixture.snapshot();
        let failure = fixture.send_failure(
            vec![fixture.execute_instruction(&plan)],
            &[],
            "inexact delegated amount",
        );
        assert_log_contains(&failure.meta, "delegated amount");
        assert_eq!(fixture.snapshot(), before);
        assert_eq!(
            fixture.token(fixture.user_source_a).delegated_amount,
            delegated_amount
        );
    }
}

#[test]
fn revoked_delegate_and_mutated_user_terms_fail_before_the_engine() {
    let mut revoked = Fixture::new(TIMING_SINGLE);
    let revoked_plan = revoked.plan(TIMING_SINGLE, 56, vec![], vec![]);
    revoked.authorize(&revoked_plan);
    Revoke::new(&mut revoked.svm, &revoked.authority, &revoked.user_source_a)
        .send()
        .unwrap();
    let before = revoked.snapshot();
    let failure = revoked.send_failure(
        vec![revoked.execute_instruction(&revoked_plan)],
        &[],
        "revoked delegate",
    );
    assert_log_contains(&failure.meta, "spend authority");
    assert_eq!(
        frame_program_count(&failure.meta, routed_plan_engine::ID),
        0
    );
    assert_eq!(revoked.snapshot(), before);
    assert!(revoked.token(revoked.user_source_a).delegate.is_none());

    let mut base = Fixture::new(TIMING_SINGLE);
    let plan = base.plan(TIMING_SINGLE, 57, vec![], vec![]);
    base.authorize(&plan);
    for mutation in 0..6 {
        let mut fixture = base.fork();
        let mut args = plan.args.clone();
        match mutation {
            0 => args.amount_in = args.amount_in.checked_add(1).unwrap(),
            1 => args.max_total_input_debit = args.max_total_input_debit.checked_add(1).unwrap(),
            2 => args.min_output_credit = args.min_output_credit.checked_sub(1).unwrap(),
            3 => args.max_protocol_fee = args.max_protocol_fee.checked_add(1).unwrap(),
            4 => args.expires_at_slot = args.expires_at_slot.checked_add(1).unwrap(),
            5 => args.timing_mode = TIMING_PREPARE_COMMIT,
            _ => unreachable!(),
        }
        let before = fixture.snapshot();
        let failure = fixture.send_failure(
            vec![fixture.execute_instruction_with(&plan, args, plan.opaque.clone())],
            &[],
            "mutated authorized user term",
        );
        assert_eq!(
            frame_program_count(&failure.meta, routed_plan_engine::ID),
            0
        );
        assert_eq!(
            fixture.snapshot(),
            before,
            "mutation {mutation} changed state"
        );
        assert_exact_delegate(&fixture, &plan);
    }
}

#[test]
fn routed_byte_account_and_privilege_mutations_fail_closed() {
    let mut base = Fixture::new(TIMING_SINGLE);
    let plan = base.plan(TIMING_SINGLE, 58, vec![], vec![]);
    base.authorize(&plan);

    for mutation in 0..6 {
        let mut fixture = base.fork();
        let mut core = fixture.execute_instruction(&plan);
        match mutation {
            0 => core.data[0] ^= 1,
            1 => {
                let last = core.data.last_mut().unwrap();
                *last ^= 1;
            }
            2 => core.accounts.swap(5, 6),
            3 => {
                core.accounts.pop();
            }
            4 => {
                let extra = Pubkey::new_unique();
                fixture.svm.airdrop(&extra, LAMPORTS_PER_SOL).unwrap();
                core.accounts.push(AccountMeta::new_readonly(extra, false));
            }
            5 => {
                fixture
                    .svm
                    .airdrop(&plan.primary_callback, LAMPORTS_PER_SOL)
                    .unwrap();
                let callback = core
                    .accounts
                    .iter_mut()
                    .find(|meta| meta.pubkey == plan.primary_callback)
                    .unwrap();
                callback.is_writable = true;
            }
            _ => unreachable!(),
        }
        let route = fixture.route_instruction(RouterMode::ForwardOnce, core);
        assert!(route
            .accounts
            .iter()
            .all(|meta| meta.pubkey != fixture.authority.pubkey()));
        let before = fixture.snapshot();
        let failure = fixture.send_failure_as_relayer(vec![route], "mutated routed envelope");
        assert_eq!(
            frame_program_count(&failure.meta, routed_plan_engine::ID),
            0
        );
        assert_eq!(
            fixture.snapshot(),
            before,
            "mutation {mutation} changed state"
        );
        assert_exact_delegate(&fixture, &plan);
    }

    let substitutions = [
        (base.market, base.domain),
        (base.domain, base.market),
        (base.fee_ledger, base.domain),
        (base.mint_a, base.mint_b),
        (base.mint_b, base.mint_a),
        (base.user_source_a, base.provider_source_b),
        (base.user_destination_b, base.attacker_destination_a),
        (base.vault_a, base.vault_b),
        (base.vault_b, base.vault_a),
        (base.fee_vault, base.vault_a),
        (routed_plan_engine::ID, callback_capability_probe::ID),
        (base.engine_state, base.domain),
        (plan.spend_authority, plan.primary_callback),
        (plan.primary_callback, plan.commit_callback),
        (litesvm_token::TOKEN_ID, anchor_lang::system_program::ID),
    ];
    for (target, replacement) in substitutions {
        let mut fixture = base.fork();
        let mut core = fixture.execute_instruction(&plan);
        let target_meta = core
            .accounts
            .iter_mut()
            .find(|meta| meta.pubkey == target)
            .unwrap();
        target_meta.pubkey = replacement;
        let route = fixture.route_instruction(RouterMode::ForwardOnce, core);
        let before = fixture.snapshot();
        let failure = fixture.send_failure_as_relayer(vec![route], "fixed-role substitution");
        assert_eq!(
            frame_program_count(&failure.meta, routed_plan_engine::ID),
            0
        );
        assert_eq!(
            fixture.snapshot(),
            before,
            "substitution for {target} changed state"
        );
        assert_exact_delegate(&fixture, &plan);
    }
}

#[test]
fn router_cannot_use_the_spend_pda_as_its_token_signer() {
    let mut fixture = Fixture::new(TIMING_SINGLE);
    let plan = fixture.plan(TIMING_SINGLE, 9, vec![], vec![]);
    fixture.authorize(&plan);
    let before = fixture.snapshot();
    let route = fixture.route_instruction(
        RouterMode::AttemptSpendDrain {
            source_index: 0,
            destination_index: 1,
            spend_authority_index: 2,
            token_program_index: 3,
            amount: 1,
        },
        Instruction {
            program_id: programmable_routed_callback_core::ID,
            accounts: vec![
                AccountMeta::new(fixture.user_source_a, false),
                AccountMeta::new(fixture.attacker_destination_a, false),
                AccountMeta::new_readonly(plan.spend_authority, false),
                AccountMeta::new_readonly(litesvm_token::TOKEN_ID, false),
            ],
            data: vec![1],
        },
    );
    let failure = fixture.send_failure(vec![route], &[], "router spend drain");
    assert_log_contains(&failure.meta, "missing required signature");
    assert_eq!(fixture.snapshot(), before);
}

#[test]
fn replay_and_double_execution_are_atomic() {
    let mut successful = Fixture::new(TIMING_SINGLE);
    let plan = successful.plan(TIMING_SINGLE, 10, vec![], vec![]);
    successful.authorize(&plan);
    successful.send_success(
        vec![successful.execute_instruction(&plan)],
        &[],
        "first execution",
    );
    successful.svm.expire_blockhash();
    let settled = successful.snapshot();
    let replay = successful.send_failure(
        vec![successful.execute_instruction(&plan)],
        &[],
        "post-success replay",
    );
    assert_log_contains(&replay.meta, "spend authority");
    assert_eq!(successful.snapshot(), settled);

    let mut doubled = Fixture::new(TIMING_SINGLE);
    let plan = doubled.plan(TIMING_SINGLE, 11, vec![], vec![]);
    doubled.authorize(&plan);
    let before = doubled.snapshot();
    let twice =
        doubled.route_instruction(RouterMode::ForwardTwice, doubled.execute_instruction(&plan));
    let failure = doubled.send_failure(vec![twice], &[], "router double execution");
    assert_eq!(
        frame_program_count(&failure.meta, programmable_routed_callback_core::ID),
        2
    );
    let expected_core_error =
        anchor_lang::error::ERROR_CODE_OFFSET + CoreError::InvalidSpendAuthority as u32;
    assert_eq!(
        format!("{:?}", failure.err),
        format!("InstructionError(0, Custom({expected_core_error}))"),
        "the second Core invocation must fail at the consumed exact-delegate boundary"
    );
    assert!(failure
        .meta
        .logs
        .iter()
        .all(|log| !log.contains("ReplayUnexpectedlySucceeded")));
    assert_eq!(doubled.snapshot(), before);
    assert_exact_delegate(&doubled, &plan);
}

#[test]
fn intent_expiry_capability_payload_order_and_alias_are_bound() {
    for mutation in 0..3 {
        let mut fixture = Fixture::new(TIMING_SINGLE);
        let plan = fixture.plan(TIMING_SINGLE, 12 + mutation, vec![], vec![]);
        fixture.authorize(&plan);
        let before = fixture.snapshot();
        let mut args = plan.args.clone();
        match mutation {
            0 => args.authorization_nonce[0] ^= 1,
            1 => args.expected_capability_hash[0] ^= 1,
            2 => args.opaque_payload.push(1),
            _ => unreachable!(),
        }
        let failure = fixture.send_failure(
            vec![fixture.execute_instruction_with(&plan, args, plan.opaque.clone())],
            &[],
            "mutated intent",
        );
        assert!(
            failure
                .meta
                .logs
                .iter()
                .any(|log| log.contains("InvalidSpendAuthority")
                    || log.contains("CapabilityHashExpectationMismatch")),
            "mutation did not hit an authenticated boundary\n{}",
            failure.meta.pretty_logs()
        );
        assert_eq!(fixture.snapshot(), before);
    }

    let mut expired = Fixture::new(TIMING_SINGLE);
    let expiry = expired.current_slot().saturating_add(1);
    let plan = expired.plan_with_expiry(TIMING_SINGLE, 20, vec![], vec![], expiry);
    expired.authorize(&plan);
    expired.svm.warp_to_slot(expiry.saturating_add(1));
    let before = expired.snapshot();
    let failure = expired.send_failure(
        vec![expired.execute_instruction(&plan)],
        &[],
        "expired intent",
    );
    assert_log_contains(&failure.meta, "RequestExpired");
    assert_eq!(expired.snapshot(), before);

    let mut ordered = Fixture::new(TIMING_SINGLE);
    let opaque = ordered.readonly_accounts(2);
    let plan = ordered.plan(TIMING_SINGLE, 21, opaque.clone(), vec![]);
    ordered.authorize(&plan);
    let before = ordered.snapshot();
    let mut reversed = opaque;
    reversed.reverse();
    let failure = ordered.send_failure(
        vec![ordered.execute_instruction_with(&plan, plan.args.clone(), reversed)],
        &[],
        "reordered capability closure",
    );
    assert_log_contains(&failure.meta, "CapabilityHashExpectationMismatch");
    assert_eq!(ordered.snapshot(), before);

    let mut alias = Fixture::new(TIMING_SINGLE);
    let plan = alias.plan(TIMING_SINGLE, 22, vec![], vec![]);
    let before = alias.snapshot();
    let failure = alias.send_failure(
        vec![alias.execute_instruction_with(
            &plan,
            plan.args.clone(),
            vec![AccountMeta::new_readonly(alias.market, false)],
        )],
        &[],
        "fixed-role opaque alias",
    );
    assert_log_contains(&failure.meta, "OpaqueFixedRoleAlias");
    assert_eq!(alias.snapshot(), before);
}

#[test]
fn duplicate_capabilities_are_position_bound_and_effective_privileges_are_normalized() {
    let mut accepted = Fixture::new(TIMING_SINGLE);
    let external = accepted.readonly_accounts(1)[0].pubkey;
    let duplicates = vec![
        AccountMeta::new_readonly(external, false),
        AccountMeta::new_readonly(external, false),
    ];
    let plan = accepted.plan(TIMING_SINGLE, 53, duplicates, vec![]);
    accepted.authorize(&plan);
    accepted.send_success(
        vec![accepted.execute_instruction(&plan)],
        &[],
        "duplicate read-only capability positions",
    );
    assert_success_economics(&accepted, None);

    let mut rejected = Fixture::new(TIMING_SINGLE);
    let token = rejected.provider_source_b;
    let mixed_privileges = vec![
        AccountMeta::new_readonly(token, false),
        AccountMeta::new(token, false),
    ];
    let plan = rejected.plan(TIMING_SINGLE, 54, mixed_privileges, vec![]);
    let before = rejected.snapshot();
    let failure = rejected.send_failure(
        vec![rejected.execute_instruction(&plan)],
        &[],
        "duplicate token capability with effective writable privilege",
    );
    assert_log_contains(&failure.meta, "OpaqueProtectedTokenAccountWritable");
    assert_eq!(rejected.snapshot(), before);
    assert_eq!(
        frame_program_count(&failure.meta, routed_plan_engine::ID),
        0
    );
}

#[test]
fn direct_engine_wrong_entrypoint_and_callback_authority_are_rejected() {
    let mut fixture = Fixture::new(TIMING_SINGLE);
    let plan = fixture.plan(TIMING_SINGLE, 23, vec![], vec![]);
    let transition = build_engine_request(&fixture, &plan, PHASE_TRANSITION);
    let before = fixture.snapshot();
    let non_signer = direct_engine_instruction(
        &fixture,
        &plan,
        &transition,
        PHASE_TRANSITION,
        plan.primary_callback,
        false,
    );
    let failure = fixture.send_failure(vec![non_signer], &[], "callback non-signer");
    assert_log_contains(&failure.meta, "InvalidCallbackPrivileges");
    assert_eq!(fixture.snapshot(), before);

    let prepare = build_engine_request(&fixture, &plan, PHASE_PREPARE);
    let wrong_entrypoint = direct_engine_instruction(
        &fixture,
        &plan,
        &prepare,
        PHASE_TRANSITION,
        fixture.callback(plan.intent_digest, PHASE_PREPARE),
        false,
    );
    let failure = fixture.send_failure(vec![wrong_entrypoint], &[], "wrong engine entrypoint");
    assert_log_contains(&failure.meta, "EntrypointPhaseMismatch");
    assert_eq!(fixture.snapshot(), before);

    let impostor = fixture_keypair(7);
    fixture
        .svm
        .airdrop(&impostor.pubkey(), LAMPORTS_PER_SOL)
        .unwrap();
    let wrong_callback = direct_engine_instruction(
        &fixture,
        &plan,
        &transition,
        PHASE_TRANSITION,
        impostor.pubkey(),
        true,
    );
    let failure = fixture.send_failure(vec![wrong_callback], &[&impostor], "wrong callback signer");
    assert_log_contains(&failure.meta, "InvalidCallbackAuthority");
    assert_eq!(fixture.snapshot(), before);
}

#[test]
fn routed_callback_cannot_be_reused_after_core_returns() {
    let mut fixture = Fixture::new(TIMING_SINGLE);
    let plan = fixture.plan(
        TIMING_SINGLE,
        24,
        fixture.helper_opaque(),
        encode_helper_payload(HELPER_INCREMENT).to_vec(),
    );
    fixture.initialize_helper(plan.primary_callback);
    fixture.authorize(&plan);
    let before = fixture.snapshot();
    let route = fixture.route_instruction(
        RouterMode::ForwardThenReuseCallback {
            helper_program_index: 16,
            helper_state_index: 17,
            callback_authority_index: 13,
            amount: 1,
        },
        fixture.execute_instruction(&plan),
    );
    let failure = fixture.send_failure(vec![route], &[], "post-Core callback reuse");
    assert_log_contains(&failure.meta, "CallbackNotSigner");
    assert_eq!(
        frame_program_count(&failure.meta, routed_plan_engine::ID),
        1
    );
    assert_eq!(
        frame_program_count(&failure.meta, callback_capability_probe::ID),
        2,
        "the first helper call succeeds under Engine; Router's reuse is the second call"
    );
    assert_eq!(
        frame_program_count(&failure.meta, litesvm_token::TOKEN_ID),
        3
    );
    assert_eq!(fixture.snapshot(), before);
}

#[test]
fn arm_a_failure_after_engine_and_helper_mutation_rolls_back_everything() {
    let mut fixture = Fixture::new(TIMING_SINGLE);
    fixture.set_engine_mode(MODE_WRONG_EXECUTION_DIGEST);
    let plan = fixture.plan(
        TIMING_SINGLE,
        25,
        fixture.helper_opaque(),
        encode_helper_payload(HELPER_INCREMENT).to_vec(),
    );
    fixture.initialize_helper(plan.primary_callback);
    fixture.authorize(&plan);
    let before = fixture.snapshot();
    let failure = fixture.send_failure(
        vec![fixture.execute_instruction(&plan)],
        &[],
        "Arm A failure after mutation",
    );
    assert_log_contains(&failure.meta, "EngineReceiptExecutionMismatch");
    assert_eq!(
        frame_program_count(&failure.meta, routed_plan_engine::ID),
        1
    );
    assert_eq!(
        frame_program_count(&failure.meta, callback_capability_probe::ID),
        1
    );
    assert_eq!(
        frame_program_count(&failure.meta, litesvm_token::TOKEN_ID),
        0
    );
    assert_eq!(fixture.snapshot(), before);
    assert_exact_delegate(&fixture, &plan);
}

#[test]
fn arm_b_late_commit_failure_rolls_back_prepare_settlement_and_commit() {
    let mut fixture = Fixture::new(TIMING_PREPARE_COMMIT);
    fixture.set_engine_mode(MODE_LATE_COMMIT_FAILURE);
    let plan = fixture.plan(
        TIMING_PREPARE_COMMIT,
        26,
        fixture.helper_opaque(),
        encode_helper_payload(HELPER_INCREMENT).to_vec(),
    );
    fixture.initialize_helper(plan.commit_callback);
    fixture.authorize(&plan);
    let before = fixture.snapshot();
    let failure = fixture.send_failure(
        vec![fixture.execute_instruction(&plan)],
        &[],
        "late COMMIT failure",
    );
    assert_log_contains(&failure.meta, "DeliberateLateCommitFailure");
    assert_eq!(
        frame_program_count(&failure.meta, routed_plan_engine::ID),
        2
    );
    assert_eq!(
        frame_program_count(&failure.meta, callback_capability_probe::ID),
        1
    );
    assert_eq!(
        frame_program_count(&failure.meta, litesvm_token::TOKEN_ID),
        3
    );
    assert_prepare_settlement_commit_order(&failure.meta, 0);
    assert_eq!(fixture.snapshot(), before);
    assert_exact_delegate(&fixture, &plan);
}

#[test]
fn malformed_wrongly_bound_and_out_of_bounds_receipts_all_rollback() {
    let cases = [
        (MODE_MISSING_RECEIPT, "MissingEngineReceipt"),
        (MODE_WRONG_INTENT_DIGEST, "EngineReceiptIntentMismatch"),
        (
            MODE_WRONG_EXECUTION_DIGEST,
            "EngineReceiptExecutionMismatch",
        ),
        (MODE_MALFORMED_RECEIPT, "InvalidEngineReceipt"),
        (MODE_ZERO_OUTPUT, "ZeroAmount"),
        (MODE_OVERSIZED_OUTPUT, "InsufficientAccountedLiquidity"),
        (MODE_WRONG_RECEIPT_MAGIC, "InvalidEngineReceipt"),
        (MODE_WRONG_RECEIPT_VERSION, "InvalidEngineReceipt"),
        (MODE_TRAILING_RECEIPT_BYTE, "InvalidEngineReceipt"),
        (MODE_WRONG_RECEIPT_PHASE, "EngineReceiptPhaseMismatch"),
    ];
    for (index, (mode, expected)) in cases.into_iter().enumerate() {
        let mut fixture = Fixture::new(TIMING_SINGLE);
        fixture.set_engine_mode(mode);
        let plan = fixture.plan(TIMING_SINGLE, 30 + index as u8, vec![], vec![]);
        fixture.authorize(&plan);
        let before = fixture.snapshot();
        let failure = fixture.send_failure(
            vec![fixture.execute_instruction(&plan)],
            &[],
            "invalid engine receipt",
        );
        assert_log_contains(&failure.meta, expected);
        assert_eq!(
            frame_program_count(&failure.meta, routed_plan_engine::ID),
            1
        );
        assert_eq!(fixture.snapshot(), before, "mode {mode} leaked mutation");
        assert_exact_delegate(&fixture, &plan);
    }
}

#[test]
fn helper_return_data_is_rejected_as_the_engine_receipt_setter() {
    let mut fixture = Fixture::new(TIMING_SINGLE);
    fixture.set_engine_mode(MODE_MISSING_RECEIPT);
    let plan = fixture.plan(
        TIMING_SINGLE,
        50,
        fixture.helper_opaque(),
        encode_helper_payload(HELPER_INCREMENT).to_vec(),
    );
    fixture.initialize_helper(plan.primary_callback);
    fixture.authorize(&plan);
    let before = fixture.snapshot();
    let failure = fixture.send_failure(
        vec![fixture.execute_instruction(&plan)],
        &[],
        "wrong receipt setter",
    );
    assert_log_contains(&failure.meta, "InvalidEngineReceiptSetter");
    assert_eq!(
        frame_program_count(&failure.meta, callback_capability_probe::ID),
        1
    );
    assert_eq!(fixture.snapshot(), before);
    assert_exact_delegate(&fixture, &plan);
}

#[test]
fn maximum_eight_account_128_byte_routed_closure_uses_a_live_alt() {
    run_maximum_resource_case(TIMING_SINGLE, 51, "routed-max-single-8x128-v0-alt");
    run_maximum_resource_case(
        TIMING_PREPARE_COMMIT,
        52,
        "routed-max-prepare-commit-8x128-v0-alt",
    );
}

fn run_maximum_resource_case(timing_mode: u8, nonce: u8, label: &str) {
    let mut fixture = Fixture::new(timing_mode);
    let mut opaque = fixture.helper_opaque();
    opaque.extend(fixture.readonly_accounts(MAX_OPAQUE_ACCOUNTS - opaque.len()));
    assert_eq!(opaque.len(), MAX_OPAQUE_ACCOUNTS);

    let mut payload = vec![0xa5; MAX_OPAQUE_PAYLOAD_LEN];
    payload[..encode_helper_payload(HELPER_INCREMENT).len()]
        .copy_from_slice(&encode_helper_payload(HELPER_INCREMENT));
    let plan = fixture.plan(timing_mode, nonce, opaque, payload);
    assert_eq!(plan.args.opaque_payload.len(), MAX_OPAQUE_PAYLOAD_LEN);
    fixture.initialize_helper(if timing_mode == TIMING_SINGLE {
        plan.primary_callback
    } else {
        plan.commit_callback
    });
    fixture.authorize(&plan);

    let route =
        fixture.route_instruction(RouterMode::ForwardOnce, fixture.execute_instruction(&plan));
    assert!(route
        .accounts
        .iter()
        .all(|meta| meta.pubkey != fixture.authority.pubkey()));
    assert!(route
        .accounts
        .iter()
        .all(|meta| meta.pubkey != fixture.relayer.pubkey() && !meta.is_signer));
    let instructions = vec![route];
    let (legacy_packet, legacy_locks, _) = legacy_resources(
        &fixture.relayer,
        &instructions,
        fixture.svm.latest_blockhash(),
    );
    assert!(
        legacy_packet > PACKET_LIMIT,
        "the measured legacy form ({legacy_packet}) unexpectedly fits; v0 must not be cosmetic"
    );
    eprintln!("RESOURCE {label}: legacy_packet={legacy_packet} requires_v0_alt=true");

    let lookup_addresses = lookup_addresses(&instructions, fixture.relayer.pubkey());
    let lookup = install_lookup_table(&mut fixture, lookup_addresses);
    let blockhash = fixture.svm.latest_blockhash();
    let message = MessageV0::try_compile(
        &fixture.relayer.pubkey(),
        &instructions,
        std::slice::from_ref(&lookup),
        blockhash,
    )
    .unwrap();
    let resolved_locks = message.account_keys.len()
        + message
            .address_table_lookups
            .iter()
            .map(|entry| entry.writable_indexes.len() + entry.readonly_indexes.len())
            .sum::<usize>();
    let static_writable = {
        let signatures = usize::from(message.header.num_required_signatures);
        let writable_signers =
            signatures - usize::from(message.header.num_readonly_signed_accounts);
        let unsigned = message.account_keys.len() - signatures;
        let writable_unsigned =
            unsigned - usize::from(message.header.num_readonly_unsigned_accounts);
        writable_signers + writable_unsigned
    };
    let writable_accounts = static_writable
        + message
            .address_table_lookups
            .iter()
            .map(|entry| entry.writable_indexes.len())
            .sum::<usize>();
    assert_eq!(
        resolved_locks, legacy_locks,
        "v0 must resolve the same unique lock set as the oversized legacy message"
    );
    assert!(!message.address_table_lookups.is_empty());

    let transaction =
        VersionedTransaction::try_new(VersionedMessage::V0(message), &[&fixture.relayer]).unwrap();
    assert_eq!(transaction.signatures.len(), 1);
    let packet = wincode::serialize(&transaction).unwrap().len();
    let metadata = fixture
        .svm
        .send_transaction(transaction)
        .unwrap_or_else(|failure| {
            panic!(
                "maximum v0 route failed: {:?}\n{}\n{}",
                failure.err,
                failure.meta.pretty_logs(),
                failure.meta.pretty_cpi_tree(),
            )
        });
    assert_success_economics(&fixture, Some((1, HELPER_INCREMENT)));
    if timing_mode == TIMING_SINGLE {
        assert_single_shape(&metadata, true, true);
    } else {
        assert_two_phase_shape(&metadata, true);
    }
    assert_resources(
        label,
        &metadata,
        packet,
        resolved_locks,
        writable_accounts,
        1,
    );
}

fn build_engine_request(fixture: &Fixture, plan: &TradePlan, phase: u8) -> EngineRequest {
    let domain: DomainV0 = fixture.read_anchor(fixture.domain);
    let ledger: FeeLedgerV0 = fixture.read_anchor(fixture.fee_ledger);
    let engine: EngineState = fixture.read_anchor(fixture.engine_state);
    let phase_capability_hash = fixture.capability_hash(&plan.opaque, phase != PHASE_PREPARE);
    let binding = ExecutionBinding::new(
        phase,
        plan.intent_digest,
        [0; 32],
        fixture.market,
        fixture.domain,
        ENGINE_REVISION,
        plan.args.amount_in,
        protocol_fee(plan.args.amount_in),
        domain.accounted_a,
        domain.accounted_b,
        ledger.accounted_fee_a,
        engine.sequence,
        plan.args.expected_capability_hash,
        phase_capability_hash,
        u16::try_from(plan.opaque.len()).unwrap(),
        &plan.args.opaque_payload,
    )
    .unwrap();
    EngineRequest::new(binding).unwrap()
}

fn direct_engine_instruction(
    fixture: &Fixture,
    plan: &TradePlan,
    request: &EngineRequest,
    entrypoint_phase: u8,
    callback: Pubkey,
    callback_is_signer: bool,
) -> Instruction {
    let mut accounts = vec![
        AccountMeta::new(fixture.engine_state, false),
        AccountMeta::new_readonly(callback, callback_is_signer),
    ];
    accounts.extend(plan.opaque.clone());
    let wire_request = encode_engine_request(request).unwrap();
    let data = match entrypoint_phase {
        PHASE_TRANSITION => engine_instruction::Transition { wire_request }.data(),
        PHASE_PREPARE => engine_instruction::Prepare { wire_request }.data(),
        PHASE_COMMIT => engine_instruction::Commit { wire_request }.data(),
        _ => unreachable!(),
    };
    Instruction {
        program_id: routed_plan_engine::ID,
        accounts,
        data,
    }
}

fn lookup_addresses(instructions: &[Instruction], payer: Pubkey) -> Vec<Pubkey> {
    let mut seen = HashSet::new();
    let mut addresses = Vec::new();
    for instruction in instructions {
        if instruction.program_id != payer && seen.insert(instruction.program_id) {
            addresses.push(instruction.program_id);
        }
        for meta in &instruction.accounts {
            if !meta.is_signer && meta.pubkey != payer && seen.insert(meta.pubkey) {
                addresses.push(meta.pubkey);
            }
        }
    }
    addresses
}

fn install_lookup_table(
    fixture: &mut Fixture,
    addresses: Vec<Pubkey>,
) -> AddressLookupTableAccount {
    assert!(!addresses.is_empty());
    let recent_slot = fixture.current_slot();
    let relayer = fixture.relayer.pubkey();
    let (create, lookup_table) = create_lookup_table(relayer, relayer, recent_slot);
    let extend = extend_lookup_table(lookup_table, relayer, Some(relayer), addresses.clone());
    must_send(
        &mut fixture.svm,
        &fixture.relayer,
        vec![create, extend],
        &[],
        "create and extend live lookup table",
    );
    fixture.svm.warp_to_slot(recent_slot.saturating_add(1));
    assert!(
        fixture.svm.get_account(&lookup_table).is_some(),
        "LiteSVM must contain the lookup-table account"
    );
    AddressLookupTableAccount {
        key: lookup_table,
        addresses,
    }
}

fn assert_prepare_settlement_commit_order(
    metadata: &TransactionMetadata,
    execute_root_index: usize,
) {
    let roots = metadata.cpi_tree();
    let root = &roots[execute_root_index];
    let programs: Vec<_> = root.children.iter().map(|frame| frame.program_id).collect();
    assert_eq!(
        programs,
        vec![
            routed_plan_engine::ID,
            litesvm_token::TOKEN_ID,
            litesvm_token::TOKEN_ID,
            litesvm_token::TOKEN_ID,
            routed_plan_engine::ID,
        ],
        "{}",
        metadata.pretty_cpi_tree(),
    );
    assert!(root.children[0].children.is_empty());
    assert_eq!(root.children[4].children.len(), 1);
}

fn initial_economics() -> Economics {
    Economics {
        source: INITIAL_MINT_AMOUNT - INITIAL_LIQUIDITY,
        destination: 0,
        vault_a: INITIAL_LIQUIDITY,
        vault_b: INITIAL_LIQUIDITY,
        fee_vault: 0,
        accounted_a: INITIAL_LIQUIDITY,
        accounted_b: INITIAL_LIQUIDITY,
        accounted_fee_a: 0,
    }
}

fn expected_output() -> u64 {
    quote_exact_in(
        INITIAL_LIQUIDITY,
        INITIAL_LIQUIDITY,
        TRADE_INPUT,
        ENGINE_LP_FEE_BPS,
    )
    .unwrap()
    .amount_out
}

fn assert_success_economics(fixture: &Fixture, helper: Option<(u64, u64)>) {
    let output = expected_output();
    assert_eq!(
        fixture.economics(),
        Economics {
            source: INITIAL_MINT_AMOUNT - INITIAL_LIQUIDITY - TRADE_INPUT - CORE_FEE,
            destination: output,
            vault_a: INITIAL_LIQUIDITY + TRADE_INPUT,
            vault_b: INITIAL_LIQUIDITY - output,
            fee_vault: CORE_FEE,
            accounted_a: INITIAL_LIQUIDITY + TRADE_INPUT,
            accounted_b: INITIAL_LIQUIDITY - output,
            accounted_fee_a: CORE_FEE,
        }
    );
    if let Some((calls, value)) = helper {
        let state: HelperState = fixture.read_anchor(fixture.helper_state);
        assert_eq!((state.calls, state.value), (calls, value));
    }
}

fn assert_delegate_cleared(fixture: &Fixture) {
    let source = fixture.token(fixture.user_source_a);
    assert!(source.delegate.is_none());
    assert_eq!(source.delegated_amount, 0);
}

fn assert_exact_delegate(fixture: &Fixture, plan: &TradePlan) {
    let source = fixture.token(fixture.user_source_a);
    assert_eq!(source.delegate, Some(plan.spend_authority).into());
    assert_eq!(source.delegated_amount, TRADE_INPUT + CORE_FEE);
}

fn protocol_fee(amount: u64) -> u64 {
    let numerator = u128::from(amount) * u128::from(PROTOCOL_FEE_BPS_V0);
    numerator.div_ceil(10_000).try_into().unwrap()
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
        program_id: programmable_routed_callback_core::ID,
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
    must_send(svm, authority, vec![instruction], &[], "deposit");
}

fn signed_transaction(
    payer: &Keypair,
    instructions: Vec<Instruction>,
    additional_signers: &[&Keypair],
    blockhash: solana_message::Hash,
) -> Transaction {
    let mut signers = Vec::with_capacity(1 + additional_signers.len());
    signers.push(payer);
    signers.extend_from_slice(additional_signers);
    Transaction::new(
        &signers,
        Message::new(&instructions, Some(&payer.pubkey())),
        blockhash,
    )
}

fn must_send(
    svm: &mut LiteSVM,
    payer: &Keypair,
    instructions: Vec<Instruction>,
    additional_signers: &[&Keypair],
    label: &str,
) -> TransactionMetadata {
    let transaction = signed_transaction(
        payer,
        instructions,
        additional_signers,
        svm.latest_blockhash(),
    );
    svm.send_transaction(transaction).unwrap_or_else(|failure| {
        panic!(
            "{label} failed: {:?}\n{}\n{}",
            failure.err,
            failure.meta.pretty_logs(),
            failure.meta.pretty_cpi_tree(),
        )
    })
}

fn legacy_resources(
    payer: &Keypair,
    instructions: &[Instruction],
    blockhash: solana_message::Hash,
) -> (usize, usize, usize) {
    let message = Message::new(instructions, Some(&payer.pubkey()));
    let account_locks = message.account_keys.len();
    let writable = writable_legacy_accounts(&message);
    let transaction = Transaction::new(&[payer], message, blockhash);
    (
        wincode::serialize(&transaction).unwrap().len(),
        account_locks,
        writable,
    )
}

fn writable_legacy_accounts(message: &Message) -> usize {
    let signatures = usize::from(message.header.num_required_signatures);
    let writable_signers = signatures - usize::from(message.header.num_readonly_signed_accounts);
    let unsigned = message.account_keys.len() - signatures;
    let writable_unsigned = unsigned - usize::from(message.header.num_readonly_unsigned_accounts);
    writable_signers + writable_unsigned
}

fn load_program(svm: &mut LiteSVM, program_id: Pubkey, file_name: &str) {
    let path = program_artifact(file_name);
    assert!(
        path.is_file(),
        "missing {}; run `./scripts/build-sbf.sh` first",
        path.display()
    );
    svm.add_program_from_file(program_id, path).unwrap();
}

fn program_artifact(file_name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/deploy")
        .join(file_name)
}

fn assert_single_shape(metadata: &TransactionMetadata, routed: bool, helper: bool) {
    let roots = metadata.cpi_tree();
    assert_eq!(roots.len(), 1, "{}", metadata.pretty_cpi_tree());
    if routed {
        assert_eq!(roots[0].program_id, hostile_router_probe::ID);
    } else {
        assert_eq!(roots[0].program_id, programmable_routed_callback_core::ID);
    }
    assert_eq!(frame_program_count(metadata, routed_plan_engine::ID), 1);
    assert_eq!(frame_program_count(metadata, litesvm_token::TOKEN_ID), 3);
    assert_eq!(
        frame_program_count(metadata, callback_capability_probe::ID),
        usize::from(helper)
    );
    assert_eq!(
        max_depth(&roots),
        if routed {
            if helper {
                4
            } else {
                3
            }
        } else if helper {
            3
        } else {
            2
        }
    );
}

fn assert_two_phase_shape(metadata: &TransactionMetadata, routed: bool) {
    let roots = metadata.cpi_tree();
    assert_eq!(roots.len(), 1, "{}", metadata.pretty_cpi_tree());
    assert_eq!(frame_program_count(metadata, routed_plan_engine::ID), 2);
    assert_eq!(frame_program_count(metadata, litesvm_token::TOKEN_ID), 3);
    assert_eq!(
        frame_program_count(metadata, callback_capability_probe::ID),
        1
    );
    assert_eq!(max_depth(&roots), if routed { 4 } else { 3 });
    let execute_root = &roots[0];
    let core = if routed {
        execute_root
            .children
            .iter()
            .find(|frame| frame.program_id == programmable_routed_callback_core::ID)
            .unwrap()
    } else {
        execute_root
    };
    let programs: Vec<_> = core.children.iter().map(|frame| frame.program_id).collect();
    assert_eq!(
        programs,
        vec![
            routed_plan_engine::ID,
            litesvm_token::TOKEN_ID,
            litesvm_token::TOKEN_ID,
            litesvm_token::TOKEN_ID,
            routed_plan_engine::ID,
        ],
        "PREPARE must precede settlement and COMMIT must follow it\n{}",
        metadata.pretty_cpi_tree(),
    );
    assert!(
        core.children[0].children.is_empty(),
        "PREPARE must not call helper"
    );
    assert_eq!(
        core.children[4].children.len(),
        1,
        "COMMIT must call helper once"
    );
}

fn assert_resources(
    label: &str,
    metadata: &TransactionMetadata,
    packet: usize,
    account_locks: usize,
    writable_accounts: usize,
    top_level: usize,
) {
    let roots = metadata.cpi_tree();
    let frames: usize = roots.iter().map(frame_count).sum();
    let depth = max_depth(&roots);
    eprintln!(
        "RESOURCE {label}: packet={packet} locks={account_locks} writable={writable_accounts} compute_units={} top_level={top_level} frames={frames} depth={depth}",
        metadata.compute_units_consumed,
    );
    assert!(
        packet <= PACKET_LIMIT,
        "{label} packet {packet} exceeds {PACKET_LIMIT}"
    );
    assert!(metadata.compute_units_consumed < CU_LIMIT);
    assert!(account_locks <= 64);
    assert_eq!(roots.len(), top_level);
    assert!(frames <= 64);
    assert!(depth <= 5);
    assert!(metadata.return_data.data.len() <= 1_024);
    let encoded_engine_calls = metadata
        .inner_instructions
        .iter()
        .flatten()
        .filter(|entry| entry.instruction.data.len() == ENGINE_INSTRUCTION_LEN)
        .count();
    let traced_engine_calls = frame_program_count(metadata, routed_plan_engine::ID);
    assert_eq!(encoded_engine_calls, traced_engine_calls);
    assert_engine_receipt_logs(metadata, traced_engine_calls);
    if traced_engine_calls >= 2 {
        assert_eq!(metadata.return_data.program_id, routed_plan_engine::ID);
        assert_eq!(metadata.return_data.data.len(), ENGINE_RECEIPT_LEN);
    } else {
        assert_eq!(metadata.return_data.program_id, litesvm_token::TOKEN_ID);
        assert!(metadata.return_data.data.is_empty());
    }
    eprintln!(
        "RESOURCE {label}: engine_instruction_len={ENGINE_INSTRUCTION_LEN} receipt_len={ENGINE_RECEIPT_LEN} engine_calls={encoded_engine_calls}"
    );
}

fn assert_engine_receipt_logs(metadata: &TransactionMetadata, expected: usize) {
    let prefix = format!("Program return: {} ", routed_plan_engine::ID);
    let receipts = metadata
        .logs
        .iter()
        .filter_map(|line| line.strip_prefix(&prefix))
        .map(|encoded| BASE64_STANDARD.decode(encoded).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(receipts.len(), expected);
    assert!(receipts
        .iter()
        .all(|receipt| receipt.len() == ENGINE_RECEIPT_LEN));
}

fn frame_program_count(metadata: &TransactionMetadata, program_id: Pubkey) -> usize {
    fn count(frame: &CpiFrame, program_id: Pubkey) -> usize {
        usize::from(frame.program_id == program_id)
            + frame
                .children
                .iter()
                .map(|child| count(child, program_id))
                .sum::<usize>()
    }
    metadata
        .cpi_tree()
        .iter()
        .map(|root| count(root, program_id))
        .sum()
}

fn frame_count(frame: &CpiFrame) -> usize {
    1 + frame.children.iter().map(frame_count).sum::<usize>()
}

fn frame_depth(frame: &CpiFrame) -> usize {
    1 + frame.children.iter().map(frame_depth).max().unwrap_or(0)
}

fn max_depth(roots: &[CpiFrame]) -> usize {
    roots.iter().map(frame_depth).max().unwrap_or(0)
}

fn assert_log_contains(metadata: &TransactionMetadata, expected: &str) {
    assert!(
        metadata.logs.iter().any(|line| line
            .to_ascii_lowercase()
            .contains(&expected.to_ascii_lowercase())),
        "missing `{expected}` in logs:\n{}",
        metadata.pretty_logs(),
    );
}
