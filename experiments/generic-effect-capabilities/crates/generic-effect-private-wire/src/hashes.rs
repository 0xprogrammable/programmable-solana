//! Exact domain-separated SHA-256 helper used only by this experiment.

use alloc::vec::Vec;

use crate::codec::{checked_u16, checked_u32, put_bytes, put_u16, put_u32};
use crate::{WireError, WireResult};

pub(crate) const PRIVATE_HASH_DOMAIN: &[u8] =
    b"programmable/private-effect-capabilities/2026-08-28";

pub(crate) const LABEL_OPAQUE_CAPABILITY_LIST: &[u8] = b"opaque-capability-set-v0";
pub(crate) const LABEL_INTENT: &[u8] = b"intent-v0";
pub(crate) const LABEL_INTENT_SET: &[u8] = b"intent-set-v0";
pub(crate) const LABEL_AUTHORIZATION_STATE: &[u8] = b"authorization-state-v0";
pub(crate) const LABEL_AUTHORIZATION_CAPABILITY_STATE: &[u8] = b"authorization-capability-state-v0";
pub(crate) const LABEL_AUTHORIZATION_FEE_STATE: &[u8] = b"authorization-fee-state-v0";
pub(crate) const LABEL_AUTHORIZATION_VIEW_SET: &[u8] = b"authorization-view-set-v0";
pub(crate) const LABEL_INTENT_SPEND_SEED: &[u8] = b"intent-spend-seed-v0";
pub(crate) const LABEL_PROTECTED_EXECUTION: &[u8] = b"protected-execution-v0";
pub(crate) const LABEL_PROTECTED_CAPABILITY_SET: &[u8] = b"protected-capability-set-v0";
pub(crate) const LABEL_CLASSIC_SPL_ENDPOINT_STATE: &[u8] = b"classic-spl-endpoint-state-v0";
pub(crate) const LABEL_OBSERVED_PROTECTED_DELTA_SET: &[u8] = b"observed-protected-delta-set-v0";
pub(crate) const LABEL_FEE_SHARD_SET: &[u8] = b"fee-shard-set-v0";
pub(crate) const LABEL_FEE_SHARD_DESCRIPTOR: &[u8] = b"fee-shard-descriptor-v0";
pub(crate) const LABEL_EXACT_FEE_RECIPIENT: &[u8] = b"exact-fee-recipient-v0";
pub(crate) const LABEL_DOMAIN_DESCRIPTOR: &[u8] = b"domain-descriptor-v0";
pub(crate) const LABEL_DOMAIN_EXECUTION: &[u8] = b"domain-execution-v0";
pub(crate) const LABEL_DOMAIN_SET: &[u8] = b"domain-set-v0";
pub(crate) const LABEL_ASSET_SET: &[u8] = b"asset-set-v0";
pub(crate) const LABEL_ASSET: &[u8] = b"asset-v0";
pub(crate) const LABEL_MARKET_BINDING: &[u8] = b"market-binding-v0";
pub(crate) const LABEL_ENGINE_ADMISSION_POLICY: &[u8] = b"engine-admission-policy-v0";
pub(crate) const LABEL_ENGINE_LOADER_STATE_SNAPSHOT: &[u8] = b"engine-loader-state-snapshot-v0";
pub(crate) const LABEL_IMMUTABLE_ENGINE_RELEASE_OBSERVATION: &[u8] =
    b"immutable-engine-release-observation-v0";
pub(crate) const LABEL_DOMAIN_ADMISSION_ADDRESS: &[u8] = b"domain-admission-address-v0";
pub(crate) const LABEL_DOMAIN_ADMISSION_RECORD: &[u8] = b"domain-admission-record-v0";
pub(crate) const LABEL_OPEN_DOMAIN_RULE: &[u8] = b"open-domain-rule-v0";
pub(crate) const LABEL_OPEN_DOMAIN_ADMISSION: &[u8] = b"open-domain-admission-v0";
pub(crate) const LABEL_CALLBACK_SEED: &[u8] = b"callback-seed-v0";
pub(crate) const LABEL_ENGINE_REQUEST: &[u8] = b"engine-request-v0";
pub(crate) const LABEL_PAYLOAD: &[u8] = b"payload-v0";
pub(crate) const LABEL_CANONICAL_EFFECT: &[u8] = b"canonical-effect-v0";
pub(crate) const LABEL_FEE_ASSESSMENT: &[u8] = b"fee-assessment-v0";
pub(crate) const LABEL_INTENT_CORE_TERMS: &[u8] = b"intent-core-terms-v0";
pub(crate) const LABEL_INTENT_CAPABILITY_TERMS: &[u8] = b"intent-capability-terms-v0";
pub(crate) const LABEL_INTENT_CREDIT_CONSTRAINTS: &[u8] = b"intent-credit-constraints-v0";
pub(crate) const LABEL_INTENT_DEBIT_GROUP: &[u8] = b"intent-debit-group-v0";
pub(crate) const LABEL_FEE_PRINCIPAL: &[u8] = b"fee-principal-v0";
pub(crate) const LABEL_FEE_POLICY: &[u8] = b"fee-policy-v0";
pub(crate) const LABEL_FEE_ROUNDING_GROUP: &[u8] = b"fee-rounding-group-v0";
pub(crate) const LABEL_FEE_COLLECTION: &[u8] = b"fee-collection-v0";
pub(crate) const LABEL_FEE_ASSESSMENT_SET: &[u8] = b"fee-assessment-set-v0";
pub(crate) const LABEL_CORE_VERIFIED_EVIDENCE: &[u8] = b"core-verified-evidence-v0";
pub(crate) const LABEL_ENGINE_ATTESTED_EVIDENCE: &[u8] = b"engine-attested-evidence-v0";
pub(crate) const LABEL_EXACT_ENGINE_INSTANCE_POLICY: &[u8] = b"exact-engine-instance-policy-v0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtectedExecutionRootInputs<'a> {
    pub core_program: &'a [u8; 32],
    pub market_binding_digest: &'a [u8; 32],
    pub engine_loader_state_snapshot_digest: &'a [u8; 32],
    pub domain_set_digest: &'a [u8; 32],
    pub intent_set_digest: &'a [u8; 32],
    pub fee_policy_digest: &'a [u8; 32],
    pub asset_set_digest: &'a [u8; 32],
    pub authorization_view_set_digest: &'a [u8; 32],
    pub fee_shard_set_digest: &'a [u8; 32],
    pub protected_capability_set_digest: &'a [u8; 32],
}

/// Computes exactly `H(label, parts...)` from the frozen private specification.
///
/// The length checks prevent truncation of the encoded `u16` and `u32` fields.
/// Labels must be non-empty lower-case ASCII identifiers. This is experiment
/// machinery, not a public hashing convention.
pub(crate) fn hash_private(label: &[u8], parts: &[&[u8]]) -> WireResult<[u8; 32]> {
    validate_label(label)?;

    let label_len = checked_u16(label.len())?;
    let part_count = checked_u16(parts.len())?;
    let capacity = PRIVATE_HASH_DOMAIN
        .len()
        .checked_add(2)
        .and_then(|value| value.checked_add(label.len()))
        .and_then(|value| value.checked_add(2))
        .ok_or(WireError::LengthOverflow)?;
    let mut preimage = Vec::with_capacity(capacity);
    put_bytes(&mut preimage, PRIVATE_HASH_DOMAIN);
    put_u16(&mut preimage, label_len);
    put_bytes(&mut preimage, label);
    put_u16(&mut preimage, part_count);
    for part in parts {
        put_u32(&mut preimage, checked_u32(part.len())?);
        put_bytes(&mut preimage, part);
    }
    Ok(solana_sha256_hasher::hash(&preimage).to_bytes())
}

/// Computes the specified canonical list root: count first, then every complete
/// row as an independently length-prefixed `H` part. No sorting is performed.
pub(crate) fn hash_list(label: &[u8], rows: &[&[u8]]) -> WireResult<[u8; 32]> {
    let count = checked_u32(rows.len())?.to_le_bytes();
    let mut parts = Vec::with_capacity(rows.len().saturating_add(1));
    parts.push(count.as_slice());
    parts.extend_from_slice(rows);
    hash_private(label, &parts)
}

pub fn compute_payload_digest(payload: &[u8]) -> WireResult<[u8; 32]> {
    hash_private(LABEL_PAYLOAD, &[payload])
}

pub(crate) fn hash_market_binding_row(encoded_binding: &[u8]) -> WireResult<[u8; 32]> {
    hash_private(LABEL_MARKET_BINDING, &[encoded_binding])
}

pub(crate) fn hash_asset_set_rows(rows: &[&[u8]]) -> WireResult<[u8; 32]> {
    hash_list(LABEL_ASSET_SET, rows)
}

pub(crate) fn hash_domain_set_rows(rows: &[&[u8]]) -> WireResult<[u8; 32]> {
    hash_list(LABEL_DOMAIN_SET, rows)
}

pub(crate) fn hash_authorization_view_set_rows(rows: &[&[u8]]) -> WireResult<[u8; 32]> {
    hash_list(LABEL_AUTHORIZATION_VIEW_SET, rows)
}

pub(crate) fn hash_fee_shard_set_rows(rows: &[&[u8]]) -> WireResult<[u8; 32]> {
    hash_list(LABEL_FEE_SHARD_SET, rows)
}

pub(crate) fn hash_protected_capability_set_rows(rows: &[&[u8]]) -> WireResult<[u8; 32]> {
    hash_list(LABEL_PROTECTED_CAPABILITY_SET, rows)
}

pub fn compute_protected_execution_root(
    inputs: ProtectedExecutionRootInputs<'_>,
) -> WireResult<[u8; 32]> {
    let major = crate::CORE_EXPERIMENTAL_MAJOR.to_le_bytes();
    hash_private(
        LABEL_PROTECTED_EXECUTION,
        &[
            inputs.core_program,
            &major,
            inputs.market_binding_digest,
            inputs.engine_loader_state_snapshot_digest,
            inputs.domain_set_digest,
            inputs.intent_set_digest,
            inputs.fee_policy_digest,
            inputs.asset_set_digest,
            inputs.authorization_view_set_digest,
            inputs.fee_shard_set_digest,
            inputs.protected_capability_set_digest,
        ],
    )
}

fn validate_label(label: &[u8]) -> WireResult<()> {
    if label.is_empty()
        || label
            .iter()
            .any(|byte| !byte.is_ascii_lowercase() && !byte.is_ascii_digit() && *byte != b'-')
    {
        Err(WireError::InvalidLabel)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_preimage_is_unambiguous() {
        let joined = hash_private(b"asset-v0", &[b"a", b"bc"]).unwrap();
        let split = hash_private(b"asset-v0", &[b"ab", b"c"]).unwrap();
        assert_ne!(joined, split);
    }

    #[test]
    fn list_root_binds_count_order_and_empty_rows() {
        let empty = hash_list(LABEL_OPAQUE_CAPABILITY_LIST, &[]).unwrap();
        let one_empty = hash_list(LABEL_OPAQUE_CAPABILITY_LIST, &[b""]).unwrap();
        let ab = hash_list(LABEL_OPAQUE_CAPABILITY_LIST, &[b"a", b"b"]).unwrap();
        let ba = hash_list(LABEL_OPAQUE_CAPABILITY_LIST, &[b"b", b"a"]).unwrap();
        assert_ne!(empty, one_empty);
        assert_ne!(ab, ba);
    }

    #[test]
    fn labels_reject_uppercase_and_non_ascii() {
        assert_eq!(hash_private(b"Asset-v0", &[]), Err(WireError::InvalidLabel));
        assert_eq!(hash_private(&[0xff], &[]), Err(WireError::InvalidLabel));
    }
}
