use std::str::FromStr;

use anchor_lang::{prelude::Pubkey, AnchorDeserialize, Discriminator};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use generic_effect_private_wire::{
    compute_canonical_effect_digest, EffectReceiptCandidateV0, EngineRequestCandidateV0,
};
use litesvm::types::TransactionMetadata;
use programmable_generic_effect_core::events::{
    CoreVerifiedEvidenceCandidateV0, EngineAttestedEvidenceCandidateV0,
};

pub struct DecodedExecutionEvidence {
    pub core_verified: CoreVerifiedEvidenceCandidateV0,
    pub engine_attested: EngineAttestedEvidenceCandidateV0,
}

/// Decodes the two structured Core events and binds them to an independently
/// constructed canonical engine receipt. The transaction-global return-data
/// slot is deliberately ignored: protected SPL settlement performs later CPIs
/// and legitimately becomes its final setter.
pub fn decode_execution_evidence(
    metadata: &TransactionMetadata,
    expected_engine: Pubkey,
    expected_request: &EngineRequestCandidateV0,
    expected_receipt: &EffectReceiptCandidateV0,
) -> Result<DecodedExecutionEvidence, String> {
    expected_request
        .validate()
        .map_err(|error| format!("invalid expected engine request: {error:?}"))?;
    expected_receipt
        .validate()
        .map_err(|error| format!("invalid expected engine receipt: {error:?}"))?;
    let (core_verified, engine_attested) = decode_core_evidence_events(&metadata.logs)?;

    core_verified
        .validate()
        .map_err(|error| format!("invalid CoreVerified evidence: {error}"))?;
    engine_attested
        .validate()
        .map_err(|error| format!("invalid EngineAttested evidence: {error}"))?;
    let canonical_effect_digest = compute_canonical_effect_digest(
        &expected_receipt.request_digest,
        &expected_receipt.protected_execution_root,
        &expected_receipt.moves,
    )
    .map_err(|error| format!("invalid canonical effect receipt: {error:?}"))?;
    let receipt_move_count = u8::try_from(expected_receipt.moves.len())
        .map_err(|_| "engine receipt move count does not fit u8".to_owned())?;
    let request_digest = expected_request
        .digest()
        .map_err(|error| format!("invalid expected engine request: {error:?}"))?;

    if expected_receipt.request_digest != request_digest
        || expected_receipt.intent_set_digest != expected_request.header.intent_set_digest
        || expected_receipt.protected_execution_root
            != expected_request.header.protected_execution_root
        || receipt_move_count > expected_request.header.maximum_engine_moves
    {
        return Err("expected engine receipt does not exactly bind the engine request".to_owned());
    }

    if core_verified.market_binding_digest != expected_request.header.market_binding_digest
        || core_verified.loader_state_snapshot_digest
            != expected_request.header.engine_loader_state_snapshot_digest
        || core_verified.intent_set_digest != expected_receipt.intent_set_digest
        || core_verified.domain_set_digest != expected_request.header.domain_set_digest
        || core_verified.protected_execution_root != expected_receipt.protected_execution_root
        || core_verified.opaque_capability_root != expected_request.header.opaque_capability_root
        || core_verified.request_digest != request_digest
        || core_verified.effect_digest != canonical_effect_digest
        || core_verified.move_count != receipt_move_count
        || core_verified.intent_count != expected_request.header.intent_count
        || core_verified.domain_count != expected_request.header.domain_count
    {
        return Err("CoreVerified evidence does not exactly bind the engine receipt".to_owned());
    }
    if engine_attested.engine_program != expected_engine
        || engine_attested.engine_interface_id != expected_request.header.engine_interface_id
        || engine_attested.engine_instance_id != expected_request.header.engine_instance_id
        || engine_attested.request_digest != request_digest
        || engine_attested.engine_supplied_digest
            != expected_receipt.engine_supplied_evidence_digest
    {
        return Err("EngineAttested evidence does not exactly bind the engine receipt".to_owned());
    }
    Ok(DecodedExecutionEvidence {
        core_verified,
        engine_attested,
    })
}

/// Strictly decodes one CoreVerified event and one EngineAttested event from
/// Core-owned `sol_log_data` frames.
pub fn decode_core_evidence_events(
    logs: &[String],
) -> Result<
    (
        CoreVerifiedEvidenceCandidateV0,
        EngineAttestedEvidenceCandidateV0,
    ),
    String,
> {
    let mut stack = Vec::<Pubkey>::new();
    let mut core_verified = None;
    let mut engine_attested = None;
    let mut observed_order = Vec::with_capacity(2);

    for line in logs {
        if let Some(encoded) = line.strip_prefix("Program data: ") {
            let decoded = STANDARD
                .decode(encoded)
                .map_err(|error| format!("invalid base64 program-data frame: {error}"))?;
            let current_program = stack.last().copied();
            if decoded.starts_with(CoreVerifiedEvidenceCandidateV0::DISCRIMINATOR) {
                require_core_frame(current_program)?;
                if core_verified.is_some() {
                    return Err("duplicate CoreVerified event".to_owned());
                }
                core_verified =
                    Some(decode_anchor_event_exact::<CoreVerifiedEvidenceCandidateV0>(&decoded)?);
                observed_order.push(EvidenceEventKind::CoreVerified);
            } else if decoded.starts_with(EngineAttestedEvidenceCandidateV0::DISCRIMINATOR) {
                require_core_frame(current_program)?;
                if engine_attested.is_some() {
                    return Err("duplicate EngineAttested event".to_owned());
                }
                engine_attested = Some(decode_anchor_event_exact::<
                    EngineAttestedEvidenceCandidateV0,
                >(&decoded)?);
                observed_order.push(EvidenceEventKind::EngineAttested);
            } else if current_program == Some(programmable_generic_effect_core::ID) {
                return Err("unknown Core program-data frame".to_owned());
            }
            continue;
        }

        if let Some((program, depth)) = parse_invoke(line)? {
            let expected_depth = stack
                .len()
                .checked_add(1)
                .ok_or_else(|| "program invocation depth overflowed".to_owned())?;
            if depth != expected_depth {
                return Err(format!(
                    "invocation-stack depth mismatch: expected {expected_depth}, observed {depth}"
                ));
            }
            stack.push(program);
            continue;
        }
        if let Some(program) = parse_exit(line)? {
            let current = stack
                .pop()
                .ok_or_else(|| format!("program {program} exited outside an invocation frame"))?;
            if current != program {
                return Err(format!(
                    "invocation-stack exit mismatch: expected {current}, observed {program}"
                ));
            }
        }
    }

    if !stack.is_empty() {
        return Err("truncated program invocation stack".to_owned());
    }
    if observed_order
        != [
            EvidenceEventKind::CoreVerified,
            EvidenceEventKind::EngineAttested,
        ]
    {
        return Err("Core evidence events are missing, duplicated, or out of order".to_owned());
    }
    Ok((
        core_verified.ok_or_else(|| "missing CoreVerified event".to_owned())?,
        engine_attested.ok_or_else(|| "missing EngineAttested event".to_owned())?,
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EvidenceEventKind {
    CoreVerified,
    EngineAttested,
}

fn decode_anchor_event_exact<T>(data: &[u8]) -> Result<T, String>
where
    T: AnchorDeserialize + Discriminator,
{
    let payload = data
        .strip_prefix(T::DISCRIMINATOR)
        .ok_or_else(|| "event discriminator mismatch".to_owned())?;
    let mut remaining = payload;
    let event = T::deserialize(&mut remaining)
        .map_err(|error| format!("invalid Anchor event payload: {error}"))?;
    if !remaining.is_empty() {
        return Err(format!(
            "Anchor event contains {} trailing bytes",
            remaining.len()
        ));
    }
    Ok(event)
}

fn require_core_frame(current_program: Option<Pubkey>) -> Result<(), String> {
    if current_program == Some(programmable_generic_effect_core::ID) {
        Ok(())
    } else {
        Err(format!(
            "Core evidence discriminator emitted from wrong frame: {:?}",
            current_program
        ))
    }
}

fn parse_invoke(line: &str) -> Result<Option<(Pubkey, usize)>, String> {
    let Some(rest) = line.strip_prefix("Program ") else {
        return Ok(None);
    };
    let Some((program, depth)) = rest.split_once(" invoke [") else {
        return Ok(None);
    };
    let Some(depth) = depth.strip_suffix(']') else {
        return Err(format!("malformed program invocation log: {line}"));
    };
    let program = Pubkey::from_str(program)
        .map_err(|error| format!("invalid invoked program id in log: {error}"))?;
    let depth = depth
        .parse::<usize>()
        .map_err(|error| format!("invalid invocation depth in log: {error}"))?;
    Ok(Some((program, depth)))
}

fn parse_exit(line: &str) -> Result<Option<Pubkey>, String> {
    let Some(rest) = line.strip_prefix("Program ") else {
        return Ok(None);
    };
    let program = if let Some(program) = rest.strip_suffix(" success") {
        Some(program)
    } else {
        rest.split_once(" failed: ").map(|(program, _)| program)
    };
    program
        .map(|program| {
            Pubkey::from_str(program)
                .map_err(|error| format!("invalid exited program id in log: {error}"))
        })
        .transpose()
}
