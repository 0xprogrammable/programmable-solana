//! Raw runtime helpers shared by the private Core instructions.
//!
//! The helpers in this module deliberately keep transaction authentication and
//! the engine callback boundary out of the product-neutral algebra modules.
//! Nothing here is a public interface.

use anchor_lang::{
    prelude::*,
    solana_program::{
        instruction::{get_stack_height, AccountMeta, Instruction, TRANSACTION_LEVEL_STACK_HEIGHT},
        program::{get_return_data, invoke, invoke_signed, set_return_data},
        system_instruction, system_program,
    },
};

use crate::{
    account_segments::{
        load_top_level_instruction, EffectivePrivilege, TopLevelAccountMeta,
        TopLevelInstructionView,
    },
    error::CoreError,
};

/// Exact effective privilege expected for one account occurrence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestedPrivilege {
    pub key: Pubkey,
    pub signer: bool,
    pub writable: bool,
}

impl From<&AccountInfo<'_>> for RequestedPrivilege {
    fn from(account: &AccountInfo<'_>) -> Self {
        Self {
            key: *account.key,
            signer: account.is_signer,
            writable: account.is_writable,
        }
    }
}

/// Runtime-authenticated view of the transaction-level instruction currently
/// executing. `effective_accounts` preserves duplicate positions, but its
/// signer and writable bits are the sanitized message's resolved global
/// privileges for each duplicated key. Original per-position flags are not
/// observable through the Instructions sysvar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticatedTopLevelCall {
    pub instruction_index: u16,
    pub program_id: Pubkey,
    pub effective_accounts: Vec<TopLevelAccountMeta>,
    pub instruction_data: Vec<u8>,
}

/// Exact authority proof accepted by actor-controlled Core instructions.
///
/// A normal wallet key can authorize only an exact transaction-root Core
/// instruction. An off-curve program actor can authorize one direct CPI from
/// its transaction-root parent; the parent must have landed the actor key and
/// the Core frame must receive signer privilege from `invoke_signed`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthenticatedActorInvocation {
    TransactionRoot(AuthenticatedTopLevelCall),
    ProgramActor { parent: TopLevelInstructionView },
}

impl AuthenticatedActorInvocation {
    pub fn top_level_call(&self) -> Option<&AuthenticatedTopLevelCall> {
        match self {
            Self::TransactionRoot(call) => Some(call),
            Self::ProgramActor { .. } => None,
        }
    }
}

/// Applies one exact effective-privilege vector to either actor branch.
/// Transaction-root calls compare the globally resolved message metas exposed
/// by the Instructions sysvar. Program-actor calls cannot recover CPI metas
/// there, so they compare the privileges that actually landed in the Core
/// frame.
pub fn require_actor_invocation_privileges(
    authenticated: &AuthenticatedActorInvocation,
    landed_accounts: &[AccountInfo<'_>],
    expected: &[RequestedPrivilege],
) -> Result<()> {
    require_eq!(
        landed_accounts.len(),
        expected.len(),
        CoreError::DirectAuthorizationNotTransactionRoot
    );
    match authenticated {
        AuthenticatedActorInvocation::TransactionRoot(call) => {
            require_effective_top_level_privileges(call, expected)
        }
        AuthenticatedActorInvocation::ProgramActor { .. } => {
            for (landed, expected) in landed_accounts.iter().zip(expected) {
                require_keys_eq!(
                    *landed.key,
                    expected.key,
                    CoreError::DirectAuthorizationNotTransactionRoot
                );
                require_eq!(
                    landed.is_signer,
                    expected.signer,
                    CoreError::DirectAuthorizationNotTransactionRoot
                );
                require_eq!(
                    landed.is_writable,
                    expected.writable,
                    CoreError::DirectAuthorizationNotTransactionRoot
                );
            }
            Ok(())
        }
    }
}

/// Authenticates the dual actor model without pretending the Instructions
/// sysvar exposes inner-CPI bytes or requested metas.
///
/// `forbidden_core_authorities` must contain every participating Core-derived
/// authority/control PDA in the instruction (callback, stored authorization,
/// intent spend, domain accounting, fee controls, and any instruction-specific
/// Core PDA). This prevents one Core authority from being recycled as an
/// application actor in the program-actor branch.
pub fn authenticate_actor_invocation(
    core_program: &Pubkey,
    landed_accounts: &[AccountInfo<'_>],
    complete_instruction_data: &[u8],
    instructions_sysvar: &AccountInfo<'_>,
    actor_position: usize,
    forbidden_core_authorities: &[Pubkey],
) -> Result<AuthenticatedActorInvocation> {
    let actor = landed_accounts
        .get(actor_position)
        .ok_or(CoreError::DirectAuthorizationNotTransactionRoot)?;
    require!(
        actor.is_signer && !actor.executable,
        CoreError::DirectAuthorizationNotTransactionRoot
    );
    require!(
        forbidden_core_authorities
            .iter()
            .all(|forbidden| forbidden != actor.key),
        CoreError::DirectAuthorizationNotTransactionRoot
    );

    let stack_height = get_stack_height();
    if stack_height == TRANSACTION_LEVEL_STACK_HEIGHT {
        require!(
            actor.key.is_on_curve(),
            CoreError::DirectAuthorizationNotTransactionRoot
        );
        let call = authenticate_exact_top_level_call(
            core_program,
            landed_accounts,
            complete_instruction_data,
            instructions_sysvar,
        )?;
        require_top_level_signer(&call, actor_position, actor.key)?;
        return Ok(AuthenticatedActorInvocation::TransactionRoot(call));
    }

    let direct_parent_height = TRANSACTION_LEVEL_STACK_HEIGHT
        .checked_add(1)
        .ok_or(CoreError::ArithmeticOverflow)?;
    require_eq!(
        stack_height,
        direct_parent_height,
        CoreError::DirectAuthorizationNotTransactionRoot
    );
    require!(
        !actor.key.is_on_curve(),
        CoreError::DirectAuthorizationNotTransactionRoot
    );
    let parent = load_top_level_instruction(instructions_sysvar)?;
    require_keys_neq!(
        parent.program_id,
        *core_program,
        CoreError::DirectAuthorizationNotTransactionRoot
    );
    // The parent meta itself need not request signer: an off-curve PDA becomes
    // signer only in the Core CPI through the parent's `invoke_signed`.
    require!(
        parent.accounts.iter().any(|meta| meta.key == *actor.key),
        CoreError::DirectAuthorizationNotTransactionRoot
    );
    Ok(AuthenticatedActorInvocation::ProgramActor { parent })
}

/// Proves that this invocation is transaction-level and that the runtime's
/// current top-level instruction is the exact Core call being handled.
///
/// The Instructions sysvar and AccountInfo both expose resolved global message
/// privileges, not original source-level flags. Duplicate positions remain
/// ordered, but each duplicate carries the same unioned privilege. Callers
/// separately provide the exact effective privilege expected at every
/// position.
pub fn authenticate_exact_top_level_call(
    core_program: &Pubkey,
    landed_accounts: &[AccountInfo<'_>],
    complete_instruction_data: &[u8],
    instructions_sysvar: &AccountInfo<'_>,
) -> Result<AuthenticatedTopLevelCall> {
    use solana_instructions_sysvar::{load_current_index_checked, load_instruction_at_checked, ID};

    require_eq!(
        get_stack_height(),
        TRANSACTION_LEVEL_STACK_HEIGHT,
        CoreError::DirectAuthorizationNotTransactionRoot
    );
    require_keys_eq!(*instructions_sysvar.key, ID, CoreError::InvalidWireEncoding);
    require_keys_eq!(
        *instructions_sysvar.owner,
        solana_sdk_ids::sysvar::ID,
        CoreError::InvalidWireEncoding
    );
    require!(
        !instructions_sysvar.is_signer && !instructions_sysvar.is_writable,
        CoreError::UnexpectedWritablePrivilege
    );

    let instruction_index = load_current_index_checked(instructions_sysvar)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    let instruction =
        load_instruction_at_checked(usize::from(instruction_index), instructions_sysvar)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?;

    require_keys_eq!(
        instruction.program_id,
        *core_program,
        CoreError::DirectAuthorizationNotTransactionRoot
    );
    require!(
        instruction.data.as_slice() == complete_instruction_data,
        CoreError::DirectAuthorizationNotTransactionRoot
    );
    require_eq!(
        instruction.accounts.len(),
        landed_accounts.len(),
        CoreError::DirectAuthorizationNotTransactionRoot
    );
    for (requested, landed) in instruction.accounts.iter().zip(landed_accounts) {
        require_keys_eq!(
            requested.pubkey,
            *landed.key,
            CoreError::DirectAuthorizationNotTransactionRoot
        );
    }

    Ok(AuthenticatedTopLevelCall {
        instruction_index,
        program_id: instruction.program_id,
        effective_accounts: instruction
            .accounts
            .into_iter()
            .map(|meta| TopLevelAccountMeta {
                key: meta.pubkey,
                signer: meta.is_signer,
                writable: meta.is_writable,
            })
            .collect(),
        instruction_data: instruction.data,
    })
}

/// Matches the ordered, globally resolved top-level metas exactly. This rejects
/// unexpected privilege union from duplicate keys or sibling transaction
/// instructions without claiming access to pre-compilation source flags.
pub fn require_effective_top_level_privileges(
    authenticated: &AuthenticatedTopLevelCall,
    expected: &[RequestedPrivilege],
) -> Result<()> {
    require_eq!(
        authenticated.effective_accounts.len(),
        expected.len(),
        CoreError::DirectAuthorizationNotTransactionRoot
    );
    for (effective, expected) in authenticated.effective_accounts.iter().zip(expected) {
        require_keys_eq!(
            effective.key,
            expected.key,
            CoreError::DirectAuthorizationNotTransactionRoot
        );
        require_eq!(
            effective.signer,
            expected.signer,
            CoreError::DirectAuthorizationNotTransactionRoot
        );
        require_eq!(
            effective.writable,
            expected.writable,
            CoreError::DirectAuthorizationNotTransactionRoot
        );
    }
    Ok(())
}

pub fn require_top_level_signer(
    authenticated: &AuthenticatedTopLevelCall,
    position: usize,
    key: &Pubkey,
) -> Result<()> {
    let meta = authenticated
        .effective_accounts
        .get(position)
        .ok_or(CoreError::DirectAuthorizationNotTransactionRoot)?;
    require_keys_eq!(
        meta.key,
        *key,
        CoreError::DirectAuthorizationNotTransactionRoot
    );
    require!(
        meta.signer,
        CoreError::DirectAuthorizationNotTransactionRoot
    );
    Ok(())
}

/// Invokes one engine transition with the exact callee capability closure:
/// callback signer first, then the opaque tail, and nothing else.
///
/// The engine executable AccountInfo is supplied only as the CPI target. It is
/// intentionally absent from `Instruction.accounts`, so it is not exposed to
/// the engine as an additional callee capability.
pub fn invoke_engine_transition<'info>(
    engine_program: &AccountInfo<'info>,
    callback_authority: &AccountInfo<'info>,
    opaque_accounts: &[AccountInfo<'info>],
    opaque_privileges: &[EffectivePrivilege],
    engine_instruction_data: Vec<u8>,
    callback_signer_seeds: &[&[u8]],
) -> Result<Vec<u8>> {
    require!(
        engine_program.executable,
        CoreError::EngineProgramNotExecutable
    );
    require!(
        !engine_program.is_signer && !engine_program.is_writable,
        CoreError::WritableLoaderIdentityAccount
    );
    require!(
        !callback_authority.executable
            && !callback_authority.is_signer
            && !callback_authority.is_writable,
        CoreError::OpaqueProtectedAlias
    );
    require_eq!(
        opaque_accounts.len(),
        opaque_privileges.len(),
        CoreError::AccountSegmentLengthMismatch
    );

    let mut metas = Vec::with_capacity(1 + opaque_privileges.len());
    metas.push(AccountMeta::new_readonly(*callback_authority.key, true));
    for (account, privilege) in opaque_accounts.iter().zip(opaque_privileges) {
        require_keys_eq!(
            *account.key,
            privilege.key,
            CoreError::AccountSegmentLengthMismatch
        );
        require_keys_eq!(
            *account.owner,
            privilege.owner,
            CoreError::DuplicateAccountIdentityDrift
        );
        require_eq!(
            account.executable,
            privilege.executable,
            CoreError::DuplicateAccountIdentityDrift
        );
        require!(!privilege.signer, CoreError::OpaqueSignerForbidden);
        // A routed caller may downgrade an account before invoking Core. Core
        // must not promise the engine the stronger top-level capability when
        // it cannot actually forward it.
        require_eq!(
            account.is_writable,
            privilege.writable,
            CoreError::UnexpectedWritablePrivilege
        );
        require!(!account.is_signer, CoreError::OpaqueSignerForbidden);
        metas.push(if privilege.writable {
            AccountMeta::new(privilege.key, false)
        } else {
            AccountMeta::new_readonly(privilege.key, false)
        });
    }

    let instruction = Instruction {
        program_id: *engine_program.key,
        accounts: metas,
        data: engine_instruction_data,
    };
    let mut infos = Vec::with_capacity(2 + opaque_accounts.len());
    infos.push(engine_program.clone());
    infos.push(callback_authority.clone());
    infos.extend(opaque_accounts.iter().cloned());

    // Remove any earlier return data. If the engine does not set a final
    // receipt, the setter remains Core rather than accidentally accepting a
    // stale value from an unrelated earlier instruction.
    set_return_data(&[]);
    invoke_signed(&instruction, &infos, &[callback_signer_seeds])?;
    let (setter, receipt) = get_return_data().ok_or(CoreError::InvalidWireEncoding)?;
    require_keys_eq!(setter, *engine_program.key, CoreError::InvalidWireEncoding);
    require!(!receipt.is_empty(), CoreError::InvalidWireEncoding);
    Ok(receipt)
}

/// Creates one exact Core-owned PDA while remaining safe when its address was
/// prefunded. System-owned zero-data PDAs are funded, allocated and assigned in
/// separate steps so an unsolicited lamport transfer cannot squat the address.
pub fn create_core_pda_account_exact<'info>(
    core_program: &Pubkey,
    payer: &AccountInfo<'info>,
    pda: &AccountInfo<'info>,
    system_program_account: &AccountInfo<'info>,
    space: usize,
    pda_signer_seeds: &[&[u8]],
) -> Result<()> {
    require_keys_eq!(
        *system_program_account.key,
        system_program::ID,
        CoreError::InvalidWireEncoding
    );
    require!(
        system_program_account.executable
            && !system_program_account.is_signer
            && !system_program_account.is_writable,
        CoreError::InvalidWireEncoding
    );
    require!(
        payer.is_signer && payer.is_writable,
        CoreError::DirectAuthorizationNotTransactionRoot
    );
    require!(
        pda.is_writable && !pda.is_signer && !pda.executable,
        CoreError::UnexpectedWritablePrivilege
    );
    require_keys_eq!(
        *pda.owner,
        system_program::ID,
        CoreError::InvalidWireEncoding
    );
    require_eq!(pda.data_len(), 0, CoreError::InvalidWireEncoding);
    let derived = Pubkey::create_program_address(pda_signer_seeds, core_program)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    require_keys_eq!(derived, *pda.key, CoreError::InvalidWireEncoding);

    let minimum_balance = Rent::get()?.minimum_balance(space);
    let current_lamports = pda.lamports();
    if current_lamports < minimum_balance {
        let required = minimum_balance
            .checked_sub(current_lamports)
            .ok_or(CoreError::ArithmeticOverflow)?;
        let transfer = system_instruction::transfer(payer.key, pda.key, required);
        invoke(
            &transfer,
            &[payer.clone(), pda.clone(), system_program_account.clone()],
        )?;
    }

    let allocated_space = u64::try_from(space).map_err(|_| CoreError::ArithmeticOverflow)?;
    let allocate = system_instruction::allocate(pda.key, allocated_space);
    invoke_signed(
        &allocate,
        &[pda.clone(), system_program_account.clone()],
        &[pda_signer_seeds],
    )?;
    let assign = system_instruction::assign(pda.key, core_program);
    invoke_signed(
        &assign,
        &[pda.clone(), system_program_account.clone()],
        &[pda_signer_seeds],
    )?;

    require_keys_eq!(*pda.owner, *core_program, CoreError::InvalidWireEncoding);
    require_eq!(pda.data_len(), space, CoreError::InvalidWireEncoding);
    require!(
        pda.lamports() >= minimum_balance,
        CoreError::InvalidWireEncoding
    );
    Ok(())
}
