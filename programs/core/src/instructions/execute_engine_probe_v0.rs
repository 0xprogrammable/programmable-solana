use anchor_lang::{
    prelude::*,
    solana_program::{
        instruction::{get_stack_height, AccountMeta, Instruction, TRANSACTION_LEVEL_STACK_HEIGHT},
        program::{get_return_data, invoke},
    },
};
use anchor_spl::token::{self, Mint, Token, TokenAccount, TransferChecked};
use engine_probe_interface::{
    compute_plan_hash, decode_receipt, encode_evaluate_instruction, EngineRequest, PlanBinding,
};

use crate::{
    constants::{
        ASSET_A_INDEX_V0, ASSET_A_SEED_V0, ASSET_B_INDEX_V0, ASSET_B_SEED_V0, DOMAIN_SEED_V0,
        FEE_LEDGER_SEED_V0, FEE_VAULT_SEED_V0, INSTRUCTIONS_SYSVAR_ID, MARKET_SEED_V0,
        VAULT_SEED_V0,
    },
    error::CoreError,
    events::EngineProbeExecutedV0,
    math::fee_ceil,
    state::{DomainV0, FeeLedgerV0, MarketV0},
    validation::{
        canonical_domain_vault, canonical_fee_vault, ensure_distinct_roles,
        ensure_no_remaining_accounts, exact_credit, exact_debit, require_raw_covers_accounted,
        validate_classic_mint, validate_credit_destination, validate_fee_ledger,
        validate_market_domain, validate_protected_token_account,
    },
    ID,
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct ExecuteEngineProbeV0Args {
    pub amount_in: u64,
    pub amount_out: u64,
    pub max_total_input_debit: u64,
    pub min_output_credit: u64,
    pub max_protocol_fee: u64,
    pub expires_at_slot: u64,
}

#[derive(Accounts)]
pub struct ExecuteEngineProbeV0<'info> {
    pub user: Signer<'info>,
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
    #[account(
        mut,
        token::mint = mint_a,
        token::authority = user
    )]
    pub user_source_a: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        token::mint = mint_b
    )]
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
    /// CHECK: The exact arbitrary engine address is stored immutably in the authenticated market.
    #[account(address = market.engine_program @ CoreError::InvalidEngineProgram)]
    pub engine_program: UncheckedAccount<'info>,
    /// CHECK: Core authenticates this exact address and owner but never interprets engine state.
    #[account(
        mut,
        address = market.engine_state @ CoreError::InvalidEngineState,
        owner = market.engine_program @ CoreError::InvalidEngineStateOwner
    )]
    pub engine_state: UncheckedAccount<'info>,
    /// CHECK: The address constraint authenticates the canonical Instructions sysvar. It is passed
    /// read-only so the engine can authenticate the top-level Core call without gaining authority.
    #[account(address = INSTRUCTIONS_SYSVAR_ID)]
    pub instructions_sysvar: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token>,
}

pub fn handle_execute_engine_probe_v0(
    mut ctx: Context<ExecuteEngineProbeV0>,
    args: ExecuteEngineProbeV0Args,
) -> Result<()> {
    ensure_no_remaining_accounts(ctx.remaining_accounts)?;
    require!(
        get_stack_height() == TRANSACTION_LEVEL_STACK_HEIGHT,
        CoreError::DirectInvocationRequired
    );

    let accounts = &mut ctx.accounts;
    ensure_distinct_roles(&[
        accounts.user.key(),
        accounts.market.key(),
        accounts.domain.key(),
        accounts.fee_ledger.key(),
        accounts.mint_a.key(),
        accounts.mint_b.key(),
        accounts.user_source_a.key(),
        accounts.user_destination_b.key(),
        accounts.vault_a.key(),
        accounts.vault_b.key(),
        accounts.fee_vault.key(),
        accounts.engine_program.key(),
        accounts.engine_state.key(),
        accounts.instructions_sysvar.key(),
        accounts.token_program.key(),
    ])?;

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
            && !accounts.instructions_sysvar.is_writable
            && !accounts.token_program.to_account_info().is_writable,
        CoreError::UnexpectedWritablePrivilege
    );

    validate_classic_mint(accounts.mint_a.key(), &accounts.mint_a)?;
    validate_classic_mint(accounts.mint_b.key(), &accounts.mint_b)?;
    validate_protected_token_account(
        &accounts.user_source_a,
        accounts.mint_a.key(),
        accounts.user.key(),
    )?;
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

    require!(
        args.amount_in > 0 && args.amount_out > 0,
        CoreError::ZeroAmount
    );
    require!(
        Clock::get()?.slot <= args.expires_at_slot,
        CoreError::RequestExpired
    );
    require!(
        args.amount_out >= args.min_output_credit,
        CoreError::OutputBelowUserMinimum
    );
    require!(
        accounts.domain.accounted_b >= args.amount_out,
        CoreError::InsufficientAccountedLiquidity
    );

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

    let post_accounted_a = accounts
        .domain
        .accounted_a
        .checked_add(args.amount_in)
        .ok_or(CoreError::ArithmeticOverflow)?;
    let post_accounted_b = accounts
        .domain
        .accounted_b
        .checked_sub(args.amount_out)
        .ok_or(CoreError::ArithmeticOverflow)?;
    let post_accounted_fee_a = accounts
        .fee_ledger
        .accounted_fee_a
        .checked_add(protocol_fee)
        .ok_or(CoreError::ArithmeticOverflow)?;

    let plan_binding = PlanBinding {
        core_program: ID,
        market: accounts.market.key(),
        domain: accounts.domain.key(),
        engine_program: accounts.engine_program.key(),
        engine_state: accounts.engine_state.key(),
        user_authority: accounts.user.key(),
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
        amount_out: args.amount_out,
        protocol_fee,
        max_total_input_debit: args.max_total_input_debit,
        min_output_credit: args.min_output_credit,
        max_protocol_fee: args.max_protocol_fee,
        accounted_input_before: accounts.domain.accounted_a,
        accounted_output_before: accounts.domain.accounted_b,
        accounted_fee_before: accounts.fee_ledger.accounted_fee_a,
        expires_at_slot: args.expires_at_slot,
    };
    let plan_digest = compute_plan_hash(&plan_binding);
    let request = EngineRequest {
        plan_hash: plan_digest,
        market: accounts.market.key(),
        domain: accounts.domain.key(),
        engine_revision: accounts.market.engine_revision,
        amount_in: args.amount_in,
        amount_out: args.amount_out,
        protocol_fee,
        accounted_input_before: accounts.domain.accounted_a,
        accounted_output_before: accounts.domain.accounted_b,
        accounted_fee_before: accounts.fee_ledger.accounted_fee_a,
    };
    let engine_instruction = Instruction {
        program_id: accounts.engine_program.key(),
        accounts: vec![
            AccountMeta::new(accounts.engine_state.key(), false),
            AccountMeta::new_readonly(accounts.instructions_sysvar.key(), false),
        ],
        data: encode_evaluate_instruction(&request).to_vec(),
    };

    invoke(
        &engine_instruction,
        &[
            accounts.engine_state.to_account_info(),
            accounts.instructions_sysvar.to_account_info(),
        ],
    )?;

    let (receipt_setter, receipt_data) =
        get_return_data().ok_or(CoreError::MissingEngineReceipt)?;
    require_keys_eq!(
        receipt_setter,
        accounts.engine_program.key(),
        CoreError::InvalidEngineReceiptSetter
    );
    let receipt =
        decode_receipt(&receipt_data).map_err(|_| error!(CoreError::InvalidEngineReceipt))?;
    require!(
        receipt.plan_hash == plan_digest,
        CoreError::EngineReceiptPlanMismatch
    );

    let source_before = accounts.user_source_a.amount;
    let destination_before = accounts.user_destination_b.amount;
    let vault_a_before = accounts.vault_a.amount;
    let vault_b_before = accounts.vault_b.amount;
    let fee_vault_before = accounts.fee_vault.amount;

    token::transfer_checked(
        CpiContext::new(
            Token::id(),
            TransferChecked {
                from: accounts.user_source_a.to_account_info(),
                mint: accounts.mint_a.to_account_info(),
                to: accounts.vault_a.to_account_info(),
                authority: accounts.user.to_account_info(),
            },
        ),
        args.amount_in,
        accounts.mint_a.decimals,
    )?;
    token::transfer_checked(
        CpiContext::new(
            Token::id(),
            TransferChecked {
                from: accounts.user_source_a.to_account_info(),
                mint: accounts.mint_a.to_account_info(),
                to: accounts.fee_vault.to_account_info(),
                authority: accounts.user.to_account_info(),
            },
        ),
        protocol_fee,
        accounts.mint_a.decimals,
    )?;

    let market_key = accounts.market.key();
    let domain_bump = [accounts.domain.bump];
    let domain_seeds = [DOMAIN_SEED_V0, market_key.as_ref(), domain_bump.as_ref()];
    let signer_seeds = [&domain_seeds[..]];
    token::transfer_checked(
        CpiContext::new_with_signer(
            Token::id(),
            TransferChecked {
                from: accounts.vault_b.to_account_info(),
                mint: accounts.mint_b.to_account_info(),
                to: accounts.user_destination_b.to_account_info(),
                authority: accounts.domain.to_account_info(),
            },
            &signer_seeds,
        ),
        args.amount_out,
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
        total_input_debit,
        CoreError::UnexpectedSourceDebit,
    )?;
    exact_credit(
        vault_a_before,
        accounts.vault_a.amount,
        args.amount_in,
        CoreError::UnexpectedVaultCredit,
    )?;
    exact_credit(
        fee_vault_before,
        accounts.fee_vault.amount,
        protocol_fee,
        CoreError::UnexpectedFeeVaultCredit,
    )?;
    exact_debit(
        vault_b_before,
        accounts.vault_b.amount,
        args.amount_out,
        CoreError::UnexpectedVaultDebit,
    )?;
    exact_credit(
        destination_before,
        accounts.user_destination_b.amount,
        args.amount_out,
        CoreError::UnexpectedDestinationCredit,
    )?;

    accounts.domain.accounted_a = post_accounted_a;
    accounts.domain.accounted_b = post_accounted_b;
    accounts.fee_ledger.accounted_fee_a = post_accounted_fee_a;

    emit!(EngineProbeExecutedV0 {
        market: accounts.market.key(),
        domain: accounts.domain.key(),
        engine_program: accounts.engine_program.key(),
        engine_state: accounts.engine_state.key(),
        user: accounts.user.key(),
        mint_a: accounts.mint_a.key(),
        mint_b: accounts.mint_b.key(),
        amount_in: args.amount_in,
        amount_out: args.amount_out,
        protocol_fee,
        plan_digest,
        engine_sequence: receipt.state_sequence,
        post_accounted_a,
        post_accounted_b,
        post_accounted_fee_a,
    });

    Ok(())
}
