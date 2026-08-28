use anchor_lang::{
    prelude::*,
    solana_program::instruction::{get_stack_height, TRANSACTION_LEVEL_STACK_HEIGHT},
};
use anchor_spl::token::{self, ApproveChecked, Mint, Token, TokenAccount};
use routed_callback_probe_wire::{
    compute_intent_digest, decode_intent_binding, INTENT_BINDING_LEN,
};

use crate::{
    constants::SPEND_AUTHORITY_SEED_V0,
    error::CoreError,
    events::SpendAuthorizedV0,
    validation::{
        ensure_distinct_roles, ensure_no_remaining_accounts, validate_classic_mint,
        validate_user_source_before_authorization,
    },
    ID,
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct AuthorizeSpendV0Args {
    pub wire_intent: [u8; INTENT_BINDING_LEN],
}

#[derive(Accounts)]
pub struct AuthorizeSpendV0<'info> {
    pub user: Signer<'info>,
    #[account(mut, token::mint = mint, token::authority = user)]
    pub source: Box<Account<'info, TokenAccount>>,
    pub mint: Box<Account<'info, Mint>>,
    /// CHECK: This account is never read or written. Its address is the
    /// canonical intent-bound PDA that classic SPL Token records as delegate.
    pub spend_authority: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token>,
}

pub fn handle_authorize_spend_v0(
    mut ctx: Context<AuthorizeSpendV0>,
    args: AuthorizeSpendV0Args,
) -> Result<()> {
    require!(
        get_stack_height() == TRANSACTION_LEVEL_STACK_HEIGHT,
        CoreError::DirectInvocationRequired
    );
    ensure_no_remaining_accounts(ctx.remaining_accounts)?;
    let binding = Box::new(
        decode_intent_binding(&args.wire_intent)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?,
    );
    require_keys_eq!(
        binding.core_program,
        ID,
        CoreError::IntentCoreProgramMismatch
    );
    require!(binding.amount_in > 0, CoreError::ZeroAmount);
    require!(
        binding.protocol_fee <= binding.max_protocol_fee,
        CoreError::ProtocolFeeAboveUserMaximum
    );
    let exact_total_debit = binding
        .amount_in
        .checked_add(binding.protocol_fee)
        .ok_or(CoreError::ArithmeticOverflow)?;
    require!(
        exact_total_debit <= binding.max_total_input_debit,
        CoreError::TotalDebitAboveUserMaximum
    );
    require!(
        Clock::get()?.slot <= binding.expires_at_slot,
        CoreError::RequestExpired
    );
    let intent_digest =
        compute_intent_digest(&binding).map_err(|_| error!(CoreError::InvalidWireEncoding))?;

    let accounts = &mut ctx.accounts;
    ensure_distinct_roles(&[
        accounts.user.key(),
        accounts.source.key(),
        accounts.mint.key(),
        accounts.spend_authority.key(),
        accounts.token_program.key(),
        ID,
    ])?;
    require_keys_eq!(
        binding.user_authority,
        accounts.user.key(),
        CoreError::IntentUserAuthorityMismatch
    );
    require_keys_eq!(
        binding.user_input,
        accounts.source.key(),
        CoreError::IntentUserInputMismatch
    );
    require_keys_eq!(
        binding.mint_in,
        accounts.mint.key(),
        CoreError::IntentInputMintMismatch
    );
    require_keys_eq!(
        binding.token_program,
        accounts.token_program.key(),
        CoreError::IntentTokenProgramMismatch
    );
    require!(
        !accounts.mint.to_account_info().is_writable
            && !accounts.spend_authority.is_writable
            && !accounts.token_program.to_account_info().is_writable,
        CoreError::UnexpectedWritablePrivilege
    );
    require!(
        !accounts.source.to_account_info().is_signer
            && !accounts.mint.to_account_info().is_signer
            && !accounts.spend_authority.is_signer
            && !accounts.token_program.to_account_info().is_signer,
        CoreError::UnexpectedSignerPrivilege
    );
    require!(
        !accounts.spend_authority.executable,
        CoreError::InvalidSpendAuthority
    );

    let (expected_spend_authority, _) = Pubkey::find_program_address(
        &[
            SPEND_AUTHORITY_SEED_V0,
            accounts.source.key().as_ref(),
            intent_digest.as_ref(),
        ],
        &ID,
    );
    require_keys_eq!(
        accounts.spend_authority.key(),
        expected_spend_authority,
        CoreError::InvalidSpendAuthority
    );

    validate_classic_mint(accounts.mint.key(), &accounts.mint)?;
    validate_user_source_before_authorization(
        &accounts.source,
        accounts.mint.key(),
        accounts.user.key(),
    )?;
    require!(
        accounts.source.amount >= exact_total_debit,
        CoreError::InsufficientUserSourceBalance
    );

    let source_balance_before = accounts.source.amount;
    token::approve_checked(
        CpiContext::new(
            accounts.token_program.key(),
            ApproveChecked {
                to: accounts.source.to_account_info(),
                mint: accounts.mint.to_account_info(),
                delegate: accounts.spend_authority.to_account_info(),
                authority: accounts.user.to_account_info(),
            },
        ),
        exact_total_debit,
        accounts.mint.decimals,
    )?;

    accounts.source.reload()?;
    require!(
        accounts.source.amount == source_balance_before,
        CoreError::SourceBalanceChangedDuringAuthorization
    );
    require!(
        accounts.source.delegate
            == anchor_lang::solana_program::program_option::COption::Some(expected_spend_authority),
        CoreError::InvalidSpendAuthority
    );
    require!(
        accounts.source.delegated_amount == exact_total_debit,
        CoreError::DelegatedAmountMismatch
    );

    emit!(SpendAuthorizedV0 {
        user_authority: accounts.user.key(),
        user_input: accounts.source.key(),
        mint_in: accounts.mint.key(),
        spend_authority: expected_spend_authority,
        intent_digest,
        timing_mode: binding.timing_mode,
        authorization_nonce: binding.authorization_nonce,
        exact_total_debit,
        expires_at_slot: binding.expires_at_slot,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spend_authority_is_bound_to_source_and_intent() {
        let source = Pubkey::new_unique();
        let first_intent = [7; 32];
        let second_intent = [8; 32];
        let (first, _) = Pubkey::find_program_address(
            &[SPEND_AUTHORITY_SEED_V0, source.as_ref(), &first_intent],
            &ID,
        );
        let (second, _) = Pubkey::find_program_address(
            &[SPEND_AUTHORITY_SEED_V0, source.as_ref(), &second_intent],
            &ID,
        );

        assert_ne!(first, second);
    }
}
