//! Disposable callback-capability helper for the generic-effect experiment.
//!
//! This fixture proves that the Core-derived callback signer may be forwarded
//! through the selected engine while remaining read-only and rightless. It is
//! private test machinery, not a protocol component or maintained interface.

use anchor_lang::{prelude::*, solana_program::program::set_return_data, AccountsExit};

declare_id!("3yS1JFVT284y8z1LC9MRoWxZjzFrdoD5axKsZiyMsfC7");

pub const PROBE_RETURN_MAGIC: [u8; 8] = *b"GECHELP0";
pub const PROBE_RETURN_DATA_LEN: usize = PROBE_RETURN_MAGIC.len() + 8 + 8 + 8;
pub const MAX_DESCENDANT_RETURN_DATA_LEN: usize = 1_024;

#[program]
pub mod callback_capability_probe {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, allowed_callback: Pubkey) -> Result<()> {
        require!(
            ctx.remaining_accounts.is_empty(),
            ProbeError::UnexpectedAccount
        );
        require!(
            allowed_callback != Pubkey::default(),
            ProbeError::InvalidCallback
        );
        require_keys_neq!(
            allowed_callback,
            ctx.accounts.helper_state.key(),
            ProbeError::InvalidCallback
        );
        require_keys_neq!(allowed_callback, crate::ID, ProbeError::InvalidCallback);

        let state = &mut ctx.accounts.helper_state;
        state.allowed_callback = allowed_callback;
        state.calls = 0;
        state.value = 0;
        state.descendant_receipt_sets = 0;
        Ok(())
    }

    /// Mutate helper-owned state after authenticating the exact callback
    /// capability. A conforming engine overwrites this descendant return data
    /// with its own receipt after the helper returns.
    pub fn increment(ctx: Context<CallbackUse>, amount: u64) -> Result<()> {
        validate_callback_use(&ctx)?;
        require!(amount > 0, ProbeError::InvalidAmount);

        let (next_calls, next_value, receipt_sets) =
            apply_increment(&mut ctx.accounts.helper_state, amount)?;
        set_return_data(&encode_probe_return_data(
            next_calls,
            next_value,
            receipt_sets,
        ));
        Ok(())
    }

    /// Serialize a mutation and then fail. The runtime must roll this write
    /// back together with every earlier mutation in the invocation tree.
    pub fn increment_and_fail(ctx: Context<CallbackUse>, amount: u64) -> Result<()> {
        validate_callback_use(&ctx)?;
        require!(amount > 0, ProbeError::InvalidAmount);

        let (next_calls, next_value, receipt_sets) =
            apply_increment(&mut ctx.accounts.helper_state, amount)?;
        set_return_data(&encode_probe_return_data(
            next_calls,
            next_value,
            receipt_sets,
        ));
        ctx.accounts.helper_state.exit(ctx.program_id)?;
        err!(ProbeError::DeliberateFailure)
    }

    /// Set caller-supplied return bytes as a descendant of the selected
    /// engine. Core must reject these bytes because the setter is this helper,
    /// not the selected engine. The counter makes rollback observable.
    pub fn set_descendant_receipt(ctx: Context<CallbackUse>, receipt_data: Vec<u8>) -> Result<()> {
        validate_callback_use(&ctx)?;
        require!(
            !receipt_data.is_empty() && receipt_data.len() <= MAX_DESCENDANT_RETURN_DATA_LEN,
            ProbeError::InvalidReturnData
        );

        let state = &mut ctx.accounts.helper_state;
        state.descendant_receipt_sets = state
            .descendant_receipt_sets
            .checked_add(1)
            .ok_or(ProbeError::ArithmeticOverflow)?;
        set_return_data(&receipt_data);
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = payer, space = 8 + HelperState::INIT_SPACE)]
    pub helper_state: Account<'info, HelperState>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CallbackUse<'info> {
    #[account(mut)]
    pub helper_state: Account<'info, HelperState>,
    /// CHECK: The handler pins this key to `helper_state.allowed_callback` and
    /// validates its exact inner-invocation privileges.
    pub callback_authority: UncheckedAccount<'info>,
}

#[account]
#[derive(InitSpace)]
pub struct HelperState {
    pub allowed_callback: Pubkey,
    pub calls: u64,
    pub value: u64,
    pub descendant_receipt_sets: u64,
}

fn validate_callback_use(ctx: &Context<CallbackUse>) -> Result<()> {
    require!(
        ctx.remaining_accounts.is_empty(),
        ProbeError::UnexpectedAccount
    );

    let callback = &ctx.accounts.callback_authority;
    require_keys_eq!(
        callback.key(),
        ctx.accounts.helper_state.allowed_callback,
        ProbeError::InvalidCallback
    );
    require!(callback.is_signer, ProbeError::CallbackNotSigner);
    require!(!callback.is_writable, ProbeError::CallbackWritable);
    require!(!callback.executable, ProbeError::CallbackExecutable);
    require_keys_neq!(
        callback.key(),
        ctx.accounts.helper_state.key(),
        ProbeError::InvalidCallback
    );
    Ok(())
}

fn apply_increment(state: &mut Account<HelperState>, amount: u64) -> Result<(u64, u64, u64)> {
    let next_calls = state
        .calls
        .checked_add(1)
        .ok_or(ProbeError::ArithmeticOverflow)?;
    let next_value = state
        .value
        .checked_add(amount)
        .ok_or(ProbeError::ArithmeticOverflow)?;

    state.calls = next_calls;
    state.value = next_value;
    Ok((next_calls, next_value, state.descendant_receipt_sets))
}

pub fn encode_probe_return_data(
    calls: u64,
    value: u64,
    descendant_receipt_sets: u64,
) -> [u8; PROBE_RETURN_DATA_LEN] {
    let mut data = [0_u8; PROBE_RETURN_DATA_LEN];
    data[..8].copy_from_slice(&PROBE_RETURN_MAGIC);
    data[8..16].copy_from_slice(&calls.to_le_bytes());
    data[16..24].copy_from_slice(&value.to_le_bytes());
    data[24..32].copy_from_slice(&descendant_receipt_sets.to_le_bytes());
    data
}

#[error_code]
pub enum ProbeError {
    #[msg("The callback helper received an undeclared account")]
    UnexpectedAccount,
    #[msg("The callback capability is invalid")]
    InvalidCallback,
    #[msg("The callback capability is not a signer in this invocation")]
    CallbackNotSigner,
    #[msg("The callback capability must remain read-only")]
    CallbackWritable,
    #[msg("The callback capability must not be executable")]
    CallbackExecutable,
    #[msg("The helper increment must be nonzero")]
    InvalidAmount,
    #[msg("The helper counter overflowed")]
    ArithmeticOverflow,
    #[msg("The descendant return-data payload is empty or too large")]
    InvalidReturnData,
    #[msg("The callback helper failed after serializing its mutation")]
    DeliberateFailure,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_data_is_distinctive_and_fixed_width() {
        let data = encode_probe_return_data(
            0x0102_0304_0506_0708,
            0x1112_1314_1516_1718,
            0x2122_2324_2526_2728,
        );

        assert_eq!(data.len(), PROBE_RETURN_DATA_LEN);
        assert_eq!(&data[..8], &PROBE_RETURN_MAGIC);
        assert_eq!(&data[8..16], &0x0102_0304_0506_0708_u64.to_le_bytes());
        assert_eq!(&data[16..24], &0x1112_1314_1516_1718_u64.to_le_bytes());
        assert_eq!(&data[24..32], &0x2122_2324_2526_2728_u64.to_le_bytes());
    }

    #[test]
    fn helper_state_layout_is_fixed_for_the_private_fixture() {
        assert_eq!(HelperState::INIT_SPACE, 32 + 8 + 8 + 8);
    }

    #[test]
    fn callback_probe_program_id_is_disposable_and_stable() {
        assert_eq!(crate::ID.to_bytes(), [44_u8; 32]);
        assert_eq!(
            crate::ID.to_string(),
            "3yS1JFVT284y8z1LC9MRoWxZjzFrdoD5axKsZiyMsfC7"
        );
    }
}
