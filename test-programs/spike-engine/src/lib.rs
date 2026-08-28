//! Disposable engine used to falsify the Core authority-boundary design.
//!
//! This is a hostile test fixture, not a production market implementation.

use anchor_lang::{
    prelude::*,
    solana_program::{
        instruction::{get_stack_height, AccountMeta, Instruction, TRANSACTION_LEVEL_STACK_HEIGHT},
        program::{invoke, set_return_data},
    },
    AccountsExit, InstructionData,
};
use engine_probe_interface::{
    decode_request, encode_receipt, EngineReceipt, CORE_EXECUTE_ENGINE_PROBE_DISCRIMINATOR,
    DISPOSABLE_CORE_PROGRAM_ID, ENGINE_REQUEST_LEN,
};
use solana_instructions_sysvar::{load_current_index_checked, load_instruction_at_checked};

declare_id!("HAhZQp2iaVWfP2mbSpSJvwMqUGaES4S5i4PxwHr6bNkQ");

pub const MODE_ACCEPT: u8 = 0;
pub const MODE_HOSTILE_READONLY_ESCALATION: u8 = 1;
pub const MODE_MISSING_RECEIPT: u8 = 2;
pub const MODE_WRONG_PLAN_HASH: u8 = 3;
pub const MODE_MALFORMED_RECEIPT: u8 = 4;

const _: [(); ENGINE_REQUEST_LEN] = [(); 153];

#[program]
pub mod programmable_spike_engine {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, market: Pubkey, revision: u64) -> Result<()> {
        require!(
            ctx.remaining_accounts.is_empty(),
            ProbeEngineError::UnexpectedAccount
        );
        require!(market != Pubkey::default(), ProbeEngineError::InvalidMarket);
        require!(revision != 0, ProbeEngineError::InvalidRevision);

        let state = &mut ctx.accounts.engine_state;
        state.authority = ctx.accounts.authority.key();
        state.market = market;
        state.revision = revision;
        state.sequence = 0;
        state.last_plan_hash = [0; 32];
        state.mode = MODE_ACCEPT;
        Ok(())
    }

    pub fn set_mode(ctx: Context<SetMode>, mode: u8) -> Result<()> {
        require!(
            ctx.remaining_accounts.is_empty(),
            ProbeEngineError::UnexpectedAccount
        );
        require!(
            matches!(
                mode,
                MODE_ACCEPT
                    | MODE_HOSTILE_READONLY_ESCALATION
                    | MODE_MISSING_RECEIPT
                    | MODE_WRONG_PLAN_HASH
                    | MODE_MALFORMED_RECEIPT
            ),
            ProbeEngineError::InvalidMode
        );
        ctx.accounts.engine_state.mode = mode;
        Ok(())
    }

    pub fn evaluate(ctx: Context<Evaluate>, wire_request: [u8; 153]) -> Result<()> {
        require!(
            ctx.remaining_accounts.is_empty(),
            ProbeEngineError::UnexpectedAccount
        );
        require_direct_core_cpi(&ctx.accounts.instructions.to_account_info())?;

        let request = decode_request(&wire_request)
            .map_err(|_| error!(ProbeEngineError::InvalidRequestEncoding))?;
        let state = &mut ctx.accounts.engine_state;
        require_keys_eq!(
            state.market,
            request.market,
            ProbeEngineError::MarketMismatch
        );
        require_eq!(
            state.revision,
            request.engine_revision,
            ProbeEngineError::RevisionMismatch
        );

        state.sequence = state
            .sequence
            .checked_add(1)
            .ok_or(ProbeEngineError::SequenceOverflow)?;
        state.last_plan_hash = request.plan_hash;

        let receipt = encode_receipt(&EngineReceipt {
            plan_hash: request.plan_hash,
            state_sequence: state.sequence,
        });

        match state.mode {
            MODE_ACCEPT => {
                set_return_data(&receipt);
                Ok(())
            }
            MODE_HOSTILE_READONLY_ESCALATION => {
                // Persist the mutation before the deliberately invalid CPI. A runtime
                // rejection must roll this write back atomically with the Core call.
                state.exit(ctx.program_id)?;
                attempt_readonly_to_writable_self_cpi(
                    &ctx.accounts.instructions.to_account_info(),
                )?;
                err!(ProbeEngineError::HostileCpiUnexpectedlySucceeded)
            }
            MODE_MISSING_RECEIPT => Ok(()),
            MODE_WRONG_PLAN_HASH => {
                let mut wrong_receipt = receipt;
                wrong_receipt[9] ^= 1;
                set_return_data(&wrong_receipt);
                Ok(())
            }
            MODE_MALFORMED_RECEIPT => {
                set_return_data(&receipt[..receipt.len() - 1]);
                Ok(())
            }
            _ => err!(ProbeEngineError::InvalidMode),
        }
    }

    pub fn hostile_escalation_probe(_ctx: Context<HostileEscalationProbe>) -> Result<()> {
        err!(ProbeEngineError::HostileCpiUnexpectedlySucceeded)
    }
}

fn require_direct_core_cpi(instructions: &AccountInfo<'_>) -> Result<()> {
    let expected_height = TRANSACTION_LEVEL_STACK_HEIGHT + 1;
    require_eq!(
        get_stack_height(),
        expected_height,
        ProbeEngineError::InvalidInvocationDepth
    );

    let current_index = load_current_index_checked(instructions)
        .map_err(|_| error!(ProbeEngineError::InvalidInstructionsSysvar))?;
    let top_level_instruction = load_instruction_at_checked(current_index.into(), instructions)
        .map_err(|_| error!(ProbeEngineError::InvalidInstructionsSysvar))?;
    require_keys_eq!(
        top_level_instruction.program_id,
        DISPOSABLE_CORE_PROGRAM_ID,
        ProbeEngineError::InvalidCoreCaller
    );
    require!(
        top_level_instruction
            .data
            .starts_with(&CORE_EXECUTE_ENGINE_PROBE_DISCRIMINATOR),
        ProbeEngineError::InvalidCoreInstruction
    );
    Ok(())
}

fn attempt_readonly_to_writable_self_cpi(instructions: &AccountInfo<'_>) -> Result<()> {
    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![AccountMeta::new(*instructions.key, false)],
        data: crate::instruction::HostileEscalationProbe {}.data(),
    };

    invoke(&instruction, core::slice::from_ref(instructions)).map_err(Into::into)
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = authority, space = 8 + EngineState::INIT_SPACE)]
    pub engine_state: Account<'info, EngineState>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SetMode<'info> {
    #[account(mut, has_one = authority @ ProbeEngineError::InvalidAuthority)]
    pub engine_state: Account<'info, EngineState>,
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct Evaluate<'info> {
    #[account(mut)]
    pub engine_state: Account<'info, EngineState>,
    /// CHECK: The address constraint pins the canonical introspection sysvar.
    #[account(address = solana_instructions_sysvar::ID)]
    pub instructions: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct HostileEscalationProbe<'info> {
    /// CHECK: This instruction must never pass the runtime privilege check.
    #[account(mut, address = solana_instructions_sysvar::ID)]
    pub instructions: UncheckedAccount<'info>,
}

#[account]
#[derive(InitSpace)]
pub struct EngineState {
    pub authority: Pubkey,
    pub market: Pubkey,
    pub revision: u64,
    pub sequence: u64,
    pub last_plan_hash: [u8; 32],
    pub mode: u8,
}

#[error_code]
pub enum ProbeEngineError {
    #[msg("The probe received an account outside its fixed closure")]
    UnexpectedAccount,
    #[msg("The market address is invalid")]
    InvalidMarket,
    #[msg("The engine revision is invalid")]
    InvalidRevision,
    #[msg("The caller is not the disposable Core probe")]
    InvalidCoreCaller,
    #[msg("The Core call is not the exact engine-probe route")]
    InvalidCoreInstruction,
    #[msg("The probe must be called by one direct Core CPI")]
    InvalidInvocationDepth,
    #[msg("The instructions sysvar could not be introspected")]
    InvalidInstructionsSysvar,
    #[msg("The request codec rejected the payload")]
    InvalidRequestEncoding,
    #[msg("The request market does not match engine state")]
    MarketMismatch,
    #[msg("The request revision does not match engine state")]
    RevisionMismatch,
    #[msg("The engine sequence overflowed")]
    SequenceOverflow,
    #[msg("The authority does not control this engine state")]
    InvalidAuthority,
    #[msg("The requested probe mode is invalid")]
    InvalidMode,
    #[msg("The runtime unexpectedly allowed writable privilege escalation")]
    HostileCpiUnexpectedlySucceeded,
}
