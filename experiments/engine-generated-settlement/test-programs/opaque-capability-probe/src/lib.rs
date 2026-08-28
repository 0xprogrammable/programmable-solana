//! Disposable external program used to prove an opaque engine capability CPI.
//!
//! This fixture is not a protocol component or a public interface.

use anchor_lang::{prelude::*, solana_program::program::set_return_data};

declare_id!("EsZGEzu3NgpwumgwdsjxW3c6xB9wR6gy3qj9Y86nZ7Uv");

pub const PROBE_RETURN_MAGIC: [u8; 8] = *b"OPAQPRB0";
pub const PROBE_RETURN_DATA_LEN: usize = PROBE_RETURN_MAGIC.len() + 8 + 8;

#[program]
pub mod opaque_capability_probe {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, authority: Pubkey) -> Result<()> {
        require!(
            ctx.remaining_accounts.is_empty(),
            ProbeError::UnexpectedAccount
        );
        require!(authority != Pubkey::default(), ProbeError::InvalidAuthority);

        let state = &mut ctx.accounts.helper_state;
        state.authority = authority;
        state.calls = 0;
        state.value = 0;
        Ok(())
    }

    pub fn increment(ctx: Context<Increment>, amount: u64) -> Result<()> {
        require!(
            ctx.remaining_accounts.is_empty(),
            ProbeError::UnexpectedAccount
        );

        let state = &mut ctx.accounts.helper_state;
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
        set_return_data(&encode_probe_return_data(next_calls, next_value));
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
pub struct Increment<'info> {
    #[account(mut, has_one = authority @ ProbeError::InvalidAuthority)]
    pub helper_state: Account<'info, HelperState>,
    pub authority: Signer<'info>,
}

#[account]
#[derive(InitSpace)]
pub struct HelperState {
    pub authority: Pubkey,
    pub calls: u64,
    pub value: u64,
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
    #[msg("The opaque capability probe received an undeclared account")]
    UnexpectedAccount,
    #[msg("The signer does not match the stored helper authority")]
    InvalidAuthority,
    #[msg("The helper counter overflowed")]
    ArithmeticOverflow,
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
}
