use anchor_lang::prelude::*;

#[constant]
pub const EXPERIMENT_VERSION_V0: u8 = 0;

#[constant]
pub const MARKET_SEED_V0: &[u8] = b"routed-market-v0";

#[constant]
pub const DOMAIN_SEED_V0: &[u8] = b"routed-domain-v0";

#[constant]
pub const VAULT_SEED_V0: &[u8] = b"routed-vault-v0";

#[constant]
pub const FEE_LEDGER_SEED_V0: &[u8] = b"routed-fee-ledger-v0";

#[constant]
pub const FEE_VAULT_SEED_V0: &[u8] = b"routed-fee-vault-v0";

/// Intent-scoped user debit authority. It may persist until expiry, explicit
/// SPL revocation, or successful execution, and signs only the two exact
/// classic SPL Token debits selected by the authenticated intent.
#[constant]
pub const SPEND_AUTHORITY_SEED_V0: &[u8] = b"spend:v0";

/// Phase-scoped engine callback authority. It is never accepted as a token or
/// Core-state authority.
#[constant]
pub const CALLBACK_AUTHORITY_SEED_V0: &[u8] = b"engine-callback:v0";

#[constant]
pub const ASSET_A_INDEX_V0: u8 = 0;

#[constant]
pub const ASSET_B_INDEX_V0: u8 = 1;

pub const ASSET_A_SEED_V0: &[u8] = &[ASSET_A_INDEX_V0];
pub const ASSET_B_SEED_V0: &[u8] = &[ASSET_B_INDEX_V0];

/// Fixed experiment policy, not accepted product economics.
#[constant]
pub const PROTOCOL_FEE_BPS_V0: u16 = 30;

#[constant]
pub const FEE_POLICY_REVISION_V0: u64 = 0;

#[constant]
pub const BASIS_POINTS_DENOMINATOR: u128 = 10_000;

pub const NO_COMMIT_DIGEST_V0: [u8; 32] = [0; 32];
