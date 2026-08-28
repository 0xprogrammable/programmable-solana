//! Disposable CPMM engine for the engine-generated settlement experiment.
//!
//! This program exists to prove that an engine can derive an economic result
//! from Core-accounted reserves while receiving only a declared capability
//! closure. It is not a production market implementation or a public ABI.

use anchor_lang::{
    prelude::*,
    solana_program::{
        instruction::{get_stack_height, AccountMeta, Instruction, TRANSACTION_LEVEL_STACK_HEIGHT},
        program::{invoke, invoke_signed, set_return_data},
    },
    AccountsExit, InstructionData,
};
use generated_settlement_probe_wire::{
    compute_capability_hash, decode_request, encode_receipt, CapabilityDescriptor, EngineReceipt,
    CORE_EXECUTE_ENGINE_GENERATED_PROBE_DISCRIMINATOR, DISPOSABLE_CORE_PROGRAM_ID,
    DISPOSABLE_HELPER_PROGRAM_ID, ENGINE_REQUEST_LEN, MAX_OPAQUE_ACCOUNTS, MAX_OPAQUE_PAYLOAD_LEN,
};
use solana_instructions_sysvar::{load_current_index_checked, load_instruction_at_checked};

declare_id!("EAX2oQEejkYYTxaVCbQ3pfy9bySj3WMwtV36gvf77Mj1");

pub const CAPABILITY_AUTHORITY_SEED: &[u8] = b"capability-authority-v0";
pub const BASIS_POINTS_DENOMINATOR: u128 = 10_000;
pub const MAX_OPAQUE_CAPABILITIES: usize = MAX_OPAQUE_ACCOUNTS;

pub const MODE_ACCEPT: u8 = 0;
pub const MODE_MISSING_RECEIPT: u8 = 1;
pub const MODE_WRONG_REQUEST_HASH: u8 = 2;
pub const MODE_MALFORMED_RECEIPT: u8 = 3;
pub const MODE_ZERO_OUTPUT: u8 = 4;
pub const MODE_OVERSIZED_OUTPUT: u8 = 5;
pub const MODE_HOSTILE_READONLY_ESCALATION: u8 = 6;
pub const MODE_WRONG_RECEIPT_MAGIC: u8 = 7;
pub const MODE_WRONG_RECEIPT_VERSION: u8 = 8;
pub const MODE_TRAILING_RECEIPT_BYTE: u8 = 9;

pub const HELPER_PAYLOAD_TAG: u8 = 1;
pub const HELPER_INCREMENT_PAYLOAD_LEN: usize = 1 + 8;
const HELPER_INCREMENT_DISCRIMINATOR: [u8; 8] = [0x0b, 0x12, 0x68, 0x09, 0x68, 0xae, 0x3b, 0x21];
const HELPER_INCREMENT_DATA_LEN: usize = HELPER_INCREMENT_DISCRIMINATOR.len() + 8;
const _: [(); ENGINE_REQUEST_LEN] = [(); 293];

#[program]
pub mod generated_plan_engine {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        market: Pubkey,
        revision: u64,
        lp_fee_bps: u16,
    ) -> Result<()> {
        require!(
            ctx.remaining_accounts.is_empty(),
            EngineError::UnexpectedAccount
        );
        require!(market != Pubkey::default(), EngineError::InvalidMarket);
        require!(revision != 0, EngineError::InvalidRevision);
        require!(
            u128::from(lp_fee_bps) < BASIS_POINTS_DENOMINATOR,
            EngineError::InvalidLpFeeBps
        );

        let engine_state_key = ctx.accounts.engine_state.key();
        ctx.accounts.capability_authority.engine_state = engine_state_key;

        let state = &mut ctx.accounts.engine_state;
        state.authority = ctx.accounts.authority.key();
        state.market = market;
        state.revision = revision;
        state.lp_fee_bps = lp_fee_bps;
        state.sequence = 0;
        state.last_request_hash = [0; 32];
        state.last_amount_out = 0;
        state.capability_authority_bump = ctx.bumps.capability_authority;
        state.mode = MODE_ACCEPT;
        Ok(())
    }

    pub fn set_mode(ctx: Context<SetMode>, mode: u8) -> Result<()> {
        require!(
            ctx.remaining_accounts.is_empty(),
            EngineError::UnexpectedAccount
        );
        require!(is_supported_mode(mode), EngineError::InvalidMode);
        ctx.accounts.engine_state.mode = mode;
        Ok(())
    }

    pub fn evaluate(ctx: Context<Evaluate>, wire_request: [u8; 293]) -> Result<()> {
        require_direct_core_cpi(&ctx.accounts.instructions_sysvar.to_account_info())?;

        let request = decode_request(&wire_request)
            .map_err(|_| error!(EngineError::InvalidRequestEncoding))?;
        let remaining_count = ctx.remaining_accounts.len();
        require!(
            remaining_count <= MAX_OPAQUE_CAPABILITIES,
            EngineError::TooManyCapabilities
        );
        require_eq!(
            remaining_count,
            usize::from(request.opaque_account_count),
            EngineError::CapabilityCountMismatch
        );

        let state = &ctx.accounts.engine_state;
        require_keys_eq!(state.market, request.market, EngineError::MarketMismatch);
        require_eq!(
            state.revision,
            request.engine_revision,
            EngineError::RevisionMismatch
        );

        let mut descriptors = Vec::with_capacity(remaining_count + 2);
        descriptors.push(capability_descriptor(
            &ctx.accounts.engine_state.to_account_info(),
        ));
        descriptors.push(capability_descriptor(
            &ctx.accounts.instructions_sysvar.to_account_info(),
        ));
        descriptors.extend(ctx.remaining_accounts.iter().map(capability_descriptor));
        let observed_capability_hash = compute_capability_hash(&crate::ID, &descriptors)
            .map_err(|_| error!(EngineError::CapabilityHashComputationFailed))?;
        require!(
            observed_capability_hash == request.capability_hash,
            EngineError::CapabilityHashMismatch
        );

        let quote = quote_exact_in(
            request.accounted_input_before,
            request.accounted_output_before,
            request.amount_in,
            state.lp_fee_bps,
        )?;
        let mut amount_out = quote.amount_out;
        match state.mode {
            MODE_ZERO_OUTPUT => amount_out = 0,
            MODE_OVERSIZED_OUTPUT => amount_out = u64::MAX,
            _ => {}
        }

        maybe_invoke_helper(
            request
                .payload_bytes()
                .map_err(|_| error!(EngineError::InvalidRequestEncoding))?,
            ctx.remaining_accounts,
            &ctx.accounts.engine_state.key(),
            state.capability_authority_bump,
        )?;

        if state.mode == MODE_HOSTILE_READONLY_ESCALATION {
            let state = &mut ctx.accounts.engine_state;
            state.sequence = state
                .sequence
                .checked_add(1)
                .ok_or(EngineError::SequenceOverflow)?;
            state.last_request_hash = request.request_hash;
            state.last_amount_out = amount_out;
            state.exit(ctx.program_id)?;
            attempt_readonly_to_writable_self_cpi(
                &ctx.accounts.instructions_sysvar.to_account_info(),
            )?;
            return err!(EngineError::HostileCpiUnexpectedlySucceeded);
        }

        let state = &mut ctx.accounts.engine_state;
        state.sequence = state
            .sequence
            .checked_add(1)
            .ok_or(EngineError::SequenceOverflow)?;
        state.last_request_hash = request.request_hash;
        state.last_amount_out = amount_out;

        let receipt = encode_receipt(&EngineReceipt {
            request_hash: request.request_hash,
            amount_out,
            state_sequence: state.sequence,
        });
        match state.mode {
            MODE_ACCEPT | MODE_ZERO_OUTPUT | MODE_OVERSIZED_OUTPUT => {
                // This must occur after every nested CPI because nested programs may
                // overwrite the single transaction return-data slot.
                set_return_data(&receipt);
                Ok(())
            }
            MODE_MISSING_RECEIPT => Ok(()),
            MODE_WRONG_REQUEST_HASH => {
                let mut wrong_receipt = receipt;
                wrong_receipt[9] ^= 1;
                set_return_data(&wrong_receipt);
                Ok(())
            }
            MODE_MALFORMED_RECEIPT => {
                set_return_data(&receipt[..receipt.len() - 1]);
                Ok(())
            }
            MODE_WRONG_RECEIPT_MAGIC => {
                let mut wrong_receipt = receipt;
                wrong_receipt[0] ^= 1;
                set_return_data(&wrong_receipt);
                Ok(())
            }
            MODE_WRONG_RECEIPT_VERSION => {
                let mut wrong_receipt = receipt;
                wrong_receipt[8] ^= 1;
                set_return_data(&wrong_receipt);
                Ok(())
            }
            MODE_TRAILING_RECEIPT_BYTE => {
                let mut trailing_receipt = receipt.to_vec();
                trailing_receipt.push(0);
                set_return_data(&trailing_receipt);
                Ok(())
            }
            _ => err!(EngineError::InvalidMode),
        }
    }

    pub fn hostile_escalation_probe(_ctx: Context<HostileEscalationProbe>) -> Result<()> {
        err!(EngineError::HostileCpiUnexpectedlySucceeded)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpmmQuote {
    pub effective_input: u64,
    pub lp_fee_retained: u64,
    pub amount_out: u64,
}

pub fn encode_helper_payload(amount: u64) -> [u8; HELPER_INCREMENT_PAYLOAD_LEN] {
    let mut payload = [0_u8; HELPER_INCREMENT_PAYLOAD_LEN];
    payload[0] = HELPER_PAYLOAD_TAG;
    payload[1..].copy_from_slice(&amount.to_le_bytes());
    payload
}

/// Computes a pool-favouring, two-stage integer CPMM exact-input quote.
///
/// The entire gross input is settled into the input reserve. Only the effective
/// input enters the pricing fraction, so `lp_fee_retained` remains implicit in
/// the pool. A separate Core protocol fee is intentionally absent from this API.
pub fn quote_exact_in(
    reserve_in: u64,
    reserve_out: u64,
    gross_input: u64,
    lp_fee_bps: u16,
) -> Result<CpmmQuote> {
    require!(reserve_in > 0 && reserve_out > 0, EngineError::ZeroReserve);
    require!(gross_input > 0, EngineError::ZeroInput);
    let retained_bps = BASIS_POINTS_DENOMINATOR
        .checked_sub(u128::from(lp_fee_bps))
        .ok_or(EngineError::InvalidLpFeeBps)?;
    require!(retained_bps > 0, EngineError::InvalidLpFeeBps);

    // The Core accounts the full input, so reject a quote that could not be
    // represented by its u64 reserve accounting even if the pricing fraction fits.
    reserve_in
        .checked_add(gross_input)
        .ok_or(EngineError::ArithmeticOverflow)?;

    let effective_input_u128 = u128::from(gross_input)
        .checked_mul(retained_bps)
        .and_then(|value| value.checked_div(BASIS_POINTS_DENOMINATOR))
        .ok_or(EngineError::ArithmeticOverflow)?;
    let effective_input = u64::try_from(effective_input_u128)
        .map_err(|_| error!(EngineError::IntegerConversionFailed))?;
    require!(effective_input > 0, EngineError::EffectiveInputZero);

    let numerator = u128::from(reserve_out)
        .checked_mul(u128::from(effective_input))
        .ok_or(EngineError::ArithmeticOverflow)?;
    let denominator = u128::from(reserve_in)
        .checked_add(u128::from(effective_input))
        .ok_or(EngineError::ArithmeticOverflow)?;
    let amount_out_u128 = numerator
        .checked_div(denominator)
        .ok_or(EngineError::ArithmeticOverflow)?;
    let amount_out =
        u64::try_from(amount_out_u128).map_err(|_| error!(EngineError::IntegerConversionFailed))?;
    require!(amount_out > 0, EngineError::OutputAmountZero);
    require!(amount_out < reserve_out, EngineError::OutputReserveExceeded);

    let lp_fee_retained = gross_input
        .checked_sub(effective_input)
        .ok_or(EngineError::ArithmeticOverflow)?;
    Ok(CpmmQuote {
        effective_input,
        lp_fee_retained,
        amount_out,
    })
}

fn is_supported_mode(mode: u8) -> bool {
    matches!(
        mode,
        MODE_ACCEPT
            | MODE_MISSING_RECEIPT
            | MODE_WRONG_REQUEST_HASH
            | MODE_MALFORMED_RECEIPT
            | MODE_ZERO_OUTPUT
            | MODE_OVERSIZED_OUTPUT
            | MODE_HOSTILE_READONLY_ESCALATION
            | MODE_WRONG_RECEIPT_MAGIC
            | MODE_WRONG_RECEIPT_VERSION
            | MODE_TRAILING_RECEIPT_BYTE
    )
}

fn require_direct_core_cpi(instructions: &AccountInfo<'_>) -> Result<()> {
    let expected_height = TRANSACTION_LEVEL_STACK_HEIGHT + 1;
    require_eq!(
        get_stack_height(),
        expected_height,
        EngineError::InvalidInvocationDepth
    );

    let current_index = load_current_index_checked(instructions)
        .map_err(|_| error!(EngineError::InvalidInstructionsSysvar))?;
    let top_level_instruction = load_instruction_at_checked(current_index.into(), instructions)
        .map_err(|_| error!(EngineError::InvalidInstructionsSysvar))?;
    require_keys_eq!(
        top_level_instruction.program_id,
        DISPOSABLE_CORE_PROGRAM_ID,
        EngineError::InvalidCoreCaller
    );
    require!(
        top_level_instruction
            .data
            .starts_with(&CORE_EXECUTE_ENGINE_GENERATED_PROBE_DISCRIMINATOR),
        EngineError::InvalidCoreInstruction
    );
    Ok(())
}

fn capability_descriptor(account: &AccountInfo<'_>) -> CapabilityDescriptor {
    CapabilityDescriptor {
        key: *account.key,
        owner: *account.owner,
        is_writable: account.is_writable,
        is_signer: account.is_signer,
        is_executable: account.executable,
    }
}

fn maybe_invoke_helper<'info>(
    payload: &[u8],
    remaining_accounts: &[AccountInfo<'info>],
    engine_state_key: &Pubkey,
    capability_authority_bump: u8,
) -> Result<()> {
    if payload.first().copied() != Some(HELPER_PAYLOAD_TAG) {
        return Ok(());
    }
    require!(
        (3..=MAX_OPAQUE_CAPABILITIES).contains(&remaining_accounts.len()),
        EngineError::InvalidHelperCapabilityClosure
    );

    // Core has authenticated and hash-bound the full ordered closure. This
    // engine-specific helper command consumes only its required three-account prefix.
    let helper_program = &remaining_accounts[0];
    let helper_state = &remaining_accounts[1];
    let capability_authority = &remaining_accounts[2];
    require!(
        helper_program.executable && !helper_program.is_writable && !helper_program.is_signer,
        EngineError::InvalidHelperProgram
    );
    require_keys_eq!(
        *helper_program.key,
        DISPOSABLE_HELPER_PROGRAM_ID,
        EngineError::InvalidHelperProgram
    );
    require!(
        helper_state.is_writable && !helper_state.is_signer && !helper_state.executable,
        EngineError::InvalidHelperState
    );
    require_keys_eq!(
        *helper_state.owner,
        *helper_program.key,
        EngineError::InvalidHelperState
    );
    require!(
        !capability_authority.is_writable
            && !capability_authority.is_signer
            && !capability_authority.executable,
        EngineError::InvalidCapabilityAuthority
    );
    require_keys_eq!(
        *capability_authority.owner,
        crate::ID,
        EngineError::InvalidCapabilityAuthority
    );
    require!(
        helper_program.key != helper_state.key
            && helper_program.key != capability_authority.key
            && helper_state.key != capability_authority.key,
        EngineError::InvalidHelperCapabilityClosure
    );

    let bump = [capability_authority_bump];
    let authority_seeds = [
        CAPABILITY_AUTHORITY_SEED,
        engine_state_key.as_ref(),
        bump.as_ref(),
    ];
    let expected_authority = Pubkey::create_program_address(&authority_seeds, &crate::ID)
        .map_err(|_| error!(EngineError::InvalidCapabilityAuthority))?;
    require_keys_eq!(
        expected_authority,
        *capability_authority.key,
        EngineError::InvalidCapabilityAuthority
    );

    let mut marker_data: &[u8] = &capability_authority.try_borrow_data()?;
    let marker = CapabilityAuthorityMarker::try_deserialize(&mut marker_data)
        .map_err(|_| error!(EngineError::InvalidCapabilityAuthority))?;
    require_keys_eq!(
        marker.engine_state,
        *engine_state_key,
        EngineError::InvalidCapabilityAuthority
    );

    let amount = match payload.len() {
        1 => 1,
        HELPER_INCREMENT_PAYLOAD_LEN | MAX_OPAQUE_PAYLOAD_LEN => {
            // The maximum-resource fixture hash-binds the trailing bytes but
            // deliberately keeps the helper command in the fixed nine-byte prefix.
            let mut encoded = [0_u8; 8];
            encoded.copy_from_slice(&payload[1..HELPER_INCREMENT_PAYLOAD_LEN]);
            u64::from_le_bytes(encoded)
        }
        _ => return err!(EngineError::InvalidHelperPayload),
    };
    require!(amount > 0, EngineError::InvalidHelperPayload);

    let mut data = [0_u8; HELPER_INCREMENT_DATA_LEN];
    data[..HELPER_INCREMENT_DISCRIMINATOR.len()].copy_from_slice(&HELPER_INCREMENT_DISCRIMINATOR);
    data[HELPER_INCREMENT_DISCRIMINATOR.len()..].copy_from_slice(&amount.to_le_bytes());
    let instruction = Instruction {
        program_id: *helper_program.key,
        accounts: vec![
            AccountMeta::new(*helper_state.key, false),
            AccountMeta::new_readonly(*capability_authority.key, true),
        ],
        data: data.to_vec(),
    };
    let signer_seeds = [&authority_seeds[..]];
    invoke_signed(
        &instruction,
        &[
            helper_state.clone(),
            capability_authority.clone(),
            helper_program.clone(),
        ],
        &signer_seeds,
    )
    .map_err(Into::into)
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
    #[account(
        init,
        payer = authority,
        space = 8 + CapabilityAuthorityMarker::INIT_SPACE,
        seeds = [CAPABILITY_AUTHORITY_SEED, engine_state.key().as_ref()],
        bump
    )]
    pub capability_authority: Account<'info, CapabilityAuthorityMarker>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SetMode<'info> {
    #[account(mut, has_one = authority @ EngineError::InvalidAuthority)]
    pub engine_state: Account<'info, EngineState>,
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct Evaluate<'info> {
    #[account(mut)]
    pub engine_state: Account<'info, EngineState>,
    /// CHECK: The address constraint pins the introspection sysvar. It is also
    /// included in the hashed capability closure with its actual privileges.
    #[account(address = solana_instructions_sysvar::ID)]
    pub instructions_sysvar: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct HostileEscalationProbe<'info> {
    /// CHECK: This instruction must never pass the runtime privilege check.
    #[account(mut, address = solana_instructions_sysvar::ID)]
    pub instructions_sysvar: UncheckedAccount<'info>,
}

#[account]
#[derive(InitSpace)]
pub struct EngineState {
    pub authority: Pubkey,
    pub market: Pubkey,
    pub revision: u64,
    pub lp_fee_bps: u16,
    pub sequence: u64,
    pub last_request_hash: [u8; 32],
    pub last_amount_out: u64,
    pub capability_authority_bump: u8,
    pub mode: u8,
}

#[account]
#[derive(InitSpace)]
pub struct CapabilityAuthorityMarker {
    pub engine_state: Pubkey,
}

#[error_code]
pub enum EngineError {
    #[msg("The engine received an undeclared fixed account")]
    UnexpectedAccount,
    #[msg("The market address is invalid")]
    InvalidMarket,
    #[msg("The engine revision is invalid")]
    InvalidRevision,
    #[msg("The LP fee must be below 10,000 basis points")]
    InvalidLpFeeBps,
    #[msg("The signer does not control this engine state")]
    InvalidAuthority,
    #[msg("The requested probe mode is invalid")]
    InvalidMode,
    #[msg("The request codec rejected the payload")]
    InvalidRequestEncoding,
    #[msg("The request market does not match engine state")]
    MarketMismatch,
    #[msg("The request revision does not match engine state")]
    RevisionMismatch,
    #[msg("The capability closure exceeds the experiment bound")]
    TooManyCapabilities,
    #[msg("The capability count does not match the request")]
    CapabilityCountMismatch,
    #[msg("The observed capability closure does not match the request")]
    CapabilityHashMismatch,
    #[msg("The capability closure could not be hashed")]
    CapabilityHashComputationFailed,
    #[msg("The input and output reserves must be nonzero")]
    ZeroReserve,
    #[msg("The gross input must be nonzero")]
    ZeroInput,
    #[msg("The fee-rounded effective input is zero")]
    EffectiveInputZero,
    #[msg("The rounded CPMM output is zero")]
    OutputAmountZero,
    #[msg("The CPMM output must leave a positive output reserve")]
    OutputReserveExceeded,
    #[msg("The arithmetic operation overflowed or underflowed")]
    ArithmeticOverflow,
    #[msg("The arithmetic result does not fit its target integer type")]
    IntegerConversionFailed,
    #[msg("The engine sequence overflowed")]
    SequenceOverflow,
    #[msg("The engine must be called by one direct Core CPI")]
    InvalidInvocationDepth,
    #[msg("The caller is not the disposable generated-settlement Core")]
    InvalidCoreCaller,
    #[msg("The Core call is not the generated-plan probe route")]
    InvalidCoreInstruction,
    #[msg("The instructions sysvar could not be introspected")]
    InvalidInstructionsSysvar,
    #[msg("The helper capability closure does not contain the required ordered prefix")]
    InvalidHelperCapabilityClosure,
    #[msg("The helper program capability is invalid")]
    InvalidHelperProgram,
    #[msg("The helper state capability is invalid")]
    InvalidHelperState,
    #[msg("The engine PDA capability authority is invalid")]
    InvalidCapabilityAuthority,
    #[msg("The helper payload is malformed")]
    InvalidHelperPayload,
    #[msg("The runtime unexpectedly allowed writable privilege escalation")]
    HostileCpiUnexpectedlySucceeded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_quote_separates_implicit_lp_fee() {
        let quote = quote_exact_in(1_000_000, 1_000_000, 100_000, 30).unwrap();

        assert_eq!(
            quote,
            CpmmQuote {
                effective_input: 99_700,
                lp_fee_retained: 300,
                amount_out: 90_661,
            }
        );
        assert_invariant(1_000_000, 1_000_000, 100_000, quote.amount_out);
    }

    #[test]
    fn zero_fee_quote_has_no_retained_input() {
        let quote = quote_exact_in(1_000_000, 1_000_000, 100_000, 0).unwrap();

        assert_eq!(quote.effective_input, 100_000);
        assert_eq!(quote.lp_fee_retained, 0);
        assert_eq!(quote.amount_out, 90_909);
    }

    #[test]
    fn lp_fee_rounding_is_explicitly_pool_favouring() {
        let quote = quote_exact_in(1_000_000, 1_000_000, 334, 30).unwrap();

        assert_eq!(quote.effective_input, 332);
        assert_eq!(quote.lp_fee_retained, 2);
        assert_eq!(quote.amount_out, 331);
    }

    #[test]
    fn helper_payload_encoding_is_fixed_width_and_little_endian() {
        let amount = 0x0102_0304_0506_0708_u64;
        let payload = encode_helper_payload(amount);

        assert_eq!(payload.len(), HELPER_INCREMENT_PAYLOAD_LEN);
        assert_eq!(payload[0], HELPER_PAYLOAD_TAG);
        assert_eq!(&payload[1..], &amount.to_le_bytes());
    }

    #[test]
    fn hostile_receipt_mode_values_are_stable_and_supported() {
        let modes = [
            MODE_WRONG_RECEIPT_MAGIC,
            MODE_WRONG_RECEIPT_VERSION,
            MODE_TRAILING_RECEIPT_BYTE,
        ];

        assert_eq!(modes, [7, 8, 9]);
        assert!(modes.into_iter().all(is_supported_mode));
        assert!(!is_supported_mode(10));
    }

    #[test]
    fn rejects_invalid_and_unrepresentable_quotes() {
        assert!(quote_exact_in(0, 1, 1, 0).is_err());
        assert!(quote_exact_in(1, 0, 1, 0).is_err());
        assert!(quote_exact_in(1, 1, 0, 0).is_err());
        assert!(quote_exact_in(1, 1, 1, 10_000).is_err());
        assert!(quote_exact_in(1, 1, 1, u16::MAX).is_err());
        assert!(quote_exact_in(1_000, 1_000, 1, 30).is_err());
        assert!(quote_exact_in(u64::MAX, 1, 1, 0).is_err());
        assert!(quote_exact_in(1, 1, 1, 0).is_err());
    }

    #[test]
    fn maximum_width_products_remain_checked() {
        let quote = quote_exact_in(1, u64::MAX, u64::MAX - 1, 0).unwrap();

        assert_eq!(quote.effective_input, u64::MAX - 1);
        assert_eq!(quote.amount_out, u64::MAX - 1);
        assert_invariant(1, u64::MAX, u64::MAX - 1, quote.amount_out);
    }

    #[test]
    fn deterministic_property_grid_preserves_cpmm_and_monotonicity() {
        let reserves = [1_u64, 2, 3, 10, 997, 10_000, 1_000_000, u32::MAX as u64];
        let inputs = [1_u64, 2, 3, 10, 334, 10_000, 100_000, u32::MAX as u64];
        let fees = [0_u16, 1, 30, 100, 1_000, 5_000, 9_999];

        for reserve_in in reserves {
            for reserve_out in reserves {
                for gross_input in inputs {
                    if reserve_in.checked_add(gross_input).is_none() {
                        continue;
                    }
                    let mut prior_successful_output = None;
                    for fee in fees {
                        let Ok(quote) = quote_exact_in(reserve_in, reserve_out, gross_input, fee)
                        else {
                            continue;
                        };
                        assert!(quote.effective_input <= gross_input);
                        assert_eq!(quote.lp_fee_retained, gross_input - quote.effective_input);
                        assert!(quote.amount_out < reserve_out);
                        assert_invariant(reserve_in, reserve_out, gross_input, quote.amount_out);
                        if let Some(prior_output) = prior_successful_output {
                            assert!(quote.amount_out <= prior_output);
                        }
                        prior_successful_output = Some(quote.amount_out);
                    }
                }
            }
        }
    }

    #[test]
    fn deterministic_randomized_quotes_match_an_independent_integer_model() {
        let boundary_cases = [
            (0, 1, 1, 0),
            (1, 0, 1, 0),
            (1, 1, 0, 0),
            (1, 1, 1, 0),
            (1_000, 1_000, 1, 30),
            (1, u64::MAX, u64::MAX - 1, 0),
            (u64::MAX, 1, 1, 0),
            (u64::MAX - 1, u64::MAX, 1, 9_999),
            (1_000_000, 1_000_000, 100_000, 10_000),
        ];
        for (reserve_in, reserve_out, gross_input, fee) in boundary_cases {
            assert_matches_independent_model(reserve_in, reserve_out, gross_input, fee);
        }

        // Fixed seed and generator make failures exactly reproducible. The model
        // below does not call the implementation or its checked-arithmetic path.
        let mut state = 0x5e77_1e5d_0c0d_e5a1_u64;
        for index in 0..4_096_u64 {
            let reserve_in = next_xorshift64(&mut state);
            let reserve_out = next_xorshift64(&mut state);
            let gross_input = next_xorshift64(&mut state);
            let fee = (next_xorshift64(&mut state) % 10_001) as u16;
            assert_matches_independent_model(reserve_in, reserve_out, gross_input, fee);

            if index % 257 == 0 {
                assert_matches_independent_model(1, reserve_out, gross_input, fee);
                assert_matches_independent_model(reserve_in, 1, gross_input, fee);
                assert_matches_independent_model(reserve_in, reserve_out, 1, fee);
            }
        }
    }

    #[test]
    fn output_is_monotone_in_successful_exact_inputs() {
        let inputs = [1_u64, 2, 3, 10, 100, 1_000, 10_000, 100_000];
        let mut prior_successful_output = None;

        for gross_input in inputs {
            let Ok(quote) = quote_exact_in(1_000_000, 1_000_000, gross_input, 30) else {
                continue;
            };
            if let Some(prior_output) = prior_successful_output {
                assert!(quote.amount_out >= prior_output);
            }
            prior_successful_output = Some(quote.amount_out);
        }
    }

    fn assert_invariant(reserve_in: u64, reserve_out: u64, gross_input: u64, amount_out: u64) {
        let pre_k = u128::from(reserve_in)
            .checked_mul(u128::from(reserve_out))
            .unwrap();
        let post_in = reserve_in.checked_add(gross_input).unwrap();
        let post_out = reserve_out.checked_sub(amount_out).unwrap();
        let post_k = u128::from(post_in)
            .checked_mul(u128::from(post_out))
            .unwrap();
        assert!(post_k >= pre_k, "{post_k} < {pre_k}");
    }

    fn assert_matches_independent_model(
        reserve_in: u64,
        reserve_out: u64,
        gross_input: u64,
        lp_fee_bps: u16,
    ) {
        let actual = quote_exact_in(reserve_in, reserve_out, gross_input, lp_fee_bps);
        let expected = independent_quote_model(reserve_in, reserve_out, gross_input, lp_fee_bps);

        match (actual, expected) {
            (Ok(actual), Some(expected)) => assert_eq!(actual, expected),
            (Err(_), None) => {}
            (actual, expected) => panic!(
                "model mismatch for x={reserve_in}, y={reserve_out}, dx={gross_input}, fee={lp_fee_bps}: actual={actual:?}, expected={expected:?}"
            ),
        }
    }

    fn independent_quote_model(
        reserve_in: u64,
        reserve_out: u64,
        gross_input: u64,
        lp_fee_bps: u16,
    ) -> Option<CpmmQuote> {
        if reserve_in == 0
            || reserve_out == 0
            || gross_input == 0
            || lp_fee_bps >= 10_000
            || reserve_in.checked_add(gross_input).is_none()
        {
            return None;
        }

        // Products of two u64 values fit u128, so the reference model can use
        // direct arithmetic and stay independent from the implementation's
        // checked-operation control flow.
        let retained_bps = 10_000_u128 - u128::from(lp_fee_bps);
        let effective_input = (u128::from(gross_input) * retained_bps / 10_000_u128) as u64;
        if effective_input == 0 {
            return None;
        }
        let amount_out = (u128::from(reserve_out) * u128::from(effective_input)
            / (u128::from(reserve_in) + u128::from(effective_input)))
            as u64;
        if amount_out == 0 || amount_out >= reserve_out {
            return None;
        }

        Some(CpmmQuote {
            effective_input,
            lp_fee_retained: gross_input - effective_input,
            amount_out,
        })
    }

    fn next_xorshift64(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }
}
