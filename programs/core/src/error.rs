use anchor_lang::prelude::*;

#[error_code]
pub enum CoreError {
    #[msg("This instruction accepts no remaining accounts")]
    UnexpectedRemainingAccounts,
    #[msg("Semantic account roles must use distinct addresses")]
    DuplicateAccountRole,
    #[msg("The selected asset index is not supported by this V0 experiment")]
    UnsupportedAssetIndex,
    #[msg("The amount must be greater than zero")]
    ZeroAmount,
    #[msg("The arithmetic operation overflowed or underflowed")]
    ArithmeticOverflow,
    #[msg("The result does not fit in the target integer type")]
    IntegerConversionFailed,
    #[msg("The two market mints must be distinct")]
    IdenticalMints,
    #[msg("Wrapped native SOL is outside the V0 token profile")]
    NativeMintUnsupported,
    #[msg("The mint is not initialized")]
    MintNotInitialized,
    #[msg("A mint freeze authority is outside the V0 token profile")]
    MintFreezeAuthorityUnsupported,
    #[msg("The token account is not initialized")]
    TokenAccountNotInitialized,
    #[msg("The token account is frozen")]
    FrozenTokenAccount,
    #[msg("A native token account is outside the V0 token profile")]
    NativeTokenAccountUnsupported,
    #[msg("The token account mint does not match the authenticated market")]
    InvalidTokenMint,
    #[msg("The token account authority does not match the authenticated role")]
    InvalidTokenAuthority,
    #[msg("A token delegate is outside the protected V0 account profile")]
    TokenDelegateUnsupported,
    #[msg("A token close authority is outside the protected V0 account profile")]
    TokenCloseAuthorityUnsupported,
    #[msg("The delegated amount must be zero when no delegate exists")]
    InvalidDelegatedAmount,
    #[msg("The market PDA or stored market bump is invalid")]
    InvalidMarket,
    #[msg("The domain is not immutably admitted to this market")]
    InvalidDomainAdmission,
    #[msg("The domain PDA or stored domain bump is invalid")]
    InvalidDomain,
    #[msg("The domain vault is not the canonical vault for this asset")]
    InvalidDomainVault,
    #[msg("The fee ledger is not the canonical ledger for this market")]
    InvalidFeeLedger,
    #[msg("The fee vault is not the canonical vault for this ledger")]
    InvalidFeeVault,
    #[msg("The market is bound to a different engine program")]
    InvalidEngineProgram,
    #[msg("The market is bound to a different engine state")]
    InvalidEngineState,
    #[msg("The domain is bound to a different engine revision")]
    InvalidEngineRevision,
    #[msg("The market fee policy does not match the immutable V0 policy")]
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
    #[msg("The raw vault balance is below the Core-accounted balance")]
    VaultBelowAccountedBalance,
    #[msg("The requested output exceeds Core-accounted liquidity")]
    InsufficientAccountedLiquidity,
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
    #[msg("The requested output is below the user's minimum")]
    OutputBelowUserMinimum,
    #[msg("The Core-derived protocol fee exceeds the user's ceiling")]
    ProtocolFeeAboveUserMaximum,
    #[msg("The total input debit exceeds the user's ceiling")]
    TotalDebitAboveUserMaximum,
    #[msg("The instruction must be invoked directly by the transaction")]
    DirectInvocationRequired,
    #[msg("The engine did not return a receipt")]
    MissingEngineReceipt,
    #[msg("The engine receipt was set by a different program")]
    InvalidEngineReceiptSetter,
    #[msg("The engine receipt could not be decoded")]
    InvalidEngineReceipt,
    #[msg("The engine receipt authenticates a different execution plan")]
    EngineReceiptPlanMismatch,
}
