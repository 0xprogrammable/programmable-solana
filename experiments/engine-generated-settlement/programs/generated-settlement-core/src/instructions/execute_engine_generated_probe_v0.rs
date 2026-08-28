use anchor_lang::{
    prelude::*,
    solana_program::{
        instruction::{get_stack_height, AccountMeta, Instruction, TRANSACTION_LEVEL_STACK_HEIGHT},
        program::{get_return_data, invoke},
    },
};
use anchor_spl::token::{self, Mint, Token, TokenAccount, TransferChecked};
use generated_settlement_probe_wire::{
    compute_capability_hash, compute_payload_hash, compute_request_hash, decode_receipt,
    encode_evaluate_instruction, CapabilityDescriptor, EngineRequest, RequestBinding,
    CAPABILITY_PREFIX_ACCOUNTS,
};

use crate::{
    constants::{
        ASSET_A_INDEX_V0, ASSET_A_SEED_V0, ASSET_B_INDEX_V0, ASSET_B_SEED_V0, DOMAIN_SEED_V0,
        FEE_LEDGER_SEED_V0, FEE_VAULT_SEED_V0, INSTRUCTIONS_SYSVAR_ID, MARKET_SEED_V0,
        SETTLEMENT_HASH_DOMAIN_V0, VAULT_SEED_V0,
    },
    error::CoreError,
    events::EngineGeneratedProbeExecutedV0,
    math::fee_ceil,
    state::{DomainV0, FeeLedgerV0, MarketV0},
    validation::{
        canonical_domain_vault, canonical_fee_vault, ensure_distinct_roles, exact_credit,
        exact_debit, require_raw_covers_accounted, validate_classic_mint,
        validate_credit_destination, validate_fee_ledger, validate_market_domain,
        validate_opaque_capabilities, validate_opaque_payload, validate_protected_token_account,
    },
    ID,
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ExecuteEngineGeneratedProbeV0Args {
    pub amount_in: u64,
    pub max_total_input_debit: u64,
    pub min_output_credit: u64,
    pub max_protocol_fee: u64,
    pub expires_at_slot: u64,
    pub expected_capability_hash: [u8; 32],
    pub opaque_payload: Vec<u8>,
}

#[derive(Accounts)]
pub struct ExecuteEngineGeneratedProbeV0<'info> {
    pub user: Signer<'info>,
    #[account(
        seeds = [MARKET_SEED_V0, market.initializer.as_ref(), market.market_id.as_ref()],
        bump = market.bump
    )]
    pub market: Box<Account<'info, MarketV0>>,
    #[account(
        mut,
        seeds = [DOMAIN_SEED_V0, market.key().as_ref()],
        bump = domain.bump
    )]
    pub domain: Box<Account<'info, DomainV0>>,
    #[account(
        mut,
        seeds = [FEE_LEDGER_SEED_V0, market.key().as_ref(), mint_a.key().as_ref()],
        bump = fee_ledger.bump
    )]
    pub fee_ledger: Box<Account<'info, FeeLedgerV0>>,
    #[account(address = market.mint_a @ CoreError::InvalidTokenMint)]
    pub mint_a: Box<Account<'info, Mint>>,
    #[account(address = market.mint_b @ CoreError::InvalidTokenMint)]
    pub mint_b: Box<Account<'info, Mint>>,
    #[account(
        mut,
        token::mint = mint_a,
        token::authority = user
    )]
    pub user_source_a: Box<Account<'info, TokenAccount>>,
    #[account(mut, token::mint = mint_b)]
    pub user_destination_b: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        seeds = [VAULT_SEED_V0, domain.key().as_ref(), ASSET_A_SEED_V0],
        bump = domain.vault_a_bump,
        token::mint = mint_a,
        token::authority = domain
    )]
    pub vault_a: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        seeds = [VAULT_SEED_V0, domain.key().as_ref(), ASSET_B_SEED_V0],
        bump = domain.vault_b_bump,
        token::mint = mint_b,
        token::authority = domain
    )]
    pub vault_b: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        seeds = [FEE_VAULT_SEED_V0, fee_ledger.key().as_ref()],
        bump = fee_ledger.fee_vault_bump,
        token::mint = mint_a,
        token::authority = fee_ledger
    )]
    pub fee_vault: Box<Account<'info, TokenAccount>>,
    /// CHECK: The authenticated market binds this exact executable program.
    #[account(address = market.engine_program @ CoreError::InvalidEngineProgram)]
    pub engine_program: UncheckedAccount<'info>,
    /// CHECK: Core binds the exact address and owner and forwards only this state
    /// as the first writable engine capability.
    #[account(
        mut,
        address = market.engine_state @ CoreError::InvalidEngineState,
        owner = market.engine_program @ CoreError::InvalidEngineStateOwner
    )]
    pub engine_state: UncheckedAccount<'info>,
    /// CHECK: The canonical Instructions sysvar is the second, read-only engine
    /// capability and lets the engine authenticate the direct Core envelope.
    #[account(address = INSTRUCTIONS_SYSVAR_ID)]
    pub instructions_sysvar: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Clone, Copy)]
struct AcceptedEnginePlan {
    amount_out: u64,
    protocol_fee: u64,
    total_input_debit: u64,
    request_hash: [u8; 32],
    capability_hash: [u8; 32],
    payload_hash: [u8; 32],
    settlement_hash: [u8; 32],
    engine_sequence: u64,
    opaque_account_count: u8,
    post_accounted_a: u64,
    post_accounted_b: u64,
    post_accounted_fee_a: u64,
}

#[allow(clippy::vec_init_then_push)] // Direct heap writes preserve SBF stack headroom.
pub fn handle_execute_engine_generated_probe_v0<'info>(
    mut ctx: Context<'info, ExecuteEngineGeneratedProbeV0<'info>>,
    args: ExecuteEngineGeneratedProbeV0Args,
) -> Result<()> {
    require!(
        get_stack_height() == TRANSACTION_LEVEL_STACK_HEIGHT,
        CoreError::DirectInvocationRequired
    );
    validate_opaque_payload(&args.opaque_payload)?;

    let accounts = &mut ctx.accounts;
    // Push directly into heap storage. A fixed `[Pubkey; 16]` temporary alone
    // consumes 512 bytes of the 4 KiB SBF stack frame.
    let mut fixed_envelope_keys = Vec::with_capacity(16);
    fixed_envelope_keys.push(accounts.user.key());
    fixed_envelope_keys.push(accounts.market.key());
    fixed_envelope_keys.push(accounts.domain.key());
    fixed_envelope_keys.push(accounts.fee_ledger.key());
    fixed_envelope_keys.push(accounts.mint_a.key());
    fixed_envelope_keys.push(accounts.mint_b.key());
    fixed_envelope_keys.push(accounts.user_source_a.key());
    fixed_envelope_keys.push(accounts.user_destination_b.key());
    fixed_envelope_keys.push(accounts.vault_a.key());
    fixed_envelope_keys.push(accounts.vault_b.key());
    fixed_envelope_keys.push(accounts.fee_vault.key());
    fixed_envelope_keys.push(accounts.engine_program.key());
    fixed_envelope_keys.push(accounts.engine_state.key());
    fixed_envelope_keys.push(accounts.instructions_sysvar.key());
    fixed_envelope_keys.push(accounts.token_program.key());
    fixed_envelope_keys.push(ID);
    ensure_distinct_roles(&fixed_envelope_keys)?;

    validate_market_domain(
        accounts.market.key(),
        accounts.market.as_ref(),
        accounts.domain.key(),
        accounts.domain.as_ref(),
    )?;
    validate_fee_ledger(
        accounts.market.key(),
        accounts.market.as_ref(),
        accounts.fee_ledger.key(),
        accounts.fee_ledger.as_ref(),
    )?;

    require_keys_eq!(
        canonical_domain_vault(
            accounts.domain.key(),
            ASSET_A_INDEX_V0,
            accounts.domain.vault_a_bump,
        )?,
        accounts.vault_a.key(),
        CoreError::InvalidDomainVault
    );
    require_keys_eq!(
        canonical_domain_vault(
            accounts.domain.key(),
            ASSET_B_INDEX_V0,
            accounts.domain.vault_b_bump,
        )?,
        accounts.vault_b.key(),
        CoreError::InvalidDomainVault
    );
    require_keys_eq!(
        canonical_fee_vault(
            accounts.fee_ledger.key(),
            accounts.fee_ledger.fee_vault_bump,
        )?,
        accounts.fee_vault.key(),
        CoreError::InvalidFeeVault
    );

    require!(
        accounts.engine_program.executable,
        CoreError::EngineProgramNotExecutable
    );
    require_keys_neq!(
        accounts.engine_program.key(),
        ID,
        CoreError::CoreCannotBeEngine
    );
    require!(
        !accounts.engine_state.executable,
        CoreError::EngineStateExecutable
    );
    require_keys_eq!(
        *accounts.engine_state.owner,
        accounts.engine_program.key(),
        CoreError::InvalidEngineStateOwner
    );
    require!(
        !accounts.engine_program.is_signer
            && !accounts.engine_state.is_signer
            && !accounts.instructions_sysvar.is_signer,
        CoreError::UnexpectedSignerPrivilege
    );
    require!(
        !accounts.market.to_account_info().is_signer
            && !accounts.domain.to_account_info().is_signer
            && !accounts.fee_ledger.to_account_info().is_signer
            && !accounts.mint_a.to_account_info().is_signer
            && !accounts.mint_b.to_account_info().is_signer
            && !accounts.user_source_a.to_account_info().is_signer
            && !accounts.user_destination_b.to_account_info().is_signer
            && !accounts.vault_a.to_account_info().is_signer
            && !accounts.vault_b.to_account_info().is_signer
            && !accounts.fee_vault.to_account_info().is_signer
            && !accounts.token_program.to_account_info().is_signer,
        CoreError::UnexpectedSignerPrivilege
    );
    require!(
        accounts.engine_state.is_writable,
        CoreError::UnexpectedWritablePrivilege
    );
    require!(
        !accounts.market.to_account_info().is_writable
            && !accounts.mint_a.to_account_info().is_writable
            && !accounts.mint_b.to_account_info().is_writable
            && !accounts.engine_program.is_writable
            && !accounts.instructions_sysvar.is_writable
            && !accounts.token_program.to_account_info().is_writable,
        CoreError::UnexpectedWritablePrivilege
    );

    validate_classic_mint(accounts.mint_a.key(), &accounts.mint_a)?;
    validate_classic_mint(accounts.mint_b.key(), &accounts.mint_b)?;
    validate_protected_token_account(
        &accounts.user_source_a,
        accounts.mint_a.key(),
        accounts.user.key(),
    )?;
    validate_credit_destination(&accounts.user_destination_b, accounts.mint_b.key())?;
    validate_protected_token_account(
        &accounts.vault_a,
        accounts.mint_a.key(),
        accounts.domain.key(),
    )?;
    validate_protected_token_account(
        &accounts.vault_b,
        accounts.mint_b.key(),
        accounts.domain.key(),
    )?;
    validate_protected_token_account(
        &accounts.fee_vault,
        accounts.mint_a.key(),
        accounts.fee_ledger.key(),
    )?;

    require_raw_covers_accounted(accounts.vault_a.amount, accounts.domain.accounted_a)?;
    require_raw_covers_accounted(accounts.vault_b.amount, accounts.domain.accounted_b)?;
    require_raw_covers_accounted(
        accounts.fee_vault.amount,
        accounts.fee_ledger.accounted_fee_a,
    )?;

    require!(args.amount_in > 0, CoreError::ZeroAmount);
    require!(
        Clock::get()?.slot <= args.expires_at_slot,
        CoreError::RequestExpired
    );

    let protocol_fee = fee_ceil(args.amount_in, accounts.market.fee_bps)?;
    require!(
        protocol_fee <= args.max_protocol_fee,
        CoreError::ProtocolFeeAboveUserMaximum
    );
    let total_input_debit = args
        .amount_in
        .checked_add(protocol_fee)
        .ok_or(CoreError::ArithmeticOverflow)?;
    require!(
        total_input_debit <= args.max_total_input_debit,
        CoreError::TotalDebitAboveUserMaximum
    );
    require!(
        accounts.user_source_a.amount >= total_input_debit,
        CoreError::InsufficientUserSourceBalance
    );
    require!(
        accounts.domain.accounted_b >= args.min_output_credit,
        CoreError::InsufficientAccountedLiquidity
    );
    let post_accounted_a = accounts
        .domain
        .accounted_a
        .checked_add(args.amount_in)
        .ok_or(CoreError::ArithmeticOverflow)?;
    let post_accounted_fee_a = accounts
        .fee_ledger
        .accounted_fee_a
        .checked_add(protocol_fee)
        .ok_or(CoreError::ArithmeticOverflow)?;

    // Validate every opaque position before the untrusted CPI. The resulting
    // descriptors preserve duplicates and bind the actual effective flags.
    let opaque_descriptors =
        validate_opaque_capabilities(ctx.remaining_accounts, &fixed_envelope_keys)?;
    let mut capability_descriptors =
        Vec::with_capacity(CAPABILITY_PREFIX_ACCOUNTS + opaque_descriptors.len());
    capability_descriptors.push(CapabilityDescriptor {
        key: accounts.engine_state.key(),
        owner: *accounts.engine_state.owner,
        is_writable: true,
        is_signer: false,
        is_executable: false,
    });
    capability_descriptors.push(CapabilityDescriptor {
        key: accounts.instructions_sysvar.key(),
        owner: *accounts.instructions_sysvar.owner,
        is_writable: false,
        is_signer: false,
        is_executable: false,
    });
    capability_descriptors.extend_from_slice(&opaque_descriptors);

    let capability_hash =
        compute_capability_hash(&accounts.engine_program.key(), &capability_descriptors)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    require!(
        capability_hash == args.expected_capability_hash,
        CoreError::CapabilityHashExpectationMismatch
    );
    let payload_hash = compute_payload_hash(&args.opaque_payload)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;

    let request_binding = RequestBinding {
        core_program: ID,
        market: accounts.market.key(),
        domain: accounts.domain.key(),
        engine_program: accounts.engine_program.key(),
        engine_state: accounts.engine_state.key(),
        user_authority: accounts.user.key(),
        user_input: accounts.user_source_a.key(),
        user_output: accounts.user_destination_b.key(),
        mint_in: accounts.mint_a.key(),
        mint_out: accounts.mint_b.key(),
        domain_input_vault: accounts.vault_a.key(),
        domain_output_vault: accounts.vault_b.key(),
        protocol_fee_vault: accounts.fee_vault.key(),
        fee_ledger: accounts.fee_ledger.key(),
        token_program: accounts.token_program.key(),
        engine_revision: accounts.market.engine_revision,
        fee_policy_revision: accounts.market.fee_policy_revision,
        amount_in: args.amount_in,
        protocol_fee,
        max_total_input_debit: args.max_total_input_debit,
        min_output_credit: args.min_output_credit,
        max_protocol_fee: args.max_protocol_fee,
        accounted_input_before: accounts.domain.accounted_a,
        accounted_output_before: accounts.domain.accounted_b,
        accounted_fee_before: accounts.fee_ledger.accounted_fee_a,
        expires_at_slot: args.expires_at_slot,
        capability_hash,
        payload_hash,
    };
    let request_hash = compute_request_hash(&request_binding);
    let opaque_account_count = u16::try_from(ctx.remaining_accounts.len())
        .map_err(|_| error!(CoreError::IntegerConversionFailed))?;
    let engine_request = EngineRequest::new(
        request_hash,
        accounts.market.key(),
        accounts.domain.key(),
        accounts.market.engine_revision,
        args.amount_in,
        accounts.domain.accounted_a,
        accounts.domain.accounted_b,
        opaque_account_count,
        capability_hash,
        &args.opaque_payload,
    )
    .map_err(|_| error!(CoreError::InvalidWireEncoding))?;

    let mut engine_metas = Vec::with_capacity(capability_descriptors.len());
    engine_metas.push(AccountMeta::new(accounts.engine_state.key(), false));
    engine_metas.push(AccountMeta::new_readonly(
        accounts.instructions_sysvar.key(),
        false,
    ));
    for descriptor in &opaque_descriptors {
        engine_metas.push(if descriptor.is_writable {
            AccountMeta::new(descriptor.key, false)
        } else {
            AccountMeta::new_readonly(descriptor.key, false)
        });
    }

    let engine_instruction = Instruction {
        program_id: accounts.engine_program.key(),
        accounts: engine_metas,
        data: encode_evaluate_instruction(&engine_request)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?
            .to_vec(),
    };
    let mut engine_infos = Vec::with_capacity(capability_descriptors.len());
    engine_infos.push(accounts.engine_state.to_account_info());
    engine_infos.push(accounts.instructions_sysvar.to_account_info());
    for descriptor in &opaque_descriptors {
        let normalized_info = ctx
            .remaining_accounts
            .iter()
            .find(|candidate| {
                *candidate.key == descriptor.key
                    && *candidate.owner == descriptor.owner
                    && candidate.executable == descriptor.is_executable
                    && (!descriptor.is_writable || candidate.is_writable)
                    && (!descriptor.is_signer || candidate.is_signer)
            })
            .ok_or(CoreError::OpaqueNormalizedPrivilegeUnavailable)?;
        engine_infos.push(normalized_info.clone());
    }

    invoke(&engine_instruction, &engine_infos)?;

    // Return data is consumed immediately so a later CPI cannot replace it.
    let (receipt_setter, receipt_data) =
        get_return_data().ok_or(CoreError::MissingEngineReceipt)?;
    require_keys_eq!(
        receipt_setter,
        accounts.engine_program.key(),
        CoreError::InvalidEngineReceiptSetter
    );
    let receipt =
        decode_receipt(&receipt_data).map_err(|_| error!(CoreError::InvalidEngineReceipt))?;
    require!(
        receipt.request_hash == request_hash,
        CoreError::EngineReceiptRequestMismatch
    );
    require!(receipt.amount_out > 0, CoreError::ZeroAmount);
    require!(
        receipt.amount_out >= args.min_output_credit,
        CoreError::OutputBelowUserMinimum
    );
    require!(
        receipt.amount_out <= accounts.domain.accounted_b,
        CoreError::InsufficientAccountedLiquidity
    );

    let post_accounted_b = accounts
        .domain
        .accounted_b
        .checked_sub(receipt.amount_out)
        .ok_or(CoreError::ArithmeticOverflow)?;

    let amount_out_bytes = receipt.amount_out.to_le_bytes();
    let protocol_fee_bytes = protocol_fee.to_le_bytes();
    let engine_sequence_bytes = receipt.state_sequence.to_le_bytes();
    let settlement_hash = solana_sha256_hasher::hashv(&[
        SETTLEMENT_HASH_DOMAIN_V0,
        &request_hash,
        &amount_out_bytes,
        &protocol_fee_bytes,
        &engine_sequence_bytes,
    ])
    .to_bytes();

    let accepted_plan = AcceptedEnginePlan {
        amount_out: receipt.amount_out,
        protocol_fee,
        total_input_debit,
        request_hash,
        capability_hash,
        payload_hash,
        settlement_hash,
        engine_sequence: receipt.state_sequence,
        opaque_account_count: u8::try_from(opaque_account_count)
            .map_err(|_| error!(CoreError::IntegerConversionFailed))?,
        post_accounted_a,
        post_accounted_b,
        post_accounted_fee_a,
    };

    settle_engine_generated_probe(accounts, &args, accepted_plan)
}

#[inline(never)]
fn settle_engine_generated_probe<'info>(
    accounts: &mut ExecuteEngineGeneratedProbeV0<'info>,
    args: &ExecuteEngineGeneratedProbeV0Args,
    plan: AcceptedEnginePlan,
) -> Result<()> {
    let source_before = accounts.user_source_a.amount;
    let destination_before = accounts.user_destination_b.amount;
    let vault_a_before = accounts.vault_a.amount;
    let vault_b_before = accounts.vault_b.amount;
    let fee_vault_before = accounts.fee_vault.amount;

    token::transfer_checked(
        CpiContext::new(
            Token::id(),
            TransferChecked {
                from: accounts.user_source_a.to_account_info(),
                mint: accounts.mint_a.to_account_info(),
                to: accounts.vault_a.to_account_info(),
                authority: accounts.user.to_account_info(),
            },
        ),
        args.amount_in,
        accounts.mint_a.decimals,
    )?;
    token::transfer_checked(
        CpiContext::new(
            Token::id(),
            TransferChecked {
                from: accounts.user_source_a.to_account_info(),
                mint: accounts.mint_a.to_account_info(),
                to: accounts.fee_vault.to_account_info(),
                authority: accounts.user.to_account_info(),
            },
        ),
        plan.protocol_fee,
        accounts.mint_a.decimals,
    )?;

    let market_key = accounts.market.key();
    let domain_bump = [accounts.domain.bump];
    let domain_seeds = [DOMAIN_SEED_V0, market_key.as_ref(), domain_bump.as_ref()];
    let signer_seeds = [&domain_seeds[..]];
    token::transfer_checked(
        CpiContext::new_with_signer(
            Token::id(),
            TransferChecked {
                from: accounts.vault_b.to_account_info(),
                mint: accounts.mint_b.to_account_info(),
                to: accounts.user_destination_b.to_account_info(),
                authority: accounts.domain.to_account_info(),
            },
            &signer_seeds,
        ),
        plan.amount_out,
        accounts.mint_b.decimals,
    )?;

    accounts.user_source_a.reload()?;
    accounts.user_destination_b.reload()?;
    accounts.vault_a.reload()?;
    accounts.vault_b.reload()?;
    accounts.fee_vault.reload()?;

    exact_debit(
        source_before,
        accounts.user_source_a.amount,
        plan.total_input_debit,
        CoreError::UnexpectedSourceDebit,
    )?;
    exact_credit(
        vault_a_before,
        accounts.vault_a.amount,
        args.amount_in,
        CoreError::UnexpectedVaultCredit,
    )?;
    exact_credit(
        fee_vault_before,
        accounts.fee_vault.amount,
        plan.protocol_fee,
        CoreError::UnexpectedFeeVaultCredit,
    )?;
    exact_debit(
        vault_b_before,
        accounts.vault_b.amount,
        plan.amount_out,
        CoreError::UnexpectedVaultDebit,
    )?;
    exact_credit(
        destination_before,
        accounts.user_destination_b.amount,
        plan.amount_out,
        CoreError::UnexpectedDestinationCredit,
    )?;

    // Core accounting changes only after every exact token delta is observed.
    accounts.domain.accounted_a = plan.post_accounted_a;
    accounts.domain.accounted_b = plan.post_accounted_b;
    accounts.fee_ledger.accounted_fee_a = plan.post_accounted_fee_a;

    emit!(EngineGeneratedProbeExecutedV0 {
        market: accounts.market.key(),
        domain: accounts.domain.key(),
        engine_program: accounts.engine_program.key(),
        engine_state: accounts.engine_state.key(),
        user: accounts.user.key(),
        mint_a: accounts.mint_a.key(),
        mint_b: accounts.mint_b.key(),
        amount_in: args.amount_in,
        amount_out: plan.amount_out,
        protocol_fee: plan.protocol_fee,
        request_hash: plan.request_hash,
        capability_hash: plan.capability_hash,
        payload_hash: plan.payload_hash,
        settlement_hash: plan.settlement_hash,
        engine_sequence: plan.engine_sequence,
        opaque_account_count: plan.opaque_account_count,
        post_accounted_a: plan.post_accounted_a,
        post_accounted_b: plan.post_accounted_b,
        post_accounted_fee_a: plan.post_accounted_fee_a,
    });

    Ok(())
}
