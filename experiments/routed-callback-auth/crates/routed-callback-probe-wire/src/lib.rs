#![no_std]

//! Canonical, experiment-local wire types for the disposable routed callback
//! authentication spike.
//!
//! The stable intent digest never contains either authority PDA. The spend PDA
//! and each phase-specific callback PDA are derived from that digest, so there
//! is no hash/address cycle. Execution digests separately bind dynamic state.
//! This crate is unpublished experiment machinery, not a promised protocol ABI.

extern crate alloc;

use alloc::vec::Vec;
use solana_pubkey::Pubkey;

pub const PROBE_WIRE_VERSION: u8 = 0;

pub const TIMING_SINGLE: u8 = 0;
pub const TIMING_PREPARE_COMMIT: u8 = 1;

pub const PHASE_TRANSITION: u8 = 0;
pub const PHASE_PREPARE: u8 = 1;
pub const PHASE_COMMIT: u8 = 2;

pub const MAX_OPAQUE_ACCOUNTS: usize = 8;
/// The engine state is committed before the ordered opaque closure. The
/// callback authority is an authentication-plane account and is never hashed
/// as a capability descriptor.
pub const CAPABILITY_PREFIX_ACCOUNTS: usize = 1;
pub const MAX_CAPABILITY_DESCRIPTORS: usize = CAPABILITY_PREFIX_ACCOUNTS + MAX_OPAQUE_ACCOUNTS;
pub const MAX_OPAQUE_PAYLOAD_LEN: usize = 128;

pub const DISPOSABLE_CORE_PROGRAM_ID: Pubkey =
    solana_pubkey::pubkey!("Bwhiw9S9ZdHkEhFF2Ps89HMxa5dHX1xSbdsGZ8W3qR2b");
pub const DISPOSABLE_ENGINE_PROGRAM_ID: Pubkey =
    solana_pubkey::pubkey!("5UNyG5GQpPwyoDgsvt4JzdqJxJzPh52pVbUDjEa5Gikh");
pub const DISPOSABLE_ROUTER_PROGRAM_ID: Pubkey =
    solana_pubkey::pubkey!("F62maceZqpLAayyBLsXNGdrmKg9cZWdpSDbzoHuNgk6Q");
pub const DISPOSABLE_HELPER_PROGRAM_ID: Pubkey =
    solana_pubkey::pubkey!("6QXXm7aqjRxQGJ6V3nvtS5taHuojM9SisVrHg3Xrj1Vj");

/// Core PDA seeds: `[SPEND_AUTHORITY_SEED, user_input, intent_digest]`.
pub const SPEND_AUTHORITY_SEED: &[u8] = b"spend:v0";
/// Core PDA seeds: `[CALLBACK_AUTHORITY_SEED, engine_program, engine_state,
/// market, domain, intent_digest, phase_byte]`.
pub const CALLBACK_AUTHORITY_SEED: &[u8] = b"engine-callback:v0";

pub const ENGINE_TRANSITION_DISCRIMINATOR: [u8; 8] =
    [0xfa, 0x8d, 0x5a, 0xf4, 0x72, 0x49, 0x21, 0x6a];
pub const ENGINE_PREPARE_DISCRIMINATOR: [u8; 8] = [0x79, 0x9b, 0x9c, 0x5a, 0xa4, 0xfc, 0xdc, 0x6d];
pub const ENGINE_COMMIT_DISCRIMINATOR: [u8; 8] = [0xdf, 0x8c, 0x8e, 0xa5, 0xe5, 0xd0, 0x9c, 0x4a];

pub const RECEIPT_MAGIC: [u8; 8] = *b"PMBRCB00";

pub const CAPABILITY_HASH_DOMAIN: &[u8] = b"programmable:routed-callback-auth:capability:v0";
pub const PAYLOAD_HASH_DOMAIN: &[u8] = b"programmable:routed-callback-auth:payload:v0";
pub const INTENT_HASH_DOMAIN: &[u8] = b"programmable:routed-callback-auth:intent:v0";
pub const EXECUTION_HASH_DOMAIN: &[u8] = b"programmable:routed-callback-auth:execution:v0";
pub const RECEIPT_HASH_DOMAIN: &[u8] = b"programmable:routed-callback-auth:receipt:v0";
pub const SETTLEMENT_HASH_DOMAIN: &[u8] = b"programmable:routed-callback-auth:settlement:v0";

const PUBKEY_LEN: usize = 32;
const HASH_LEN: usize = 32;
const U64_LEN: usize = 8;
const U16_LEN: usize = 2;
const INTENT_BINDING_PUBKEYS: usize = 15;
const INTENT_BINDING_U64S: usize = 8;
const INTENT_BINDING_HASHES: usize = 3;
const EXECUTION_BINDING_PUBKEYS: usize = 2;
const EXECUTION_BINDING_U64S: usize = 7;
const EXECUTION_BINDING_HASHES: usize = 4;
const SETTLEMENT_BINDING_HASHES: usize = 3;
const SETTLEMENT_BINDING_U64S: usize = 15;

pub const INTENT_BINDING_LEN: usize = 2
    + (INTENT_BINDING_PUBKEYS * PUBKEY_LEN)
    + (INTENT_BINDING_U64S * U64_LEN)
    + (INTENT_BINDING_HASHES * HASH_LEN);
pub const EXECUTION_BINDING_LEN: usize = 2
    + (EXECUTION_BINDING_PUBKEYS * PUBKEY_LEN)
    + (EXECUTION_BINDING_U64S * U64_LEN)
    + (EXECUTION_BINDING_HASHES * HASH_LEN)
    + (2 * U16_LEN)
    + MAX_OPAQUE_PAYLOAD_LEN;
pub const ENGINE_REQUEST_LEN: usize = EXECUTION_BINDING_LEN + HASH_LEN;
pub const ENGINE_INSTRUCTION_LEN: usize = 8 + ENGINE_REQUEST_LEN;
pub const ENGINE_RECEIPT_LEN: usize = RECEIPT_MAGIC.len() + 2 + (2 * HASH_LEN) + (2 * U64_LEN);
pub const SETTLEMENT_BINDING_LEN: usize =
    1 + (SETTLEMENT_BINDING_HASHES * HASH_LEN) + (SETTLEMENT_BINDING_U64S * U64_LEN);

const CAPABILITY_FLAGS_WRITABLE: u8 = 1 << 0;
const CAPABILITY_FLAGS_SIGNER: u8 = 1 << 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityDescriptor {
    pub key: Pubkey,
    pub owner: Pubkey,
    pub is_writable: bool,
    pub is_signer: bool,
    pub is_executable: bool,
}

impl CapabilityDescriptor {
    pub const fn privilege_flags(self) -> u8 {
        (if self.is_writable {
            CAPABILITY_FLAGS_WRITABLE
        } else {
            0
        }) | (if self.is_signer {
            CAPABILITY_FLAGS_SIGNER
        } else {
            0
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntentBinding {
    pub timing_mode: u8,
    pub core_program: Pubkey,
    pub market: Pubkey,
    pub domain: Pubkey,
    pub engine_program: Pubkey,
    pub engine_state: Pubkey,
    pub user_authority: Pubkey,
    pub user_input: Pubkey,
    pub user_output: Pubkey,
    pub mint_in: Pubkey,
    pub mint_out: Pubkey,
    pub domain_input_vault: Pubkey,
    pub domain_output_vault: Pubkey,
    pub protocol_fee_vault: Pubkey,
    pub fee_ledger: Pubkey,
    pub token_program: Pubkey,
    pub engine_revision: u64,
    pub fee_policy_revision: u64,
    pub amount_in: u64,
    pub protocol_fee: u64,
    pub max_total_input_debit: u64,
    pub min_output_credit: u64,
    pub max_protocol_fee: u64,
    pub expires_at_slot: u64,
    pub authorization_nonce: [u8; 32],
    pub authorized_capability_hash: [u8; 32],
    pub payload_hash: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionBinding {
    pub phase: u8,
    pub intent_digest: [u8; 32],
    /// Zero for TRANSITION/PREPARE. COMMIT binds the nonzero post-settlement
    /// digest produced by Core after the primary phase and token settlement.
    pub phase_context_digest: [u8; 32],
    pub market: Pubkey,
    pub domain: Pubkey,
    pub engine_revision: u64,
    pub amount_in: u64,
    pub protocol_fee: u64,
    pub accounted_input_before: u64,
    pub accounted_output_before: u64,
    pub accounted_fee_before: u64,
    pub pre_sequence: u64,
    pub authorized_capability_hash: [u8; 32],
    pub phase_capability_hash: [u8; 32],
    pub opaque_account_count: u16,
    pub payload_len: u16,
    pub payload: [u8; MAX_OPAQUE_PAYLOAD_LEN],
}

impl ExecutionBinding {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        phase: u8,
        intent_digest: [u8; 32],
        phase_context_digest: [u8; 32],
        market: Pubkey,
        domain: Pubkey,
        engine_revision: u64,
        amount_in: u64,
        protocol_fee: u64,
        accounted_input_before: u64,
        accounted_output_before: u64,
        accounted_fee_before: u64,
        pre_sequence: u64,
        authorized_capability_hash: [u8; 32],
        phase_capability_hash: [u8; 32],
        opaque_account_count: u16,
        payload: &[u8],
    ) -> Result<Self, CodecError> {
        require_phase(phase)?;
        validate_opaque_account_count(opaque_account_count)?;
        let (payload_len, payload) = canonical_payload(payload)?;
        let binding = Self {
            phase,
            intent_digest,
            phase_context_digest,
            market,
            domain,
            engine_revision,
            amount_in,
            protocol_fee,
            accounted_input_before,
            accounted_output_before,
            accounted_fee_before,
            pre_sequence,
            authorized_capability_hash,
            phase_capability_hash,
            opaque_account_count,
            payload_len,
            payload,
        };
        validate_execution_binding(&binding)?;
        Ok(binding)
    }

    pub fn payload_bytes(&self) -> Result<&[u8], CodecError> {
        validate_execution_binding(self)?;
        Ok(&self.payload[..usize::from(self.payload_len)])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineRequest {
    pub binding: ExecutionBinding,
    pub execution_digest: [u8; 32],
}

impl EngineRequest {
    pub fn new(binding: ExecutionBinding) -> Result<Self, CodecError> {
        let execution_digest = compute_execution_digest(&binding)?;
        Ok(Self {
            binding,
            execution_digest,
        })
    }

    pub fn payload_bytes(&self) -> Result<&[u8], CodecError> {
        self.binding.payload_bytes()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineReceipt {
    pub phase: u8,
    pub intent_digest: [u8; 32],
    pub execution_digest: [u8; 32],
    pub amount_out: u64,
    pub state_sequence: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettlementBinding {
    pub intent_digest: [u8; 32],
    pub primary_execution_digest: [u8; 32],
    pub primary_receipt_digest: [u8; 32],
    pub amount_in: u64,
    pub amount_out: u64,
    pub protocol_fee: u64,
    pub total_input_debit: u64,
    pub accounted_input_before: u64,
    pub accounted_output_before: u64,
    pub accounted_fee_before: u64,
    pub accounted_input_after: u64,
    pub accounted_output_after: u64,
    pub accounted_fee_after: u64,
    pub observed_source_after: u64,
    pub observed_destination_after: u64,
    pub observed_input_vault_after: u64,
    pub observed_output_vault_after: u64,
    pub observed_fee_vault_after: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodecError {
    InvalidLength { expected: usize, actual: usize },
    InvalidDiscriminator,
    InvalidMagic,
    UnsupportedVersion { expected: u8, actual: u8 },
    UnsupportedTimingMode { actual: u8 },
    UnsupportedPhase { actual: u8 },
    InvalidPhaseForTiming { timing_mode: u8, phase: u8 },
    TooManyCapabilityDescriptors { maximum: usize, actual: usize },
    InvalidOpaqueAccountCount { maximum: u16, actual: u16 },
    PayloadTooLong { maximum: usize, actual: usize },
    InvalidPayloadLength { maximum: u16, actual: u16 },
    NonCanonicalPayloadPadding,
    InvalidPhaseContext { phase: u8, expected_nonzero: bool },
    ExecutionDigestMismatch,
}

pub fn validate_phase_for_timing(timing_mode: u8, phase: u8) -> Result<(), CodecError> {
    require_timing_mode(timing_mode)?;
    require_phase(phase)?;
    let valid = matches!(
        (timing_mode, phase),
        (TIMING_SINGLE, PHASE_TRANSITION)
            | (TIMING_PREPARE_COMMIT, PHASE_PREPARE)
            | (TIMING_PREPARE_COMMIT, PHASE_COMMIT)
    );
    if !valid {
        return Err(CodecError::InvalidPhaseForTiming { timing_mode, phase });
    }
    Ok(())
}

pub fn engine_phase_discriminator(phase: u8) -> Result<[u8; 8], CodecError> {
    match phase {
        PHASE_TRANSITION => Ok(ENGINE_TRANSITION_DISCRIMINATOR),
        PHASE_PREPARE => Ok(ENGINE_PREPARE_DISCRIMINATOR),
        PHASE_COMMIT => Ok(ENGINE_COMMIT_DISCRIMINATOR),
        actual => Err(CodecError::UnsupportedPhase { actual }),
    }
}

/// Hashes engine state followed by the exact ordered opaque descriptors.
/// Callers must never place callback authority in this list. Duplicate keys
/// remain distinct positions and are not deduplicated.
pub fn compute_capability_hash(
    engine_program: &Pubkey,
    descriptors: &[CapabilityDescriptor],
) -> Result<[u8; 32], CodecError> {
    if descriptors.len() > MAX_CAPABILITY_DESCRIPTORS {
        return Err(CodecError::TooManyCapabilityDescriptors {
            maximum: MAX_CAPABILITY_DESCRIPTORS,
            actual: descriptors.len(),
        });
    }
    let mut encoded = Vec::with_capacity(1 + PUBKEY_LEN + U16_LEN + (descriptors.len() * 68));
    encoded.push(PROBE_WIRE_VERSION);
    encoded.extend_from_slice(engine_program.as_ref());
    encoded.extend_from_slice(&(descriptors.len() as u16).to_le_bytes());
    for (index, descriptor) in descriptors.iter().enumerate() {
        encoded.extend_from_slice(&(index as u16).to_le_bytes());
        encoded.extend_from_slice(descriptor.key.as_ref());
        encoded.extend_from_slice(descriptor.owner.as_ref());
        encoded.push(u8::from(descriptor.is_executable));
        encoded.push(descriptor.privilege_flags());
    }
    Ok(solana_sha256_hasher::hashv(&[CAPABILITY_HASH_DOMAIN, &encoded]).to_bytes())
}

pub fn compute_payload_hash(payload: &[u8]) -> Result<[u8; 32], CodecError> {
    if payload.len() > MAX_OPAQUE_PAYLOAD_LEN {
        return Err(CodecError::PayloadTooLong {
            maximum: MAX_OPAQUE_PAYLOAD_LEN,
            actual: payload.len(),
        });
    }
    let mut encoded = Vec::with_capacity(1 + U16_LEN + payload.len());
    encoded.push(PROBE_WIRE_VERSION);
    encoded.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    encoded.extend_from_slice(payload);
    Ok(solana_sha256_hasher::hashv(&[PAYLOAD_HASH_DOMAIN, &encoded]).to_bytes())
}

pub fn encode_intent_binding(
    binding: &IntentBinding,
) -> Result<[u8; INTENT_BINDING_LEN], CodecError> {
    require_timing_mode(binding.timing_mode)?;
    let mut output = [0_u8; INTENT_BINDING_LEN];
    let mut cursor = 0;
    put_u8(&mut output, &mut cursor, PROBE_WIRE_VERSION);
    put_u8(&mut output, &mut cursor, binding.timing_mode);
    put_pubkey(&mut output, &mut cursor, &binding.core_program);
    put_pubkey(&mut output, &mut cursor, &binding.market);
    put_pubkey(&mut output, &mut cursor, &binding.domain);
    put_pubkey(&mut output, &mut cursor, &binding.engine_program);
    put_pubkey(&mut output, &mut cursor, &binding.engine_state);
    put_pubkey(&mut output, &mut cursor, &binding.user_authority);
    put_pubkey(&mut output, &mut cursor, &binding.user_input);
    put_pubkey(&mut output, &mut cursor, &binding.user_output);
    put_pubkey(&mut output, &mut cursor, &binding.mint_in);
    put_pubkey(&mut output, &mut cursor, &binding.mint_out);
    put_pubkey(&mut output, &mut cursor, &binding.domain_input_vault);
    put_pubkey(&mut output, &mut cursor, &binding.domain_output_vault);
    put_pubkey(&mut output, &mut cursor, &binding.protocol_fee_vault);
    put_pubkey(&mut output, &mut cursor, &binding.fee_ledger);
    put_pubkey(&mut output, &mut cursor, &binding.token_program);
    put_u64(&mut output, &mut cursor, binding.engine_revision);
    put_u64(&mut output, &mut cursor, binding.fee_policy_revision);
    put_u64(&mut output, &mut cursor, binding.amount_in);
    put_u64(&mut output, &mut cursor, binding.protocol_fee);
    put_u64(&mut output, &mut cursor, binding.max_total_input_debit);
    put_u64(&mut output, &mut cursor, binding.min_output_credit);
    put_u64(&mut output, &mut cursor, binding.max_protocol_fee);
    put_u64(&mut output, &mut cursor, binding.expires_at_slot);
    put_bytes(&mut output, &mut cursor, &binding.authorization_nonce);
    put_bytes(
        &mut output,
        &mut cursor,
        &binding.authorized_capability_hash,
    );
    put_bytes(&mut output, &mut cursor, &binding.payload_hash);
    debug_assert_eq!(cursor, INTENT_BINDING_LEN);
    Ok(output)
}

pub fn decode_intent_binding(data: &[u8]) -> Result<IntentBinding, CodecError> {
    require_length(data, INTENT_BINDING_LEN)?;
    let mut cursor = 0;
    require_version(read_u8(data, &mut cursor))?;
    let timing_mode = read_u8(data, &mut cursor);
    require_timing_mode(timing_mode)?;
    let binding = IntentBinding {
        timing_mode,
        core_program: read_pubkey(data, &mut cursor),
        market: read_pubkey(data, &mut cursor),
        domain: read_pubkey(data, &mut cursor),
        engine_program: read_pubkey(data, &mut cursor),
        engine_state: read_pubkey(data, &mut cursor),
        user_authority: read_pubkey(data, &mut cursor),
        user_input: read_pubkey(data, &mut cursor),
        user_output: read_pubkey(data, &mut cursor),
        mint_in: read_pubkey(data, &mut cursor),
        mint_out: read_pubkey(data, &mut cursor),
        domain_input_vault: read_pubkey(data, &mut cursor),
        domain_output_vault: read_pubkey(data, &mut cursor),
        protocol_fee_vault: read_pubkey(data, &mut cursor),
        fee_ledger: read_pubkey(data, &mut cursor),
        token_program: read_pubkey(data, &mut cursor),
        engine_revision: read_u64(data, &mut cursor),
        fee_policy_revision: read_u64(data, &mut cursor),
        amount_in: read_u64(data, &mut cursor),
        protocol_fee: read_u64(data, &mut cursor),
        max_total_input_debit: read_u64(data, &mut cursor),
        min_output_credit: read_u64(data, &mut cursor),
        max_protocol_fee: read_u64(data, &mut cursor),
        expires_at_slot: read_u64(data, &mut cursor),
        authorization_nonce: read_array_32(data, &mut cursor),
        authorized_capability_hash: read_array_32(data, &mut cursor),
        payload_hash: read_array_32(data, &mut cursor),
    };
    debug_assert_eq!(cursor, INTENT_BINDING_LEN);
    Ok(binding)
}

pub fn compute_intent_digest(binding: &IntentBinding) -> Result<[u8; 32], CodecError> {
    let encoded = encode_intent_binding(binding)?;
    Ok(solana_sha256_hasher::hashv(&[INTENT_HASH_DOMAIN, &encoded]).to_bytes())
}

pub fn encode_execution_binding(
    binding: &ExecutionBinding,
) -> Result<[u8; EXECUTION_BINDING_LEN], CodecError> {
    validate_execution_binding(binding)?;
    let mut output = [0_u8; EXECUTION_BINDING_LEN];
    let mut cursor = 0;
    put_u8(&mut output, &mut cursor, PROBE_WIRE_VERSION);
    put_u8(&mut output, &mut cursor, binding.phase);
    put_bytes(&mut output, &mut cursor, &binding.intent_digest);
    put_bytes(&mut output, &mut cursor, &binding.phase_context_digest);
    put_pubkey(&mut output, &mut cursor, &binding.market);
    put_pubkey(&mut output, &mut cursor, &binding.domain);
    put_u64(&mut output, &mut cursor, binding.engine_revision);
    put_u64(&mut output, &mut cursor, binding.amount_in);
    put_u64(&mut output, &mut cursor, binding.protocol_fee);
    put_u64(&mut output, &mut cursor, binding.accounted_input_before);
    put_u64(&mut output, &mut cursor, binding.accounted_output_before);
    put_u64(&mut output, &mut cursor, binding.accounted_fee_before);
    put_u64(&mut output, &mut cursor, binding.pre_sequence);
    put_bytes(
        &mut output,
        &mut cursor,
        &binding.authorized_capability_hash,
    );
    put_bytes(&mut output, &mut cursor, &binding.phase_capability_hash);
    put_u16(&mut output, &mut cursor, binding.opaque_account_count);
    put_u16(&mut output, &mut cursor, binding.payload_len);
    put_bytes(&mut output, &mut cursor, &binding.payload);
    debug_assert_eq!(cursor, EXECUTION_BINDING_LEN);
    Ok(output)
}

pub fn decode_execution_binding(data: &[u8]) -> Result<ExecutionBinding, CodecError> {
    require_length(data, EXECUTION_BINDING_LEN)?;
    let mut cursor = 0;
    require_version(read_u8(data, &mut cursor))?;
    let binding = ExecutionBinding {
        phase: read_u8(data, &mut cursor),
        intent_digest: read_array_32(data, &mut cursor),
        phase_context_digest: read_array_32(data, &mut cursor),
        market: read_pubkey(data, &mut cursor),
        domain: read_pubkey(data, &mut cursor),
        engine_revision: read_u64(data, &mut cursor),
        amount_in: read_u64(data, &mut cursor),
        protocol_fee: read_u64(data, &mut cursor),
        accounted_input_before: read_u64(data, &mut cursor),
        accounted_output_before: read_u64(data, &mut cursor),
        accounted_fee_before: read_u64(data, &mut cursor),
        pre_sequence: read_u64(data, &mut cursor),
        authorized_capability_hash: read_array_32(data, &mut cursor),
        phase_capability_hash: read_array_32(data, &mut cursor),
        opaque_account_count: read_u16(data, &mut cursor),
        payload_len: read_u16(data, &mut cursor),
        payload: read_array_128(data, &mut cursor),
    };
    debug_assert_eq!(cursor, EXECUTION_BINDING_LEN);
    validate_execution_binding(&binding)?;
    Ok(binding)
}

pub fn compute_execution_digest(binding: &ExecutionBinding) -> Result<[u8; 32], CodecError> {
    let encoded = encode_execution_binding(binding)?;
    Ok(solana_sha256_hasher::hashv(&[EXECUTION_HASH_DOMAIN, &encoded]).to_bytes())
}

pub fn encode_engine_request(
    request: &EngineRequest,
) -> Result<[u8; ENGINE_REQUEST_LEN], CodecError> {
    validate_engine_request(request)?;
    let encoded_binding = encode_execution_binding(&request.binding)?;
    let mut output = [0_u8; ENGINE_REQUEST_LEN];
    output[..EXECUTION_BINDING_LEN].copy_from_slice(&encoded_binding);
    output[EXECUTION_BINDING_LEN..].copy_from_slice(&request.execution_digest);
    Ok(output)
}

pub fn decode_engine_request(data: &[u8]) -> Result<EngineRequest, CodecError> {
    require_length(data, ENGINE_REQUEST_LEN)?;
    let binding = decode_execution_binding(&data[..EXECUTION_BINDING_LEN])?;
    let mut execution_digest = [0_u8; HASH_LEN];
    execution_digest.copy_from_slice(&data[EXECUTION_BINDING_LEN..]);
    let request = EngineRequest {
        binding,
        execution_digest,
    };
    validate_engine_request(&request)?;
    Ok(request)
}

pub fn encode_engine_instruction(
    request: &EngineRequest,
) -> Result<[u8; ENGINE_INSTRUCTION_LEN], CodecError> {
    let encoded_request = encode_engine_request(request)?;
    let discriminator = engine_phase_discriminator(request.binding.phase)?;
    let mut output = [0_u8; ENGINE_INSTRUCTION_LEN];
    output[..8].copy_from_slice(&discriminator);
    output[8..].copy_from_slice(&encoded_request);
    Ok(output)
}

pub fn decode_engine_instruction(data: &[u8]) -> Result<EngineRequest, CodecError> {
    require_length(data, ENGINE_INSTRUCTION_LEN)?;
    let request = decode_engine_request(&data[8..])?;
    let expected = engine_phase_discriminator(request.binding.phase)?;
    if data[..8] != expected {
        return Err(CodecError::InvalidDiscriminator);
    }
    Ok(request)
}

pub fn encode_receipt(receipt: &EngineReceipt) -> Result<[u8; ENGINE_RECEIPT_LEN], CodecError> {
    require_phase(receipt.phase)?;
    let mut output = [0_u8; ENGINE_RECEIPT_LEN];
    let mut cursor = 0;
    put_bytes(&mut output, &mut cursor, &RECEIPT_MAGIC);
    put_u8(&mut output, &mut cursor, PROBE_WIRE_VERSION);
    put_u8(&mut output, &mut cursor, receipt.phase);
    put_bytes(&mut output, &mut cursor, &receipt.intent_digest);
    put_bytes(&mut output, &mut cursor, &receipt.execution_digest);
    put_u64(&mut output, &mut cursor, receipt.amount_out);
    put_u64(&mut output, &mut cursor, receipt.state_sequence);
    debug_assert_eq!(cursor, ENGINE_RECEIPT_LEN);
    Ok(output)
}

pub fn decode_receipt(data: &[u8]) -> Result<EngineReceipt, CodecError> {
    require_length(data, ENGINE_RECEIPT_LEN)?;
    let mut cursor = 0;
    if read_array_8(data, &mut cursor) != RECEIPT_MAGIC {
        return Err(CodecError::InvalidMagic);
    }
    require_version(read_u8(data, &mut cursor))?;
    let phase = read_u8(data, &mut cursor);
    require_phase(phase)?;
    let receipt = EngineReceipt {
        phase,
        intent_digest: read_array_32(data, &mut cursor),
        execution_digest: read_array_32(data, &mut cursor),
        amount_out: read_u64(data, &mut cursor),
        state_sequence: read_u64(data, &mut cursor),
    };
    debug_assert_eq!(cursor, ENGINE_RECEIPT_LEN);
    Ok(receipt)
}

pub fn compute_receipt_digest(receipt: &EngineReceipt) -> Result<[u8; 32], CodecError> {
    let encoded = encode_receipt(receipt)?;
    Ok(solana_sha256_hasher::hashv(&[RECEIPT_HASH_DOMAIN, &encoded]).to_bytes())
}

pub fn encode_settlement_binding(binding: &SettlementBinding) -> [u8; SETTLEMENT_BINDING_LEN] {
    let mut output = [0_u8; SETTLEMENT_BINDING_LEN];
    let mut cursor = 0;
    put_u8(&mut output, &mut cursor, PROBE_WIRE_VERSION);
    put_bytes(&mut output, &mut cursor, &binding.intent_digest);
    put_bytes(&mut output, &mut cursor, &binding.primary_execution_digest);
    put_bytes(&mut output, &mut cursor, &binding.primary_receipt_digest);
    put_u64(&mut output, &mut cursor, binding.amount_in);
    put_u64(&mut output, &mut cursor, binding.amount_out);
    put_u64(&mut output, &mut cursor, binding.protocol_fee);
    put_u64(&mut output, &mut cursor, binding.total_input_debit);
    put_u64(&mut output, &mut cursor, binding.accounted_input_before);
    put_u64(&mut output, &mut cursor, binding.accounted_output_before);
    put_u64(&mut output, &mut cursor, binding.accounted_fee_before);
    put_u64(&mut output, &mut cursor, binding.accounted_input_after);
    put_u64(&mut output, &mut cursor, binding.accounted_output_after);
    put_u64(&mut output, &mut cursor, binding.accounted_fee_after);
    put_u64(&mut output, &mut cursor, binding.observed_source_after);
    put_u64(&mut output, &mut cursor, binding.observed_destination_after);
    put_u64(&mut output, &mut cursor, binding.observed_input_vault_after);
    put_u64(
        &mut output,
        &mut cursor,
        binding.observed_output_vault_after,
    );
    put_u64(&mut output, &mut cursor, binding.observed_fee_vault_after);
    debug_assert_eq!(cursor, SETTLEMENT_BINDING_LEN);
    output
}

pub fn decode_settlement_binding(data: &[u8]) -> Result<SettlementBinding, CodecError> {
    require_length(data, SETTLEMENT_BINDING_LEN)?;
    let mut cursor = 0;
    require_version(read_u8(data, &mut cursor))?;
    let binding = SettlementBinding {
        intent_digest: read_array_32(data, &mut cursor),
        primary_execution_digest: read_array_32(data, &mut cursor),
        primary_receipt_digest: read_array_32(data, &mut cursor),
        amount_in: read_u64(data, &mut cursor),
        amount_out: read_u64(data, &mut cursor),
        protocol_fee: read_u64(data, &mut cursor),
        total_input_debit: read_u64(data, &mut cursor),
        accounted_input_before: read_u64(data, &mut cursor),
        accounted_output_before: read_u64(data, &mut cursor),
        accounted_fee_before: read_u64(data, &mut cursor),
        accounted_input_after: read_u64(data, &mut cursor),
        accounted_output_after: read_u64(data, &mut cursor),
        accounted_fee_after: read_u64(data, &mut cursor),
        observed_source_after: read_u64(data, &mut cursor),
        observed_destination_after: read_u64(data, &mut cursor),
        observed_input_vault_after: read_u64(data, &mut cursor),
        observed_output_vault_after: read_u64(data, &mut cursor),
        observed_fee_vault_after: read_u64(data, &mut cursor),
    };
    debug_assert_eq!(cursor, SETTLEMENT_BINDING_LEN);
    Ok(binding)
}

pub fn compute_settlement_digest(binding: &SettlementBinding) -> [u8; 32] {
    let encoded = encode_settlement_binding(binding);
    solana_sha256_hasher::hashv(&[SETTLEMENT_HASH_DOMAIN, &encoded]).to_bytes()
}

fn canonical_payload(payload: &[u8]) -> Result<(u16, [u8; MAX_OPAQUE_PAYLOAD_LEN]), CodecError> {
    if payload.len() > MAX_OPAQUE_PAYLOAD_LEN {
        return Err(CodecError::PayloadTooLong {
            maximum: MAX_OPAQUE_PAYLOAD_LEN,
            actual: payload.len(),
        });
    }
    let mut padded = [0_u8; MAX_OPAQUE_PAYLOAD_LEN];
    padded[..payload.len()].copy_from_slice(payload);
    Ok((payload.len() as u16, padded))
}

fn validate_execution_binding(binding: &ExecutionBinding) -> Result<(), CodecError> {
    require_phase(binding.phase)?;
    let context_is_nonzero = binding.phase_context_digest != [0_u8; HASH_LEN];
    let expected_nonzero = binding.phase == PHASE_COMMIT;
    if context_is_nonzero != expected_nonzero {
        return Err(CodecError::InvalidPhaseContext {
            phase: binding.phase,
            expected_nonzero,
        });
    }
    validate_opaque_account_count(binding.opaque_account_count)?;
    let payload_len = usize::from(binding.payload_len);
    if payload_len > MAX_OPAQUE_PAYLOAD_LEN {
        return Err(CodecError::InvalidPayloadLength {
            maximum: MAX_OPAQUE_PAYLOAD_LEN as u16,
            actual: binding.payload_len,
        });
    }
    if binding.payload[payload_len..].iter().any(|byte| *byte != 0) {
        return Err(CodecError::NonCanonicalPayloadPadding);
    }
    Ok(())
}

fn validate_engine_request(request: &EngineRequest) -> Result<(), CodecError> {
    let expected = compute_execution_digest(&request.binding)?;
    if request.execution_digest != expected {
        return Err(CodecError::ExecutionDigestMismatch);
    }
    Ok(())
}

fn validate_opaque_account_count(actual: u16) -> Result<(), CodecError> {
    if usize::from(actual) > MAX_OPAQUE_ACCOUNTS {
        return Err(CodecError::InvalidOpaqueAccountCount {
            maximum: MAX_OPAQUE_ACCOUNTS as u16,
            actual,
        });
    }
    Ok(())
}

fn require_length(data: &[u8], expected: usize) -> Result<(), CodecError> {
    if data.len() != expected {
        return Err(CodecError::InvalidLength {
            expected,
            actual: data.len(),
        });
    }
    Ok(())
}

fn require_version(actual: u8) -> Result<(), CodecError> {
    if actual != PROBE_WIRE_VERSION {
        return Err(CodecError::UnsupportedVersion {
            expected: PROBE_WIRE_VERSION,
            actual,
        });
    }
    Ok(())
}

fn require_timing_mode(actual: u8) -> Result<(), CodecError> {
    if matches!(actual, TIMING_SINGLE | TIMING_PREPARE_COMMIT) {
        Ok(())
    } else {
        Err(CodecError::UnsupportedTimingMode { actual })
    }
}

fn require_phase(actual: u8) -> Result<(), CodecError> {
    if matches!(actual, PHASE_TRANSITION | PHASE_PREPARE | PHASE_COMMIT) {
        Ok(())
    } else {
        Err(CodecError::UnsupportedPhase { actual })
    }
}

fn put_u8<const N: usize>(output: &mut [u8; N], cursor: &mut usize, value: u8) {
    output[*cursor] = value;
    *cursor += 1;
}

fn put_u16<const N: usize>(output: &mut [u8; N], cursor: &mut usize, value: u16) {
    put_bytes(output, cursor, &value.to_le_bytes());
}

fn put_u64<const N: usize>(output: &mut [u8; N], cursor: &mut usize, value: u64) {
    put_bytes(output, cursor, &value.to_le_bytes());
}

fn put_pubkey<const N: usize>(output: &mut [u8; N], cursor: &mut usize, value: &Pubkey) {
    put_bytes(output, cursor, value.as_ref());
}

fn put_bytes<const N: usize>(output: &mut [u8; N], cursor: &mut usize, value: &[u8]) {
    let end = *cursor + value.len();
    output[*cursor..end].copy_from_slice(value);
    *cursor = end;
}

fn read_u8(data: &[u8], cursor: &mut usize) -> u8 {
    let value = data[*cursor];
    *cursor += 1;
    value
}

fn read_u16(data: &[u8], cursor: &mut usize) -> u16 {
    u16::from_le_bytes(read_array_2(data, cursor))
}

fn read_u64(data: &[u8], cursor: &mut usize) -> u64 {
    u64::from_le_bytes(read_array_8(data, cursor))
}

fn read_pubkey(data: &[u8], cursor: &mut usize) -> Pubkey {
    Pubkey::new_from_array(read_array_32(data, cursor))
}

fn read_array_2(data: &[u8], cursor: &mut usize) -> [u8; 2] {
    let end = *cursor + 2;
    let mut value = [0_u8; 2];
    value.copy_from_slice(&data[*cursor..end]);
    *cursor = end;
    value
}

fn read_array_8(data: &[u8], cursor: &mut usize) -> [u8; 8] {
    let end = *cursor + 8;
    let mut value = [0_u8; 8];
    value.copy_from_slice(&data[*cursor..end]);
    *cursor = end;
    value
}

fn read_array_32(data: &[u8], cursor: &mut usize) -> [u8; 32] {
    let end = *cursor + 32;
    let mut value = [0_u8; 32];
    value.copy_from_slice(&data[*cursor..end]);
    *cursor = end;
    value
}

fn read_array_128(data: &[u8], cursor: &mut usize) -> [u8; MAX_OPAQUE_PAYLOAD_LEN] {
    let end = *cursor + MAX_OPAQUE_PAYLOAD_LEN;
    let mut value = [0_u8; MAX_OPAQUE_PAYLOAD_LEN];
    value.copy_from_slice(&data[*cursor..end]);
    *cursor = end;
    value
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    const INTENT_TIMING_OFFSET: usize = 1;
    const EXECUTION_PHASE_OFFSET: usize = 1;
    const EXECUTION_PHASE_CONTEXT_OFFSET: usize = 2 + HASH_LEN;
    const EXECUTION_OPAQUE_COUNT_OFFSET: usize = 2
        + (EXECUTION_BINDING_PUBKEYS * PUBKEY_LEN)
        + (EXECUTION_BINDING_U64S * U64_LEN)
        + (EXECUTION_BINDING_HASHES * HASH_LEN);
    const EXECUTION_PAYLOAD_LEN_OFFSET: usize = EXECUTION_OPAQUE_COUNT_OFFSET + U16_LEN;
    const EXECUTION_PAYLOAD_OFFSET: usize = EXECUTION_PAYLOAD_LEN_OFFSET + U16_LEN;

    fn pubkey(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn descriptor(byte: u8) -> CapabilityDescriptor {
        CapabilityDescriptor {
            key: pubkey(byte),
            owner: pubkey(byte.wrapping_add(32)),
            is_writable: false,
            is_signer: false,
            is_executable: false,
        }
    }

    fn intent() -> IntentBinding {
        IntentBinding {
            timing_mode: TIMING_SINGLE,
            core_program: pubkey(1),
            market: pubkey(2),
            domain: pubkey(3),
            engine_program: pubkey(4),
            engine_state: pubkey(5),
            user_authority: pubkey(6),
            user_input: pubkey(7),
            user_output: pubkey(8),
            mint_in: pubkey(9),
            mint_out: pubkey(10),
            domain_input_vault: pubkey(11),
            domain_output_vault: pubkey(12),
            protocol_fee_vault: pubkey(13),
            fee_ledger: pubkey(14),
            token_program: pubkey(15),
            engine_revision: 16,
            fee_policy_revision: 17,
            amount_in: 18,
            protocol_fee: 19,
            max_total_input_debit: 20,
            min_output_credit: 21,
            max_protocol_fee: 22,
            expires_at_slot: 23,
            authorization_nonce: [24; 32],
            authorized_capability_hash: [25; 32],
            payload_hash: [26; 32],
        }
    }

    fn execution() -> ExecutionBinding {
        ExecutionBinding::new(
            PHASE_TRANSITION,
            [1; 32],
            [0; 32],
            pubkey(2),
            pubkey(3),
            4,
            5,
            6,
            7,
            8,
            9,
            10,
            [11; 32],
            [12; 32],
            2,
            b"curve:constant-product",
        )
        .unwrap()
    }

    fn settlement() -> SettlementBinding {
        SettlementBinding {
            intent_digest: [1; 32],
            primary_execution_digest: [2; 32],
            primary_receipt_digest: [3; 32],
            amount_in: 4,
            amount_out: 5,
            protocol_fee: 6,
            total_input_debit: 7,
            accounted_input_before: 8,
            accounted_output_before: 9,
            accounted_fee_before: 10,
            accounted_input_after: 11,
            accounted_output_after: 12,
            accounted_fee_after: 13,
            observed_source_after: 14,
            observed_destination_after: 15,
            observed_input_vault_after: 16,
            observed_output_vault_after: 17,
            observed_fee_vault_after: 18,
        }
    }

    fn request() -> EngineRequest {
        EngineRequest::new(execution()).unwrap()
    }

    #[test]
    fn identities_seeds_discriminators_and_lengths_are_stable() {
        assert_eq!(
            DISPOSABLE_CORE_PROGRAM_ID.to_string(),
            "Bwhiw9S9ZdHkEhFF2Ps89HMxa5dHX1xSbdsGZ8W3qR2b"
        );
        assert_eq!(
            DISPOSABLE_ENGINE_PROGRAM_ID.to_string(),
            "5UNyG5GQpPwyoDgsvt4JzdqJxJzPh52pVbUDjEa5Gikh"
        );
        assert_eq!(
            DISPOSABLE_ROUTER_PROGRAM_ID.to_string(),
            "F62maceZqpLAayyBLsXNGdrmKg9cZWdpSDbzoHuNgk6Q"
        );
        assert_eq!(
            DISPOSABLE_HELPER_PROGRAM_ID.to_string(),
            "6QXXm7aqjRxQGJ6V3nvtS5taHuojM9SisVrHg3Xrj1Vj"
        );
        assert_eq!(SPEND_AUTHORITY_SEED, b"spend:v0");
        assert_eq!(CALLBACK_AUTHORITY_SEED, b"engine-callback:v0");

        for (name, expected) in [
            (
                b"global:transition".as_slice(),
                ENGINE_TRANSITION_DISCRIMINATOR,
            ),
            (b"global:prepare".as_slice(), ENGINE_PREPARE_DISCRIMINATOR),
            (b"global:commit".as_slice(), ENGINE_COMMIT_DISCRIMINATOR),
        ] {
            let digest = solana_sha256_hasher::hashv(&[name]).to_bytes();
            assert_eq!(&digest[..8], &expected);
        }

        assert_eq!(INTENT_BINDING_LEN, 642);
        assert_eq!(EXECUTION_BINDING_LEN, 382);
        assert_eq!(ENGINE_REQUEST_LEN, 414);
        assert_eq!(ENGINE_INSTRUCTION_LEN, 422);
        assert_eq!(ENGINE_RECEIPT_LEN, 90);
        assert_eq!(SETTLEMENT_BINDING_LEN, 217);
    }

    #[test]
    fn timing_phase_matrix_is_explicit_and_fail_closed() {
        assert_eq!(
            validate_phase_for_timing(TIMING_SINGLE, PHASE_TRANSITION),
            Ok(())
        );
        assert_eq!(
            validate_phase_for_timing(TIMING_PREPARE_COMMIT, PHASE_PREPARE),
            Ok(())
        );
        assert_eq!(
            validate_phase_for_timing(TIMING_PREPARE_COMMIT, PHASE_COMMIT),
            Ok(())
        );
        assert_eq!(
            validate_phase_for_timing(TIMING_SINGLE, PHASE_PREPARE),
            Err(CodecError::InvalidPhaseForTiming {
                timing_mode: TIMING_SINGLE,
                phase: PHASE_PREPARE,
            })
        );
        assert_eq!(
            validate_phase_for_timing(TIMING_PREPARE_COMMIT, PHASE_TRANSITION),
            Err(CodecError::InvalidPhaseForTiming {
                timing_mode: TIMING_PREPARE_COMMIT,
                phase: PHASE_TRANSITION,
            })
        );
        assert_eq!(
            validate_phase_for_timing(9, PHASE_TRANSITION),
            Err(CodecError::UnsupportedTimingMode { actual: 9 })
        );
        assert_eq!(
            validate_phase_for_timing(TIMING_SINGLE, 9),
            Err(CodecError::UnsupportedPhase { actual: 9 })
        );
    }

    #[test]
    fn capability_hash_binds_target_order_position_duplicates_and_flags() {
        let first = descriptor(1);
        let second = descriptor(2);
        let baseline = compute_capability_hash(&pubkey(9), &[first, second]).unwrap();

        assert_ne!(
            baseline,
            compute_capability_hash(&pubkey(8), &[first, second]).unwrap()
        );
        assert_ne!(
            baseline,
            compute_capability_hash(&pubkey(9), &[second, first]).unwrap()
        );
        assert_ne!(
            baseline,
            compute_capability_hash(&pubkey(9), &[first]).unwrap()
        );

        let mut changed = first;
        changed.key = pubkey(3);
        assert_ne!(
            baseline,
            compute_capability_hash(&pubkey(9), &[changed, second]).unwrap()
        );
        changed = first;
        changed.owner = pubkey(4);
        assert_ne!(
            baseline,
            compute_capability_hash(&pubkey(9), &[changed, second]).unwrap()
        );
        changed = first;
        changed.is_writable = true;
        assert_ne!(
            baseline,
            compute_capability_hash(&pubkey(9), &[changed, second]).unwrap()
        );
        changed = first;
        changed.is_signer = true;
        assert_ne!(
            baseline,
            compute_capability_hash(&pubkey(9), &[changed, second]).unwrap()
        );
        changed = first;
        changed.is_executable = true;
        assert_ne!(
            baseline,
            compute_capability_hash(&pubkey(9), &[changed, second]).unwrap()
        );

        let duplicate = compute_capability_hash(&pubkey(9), &[first, first]).unwrap();
        assert_ne!(
            duplicate,
            compute_capability_hash(&pubkey(9), &[first]).unwrap()
        );

        let maximum = [descriptor(1); MAX_CAPABILITY_DESCRIPTORS];
        assert!(compute_capability_hash(&pubkey(9), &maximum).is_ok());
        let too_many = [descriptor(1); MAX_CAPABILITY_DESCRIPTORS + 1];
        assert_eq!(
            compute_capability_hash(&pubkey(9), &too_many),
            Err(CodecError::TooManyCapabilityDescriptors {
                maximum: MAX_CAPABILITY_DESCRIPTORS,
                actual: MAX_CAPABILITY_DESCRIPTORS + 1,
            })
        );
    }

    #[test]
    fn payload_hash_binds_exact_length_and_bytes() {
        let baseline = compute_payload_hash(b"abc").unwrap();
        assert_ne!(baseline, compute_payload_hash(b"abd").unwrap());
        assert_ne!(baseline, compute_payload_hash(b"abc\0").unwrap());
        assert_ne!(baseline, compute_payload_hash(b"").unwrap());

        let oversized = [7_u8; MAX_OPAQUE_PAYLOAD_LEN + 1];
        assert_eq!(
            compute_payload_hash(&oversized),
            Err(CodecError::PayloadTooLong {
                maximum: MAX_OPAQUE_PAYLOAD_LEN,
                actual: MAX_OPAQUE_PAYLOAD_LEN + 1,
            })
        );
    }

    #[test]
    fn intent_codec_is_fixed_and_digest_binds_every_field() {
        let expected = intent();
        let encoded = encode_intent_binding(&expected).unwrap();
        assert_eq!(encoded[0], PROBE_WIRE_VERSION);
        assert_eq!(encoded[INTENT_TIMING_OFFSET], TIMING_SINGLE);
        assert_eq!(decode_intent_binding(&encoded), Ok(expected));

        let baseline = compute_intent_digest(&expected).unwrap();
        macro_rules! changed {
            ($field:ident, $value:expr) => {{
                let mut mutated = expected;
                mutated.$field = $value;
                assert_ne!(
                    baseline,
                    compute_intent_digest(&mutated).unwrap(),
                    "field {} was not bound",
                    stringify!($field)
                );
            }};
        }

        changed!(timing_mode, TIMING_PREPARE_COMMIT);
        changed!(core_program, pubkey(101));
        changed!(market, pubkey(102));
        changed!(domain, pubkey(103));
        changed!(engine_program, pubkey(104));
        changed!(engine_state, pubkey(105));
        changed!(user_authority, pubkey(106));
        changed!(user_input, pubkey(107));
        changed!(user_output, pubkey(108));
        changed!(mint_in, pubkey(109));
        changed!(mint_out, pubkey(110));
        changed!(domain_input_vault, pubkey(111));
        changed!(domain_output_vault, pubkey(112));
        changed!(protocol_fee_vault, pubkey(113));
        changed!(fee_ledger, pubkey(114));
        changed!(token_program, pubkey(115));
        changed!(engine_revision, 116);
        changed!(fee_policy_revision, 117);
        changed!(amount_in, 118);
        changed!(protocol_fee, 119);
        changed!(max_total_input_debit, 120);
        changed!(min_output_credit, 121);
        changed!(max_protocol_fee, 122);
        changed!(expires_at_slot, 123);
        changed!(authorization_nonce, [124; 32]);
        changed!(authorized_capability_hash, [125; 32]);
        changed!(payload_hash, [126; 32]);
    }

    #[test]
    fn intent_decoder_rejects_shape_version_and_unknown_timing() {
        let encoded = encode_intent_binding(&intent()).unwrap();
        assert_eq!(
            decode_intent_binding(&encoded[..encoded.len() - 1]),
            Err(CodecError::InvalidLength {
                expected: INTENT_BINDING_LEN,
                actual: INTENT_BINDING_LEN - 1,
            })
        );

        let mut wrong_version = encoded;
        wrong_version[0] = 7;
        assert_eq!(
            decode_intent_binding(&wrong_version),
            Err(CodecError::UnsupportedVersion {
                expected: PROBE_WIRE_VERSION,
                actual: 7,
            })
        );

        let mut unknown_timing = encoded;
        unknown_timing[INTENT_TIMING_OFFSET] = 7;
        assert_eq!(
            decode_intent_binding(&unknown_timing),
            Err(CodecError::UnsupportedTimingMode { actual: 7 })
        );

        let mut invalid_struct = intent();
        invalid_struct.timing_mode = 7;
        assert_eq!(
            encode_intent_binding(&invalid_struct),
            Err(CodecError::UnsupportedTimingMode { actual: 7 })
        );
    }

    #[test]
    fn execution_codec_is_padded_and_digest_binds_every_field() {
        let expected = execution();
        let encoded = encode_execution_binding(&expected).unwrap();
        assert_eq!(encoded[0], PROBE_WIRE_VERSION);
        assert_eq!(encoded[EXECUTION_PHASE_OFFSET], PHASE_TRANSITION);
        assert_eq!(
            &encoded[EXECUTION_PAYLOAD_OFFSET
                ..EXECUTION_PAYLOAD_OFFSET + usize::from(expected.payload_len)],
            expected.payload_bytes().unwrap()
        );
        assert!(
            encoded[EXECUTION_PAYLOAD_OFFSET + usize::from(expected.payload_len)..]
                .iter()
                .all(|byte| *byte == 0)
        );
        assert_eq!(decode_execution_binding(&encoded), Ok(expected));

        let baseline = compute_execution_digest(&expected).unwrap();
        macro_rules! changed {
            ($field:ident, $value:expr) => {{
                let mut mutated = expected;
                mutated.$field = $value;
                assert_ne!(
                    baseline,
                    compute_execution_digest(&mutated).unwrap(),
                    "field {} was not bound",
                    stringify!($field)
                );
            }};
        }

        changed!(phase, PHASE_PREPARE);
        changed!(intent_digest, [101; 32]);
        changed!(market, pubkey(102));
        changed!(domain, pubkey(103));
        changed!(engine_revision, 104);
        changed!(amount_in, 105);
        changed!(protocol_fee, 106);
        changed!(accounted_input_before, 107);
        changed!(accounted_output_before, 108);
        changed!(accounted_fee_before, 109);
        changed!(pre_sequence, 110);
        changed!(authorized_capability_hash, [111; 32]);
        changed!(phase_capability_hash, [112; 32]);
        changed!(opaque_account_count, 3);
        changed!(payload_len, expected.payload_len + 1);

        let mut changed_payload = expected;
        changed_payload.payload[0] ^= 1;
        assert_ne!(
            baseline,
            compute_execution_digest(&changed_payload).unwrap()
        );

        let mut commit = expected;
        commit.phase = PHASE_COMMIT;
        commit.phase_context_digest = [1; 32];
        let commit_baseline = compute_execution_digest(&commit).unwrap();
        commit.phase_context_digest = [2; 32];
        assert_ne!(commit_baseline, compute_execution_digest(&commit).unwrap());
    }

    #[test]
    fn execution_decoder_rejects_unknowns_count_length_and_padding() {
        let encoded = encode_execution_binding(&execution()).unwrap();

        let mut unknown_phase = encoded;
        unknown_phase[EXECUTION_PHASE_OFFSET] = 9;
        assert_eq!(
            decode_execution_binding(&unknown_phase),
            Err(CodecError::UnsupportedPhase { actual: 9 })
        );

        let mut unexpected_context = encoded;
        unexpected_context[EXECUTION_PHASE_CONTEXT_OFFSET] = 1;
        assert_eq!(
            decode_execution_binding(&unexpected_context),
            Err(CodecError::InvalidPhaseContext {
                phase: PHASE_TRANSITION,
                expected_nonzero: false,
            })
        );

        assert_eq!(
            ExecutionBinding::new(
                PHASE_COMMIT,
                [0; 32],
                [0; 32],
                pubkey(1),
                pubkey(2),
                3,
                4,
                5,
                6,
                7,
                8,
                9,
                [0; 32],
                [0; 32],
                0,
                &[],
            ),
            Err(CodecError::InvalidPhaseContext {
                phase: PHASE_COMMIT,
                expected_nonzero: true,
            })
        );

        let mut too_many = encoded;
        too_many[EXECUTION_OPAQUE_COUNT_OFFSET..EXECUTION_OPAQUE_COUNT_OFFSET + 2]
            .copy_from_slice(&((MAX_OPAQUE_ACCOUNTS + 1) as u16).to_le_bytes());
        assert_eq!(
            decode_execution_binding(&too_many),
            Err(CodecError::InvalidOpaqueAccountCount {
                maximum: MAX_OPAQUE_ACCOUNTS as u16,
                actual: (MAX_OPAQUE_ACCOUNTS + 1) as u16,
            })
        );

        let mut invalid_payload_len = encoded;
        invalid_payload_len[EXECUTION_PAYLOAD_LEN_OFFSET..EXECUTION_PAYLOAD_LEN_OFFSET + 2]
            .copy_from_slice(&((MAX_OPAQUE_PAYLOAD_LEN + 1) as u16).to_le_bytes());
        assert_eq!(
            decode_execution_binding(&invalid_payload_len),
            Err(CodecError::InvalidPayloadLength {
                maximum: MAX_OPAQUE_PAYLOAD_LEN as u16,
                actual: (MAX_OPAQUE_PAYLOAD_LEN + 1) as u16,
            })
        );

        let mut noncanonical = encoded;
        noncanonical[EXECUTION_BINDING_LEN - 1] = 1;
        assert_eq!(
            decode_execution_binding(&noncanonical),
            Err(CodecError::NonCanonicalPayloadPadding)
        );

        let oversized = [1_u8; MAX_OPAQUE_PAYLOAD_LEN + 1];
        assert_eq!(
            ExecutionBinding::new(
                PHASE_TRANSITION,
                [0; 32],
                [0; 32],
                pubkey(1),
                pubkey(2),
                3,
                4,
                5,
                6,
                7,
                8,
                9,
                [0; 32],
                [0; 32],
                0,
                &oversized,
            ),
            Err(CodecError::PayloadTooLong {
                maximum: MAX_OPAQUE_PAYLOAD_LEN,
                actual: MAX_OPAQUE_PAYLOAD_LEN + 1,
            })
        );
    }

    #[test]
    fn engine_request_and_instruction_verify_digest_and_phase_discriminator() {
        for phase in [PHASE_TRANSITION, PHASE_PREPARE, PHASE_COMMIT] {
            let mut binding = execution();
            binding.phase = phase;
            binding.phase_context_digest = if phase == PHASE_COMMIT {
                [0x5a; 32]
            } else {
                [0; 32]
            };
            let expected = EngineRequest::new(binding).unwrap();
            let encoded = encode_engine_request(&expected).unwrap();
            assert_eq!(decode_engine_request(&encoded), Ok(expected));

            let instruction = encode_engine_instruction(&expected).unwrap();
            assert_eq!(
                &instruction[..8],
                &engine_phase_discriminator(phase).unwrap()
            );
            assert_eq!(decode_engine_instruction(&instruction), Ok(expected));
        }

        let encoded = encode_engine_request(&request()).unwrap();
        let mut wrong_digest = encoded;
        wrong_digest[ENGINE_REQUEST_LEN - 1] ^= 1;
        assert_eq!(
            decode_engine_request(&wrong_digest),
            Err(CodecError::ExecutionDigestMismatch)
        );

        let mut inconsistent = request();
        inconsistent.execution_digest[0] ^= 1;
        assert_eq!(
            encode_engine_request(&inconsistent),
            Err(CodecError::ExecutionDigestMismatch)
        );

        let mut wrong_discriminator = encode_engine_instruction(&request()).unwrap();
        wrong_discriminator[0] ^= 1;
        assert_eq!(
            decode_engine_instruction(&wrong_discriminator),
            Err(CodecError::InvalidDiscriminator)
        );

        let mut trailing = encode_engine_instruction(&request()).unwrap().to_vec();
        trailing.push(0);
        assert_eq!(
            decode_engine_instruction(&trailing),
            Err(CodecError::InvalidLength {
                expected: ENGINE_INSTRUCTION_LEN,
                actual: ENGINE_INSTRUCTION_LEN + 1,
            })
        );
    }

    #[test]
    fn receipt_codec_binds_phase_and_both_digest_layers() {
        let expected = EngineReceipt {
            phase: PHASE_COMMIT,
            intent_digest: [0x5a; 32],
            execution_digest: [0xa5; 32],
            amount_out: 0x0102_0304_0506_0708,
            state_sequence: 0x1112_1314_1516_1718,
        };
        let encoded = encode_receipt(&expected).unwrap();
        assert_eq!(&encoded[..8], &RECEIPT_MAGIC);
        assert_eq!(decode_receipt(&encoded), Ok(expected));
        let baseline_digest = compute_receipt_digest(&expected).unwrap();
        let mut changed_receipt = expected;
        changed_receipt.amount_out += 1;
        assert_ne!(
            baseline_digest,
            compute_receipt_digest(&changed_receipt).unwrap()
        );

        let mut wrong_magic = encoded;
        wrong_magic[0] ^= 1;
        assert_eq!(decode_receipt(&wrong_magic), Err(CodecError::InvalidMagic));

        let mut wrong_version = encoded;
        wrong_version[RECEIPT_MAGIC.len()] = 1;
        assert_eq!(
            decode_receipt(&wrong_version),
            Err(CodecError::UnsupportedVersion {
                expected: PROBE_WIRE_VERSION,
                actual: 1,
            })
        );

        let mut unknown_phase = encoded;
        unknown_phase[RECEIPT_MAGIC.len() + 1] = 9;
        assert_eq!(
            decode_receipt(&unknown_phase),
            Err(CodecError::UnsupportedPhase { actual: 9 })
        );

        let mut invalid_struct = expected;
        invalid_struct.phase = 9;
        assert_eq!(
            encode_receipt(&invalid_struct),
            Err(CodecError::UnsupportedPhase { actual: 9 })
        );

        assert_eq!(
            decode_receipt(&encoded[..encoded.len() - 1]),
            Err(CodecError::InvalidLength {
                expected: ENGINE_RECEIPT_LEN,
                actual: ENGINE_RECEIPT_LEN - 1,
            })
        );
    }

    #[test]
    fn settlement_codec_is_fixed_and_digest_binds_every_observation() {
        let expected = settlement();
        let encoded = encode_settlement_binding(&expected);
        assert_eq!(encoded[0], PROBE_WIRE_VERSION);
        assert_eq!(decode_settlement_binding(&encoded), Ok(expected));

        let baseline = compute_settlement_digest(&expected);
        macro_rules! changed {
            ($field:ident, $value:expr) => {{
                let mut mutated = expected;
                mutated.$field = $value;
                assert_ne!(
                    baseline,
                    compute_settlement_digest(&mutated),
                    "field {} was not bound",
                    stringify!($field)
                );
            }};
        }

        changed!(intent_digest, [101; 32]);
        changed!(primary_execution_digest, [102; 32]);
        changed!(primary_receipt_digest, [103; 32]);
        changed!(amount_in, 104);
        changed!(amount_out, 105);
        changed!(protocol_fee, 106);
        changed!(total_input_debit, 107);
        changed!(accounted_input_before, 108);
        changed!(accounted_output_before, 109);
        changed!(accounted_fee_before, 110);
        changed!(accounted_input_after, 111);
        changed!(accounted_output_after, 112);
        changed!(accounted_fee_after, 113);
        changed!(observed_source_after, 114);
        changed!(observed_destination_after, 115);
        changed!(observed_input_vault_after, 116);
        changed!(observed_output_vault_after, 117);
        changed!(observed_fee_vault_after, 118);

        let mut wrong_version = encoded;
        wrong_version[0] = 9;
        assert_eq!(
            decode_settlement_binding(&wrong_version),
            Err(CodecError::UnsupportedVersion {
                expected: PROBE_WIRE_VERSION,
                actual: 9,
            })
        );
        assert_eq!(
            decode_settlement_binding(&encoded[..encoded.len() - 1]),
            Err(CodecError::InvalidLength {
                expected: SETTLEMENT_BINDING_LEN,
                actual: SETTLEMENT_BINDING_LEN - 1,
            })
        );
    }
}
