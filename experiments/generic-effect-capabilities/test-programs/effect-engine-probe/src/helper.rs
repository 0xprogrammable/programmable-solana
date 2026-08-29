//! Opaque helper forwarding for callback-capability and return-setter tests.

use anchor_lang::{
    solana_program::{
        account_info::AccountInfo,
        instruction::{AccountMeta, Instruction},
        program::invoke,
    },
    InstructionData,
};
use generic_effect_private_wire::DISPOSABLE_HELPER_PROGRAM_ID;

use crate::{engine_error, plan::EnginePlan, EngineError, EngineResult};

pub fn invoke_increment<'info>(
    callback_authority: &AccountInfo<'info>,
    opaque_accounts: &[AccountInfo<'info>],
    plan: &EnginePlan,
    amount: u64,
) -> EngineResult<()> {
    let (helper_program, helper_state) = helper_accounts(opaque_accounts, plan)?;
    let data = callback_capability_probe::instruction::Increment { amount }.data();
    invoke_helper(callback_authority, helper_program, helper_state, data)
}

pub fn invoke_descendant_setter<'info>(
    callback_authority: &AccountInfo<'info>,
    opaque_accounts: &[AccountInfo<'info>],
    plan: &EnginePlan,
    receipt_data: Vec<u8>,
) -> EngineResult<()> {
    let (helper_program, helper_state) = helper_accounts(opaque_accounts, plan)?;
    let data = callback_capability_probe::instruction::SetDescendantReceipt { receipt_data }.data();
    invoke_helper(callback_authority, helper_program, helper_state, data)
}

fn helper_accounts<'a, 'info>(
    opaque_accounts: &'a [AccountInfo<'info>],
    plan: &EnginePlan,
) -> EngineResult<(&'a AccountInfo<'info>, &'a AccountInfo<'info>)> {
    if !plan.has_helper() {
        return Err(engine_error(EngineError::IncompleteHelperClosure));
    }
    let helper_program = opaque_accounts
        .get(usize::from(plan.helper_program_position_or_none))
        .ok_or_else(|| engine_error(EngineError::InvalidOpaquePosition))?;
    let helper_state = opaque_accounts
        .get(usize::from(plan.helper_state_position_or_none))
        .ok_or_else(|| engine_error(EngineError::InvalidOpaquePosition))?;

    if *helper_program.key != DISPOSABLE_HELPER_PROGRAM_ID
        || !helper_program.executable
        || helper_program.is_writable
        || helper_program.is_signer
    {
        return Err(engine_error(EngineError::InvalidHelperProgram));
    }
    if *helper_state.owner != *helper_program.key
        || !helper_state.is_writable
        || helper_state.is_signer
        || helper_state.executable
    {
        return Err(engine_error(EngineError::InvalidHelperState));
    }
    if helper_program.key == helper_state.key {
        return Err(engine_error(EngineError::AliasedHelperClosure));
    }
    Ok((helper_program, helper_state))
}

fn invoke_helper<'info>(
    callback_authority: &AccountInfo<'info>,
    helper_program: &AccountInfo<'info>,
    helper_state: &AccountInfo<'info>,
    data: Vec<u8>,
) -> EngineResult<()> {
    let instruction = Instruction {
        program_id: *helper_program.key,
        accounts: vec![
            AccountMeta::new(*helper_state.key, false),
            AccountMeta::new_readonly(*callback_authority.key, true),
        ],
        data,
    };
    invoke(
        &instruction,
        &[
            helper_state.clone(),
            callback_authority.clone(),
            helper_program.clone(),
        ],
    )
    .map_err(|_| engine_error(EngineError::HelperInvocationFailed))
}
