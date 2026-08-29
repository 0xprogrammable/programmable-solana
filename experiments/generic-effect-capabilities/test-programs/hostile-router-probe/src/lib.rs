//! Disposable permissionless router and hostile CPI fixture.
//!
//! The router owns no execution authority. Its success path forwards the exact
//! Core instruction bytes and exact ordered account positions it received.
//! Negative modes deliberately mutate that closure or attempt authority reuse.
//! This crate is private experiment machinery, not a maintained router API.

use anchor_lang::{
    prelude::*,
    solana_program::{
        instruction::{AccountMeta, Instruction},
        program::{invoke, invoke_signed},
        system_program,
    },
    InstructionData,
};
use generic_effect_private_wire::DISPOSABLE_CORE_PROGRAM_ID;

declare_id!("3uWi9x2SRpmjztkpkr2WWeBoVq3exjXG2YfDWLvm8KsQ");

pub const MAX_ROUTER_ACCOUNT_POSITIONS: usize = 64;
pub const MAX_CORE_INSTRUCTION_DATA_LEN: usize = 10_240;
pub const ROUTER_PROGRAM_ACTOR_SEED: &[u8] = b"program-actor-v0";
pub const SIGNED_ACTOR_PAYER_INDEX: usize = 0;
pub const SIGNED_ACTOR_ACTOR_INDEX: usize = 1;
pub const SIGNED_ACTOR_INIT_ACCOUNT_COUNT: usize = 5;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct RouteProbeArgs {
    /// Number of remaining-account positions in the unmodified Core closure.
    /// Attack-only accounts, when needed, follow this prefix.
    pub core_account_count: u8,
    pub mode: RouterMode,
    /// Exact private instruction bytes passed to the disposable Core.
    pub core_instruction_data: Vec<u8>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouterMode {
    ForwardExactOnce,
    ForwardExactTwice,
    DuplicatePosition {
        source_index: u8,
        insert_at: u8,
    },
    ReorderPositions {
        first_index: u8,
        second_index: u8,
    },
    OmitPosition {
        omitted_index: u8,
    },
    AddPosition {
        added_outer_index: u8,
        insert_at: u8,
    },
    ForwardThenReuseCallback {
        helper_program_index: u8,
        helper_state_index: u8,
        callback_authority_index: u8,
        amount: u64,
    },
    /// Requests signer privilege for one router PDA at both the payer and
    /// actor positions of the exact five-account Core initialize closure.
    /// Core must reject this ProgramActor alias even though invoke_signed
    /// validly supplies the signer privilege.
    ForwardInitWithSignedActorAlias,
    /// Positively forwards one exact Core execution while granting signer
    /// privilege to the router's one fixed ProgramActor PDA. The PDA must
    /// occur exactly once in the Core closure and must be readonly in the
    /// outer router frame.
    ForwardExactOnceWithSignedProgramActor,
}

#[program]
pub mod hostile_router_probe {
    use super::*;

    pub fn route<'info>(ctx: Context<'info, Route<'info>>, args: RouteProbeArgs) -> Result<()> {
        validate_router_envelope(&ctx, &args)?;
        let core_count = usize::from(args.core_account_count);
        let core_accounts = &ctx.remaining_accounts[..core_count];

        match args.mode {
            RouterMode::ForwardExactOnce => forward_core(
                &ctx.accounts.core_program.to_account_info(),
                core_accounts,
                ctx.remaining_accounts,
                &args.core_instruction_data,
            ),
            RouterMode::ForwardExactTwice => {
                forward_core(
                    &ctx.accounts.core_program.to_account_info(),
                    core_accounts,
                    ctx.remaining_accounts,
                    &args.core_instruction_data,
                )?;
                forward_core(
                    &ctx.accounts.core_program.to_account_info(),
                    core_accounts,
                    ctx.remaining_accounts,
                    &args.core_instruction_data,
                )?;
                err!(RouterError::ReplayUnexpectedlySucceeded)
            }
            RouterMode::DuplicatePosition {
                source_index,
                insert_at,
            } => forward_mutated(
                &ctx.accounts.core_program.to_account_info(),
                ctx.remaining_accounts,
                &args.core_instruction_data,
                mutated_core_accounts(
                    core_accounts,
                    ctx.remaining_accounts,
                    AccountMutation::Duplicate {
                        source_index,
                        insert_at,
                    },
                )?,
            ),
            RouterMode::ReorderPositions {
                first_index,
                second_index,
            } => forward_mutated(
                &ctx.accounts.core_program.to_account_info(),
                ctx.remaining_accounts,
                &args.core_instruction_data,
                mutated_core_accounts(
                    core_accounts,
                    ctx.remaining_accounts,
                    AccountMutation::Reorder {
                        first_index,
                        second_index,
                    },
                )?,
            ),
            RouterMode::OmitPosition { omitted_index } => forward_mutated(
                &ctx.accounts.core_program.to_account_info(),
                ctx.remaining_accounts,
                &args.core_instruction_data,
                mutated_core_accounts(
                    core_accounts,
                    ctx.remaining_accounts,
                    AccountMutation::Omit { omitted_index },
                )?,
            ),
            RouterMode::AddPosition {
                added_outer_index,
                insert_at,
            } => forward_mutated(
                &ctx.accounts.core_program.to_account_info(),
                ctx.remaining_accounts,
                &args.core_instruction_data,
                mutated_core_accounts(
                    core_accounts,
                    ctx.remaining_accounts,
                    AccountMutation::Add {
                        added_outer_index,
                        insert_at,
                    },
                )?,
            ),
            RouterMode::ForwardThenReuseCallback {
                helper_program_index,
                helper_state_index,
                callback_authority_index,
                amount,
            } => {
                forward_core(
                    &ctx.accounts.core_program.to_account_info(),
                    core_accounts,
                    ctx.remaining_accounts,
                    &args.core_instruction_data,
                )?;
                attempt_callback_reuse(
                    ctx.remaining_accounts,
                    helper_program_index,
                    helper_state_index,
                    callback_authority_index,
                    amount,
                )
            }
            RouterMode::ForwardInitWithSignedActorAlias => forward_init_with_signed_actor_alias(
                &ctx.accounts.core_program.to_account_info(),
                core_accounts,
                ctx.remaining_accounts,
                &args.core_instruction_data,
            ),
            RouterMode::ForwardExactOnceWithSignedProgramActor => {
                forward_core_with_signed_program_actor(
                    &ctx.accounts.core_program.to_account_info(),
                    core_accounts,
                    ctx.remaining_accounts,
                    &args.core_instruction_data,
                )
            }
        }
    }
}

#[derive(Accounts)]
pub struct Route<'info> {
    /// CHECK: This is the one fixed disposable Core target. The handler checks
    /// its executable and effective privilege flags before any CPI.
    #[account(address = DISPOSABLE_CORE_PROGRAM_ID @ RouterError::InvalidCoreProgram)]
    pub core_program: UncheckedAccount<'info>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccountMutation {
    Duplicate {
        source_index: u8,
        insert_at: u8,
    },
    Reorder {
        first_index: u8,
        second_index: u8,
    },
    Omit {
        omitted_index: u8,
    },
    Add {
        added_outer_index: u8,
        insert_at: u8,
    },
}

fn validate_router_envelope(ctx: &Context<Route>, args: &RouteProbeArgs) -> Result<()> {
    let core_program = &ctx.accounts.core_program;
    require!(core_program.executable, RouterError::InvalidCoreProgram);
    require!(
        !core_program.is_signer && !core_program.is_writable,
        RouterError::InvalidCoreProgram
    );
    require!(
        !args.core_instruction_data.is_empty()
            && args.core_instruction_data.len() <= MAX_CORE_INSTRUCTION_DATA_LEN,
        RouterError::InvalidCoreInstructionData
    );
    require!(
        !ctx.remaining_accounts.is_empty()
            && ctx.remaining_accounts.len() <= MAX_ROUTER_ACCOUNT_POSITIONS,
        RouterError::InvalidAccountCount
    );

    let core_count = usize::from(args.core_account_count);
    require!(
        core_count > 0 && core_count <= ctx.remaining_accounts.len(),
        RouterError::InvalidAccountCount
    );
    match args.mode {
        RouterMode::AddPosition {
            added_outer_index, ..
        } => {
            require!(
                ctx.remaining_accounts.len() == core_count + 1
                    && usize::from(added_outer_index) == core_count,
                RouterError::InvalidAttackClosure
            );
        }
        _ => require!(
            ctx.remaining_accounts.len() == core_count,
            RouterError::InvalidAttackClosure
        ),
    }
    Ok(())
}

fn forward_mutated<'info>(
    core_program: &AccountInfo<'info>,
    outer_accounts: &[AccountInfo<'info>],
    instruction_data: &[u8],
    mutated_accounts: Vec<AccountInfo<'info>>,
) -> Result<()> {
    forward_core(
        core_program,
        &mutated_accounts,
        outer_accounts,
        instruction_data,
    )?;
    err!(RouterError::MutatedClosureUnexpectedlyAccepted)
}

fn forward_core<'info>(
    core_program: &AccountInfo<'info>,
    accounts: &[AccountInfo<'info>],
    outer_accounts: &[AccountInfo<'info>],
    instruction_data: &[u8],
) -> Result<()> {
    let instruction = Instruction {
        program_id: DISPOSABLE_CORE_PROGRAM_ID,
        accounts: accounts
            .iter()
            .map(|account| normalized_meta(account, outer_accounts))
            .collect(),
        data: instruction_data.to_vec(),
    };
    let mut infos = Vec::with_capacity(accounts.len() + 1);
    infos.extend(accounts.iter().cloned());
    infos.push(core_program.clone());
    invoke(&instruction, &infos).map_err(Into::into)
}

pub fn router_program_actor_address() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[ROUTER_PROGRAM_ACTOR_SEED], &crate::ID)
}

fn forward_core_with_signed_program_actor<'info>(
    core_program: &AccountInfo<'info>,
    accounts: &[AccountInfo<'info>],
    outer_accounts: &[AccountInfo<'info>],
    instruction_data: &[u8],
) -> Result<()> {
    let (expected_actor, bump) = router_program_actor_address();
    let mut actor_occurrences = accounts
        .iter()
        .filter(|account| *account.key == expected_actor);
    let actor = actor_occurrences
        .next()
        .ok_or_else(|| error!(RouterError::InvalidProgramActorProbe))?;
    require!(
        actor_occurrences.next().is_none(),
        RouterError::InvalidProgramActorProbe
    );
    require!(
        !actor.is_signer
            && !actor.is_writable
            && !actor.executable
            && *actor.owner == system_program::ID
            && actor.data_len() == 0,
        RouterError::InvalidProgramActorProbe
    );

    let instruction = Instruction {
        program_id: DISPOSABLE_CORE_PROGRAM_ID,
        accounts: accounts
            .iter()
            .map(|account| {
                if *account.key == expected_actor {
                    AccountMeta::new_readonly(expected_actor, true)
                } else {
                    normalized_meta(account, outer_accounts)
                }
            })
            .collect(),
        data: instruction_data.to_vec(),
    };
    let mut infos = Vec::with_capacity(accounts.len() + 1);
    infos.extend(accounts.iter().cloned());
    infos.push(core_program.clone());
    let bump_seed = [bump];
    let actor_seeds = [ROUTER_PROGRAM_ACTOR_SEED, bump_seed.as_ref()];
    invoke_signed(&instruction, &infos, &[&actor_seeds]).map_err(Into::into)
}

fn forward_init_with_signed_actor_alias<'info>(
    core_program: &AccountInfo<'info>,
    accounts: &[AccountInfo<'info>],
    outer_accounts: &[AccountInfo<'info>],
    instruction_data: &[u8],
) -> Result<()> {
    require_eq!(
        accounts.len(),
        SIGNED_ACTOR_INIT_ACCOUNT_COUNT,
        RouterError::InvalidProgramActorProbe
    );
    let payer = &accounts[SIGNED_ACTOR_PAYER_INDEX];
    let actor = &accounts[SIGNED_ACTOR_ACTOR_INDEX];
    let (expected_actor, bump) = router_program_actor_address();
    require_keys_eq!(
        *payer.key,
        expected_actor,
        RouterError::InvalidProgramActorProbe
    );
    require_keys_eq!(
        *actor.key,
        expected_actor,
        RouterError::InvalidProgramActorProbe
    );
    require!(
        payer.is_writable
            && actor.is_writable
            && !payer.is_signer
            && !actor.is_signer
            && !payer.executable
            && !actor.executable
            && *payer.owner == system_program::ID
            && *actor.owner == system_program::ID
            && payer.data_len() == 0
            && actor.data_len() == 0,
        RouterError::InvalidProgramActorProbe
    );

    let instruction = Instruction {
        program_id: DISPOSABLE_CORE_PROGRAM_ID,
        accounts: accounts
            .iter()
            .enumerate()
            .map(|(position, account)| {
                if matches!(
                    position,
                    SIGNED_ACTOR_PAYER_INDEX | SIGNED_ACTOR_ACTOR_INDEX
                ) {
                    AccountMeta::new(*account.key, true)
                } else {
                    normalized_meta(account, outer_accounts)
                }
            })
            .collect(),
        data: instruction_data.to_vec(),
    };
    let mut infos = Vec::with_capacity(accounts.len() + 1);
    infos.extend(accounts.iter().cloned());
    infos.push(core_program.clone());
    let bump_seed = [bump];
    let actor_seeds = [ROUTER_PROGRAM_ACTOR_SEED, bump_seed.as_ref()];
    invoke_signed(&instruction, &infos, &[&actor_seeds]).map_err(Into::into)
}

fn mutated_core_accounts<'info>(
    core_accounts: &[AccountInfo<'info>],
    outer_accounts: &[AccountInfo<'info>],
    mutation: AccountMutation,
) -> Result<Vec<AccountInfo<'info>>> {
    let mut accounts = core_accounts.to_vec();
    match mutation {
        AccountMutation::Duplicate {
            source_index,
            insert_at,
        } => {
            let source = account_at(core_accounts, source_index)?.clone();
            let insert_at = insertion_index(accounts.len(), insert_at)?;
            accounts.insert(insert_at, source);
        }
        AccountMutation::Reorder {
            first_index,
            second_index,
        } => {
            let first = existing_index(accounts.len(), first_index)?;
            let second = existing_index(accounts.len(), second_index)?;
            require!(first != second, RouterError::InvalidMutation);
            accounts.swap(first, second);
        }
        AccountMutation::Omit { omitted_index } => {
            let omitted = existing_index(accounts.len(), omitted_index)?;
            accounts.remove(omitted);
        }
        AccountMutation::Add {
            added_outer_index,
            insert_at,
        } => {
            let added = account_at(outer_accounts, added_outer_index)?.clone();
            require!(
                usize::from(added_outer_index) >= core_accounts.len(),
                RouterError::InvalidMutation
            );
            let insert_at = insertion_index(accounts.len(), insert_at)?;
            accounts.insert(insert_at, added);
        }
    }
    Ok(accounts)
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

fn normalized_meta(account: &AccountInfo<'_>, all_accounts: &[AccountInfo<'_>]) -> AccountMeta {
    let (is_writable, is_signer) = all_accounts
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

fn existing_index(len: usize, index: u8) -> Result<usize> {
    let index = usize::from(index);
    require!(index < len, RouterError::InvalidAccountIndex);
    Ok(index)
}

fn insertion_index(len: usize, index: u8) -> Result<usize> {
    let index = usize::from(index);
    require!(index <= len, RouterError::InvalidAccountIndex);
    Ok(index)
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
    #[msg("The serialized Core instruction is empty or exceeds the CPI data limit")]
    InvalidCoreInstructionData,
    #[msg("The router account-position declaration is invalid")]
    InvalidAccountCount,
    #[msg("The hostile mode has an invalid extra-account closure")]
    InvalidAttackClosure,
    #[msg("The hostile mode references an unavailable account position")]
    InvalidAccountIndex,
    #[msg("The hostile account-position mutation is degenerate or invalid")]
    InvalidMutation,
    #[msg("Two hostile-probe roles alias the same public key")]
    AliasedAttackRole,
    #[msg("The hostile probe amount must be nonzero")]
    InvalidProbeAmount,
    #[msg("The second exact Core execution unexpectedly succeeded")]
    ReplayUnexpectedlySucceeded,
    #[msg("Core accepted a duplicate, reordered, omitted, or added account closure")]
    MutatedClosureUnexpectedlyAccepted,
    #[msg("The callback helper program is invalid")]
    InvalidHelperProgram,
    #[msg("The callback helper state is invalid")]
    InvalidHelperState,
    #[msg("The callback signer privilege leaked back into the router frame")]
    CallbackSignerLeaked,
    #[msg("The callback capability was reusable after Core returned")]
    CallbackReuseUnexpectedlySucceeded,
    #[msg("The signed ProgramActor alias probe is malformed")]
    InvalidProgramActorProbe,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_arguments_round_trip_for_every_mode() {
        let modes = [
            RouterMode::ForwardExactOnce,
            RouterMode::ForwardExactTwice,
            RouterMode::DuplicatePosition {
                source_index: 1,
                insert_at: 2,
            },
            RouterMode::ReorderPositions {
                first_index: 3,
                second_index: 4,
            },
            RouterMode::OmitPosition { omitted_index: 5 },
            RouterMode::AddPosition {
                added_outer_index: 10,
                insert_at: 6,
            },
            RouterMode::ForwardThenReuseCallback {
                helper_program_index: 7,
                helper_state_index: 8,
                callback_authority_index: 9,
                amount: 10,
            },
            RouterMode::ForwardInitWithSignedActorAlias,
            RouterMode::ForwardExactOnceWithSignedProgramActor,
        ];

        for mode in modes {
            let expected = RouteProbeArgs {
                core_account_count: 19,
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
    fn router_program_id_is_disposable_and_stable() {
        assert_eq!(crate::ID.to_bytes(), [43_u8; 32]);
        assert_eq!(DISPOSABLE_CORE_PROGRAM_ID.to_bytes(), [41_u8; 32]);
        assert_eq!(
            crate::ID.to_string(),
            "3uWi9x2SRpmjztkpkr2WWeBoVq3exjXG2YfDWLvm8KsQ"
        );
    }

    #[test]
    fn signed_program_actor_address_is_stable_and_off_curve() {
        let (actor, bump) = router_program_actor_address();
        assert!(!actor.is_on_curve());
        assert_eq!(
            Pubkey::create_program_address(&[ROUTER_PROGRAM_ACTOR_SEED, &[bump]], &crate::ID)
                .unwrap(),
            actor
        );
    }

    #[test]
    fn signed_program_actor_forward_mode_has_one_stable_codec_byte() {
        let mut encoded = Vec::new();
        RouterMode::ForwardExactOnceWithSignedProgramActor
            .serialize(&mut encoded)
            .unwrap();
        assert_eq!(encoded, vec![8]);
        assert_eq!(
            RouterMode::deserialize(&mut encoded.as_slice()).unwrap(),
            RouterMode::ForwardExactOnceWithSignedProgramActor
        );
    }

    #[test]
    fn signed_program_actor_route_has_one_exact_instruction_encoding() {
        let args = RouteProbeArgs {
            core_account_count: 2,
            mode: RouterMode::ForwardExactOnceWithSignedProgramActor,
            core_instruction_data: vec![0xAA, 0xBB],
        };
        let encoded = crate::instruction::Route { args: args.clone() }.data();
        assert_eq!(
            encoded,
            vec![
                229, 23, 203, 151, 122, 227, 173, 42, // Anchor route discriminator.
                2, 8, // Core account count and stable enum discriminant.
                2, 0, 0, 0, 0xAA, 0xBB, // Exact length-prefixed Core data.
            ]
        );

        let mut payload = &encoded[8..];
        let decoded = RouteProbeArgs::deserialize(&mut payload).unwrap();
        assert!(payload.is_empty());
        assert_eq!(decoded, args);
    }
}
