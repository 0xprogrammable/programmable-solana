//! Disposable CPMM engine for the routed callback authentication experiment.
//!
//! The three phase-specific Anchor entrypoints share one evaluator. Core proves
//! authority with a phase-scoped PDA signer; the engine independently binds
//! that signer, the visible capability closure, the request digest, and its own
//! state sequence. No instruction introspection or stack-depth assumption is
//! part of the authorization model.

use anchor_lang::{
    prelude::*,
    solana_program::{
        instruction::{AccountMeta, Instruction},
        program::{invoke, set_return_data},
    },
};
use routed_callback_probe_wire::{
    compute_capability_hash, compute_execution_digest, decode_engine_request, encode_receipt,
    validate_phase_for_timing, CapabilityDescriptor, EngineReceipt, ExecutionBinding,
    CALLBACK_AUTHORITY_SEED, DISPOSABLE_CORE_PROGRAM_ID, DISPOSABLE_HELPER_PROGRAM_ID,
    ENGINE_REQUEST_LEN, MAX_OPAQUE_ACCOUNTS, MAX_OPAQUE_PAYLOAD_LEN, PHASE_COMMIT, PHASE_PREPARE,
    PHASE_TRANSITION, TIMING_PREPARE_COMMIT, TIMING_SINGLE,
};

declare_id!("5UNyG5GQpPwyoDgsvt4JzdqJxJzPh52pVbUDjEa5Gikh");

pub const BASIS_POINTS_DENOMINATOR: u128 = 10_000;
pub const MAX_OPAQUE_CAPABILITIES: usize = MAX_OPAQUE_ACCOUNTS;
pub const NO_COMPLETED_PHASE: u8 = u8::MAX;

pub const MODE_ACCEPT: u8 = 0;
pub const MODE_MISSING_RECEIPT: u8 = 1;
pub const MODE_WRONG_INTENT_DIGEST: u8 = 2;
pub const MODE_WRONG_EXECUTION_DIGEST: u8 = 3;
pub const MODE_MALFORMED_RECEIPT: u8 = 4;
pub const MODE_ZERO_OUTPUT: u8 = 5;
pub const MODE_OVERSIZED_OUTPUT: u8 = 6;
pub const MODE_WRONG_RECEIPT_MAGIC: u8 = 7;
pub const MODE_WRONG_RECEIPT_VERSION: u8 = 8;
pub const MODE_TRAILING_RECEIPT_BYTE: u8 = 9;
pub const MODE_WRONG_RECEIPT_PHASE: u8 = 10;
pub const MODE_LATE_COMMIT_FAILURE: u8 = 11;

pub const HELPER_PAYLOAD_TAG: u8 = 1;
pub const HELPER_INCREMENT_PAYLOAD_LEN: usize = 1 + 8;
const HELPER_INCREMENT_DISCRIMINATOR: [u8; 8] = [0x0b, 0x12, 0x68, 0x09, 0x68, 0xae, 0x3b, 0x21];
const HELPER_INCREMENT_DATA_LEN: usize = HELPER_INCREMENT_DISCRIMINATOR.len() + 8;
const _: [(); ENGINE_REQUEST_LEN] = [(); 414];

#[program]
pub mod routed_plan_engine {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        market: Pubkey,
        revision: u64,
        lp_fee_bps: u16,
        timing_mode: u8,
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
        validate_configured_timing_mode(timing_mode)?;

        let state = &mut ctx.accounts.engine_state;
        state.authority = ctx.accounts.authority.key();
        state.market = market;
        state.revision = revision;
        state.lp_fee_bps = lp_fee_bps;
        state.timing_mode = timing_mode;
        state.sequence = 0;
        state.last_intent_digest = [0; 32];
        state.last_phase_context_digest = [0; 32];
        state.last_execution_digest = [0; 32];
        state.last_amount_out = 0;
        state.last_phase = NO_COMPLETED_PHASE;
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

    pub fn transition<'info>(
        ctx: Context<'info, EvaluatePhase<'info>>,
        wire_request: [u8; ENGINE_REQUEST_LEN],
    ) -> Result<()> {
        evaluate_phase(
            ctx.accounts,
            ctx.remaining_accounts,
            PHASE_TRANSITION,
            &wire_request,
        )
    }

    pub fn prepare<'info>(
        ctx: Context<'info, EvaluatePhase<'info>>,
        wire_request: [u8; ENGINE_REQUEST_LEN],
    ) -> Result<()> {
        evaluate_phase(
            ctx.accounts,
            ctx.remaining_accounts,
            PHASE_PREPARE,
            &wire_request,
        )
    }

    pub fn commit<'info>(
        ctx: Context<'info, EvaluatePhase<'info>>,
        wire_request: [u8; ENGINE_REQUEST_LEN],
    ) -> Result<()> {
        evaluate_phase(
            ctx.accounts,
            ctx.remaining_accounts,
            PHASE_COMMIT,
            &wire_request,
        )
    }
}

fn evaluate_phase<'info>(
    accounts: &EvaluatePhase<'info>,
    opaque_accounts: &[AccountInfo<'info>],
    expected_phase: u8,
    wire_request: &[u8; ENGINE_REQUEST_LEN],
) -> Result<()> {
    let request = decode_engine_request(wire_request)
        .map_err(|_| error!(EngineError::InvalidRequestEncoding))?;
    let binding = request.binding;

    require_eq!(
        binding.phase,
        expected_phase,
        EngineError::EntrypointPhaseMismatch
    );
    require!(
        opaque_accounts.len() <= MAX_OPAQUE_CAPABILITIES,
        EngineError::TooManyCapabilities
    );
    require_eq!(
        opaque_accounts.len(),
        usize::from(binding.opaque_account_count),
        EngineError::CapabilityCountMismatch
    );

    validate_fixed_account_privileges(accounts, opaque_accounts, binding.phase)?;
    let mut state = load_engine_state(&accounts.engine_state.to_account_info())?;
    require!(is_supported_mode(state.mode), EngineError::InvalidMode);
    require_keys_eq!(state.market, binding.market, EngineError::MarketMismatch);
    require_eq!(
        state.revision,
        binding.engine_revision,
        EngineError::RevisionMismatch
    );
    validate_phase_for_timing(state.timing_mode, binding.phase)
        .map_err(|_| error!(EngineError::InvalidPhaseForTiming))?;
    require_eq!(
        state.sequence,
        binding.pre_sequence,
        EngineError::SequenceMismatch
    );

    authenticate_callback_authority(
        &accounts.engine_state.to_account_info(),
        &accounts.callback_authority.to_account_info(),
        &binding,
    )?;

    let mut descriptors = Vec::with_capacity(opaque_accounts.len() + 1);
    descriptors.push(capability_descriptor(
        &accounts.engine_state.to_account_info(),
    ));
    descriptors.extend(opaque_accounts.iter().map(capability_descriptor));
    let observed_capability_hash = compute_capability_hash(&crate::ID, &descriptors)
        .map_err(|_| error!(EngineError::CapabilityHashComputationFailed))?;
    require!(
        observed_capability_hash == binding.phase_capability_hash,
        EngineError::CapabilityHashMismatch
    );

    let observed_execution_digest = compute_execution_digest(&binding)
        .map_err(|_| error!(EngineError::ExecutionDigestComputationFailed))?;
    require!(
        observed_execution_digest == request.execution_digest,
        EngineError::ExecutionDigestMismatch
    );

    let quote = quote_exact_in(
        binding.accounted_input_before,
        binding.accounted_output_before,
        binding.amount_in,
        state.lp_fee_bps,
    )?;
    let mut amount_out = quote.amount_out;
    match state.mode {
        MODE_ZERO_OUTPUT => amount_out = 0,
        MODE_OVERSIZED_OUTPUT => amount_out = u64::MAX,
        _ => {}
    }

    let receipt_sequence = if binding.phase == PHASE_PREPARE {
        state.sequence
    } else {
        maybe_invoke_helper(
            request
                .payload_bytes()
                .map_err(|_| error!(EngineError::InvalidRequestEncoding))?,
            opaque_accounts,
            &accounts.engine_state.to_account_info(),
            &accounts.callback_authority.to_account_info(),
        )?;

        state.sequence = state
            .sequence
            .checked_add(1)
            .ok_or(EngineError::SequenceOverflow)?;
        state.last_intent_digest = binding.intent_digest;
        // The wire codec requires zero context for TRANSITION/PREPARE and a
        // nonzero Core-computed settlement digest for COMMIT. The execution
        // digest authenticates this field; persisting it makes that final
        // settlement context explicit engine-state evidence.
        state.last_phase_context_digest = binding.phase_context_digest;
        state.last_execution_digest = request.execution_digest;
        state.last_amount_out = amount_out;
        state.last_phase = binding.phase;
        store_engine_state(&accounts.engine_state.to_account_info(), &state)?;

        if state.mode == MODE_LATE_COMMIT_FAILURE && binding.phase == PHASE_COMMIT {
            return err!(EngineError::DeliberateLateCommitFailure);
        }
        state.sequence
    };

    emit_receipt(
        state.mode,
        EngineReceipt {
            phase: binding.phase,
            intent_digest: binding.intent_digest,
            execution_digest: request.execution_digest,
            amount_out,
            state_sequence: receipt_sequence,
        },
    )
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
            | MODE_WRONG_INTENT_DIGEST
            | MODE_WRONG_EXECUTION_DIGEST
            | MODE_MALFORMED_RECEIPT
            | MODE_ZERO_OUTPUT
            | MODE_OVERSIZED_OUTPUT
            | MODE_WRONG_RECEIPT_MAGIC
            | MODE_WRONG_RECEIPT_VERSION
            | MODE_TRAILING_RECEIPT_BYTE
            | MODE_WRONG_RECEIPT_PHASE
            | MODE_LATE_COMMIT_FAILURE
    )
}

fn validate_configured_timing_mode(timing_mode: u8) -> Result<()> {
    let representative_phase = match timing_mode {
        TIMING_SINGLE => PHASE_TRANSITION,
        TIMING_PREPARE_COMMIT => PHASE_PREPARE,
        _ => return err!(EngineError::InvalidTimingMode),
    };
    validate_phase_for_timing(timing_mode, representative_phase)
        .map_err(|_| error!(EngineError::InvalidTimingMode))?;
    Ok(())
}

fn validate_fixed_account_privileges(
    accounts: &EvaluatePhase<'_>,
    opaque_accounts: &[AccountInfo<'_>],
    phase: u8,
) -> Result<()> {
    let engine_state = accounts.engine_state.to_account_info();
    let callback = accounts.callback_authority.to_account_info();

    require!(!engine_state.executable, EngineError::InvalidEngineState);
    require!(!engine_state.is_signer, EngineError::EngineStateSigner);
    require!(
        callback.is_signer && !callback.is_writable && !callback.executable,
        EngineError::InvalidCallbackPrivileges
    );
    require_keys_neq!(
        *engine_state.key,
        *callback.key,
        EngineError::AliasedCallbackAuthority
    );

    match phase {
        PHASE_PREPARE => {
            require!(
                !engine_state.is_writable,
                EngineError::PrepareStateMustBeReadOnly
            );
        }
        PHASE_TRANSITION | PHASE_COMMIT => {
            require!(
                engine_state.is_writable,
                EngineError::MutationStateMustBeWritable
            );
        }
        _ => return err!(EngineError::EntrypointPhaseMismatch),
    }

    for account in opaque_accounts {
        require!(!account.is_signer, EngineError::OpaqueSignerForbidden);
        require_keys_neq!(
            *account.key,
            *callback.key,
            EngineError::AliasedCallbackAuthority
        );
        if phase == PHASE_PREPARE {
            require!(!account.is_writable, EngineError::PrepareCapabilityWritable);
        }
    }
    Ok(())
}

fn authenticate_callback_authority(
    engine_state: &AccountInfo<'_>,
    callback: &AccountInfo<'_>,
    binding: &ExecutionBinding,
) -> Result<()> {
    let phase = [binding.phase];
    let (expected_callback, _) = Pubkey::find_program_address(
        &[
            CALLBACK_AUTHORITY_SEED,
            crate::ID.as_ref(),
            engine_state.key.as_ref(),
            binding.market.as_ref(),
            binding.domain.as_ref(),
            binding.intent_digest.as_ref(),
            phase.as_ref(),
        ],
        &DISPOSABLE_CORE_PROGRAM_ID,
    );
    require_keys_eq!(
        expected_callback,
        *callback.key,
        EngineError::InvalidCallbackAuthority
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

fn load_engine_state(account: &AccountInfo<'_>) -> Result<EngineState> {
    require_keys_eq!(*account.owner, crate::ID, EngineError::InvalidEngineState);
    require_eq!(
        account.data_len(),
        8 + EngineState::INIT_SPACE,
        EngineError::InvalidEngineState
    );
    let data = account.try_borrow_data()?;
    let mut encoded: &[u8] = &data;
    EngineState::try_deserialize(&mut encoded).map_err(|_| error!(EngineError::InvalidEngineState))
}

fn store_engine_state(account: &AccountInfo<'_>, state: &EngineState) -> Result<()> {
    require!(
        account.is_writable,
        EngineError::MutationStateMustBeWritable
    );
    require_keys_eq!(*account.owner, crate::ID, EngineError::InvalidEngineState);
    require_eq!(
        account.data_len(),
        8 + EngineState::INIT_SPACE,
        EngineError::InvalidEngineState
    );
    let mut data = account.try_borrow_mut_data()?;
    let mut encoded: &mut [u8] = &mut data;
    state
        .try_serialize(&mut encoded)
        .map_err(|_| error!(EngineError::InvalidEngineState))
}

fn maybe_invoke_helper<'info>(
    payload: &[u8],
    opaque_accounts: &[AccountInfo<'info>],
    engine_state: &AccountInfo<'info>,
    callback_authority: &AccountInfo<'info>,
) -> Result<()> {
    if payload.first().copied() != Some(HELPER_PAYLOAD_TAG) {
        return Ok(());
    }
    require!(
        (2..=MAX_OPAQUE_CAPABILITIES).contains(&opaque_accounts.len()),
        EngineError::InvalidHelperCapabilityClosure
    );

    // The callback authority is fixed, excluded from the economic capability
    // hash, and inherited as a Core PDA signer. The helper consumes only the
    // first two opaque capabilities and never receives an engine-derived PDA.
    let helper_program = &opaque_accounts[0];
    let helper_state = &opaque_accounts[1];
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
        helper_program.key != helper_state.key
            && helper_program.key != callback_authority.key
            && helper_state.key != callback_authority.key
            && helper_program.key != engine_state.key
            && helper_state.key != engine_state.key,
        EngineError::InvalidHelperCapabilityClosure
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
            AccountMeta::new_readonly(*callback_authority.key, true),
        ],
        data: data.to_vec(),
    };
    invoke(
        &instruction,
        &[
            helper_state.clone(),
            callback_authority.clone(),
            helper_program.clone(),
        ],
    )
    .map_err(Into::into)
}

fn emit_receipt(mode: u8, receipt: EngineReceipt) -> Result<()> {
    if mode == MODE_MISSING_RECEIPT {
        return Ok(());
    }

    let encoded =
        encode_receipt(&receipt).map_err(|_| error!(EngineError::InvalidReceiptEncoding))?;
    match mode {
        MODE_ACCEPT | MODE_ZERO_OUTPUT | MODE_OVERSIZED_OUTPUT | MODE_LATE_COMMIT_FAILURE => {
            // This must remain after every nested CPI because Solana has one
            // transaction return-data slot and a helper may overwrite it.
            set_return_data(&encoded);
            Ok(())
        }
        MODE_WRONG_INTENT_DIGEST => {
            let mut wrong = receipt;
            wrong.intent_digest[0] ^= 1;
            let encoded =
                encode_receipt(&wrong).map_err(|_| error!(EngineError::InvalidReceiptEncoding))?;
            set_return_data(&encoded);
            Ok(())
        }
        MODE_WRONG_EXECUTION_DIGEST => {
            let mut wrong = receipt;
            wrong.execution_digest[0] ^= 1;
            let encoded =
                encode_receipt(&wrong).map_err(|_| error!(EngineError::InvalidReceiptEncoding))?;
            set_return_data(&encoded);
            Ok(())
        }
        MODE_MALFORMED_RECEIPT => {
            set_return_data(&encoded[..encoded.len() - 1]);
            Ok(())
        }
        MODE_WRONG_RECEIPT_MAGIC => {
            let mut wrong = encoded;
            wrong[0] ^= 1;
            set_return_data(&wrong);
            Ok(())
        }
        MODE_WRONG_RECEIPT_VERSION => {
            let mut wrong = encoded;
            wrong[8] ^= 1;
            set_return_data(&wrong);
            Ok(())
        }
        MODE_TRAILING_RECEIPT_BYTE => {
            let mut trailing = encoded.to_vec();
            trailing.push(0);
            set_return_data(&trailing);
            Ok(())
        }
        MODE_WRONG_RECEIPT_PHASE => {
            let mut wrong = receipt;
            wrong.phase = if receipt.phase == PHASE_TRANSITION {
                PHASE_PREPARE
            } else {
                PHASE_TRANSITION
            };
            let encoded =
                encode_receipt(&wrong).map_err(|_| error!(EngineError::InvalidReceiptEncoding))?;
            set_return_data(&encoded);
            Ok(())
        }
        _ => err!(EngineError::InvalidMode),
    }
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
    #[account(mut, has_one = authority @ EngineError::InvalidAuthority)]
    pub engine_state: Account<'info, EngineState>,
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct EvaluatePhase<'info> {
    /// CHECK: Parsed and serialized manually because PREPARE intentionally
    /// receives this program-owned account without writable privilege.
    pub engine_state: UncheckedAccount<'info>,
    /// CHECK: Authenticated as the exact phase-scoped Core PDA below. The
    /// effective signer privilege is inherited from Core's invoke_signed.
    pub callback_authority: UncheckedAccount<'info>,
}

#[account]
#[derive(InitSpace)]
pub struct EngineState {
    pub authority: Pubkey,
    pub market: Pubkey,
    pub revision: u64,
    pub lp_fee_bps: u16,
    pub timing_mode: u8,
    pub sequence: u64,
    pub last_intent_digest: [u8; 32],
    pub last_phase_context_digest: [u8; 32],
    pub last_execution_digest: [u8; 32],
    pub last_amount_out: u64,
    pub last_phase: u8,
    pub mode: u8,
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
    #[msg("The engine timing mode is unsupported")]
    InvalidTimingMode,
    #[msg("The signer does not control this engine state")]
    InvalidAuthority,
    #[msg("The requested probe mode is invalid")]
    InvalidMode,
    #[msg("The request codec rejected the payload")]
    InvalidRequestEncoding,
    #[msg("The request phase does not match the selected engine entrypoint")]
    EntrypointPhaseMismatch,
    #[msg("The request phase is invalid for this engine's configured timing mode")]
    InvalidPhaseForTiming,
    #[msg("The engine state account is malformed or belongs to another program")]
    InvalidEngineState,
    #[msg("The engine state account must never be a signer")]
    EngineStateSigner,
    #[msg("PREPARE requires a read-only engine state")]
    PrepareStateMustBeReadOnly,
    #[msg("TRANSITION and COMMIT require a writable engine state")]
    MutationStateMustBeWritable,
    #[msg("The callback authority has invalid effective privileges")]
    InvalidCallbackPrivileges,
    #[msg("The callback authority is not the canonical phase-scoped Core PDA")]
    InvalidCallbackAuthority,
    #[msg("The callback authority must not alias a capability account")]
    AliasedCallbackAuthority,
    #[msg("Opaque engine capabilities must not be signers")]
    OpaqueSignerForbidden,
    #[msg("PREPARE requires every opaque capability to be read-only")]
    PrepareCapabilityWritable,
    #[msg("The request market does not match engine state")]
    MarketMismatch,
    #[msg("The request revision does not match engine state")]
    RevisionMismatch,
    #[msg("The request pre-sequence does not match engine state")]
    SequenceMismatch,
    #[msg("The capability closure exceeds the experiment bound")]
    TooManyCapabilities,
    #[msg("The capability count does not match the request")]
    CapabilityCountMismatch,
    #[msg("The observed capability closure does not match the request")]
    CapabilityHashMismatch,
    #[msg("The capability closure could not be hashed")]
    CapabilityHashComputationFailed,
    #[msg("The execution digest could not be recomputed")]
    ExecutionDigestComputationFailed,
    #[msg("The recomputed execution digest does not match the request")]
    ExecutionDigestMismatch,
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
    #[msg("The helper capability closure does not contain the required ordered prefix")]
    InvalidHelperCapabilityClosure,
    #[msg("The helper program capability is invalid")]
    InvalidHelperProgram,
    #[msg("The helper state capability is invalid")]
    InvalidHelperState,
    #[msg("The helper payload is malformed")]
    InvalidHelperPayload,
    #[msg("The engine could not encode its callback receipt")]
    InvalidReceiptEncoding,
    #[msg("The engine deliberately failed after COMMIT-side effects")]
    DeliberateLateCommitFailure,
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
            MODE_MISSING_RECEIPT,
            MODE_WRONG_INTENT_DIGEST,
            MODE_WRONG_EXECUTION_DIGEST,
            MODE_MALFORMED_RECEIPT,
            MODE_ZERO_OUTPUT,
            MODE_OVERSIZED_OUTPUT,
            MODE_WRONG_RECEIPT_MAGIC,
            MODE_WRONG_RECEIPT_VERSION,
            MODE_TRAILING_RECEIPT_BYTE,
            MODE_WRONG_RECEIPT_PHASE,
            MODE_LATE_COMMIT_FAILURE,
        ];

        assert_eq!(modes, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
        assert!(modes.into_iter().all(is_supported_mode));
        assert!(!is_supported_mode(12));
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
