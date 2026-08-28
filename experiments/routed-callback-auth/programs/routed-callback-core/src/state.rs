use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct MarketV0 {
    pub version: u8,
    pub bump: u8,
    pub initializer: Pubkey,
    pub market_id: [u8; 32],
    pub engine_program: Pubkey,
    pub engine_state: Pubkey,
    pub engine_revision: u64,
    pub mint_a: Pubkey,
    pub mint_b: Pubkey,
    pub fee_bps: u16,
    pub fee_policy_revision: u64,
}

#[account]
#[derive(InitSpace)]
pub struct DomainV0 {
    pub version: u8,
    pub bump: u8,
    pub vault_a_bump: u8,
    pub vault_b_bump: u8,
    pub market: Pubkey,
    pub engine_program: Pubkey,
    pub engine_state: Pubkey,
    pub engine_revision: u64,
    pub accounted_a: u64,
    pub accounted_b: u64,
}

#[account]
#[derive(InitSpace)]
pub struct FeeLedgerV0 {
    pub version: u8,
    pub bump: u8,
    pub fee_vault_bump: u8,
    pub market: Pubkey,
    pub mint_a: Pubkey,
    pub accounted_fee_a: u64,
}
