//! Disposable constants for the private generic-effect experiment.
//!
//! None of these values are a compatibility promise.

pub const EXPERIMENTAL_MAJOR: u32 = 0;
pub const WIRE_VERSION_V0: u8 = 0;
pub const TRANSITION_PHASE_V0: u8 = 0;
pub const ABSENT_INDEX: u8 = u8::MAX;

pub const FIXED_ACCOUNT_COUNT: usize = 6;
pub const CONFIG_ACCOUNT_INDEX: usize = 0;
pub const MARKET_ACCOUNT_INDEX: usize = 1;
pub const FEE_POLICY_ACCOUNT_INDEX: usize = 2;
pub const ENGINE_PROGRAM_ACCOUNT_INDEX: usize = 3;
pub const CALLBACK_ACCOUNT_INDEX: usize = 4;
pub const INSTRUCTIONS_SYSVAR_ACCOUNT_INDEX: usize = 5;

pub const MAX_LOADER_POLICY_ACCOUNTS: usize = 1;
pub const MAX_DOMAIN_CONTROL_ACCOUNTS: usize = 12;
pub const MAX_AUTHORIZATION_CONTROL_ACCOUNTS: usize = 20;
pub const MAX_PROTECTED_PROFILE_ACCOUNTS: usize = 9;
pub const MAX_FEE_CONTROL_ACCOUNTS: usize = 8;
pub const MAX_SETTLEMENT_CAPABILITIES: usize = 12;
pub const MAX_OPAQUE_CAPABILITIES: usize = 8;
pub const MAX_ENGINE_MOVES: usize = 12;
pub const MAX_DOMAINS: usize = 4;
pub const MAX_INTENTS: usize = 8;
pub const MAX_ASSETS: usize = 8;
pub const MAX_FEE_SHARDS: usize = 4;
pub const MAX_OPAQUE_PAYLOAD_BYTES: usize = 128;

pub const POLICY_IMMUTABLE_DEPLOYMENT: u8 = 0;
pub const POLICY_PINNED_MUTABLE_DEPLOYMENT: u8 = 1;
pub const POLICY_MUTABLE_CONTROLLER_RISK: u8 = 2;

pub const AUTHORITY_INTENT_FUNDED_DEBIT: u8 = 0;
pub const AUTHORITY_DOMAIN_ACCOUNTED: u8 = 1;
pub const AUTHORITY_EXACT_EXTERNAL_CREDIT: u8 = 2;
pub const AUTHORITY_CORE_RESERVED_FEE_CREDIT: u8 = 3;

pub const RIGHT_DEBIT: u16 = 1 << 0;
pub const RIGHT_CREDIT: u16 = 1 << 1;
pub const RIGHT_DOMAIN_ACCOUNTED: u16 = 1 << 2;
pub const RIGHT_EXACT_EXTERNAL_RECIPIENT: u16 = 1 << 3;
pub const RIGHT_CORE_RESERVED_FEE: u16 = 1 << 4;
pub const KNOWN_SETTLEMENT_RIGHTS: u16 = RIGHT_DEBIT
    | RIGHT_CREDIT
    | RIGHT_DOMAIN_ACCOUNTED
    | RIGHT_EXACT_EXTERNAL_RECIPIENT
    | RIGHT_CORE_RESERVED_FEE;
pub const KNOWN_ENGINE_CONTEXT_RIGHTS: u16 =
    RIGHT_DEBIT | RIGHT_CREDIT | RIGHT_DOMAIN_ACCOUNTED | RIGHT_EXACT_EXTERNAL_RECIPIENT;

pub const WITNESS_DIRECT_ACTOR: u8 = 0;
pub const WITNESS_EXACT_ONE_SHOT_DELEGATE: u8 = 1;
pub const WITNESS_STORED_AUTHORIZATION: u8 = 2;

pub const ROUND_FLOOR: u8 = 0;
pub const ROUND_CEILING: u8 = 1;
pub const FEE_CLASS_NONE: u8 = 0;
pub const FEE_CLASS_GROSS_DEBIT_RATE: u8 = 1;
pub const FEE_CLASS_FIXED_ENVELOPE_DISABLED: u8 = 2;

pub const EVIDENCE_CORE_VERIFIED: u8 = 0;
pub const EVIDENCE_ENGINE_ATTESTED: u8 = 1;

pub const UPGRADEABLE_LOADER_UNINITIALIZED_TAG: u32 = 0;
pub const UPGRADEABLE_LOADER_BUFFER_TAG: u32 = 1;
pub const UPGRADEABLE_LOADER_PROGRAM_TAG: u32 = 2;
pub const UPGRADEABLE_LOADER_PROGRAM_DATA_TAG: u32 = 3;
pub const UPGRADEABLE_LOADER_PROGRAM_BYTES: usize = 36;
pub const UPGRADEABLE_LOADER_PROGRAM_DATA_METADATA_BYTES: usize = 45;

pub const IMMUTABLE_RELEASE_SEED: &[u8] = b"immutable-release-v0";
pub const DOMAIN_ACCOUNTING_SEED: &[u8] = b"domain-accounting-v0";
pub const DOMAIN_ADMISSION_SEED: &[u8] = b"domain-admission-v0";
pub const STORED_AUTHORIZATION_SEED: &[u8] = b"stored-authorization-v0";
pub const FEE_SHARD_DESCRIPTOR_SEED: &[u8] = b"fee-shard-v0";
pub const FEE_LIABILITY_SEED: &[u8] = b"fee-liability-v0";
