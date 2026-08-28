use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, TransferChecked};

use crate::{
    constants::{ASSET_A_INDEX_V0, ASSET_B_INDEX_V0, DOMAIN_SEED_V0, MARKET_SEED_V0},
    error::CoreError,
    events::LiquidityDepositedV0,
    state::{DomainV0, MarketV0},
    validation::{
        canonical_domain_vault, ensure_distinct_roles, ensure_no_remaining_accounts, exact_credit,
        exact_debit, require_raw_covers_accounted, validate_classic_mint, validate_market_domain,
        validate_protected_token_account,
    },
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct DepositV0Args {
    pub asset_index: u8,
    pub amount: u64,
}

#[derive(Accounts)]
pub struct DepositV0<'info> {
    pub provider: Signer<'info>,
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
    pub mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub provider_source: Box<Account<'info, TokenAccount>>,
    #[account(mut)]
    pub domain_vault: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
}

pub fn handle_deposit_v0(mut ctx: Context<DepositV0>, args: DepositV0Args) -> Result<()> {
    ensure_no_remaining_accounts(ctx.remaining_accounts)?;
    require!(args.amount > 0, CoreError::ZeroAmount);

    let accounts = &mut ctx.accounts;
    ensure_distinct_roles(&[
        accounts.provider.key(),
        accounts.market.key(),
        accounts.domain.key(),
        accounts.mint.key(),
        accounts.provider_source.key(),
        accounts.domain_vault.key(),
        accounts.token_program.key(),
    ])?;

    validate_market_domain(
        accounts.market.key(),
        accounts.market.as_ref(),
        accounts.domain.key(),
        accounts.domain.as_ref(),
    )?;
    validate_classic_mint(accounts.mint.key(), &accounts.mint)?;
    require!(
        !accounts.market.to_account_info().is_writable
            && !accounts.mint.to_account_info().is_writable
            && !accounts.token_program.to_account_info().is_writable,
        CoreError::UnexpectedWritablePrivilege
    );

    let (expected_mint, vault_bump, accounted_before) = match args.asset_index {
        ASSET_A_INDEX_V0 => (
            accounts.market.mint_a,
            accounts.domain.vault_a_bump,
            accounts.domain.accounted_a,
        ),
        ASSET_B_INDEX_V0 => (
            accounts.market.mint_b,
            accounts.domain.vault_b_bump,
            accounts.domain.accounted_b,
        ),
        _ => return err!(CoreError::UnsupportedAssetIndex),
    };
    require_keys_eq!(
        accounts.mint.key(),
        expected_mint,
        CoreError::InvalidTokenMint
    );

    let expected_vault =
        canonical_domain_vault(accounts.domain.key(), args.asset_index, vault_bump)?;
    require_keys_eq!(
        accounts.domain_vault.key(),
        expected_vault,
        CoreError::InvalidDomainVault
    );

    validate_protected_token_account(
        &accounts.provider_source,
        expected_mint,
        accounts.provider.key(),
    )?;
    validate_protected_token_account(&accounts.domain_vault, expected_mint, accounts.domain.key())?;
    require_raw_covers_accounted(accounts.domain_vault.amount, accounted_before)?;

    let accounted_after = accounted_before
        .checked_add(args.amount)
        .ok_or(CoreError::ArithmeticOverflow)?;
    let source_before = accounts.provider_source.amount;
    let vault_before = accounts.domain_vault.amount;

    token::transfer_checked(
        CpiContext::new(
            Token::id(),
            TransferChecked {
                from: accounts.provider_source.to_account_info(),
                mint: accounts.mint.to_account_info(),
                to: accounts.domain_vault.to_account_info(),
                authority: accounts.provider.to_account_info(),
            },
        ),
        args.amount,
        accounts.mint.decimals,
    )?;

    accounts.provider_source.reload()?;
    accounts.domain_vault.reload()?;
    exact_debit(
        source_before,
        accounts.provider_source.amount,
        args.amount,
        CoreError::UnexpectedSourceDebit,
    )?;
    exact_credit(
        vault_before,
        accounts.domain_vault.amount,
        args.amount,
        CoreError::UnexpectedVaultCredit,
    )?;

    match args.asset_index {
        ASSET_A_INDEX_V0 => accounts.domain.accounted_a = accounted_after,
        ASSET_B_INDEX_V0 => accounts.domain.accounted_b = accounted_after,
        _ => return err!(CoreError::UnsupportedAssetIndex),
    }

    emit!(LiquidityDepositedV0 {
        market: accounts.market.key(),
        domain: accounts.domain.key(),
        provider: accounts.provider.key(),
        mint: accounts.mint.key(),
        asset_index: args.asset_index,
        amount: args.amount,
        post_accounted_balance: accounted_after,
    });

    Ok(())
}
