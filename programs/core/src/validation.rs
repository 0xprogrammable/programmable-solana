use anchor_lang::{prelude::*, solana_program::program_option::COption};
use anchor_spl::token::{
    spl_token::{native_mint, state::AccountState},
    Mint, TokenAccount,
};

use crate::{
    constants::{
        DOMAIN_SEED_V0, EXPERIMENT_VERSION_V0, FEE_LEDGER_SEED_V0, FEE_POLICY_REVISION_V0,
        FEE_VAULT_SEED_V0, MARKET_SEED_V0, PROTOCOL_FEE_BPS_V0, VAULT_SEED_V0,
    },
    error::CoreError,
    state::{DomainV0, FeeLedgerV0, MarketV0},
    ID,
};

pub fn ensure_no_remaining_accounts(accounts: &[AccountInfo<'_>]) -> Result<()> {
    require!(accounts.is_empty(), CoreError::UnexpectedRemainingAccounts);
    Ok(())
}

pub fn ensure_distinct_roles(keys: &[Pubkey]) -> Result<()> {
    for (index, key) in keys.iter().enumerate() {
        require!(
            !keys[index + 1..].iter().any(|candidate| candidate == key),
            CoreError::DuplicateAccountRole
        );
    }
    Ok(())
}

pub fn validate_market_domain(
    market_key: Pubkey,
    market: &MarketV0,
    domain_key: Pubkey,
    domain: &DomainV0,
) -> Result<()> {
    require!(
        market.version == EXPERIMENT_VERSION_V0,
        CoreError::InvalidMarket
    );
    require!(
        domain.version == EXPERIMENT_VERSION_V0,
        CoreError::InvalidDomain
    );
    require!(
        market.fee_bps == PROTOCOL_FEE_BPS_V0
            && market.fee_policy_revision == FEE_POLICY_REVISION_V0,
        CoreError::InvalidFeePolicy
    );

    let market_bump = [market.bump];
    let expected_market = Pubkey::create_program_address(
        &[
            MARKET_SEED_V0,
            market.initializer.as_ref(),
            market.market_id.as_ref(),
            market_bump.as_ref(),
        ],
        &ID,
    )
    .map_err(|_| error!(CoreError::InvalidMarket))?;
    require_keys_eq!(market_key, expected_market, CoreError::InvalidMarket);

    require_keys_eq!(domain.market, market_key, CoreError::InvalidDomainAdmission);
    let domain_bump = [domain.bump];
    let expected_domain = Pubkey::create_program_address(
        &[DOMAIN_SEED_V0, market_key.as_ref(), domain_bump.as_ref()],
        &ID,
    )
    .map_err(|_| error!(CoreError::InvalidDomain))?;
    require_keys_eq!(domain_key, expected_domain, CoreError::InvalidDomain);

    require_keys_eq!(
        domain.engine_program,
        market.engine_program,
        CoreError::InvalidDomainAdmission
    );
    require_keys_eq!(
        domain.engine_state,
        market.engine_state,
        CoreError::InvalidDomainAdmission
    );
    require!(
        domain.engine_revision == market.engine_revision,
        CoreError::InvalidEngineRevision
    );
    Ok(())
}

pub fn validate_fee_ledger(
    market_key: Pubkey,
    market: &MarketV0,
    fee_ledger_key: Pubkey,
    fee_ledger: &FeeLedgerV0,
) -> Result<()> {
    require!(
        fee_ledger.version == EXPERIMENT_VERSION_V0,
        CoreError::InvalidFeeLedger
    );
    require_keys_eq!(fee_ledger.market, market_key, CoreError::InvalidFeeLedger);
    require_keys_eq!(
        fee_ledger.mint_a,
        market.mint_a,
        CoreError::InvalidFeeLedger
    );

    let bump = [fee_ledger.bump];
    let expected = Pubkey::create_program_address(
        &[
            FEE_LEDGER_SEED_V0,
            market_key.as_ref(),
            market.mint_a.as_ref(),
            bump.as_ref(),
        ],
        &ID,
    )
    .map_err(|_| error!(CoreError::InvalidFeeLedger))?;
    require_keys_eq!(fee_ledger_key, expected, CoreError::InvalidFeeLedger);
    Ok(())
}

pub fn canonical_domain_vault(domain_key: Pubkey, asset_index: u8, bump: u8) -> Result<Pubkey> {
    let asset = [asset_index];
    let bump = [bump];
    Pubkey::create_program_address(
        &[
            VAULT_SEED_V0,
            domain_key.as_ref(),
            asset.as_ref(),
            bump.as_ref(),
        ],
        &ID,
    )
    .map_err(|_| error!(CoreError::InvalidDomainVault))
}

pub fn canonical_fee_vault(fee_ledger_key: Pubkey, bump: u8) -> Result<Pubkey> {
    let bump = [bump];
    Pubkey::create_program_address(
        &[FEE_VAULT_SEED_V0, fee_ledger_key.as_ref(), bump.as_ref()],
        &ID,
    )
    .map_err(|_| error!(CoreError::InvalidFeeVault))
}

pub fn validate_classic_mint(mint_key: Pubkey, mint: &Mint) -> Result<()> {
    require!(mint.is_initialized, CoreError::MintNotInitialized);
    require_keys_neq!(mint_key, native_mint::ID, CoreError::NativeMintUnsupported);
    require!(
        mint.freeze_authority == COption::None,
        CoreError::MintFreezeAuthorityUnsupported
    );
    Ok(())
}

fn validate_common_token_account(account: &TokenAccount, expected_mint: Pubkey) -> Result<()> {
    require!(
        account.state != AccountState::Frozen,
        CoreError::FrozenTokenAccount
    );
    require!(
        account.state == AccountState::Initialized,
        CoreError::TokenAccountNotInitialized
    );
    require!(
        account.is_native == COption::None,
        CoreError::NativeTokenAccountUnsupported
    );
    require_keys_eq!(account.mint, expected_mint, CoreError::InvalidTokenMint);
    Ok(())
}

pub fn validate_protected_token_account(
    account: &TokenAccount,
    expected_mint: Pubkey,
    expected_authority: Pubkey,
) -> Result<()> {
    validate_common_token_account(account, expected_mint)?;
    require_keys_eq!(
        account.owner,
        expected_authority,
        CoreError::InvalidTokenAuthority
    );
    require!(
        account.delegate == COption::None,
        CoreError::TokenDelegateUnsupported
    );
    require!(
        account.delegated_amount == 0,
        CoreError::InvalidDelegatedAmount
    );
    require!(
        account.close_authority == COption::None,
        CoreError::TokenCloseAuthorityUnsupported
    );
    Ok(())
}

pub fn validate_credit_destination(account: &TokenAccount, expected_mint: Pubkey) -> Result<()> {
    validate_common_token_account(account, expected_mint)
}

pub fn require_raw_covers_accounted(raw: u64, accounted: u64) -> Result<()> {
    require!(raw >= accounted, CoreError::VaultBelowAccountedBalance);
    Ok(())
}

pub fn exact_debit(before: u64, after: u64, expected: u64, error_code: CoreError) -> Result<()> {
    let observed = before
        .checked_sub(after)
        .ok_or(CoreError::ArithmeticOverflow)?;
    if observed != expected {
        return Err(error_code.into());
    }
    Ok(())
}

pub fn exact_credit(before: u64, after: u64, expected: u64, error_code: CoreError) -> Result<()> {
    let observed = after
        .checked_sub(before)
        .ok_or(CoreError::ArithmeticOverflow)?;
    if observed != expected {
        return Err(error_code.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_semantic_roles() {
        let first = Pubkey::new_unique();
        let second = Pubkey::new_unique();
        assert!(ensure_distinct_roles(&[first, second, first]).is_err());
    }

    #[test]
    fn accepts_unique_semantic_roles() {
        let keys = [
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        ];
        assert!(ensure_distinct_roles(&keys).is_ok());
    }

    #[test]
    fn exact_delta_helpers_reject_reversed_or_wrong_deltas() {
        assert!(exact_debit(10, 5, 5, CoreError::UnexpectedSourceDebit).is_ok());
        assert!(exact_debit(5, 10, 5, CoreError::UnexpectedSourceDebit).is_err());
        assert!(exact_credit(5, 10, 5, CoreError::UnexpectedVaultCredit).is_ok());
        assert!(exact_credit(10, 5, 5, CoreError::UnexpectedVaultCredit).is_err());
    }
}
