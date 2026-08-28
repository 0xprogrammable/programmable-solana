use anchor_lang::{prelude::*, solana_program::program_option::COption};
use anchor_spl::token::{
    spl_token::{native_mint, state::AccountState},
    Mint, TokenAccount,
};
use generated_settlement_probe_wire::{
    CapabilityDescriptor, MAX_OPAQUE_ACCOUNTS, MAX_OPAQUE_PAYLOAD_LEN,
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

pub fn validate_opaque_payload(payload: &[u8]) -> Result<()> {
    require!(
        payload.len() <= MAX_OPAQUE_PAYLOAD_LEN,
        CoreError::OpaquePayloadTooLarge
    );
    Ok(())
}

/// Validate the exact ordered capability closure that will be forwarded to the
/// engine. Duplicate opaque positions are intentional: order, position and the
/// effective privileges observed on every AccountInfo are hash-bound later.
pub fn validate_opaque_capabilities(
    opaque_accounts: &[AccountInfo<'_>],
    fixed_envelope_keys: &[Pubkey],
) -> Result<Vec<CapabilityDescriptor>> {
    require!(
        opaque_accounts.len() <= MAX_OPAQUE_ACCOUNTS,
        CoreError::TooManyOpaqueAccounts
    );

    let mut descriptors = Vec::with_capacity(opaque_accounts.len());
    for account in opaque_accounts {
        // Solana unions privileges for duplicate metas. Normalize explicitly so
        // validation and the committed descriptor do not depend on how a host
        // framework happens to expose duplicate AccountInfo positions.
        let effective_is_signer = opaque_accounts
            .iter()
            .any(|candidate| candidate.key == account.key && candidate.is_signer);
        let effective_is_writable = opaque_accounts
            .iter()
            .any(|candidate| candidate.key == account.key && candidate.is_writable);

        require!(!effective_is_signer, CoreError::OpaqueSignerForbidden);
        require!(
            !fixed_envelope_keys
                .iter()
                .any(|fixed_key| fixed_key == account.key),
            CoreError::OpaqueFixedRoleAlias
        );
        require_keys_neq!(*account.owner, ID, CoreError::OpaqueCoreOwnedAccount);
        require!(
            !(account.executable && effective_is_writable),
            CoreError::OpaqueExecutableWritable
        );
        require!(
            !(effective_is_writable
                && (*account.owner == anchor_spl::token::ID
                    || *account.owner == anchor_spl::token_2022::ID)),
            CoreError::OpaqueProtectedTokenAccountWritable
        );

        descriptors.push(CapabilityDescriptor {
            key: *account.key,
            owner: *account.owner,
            is_writable: effective_is_writable,
            is_signer: effective_is_signer,
            is_executable: account.executable,
        });
    }

    Ok(descriptors)
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

    fn account_info(
        key: &'static Pubkey,
        owner: &'static Pubkey,
        is_signer: bool,
        is_writable: bool,
        executable: bool,
    ) -> AccountInfo<'static> {
        let lamports = Box::leak(Box::new(1));
        let data = Box::leak(Vec::<u8>::new().into_boxed_slice());
        AccountInfo::new(
            key,
            is_signer,
            is_writable,
            lamports,
            data,
            owner,
            executable,
        )
    }

    fn leaked_key() -> &'static Pubkey {
        Box::leak(Box::new(Pubkey::new_unique()))
    }

    #[test]
    fn opaque_duplicates_preserve_positions_and_effective_flags() {
        let key = leaked_key();
        let owner = leaked_key();
        let accounts = [
            account_info(key, owner, false, false, false),
            account_info(key, owner, false, true, false),
        ];
        let descriptors = validate_opaque_capabilities(&accounts, &[]).unwrap();

        assert_eq!(descriptors.len(), 2);
        assert_eq!(descriptors[0].key, descriptors[1].key);
        assert!(descriptors[0].is_writable);
        assert!(descriptors[1].is_writable);
    }

    #[test]
    fn duplicate_signer_privilege_is_rejected_for_every_position() {
        let key = leaked_key();
        let owner = leaked_key();
        let accounts = [
            account_info(key, owner, false, false, false),
            account_info(key, owner, true, false, false),
        ];

        assert!(validate_opaque_capabilities(&accounts, &[]).is_err());
    }

    #[test]
    fn normalized_writable_privilege_rejects_executables_and_token_accounts() {
        let executable_key = leaked_key();
        let executable_owner = leaked_key();
        let executable_duplicate = [
            account_info(executable_key, executable_owner, false, false, true),
            account_info(executable_key, executable_owner, false, true, false),
        ];
        assert!(validate_opaque_capabilities(&executable_duplicate, &[]).is_err());

        for token_owner in [anchor_spl::token::ID, anchor_spl::token_2022::ID] {
            let token_key = leaked_key();
            let token_owner = Box::leak(Box::new(token_owner));
            let token_account = account_info(token_key, token_owner, false, true, false);
            assert!(validate_opaque_capabilities(&[token_account], &[]).is_err());
        }
    }

    #[test]
    fn payload_and_capability_count_limits_fail_closed() {
        assert!(validate_opaque_payload(&[0; MAX_OPAQUE_PAYLOAD_LEN + 1]).is_err());

        let owner = leaked_key();
        let accounts = (0..=MAX_OPAQUE_ACCOUNTS)
            .map(|_| account_info(leaked_key(), owner, false, false, false))
            .collect::<Vec<_>>();
        assert!(validate_opaque_capabilities(&accounts, &[]).is_err());
    }

    #[test]
    fn opaque_closure_rejects_signers_fixed_aliases_and_core_owners() {
        let external_key = leaked_key();
        let external_owner = leaked_key();
        let signer = account_info(external_key, external_owner, true, false, false);
        assert!(validate_opaque_capabilities(&[signer], &[]).is_err());

        let alias_key = leaked_key();
        let alias = account_info(alias_key, external_owner, false, false, false);
        assert!(validate_opaque_capabilities(&[alias], &[*alias_key]).is_err());

        let core_owned = account_info(external_key, &ID, false, false, false);
        assert!(validate_opaque_capabilities(&[core_owned], &[]).is_err());
    }

    #[test]
    fn rejects_duplicate_fixed_roles_and_wrong_deltas() {
        let first = Pubkey::new_unique();
        let second = Pubkey::new_unique();
        assert!(ensure_distinct_roles(&[first, second, first]).is_err());
        assert!(exact_debit(10, 5, 5, CoreError::UnexpectedSourceDebit).is_ok());
        assert!(exact_debit(5, 10, 5, CoreError::UnexpectedSourceDebit).is_err());
        assert!(exact_credit(5, 10, 5, CoreError::UnexpectedVaultCredit).is_ok());
    }
}
