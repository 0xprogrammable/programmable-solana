//! Exact Classic SPL Token settlement for accepted protected Move plans.
//!
//! Token-2022, native/WSOL lifecycle, transfer hooks, mint/burn and arbitrary
//! settlement drivers are intentionally absent from this private profile.

use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke_signed, program_option::COption, program_pack::Pack},
};
use anchor_spl::token::spl_token::{
    self,
    state::{Account as SplTokenAccount, AccountState, Mint as SplMint},
};

use crate::error::CoreError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClassicSplMintSnapshot {
    pub key: Pubkey,
    pub decimals: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClassicSplEndpointSnapshot {
    pub key: Pubkey,
    pub mint: Pubkey,
    pub authority: Pubkey,
    pub amount: u64,
    pub delegate: Option<Pubkey>,
    pub delegated_amount: u64,
    pub close_authority: Option<Pubkey>,
}

impl ClassicSplEndpointSnapshot {
    /// Converts the fully parsed Classic SPL account prestate into the one
    /// canonical Wire row committed by every protected capability.
    pub fn wire_state_row(
        self,
    ) -> generic_effect_private_wire::ClassicSplEndpointStateRowCandidateV0 {
        generic_effect_private_wire::ClassicSplEndpointStateRowCandidateV0 {
            wire_version: generic_effect_private_wire::WIRE_VERSION,
            account_state: generic_effect_private_wire::CLASSIC_SPL_ACCOUNT_STATE_INITIALIZED,
            delegate_present: self.delegate.is_some(),
            native_present: false,
            close_authority_present: self.close_authority.is_some(),
            endpoint_key: self.key.to_bytes(),
            token_program: spl_token::ID.to_bytes(),
            mint: self.mint.to_bytes(),
            token_owner_authority: self.authority.to_bytes(),
            delegate_or_zero: self.delegate.unwrap_or_default().to_bytes(),
            close_authority_or_zero: self.close_authority.unwrap_or_default().to_bytes(),
            amount: self.amount,
            delegated_amount: self.delegated_amount,
            native_reserve_or_zero: 0,
        }
    }

    pub fn lifecycle_digest(self) -> Result<[u8; 32]> {
        generic_effect_private_wire::compute_classic_spl_endpoint_state_digest(
            &self.wire_state_row(),
        )
        .map_err(|_| error!(CoreError::InvalidWireEncoding))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObservedClassicSplDelta {
    pub key: Pubkey,
    pub mint: Pubkey,
    pub amount_before: u64,
    pub amount_after: u64,
    pub expected_debit: u128,
    pub expected_credit: u128,
    pub delegate_after: Option<Pubkey>,
    pub delegated_amount_after: u64,
}

/// One already-authorized transfer. A direct signer uses `None`; a Core PDA
/// supplies exactly one seed slice. Multisig authority is not part of the first
/// protected profile.
pub struct ClassicSplTransfer<'a, 'info> {
    pub source: &'a AccountInfo<'info>,
    pub destination: &'a AccountInfo<'info>,
    pub mint: &'a AccountInfo<'info>,
    pub authority: &'a AccountInfo<'info>,
    pub amount: u64,
    pub authority_signer_seeds: Option<&'a [&'a [u8]]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ExpectedEndpointDelta {
    before: ClassicSplEndpointSnapshot,
    debits: u128,
    credits: u128,
}

pub fn load_classic_spl_mint(account: &AccountInfo<'_>) -> Result<ClassicSplMintSnapshot> {
    require_keys_eq!(
        *account.owner,
        spl_token::ID,
        CoreError::MoveAssetProfileMismatch
    );
    require!(!account.executable, CoreError::MoveAssetProfileMismatch);
    require_eq!(
        account.data_len(),
        SplMint::LEN,
        CoreError::MoveAssetProfileMismatch
    );
    let data = account
        .try_borrow_data()
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    let mint = SplMint::unpack(&data).map_err(|_| error!(CoreError::MoveAssetProfileMismatch))?;
    require!(mint.is_initialized, CoreError::MoveAssetProfileMismatch);
    Ok(ClassicSplMintSnapshot {
        key: *account.key,
        decimals: mint.decimals,
    })
}

pub fn load_classic_spl_endpoint(account: &AccountInfo<'_>) -> Result<ClassicSplEndpointSnapshot> {
    require_keys_eq!(
        *account.owner,
        spl_token::ID,
        CoreError::MoveAssetProfileMismatch
    );
    require!(!account.executable, CoreError::MoveAssetProfileMismatch);
    require_eq!(
        account.data_len(),
        SplTokenAccount::LEN,
        CoreError::MoveAssetProfileMismatch
    );
    let data = account
        .try_borrow_data()
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    let token =
        SplTokenAccount::unpack(&data).map_err(|_| error!(CoreError::MoveAssetProfileMismatch))?;
    require!(
        token.state == AccountState::Initialized,
        CoreError::MoveAssetProfileMismatch
    );
    // Native/WSOL lifecycle needs its own authority profile and accounting
    // rules. Accepting it here would make lamports an undeclared effect.
    require!(
        matches!(token.is_native, COption::None),
        CoreError::MoveAssetProfileMismatch
    );
    Ok(ClassicSplEndpointSnapshot {
        key: *account.key,
        mint: token.mint,
        authority: token.owner,
        amount: token.amount,
        delegate: match token.delegate {
            COption::Some(delegate) => Some(delegate),
            COption::None => None,
        },
        delegated_amount: token.delegated_amount,
        close_authority: match token.close_authority {
            COption::Some(authority) => Some(authority),
            COption::None => None,
        },
    })
}

/// Executes all accepted engine and Core-derived fee transfers, then reloads
/// every affected endpoint once and verifies the exact aggregate deltas.
///
/// The caller must pass the canonical order: accepted engine moves followed by
/// Core-derived fee moves. A failure at any CPI or postcondition rolls the
/// complete transaction back, including earlier engine mutations.
pub fn execute_classic_spl_transfers<'a, 'info>(
    token_program: &AccountInfo<'info>,
    transfers: &[ClassicSplTransfer<'a, 'info>],
) -> Result<Vec<ObservedClassicSplDelta>> {
    require_keys_eq!(
        *token_program.key,
        spl_token::ID,
        CoreError::MoveAssetProfileMismatch
    );
    require!(
        token_program.executable && !token_program.is_signer && !token_program.is_writable,
        CoreError::MoveAssetProfileMismatch
    );

    let mut endpoints: Vec<ExpectedEndpointDelta> = Vec::new();
    for transfer in transfers {
        require!(transfer.amount != 0, CoreError::ZeroMoveAmount);
        require_keys_neq!(
            *transfer.source.key,
            *transfer.destination.key,
            CoreError::MoveEndpointsIdentical
        );
        require!(
            transfer.source.is_writable && transfer.destination.is_writable,
            CoreError::UnexpectedWritablePrivilege
        );
        require!(
            !transfer.source.is_signer && !transfer.destination.is_signer,
            CoreError::UnexpectedSignerPrivilege
        );
        require!(
            !transfer.mint.is_signer && !transfer.mint.is_writable,
            CoreError::UnexpectedWritablePrivilege
        );

        let mint = load_classic_spl_mint(transfer.mint)?;
        let source = load_classic_spl_endpoint(transfer.source)?;
        let destination = load_classic_spl_endpoint(transfer.destination)?;
        require_keys_eq!(source.mint, mint.key, CoreError::MoveAssetProfileMismatch);
        require_keys_eq!(
            destination.mint,
            mint.key,
            CoreError::MoveAssetProfileMismatch
        );
        let authorized_as_owner = source.authority == *transfer.authority.key;
        let authorized_as_delegate = source.delegate == Some(*transfer.authority.key);
        require!(
            authorized_as_owner || authorized_as_delegate,
            CoreError::ExactDelegateConsumptionMismatch
        );
        require!(
            transfer.authority.is_signer || transfer.authority_signer_seeds.is_some(),
            CoreError::ExactDelegateConsumptionMismatch
        );

        add_expected_delta(&mut endpoints, source, u128::from(transfer.amount), 0)?;
        add_expected_delta(&mut endpoints, destination, 0, u128::from(transfer.amount))?;
    }

    // A delegate allowance is consumed across the aggregate source debit, not
    // revalidated independently per transfer.
    for transfer in transfers {
        let source = load_classic_spl_endpoint(transfer.source)?;
        if source.delegate == Some(*transfer.authority.key) {
            let aggregate = endpoints
                .iter()
                .find(|endpoint| endpoint.before.key == source.key)
                .ok_or(CoreError::ObservedProtectedDeltaMismatch)?;
            require!(
                aggregate.debits <= u128::from(source.delegated_amount),
                CoreError::ExactDelegateConsumptionMismatch
            );
        }
    }

    for transfer in transfers {
        let mint = load_classic_spl_mint(transfer.mint)?;
        let instruction = spl_token::instruction::transfer_checked(
            &spl_token::ID,
            transfer.source.key,
            transfer.mint.key,
            transfer.destination.key,
            transfer.authority.key,
            &[],
            transfer.amount,
            mint.decimals,
        )
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
        let infos = [
            transfer.source.clone(),
            transfer.mint.clone(),
            transfer.destination.clone(),
            transfer.authority.clone(),
            token_program.clone(),
        ];
        match transfer.authority_signer_seeds {
            Some(seeds) => invoke_signed(&instruction, &infos, &[seeds])?,
            None => invoke_signed(&instruction, &infos, &[])?,
        }
    }

    let mut observations = Vec::with_capacity(endpoints.len());
    for expected in endpoints {
        let account = transfers
            .iter()
            .flat_map(|transfer| [transfer.source, transfer.destination])
            .find(|account| *account.key == expected.before.key)
            .ok_or(CoreError::ObservedProtectedDeltaMismatch)?;
        let after = load_classic_spl_endpoint(account)?;
        require_keys_eq!(
            after.mint,
            expected.before.mint,
            CoreError::ObservedProtectedDeltaMismatch
        );
        let expected_after = u128::from(expected.before.amount)
            .checked_add(expected.credits)
            .and_then(|amount| amount.checked_sub(expected.debits))
            .ok_or(CoreError::ArithmeticOverflow)?;
        require_eq!(
            u128::from(after.amount),
            expected_after,
            CoreError::ObservedProtectedDeltaMismatch
        );
        observations.push(ObservedClassicSplDelta {
            key: expected.before.key,
            mint: expected.before.mint,
            amount_before: expected.before.amount,
            amount_after: after.amount,
            expected_debit: expected.debits,
            expected_credit: expected.credits,
            delegate_after: after.delegate,
            delegated_amount_after: after.delegated_amount,
        });
    }
    Ok(observations)
}

fn add_expected_delta(
    endpoints: &mut Vec<ExpectedEndpointDelta>,
    snapshot: ClassicSplEndpointSnapshot,
    debit: u128,
    credit: u128,
) -> Result<()> {
    if let Some(existing) = endpoints
        .iter_mut()
        .find(|existing| existing.before.key == snapshot.key)
    {
        require!(
            existing.before == snapshot,
            CoreError::DuplicateAccountIdentityDrift
        );
        existing.debits = existing
            .debits
            .checked_add(debit)
            .ok_or(CoreError::ArithmeticOverflow)?;
        existing.credits = existing
            .credits
            .checked_add(credit)
            .ok_or(CoreError::ArithmeticOverflow)?;
    } else {
        endpoints.push(ExpectedEndpointDelta {
            before: snapshot,
            debits: debit,
            credits: credit,
        });
    }
    Ok(())
}
