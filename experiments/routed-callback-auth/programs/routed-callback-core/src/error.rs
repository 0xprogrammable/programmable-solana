use anchor_lang::prelude::*;

#[error_code]
pub enum CoreError {
    #[msg("This instruction accepts no remaining accounts")]
    UnexpectedRemainingAccounts,
    #[msg("The opaque capability closure exceeds the experiment limit")]
    TooManyOpaqueAccounts,
    #[msg("The opaque payload exceeds the experiment limit")]
    OpaquePayloadTooLarge,
    #[msg("An opaque capability must not be a signer")]
    OpaqueSignerForbidden,
    #[msg("An opaque capability aliases a fixed execution-envelope role")]
    OpaqueFixedRoleAlias,
    #[msg("An opaque capability is owned by this Core program")]
    OpaqueCoreOwnedAccount,
    #[msg("An executable opaque capability must not be writable")]
    OpaqueExecutableWritable,
    #[msg("A writable opaque capability must not be owned by a protected token program")]
    OpaqueProtectedTokenAccountWritable,
    #[msg("The normalized opaque capability cannot be represented by a supplied account position")]
    OpaqueNormalizedPrivilegeUnavailable,
    #[msg("The observed capability closure does not match the user's expected capability hash")]
    CapabilityHashExpectationMismatch,
    #[msg("The timing mode is not supported by this experiment")]
    UnsupportedTimingMode,
    #[msg("The wire codec rejected non-canonical data")]
    InvalidWireEncoding,
    #[msg("The intent names a different Core program")]
    IntentCoreProgramMismatch,
    #[msg("The intent names a different user authority")]
    IntentUserAuthorityMismatch,
    #[msg("The intent names a different user input account")]
    IntentUserInputMismatch,
    #[msg("The intent names a different input mint")]
    IntentInputMintMismatch,
    #[msg("The intent names a different token program")]
    IntentTokenProgramMismatch,
    #[msg("Semantic account roles must use distinct addresses")]
    DuplicateAccountRole,
    #[msg("The selected asset index is not supported by this experiment")]
    UnsupportedAssetIndex,
    #[msg("The amount must be greater than zero")]
    ZeroAmount,
    #[msg("The arithmetic operation overflowed or underflowed")]
    ArithmeticOverflow,
    #[msg("The result does not fit in the target integer type")]
    IntegerConversionFailed,
    #[msg("The two market mints must be distinct")]
    IdenticalMints,
    #[msg("Wrapped native SOL is outside the token profile")]
    NativeMintUnsupported,
    #[msg("The mint is not initialized")]
    MintNotInitialized,
    #[msg("A mint freeze authority is outside the token profile")]
    MintFreezeAuthorityUnsupported,
    #[msg("The token account is not initialized")]
    TokenAccountNotInitialized,
    #[msg("The token account is frozen")]
    FrozenTokenAccount,
    #[msg("A native token account is outside the token profile")]
    NativeTokenAccountUnsupported,
    #[msg("The token account mint does not match the authenticated market")]
    InvalidTokenMint,
    #[msg("The token account authority does not match the authenticated role")]
    InvalidTokenAuthority,
    #[msg("A token delegate is outside the protected account profile")]
    TokenDelegateUnsupported,
    #[msg("The token account already has a delegate")]
    ExistingTokenDelegate,
    #[msg("A token close authority is outside the protected account profile")]
    TokenCloseAuthorityUnsupported,
    #[msg("The delegated amount must be zero when no delegate exists")]
    InvalidDelegatedAmount,
    #[msg("The delegated amount does not equal the intent's exact total debit")]
    DelegatedAmountMismatch,
    #[msg("The delegated spend authority does not authenticate this intent")]
    InvalidSpendAuthority,
    #[msg("The delegated spend authorization was not fully consumed")]
    SpendDelegateNotCleared,
    #[msg("ApproveChecked unexpectedly changed the user's token balance")]
    SourceBalanceChangedDuringAuthorization,
    #[msg("The market PDA or stored market bump is invalid")]
    InvalidMarket,
    #[msg("The domain is not immutably admitted to this market")]
    InvalidDomainAdmission,
    #[msg("The domain PDA or stored domain bump is invalid")]
    InvalidDomain,
    #[msg("The domain vault is not the canonical vault for this asset")]
    InvalidDomainVault,
    #[msg("The fee ledger is not canonical for this market")]
    InvalidFeeLedger,
    #[msg("The fee vault is not canonical for this ledger")]
    InvalidFeeVault,
    #[msg("The market is bound to a different engine program")]
    InvalidEngineProgram,
    #[msg("The market is bound to a different engine state")]
    InvalidEngineState,
    #[msg("The domain is bound to a different engine revision")]
    InvalidEngineRevision,
    #[msg("The market fee policy does not match the experiment policy")]
    InvalidFeePolicy,
    #[msg("The engine state must be owned by the selected engine program")]
    InvalidEngineStateOwner,
    #[msg("The selected engine program must be executable")]
    EngineProgramNotExecutable,
    #[msg("The selected engine state must not be executable")]
    EngineStateExecutable,
    #[msg("The Core program cannot be selected as its own engine")]
    CoreCannotBeEngine,
    #[msg("A read-only account was supplied with writable privilege")]
    UnexpectedWritablePrivilege,
    #[msg("A fixed non-user account was supplied with signer privilege")]
    UnexpectedSignerPrivilege,
    #[msg("The raw vault balance is below the Core-accounted balance")]
    VaultBelowAccountedBalance,
    #[msg("The engine-selected output exceeds Core-accounted liquidity")]
    InsufficientAccountedLiquidity,
    #[msg("The user source cannot cover the input and protocol fee")]
    InsufficientUserSourceBalance,
    #[msg("The observed token source debit was not exact")]
    UnexpectedSourceDebit,
    #[msg("The observed token destination credit was not exact")]
    UnexpectedDestinationCredit,
    #[msg("The observed vault credit was not exact")]
    UnexpectedVaultCredit,
    #[msg("The observed vault debit was not exact")]
    UnexpectedVaultDebit,
    #[msg("The observed fee-vault credit was not exact")]
    UnexpectedFeeVaultCredit,
    #[msg("The request has expired")]
    RequestExpired,
    #[msg("The engine-selected output is below the user's minimum")]
    OutputBelowUserMinimum,
    #[msg("The Core-derived protocol fee exceeds the user's ceiling")]
    ProtocolFeeAboveUserMaximum,
    #[msg("The total input debit exceeds the user's ceiling")]
    TotalDebitAboveUserMaximum,
    #[msg("The instruction must be invoked directly by the transaction")]
    DirectInvocationRequired,
    #[msg("The callback authority is not canonical for this execution phase")]
    InvalidCallbackAuthority,
    #[msg("The engine did not return a receipt")]
    MissingEngineReceipt,
    #[msg("The engine receipt was set by a different program")]
    InvalidEngineReceiptSetter,
    #[msg("The engine receipt could not be decoded")]
    InvalidEngineReceipt,
    #[msg("The engine receipt authenticates a different phase")]
    EngineReceiptPhaseMismatch,
    #[msg("The engine receipt authenticates a different intent")]
    EngineReceiptIntentMismatch,
    #[msg("The engine receipt authenticates a different execution")]
    EngineReceiptExecutionMismatch,
    #[msg("The read-only prepare phase unexpectedly changed the engine sequence")]
    PrepareSequenceChanged,
    #[msg("The transition phase did not advance the engine sequence exactly once")]
    TransitionSequenceMismatch,
    #[msg("The commit phase did not advance the engine sequence exactly once")]
    CommitSequenceMismatch,
    #[msg("The commit phase returned a different output amount than prepare")]
    CommitOutputMismatch,
}
