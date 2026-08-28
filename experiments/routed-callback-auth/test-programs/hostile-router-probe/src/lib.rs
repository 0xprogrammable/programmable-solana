//! Disposable permissionless router and hostile CPI fixture.
//!
//! The router has no authority of its own. It forwards the exact ordered
//! remaining-account list with privileges normalized from the AccountInfos it
//! actually received. The negative modes deliberately try operations that must
//! fail without an inherited signer. This is experiment machinery, not an API.

use anchor_lang::{
    prelude::*,
    solana_program::{
        instruction::{AccountMeta, Instruction},
        program::invoke,
    },
    InstructionData,
};
use routed_callback_probe_wire::DISPOSABLE_CORE_PROGRAM_ID;

declare_id!("F62maceZqpLAayyBLsXNGdrmKg9cZWdpSDbzoHuNgk6Q");

pub const MAX_FORWARDED_ACCOUNTS: usize = 64;
pub const MAX_FORWARDED_INSTRUCTION_DATA_LEN: usize = 10_240;
const CLASSIC_TOKEN_TRANSFER_TAG: u8 = 3;
const CLASSIC_TOKEN_TRANSFER_DATA_LEN: usize = 1 + 8;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct RouteProbeArgs {
    pub mode: RouterMode,
    /// The exact instruction bytes passed to the fixed disposable Core.
    pub core_instruction_data: Vec<u8>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouterMode {
    ForwardOnce,
    ForwardTwice,
    AttemptSpendDrain {
        source_index: u8,
        destination_index: u8,
        spend_authority_index: u8,
        token_program_index: u8,
        amount: u64,
    },
    ForwardThenReuseCallback {
        helper_program_index: u8,
        helper_state_index: u8,
        callback_authority_index: u8,
        amount: u64,
    },
}

#[program]
pub mod hostile_router_probe {
    use super::*;

    pub fn route<'info>(ctx: Context<'info, Route<'info>>, args: RouteProbeArgs) -> Result<()> {
        validate_router_envelope(&ctx)?;
        let RouteProbeArgs {
            mode,
            core_instruction_data,
        } = args;

        match mode {
            RouterMode::ForwardOnce => forward_core(
                &ctx.accounts.core_program.to_account_info(),
                ctx.remaining_accounts,
                &core_instruction_data,
            ),
            RouterMode::ForwardTwice => {
                forward_core(
                    &ctx.accounts.core_program.to_account_info(),
                    ctx.remaining_accounts,
                    &core_instruction_data,
                )?;
                forward_core(
                    &ctx.accounts.core_program.to_account_info(),
                    ctx.remaining_accounts,
                    &core_instruction_data,
                )?;
                err!(RouterError::ReplayUnexpectedlySucceeded)
            }
            RouterMode::AttemptSpendDrain {
                source_index,
                destination_index,
                spend_authority_index,
                token_program_index,
                amount,
            } => attempt_spend_drain(
                ctx.remaining_accounts,
                source_index,
                destination_index,
                spend_authority_index,
                token_program_index,
                amount,
            ),
            RouterMode::ForwardThenReuseCallback {
                helper_program_index,
                helper_state_index,
                callback_authority_index,
                amount,
            } => {
                forward_core(
                    &ctx.accounts.core_program.to_account_info(),
                    ctx.remaining_accounts,
                    &core_instruction_data,
                )?;
                attempt_callback_reuse(
                    ctx.remaining_accounts,
                    helper_program_index,
                    helper_state_index,
                    callback_authority_index,
                    amount,
                )
            }
        }
    }
}

#[derive(Accounts)]
pub struct Route<'info> {
    /// CHECK: This is the one fixed disposable Core target. The handler checks
    /// its executable and privilege flags before forwarding anything.
    #[account(address = DISPOSABLE_CORE_PROGRAM_ID @ RouterError::InvalidCoreProgram)]
    pub core_program: UncheckedAccount<'info>,
}

fn validate_router_envelope(ctx: &Context<Route>) -> Result<()> {
    let core_program = &ctx.accounts.core_program;
    require!(core_program.executable, RouterError::InvalidCoreProgram);
    require!(
        !core_program.is_signer && !core_program.is_writable,
        RouterError::InvalidCoreProgram
    );
    require!(
        ctx.remaining_accounts.len() <= MAX_FORWARDED_ACCOUNTS,
        RouterError::TooManyForwardedAccounts
    );
    Ok(())
}

fn forward_core<'info>(
    core_program: &AccountInfo<'info>,
    accounts: &[AccountInfo<'info>],
    instruction_data: &[u8],
) -> Result<()> {
    require!(
        !instruction_data.is_empty()
            && instruction_data.len() <= MAX_FORWARDED_INSTRUCTION_DATA_LEN,
        RouterError::InvalidCoreInstructionData
    );

    let instruction = Instruction {
        program_id: DISPOSABLE_CORE_PROGRAM_ID,
        accounts: normalized_metas(accounts),
        data: instruction_data.to_vec(),
    };
    let mut infos = Vec::with_capacity(accounts.len() + 1);
    infos.extend(accounts.iter().cloned());
    infos.push(core_program.clone());
    invoke(&instruction, &infos).map_err(Into::into)
}

fn attempt_spend_drain<'info>(
    accounts: &[AccountInfo<'info>],
    source_index: u8,
    destination_index: u8,
    spend_authority_index: u8,
    token_program_index: u8,
    amount: u64,
) -> Result<()> {
    require!(amount > 0, RouterError::InvalidProbeAmount);
    let source = account_at(accounts, source_index)?;
    let destination = account_at(accounts, destination_index)?;
    let spend_authority = account_at(accounts, spend_authority_index)?;
    let token_program = account_at(accounts, token_program_index)?;
    require_distinct(&[
        source.key(),
        destination.key(),
        spend_authority.key(),
        token_program.key(),
    ])?;

    require!(
        source.is_writable && !source.is_signer && !source.executable,
        RouterError::InvalidDrainClosure
    );
    require!(
        destination.is_writable && !destination.is_signer && !destination.executable,
        RouterError::InvalidDrainClosure
    );
    require!(
        !spend_authority.is_writable && !spend_authority.is_signer && !spend_authority.executable,
        RouterError::SpendAuthorityUnexpectedlyPrivileged
    );
    require_keys_eq!(
        token_program.key(),
        anchor_spl::token::ID,
        RouterError::InvalidTokenProgram
    );
    require!(
        token_program.executable && !token_program.is_writable && !token_program.is_signer,
        RouterError::InvalidTokenProgram
    );

    // `Transfer` is tag 3 followed by one little-endian u64. Crucially, every
    // AccountMeta below reflects the privilege visible to this router. The
    // spend authority remains non-signer, so the Token Program must reject it.
    let mut data = [0_u8; CLASSIC_TOKEN_TRANSFER_DATA_LEN];
    data[0] = CLASSIC_TOKEN_TRANSFER_TAG;
    data[1..].copy_from_slice(&amount.to_le_bytes());
    let instruction = Instruction {
        program_id: anchor_spl::token::ID,
        accounts: vec![
            normalized_meta(source, accounts),
            normalized_meta(destination, accounts),
            normalized_meta(spend_authority, accounts),
        ],
        data: data.to_vec(),
    };
    let infos = [
        source.clone(),
        destination.clone(),
        spend_authority.clone(),
        token_program.clone(),
    ];
    invoke(&instruction, &infos)?;
    err!(RouterError::SpendDrainUnexpectedlySucceeded)
}

fn attempt_callback_reuse<'info>(
    accounts: &[AccountInfo<'info>],
    helper_program_index: u8,
    helper_state_index: u8,
    callback_authority_index: u8,
    amount: u64,
) -> Result<()> {
    require!(amount > 0, RouterError::InvalidProbeAmount);
    let helper_program = account_at(accounts, helper_program_index)?;
    let helper_state = account_at(accounts, helper_state_index)?;
    let callback_authority = account_at(accounts, callback_authority_index)?;
    require_distinct(&[
        helper_program.key(),
        helper_state.key(),
        callback_authority.key(),
    ])?;

    require_keys_eq!(
        helper_program.key(),
        callback_capability_probe::ID,
        RouterError::InvalidHelperProgram
    );
    require!(
        helper_program.executable && !helper_program.is_writable && !helper_program.is_signer,
        RouterError::InvalidHelperProgram
    );
    require!(
        helper_state.is_writable && !helper_state.is_signer && !helper_state.executable,
        RouterError::InvalidHelperState
    );
    require_keys_eq!(
        *helper_state.owner,
        callback_capability_probe::ID,
        RouterError::InvalidHelperState
    );
    require!(
        !callback_authority.is_writable
            && !callback_authority.is_signer
            && !callback_authority.executable,
        RouterError::CallbackSignerLeaked
    );

    let instruction = Instruction {
        program_id: callback_capability_probe::ID,
        accounts: vec![
            normalized_meta(helper_state, accounts),
            normalized_meta(callback_authority, accounts),
        ],
        data: callback_capability_probe::instruction::Increment { amount }.data(),
    };
    let infos = [
        helper_state.clone(),
        callback_authority.clone(),
        helper_program.clone(),
    ];
    invoke(&instruction, &infos)?;
    err!(RouterError::CallbackReuseUnexpectedlySucceeded)
}

fn normalized_metas(accounts: &[AccountInfo<'_>]) -> Vec<AccountMeta> {
    accounts
        .iter()
        .map(|account| normalized_meta(account, accounts))
        .collect()
}

fn normalized_meta(account: &AccountInfo<'_>, accounts: &[AccountInfo<'_>]) -> AccountMeta {
    let (is_writable, is_signer) = accounts
        .iter()
        .filter(|candidate| candidate.key == account.key)
        .fold((false, false), |(writable, signer), candidate| {
            (
                writable || candidate.is_writable,
                signer || candidate.is_signer,
            )
        });
    if is_writable {
        AccountMeta::new(*account.key, is_signer)
    } else {
        AccountMeta::new_readonly(*account.key, is_signer)
    }
}

fn account_at<'a, 'info>(
    accounts: &'a [AccountInfo<'info>],
    index: u8,
) -> Result<&'a AccountInfo<'info>> {
    accounts
        .get(usize::from(index))
        .ok_or_else(|| error!(RouterError::InvalidAccountIndex))
}

fn require_distinct(keys: &[Pubkey]) -> Result<()> {
    for (index, key) in keys.iter().enumerate() {
        require!(
            !keys[index + 1..].contains(key),
            RouterError::AliasedAttackRole
        );
    }
    Ok(())
}

#[error_code]
pub enum RouterError {
    #[msg("The router target is not the fixed disposable Core")]
    InvalidCoreProgram,
    #[msg("The forwarded account list exceeds the disposable router bound")]
    TooManyForwardedAccounts,
    #[msg("The serialized Core instruction is empty or exceeds the CPI data limit")]
    InvalidCoreInstructionData,
    #[msg("The hostile mode references an unavailable remaining account")]
    InvalidAccountIndex,
    #[msg("Two hostile-probe roles alias the same public key")]
    AliasedAttackRole,
    #[msg("The hostile probe amount must be nonzero")]
    InvalidProbeAmount,
    #[msg("The direct token-drain closure has invalid privileges")]
    InvalidDrainClosure,
    #[msg("The spend authority unexpectedly arrived with signer or writable privilege")]
    SpendAuthorityUnexpectedlyPrivileged,
    #[msg("The classic SPL Token Program is invalid")]
    InvalidTokenProgram,
    #[msg("The second Core execution unexpectedly succeeded")]
    ReplayUnexpectedlySucceeded,
    #[msg("The direct token drain unexpectedly succeeded without a signer")]
    SpendDrainUnexpectedlySucceeded,
    #[msg("The callback capability probe program is invalid")]
    InvalidHelperProgram,
    #[msg("The callback capability probe state is invalid")]
    InvalidHelperState,
    #[msg("The callback signer privilege leaked back into the router frame")]
    CallbackSignerLeaked,
    #[msg("The callback capability was reusable after the Core returned")]
    CallbackReuseUnexpectedlySucceeded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_arguments_round_trip_for_every_mode() {
        let modes = [
            RouterMode::ForwardOnce,
            RouterMode::ForwardTwice,
            RouterMode::AttemptSpendDrain {
                source_index: 1,
                destination_index: 2,
                spend_authority_index: 3,
                token_program_index: 4,
                amount: 5,
            },
            RouterMode::ForwardThenReuseCallback {
                helper_program_index: 6,
                helper_state_index: 7,
                callback_authority_index: 8,
                amount: 9,
            },
        ];

        for mode in modes {
            let expected = RouteProbeArgs {
                mode,
                core_instruction_data: vec![0x11, 0x22, 0x33],
            };
            let mut encoded = Vec::new();
            expected.serialize(&mut encoded).unwrap();
            let decoded = RouteProbeArgs::deserialize(&mut encoded.as_slice()).unwrap();
            assert_eq!(decoded, expected);
        }
    }

    #[test]
    fn classic_transfer_probe_encoding_is_fixed() {
        let amount = 0x0102_0304_0506_0708_u64;
        let mut data = [0_u8; CLASSIC_TOKEN_TRANSFER_DATA_LEN];
        data[0] = CLASSIC_TOKEN_TRANSFER_TAG;
        data[1..].copy_from_slice(&amount.to_le_bytes());

        assert_eq!(data, [3, 8, 7, 6, 5, 4, 3, 2, 1]);
    }

    #[test]
    fn router_program_id_is_stable() {
        assert_eq!(
            crate::ID.to_string(),
            "F62maceZqpLAayyBLsXNGdrmKg9cZWdpSDbzoHuNgk6Q"
        );
    }
}
