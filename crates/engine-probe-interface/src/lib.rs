#![no_std]

//! Fixed wire codec for the disposable authority-kernel experiment.
//!
//! This crate is deliberately unpublished and is not a promised protocol ABI.
//! Any production interface requires a separate compatibility decision.

use solana_pubkey::Pubkey;

pub const PROBE_WIRE_VERSION: u8 = 0;
pub const DISPOSABLE_CORE_PROGRAM_ID: Pubkey =
    solana_pubkey::pubkey!("CfBnUaJwALVpd5Dtkt39zsvY9nwNTdrNxDvoxuCtKiR3");
pub const DISPOSABLE_ENGINE_PROGRAM_ID: Pubkey =
    solana_pubkey::pubkey!("HAhZQp2iaVWfP2mbSpSJvwMqUGaES4S5i4PxwHr6bNkQ");
pub const EVALUATE_DISCRIMINATOR: [u8; 8] = [0xb3, 0xd3, 0x8e, 0xb7, 0x6c, 0x68, 0x14, 0xd6];
pub const CORE_EXECUTE_ENGINE_PROBE_DISCRIMINATOR: [u8; 8] =
    [0xf7, 0xcc, 0xde, 0xcd, 0x95, 0x4a, 0xc2, 0xb8];
pub const RECEIPT_MAGIC: [u8; 8] = *b"PMBRCP00";
pub const PLAN_DOMAIN: &[u8] = b"programmable:engine-probe:plan:v0";

const PUBKEY_LEN: usize = 32;
const U64_LEN: usize = 8;
const PLAN_PUBKEYS: usize = 15;
const PLAN_U64S: usize = 12;
const REQUEST_PUBKEY_SIZED_FIELDS: usize = 3;
const REQUEST_U64S: usize = 7;

pub const PLAN_BINDING_LEN: usize = 1 + (PLAN_PUBKEYS * PUBKEY_LEN) + (PLAN_U64S * U64_LEN);
pub const ENGINE_REQUEST_LEN: usize =
    1 + (REQUEST_PUBKEY_SIZED_FIELDS * PUBKEY_LEN) + (REQUEST_U64S * U64_LEN);
pub const EVALUATE_INSTRUCTION_LEN: usize = EVALUATE_DISCRIMINATOR.len() + ENGINE_REQUEST_LEN;
pub const ENGINE_RECEIPT_LEN: usize = RECEIPT_MAGIC.len() + 1 + PUBKEY_LEN + U64_LEN;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlanBinding {
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
    pub amount_out: u64,
    pub protocol_fee: u64,
    pub max_total_input_debit: u64,
    pub min_output_credit: u64,
    pub max_protocol_fee: u64,
    pub accounted_input_before: u64,
    pub accounted_output_before: u64,
    pub accounted_fee_before: u64,
    pub expires_at_slot: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineRequest {
    pub plan_hash: [u8; 32],
    pub market: Pubkey,
    pub domain: Pubkey,
    pub engine_revision: u64,
    pub amount_in: u64,
    pub amount_out: u64,
    pub protocol_fee: u64,
    pub accounted_input_before: u64,
    pub accounted_output_before: u64,
    pub accounted_fee_before: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineReceipt {
    pub plan_hash: [u8; 32],
    pub state_sequence: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodecError {
    InvalidLength { expected: usize, actual: usize },
    InvalidDiscriminator,
    InvalidMagic,
    UnsupportedVersion { expected: u8, actual: u8 },
}

pub fn encode_plan_binding(binding: &PlanBinding) -> [u8; PLAN_BINDING_LEN] {
    let mut output = [0_u8; PLAN_BINDING_LEN];
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
    put_u64(&mut output, &mut cursor, binding.amount_out);
    put_u64(&mut output, &mut cursor, binding.protocol_fee);
    put_u64(&mut output, &mut cursor, binding.max_total_input_debit);
    put_u64(&mut output, &mut cursor, binding.min_output_credit);
    put_u64(&mut output, &mut cursor, binding.max_protocol_fee);
    put_u64(&mut output, &mut cursor, binding.accounted_input_before);
    put_u64(&mut output, &mut cursor, binding.accounted_output_before);
    put_u64(&mut output, &mut cursor, binding.accounted_fee_before);
    put_u64(&mut output, &mut cursor, binding.expires_at_slot);

    debug_assert_eq!(cursor, PLAN_BINDING_LEN);
    output
}

pub fn compute_plan_hash(binding: &PlanBinding) -> [u8; 32] {
    let encoded = encode_plan_binding(binding);
    solana_sha256_hasher::hashv(&[PLAN_DOMAIN, &encoded]).to_bytes()
}

pub fn encode_request(request: &EngineRequest) -> [u8; ENGINE_REQUEST_LEN] {
    let mut output = [0_u8; ENGINE_REQUEST_LEN];
    let mut cursor = 0;

    put_u8(&mut output, &mut cursor, PROBE_WIRE_VERSION);
    put_bytes(&mut output, &mut cursor, &request.plan_hash);
    put_pubkey(&mut output, &mut cursor, &request.market);
    put_pubkey(&mut output, &mut cursor, &request.domain);
    put_u64(&mut output, &mut cursor, request.engine_revision);
    put_u64(&mut output, &mut cursor, request.amount_in);
    put_u64(&mut output, &mut cursor, request.amount_out);
    put_u64(&mut output, &mut cursor, request.protocol_fee);
    put_u64(&mut output, &mut cursor, request.accounted_input_before);
    put_u64(&mut output, &mut cursor, request.accounted_output_before);
    put_u64(&mut output, &mut cursor, request.accounted_fee_before);

    debug_assert_eq!(cursor, ENGINE_REQUEST_LEN);
    output
}

pub fn decode_request(data: &[u8]) -> Result<EngineRequest, CodecError> {
    require_length(data, ENGINE_REQUEST_LEN)?;
    let mut cursor = 0;
    require_version(read_u8(data, &mut cursor))?;

    let request = EngineRequest {
        plan_hash: read_array_32(data, &mut cursor),
        market: read_pubkey(data, &mut cursor),
        domain: read_pubkey(data, &mut cursor),
        engine_revision: read_u64(data, &mut cursor),
        amount_in: read_u64(data, &mut cursor),
        amount_out: read_u64(data, &mut cursor),
        protocol_fee: read_u64(data, &mut cursor),
        accounted_input_before: read_u64(data, &mut cursor),
        accounted_output_before: read_u64(data, &mut cursor),
        accounted_fee_before: read_u64(data, &mut cursor),
    };

    debug_assert_eq!(cursor, ENGINE_REQUEST_LEN);
    Ok(request)
}

pub fn encode_evaluate_instruction(request: &EngineRequest) -> [u8; EVALUATE_INSTRUCTION_LEN] {
    let mut output = [0_u8; EVALUATE_INSTRUCTION_LEN];
    output[..EVALUATE_DISCRIMINATOR.len()].copy_from_slice(&EVALUATE_DISCRIMINATOR);
    output[EVALUATE_DISCRIMINATOR.len()..].copy_from_slice(&encode_request(request));
    output
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
    put_bytes(&mut output, &mut cursor, &receipt.plan_hash);
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
        plan_hash: read_array_32(data, &mut cursor),
        state_sequence: read_u64(data, &mut cursor),
    };

    debug_assert_eq!(cursor, ENGINE_RECEIPT_LEN);
    Ok(receipt)
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

fn read_u64(data: &[u8], cursor: &mut usize) -> u64 {
    u64::from_le_bytes(read_array_8(data, cursor))
}

fn read_pubkey(data: &[u8], cursor: &mut usize) -> Pubkey {
    Pubkey::new_from_array(read_array_32(data, cursor))
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

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;

    fn pubkey(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    #[test]
    fn handler_discriminators_are_pinned_to_exact_names() {
        let evaluate = solana_sha256_hasher::hashv(&[b"global:evaluate"]).to_bytes();
        let execute = solana_sha256_hasher::hashv(&[b"global:execute_engine_probe"]).to_bytes();

        assert_eq!(&evaluate[..8], &EVALUATE_DISCRIMINATOR);
        assert_eq!(&execute[..8], &CORE_EXECUTE_ENGINE_PROBE_DISCRIMINATOR);
    }

    fn request() -> EngineRequest {
        EngineRequest {
            plan_hash: [0xa5; 32],
            market: pubkey(1),
            domain: pubkey(2),
            engine_revision: 3,
            amount_in: 4,
            amount_out: 5,
            protocol_fee: 6,
            accounted_input_before: 7,
            accounted_output_before: 8,
            accounted_fee_before: 9,
        }
    }

    fn binding() -> PlanBinding {
        PlanBinding {
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
            amount_out: 19,
            protocol_fee: 20,
            max_total_input_debit: 21,
            min_output_credit: 22,
            max_protocol_fee: 23,
            accounted_input_before: 24,
            accounted_output_before: 25,
            accounted_fee_before: 26,
            expires_at_slot: 27,
        }
    }

    #[test]
    fn request_codec_is_exact_and_round_trips() {
        let expected = request();
        let encoded = encode_request(&expected);

        assert_eq!(encoded.len(), ENGINE_REQUEST_LEN);
        assert_eq!(encoded[0], PROBE_WIRE_VERSION);
        assert_eq!(&encoded[1..33], &[0xa5; 32]);
        assert_eq!(decode_request(&encoded), Ok(expected));

        let instruction = encode_evaluate_instruction(&expected);
        assert_eq!(&instruction[..8], &EVALUATE_DISCRIMINATOR);
        assert_eq!(decode_evaluate_instruction(&instruction), Ok(expected));
    }

    #[test]
    fn decoders_fail_closed_on_shape_tag_and_version() {
        let encoded = encode_request(&request());
        assert_eq!(
            decode_request(&encoded[..encoded.len() - 1]),
            Err(CodecError::InvalidLength {
                expected: ENGINE_REQUEST_LEN,
                actual: ENGINE_REQUEST_LEN - 1,
            })
        );

        let mut instruction = encode_evaluate_instruction(&request());
        instruction[0] ^= 1;
        assert_eq!(
            decode_evaluate_instruction(&instruction),
            Err(CodecError::InvalidDiscriminator)
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

        let receipt = EngineReceipt {
            plan_hash: [9; 32],
            state_sequence: 10,
        };
        let mut receipt_bytes = encode_receipt(&receipt);
        receipt_bytes[0] ^= 1;
        assert_eq!(
            decode_receipt(&receipt_bytes),
            Err(CodecError::InvalidMagic)
        );
    }

    #[test]
    fn receipt_codec_is_exact_and_round_trips() {
        let expected = EngineReceipt {
            plan_hash: [0x5a; 32],
            state_sequence: 0x0102_0304_0506_0708,
        };
        let encoded = encode_receipt(&expected);

        assert_eq!(&encoded[..8], b"PMBRCP00");
        assert_eq!(encoded[8], PROBE_WIRE_VERSION);
        assert_eq!(&encoded[41..], &expected.state_sequence.to_le_bytes());
        assert_eq!(decode_receipt(&encoded), Ok(expected));
    }

    #[test]
    fn plan_binding_encoding_is_ordered_and_hash_is_stable() {
        let encoded = encode_plan_binding(&binding());

        assert_eq!(encoded.len(), PLAN_BINDING_LEN);
        assert_eq!(encoded[0], PROBE_WIRE_VERSION);
        for byte in 1_u8..=15 {
            let start = 1 + (usize::from(byte - 1) * 32);
            assert_eq!(&encoded[start..start + 32], &[byte; 32]);
        }
        for value in 16_u64..=27 {
            let start = 1 + (15 * 32) + ((value as usize - 16) * 8);
            assert_eq!(&encoded[start..start + 8], &value.to_le_bytes());
        }

        assert_eq!(
            compute_plan_hash(&binding()),
            [
                105, 20, 110, 43, 125, 224, 200, 63, 65, 198, 155, 100, 47, 0, 190, 36, 128, 201,
                69, 244, 156, 184, 117, 51, 250, 57, 177, 22, 105, 69, 19, 239,
            ]
        );
    }

    #[test]
    fn every_bound_field_changes_the_plan_hash() {
        let baseline = binding();
        let expected = compute_plan_hash(&baseline);

        macro_rules! assert_field_is_bound {
            ($field:ident, $replacement:expr) => {{
                let mut changed = baseline;
                changed.$field = $replacement;
                assert_ne!(
                    compute_plan_hash(&changed),
                    expected,
                    "{} is not bound",
                    stringify!($field)
                );
            }};
        }

        assert_field_is_bound!(core_program, pubkey(31));
        assert_field_is_bound!(market, pubkey(32));
        assert_field_is_bound!(domain, pubkey(33));
        assert_field_is_bound!(engine_program, pubkey(34));
        assert_field_is_bound!(engine_state, pubkey(35));
        assert_field_is_bound!(user_authority, pubkey(36));
        assert_field_is_bound!(user_input, pubkey(37));
        assert_field_is_bound!(user_output, pubkey(38));
        assert_field_is_bound!(mint_in, pubkey(39));
        assert_field_is_bound!(mint_out, pubkey(40));
        assert_field_is_bound!(domain_input_vault, pubkey(41));
        assert_field_is_bound!(domain_output_vault, pubkey(42));
        assert_field_is_bound!(protocol_fee_vault, pubkey(43));
        assert_field_is_bound!(fee_ledger, pubkey(44));
        assert_field_is_bound!(token_program, pubkey(45));
        assert_field_is_bound!(engine_revision, 116);
        assert_field_is_bound!(fee_policy_revision, 117);
        assert_field_is_bound!(amount_in, 118);
        assert_field_is_bound!(amount_out, 119);
        assert_field_is_bound!(protocol_fee, 120);
        assert_field_is_bound!(max_total_input_debit, 121);
        assert_field_is_bound!(min_output_credit, 122);
        assert_field_is_bound!(max_protocol_fee, 123);
        assert_field_is_bound!(accounted_input_before, 124);
        assert_field_is_bound!(accounted_output_before, 125);
        assert_field_is_bound!(accounted_fee_before, 126);
        assert_field_is_bound!(expires_at_slot, 127);
    }
}
