//! Disposable external program used to prove callback-signer forwarding.
//!
//! This fixture is not a protocol component or a public interface.

use anchor_lang::{prelude::*, solana_program::program::set_return_data, AccountsExit};

declare_id!("6QXXm7aqjRxQGJ6V3nvtS5taHuojM9SisVrHg3Xrj1Vj");

pub const PROBE_RETURN_MAGIC: [u8; 8] = *b"CBKPRB00";
pub const PROBE_RETURN_DATA_LEN: usize = PROBE_RETURN_MAGIC.len() + 8 + 8;

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
        Ok(())
    }

    pub fn increment(ctx: Context<Increment>, amount: u64) -> Result<()> {
        validate_increment(&ctx, amount)?;
        let (next_calls, next_value) = apply_increment(&mut ctx.accounts.helper_state, amount)?;
        set_return_data(&encode_probe_return_data(next_calls, next_value));
        Ok(())
    }

    /// Mutates and serializes the helper before deliberately failing. The
    /// transaction runtime must roll the write back with the rest of the call
    /// tree; this is a fixture for atomicity tests, not a catchable error path.
    pub fn increment_and_fail(ctx: Context<Increment>, amount: u64) -> Result<()> {
        validate_increment(&ctx, amount)?;
        let (next_calls, next_value) = apply_increment(&mut ctx.accounts.helper_state, amount)?;
        set_return_data(&encode_probe_return_data(next_calls, next_value));
        ctx.accounts.helper_state.exit(ctx.program_id)?;
        err!(ProbeError::DeliberateFailure)
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
pub struct Increment<'info> {
    #[account(mut)]
    pub helper_state: Account<'info, HelperState>,
    /// CHECK: The handler pins this key to `helper_state.allowed_callback` and
    /// requires the effective inner-CPI privilege to be signer but read-only.
    pub callback_authority: UncheckedAccount<'info>,
}

#[account]
#[derive(InitSpace)]
pub struct HelperState {
    pub allowed_callback: Pubkey,
    pub calls: u64,
    pub value: u64,
}

fn validate_increment(ctx: &Context<Increment>, amount: u64) -> Result<()> {
    require!(
        ctx.remaining_accounts.is_empty(),
        ProbeError::UnexpectedAccount
    );
    require!(amount > 0, ProbeError::InvalidAmount);

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

fn apply_increment(state: &mut Account<HelperState>, amount: u64) -> Result<(u64, u64)> {
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
    Ok((next_calls, next_value))
}

pub fn encode_probe_return_data(calls: u64, value: u64) -> [u8; PROBE_RETURN_DATA_LEN] {
    let mut data = [0_u8; PROBE_RETURN_DATA_LEN];
    data[..PROBE_RETURN_MAGIC.len()].copy_from_slice(&PROBE_RETURN_MAGIC);
    data[8..16].copy_from_slice(&calls.to_le_bytes());
    data[16..24].copy_from_slice(&value.to_le_bytes());
    data
}

#[error_code]
pub enum ProbeError {
    #[msg("The callback capability probe received an undeclared account")]
    UnexpectedAccount,
    #[msg("The callback capability is invalid")]
    InvalidCallback,
    #[msg("The callback capability was not a signer in this invocation")]
    CallbackNotSigner,
    #[msg("The callback capability must remain read-only")]
    CallbackWritable,
    #[msg("The callback capability must not be executable")]
    CallbackExecutable,
    #[msg("The helper increment must be nonzero")]
    InvalidAmount,
    #[msg("The helper counter overflowed")]
    ArithmeticOverflow,
    #[msg("The callback capability probe failed after serializing its mutation")]
    DeliberateFailure,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_data_is_distinctive_and_fixed_width() {
        let data = encode_probe_return_data(0x0102_0304_0506_0708, 0x1112_1314_1516_1718);

        assert_eq!(data.len(), PROBE_RETURN_DATA_LEN);
        assert_eq!(&data[..8], &PROBE_RETURN_MAGIC);
        assert_eq!(&data[8..16], &0x0102_0304_0506_0708_u64.to_le_bytes());
        assert_eq!(&data[16..24], &0x1112_1314_1516_1718_u64.to_le_bytes());
    }

    #[test]
    fn helper_state_layout_has_one_callback_and_two_counters() {
        assert_eq!(HelperState::INIT_SPACE, 32 + 8 + 8);
    }

    #[test]
    fn callback_probe_program_id_is_stable() {
        assert_eq!(
            crate::ID.to_string(),
            "6QXXm7aqjRxQGJ6V3nvtS5taHuojM9SisVrHg3Xrj1Vj"
        );
    }
}
