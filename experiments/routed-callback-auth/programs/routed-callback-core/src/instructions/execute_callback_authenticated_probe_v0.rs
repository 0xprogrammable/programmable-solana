use anchor_lang::{
    prelude::*,
    solana_program::{
        instruction::{AccountMeta, Instruction},
        program::{get_return_data, invoke_signed},
        program_option::COption,
    },
};
use anchor_spl::token::{self, Mint, Token, TokenAccount, TransferChecked};
use routed_callback_probe_wire::{
    compute_capability_hash, compute_intent_digest, compute_payload_hash, compute_receipt_digest,
    compute_settlement_digest, decode_receipt, encode_engine_instruction, CapabilityDescriptor,
    EngineReceipt, EngineRequest, ExecutionBinding, IntentBinding, SettlementBinding, PHASE_COMMIT,
    PHASE_PREPARE, PHASE_TRANSITION, TIMING_PREPARE_COMMIT, TIMING_SINGLE,
};

use crate::{
    constants::{
        ASSET_A_INDEX_V0, ASSET_A_SEED_V0, ASSET_B_INDEX_V0, ASSET_B_SEED_V0,
        CALLBACK_AUTHORITY_SEED_V0, DOMAIN_SEED_V0, FEE_LEDGER_SEED_V0, FEE_VAULT_SEED_V0,
        MARKET_SEED_V0, NO_COMMIT_DIGEST_V0, SPEND_AUTHORITY_SEED_V0, VAULT_SEED_V0,
    },
    error::CoreError,
    events::CallbackAuthenticatedProbeExecutedV0,
    math::fee_ceil,
    state::{DomainV0, FeeLedgerV0, MarketV0},
    validation::{
        canonical_domain_vault, canonical_fee_vault, ensure_distinct_roles, exact_credit,
        exact_debit, require_raw_covers_accounted, validate_classic_mint,
        validate_credit_destination, validate_delegated_spend_source, validate_fee_ledger,
        validate_market_domain, validate_opaque_capabilities, validate_opaque_payload,
        validate_protected_token_account,
    },
    ID,
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ExecuteCallbackAuthenticatedProbeV0Args {
    pub amount_in: u64,
    pub max_total_input_debit: u64,
    pub min_output_credit: u64,
    pub max_protocol_fee: u64,
    pub expires_at_slot: u64,
    pub authorization_nonce: [u8; 32],
    /// Relayer-selected execution freshness. This is deliberately not part of
    /// the stable user-authorized intent digest.
    pub expected_engine_sequence: u64,
    pub timing_mode: u8,
    pub expected_capability_hash: [u8; 32],
    pub opaque_payload: Vec<u8>,
}

#[derive(Accounts)]
pub struct ExecuteCallbackAuthenticatedProbeV0<'info> {
    #[account(
        seeds = [MARKET_SEED_V0, market.initializer.as_ref(), market.market_id.as_ref()],
        bump = market.bump
    )]
    pub market: Box<Account<'info, MarketV0>>,
    #[account(
        mut,
        seeds = [DOMAIN_SEED_V0, market.key().as_ref()],
        bump = domain.bump
    )]
    pub domain: Box<Account<'info, DomainV0>>,
    #[account(
        mut,
        seeds = [FEE_LEDGER_SEED_V0, market.key().as_ref(), mint_a.key().as_ref()],
        bump = fee_ledger.bump
    )]
    pub fee_ledger: Box<Account<'info, FeeLedgerV0>>,
    #[account(address = market.mint_a @ CoreError::InvalidTokenMint)]
    pub mint_a: Box<Account<'info, Mint>>,
    #[account(address = market.mint_b @ CoreError::InvalidTokenMint)]
    pub mint_b: Box<Account<'info, Mint>>,
    #[account(mut, token::mint = mint_a)]
    pub user_source_a: Box<Account<'info, TokenAccount>>,
    #[account(mut, token::mint = mint_b)]
    pub user_destination_b: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        seeds = [VAULT_SEED_V0, domain.key().as_ref(), ASSET_A_SEED_V0],
        bump = domain.vault_a_bump,
        token::mint = mint_a,
        token::authority = domain
    )]
    pub vault_a: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        seeds = [VAULT_SEED_V0, domain.key().as_ref(), ASSET_B_SEED_V0],
        bump = domain.vault_b_bump,
        token::mint = mint_b,
        token::authority = domain
    )]
    pub vault_b: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        seeds = [FEE_VAULT_SEED_V0, fee_ledger.key().as_ref()],
        bump = fee_ledger.fee_vault_bump,
        token::mint = mint_a,
        token::authority = fee_ledger
    )]
    pub fee_vault: Box<Account<'info, TokenAccount>>,
    /// CHECK: The authenticated market binds this exact executable program.
    #[account(address = market.engine_program @ CoreError::InvalidEngineProgram)]
    pub engine_program: UncheckedAccount<'info>,
    /// CHECK: Core binds the exact owner and address but never decodes engine
    /// state. The phase CPI controls whether the engine sees it RO or RW.
    #[account(
        mut,
        address = market.engine_state @ CoreError::InvalidEngineState,
        owner = market.engine_program @ CoreError::InvalidEngineStateOwner
    )]
    pub engine_state: UncheckedAccount<'info>,
    /// CHECK: Canonical intent-bound Core PDA. It is used only for the two
    /// exact delegated TransferChecked calls and is never forwarded.
    pub spend_authority: UncheckedAccount<'info>,
    /// CHECK: Canonical phase-bound callback PDA forwarded read-only to the
    /// primary engine phase and signed only for that phase.
    pub primary_callback: UncheckedAccount<'info>,
    /// CHECK: Canonical COMMIT callback PDA. It is supplied even in SINGLE
    /// mode so the fixed envelope is invariant across timing modes.
    pub commit_callback: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Clone, Copy)]
struct AcceptedSettlement {
    protocol_fee: u64,
    total_input_debit: u64,
    amount_out: u64,
    post_accounted_a: u64,
    post_accounted_b: u64,
    post_accounted_fee_a: u64,
}

#[derive(Clone, Copy)]
struct CallbackPda {
    key: Pubkey,
    bump: u8,
}

#[derive(Clone, Copy)]
struct AcceptedEngineExecution {
    primary_phase: u8,
    intent_digest: [u8; 32],
    payload_hash: [u8; 32],
    authorized_capability_hash: [u8; 32],
    primary_phase_capability_hash: [u8; 32],
    commit_phase_capability_hash: [u8; 32],
    primary_execution_digest: [u8; 32],
    primary_receipt_digest: [u8; 32],
    settlement_digest: [u8; 32],
    commit_execution_digest: [u8; 32],
    commit_receipt_digest: [u8; 32],
    primary_engine_sequence: u64,
    commit_engine_sequence: u64,
    opaque_account_count: u8,
}

#[allow(clippy::vec_init_then_push)]
pub fn handle_execute_callback_authenticated_probe_v0<'info>(
    mut ctx: Context<'info, ExecuteCallbackAuthenticatedProbeV0<'info>>,
    args: ExecuteCallbackAuthenticatedProbeV0Args,
) -> Result<()> {
    validate_opaque_payload(&args.opaque_payload)?;
    let primary_phase = primary_phase(args.timing_mode)?;
    require!(args.amount_in > 0, CoreError::ZeroAmount);
    require!(
        Clock::get()?.slot <= args.expires_at_slot,
        CoreError::RequestExpired
    );

    let accounts = &mut ctx.accounts;
    let mut fixed_envelope_keys = Vec::with_capacity(17);
    fixed_envelope_keys.push(accounts.market.key());
    fixed_envelope_keys.push(accounts.domain.key());
    fixed_envelope_keys.push(accounts.fee_ledger.key());
    fixed_envelope_keys.push(accounts.mint_a.key());
    fixed_envelope_keys.push(accounts.mint_b.key());
    fixed_envelope_keys.push(accounts.user_source_a.key());
    fixed_envelope_keys.push(accounts.user_destination_b.key());
    fixed_envelope_keys.push(accounts.vault_a.key());
    fixed_envelope_keys.push(accounts.vault_b.key());
    fixed_envelope_keys.push(accounts.fee_vault.key());
    fixed_envelope_keys.push(accounts.engine_program.key());
    fixed_envelope_keys.push(accounts.engine_state.key());
    fixed_envelope_keys.push(accounts.spend_authority.key());
    fixed_envelope_keys.push(accounts.primary_callback.key());
    fixed_envelope_keys.push(accounts.commit_callback.key());
    fixed_envelope_keys.push(accounts.token_program.key());
    fixed_envelope_keys.push(ID);
    ensure_distinct_roles(&fixed_envelope_keys)?;
    validate_execution_envelope(accounts)?;

    let protocol_fee = fee_ceil(args.amount_in, accounts.market.fee_bps)?;
    require!(
        protocol_fee <= args.max_protocol_fee,
        CoreError::ProtocolFeeAboveUserMaximum
    );
    let total_input_debit = args
        .amount_in
        .checked_add(protocol_fee)
        .ok_or(CoreError::ArithmeticOverflow)?;
    require!(
        total_input_debit <= args.max_total_input_debit,
        CoreError::TotalDebitAboveUserMaximum
    );
    require!(
        accounts.user_source_a.amount >= total_input_debit,
        CoreError::InsufficientUserSourceBalance
    );
    require!(
        accounts.domain.accounted_b >= args.min_output_credit,
        CoreError::InsufficientAccountedLiquidity
    );

    let opaque_descriptors =
        validate_opaque_capabilities(ctx.remaining_accounts, &fixed_envelope_keys)?;
    let authorized_descriptors = capability_descriptors(
        &accounts.engine_state,
        &opaque_descriptors,
        CapabilityMode::Authorized,
    );
    let authorized_capability_hash =
        compute_capability_hash(&accounts.engine_program.key(), &authorized_descriptors)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    require!(
        authorized_capability_hash == args.expected_capability_hash,
        CoreError::CapabilityHashExpectationMismatch
    );
    let payload_hash = compute_payload_hash(&args.opaque_payload)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;

    let intent_digest = compute_bound_intent_digest(
        accounts,
        &args,
        protocol_fee,
        authorized_capability_hash,
        payload_hash,
    )?;
    let (expected_spend_authority, spend_bump) =
        canonical_spend_authority(accounts.user_source_a.key(), intent_digest);
    require_keys_eq!(
        accounts.spend_authority.key(),
        expected_spend_authority,
        CoreError::InvalidSpendAuthority
    );
    validate_delegated_spend_source(
        &accounts.user_source_a,
        accounts.mint_a.key(),
        expected_spend_authority,
        total_input_debit,
    )?;

    let expected_primary_callback = canonical_callback(accounts, intent_digest, primary_phase);
    let expected_commit_callback = canonical_callback(accounts, intent_digest, PHASE_COMMIT);
    validate_callback_account(&accounts.primary_callback, expected_primary_callback.key)?;
    validate_callback_account(&accounts.commit_callback, expected_commit_callback.key)?;

    let primary_descriptors = capability_descriptors(
        &accounts.engine_state,
        &opaque_descriptors,
        if primary_phase == PHASE_PREPARE {
            CapabilityMode::ReadOnly
        } else {
            CapabilityMode::Authorized
        },
    );
    let primary_phase_capability_hash =
        compute_capability_hash(&accounts.engine_program.key(), &primary_descriptors)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    let primary_request = build_engine_request(
        accounts,
        &args,
        protocol_fee,
        intent_digest,
        primary_phase,
        [0; 32],
        args.expected_engine_sequence,
        authorized_capability_hash,
        primary_phase_capability_hash,
        ctx.remaining_accounts.len(),
    )?;
    let primary_receipt = invoke_engine_phase(
        accounts,
        ctx.remaining_accounts,
        &primary_descriptors[1..],
        &primary_request,
        &accounts.primary_callback,
        expected_primary_callback.bump,
    )?;
    validate_receipt_binding(
        &primary_receipt,
        &primary_request,
        primary_phase,
        intent_digest,
    )?;
    validate_primary_sequence(
        primary_phase,
        args.expected_engine_sequence,
        primary_receipt.state_sequence,
    )?;
    validate_amount_out(
        primary_receipt.amount_out,
        args.min_output_credit,
        accounts.domain.accounted_b,
    )?;
    let primary_receipt_digest = compute_receipt_digest(&primary_receipt)
        .map_err(|_| error!(CoreError::InvalidEngineReceipt))?;

    let settlement = AcceptedSettlement {
        protocol_fee,
        total_input_debit,
        amount_out: primary_receipt.amount_out,
        post_accounted_a: accounts
            .domain
            .accounted_a
            .checked_add(args.amount_in)
            .ok_or(CoreError::ArithmeticOverflow)?,
        post_accounted_b: accounts
            .domain
            .accounted_b
            .checked_sub(primary_receipt.amount_out)
            .ok_or(CoreError::ArithmeticOverflow)?,
        post_accounted_fee_a: accounts
            .fee_ledger
            .accounted_fee_a
            .checked_add(protocol_fee)
            .ok_or(CoreError::ArithmeticOverflow)?,
    };

    settle_authenticated_intent(
        accounts,
        args.amount_in,
        intent_digest,
        spend_bump,
        settlement,
    )?;
    let settlement_digest = compute_bound_settlement_digest(
        accounts,
        &args,
        settlement,
        intent_digest,
        primary_request.execution_digest,
        primary_receipt_digest,
    );

    let (
        commit_phase_capability_hash,
        commit_execution_digest,
        commit_receipt_digest,
        commit_engine_sequence,
    ) = if args.timing_mode == TIMING_PREPARE_COMMIT {
        let commit_descriptors = capability_descriptors(
            &accounts.engine_state,
            &opaque_descriptors,
            CapabilityMode::Authorized,
        );
        let commit_capability_hash =
            compute_capability_hash(&accounts.engine_program.key(), &commit_descriptors)
                .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        require!(
            commit_capability_hash == authorized_capability_hash,
            CoreError::CapabilityHashExpectationMismatch
        );
        let commit_request = build_engine_request(
            accounts,
            &args,
            protocol_fee,
            intent_digest,
            PHASE_COMMIT,
            settlement_digest,
            primary_receipt.state_sequence,
            authorized_capability_hash,
            commit_capability_hash,
            ctx.remaining_accounts.len(),
        )?;
        let commit_receipt = invoke_engine_phase(
            accounts,
            ctx.remaining_accounts,
            &commit_descriptors[1..],
            &commit_request,
            &accounts.commit_callback,
            expected_commit_callback.bump,
        )?;
        validate_receipt_binding(
            &commit_receipt,
            &commit_request,
            PHASE_COMMIT,
            intent_digest,
        )?;
        require!(
            commit_receipt.amount_out == primary_receipt.amount_out,
            CoreError::CommitOutputMismatch
        );
        let expected_commit_sequence = primary_receipt
            .state_sequence
            .checked_add(1)
            .ok_or(CoreError::ArithmeticOverflow)?;
        require!(
            commit_receipt.state_sequence == expected_commit_sequence,
            CoreError::CommitSequenceMismatch
        );
        let commit_receipt_digest = compute_receipt_digest(&commit_receipt)
            .map_err(|_| error!(CoreError::InvalidEngineReceipt))?;
        (
            commit_capability_hash,
            commit_request.execution_digest,
            commit_receipt_digest,
            commit_receipt.state_sequence,
        )
    } else {
        ([0; 32], NO_COMMIT_DIGEST_V0, NO_COMMIT_DIGEST_V0, 0)
    };

    // In the two-phase route, Core accounting remains unchanged until COMMIT
    // has authenticated the exact prepared output. A failure rolls back every
    // prior CPI atomically.
    accounts.domain.accounted_a = settlement.post_accounted_a;
    accounts.domain.accounted_b = settlement.post_accounted_b;
    accounts.fee_ledger.accounted_fee_a = settlement.post_accounted_fee_a;

    let accepted_engine = AcceptedEngineExecution {
        primary_phase,
        intent_digest,
        payload_hash,
        authorized_capability_hash,
        primary_phase_capability_hash,
        commit_phase_capability_hash,
        primary_execution_digest: primary_request.execution_digest,
        primary_receipt_digest,
        settlement_digest,
        commit_execution_digest,
        commit_receipt_digest,
        primary_engine_sequence: primary_receipt.state_sequence,
        commit_engine_sequence,
        opaque_account_count: u8::try_from(ctx.remaining_accounts.len())
            .map_err(|_| error!(CoreError::IntegerConversionFailed))?,
    };
    emit_execution_event(accounts, &args, settlement, accepted_engine);
    Ok(())
}

#[derive(Clone, Copy)]
enum CapabilityMode {
    Authorized,
    ReadOnly,
}

fn primary_phase(timing_mode: u8) -> Result<u8> {
    match timing_mode {
        TIMING_SINGLE => Ok(PHASE_TRANSITION),
        TIMING_PREPARE_COMMIT => Ok(PHASE_PREPARE),
        _ => err!(CoreError::UnsupportedTimingMode),
    }
}

#[inline(never)]
fn validate_execution_envelope(accounts: &ExecuteCallbackAuthenticatedProbeV0<'_>) -> Result<()> {
    validate_market_domain(
        accounts.market.key(),
        accounts.market.as_ref(),
        accounts.domain.key(),
        accounts.domain.as_ref(),
    )?;
    validate_fee_ledger(
        accounts.market.key(),
        accounts.market.as_ref(),
        accounts.fee_ledger.key(),
        accounts.fee_ledger.as_ref(),
    )?;
    require_keys_eq!(
        canonical_domain_vault(
            accounts.domain.key(),
            ASSET_A_INDEX_V0,
            accounts.domain.vault_a_bump,
        )?,
        accounts.vault_a.key(),
        CoreError::InvalidDomainVault
    );
    require_keys_eq!(
        canonical_domain_vault(
            accounts.domain.key(),
            ASSET_B_INDEX_V0,
            accounts.domain.vault_b_bump,
        )?,
        accounts.vault_b.key(),
        CoreError::InvalidDomainVault
    );
    require_keys_eq!(
        canonical_fee_vault(
            accounts.fee_ledger.key(),
            accounts.fee_ledger.fee_vault_bump,
        )?,
        accounts.fee_vault.key(),
        CoreError::InvalidFeeVault
    );
    require!(
        accounts.engine_program.executable,
        CoreError::EngineProgramNotExecutable
    );
    require_keys_neq!(
        accounts.engine_program.key(),
        ID,
        CoreError::CoreCannotBeEngine
    );
    require!(
        !accounts.engine_state.executable,
        CoreError::EngineStateExecutable
    );
    require_keys_eq!(
        *accounts.engine_state.owner,
        accounts.engine_program.key(),
        CoreError::InvalidEngineStateOwner
    );
    require!(
        !accounts.market.to_account_info().is_writable
            && !accounts.mint_a.to_account_info().is_writable
            && !accounts.mint_b.to_account_info().is_writable
            && !accounts.engine_program.is_writable
            && !accounts.spend_authority.is_writable
            && !accounts.primary_callback.is_writable
            && !accounts.commit_callback.is_writable
            && !accounts.token_program.to_account_info().is_writable,
        CoreError::UnexpectedWritablePrivilege
    );
    require!(
        accounts.engine_state.is_writable,
        CoreError::UnexpectedWritablePrivilege
    );
    require!(
        !accounts.market.to_account_info().is_signer
            && !accounts.domain.to_account_info().is_signer
            && !accounts.fee_ledger.to_account_info().is_signer
            && !accounts.mint_a.to_account_info().is_signer
            && !accounts.mint_b.to_account_info().is_signer
            && !accounts.user_source_a.to_account_info().is_signer
            && !accounts.user_destination_b.to_account_info().is_signer
            && !accounts.vault_a.to_account_info().is_signer
            && !accounts.vault_b.to_account_info().is_signer
            && !accounts.fee_vault.to_account_info().is_signer
            && !accounts.engine_program.is_signer
            && !accounts.engine_state.is_signer
            && !accounts.spend_authority.is_signer
            && !accounts.primary_callback.is_signer
            && !accounts.commit_callback.is_signer
            && !accounts.token_program.to_account_info().is_signer,
        CoreError::UnexpectedSignerPrivilege
    );
    require!(
        !accounts.spend_authority.executable,
        CoreError::InvalidSpendAuthority
    );
    require!(
        !accounts.primary_callback.executable && !accounts.commit_callback.executable,
        CoreError::InvalidCallbackAuthority
    );

    validate_classic_mint(accounts.mint_a.key(), &accounts.mint_a)?;
    validate_classic_mint(accounts.mint_b.key(), &accounts.mint_b)?;
    validate_credit_destination(&accounts.user_destination_b, accounts.mint_b.key())?;
    validate_protected_token_account(
        &accounts.vault_a,
        accounts.mint_a.key(),
        accounts.domain.key(),
    )?;
    validate_protected_token_account(
        &accounts.vault_b,
        accounts.mint_b.key(),
        accounts.domain.key(),
    )?;
    validate_protected_token_account(
        &accounts.fee_vault,
        accounts.mint_a.key(),
        accounts.fee_ledger.key(),
    )?;
    require_raw_covers_accounted(accounts.vault_a.amount, accounts.domain.accounted_a)?;
    require_raw_covers_accounted(accounts.vault_b.amount, accounts.domain.accounted_b)?;
    require_raw_covers_accounted(
        accounts.fee_vault.amount,
        accounts.fee_ledger.accounted_fee_a,
    )?;
    Ok(())
}

fn capability_descriptors(
    engine_state: &UncheckedAccount<'_>,
    opaque_descriptors: &[CapabilityDescriptor],
    mode: CapabilityMode,
) -> Vec<CapabilityDescriptor> {
    let is_writable = matches!(mode, CapabilityMode::Authorized);
    let mut descriptors = Vec::with_capacity(1 + opaque_descriptors.len());
    descriptors.push(CapabilityDescriptor {
        key: engine_state.key(),
        owner: *engine_state.owner,
        is_writable,
        is_signer: false,
        is_executable: false,
    });
    for descriptor in opaque_descriptors {
        let mut normalized = *descriptor;
        if !is_writable {
            normalized.is_writable = false;
            normalized.is_signer = false;
        }
        descriptors.push(normalized);
    }
    descriptors
}

#[inline(never)]
fn compute_bound_intent_digest(
    accounts: &ExecuteCallbackAuthenticatedProbeV0<'_>,
    args: &ExecuteCallbackAuthenticatedProbeV0Args,
    protocol_fee: u64,
    authorized_capability_hash: [u8; 32],
    payload_hash: [u8; 32],
) -> Result<[u8; 32]> {
    let binding = IntentBinding {
        timing_mode: args.timing_mode,
        core_program: ID,
        market: accounts.market.key(),
        domain: accounts.domain.key(),
        engine_program: accounts.engine_program.key(),
        engine_state: accounts.engine_state.key(),
        user_authority: accounts.user_source_a.owner,
        user_input: accounts.user_source_a.key(),
        user_output: accounts.user_destination_b.key(),
        mint_in: accounts.mint_a.key(),
        mint_out: accounts.mint_b.key(),
        domain_input_vault: accounts.vault_a.key(),
        domain_output_vault: accounts.vault_b.key(),
        protocol_fee_vault: accounts.fee_vault.key(),
        fee_ledger: accounts.fee_ledger.key(),
        token_program: accounts.token_program.key(),
        engine_revision: accounts.market.engine_revision,
        fee_policy_revision: accounts.market.fee_policy_revision,
        amount_in: args.amount_in,
        protocol_fee,
        max_total_input_debit: args.max_total_input_debit,
        min_output_credit: args.min_output_credit,
        max_protocol_fee: args.max_protocol_fee,
        expires_at_slot: args.expires_at_slot,
        authorization_nonce: args.authorization_nonce,
        authorized_capability_hash,
        payload_hash,
    };
    compute_intent_digest(&binding).map_err(|_| error!(CoreError::InvalidWireEncoding))
}

fn canonical_spend_authority(user_input: Pubkey, intent_digest: [u8; 32]) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            SPEND_AUTHORITY_SEED_V0,
            user_input.as_ref(),
            intent_digest.as_ref(),
        ],
        &ID,
    )
}

fn canonical_callback(
    accounts: &ExecuteCallbackAuthenticatedProbeV0<'_>,
    intent_digest: [u8; 32],
    phase: u8,
) -> CallbackPda {
    let engine_program = accounts.engine_program.key();
    let engine_state = accounts.engine_state.key();
    let market = accounts.market.key();
    let domain = accounts.domain.key();
    let phase_seed = [phase];
    let (key, bump) = Pubkey::find_program_address(
        &[
            CALLBACK_AUTHORITY_SEED_V0,
            engine_program.as_ref(),
            engine_state.as_ref(),
            market.as_ref(),
            domain.as_ref(),
            intent_digest.as_ref(),
            phase_seed.as_ref(),
        ],
        &ID,
    );
    CallbackPda { key, bump }
}

fn validate_callback_account(
    callback: &UncheckedAccount<'_>,
    expected_callback: Pubkey,
) -> Result<()> {
    require_keys_eq!(
        callback.key(),
        expected_callback,
        CoreError::InvalidCallbackAuthority
    );
    require!(
        !callback.is_signer && !callback.is_writable && !callback.executable,
        CoreError::InvalidCallbackAuthority
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[inline(never)]
fn build_engine_request(
    accounts: &ExecuteCallbackAuthenticatedProbeV0<'_>,
    args: &ExecuteCallbackAuthenticatedProbeV0Args,
    protocol_fee: u64,
    intent_digest: [u8; 32],
    phase: u8,
    phase_context_digest: [u8; 32],
    pre_sequence: u64,
    authorized_capability_hash: [u8; 32],
    phase_capability_hash: [u8; 32],
    opaque_account_count: usize,
) -> Result<EngineRequest> {
    let opaque_account_count = u16::try_from(opaque_account_count)
        .map_err(|_| error!(CoreError::IntegerConversionFailed))?;
    let binding = ExecutionBinding::new(
        phase,
        intent_digest,
        phase_context_digest,
        accounts.market.key(),
        accounts.domain.key(),
        accounts.market.engine_revision,
        args.amount_in,
        protocol_fee,
        accounts.domain.accounted_a,
        accounts.domain.accounted_b,
        accounts.fee_ledger.accounted_fee_a,
        pre_sequence,
        authorized_capability_hash,
        phase_capability_hash,
        opaque_account_count,
        &args.opaque_payload,
    )
    .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    EngineRequest::new(binding).map_err(|_| error!(CoreError::InvalidWireEncoding))
}

#[inline(never)]
fn invoke_engine_phase<'info>(
    accounts: &ExecuteCallbackAuthenticatedProbeV0<'info>,
    opaque_accounts: &[AccountInfo<'info>],
    phase_opaque_descriptors: &[CapabilityDescriptor],
    request: &EngineRequest,
    callback: &UncheckedAccount<'info>,
    callback_bump: u8,
) -> Result<EngineReceipt> {
    require!(
        phase_opaque_descriptors.len() == opaque_accounts.len(),
        CoreError::InvalidWireEncoding
    );
    let state_is_writable = request.binding.phase != PHASE_PREPARE;
    let mut metas = Vec::with_capacity(2 + phase_opaque_descriptors.len());
    metas.push(if state_is_writable {
        AccountMeta::new(accounts.engine_state.key(), false)
    } else {
        AccountMeta::new_readonly(accounts.engine_state.key(), false)
    });
    metas.push(AccountMeta::new_readonly(callback.key(), true));
    for descriptor in phase_opaque_descriptors {
        metas.push(if descriptor.is_writable {
            AccountMeta::new(descriptor.key, false)
        } else {
            AccountMeta::new_readonly(descriptor.key, false)
        });
    }

    let instruction = Instruction {
        program_id: accounts.engine_program.key(),
        accounts: metas,
        data: encode_engine_instruction(request)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?
            .to_vec(),
    };
    let mut infos = Vec::with_capacity(2 + phase_opaque_descriptors.len());
    infos.push(accounts.engine_state.to_account_info());
    infos.push(callback.to_account_info());
    for descriptor in phase_opaque_descriptors {
        let normalized_info = opaque_accounts
            .iter()
            .find(|candidate| {
                *candidate.key == descriptor.key
                    && *candidate.owner == descriptor.owner
                    && candidate.executable == descriptor.is_executable
                    && (!descriptor.is_writable || candidate.is_writable)
                    && (!descriptor.is_signer || candidate.is_signer)
            })
            .ok_or(CoreError::OpaqueNormalizedPrivilegeUnavailable)?;
        infos.push(normalized_info.clone());
    }

    let engine_program = accounts.engine_program.key();
    let engine_state = accounts.engine_state.key();
    let market = accounts.market.key();
    let domain = accounts.domain.key();
    let phase_seed = [request.binding.phase];
    let bump_seed = [callback_bump];
    let callback_seeds = [
        CALLBACK_AUTHORITY_SEED_V0,
        engine_program.as_ref(),
        engine_state.as_ref(),
        market.as_ref(),
        domain.as_ref(),
        request.binding.intent_digest.as_ref(),
        phase_seed.as_ref(),
        bump_seed.as_ref(),
    ];
    invoke_signed(&instruction, &infos, &[&callback_seeds])?;

    let (setter, data) = get_return_data().ok_or(CoreError::MissingEngineReceipt)?;
    require_keys_eq!(
        setter,
        accounts.engine_program.key(),
        CoreError::InvalidEngineReceiptSetter
    );
    decode_receipt(&data).map_err(|_| error!(CoreError::InvalidEngineReceipt))
}

fn validate_receipt_binding(
    receipt: &EngineReceipt,
    request: &EngineRequest,
    expected_phase: u8,
    expected_intent_digest: [u8; 32],
) -> Result<()> {
    require!(
        receipt.phase == expected_phase,
        CoreError::EngineReceiptPhaseMismatch
    );
    require!(
        receipt.intent_digest == expected_intent_digest,
        CoreError::EngineReceiptIntentMismatch
    );
    require!(
        receipt.execution_digest == request.execution_digest,
        CoreError::EngineReceiptExecutionMismatch
    );
    Ok(())
}

fn validate_primary_sequence(phase: u8, expected: u64, actual: u64) -> Result<()> {
    if phase == PHASE_PREPARE {
        require!(actual == expected, CoreError::PrepareSequenceChanged);
    } else {
        let next = expected
            .checked_add(1)
            .ok_or(CoreError::ArithmeticOverflow)?;
        require!(actual == next, CoreError::TransitionSequenceMismatch);
    }
    Ok(())
}

fn validate_amount_out(amount_out: u64, minimum: u64, accounted_liquidity: u64) -> Result<()> {
    require!(amount_out > 0, CoreError::ZeroAmount);
    require!(amount_out >= minimum, CoreError::OutputBelowUserMinimum);
    require!(
        amount_out <= accounted_liquidity,
        CoreError::InsufficientAccountedLiquidity
    );
    Ok(())
}

#[inline(never)]
fn settle_authenticated_intent<'info>(
    accounts: &mut ExecuteCallbackAuthenticatedProbeV0<'info>,
    amount_in: u64,
    intent_digest: [u8; 32],
    spend_bump: u8,
    settlement: AcceptedSettlement,
) -> Result<()> {
    let source_before = accounts.user_source_a.amount;
    let destination_before = accounts.user_destination_b.amount;
    let vault_a_before = accounts.vault_a.amount;
    let vault_b_before = accounts.vault_b.amount;
    let fee_vault_before = accounts.fee_vault.amount;

    let source_key = accounts.user_source_a.key();
    let spend_bump_seed = [spend_bump];
    let spend_seeds = [
        SPEND_AUTHORITY_SEED_V0,
        source_key.as_ref(),
        intent_digest.as_ref(),
        spend_bump_seed.as_ref(),
    ];
    let spend_signer = [&spend_seeds[..]];
    token::transfer_checked(
        CpiContext::new_with_signer(
            accounts.token_program.key(),
            TransferChecked {
                from: accounts.user_source_a.to_account_info(),
                mint: accounts.mint_a.to_account_info(),
                to: accounts.vault_a.to_account_info(),
                authority: accounts.spend_authority.to_account_info(),
            },
            &spend_signer,
        ),
        amount_in,
        accounts.mint_a.decimals,
    )?;
    token::transfer_checked(
        CpiContext::new_with_signer(
            accounts.token_program.key(),
            TransferChecked {
                from: accounts.user_source_a.to_account_info(),
                mint: accounts.mint_a.to_account_info(),
                to: accounts.fee_vault.to_account_info(),
                authority: accounts.spend_authority.to_account_info(),
            },
            &spend_signer,
        ),
        settlement.protocol_fee,
        accounts.mint_a.decimals,
    )?;

    let market_key = accounts.market.key();
    let domain_bump_seed = [accounts.domain.bump];
    let domain_seeds = [
        DOMAIN_SEED_V0,
        market_key.as_ref(),
        domain_bump_seed.as_ref(),
    ];
    let domain_signer = [&domain_seeds[..]];
    token::transfer_checked(
        CpiContext::new_with_signer(
            accounts.token_program.key(),
            TransferChecked {
                from: accounts.vault_b.to_account_info(),
                mint: accounts.mint_b.to_account_info(),
                to: accounts.user_destination_b.to_account_info(),
                authority: accounts.domain.to_account_info(),
            },
            &domain_signer,
        ),
        settlement.amount_out,
        accounts.mint_b.decimals,
    )?;

    accounts.user_source_a.reload()?;
    accounts.user_destination_b.reload()?;
    accounts.vault_a.reload()?;
    accounts.vault_b.reload()?;
    accounts.fee_vault.reload()?;
    exact_debit(
        source_before,
        accounts.user_source_a.amount,
        settlement.total_input_debit,
        CoreError::UnexpectedSourceDebit,
    )?;
    exact_credit(
        vault_a_before,
        accounts.vault_a.amount,
        amount_in,
        CoreError::UnexpectedVaultCredit,
    )?;
    exact_credit(
        fee_vault_before,
        accounts.fee_vault.amount,
        settlement.protocol_fee,
        CoreError::UnexpectedFeeVaultCredit,
    )?;
    exact_debit(
        vault_b_before,
        accounts.vault_b.amount,
        settlement.amount_out,
        CoreError::UnexpectedVaultDebit,
    )?;
    exact_credit(
        destination_before,
        accounts.user_destination_b.amount,
        settlement.amount_out,
        CoreError::UnexpectedDestinationCredit,
    )?;
    require!(
        accounts.user_source_a.delegate == COption::None
            && accounts.user_source_a.delegated_amount == 0,
        CoreError::SpendDelegateNotCleared
    );
    Ok(())
}

#[inline(never)]
fn compute_bound_settlement_digest(
    accounts: &ExecuteCallbackAuthenticatedProbeV0<'_>,
    args: &ExecuteCallbackAuthenticatedProbeV0Args,
    settlement: AcceptedSettlement,
    intent_digest: [u8; 32],
    primary_execution_digest: [u8; 32],
    primary_receipt_digest: [u8; 32],
) -> [u8; 32] {
    compute_settlement_digest(&SettlementBinding {
        intent_digest,
        primary_execution_digest,
        primary_receipt_digest,
        amount_in: args.amount_in,
        amount_out: settlement.amount_out,
        protocol_fee: settlement.protocol_fee,
        total_input_debit: settlement.total_input_debit,
        accounted_input_before: accounts.domain.accounted_a,
        accounted_output_before: accounts.domain.accounted_b,
        accounted_fee_before: accounts.fee_ledger.accounted_fee_a,
        accounted_input_after: settlement.post_accounted_a,
        accounted_output_after: settlement.post_accounted_b,
        accounted_fee_after: settlement.post_accounted_fee_a,
        observed_source_after: accounts.user_source_a.amount,
        observed_destination_after: accounts.user_destination_b.amount,
        observed_input_vault_after: accounts.vault_a.amount,
        observed_output_vault_after: accounts.vault_b.amount,
        observed_fee_vault_after: accounts.fee_vault.amount,
    })
}

fn emit_execution_event(
    accounts: &ExecuteCallbackAuthenticatedProbeV0<'_>,
    args: &ExecuteCallbackAuthenticatedProbeV0Args,
    settlement: AcceptedSettlement,
    engine: AcceptedEngineExecution,
) {
    emit!(CallbackAuthenticatedProbeExecutedV0 {
        market: accounts.market.key(),
        domain: accounts.domain.key(),
        engine_program: accounts.engine_program.key(),
        engine_state: accounts.engine_state.key(),
        user_authority: accounts.user_source_a.owner,
        user_input: accounts.user_source_a.key(),
        user_output: accounts.user_destination_b.key(),
        spend_authority: accounts.spend_authority.key(),
        primary_callback: accounts.primary_callback.key(),
        commit_callback: accounts.commit_callback.key(),
        mint_a: accounts.mint_a.key(),
        mint_b: accounts.mint_b.key(),
        timing_mode: args.timing_mode,
        primary_phase: engine.primary_phase,
        authorization_nonce: args.authorization_nonce,
        amount_in: args.amount_in,
        amount_out: settlement.amount_out,
        protocol_fee: settlement.protocol_fee,
        intent_digest: engine.intent_digest,
        authorized_capability_hash: engine.authorized_capability_hash,
        primary_phase_capability_hash: engine.primary_phase_capability_hash,
        commit_phase_capability_hash: engine.commit_phase_capability_hash,
        payload_hash: engine.payload_hash,
        primary_execution_digest: engine.primary_execution_digest,
        primary_receipt_digest: engine.primary_receipt_digest,
        settlement_digest: engine.settlement_digest,
        commit_execution_digest: engine.commit_execution_digest,
        commit_receipt_digest: engine.commit_receipt_digest,
        expected_engine_sequence: args.expected_engine_sequence,
        primary_engine_sequence: engine.primary_engine_sequence,
        commit_engine_sequence: engine.commit_engine_sequence,
        opaque_account_count: engine.opaque_account_count,
        post_accounted_a: settlement.post_accounted_a,
        post_accounted_b: settlement.post_accounted_b,
        post_accounted_fee_a: settlement.post_accounted_fee_a,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing_modes_select_distinct_primary_phases() {
        assert_eq!(primary_phase(TIMING_SINGLE).unwrap(), PHASE_TRANSITION);
        assert_eq!(primary_phase(TIMING_PREPARE_COMMIT).unwrap(), PHASE_PREPARE);
        assert!(primary_phase(u8::MAX).is_err());
    }

    #[test]
    fn spend_and_callback_namespaces_cannot_collide() {
        let source = Pubkey::new_unique();
        let intent_digest = [11; 32];
        let (spend, _) = canonical_spend_authority(source, intent_digest);
        let engine_program = Pubkey::new_unique();
        let engine_state = Pubkey::new_unique();
        let market = Pubkey::new_unique();
        let domain = Pubkey::new_unique();
        let (callback, _) = Pubkey::find_program_address(
            &[
                CALLBACK_AUTHORITY_SEED_V0,
                engine_program.as_ref(),
                engine_state.as_ref(),
                market.as_ref(),
                domain.as_ref(),
                intent_digest.as_ref(),
                &[PHASE_TRANSITION],
            ],
            &ID,
        );
        assert_ne!(spend, callback);
    }

    #[test]
    fn prepare_sequence_stays_fixed_while_transition_advances_once() {
        assert!(validate_primary_sequence(PHASE_PREPARE, 9, 9).is_ok());
        assert!(validate_primary_sequence(PHASE_PREPARE, 9, 10).is_err());
        assert!(validate_primary_sequence(PHASE_TRANSITION, 9, 10).is_ok());
        assert!(validate_primary_sequence(PHASE_TRANSITION, 9, 9).is_err());
    }
}
