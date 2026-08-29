use anchor_lang::prelude::*;

#[error_code]
pub enum CoreError {
    #[msg("The private wire encoding is not canonical")]
    InvalidWireEncoding,
    #[msg("A private experiment count exceeds its ceiling")]
    ExperimentLimitExceeded,
    #[msg("An account segment length overflowed")]
    AccountSegmentOverflow,
    #[msg("The declared account segments do not consume the landed accounts exactly")]
    AccountSegmentLengthMismatch,
    #[msg("An account position belongs to more than one declared control role")]
    OverlappingAccountControlOffset,
    #[msg("A declared control account was not consumed")]
    UnreferencedControlAccount,
    #[msg("Equal public keys disagree about landing-time owner or executable state")]
    DuplicateAccountIdentityDrift,
    #[msg("A fixed or protected account has signer privilege that its role forbids")]
    UnexpectedSignerPrivilege,
    #[msg("A fixed or protected account has writable privilege that its role forbids")]
    UnexpectedWritablePrivilege,
    #[msg("The selected engine program is not executable")]
    EngineProgramNotExecutable,
    #[msg("The selected engine program is owned by an unsupported loader")]
    UnsupportedEngineLoader,
    #[msg("The loader-v3 Program account is malformed")]
    MalformedLoaderProgramState,
    #[msg("The loader-v3 ProgramData account is missing or malformed")]
    MalformedLoaderProgramDataState,
    #[msg("The loader-v3 Program and ProgramData relation is not exact")]
    LoaderProgramDataRelationMismatch,
    #[msg("The loader-v3 Program or ProgramData account is effectively writable")]
    WritableLoaderIdentityAccount,
    #[msg("The loader-v3 Program or ProgramData account is effectively a signer")]
    SignerLoaderIdentityAccount,
    #[msg("The loader-state observation must be from a strictly earlier slot")]
    SameSlotEngineObservation,
    #[msg("The engine admission policy kind is unsupported")]
    UnsupportedEngineAdmissionPolicy,
    #[msg("The engine admission policy has an impossible field combination")]
    InvalidEngineAdmissionPolicy,
    #[msg("The observed engine loader state does not satisfy the admission policy")]
    EngineAdmissionPolicyMismatch,
    #[msg("The expected engine loader-state snapshot does not match")]
    EngineLoaderStateSnapshotMismatch,
    #[msg("An opaque capability must not be an effective signer")]
    OpaqueSignerForbidden,
    #[msg("An opaque capability aliases the protected or authentication plane")]
    OpaqueProtectedAlias,
    #[msg("An opaque capability is owned by the experimental Core")]
    OpaqueCoreOwnedAccount,
    #[msg("An executable opaque capability is effectively writable")]
    OpaqueExecutableWritable,
    #[msg("A writable opaque capability is owned by a protected token program")]
    OpaqueProtectedTokenAccountWritable,
    #[msg("A settlement capability public key is duplicated")]
    DuplicateSettlementCapability,
    #[msg("A settlement capability position is not canonical")]
    NonCanonicalSettlementCapabilityPosition,
    #[msg("A settlement capability declares an unknown authority class")]
    UnknownAuthorityClass,
    #[msg("A settlement capability declares unknown or inconsistent rights")]
    InvalidSettlementRights,
    #[msg("A settlement capability has an invalid domain relation")]
    InvalidSettlementDomain,
    #[msg("A settlement capability has an invalid authorization relation")]
    InvalidSettlementAuthorization,
    #[msg("A settlement capability has an invalid fee-shard relation")]
    InvalidSettlementFeeShard,
    #[msg("Settlement and opaque capabilities are not disjoint")]
    CapabilityPlaneAlias,
    #[msg("A protected Move amount must be nonzero")]
    ZeroMoveAmount,
    #[msg("A protected Move capability index is out of range")]
    MoveCapabilityIndexOutOfRange,
    #[msg("A protected Move must have distinct endpoints")]
    MoveEndpointsIdentical,
    #[msg("A protected Move endpoint lacks the declared right")]
    MoveRightMissing,
    #[msg("A protected Move references a Core-reserved fee destination")]
    EngineReferencedFeeCapability,
    #[msg("A protected Move crosses asset or settlement-profile identities")]
    MoveAssetProfileMismatch,
    #[msg("Protected Move rows are not in strict canonical order")]
    NonCanonicalMoveOrder,
    #[msg("One capability appears as both a source and destination")]
    MoveGraphCycle,
    #[msg("The protected Move graph is not conserved per exact asset profile")]
    AssetConservationMismatch,
    #[msg("A protected debit exceeds its capability maximum")]
    CapabilityMaximumDebitExceeded,
    #[msg("A protected credit is below its required minimum")]
    CapabilityMinimumCreditNotMet,
    #[msg("A domain-local debit exceeds accounted liquidity")]
    DomainAccountedLiquidityExceeded,
    #[msg("A raw protected balance is below its accounted balance")]
    RawBalanceBelowAccounted,
    #[msg("An observed protected-account delta does not match the canonical plan")]
    ObservedProtectedDeltaMismatch,
    #[msg("Arithmetic overflowed or underflowed")]
    ArithmeticOverflow,
    #[msg("A wide value does not fit the protected amount type")]
    AmountConversionFailed,
    #[msg("The fee rounding mode is unsupported")]
    UnsupportedFeeRounding,
    #[msg("The fee denominator must be nonzero")]
    ZeroFeeDenominator,
    #[msg("The cumulative fee basis decreased")]
    CumulativeFeeBasisDecreased,
    #[msg("A Core-derived fee exceeds the user's ceiling")]
    FeeCeilingExceeded,
    #[msg("A fee assessment identity was already consumed")]
    DuplicateFeeAssessment,
    #[msg("The fee liability update does not match the observed vault credit")]
    FeeLiabilityMismatch,
    #[msg("An authorization witness kind is unsupported")]
    UnsupportedAuthorizationWitness,
    #[msg("A direct authorization is not transaction-root authenticated")]
    DirectAuthorizationNotTransactionRoot,
    #[msg("The authorization identity does not match the immutable intent")]
    AuthorizationIdentityMismatch,
    #[msg("The authorization slot set is incomplete, duplicated, or unordered")]
    InvalidAuthorizationSlots,
    #[msg("The intent set contains a duplicate canonical digest")]
    DuplicateIntentDigest,
    #[msg("One protected funding key appears in multiple authorization slots")]
    CrossAuthorizationFundingAlias,
    #[msg("The authorization has expired")]
    AuthorizationExpired,
    #[msg("The stored authorization is cancelled, terminal, or exhausted")]
    AuthorizationUnavailable,
    #[msg("The stored authorization fill sequence does not match")]
    AuthorizationFillSequenceMismatch,
    #[msg("The fill exceeds a stored authorization bound")]
    AuthorizationBoundExceeded,
    #[msg("An exact one-shot delegation was not consumed exactly")]
    ExactDelegateConsumptionMismatch,
    #[msg("The authorization snapshot does not bind this exact execution")]
    AuthorizationSnapshotMismatch,
    #[msg("The evidence class is unsupported")]
    UnsupportedEvidenceClass,
    #[msg("ExecuteEffect requires the canonical controlled heap frame")]
    ControlledHeapFrameRequired,
}
