use anchor_lang::prelude::*;

#[event]
pub struct MarketDomainInitializedV0 {
    pub market: Pubkey,
    pub domain: Pubkey,
    pub engine_program: Pubkey,
    pub engine_state: Pubkey,
    pub engine_revision: u64,
    pub mint_a: Pubkey,
    pub mint_b: Pubkey,
    pub fee_ledger: Pubkey,
    pub fee_bps: u16,
}

#[event]
pub struct LiquidityDepositedV0 {
    pub market: Pubkey,
    pub domain: Pubkey,
    pub provider: Pubkey,
    pub mint: Pubkey,
    pub asset_index: u8,
    pub amount: u64,
    pub post_accounted_balance: u64,
}

#[event]
pub struct EngineGeneratedProbeExecutedV0 {
    pub market: Pubkey,
    pub domain: Pubkey,
    pub engine_program: Pubkey,
    pub engine_state: Pubkey,
    pub user: Pubkey,
    pub mint_a: Pubkey,
    pub mint_b: Pubkey,
    pub amount_in: u64,
    pub amount_out: u64,
    pub protocol_fee: u64,
    pub request_hash: [u8; 32],
    pub capability_hash: [u8; 32],
    pub payload_hash: [u8; 32],
    pub settlement_hash: [u8; 32],
    pub engine_sequence: u64,
    pub opaque_account_count: u8,
    pub post_accounted_a: u64,
    pub post_accounted_b: u64,
    pub post_accounted_fee_a: u64,
}
