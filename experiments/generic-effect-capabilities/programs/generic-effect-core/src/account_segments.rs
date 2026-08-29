//! Exact account segmentation and landing-time privilege normalization.

use anchor_lang::prelude::*;

use crate::{
    constants::{
        FIXED_ACCOUNT_COUNT, MAX_AUTHORIZATION_CONTROL_ACCOUNTS, MAX_DOMAIN_CONTROL_ACCOUNTS,
        MAX_FEE_CONTROL_ACCOUNTS, MAX_LOADER_POLICY_ACCOUNTS, MAX_OPAQUE_CAPABILITIES,
        MAX_PROTECTED_PROFILE_ACCOUNTS, MAX_SETTLEMENT_CAPABILITIES,
    },
    error::CoreError,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SegmentCounts {
    pub loader_policy: u8,
    pub domain_controls: u8,
    pub authorization_controls: u8,
    pub protected_profile: u8,
    pub fee_controls: u8,
    pub settlement: u8,
    pub opaque: u8,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AccountRange {
    pub start: usize,
    pub end: usize,
}

impl AccountRange {
    pub fn len(self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    pub fn contains(self, position: usize) -> bool {
        position >= self.start && position < self.end
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AccountSegments {
    pub fixed: AccountRange,
    pub loader_policy: AccountRange,
    pub domain_controls: AccountRange,
    pub authorization_controls: AccountRange,
    pub protected_profile: AccountRange,
    pub fee_controls: AccountRange,
    pub settlement: AccountRange,
    pub opaque: AccountRange,
}

impl AccountSegments {
    pub fn parse(counts: SegmentCounts, landed_len: usize) -> Result<Self> {
        validate_count(counts.loader_policy, MAX_LOADER_POLICY_ACCOUNTS)?;
        validate_count(counts.domain_controls, MAX_DOMAIN_CONTROL_ACCOUNTS)?;
        validate_count(
            counts.authorization_controls,
            MAX_AUTHORIZATION_CONTROL_ACCOUNTS,
        )?;
        validate_count(counts.protected_profile, MAX_PROTECTED_PROFILE_ACCOUNTS)?;
        validate_count(counts.fee_controls, MAX_FEE_CONTROL_ACCOUNTS)?;
        validate_count(counts.settlement, MAX_SETTLEMENT_CAPABILITIES)?;
        validate_count(counts.opaque, MAX_OPAQUE_CAPABILITIES)?;

        let mut cursor = FIXED_ACCOUNT_COUNT;
        let fixed = AccountRange {
            start: 0,
            end: FIXED_ACCOUNT_COUNT,
        };
        let loader_policy = take_range(&mut cursor, counts.loader_policy)?;
        let domain_controls = take_range(&mut cursor, counts.domain_controls)?;
        let authorization_controls = take_range(&mut cursor, counts.authorization_controls)?;
        let protected_profile = take_range(&mut cursor, counts.protected_profile)?;
        let fee_controls = take_range(&mut cursor, counts.fee_controls)?;
        let settlement = take_range(&mut cursor, counts.settlement)?;
        let opaque = take_range(&mut cursor, counts.opaque)?;

        require_eq!(cursor, landed_len, CoreError::AccountSegmentLengthMismatch);

        Ok(Self {
            fixed,
            loader_policy,
            domain_controls,
            authorization_controls,
            protected_profile,
            fee_controls,
            settlement,
            opaque,
        })
    }

    pub fn dynamic_ranges(self) -> [AccountRange; 7] {
        [
            self.loader_policy,
            self.domain_controls,
            self.authorization_controls,
            self.protected_profile,
            self.fee_controls,
            self.settlement,
            self.opaque,
        ]
    }
}

fn validate_count(count: u8, maximum: usize) -> Result<()> {
    require!(
        usize::from(count) <= maximum,
        CoreError::ExperimentLimitExceeded
    );
    Ok(())
}

fn take_range(cursor: &mut usize, count: u8) -> Result<AccountRange> {
    let start = *cursor;
    let end = start
        .checked_add(usize::from(count))
        .ok_or(CoreError::AccountSegmentOverflow)?;
    *cursor = end;
    Ok(AccountRange { start, end })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LandedAccount {
    pub key: Pubkey,
    pub owner: Pubkey,
    pub executable: bool,
    /// Privilege visible to Core in the current invocation. This may have been
    /// downgraded or augmented by an invoking router and is therefore not, by
    /// itself, the security privilege used below.
    pub current_signer: bool,
    pub current_writable: bool,
}

impl LandedAccount {
    pub fn from_account_info(account: &AccountInfo<'_>) -> Self {
        Self {
            key: *account.key,
            owner: *account.owner,
            executable: account.executable,
            current_signer: account.is_signer,
            current_writable: account.is_writable,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectivePrivilege {
    pub key: Pubkey,
    pub owner: Pubkey,
    pub executable: bool,
    pub signer: bool,
    pub writable: bool,
}

/// One resolved meta from an authenticated top-level instruction in the
/// Instructions sysvar. Its key is already resolved across static and ALT
/// addresses by the runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TopLevelAccountMeta {
    pub key: Pubkey,
    pub signer: bool,
    pub writable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TopLevelInstructionView {
    pub program_id: Pubkey,
    pub accounts: Vec<TopLevelAccountMeta>,
    pub data: Vec<u8>,
}

/// Loads the runtime-authenticated current top-level instruction. During a
/// routed CPI the sysvar index remains the router instruction, which is exactly
/// what prevents downgraded CPI metas from hiding landing-time privileges.
pub fn load_top_level_instruction(
    instructions_sysvar: &AccountInfo<'_>,
) -> Result<TopLevelInstructionView> {
    use solana_instructions_sysvar::{load_current_index_checked, load_instruction_at_checked};
    validate_instructions_sysvar(instructions_sysvar)?;
    let index = usize::from(
        load_current_index_checked(instructions_sysvar)
            .map_err(|_| error!(CoreError::InvalidWireEncoding))?,
    );
    let instruction = load_instruction_at_checked(index, instructions_sysvar)
        .map_err(|_| error!(CoreError::InvalidWireEncoding))?;
    Ok(TopLevelInstructionView {
        program_id: instruction.program_id,
        accounts: instruction
            .accounts
            .into_iter()
            .map(|meta| TopLevelAccountMeta {
                key: meta.pubkey,
                signer: meta.is_signer,
                writable: meta.is_writable,
            })
            .collect(),
        data: instruction.data,
    })
}

pub fn require_exact_transaction_root_invocation(
    top_level: &TopLevelInstructionView,
    expected_program: &Pubkey,
    expected_accounts: &[TopLevelAccountMeta],
    expected_data: &[u8],
) -> Result<()> {
    use anchor_lang::solana_program::instruction::{
        get_stack_height, TRANSACTION_LEVEL_STACK_HEIGHT,
    };
    require_eq!(
        get_stack_height(),
        TRANSACTION_LEVEL_STACK_HEIGHT,
        CoreError::DirectAuthorizationNotTransactionRoot
    );
    require_keys_eq!(
        top_level.program_id,
        *expected_program,
        CoreError::DirectAuthorizationNotTransactionRoot
    );
    require!(
        top_level.data.as_slice() == expected_data
            && top_level.accounts.as_slice() == expected_accounts,
        CoreError::DirectAuthorizationNotTransactionRoot
    );
    Ok(())
}

/// Loads every resolved account meta from every top-level transaction
/// instruction. Loader identity accounts use this transaction-wide union so a
/// router cannot hide a signer/writable occurrence by downgrading the CPI that
/// reaches Core, and a later top-level instruction cannot mutate an account
/// after Core has authenticated it as read-only.
pub fn load_all_top_level_account_metas(
    instructions_sysvar: &AccountInfo<'_>,
) -> Result<Vec<TopLevelAccountMeta>> {
    use anchor_lang::solana_program::program_error::ProgramError;
    use solana_instructions_sysvar::load_instruction_at_checked;

    validate_instructions_sysvar(instructions_sysvar)?;
    let mut metas = Vec::new();
    let mut index = 0usize;
    let mut saw_instruction = false;
    loop {
        match load_instruction_at_checked(index, instructions_sysvar) {
            Ok(instruction) => {
                saw_instruction = true;
                metas.extend(
                    instruction
                        .accounts
                        .into_iter()
                        .map(|meta| TopLevelAccountMeta {
                            key: meta.pubkey,
                            signer: meta.is_signer,
                            writable: meta.is_writable,
                        }),
                );
                index = index.checked_add(1).ok_or(CoreError::ArithmeticOverflow)?;
            }
            Err(ProgramError::InvalidArgument) => break,
            Err(_) => return err!(CoreError::InvalidWireEncoding),
        }
    }
    require!(saw_instruction, CoreError::InvalidWireEncoding);
    Ok(metas)
}

fn validate_instructions_sysvar(instructions_sysvar: &AccountInfo<'_>) -> Result<()> {
    use solana_instructions_sysvar::ID;
    require_keys_eq!(*instructions_sysvar.key, ID, CoreError::InvalidWireEncoding);
    require_keys_eq!(
        *instructions_sysvar.owner,
        solana_sdk_ids::sysvar::ID,
        CoreError::InvalidWireEncoding
    );
    require!(
        !instructions_sysvar.is_signer,
        CoreError::UnexpectedSignerPrivilege
    );
    require!(
        !instructions_sysvar.is_writable,
        CoreError::UnexpectedWritablePrivilege
    );
    Ok(())
}

/// Computes the security identity of every landed position after unioning
/// signer and writable privilege by public key. Order and multiplicity remain
/// unchanged in the returned vector.
pub fn union_effective_privileges(
    accounts: &[LandedAccount],
    transaction_metas: &[TopLevelAccountMeta],
) -> Result<Vec<EffectivePrivilege>> {
    let mut result = Vec::with_capacity(accounts.len());
    for account in accounts {
        let mut signer = false;
        let mut writable = false;
        for occurrence in accounts
            .iter()
            .filter(|candidate| candidate.key == account.key)
        {
            require_keys_eq!(
                occurrence.owner,
                account.owner,
                CoreError::DuplicateAccountIdentityDrift
            );
            require_eq!(
                occurrence.executable,
                account.executable,
                CoreError::DuplicateAccountIdentityDrift
            );
            signer |= occurrence.current_signer;
            writable |= occurrence.current_writable;
        }
        for occurrence in transaction_metas
            .iter()
            .filter(|candidate| candidate.key == account.key)
        {
            signer |= occurrence.signer;
            writable |= occurrence.writable;
        }
        result.push(EffectivePrivilege {
            key: account.key,
            owner: account.owner,
            executable: account.executable,
            signer,
            writable,
        });
    }
    Ok(result)
}

pub fn snapshot_and_union(
    accounts: &[AccountInfo<'_>],
    transaction_metas: &[TopLevelAccountMeta],
) -> Result<Vec<EffectivePrivilege>> {
    let landed = accounts
        .iter()
        .map(LandedAccount::from_account_info)
        .collect::<Vec<_>>();
    union_effective_privileges(&landed, transaction_metas)
}

pub fn has_transaction_signer(key: &Pubkey, top_level_metas: &[TopLevelAccountMeta]) -> bool {
    top_level_metas
        .iter()
        .any(|meta| meta.key == *key && meta.signer)
}

pub fn require_readonly_non_signer(privilege: &EffectivePrivilege) -> Result<()> {
    require!(!privilege.signer, CoreError::UnexpectedSignerPrivilege);
    require!(!privilege.writable, CoreError::UnexpectedWritablePrivilege);
    Ok(())
}

pub fn require_writable_non_signer(privilege: &EffectivePrivilege) -> Result<()> {
    require!(!privilege.signer, CoreError::UnexpectedSignerPrivilege);
    require!(privilege.writable, CoreError::UnexpectedWritablePrivilege);
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlReference {
    pub offset: u8,
    pub writable: bool,
    pub signer: bool,
}

/// Proves that row offsets form an exact one-to-one partition of a control
/// segment. This is deliberately independent of account ownership; the owning
/// profile validates those facts after the positional proof succeeds.
pub fn validate_control_partition(
    segment: AccountRange,
    references: &[ControlReference],
    effective: &[EffectivePrivilege],
) -> Result<()> {
    require_eq!(
        references.len(),
        segment.len(),
        CoreError::UnreferencedControlAccount
    );
    require!(
        effective.len() >= segment.end,
        CoreError::AccountSegmentLengthMismatch
    );

    let mut consumed = vec![false; segment.len()];
    for reference in references {
        let relative = usize::from(reference.offset);
        require!(
            relative < segment.len(),
            CoreError::AccountSegmentLengthMismatch
        );
        require!(
            !consumed[relative],
            CoreError::OverlappingAccountControlOffset
        );
        consumed[relative] = true;

        let privilege = &effective[segment.start + relative];
        require_eq!(
            privilege.writable,
            reference.writable,
            CoreError::UnexpectedWritablePrivilege
        );
        require_eq!(
            privilege.signer,
            reference.signer,
            CoreError::UnexpectedSignerPrivilege
        );
    }

    require!(
        consumed.iter().all(|is_consumed| *is_consumed),
        CoreError::UnreferencedControlAccount
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(key_byte: u8, signer: bool, writable: bool) -> LandedAccount {
        LandedAccount {
            key: Pubkey::new_from_array([key_byte; 32]),
            owner: Pubkey::new_from_array([99; 32]),
            executable: false,
            current_signer: signer,
            current_writable: writable,
        }
    }

    #[test]
    fn segments_must_consume_the_exact_outer_slice() {
        let counts = SegmentCounts {
            loader_policy: 1,
            settlement: 2,
            opaque: 1,
            ..SegmentCounts::default()
        };
        let segments = AccountSegments::parse(counts, 10).unwrap();
        assert_eq!(segments.loader_policy, AccountRange { start: 6, end: 7 });
        assert_eq!(segments.settlement, AccountRange { start: 7, end: 9 });
        assert_eq!(segments.opaque, AccountRange { start: 9, end: 10 });
        assert!(AccountSegments::parse(counts, 11).is_err());
        assert!(AccountSegments::parse(counts, 9).is_err());
    }

    #[test]
    fn privilege_union_is_by_key_and_preserves_positions() {
        let accounts = vec![
            account(1, false, false),
            account(2, false, false),
            account(1, true, true),
        ];
        let top_level = vec![TopLevelAccountMeta {
            key: accounts[1].key,
            signer: true,
            writable: true,
        }];
        let normalized = union_effective_privileges(&accounts, &top_level).unwrap();
        assert_eq!(normalized.len(), 3);
        assert!(normalized[0].signer && normalized[0].writable);
        assert!(normalized[1].signer && normalized[1].writable);
        assert!(normalized[2].signer && normalized[2].writable);
        assert_eq!(normalized[0].key, normalized[2].key);
    }
}
