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
pub struct SpendAuthorizedV0 {
    pub user_authority: Pubkey,
    pub user_input: Pubkey,
    pub mint_in: Pubkey,
    pub spend_authority: Pubkey,
    pub intent_digest: [u8; 32],
    pub timing_mode: u8,
    pub authorization_nonce: [u8; 32],
    pub exact_total_debit: u64,
    pub expires_at_slot: u64,
}

#[event]
pub struct CallbackAuthenticatedProbeExecutedV0 {
    pub market: Pubkey,
    pub domain: Pubkey,
    pub engine_program: Pubkey,
    pub engine_state: Pubkey,
    pub user_authority: Pubkey,
    pub user_input: Pubkey,
    pub user_output: Pubkey,
    pub spend_authority: Pubkey,
    pub primary_callback: Pubkey,
    pub commit_callback: Pubkey,
    pub mint_a: Pubkey,
    pub mint_b: Pubkey,
    pub timing_mode: u8,
    pub primary_phase: u8,
    pub authorization_nonce: [u8; 32],
    pub amount_in: u64,
    pub amount_out: u64,
    pub protocol_fee: u64,
    pub intent_digest: [u8; 32],
    pub authorized_capability_hash: [u8; 32],
    pub primary_phase_capability_hash: [u8; 32],
    pub commit_phase_capability_hash: [u8; 32],
    pub payload_hash: [u8; 32],
    pub primary_execution_digest: [u8; 32],
    pub primary_receipt_digest: [u8; 32],
    pub settlement_digest: [u8; 32],
    pub commit_execution_digest: [u8; 32],
    pub commit_receipt_digest: [u8; 32],
    pub expected_engine_sequence: u64,
    pub primary_engine_sequence: u64,
    pub commit_engine_sequence: u64,
    pub opaque_account_count: u8,
    pub post_accounted_a: u64,
    pub post_accounted_b: u64,
    pub post_accounted_fee_a: u64,
}
