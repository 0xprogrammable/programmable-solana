use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::{
    constants::{
        ASSET_A_SEED_V0, ASSET_B_SEED_V0, DOMAIN_SEED_V0, EXPERIMENT_VERSION_V0,
        FEE_LEDGER_SEED_V0, FEE_POLICY_REVISION_V0, FEE_VAULT_SEED_V0, MARKET_SEED_V0,
        PROTOCOL_FEE_BPS_V0, VAULT_SEED_V0,
    },
    error::CoreError,
    events::MarketDomainInitializedV0,
    state::{DomainV0, FeeLedgerV0, MarketV0},
    validation::{ensure_distinct_roles, ensure_no_remaining_accounts, validate_classic_mint},
    ID,
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct InitializeMarketDomainV0Args {
    pub market_id: [u8; 32],
    pub engine_revision: u64,
}

#[derive(Accounts)]
#[instruction(args: InitializeMarketDomainV0Args)]
pub struct InitializeMarketDomainV0<'info> {
    #[account(mut)]
    pub initializer: Signer<'info>,
    #[account(
        init,
        payer = initializer,
        space = 8 + MarketV0::INIT_SPACE,
        seeds = [MARKET_SEED_V0, initializer.key().as_ref(), args.market_id.as_ref()],
        bump
    )]
    pub market: Box<Account<'info, MarketV0>>,
    #[account(
        init,
        payer = initializer,
        space = 8 + DomainV0::INIT_SPACE,
        seeds = [DOMAIN_SEED_V0, market.key().as_ref()],
        bump
    )]
    pub domain: Box<Account<'info, DomainV0>>,
    #[account(
        init,
        payer = initializer,
        space = 8 + FeeLedgerV0::INIT_SPACE,
        seeds = [FEE_LEDGER_SEED_V0, market.key().as_ref(), mint_a.key().as_ref()],
        bump
    )]
    pub fee_ledger: Box<Account<'info, FeeLedgerV0>>,
    pub mint_a: Box<Account<'info, Mint>>,
    pub mint_b: Box<Account<'info, Mint>>,
    #[account(
        init,
        payer = initializer,
        seeds = [VAULT_SEED_V0, domain.key().as_ref(), ASSET_A_SEED_V0],
        bump,
        token::mint = mint_a,
        token::authority = domain
    )]
    pub vault_a: Box<Account<'info, TokenAccount>>,
    #[account(
        init,
        payer = initializer,
        seeds = [VAULT_SEED_V0, domain.key().as_ref(), ASSET_B_SEED_V0],
        bump,
        token::mint = mint_b,
        token::authority = domain
    )]
    pub vault_b: Box<Account<'info, TokenAccount>>,
    #[account(
        init,
        payer = initializer,
        seeds = [FEE_VAULT_SEED_V0, fee_ledger.key().as_ref()],
        bump,
        token::mint = mint_a,
        token::authority = fee_ledger
    )]
    pub fee_vault: Box<Account<'info, TokenAccount>>,
    /// CHECK: Arbitrary engines are the point of the experiment. The handler validates that this
    /// account is executable, distinct from Core, immutable in the market, and owns engine_state.
    pub engine_program: UncheckedAccount<'info>,
    /// CHECK: The engine owns and validates its own state schema. Core authenticates only its exact
    /// address, owner, non-executable status, and immutable market/domain binding.
    pub engine_state: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handle_initialize_market_domain_v0(
    mut ctx: Context<InitializeMarketDomainV0>,
    args: InitializeMarketDomainV0Args,
) -> Result<()> {
    ensure_no_remaining_accounts(ctx.remaining_accounts)?;

    let accounts = &mut ctx.accounts;
    ensure_distinct_roles(&[
        accounts.initializer.key(),
        accounts.market.key(),
        accounts.domain.key(),
        accounts.fee_ledger.key(),
        accounts.mint_a.key(),
        accounts.mint_b.key(),
        accounts.vault_a.key(),
        accounts.vault_b.key(),
        accounts.fee_vault.key(),
        accounts.engine_program.key(),
        accounts.engine_state.key(),
        accounts.token_program.key(),
        accounts.system_program.key(),
    ])?;

    require_keys_neq!(
        accounts.mint_a.key(),
        accounts.mint_b.key(),
        CoreError::IdenticalMints
    );
    require!(args.engine_revision != 0, CoreError::InvalidEngineRevision);
    validate_classic_mint(accounts.mint_a.key(), &accounts.mint_a)?;
    validate_classic_mint(accounts.mint_b.key(), &accounts.mint_b)?;

    require!(
        accounts.engine_program.executable,
        CoreError::EngineProgramNotExecutable
    );
    require!(
        !accounts.engine_program.is_writable,
        CoreError::UnexpectedWritablePrivilege
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
        !accounts.mint_a.to_account_info().is_writable
            && !accounts.mint_b.to_account_info().is_writable
            && !accounts.token_program.to_account_info().is_writable
            && !accounts.system_program.to_account_info().is_writable,
        CoreError::UnexpectedWritablePrivilege
    );

    accounts.market.set_inner(MarketV0 {
        version: EXPERIMENT_VERSION_V0,
        bump: ctx.bumps.market,
        initializer: accounts.initializer.key(),
        market_id: args.market_id,
        engine_program: accounts.engine_program.key(),
        engine_state: accounts.engine_state.key(),
        engine_revision: args.engine_revision,
        mint_a: accounts.mint_a.key(),
        mint_b: accounts.mint_b.key(),
        fee_bps: PROTOCOL_FEE_BPS_V0,
        fee_policy_revision: FEE_POLICY_REVISION_V0,
    });
    accounts.domain.set_inner(DomainV0 {
        version: EXPERIMENT_VERSION_V0,
        bump: ctx.bumps.domain,
        vault_a_bump: ctx.bumps.vault_a,
        vault_b_bump: ctx.bumps.vault_b,
        market: accounts.market.key(),
        engine_program: accounts.engine_program.key(),
        engine_state: accounts.engine_state.key(),
        engine_revision: args.engine_revision,
        accounted_a: 0,
        accounted_b: 0,
    });
    accounts.fee_ledger.set_inner(FeeLedgerV0 {
        version: EXPERIMENT_VERSION_V0,
        bump: ctx.bumps.fee_ledger,
        fee_vault_bump: ctx.bumps.fee_vault,
        market: accounts.market.key(),
        mint_a: accounts.mint_a.key(),
        accounted_fee_a: 0,
    });

    emit!(MarketDomainInitializedV0 {
        market: accounts.market.key(),
        domain: accounts.domain.key(),
        engine_program: accounts.engine_program.key(),
        engine_state: accounts.engine_state.key(),
        engine_revision: args.engine_revision,
        mint_a: accounts.mint_a.key(),
        mint_b: accounts.mint_b.key(),
        fee_ledger: accounts.fee_ledger.key(),
        fee_bps: PROTOCOL_FEE_BPS_V0,
    });

    Ok(())
}
