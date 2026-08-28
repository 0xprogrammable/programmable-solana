use anchor_lang::prelude::*;

/// This version marks the entire public surface as a disposable experiment.
#[constant]
pub const EXPERIMENT_VERSION_V0: u8 = 0;

#[constant]
pub const MARKET_SEED_V0: &[u8] = b"market-v0";

#[constant]
pub const DOMAIN_SEED_V0: &[u8] = b"domain-v0";

#[constant]
pub const VAULT_SEED_V0: &[u8] = b"vault-v0";

#[constant]
pub const FEE_LEDGER_SEED_V0: &[u8] = b"fee-ledger-v0";

#[constant]
pub const FEE_VAULT_SEED_V0: &[u8] = b"fee-vault-v0";

#[constant]
pub const ASSET_A_INDEX_V0: u8 = 0;

#[constant]
pub const ASSET_B_INDEX_V0: u8 = 1;

pub const ASSET_A_SEED_V0: &[u8] = &[ASSET_A_INDEX_V0];
pub const ASSET_B_SEED_V0: &[u8] = &[ASSET_B_INDEX_V0];

/// A hard-coded measurement policy, not accepted production economics.
#[constant]
pub const PROTOCOL_FEE_BPS_V0: u16 = 30;

#[constant]
pub const FEE_POLICY_REVISION_V0: u64 = 0;

#[constant]
pub const BASIS_POINTS_DENOMINATOR: u128 = 10_000;

pub const INSTRUCTIONS_SYSVAR_ID: Pubkey = pubkey!("Sysvar1nstructions1111111111111111111111111");
