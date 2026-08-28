#![no_std]

//! Experiment-local deterministic wire types for the disposable
//! engine-generated-settlement probe.
//!
//! This crate is unpublished experiment machinery, not a promised protocol ABI.

extern crate alloc;

use alloc::vec::Vec;
use solana_pubkey::Pubkey;

pub const PROBE_WIRE_VERSION: u8 = 0;
pub const MAX_OPAQUE_ACCOUNTS: usize = 8;
pub const CAPABILITY_PREFIX_ACCOUNTS: usize = 2;
pub const MAX_CAPABILITY_DESCRIPTORS: usize = CAPABILITY_PREFIX_ACCOUNTS + MAX_OPAQUE_ACCOUNTS;
pub const MAX_OPAQUE_PAYLOAD_LEN: usize = 128;

pub const DISPOSABLE_CORE_PROGRAM_ID: Pubkey =
    solana_pubkey::pubkey!("EJKx7XFp6CZQuAHD6AC14g7nUKeczJMr2TX9XRUEjs36");
pub const DISPOSABLE_ENGINE_PROGRAM_ID: Pubkey =
    solana_pubkey::pubkey!("EAX2oQEejkYYTxaVCbQ3pfy9bySj3WMwtV36gvf77Mj1");
pub const DISPOSABLE_HELPER_PROGRAM_ID: Pubkey =
    solana_pubkey::pubkey!("EsZGEzu3NgpwumgwdsjxW3c6xB9wR6gy3qj9Y86nZ7Uv");

pub const EVALUATE_DISCRIMINATOR: [u8; 8] = [0xb3, 0xd3, 0x8e, 0xb7, 0x6c, 0x68, 0x14, 0xd6];
pub const CORE_EXECUTE_ENGINE_GENERATED_PROBE_DISCRIMINATOR: [u8; 8] =
    [0x5f, 0xaa, 0xd9, 0x7c, 0x75, 0x15, 0xb2, 0xae];
pub const RECEIPT_MAGIC: [u8; 8] = *b"PMBGSR00";

pub const CAPABILITY_HASH_DOMAIN: &[u8] = b"programmable:generated-settlement:capability:v0";
pub const PAYLOAD_HASH_DOMAIN: &[u8] = b"programmable:generated-settlement:payload:v0";
pub const REQUEST_HASH_DOMAIN: &[u8] = b"programmable:generated-settlement:request:v0";

const PUBKEY_LEN: usize = 32;
const HASH_LEN: usize = 32;
const U64_LEN: usize = 8;
const U16_LEN: usize = 2;
const REQUEST_BINDING_PUBKEYS: usize = 15;
const REQUEST_BINDING_U64S: usize = 11;
const REQUEST_BINDING_HASHES: usize = 2;

pub const REQUEST_BINDING_LEN: usize = 1
    + (REQUEST_BINDING_PUBKEYS * PUBKEY_LEN)
    + (REQUEST_BINDING_U64S * U64_LEN)
    + (REQUEST_BINDING_HASHES * HASH_LEN);

pub const ENGINE_REQUEST_LEN: usize = 1
    + HASH_LEN
    + (2 * PUBKEY_LEN)
    + (4 * U64_LEN)
    + U16_LEN
    + HASH_LEN
    + U16_LEN
    + MAX_OPAQUE_PAYLOAD_LEN;
pub const EVALUATE_INSTRUCTION_LEN: usize = EVALUATE_DISCRIMINATOR.len() + ENGINE_REQUEST_LEN;
pub const ENGINE_RECEIPT_LEN: usize = RECEIPT_MAGIC.len() + 1 + HASH_LEN + (2 * U64_LEN);

const CAPABILITY_FLAGS_WRITABLE: u8 = 1 << 0;
const CAPABILITY_FLAGS_SIGNER: u8 = 1 << 1;

#[cfg(test)]
const ENGINE_REQUEST_OPAQUE_COUNT_OFFSET: usize = 1 + HASH_LEN + (2 * PUBKEY_LEN) + (4 * U64_LEN);
#[cfg(test)]
const ENGINE_REQUEST_PAYLOAD_LEN_OFFSET: usize =
    ENGINE_REQUEST_OPAQUE_COUNT_OFFSET + U16_LEN + HASH_LEN;
#[cfg(test)]
const ENGINE_REQUEST_PAYLOAD_OFFSET: usize = ENGINE_REQUEST_PAYLOAD_LEN_OFFSET + U16_LEN;

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
pub struct RequestBinding {
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
    pub accounted_input_before: u64,
    pub accounted_output_before: u64,
    pub accounted_fee_before: u64,
    pub expires_at_slot: u64,
    pub capability_hash: [u8; 32],
    pub payload_hash: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineRequest {
    pub request_hash: [u8; 32],
    pub market: Pubkey,
    pub domain: Pubkey,
    pub engine_revision: u64,
    pub amount_in: u64,
    pub accounted_input_before: u64,
    pub accounted_output_before: u64,
    pub opaque_account_count: u16,
    pub capability_hash: [u8; 32],
    pub payload_len: u16,
    pub payload: [u8; MAX_OPAQUE_PAYLOAD_LEN],
}

impl EngineRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_hash: [u8; 32],
        market: Pubkey,
        domain: Pubkey,
        engine_revision: u64,
        amount_in: u64,
        accounted_input_before: u64,
        accounted_output_before: u64,
        opaque_account_count: u16,
        capability_hash: [u8; 32],
        payload: &[u8],
    ) -> Result<Self, CodecError> {
        validate_opaque_account_count(opaque_account_count)?;
        let (payload_len, payload) = canonical_payload(payload)?;
        Ok(Self {
            request_hash,
            market,
            domain,
            engine_revision,
            amount_in,
            accounted_input_before,
            accounted_output_before,
            opaque_account_count,
            capability_hash,
            payload_len,
            payload,
        })
    }

    pub fn payload_bytes(&self) -> Result<&[u8], CodecError> {
        validate_engine_request(self)?;
        Ok(&self.payload[..usize::from(self.payload_len)])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineReceipt {
    pub request_hash: [u8; 32],
    pub amount_out: u64,
    pub state_sequence: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodecError {
    InvalidLength { expected: usize, actual: usize },
    InvalidDiscriminator,
    InvalidMagic,
    UnsupportedVersion { expected: u8, actual: u8 },
    TooManyCapabilityDescriptors { maximum: usize, actual: usize },
    InvalidOpaqueAccountCount { maximum: u16, actual: u16 },
    PayloadTooLong { maximum: usize, actual: usize },
    InvalidPayloadLength { maximum: u16, actual: u16 },
    NonCanonicalPayloadPadding,
}

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

    // Duplicate public keys are intentionally retained. The position, key, and
    // effective flags of every entry are committed independently.
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

pub fn encode_request_binding(binding: &RequestBinding) -> [u8; REQUEST_BINDING_LEN] {
    let mut output = [0_u8; REQUEST_BINDING_LEN];
    let mut cursor = 0;

    put_u8(&mut output, &mut cursor, PROBE_WIRE_VERSION);
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
    put_u64(&mut output, &mut cursor, binding.accounted_input_before);
    put_u64(&mut output, &mut cursor, binding.accounted_output_before);
    put_u64(&mut output, &mut cursor, binding.accounted_fee_before);
    put_u64(&mut output, &mut cursor, binding.expires_at_slot);
    put_bytes(&mut output, &mut cursor, &binding.capability_hash);
    put_bytes(&mut output, &mut cursor, &binding.payload_hash);

    debug_assert_eq!(cursor, REQUEST_BINDING_LEN);
    output
}

pub fn decode_request_binding(data: &[u8]) -> Result<RequestBinding, CodecError> {
    require_length(data, REQUEST_BINDING_LEN)?;
    let mut cursor = 0;
    require_version(read_u8(data, &mut cursor))?;

    let binding = RequestBinding {
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
        accounted_input_before: read_u64(data, &mut cursor),
        accounted_output_before: read_u64(data, &mut cursor),
        accounted_fee_before: read_u64(data, &mut cursor),
        expires_at_slot: read_u64(data, &mut cursor),
        capability_hash: read_array_32(data, &mut cursor),
        payload_hash: read_array_32(data, &mut cursor),
    };

    debug_assert_eq!(cursor, REQUEST_BINDING_LEN);
    Ok(binding)
}

pub fn compute_request_hash(binding: &RequestBinding) -> [u8; 32] {
    let encoded = encode_request_binding(binding);
    solana_sha256_hasher::hashv(&[REQUEST_HASH_DOMAIN, &encoded]).to_bytes()
}

pub fn encode_request(request: &EngineRequest) -> Result<[u8; ENGINE_REQUEST_LEN], CodecError> {
    validate_engine_request(request)?;
    let mut output = [0_u8; ENGINE_REQUEST_LEN];
    let mut cursor = 0;

    put_u8(&mut output, &mut cursor, PROBE_WIRE_VERSION);
    put_bytes(&mut output, &mut cursor, &request.request_hash);
    put_pubkey(&mut output, &mut cursor, &request.market);
    put_pubkey(&mut output, &mut cursor, &request.domain);
    put_u64(&mut output, &mut cursor, request.engine_revision);
    put_u64(&mut output, &mut cursor, request.amount_in);
    put_u64(&mut output, &mut cursor, request.accounted_input_before);
    put_u64(&mut output, &mut cursor, request.accounted_output_before);
    put_u16(&mut output, &mut cursor, request.opaque_account_count);
    put_bytes(&mut output, &mut cursor, &request.capability_hash);
    put_u16(&mut output, &mut cursor, request.payload_len);
    put_bytes(&mut output, &mut cursor, &request.payload);

    debug_assert_eq!(cursor, ENGINE_REQUEST_LEN);
    Ok(output)
}

pub fn decode_request(data: &[u8]) -> Result<EngineRequest, CodecError> {
    require_length(data, ENGINE_REQUEST_LEN)?;
    let mut cursor = 0;
    require_version(read_u8(data, &mut cursor))?;

    let request = EngineRequest {
        request_hash: read_array_32(data, &mut cursor),
        market: read_pubkey(data, &mut cursor),
        domain: read_pubkey(data, &mut cursor),
        engine_revision: read_u64(data, &mut cursor),
        amount_in: read_u64(data, &mut cursor),
        accounted_input_before: read_u64(data, &mut cursor),
        accounted_output_before: read_u64(data, &mut cursor),
        opaque_account_count: read_u16(data, &mut cursor),
        capability_hash: read_array_32(data, &mut cursor),
        payload_len: read_u16(data, &mut cursor),
        payload: read_array_128(data, &mut cursor),
    };

    debug_assert_eq!(cursor, ENGINE_REQUEST_LEN);
    validate_engine_request(&request)?;
    Ok(request)
}

pub fn encode_evaluate_instruction(
    request: &EngineRequest,
) -> Result<[u8; EVALUATE_INSTRUCTION_LEN], CodecError> {
    let encoded_request = encode_request(request)?;
    let mut output = [0_u8; EVALUATE_INSTRUCTION_LEN];
    output[..EVALUATE_DISCRIMINATOR.len()].copy_from_slice(&EVALUATE_DISCRIMINATOR);
    output[EVALUATE_DISCRIMINATOR.len()..].copy_from_slice(&encoded_request);
    Ok(output)
}

pub fn decode_evaluate_instruction(data: &[u8]) -> Result<EngineRequest, CodecError> {
    require_length(data, EVALUATE_INSTRUCTION_LEN)?;
    if data[..EVALUATE_DISCRIMINATOR.len()] != EVALUATE_DISCRIMINATOR {
        return Err(CodecError::InvalidDiscriminator);
    }
    decode_request(&data[EVALUATE_DISCRIMINATOR.len()..])
}

pub fn encode_receipt(receipt: &EngineReceipt) -> [u8; ENGINE_RECEIPT_LEN] {
    let mut output = [0_u8; ENGINE_RECEIPT_LEN];
    let mut cursor = 0;

    put_bytes(&mut output, &mut cursor, &RECEIPT_MAGIC);
    put_u8(&mut output, &mut cursor, PROBE_WIRE_VERSION);
    put_bytes(&mut output, &mut cursor, &receipt.request_hash);
    put_u64(&mut output, &mut cursor, receipt.amount_out);
    put_u64(&mut output, &mut cursor, receipt.state_sequence);

    debug_assert_eq!(cursor, ENGINE_RECEIPT_LEN);
    output
}

pub fn decode_receipt(data: &[u8]) -> Result<EngineReceipt, CodecError> {
    require_length(data, ENGINE_RECEIPT_LEN)?;
    let mut cursor = 0;
    if read_array_8(data, &mut cursor) != RECEIPT_MAGIC {
        return Err(CodecError::InvalidMagic);
    }
    require_version(read_u8(data, &mut cursor))?;

    let receipt = EngineReceipt {
        request_hash: read_array_32(data, &mut cursor),
        amount_out: read_u64(data, &mut cursor),
        state_sequence: read_u64(data, &mut cursor),
    };

    debug_assert_eq!(cursor, ENGINE_RECEIPT_LEN);
    Ok(receipt)
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

fn validate_engine_request(request: &EngineRequest) -> Result<(), CodecError> {
    validate_opaque_account_count(request.opaque_account_count)?;
    let payload_len = usize::from(request.payload_len);
    if payload_len > MAX_OPAQUE_PAYLOAD_LEN {
        return Err(CodecError::InvalidPayloadLength {
            maximum: MAX_OPAQUE_PAYLOAD_LEN as u16,
            actual: request.payload_len,
        });
    }
    if request.payload[payload_len..].iter().any(|byte| *byte != 0) {
        return Err(CodecError::NonCanonicalPayloadPadding);
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

    fn binding() -> RequestBinding {
        RequestBinding {
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
            accounted_input_before: 23,
            accounted_output_before: 24,
            accounted_fee_before: 25,
            expires_at_slot: 26,
            capability_hash: [27; 32],
            payload_hash: [28; 32],
        }
    }

    fn request() -> EngineRequest {
        EngineRequest::new(
            [0xa5; 32],
            pubkey(1),
            pubkey(2),
            3,
            4,
            5,
            6,
            2,
            [0x5a; 32],
            b"curve:constant-product",
        )
        .unwrap()
    }

    #[test]
    fn identities_and_discriminator_are_stable() {
        assert_eq!(
            DISPOSABLE_CORE_PROGRAM_ID.to_string(),
            "EJKx7XFp6CZQuAHD6AC14g7nUKeczJMr2TX9XRUEjs36"
        );
        assert_eq!(
            DISPOSABLE_ENGINE_PROGRAM_ID.to_string(),
            "EAX2oQEejkYYTxaVCbQ3pfy9bySj3WMwtV36gvf77Mj1"
        );
        assert_eq!(
            DISPOSABLE_HELPER_PROGRAM_ID.to_string(),
            "EsZGEzu3NgpwumgwdsjxW3c6xB9wR6gy3qj9Y86nZ7Uv"
        );
        let digest = solana_sha256_hasher::hashv(&[b"global:evaluate"]).to_bytes();
        assert_eq!(&digest[..8], &EVALUATE_DISCRIMINATOR);
        let execute =
            solana_sha256_hasher::hashv(&[b"global:execute_engine_generated_probe"]).to_bytes();
        assert_eq!(
            &execute[..8],
            &CORE_EXECUTE_ENGINE_GENERATED_PROBE_DISCRIMINATOR
        );
    }

    #[test]
    fn capability_hash_binds_target_order_position_and_every_flag() {
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
    }

    #[test]
    fn duplicate_capability_positions_are_preserved_not_deduplicated() {
        let read_only = descriptor(1);
        let mut writable = read_only;
        writable.is_writable = true;

        let duplicate = compute_capability_hash(&pubkey(9), &[read_only, read_only]).unwrap();
        assert_ne!(
            duplicate,
            compute_capability_hash(&pubkey(9), &[read_only]).unwrap()
        );
        assert_ne!(
            compute_capability_hash(&pubkey(9), &[read_only, writable]).unwrap(),
            compute_capability_hash(&pubkey(9), &[writable, read_only]).unwrap()
        );
    }

    #[test]
    fn capability_count_and_payload_length_are_bounded() {
        let descriptors = [descriptor(1); MAX_CAPABILITY_DESCRIPTORS + 1];
        assert_eq!(
            compute_capability_hash(&pubkey(9), &descriptors),
            Err(CodecError::TooManyCapabilityDescriptors {
                maximum: MAX_CAPABILITY_DESCRIPTORS,
                actual: MAX_CAPABILITY_DESCRIPTORS + 1,
            })
        );

        let maximum = [descriptor(1); MAX_CAPABILITY_DESCRIPTORS];
        assert!(compute_capability_hash(&pubkey(9), &maximum).is_ok());

        let oversized = [7_u8; MAX_OPAQUE_PAYLOAD_LEN + 1];
        assert_eq!(
            compute_payload_hash(&oversized),
            Err(CodecError::PayloadTooLong {
                maximum: MAX_OPAQUE_PAYLOAD_LEN,
                actual: MAX_OPAQUE_PAYLOAD_LEN + 1,
            })
        );
        assert_eq!(
            EngineRequest::new(
                [0; 32],
                pubkey(1),
                pubkey(2),
                3,
                4,
                5,
                6,
                (MAX_OPAQUE_ACCOUNTS + 1) as u16,
                [0; 32],
                &[],
            ),
            Err(CodecError::InvalidOpaqueAccountCount {
                maximum: MAX_OPAQUE_ACCOUNTS as u16,
                actual: (MAX_OPAQUE_ACCOUNTS + 1) as u16,
            })
        );
    }

    #[test]
    fn payload_hash_binds_length_and_bytes() {
        let baseline = compute_payload_hash(b"abc").unwrap();
        assert_ne!(baseline, compute_payload_hash(b"abd").unwrap());
        assert_ne!(baseline, compute_payload_hash(b"abc\0").unwrap());
        assert_ne!(baseline, compute_payload_hash(b"").unwrap());

        let maximum = [0x5a; MAX_OPAQUE_PAYLOAD_LEN];
        let maximum_hash = compute_payload_hash(&maximum).unwrap();
        for index in [0, MAX_OPAQUE_PAYLOAD_LEN / 2, MAX_OPAQUE_PAYLOAD_LEN - 1] {
            let mut changed = maximum;
            changed[index] ^= 1;
            assert_ne!(
                maximum_hash,
                compute_payload_hash(&changed).unwrap(),
                "payload byte {index} was not bound"
            );
        }
    }

    #[test]
    fn request_binding_codec_is_exact_and_round_trips() {
        let expected = binding();
        let encoded = encode_request_binding(&expected);
        assert_eq!(encoded.len(), REQUEST_BINDING_LEN);
        assert_eq!(encoded[0], PROBE_WIRE_VERSION);
        assert_eq!(decode_request_binding(&encoded), Ok(expected));
    }

    #[test]
    fn request_hash_changes_for_every_binding_field() {
        let expected = binding();
        let baseline = compute_request_hash(&expected);

        macro_rules! changed {
            ($field:ident, $value:expr) => {{
                let mut mutated = expected;
                mutated.$field = $value;
                assert_ne!(
                    baseline,
                    compute_request_hash(&mutated),
                    "field {} was not bound",
                    stringify!($field)
                );
            }};
        }

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
        changed!(accounted_input_before, 123);
        changed!(accounted_output_before, 124);
        changed!(accounted_fee_before, 125);
        changed!(expires_at_slot, 126);
        changed!(capability_hash, [127; 32]);
        changed!(payload_hash, [128; 32]);
    }

    #[test]
    fn engine_request_codec_is_fixed_padded_and_round_trips() {
        let expected = request();
        let encoded = encode_request(&expected).unwrap();
        assert_eq!(encoded.len(), ENGINE_REQUEST_LEN);
        assert_eq!(encoded[0], PROBE_WIRE_VERSION);
        assert_eq!(
            &encoded[ENGINE_REQUEST_PAYLOAD_OFFSET
                ..ENGINE_REQUEST_PAYLOAD_OFFSET + usize::from(expected.payload_len)],
            expected.payload_bytes().unwrap()
        );
        assert!(
            encoded[ENGINE_REQUEST_PAYLOAD_OFFSET + usize::from(expected.payload_len)..]
                .iter()
                .all(|byte| *byte == 0)
        );
        assert_eq!(decode_request(&encoded), Ok(expected));

        let instruction = encode_evaluate_instruction(&expected).unwrap();
        assert_eq!(&instruction[..8], &EVALUATE_DISCRIMINATOR);
        assert_eq!(decode_evaluate_instruction(&instruction), Ok(expected));
    }

    #[test]
    fn request_decoders_reject_wrong_shape_version_count_and_padding() {
        let encoded = encode_request(&request()).unwrap();
        assert_eq!(
            decode_request(&encoded[..encoded.len() - 1]),
            Err(CodecError::InvalidLength {
                expected: ENGINE_REQUEST_LEN,
                actual: ENGINE_REQUEST_LEN - 1,
            })
        );

        let mut versioned = encoded;
        versioned[0] = 1;
        assert_eq!(
            decode_request(&versioned),
            Err(CodecError::UnsupportedVersion {
                expected: PROBE_WIRE_VERSION,
                actual: 1,
            })
        );

        let mut too_many = encoded;
        too_many[ENGINE_REQUEST_OPAQUE_COUNT_OFFSET..ENGINE_REQUEST_OPAQUE_COUNT_OFFSET + 2]
            .copy_from_slice(&((MAX_OPAQUE_ACCOUNTS + 1) as u16).to_le_bytes());
        assert_eq!(
            decode_request(&too_many),
            Err(CodecError::InvalidOpaqueAccountCount {
                maximum: MAX_OPAQUE_ACCOUNTS as u16,
                actual: (MAX_OPAQUE_ACCOUNTS + 1) as u16,
            })
        );

        let mut invalid_payload_len = encoded;
        invalid_payload_len
            [ENGINE_REQUEST_PAYLOAD_LEN_OFFSET..ENGINE_REQUEST_PAYLOAD_LEN_OFFSET + 2]
            .copy_from_slice(&((MAX_OPAQUE_PAYLOAD_LEN + 1) as u16).to_le_bytes());
        assert_eq!(
            decode_request(&invalid_payload_len),
            Err(CodecError::InvalidPayloadLength {
                maximum: MAX_OPAQUE_PAYLOAD_LEN as u16,
                actual: (MAX_OPAQUE_PAYLOAD_LEN + 1) as u16,
            })
        );

        let mut noncanonical = encoded;
        noncanonical[ENGINE_REQUEST_LEN - 1] = 1;
        assert_eq!(
            decode_request(&noncanonical),
            Err(CodecError::NonCanonicalPayloadPadding)
        );

        let mut invalid_struct = request();
        invalid_struct.payload[MAX_OPAQUE_PAYLOAD_LEN - 1] = 1;
        assert_eq!(
            encode_request(&invalid_struct),
            Err(CodecError::NonCanonicalPayloadPadding)
        );
    }

    #[test]
    fn evaluate_decoder_rejects_discriminator_and_trailing_data() {
        let mut instruction = encode_evaluate_instruction(&request()).unwrap();
        instruction[0] ^= 1;
        assert_eq!(
            decode_evaluate_instruction(&instruction),
            Err(CodecError::InvalidDiscriminator)
        );

        let mut trailing = instruction.to_vec();
        trailing.push(0);
        assert_eq!(
            decode_evaluate_instruction(&trailing),
            Err(CodecError::InvalidLength {
                expected: EVALUATE_INSTRUCTION_LEN,
                actual: EVALUATE_INSTRUCTION_LEN + 1,
            })
        );
    }

    #[test]
    fn binding_and_receipt_decoders_fail_closed() {
        let binding_bytes = encode_request_binding(&binding());
        assert_eq!(
            decode_request_binding(&binding_bytes[..binding_bytes.len() - 1]),
            Err(CodecError::InvalidLength {
                expected: REQUEST_BINDING_LEN,
                actual: REQUEST_BINDING_LEN - 1,
            })
        );

        let receipt = EngineReceipt {
            request_hash: [0x5a; 32],
            amount_out: 0x0102_0304_0506_0708,
            state_sequence: 0x1112_1314_1516_1718,
        };
        let encoded = encode_receipt(&receipt);
        assert_eq!(encoded.len(), ENGINE_RECEIPT_LEN);
        assert_eq!(decode_receipt(&encoded), Ok(receipt));

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

        assert_eq!(
            decode_receipt(&encoded[..encoded.len() - 1]),
            Err(CodecError::InvalidLength {
                expected: ENGINE_RECEIPT_LEN,
                actual: ENGINE_RECEIPT_LEN - 1,
            })
        );
        let mut trailing = encoded.to_vec();
        trailing.push(0);
        assert_eq!(
            decode_receipt(&trailing),
            Err(CodecError::InvalidLength {
                expected: ENGINE_RECEIPT_LEN,
                actual: ENGINE_RECEIPT_LEN + 1,
            })
        );
    }

    #[test]
    fn stable_hash_and_wire_vectors_do_not_drift() {
        let capabilities = [
            descriptor(1),
            CapabilityDescriptor {
                key: pubkey(2),
                owner: pubkey(34),
                is_writable: true,
                is_signer: false,
                is_executable: false,
            },
            CapabilityDescriptor {
                key: pubkey(3),
                owner: pubkey(35),
                is_writable: false,
                is_signer: false,
                is_executable: true,
            },
        ];
        assert_eq!(
            compute_capability_hash(&pubkey(9), &capabilities).unwrap(),
            [
                244, 242, 31, 214, 199, 131, 36, 22, 95, 17, 28, 180, 112, 17, 175, 20, 232, 24,
                145, 66, 89, 229, 6, 239, 1, 117, 151, 220, 138, 216, 186, 6,
            ]
        );
        assert_eq!(
            compute_payload_hash(b"stable payload").unwrap(),
            [
                190, 15, 69, 58, 199, 21, 83, 243, 118, 47, 178, 101, 7, 228, 149, 59, 158, 255,
                154, 214, 207, 26, 53, 8, 94, 234, 68, 25, 213, 71, 116, 66,
            ]
        );
        assert_eq!(
            compute_request_hash(&binding()),
            [
                137, 208, 162, 57, 12, 85, 187, 5, 220, 147, 150, 29, 27, 75, 26, 173, 203, 207,
                221, 58, 98, 218, 222, 62, 94, 146, 57, 69, 66, 18, 42, 139,
            ]
        );

        let receipt = encode_receipt(&EngineReceipt {
            request_hash: [0x5a; 32],
            amount_out: 0x0102_0304_0506_0708,
            state_sequence: 0x1112_1314_1516_1718,
        });
        assert_eq!(&receipt[..9], b"PMBGSR00\0");
        assert_eq!(&receipt[41..49], &0x0102_0304_0506_0708_u64.to_le_bytes());
        assert_eq!(&receipt[49..57], &0x1112_1314_1516_1718_u64.to_le_bytes());
    }
}
