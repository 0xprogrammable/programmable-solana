use alloc::vec::Vec;

use crate::codec::{
    checked_encoded_length, put_bytes, put_u16, put_u64, put_u8, require_exact_length,
    require_zero, Reader,
};
use crate::hashes::compute_payload_digest;
use crate::rows::{
    AuthorizationSnapshotRowCandidateV0, DomainControlRowCandidateV0, FeeShardRowCandidateV0,
    InlineIntentIdentityRowCandidateV0, SettlementCapabilityRowCandidateV0,
    AUTHORITY_CORE_RESERVED_FEE, AUTHORITY_DOMAIN_ACCOUNTED, AUTHORITY_EXACT_EXTERNAL_CREDIT,
    AUTHORITY_INTENT_FUNDED, AUTHORIZATION_SNAPSHOT_ROW_LEN, DOMAIN_CONTROL_ROW_LEN,
    FEE_CLASS_GROSS_DEBIT_RATE, FEE_CLASS_NONE, FEE_SHARD_ROW_LEN, INLINE_INTENT_IDENTITY_ROW_LEN,
    RIGHT_CORE_RESERVED_FEE, RIGHT_CREDIT, RIGHT_DEBIT, RIGHT_DOMAIN_ACCOUNTED,
    RIGHT_EXACT_EXTERNAL_RECIPIENT, SETTLEMENT_CAPABILITY_ROW_LEN,
    SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT, SETTLEMENT_FLAG_FEE_FUNDING,
    WITNESS_DIRECT_ACTOR, WITNESS_EXACT_DELEGATE, WITNESS_STORED_AUTHORIZATION,
};
use crate::{
    WireError, WireResult, CORE_EXECUTE_EFFECT_DISCRIMINATOR, MAX_ASSETS,
    MAX_AUTHORIZATION_ACCOUNTS, MAX_CONTEXT_ROWS, MAX_DOMAINS, MAX_DOMAIN_CONTROL_ACCOUNTS,
    MAX_ENGINE_MOVES, MAX_FEE_CONTROL_ACCOUNTS, MAX_FEE_SHARDS, MAX_INLINE_INTENTS, MAX_INTENTS,
    MAX_LOADER_POLICY_ACCOUNTS, MAX_OPAQUE_CAPABILITIES, MAX_OPAQUE_PAYLOAD_LEN,
    MAX_PROTECTED_PROFILE_ACCOUNTS, MAX_SETTLEMENT_CAPABILITIES, NONE_INDEX, WIRE_VERSION,
};

pub const EXECUTE_ENVELOPE_HEADER_LEN: usize = 264;
pub const MAX_EXECUTE_ENVELOPE_LEN: usize = 1_424;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecuteEnvelopeHeaderCandidateV0 {
    pub wire_version: u8,
    pub loader_policy_account_count: u8,
    pub domain_control_account_count: u8,
    pub authorization_account_count: u8,
    pub protected_profile_account_count: u8,
    pub fee_control_account_count: u8,
    pub settlement_capability_count: u8,
    pub opaque_capability_count: u8,
    pub domain_count: u8,
    pub intent_count: u8,
    pub inline_intent_row_count: u8,
    pub asset_count: u8,
    pub fee_shard_count: u8,
    /// The complete authorization-snapshot row count. It must equal
    /// `intent_count`; Core derives the distinct engine context count.
    pub authorization_snapshot_row_count: u8,
    pub maximum_engine_moves: u8,
    pub flags: u8,
    pub payload_len: u16,
    pub expires_at_slot_exclusive: u64,
    pub expected_engine_sequence: u64,
    pub intent_set_digest: [u8; 32],
    pub domain_set_digest: [u8; 32],
    pub protected_execution_root: [u8; 32],
    pub expected_opaque_capability_root: [u8; 32],
    pub fee_policy_digest: [u8; 32],
    pub expected_engine_loader_state_snapshot_digest: [u8; 32],
    pub payload_digest: [u8; 32],
}

impl ExecuteEnvelopeHeaderCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; EXECUTE_ENVELOPE_HEADER_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(EXECUTE_ENVELOPE_HEADER_LEN);
        put_u8(&mut output, self.wire_version);
        put_u8(&mut output, self.loader_policy_account_count);
        put_u8(&mut output, self.domain_control_account_count);
        put_u8(&mut output, self.authorization_account_count);
        put_u8(&mut output, self.protected_profile_account_count);
        put_u8(&mut output, self.fee_control_account_count);
        put_u8(&mut output, self.settlement_capability_count);
        put_u8(&mut output, self.opaque_capability_count);
        put_u8(&mut output, self.domain_count);
        put_u8(&mut output, self.intent_count);
        put_u8(&mut output, self.inline_intent_row_count);
        put_u8(&mut output, self.asset_count);
        put_u8(&mut output, self.fee_shard_count);
        put_u8(&mut output, self.authorization_snapshot_row_count);
        put_u8(&mut output, self.maximum_engine_moves);
        put_u8(&mut output, self.flags);
        put_u16(&mut output, self.payload_len);
        put_bytes(&mut output, &[0; 6]);
        put_u64(&mut output, self.expires_at_slot_exclusive);
        put_u64(&mut output, self.expected_engine_sequence);
        put_bytes(&mut output, &self.intent_set_digest);
        put_bytes(&mut output, &self.domain_set_digest);
        put_bytes(&mut output, &self.protected_execution_root);
        put_bytes(&mut output, &self.expected_opaque_capability_root);
        put_bytes(&mut output, &self.fee_policy_digest);
        put_bytes(
            &mut output,
            &self.expected_engine_loader_state_snapshot_digest,
        );
        put_bytes(&mut output, &self.payload_digest);
        Ok(output
            .try_into()
            .expect("execute envelope header has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, EXECUTE_ENVELOPE_HEADER_LEN)?;
        let mut reader = Reader::new(data);
        let header = Self {
            wire_version: reader.read_u8()?,
            loader_policy_account_count: reader.read_u8()?,
            domain_control_account_count: reader.read_u8()?,
            authorization_account_count: reader.read_u8()?,
            protected_profile_account_count: reader.read_u8()?,
            fee_control_account_count: reader.read_u8()?,
            settlement_capability_count: reader.read_u8()?,
            opaque_capability_count: reader.read_u8()?,
            domain_count: reader.read_u8()?,
            intent_count: reader.read_u8()?,
            inline_intent_row_count: reader.read_u8()?,
            asset_count: reader.read_u8()?,
            fee_shard_count: reader.read_u8()?,
            authorization_snapshot_row_count: reader.read_u8()?,
            maximum_engine_moves: reader.read_u8()?,
            flags: reader.read_u8()?,
            payload_len: reader.read_u16()?,
            expires_at_slot_exclusive: {
                let reserved = reader.read_array::<6>()?;
                require_zero("execute envelope header reserved", &reserved)?;
                reader.read_u64()?
            },
            expected_engine_sequence: reader.read_u64()?,
            intent_set_digest: reader.read_array()?,
            domain_set_digest: reader.read_array()?,
            protected_execution_root: reader.read_array()?,
            expected_opaque_capability_root: reader.read_array()?,
            fee_policy_digest: reader.read_array()?,
            expected_engine_loader_state_snapshot_digest: reader.read_array()?,
            payload_digest: reader.read_array()?,
        };
        reader.finish()?;
        header.validate()?;
        Ok(header)
    }

    pub fn validate(&self) -> WireResult<()> {
        if self.wire_version != WIRE_VERSION {
            return Err(WireError::UnsupportedVersion {
                expected: WIRE_VERSION,
                actual: self.wire_version,
            });
        }
        if self.flags != 0 {
            return Err(WireError::UnknownFlags {
                field: "execute envelope flags",
                value: u64::from(self.flags),
            });
        }
        for (field, actual, maximum) in [
            (
                "loader policy account count",
                self.loader_policy_account_count,
                MAX_LOADER_POLICY_ACCOUNTS,
            ),
            (
                "domain control account count",
                self.domain_control_account_count,
                MAX_DOMAIN_CONTROL_ACCOUNTS,
            ),
            (
                "authorization account count",
                self.authorization_account_count,
                MAX_AUTHORIZATION_ACCOUNTS,
            ),
            (
                "protected profile account count",
                self.protected_profile_account_count,
                MAX_PROTECTED_PROFILE_ACCOUNTS,
            ),
            (
                "fee control account count",
                self.fee_control_account_count,
                MAX_FEE_CONTROL_ACCOUNTS,
            ),
            (
                "settlement capability count",
                self.settlement_capability_count,
                MAX_SETTLEMENT_CAPABILITIES,
            ),
            (
                "opaque capability count",
                self.opaque_capability_count,
                MAX_OPAQUE_CAPABILITIES,
            ),
            ("domain count", self.domain_count, MAX_DOMAINS),
            ("intent count", self.intent_count, MAX_INTENTS),
            (
                "inline identity row count",
                self.inline_intent_row_count,
                MAX_INLINE_INTENTS,
            ),
            ("asset count", self.asset_count, MAX_ASSETS),
            ("fee shard count", self.fee_shard_count, MAX_FEE_SHARDS),
            (
                "authorization snapshot row count",
                self.authorization_snapshot_row_count,
                MAX_CONTEXT_ROWS,
            ),
            (
                "maximum engine moves",
                self.maximum_engine_moves,
                MAX_ENGINE_MOVES,
            ),
        ] {
            if usize::from(actual) > maximum {
                return Err(WireError::LimitExceeded {
                    field,
                    maximum,
                    actual: usize::from(actual),
                });
            }
        }
        if self.authorization_snapshot_row_count != self.intent_count {
            return Err(WireError::InvalidLength {
                expected: usize::from(self.intent_count),
                actual: usize::from(self.authorization_snapshot_row_count),
            });
        }
        if self.inline_intent_row_count > self.intent_count {
            return Err(WireError::InvalidLength {
                expected: usize::from(self.intent_count),
                actual: usize::from(self.inline_intent_row_count),
            });
        }
        if usize::from(self.payload_len) > MAX_OPAQUE_PAYLOAD_LEN {
            return Err(WireError::LimitExceeded {
                field: "execute envelope payload length",
                maximum: MAX_OPAQUE_PAYLOAD_LEN,
                actual: usize::from(self.payload_len),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecuteEnvelopeCandidateV0 {
    pub header: ExecuteEnvelopeHeaderCandidateV0,
    pub domain_controls: Vec<DomainControlRowCandidateV0>,
    pub authorization_snapshots: Vec<AuthorizationSnapshotRowCandidateV0>,
    pub inline_intent_identities: Vec<InlineIntentIdentityRowCandidateV0>,
    pub fee_shards: Vec<FeeShardRowCandidateV0>,
    pub settlement_capabilities: Vec<SettlementCapabilityRowCandidateV0>,
    pub payload: Vec<u8>,
}

impl ExecuteEnvelopeCandidateV0 {
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub fn encode(&self) -> WireResult<Vec<u8>> {
        self.validate()?;
        let expected = execute_envelope_encoded_length(&self.header)?;
        let mut output = Vec::with_capacity(expected);
        put_bytes(&mut output, &CORE_EXECUTE_EFFECT_DISCRIMINATOR);
        put_bytes(&mut output, &self.header.encode()?);
        for row in &self.domain_controls {
            put_bytes(&mut output, &row.encode()?);
        }
        for row in &self.authorization_snapshots {
            put_bytes(&mut output, &row.encode()?);
        }
        for row in &self.inline_intent_identities {
            put_bytes(&mut output, &row.encode()?);
        }
        for row in &self.fee_shards {
            put_bytes(&mut output, &row.encode()?);
        }
        for row in &self.settlement_capabilities {
            put_bytes(&mut output, &row.encode()?);
        }
        put_bytes(&mut output, &self.payload);
        debug_assert_eq!(output.len(), expected);
        Ok(output)
    }

    pub fn validate(&self) -> WireResult<()> {
        self.header.validate()?;
        require_count(
            "domain declaration rows",
            self.domain_controls.len(),
            self.header.domain_count,
        )?;
        require_count(
            "authorization snapshot rows",
            self.authorization_snapshots.len(),
            self.header.authorization_snapshot_row_count,
        )?;
        require_count(
            "inline identity rows",
            self.inline_intent_identities.len(),
            self.header.inline_intent_row_count,
        )?;
        require_count(
            "fee shard rows",
            self.fee_shards.len(),
            self.header.fee_shard_count,
        )?;
        require_count(
            "settlement capability rows",
            self.settlement_capabilities.len(),
            self.header.settlement_capability_count,
        )?;
        if self.payload.len() != usize::from(self.header.payload_len) {
            return Err(WireError::InvalidLength {
                expected: usize::from(self.header.payload_len),
                actual: self.payload.len(),
            });
        }
        if compute_payload_digest(self.payload())? != self.header.payload_digest {
            return Err(WireError::DigestMismatch {
                field: "execute envelope payload digest",
            });
        }
        self.validate_domain_controls()?;
        self.validate_authorizations()?;
        self.validate_fee_controls()?;
        self.validate_settlement_rows()?;
        Ok(())
    }

    fn validate_domain_controls(&self) -> WireResult<()> {
        let count = self.header.domain_control_account_count;
        let mut used = [false; MAX_DOMAIN_CONTROL_ACCOUNTS];
        for row in &self.domain_controls {
            row.validate()?;
            mark_required_offset(
                "domain descriptor control offset",
                row.descriptor_control_offset,
                count,
                &mut used,
            )?;
            if row.admission_control_offset_or_none != NONE_INDEX {
                mark_required_offset(
                    "domain admission control offset",
                    row.admission_control_offset_or_none,
                    count,
                    &mut used,
                )?;
            }
            mark_required_offset(
                "domain accounting control offset",
                row.accounting_control_offset,
                count,
                &mut used,
            )?;
        }
        require_all_offsets_consumed("domain control offset", count, &used)
    }

    fn validate_authorizations(&self) -> WireResult<()> {
        let count = self.header.authorization_account_count;
        let mut controls = [false; MAX_AUTHORIZATION_ACCOUNTS];
        let mut direct_controls = [false; MAX_AUTHORIZATION_ACCOUNTS];
        let mut direct_actors = [[0_u8; 32]; MAX_AUTHORIZATION_ACCOUNTS];
        let mut identities = [false; MAX_INLINE_INTENTS];
        let mut referenced_authorizations = [false; MAX_INTENTS];
        let mut next_inline_identity = 0_u8;
        for (slot, row) in self.authorization_snapshots.iter().enumerate() {
            row.validate()?;
            if usize::from(row.authorization_slot) != slot {
                return Err(WireError::InvalidIndex {
                    field: "authorization snapshot slot",
                    index: row.authorization_slot,
                    count: self.header.intent_count,
                });
            }
            match row.witness_kind {
                WITNESS_DIRECT_ACTOR => {
                    if row.expected_fill_sequence != 0 {
                        return Err(WireError::UnsupportedValue {
                            field: "inline authorization fill sequence",
                            value: u64::from(row.expected_fill_sequence),
                        });
                    }
                    mark_required_offset(
                        "inline identity index",
                        row.inline_identity_index_or_none,
                        self.header.inline_intent_row_count,
                        &mut identities,
                    )?;
                    if row.inline_identity_index_or_none != next_inline_identity {
                        return Err(WireError::NonCanonicalOrder {
                            field: "inline identity rows by authorization slot",
                        });
                    }
                    next_inline_identity = next_inline_identity
                        .checked_add(1)
                        .ok_or(WireError::LengthOverflow)?;
                    require_index(
                        "direct actor authorization control offset",
                        row.authorization_control_offset_or_none,
                        count,
                    )?;
                    let control = usize::from(row.authorization_control_offset_or_none);
                    let actor = self.inline_intent_identities
                        [usize::from(row.inline_identity_index_or_none)]
                    .actor;
                    if controls[control]
                        && (!direct_controls[control] || direct_actors[control] != actor)
                    {
                        return Err(WireError::DuplicateIndex {
                            field: "authorization control offset",
                            index: row.authorization_control_offset_or_none,
                        });
                    }
                    controls[control] = true;
                    direct_controls[control] = true;
                    direct_actors[control] = actor;
                }
                WITNESS_EXACT_DELEGATE => {
                    if row.authorization_control_offset_or_none != NONE_INDEX {
                        return Err(WireError::InvalidIndex {
                            field: "exact-delegate primary authorization control offset",
                            index: row.authorization_control_offset_or_none,
                            count,
                        });
                    }
                    if row.expected_fill_sequence != 0 {
                        return Err(WireError::UnsupportedValue {
                            field: "inline authorization fill sequence",
                            value: u64::from(row.expected_fill_sequence),
                        });
                    }
                    mark_required_offset(
                        "inline identity index",
                        row.inline_identity_index_or_none,
                        self.header.inline_intent_row_count,
                        &mut identities,
                    )?;
                    if row.inline_identity_index_or_none != next_inline_identity {
                        return Err(WireError::NonCanonicalOrder {
                            field: "inline identity rows by authorization slot",
                        });
                    }
                    next_inline_identity = next_inline_identity
                        .checked_add(1)
                        .ok_or(WireError::LengthOverflow)?;
                }
                WITNESS_STORED_AUTHORIZATION => {
                    if row.inline_identity_index_or_none != NONE_INDEX {
                        return Err(WireError::InvalidIndex {
                            field: "stored authorization inline identity",
                            index: row.inline_identity_index_or_none,
                            count: self.header.inline_intent_row_count,
                        });
                    }
                    mark_required_offset(
                        "stored authorization control offset",
                        row.authorization_control_offset_or_none,
                        count,
                        &mut controls,
                    )?;
                }
                value => {
                    return Err(WireError::UnsupportedValue {
                        field: "authorization witness kind",
                        value: u64::from(value),
                    });
                }
            }
        }
        for capability in &self.settlement_capabilities {
            if matches!(
                capability.authority_class,
                AUTHORITY_INTENT_FUNDED | AUTHORITY_EXACT_EXTERNAL_CREDIT
            ) {
                require_index(
                    "authorization-bound capability slot",
                    capability.authorization_slot_or_none,
                    self.header.intent_count,
                )?;
                referenced_authorizations[usize::from(capability.authorization_slot_or_none)] =
                    true;
            }
            if capability.authority_class != AUTHORITY_INTENT_FUNDED {
                continue;
            }
            require_index(
                "intent-funded authorization slot",
                capability.authorization_slot_or_none,
                self.header.intent_count,
            )?;
            let snapshot =
                &self.authorization_snapshots[usize::from(capability.authorization_slot_or_none)];
            if matches!(
                snapshot.witness_kind,
                WITNESS_EXACT_DELEGATE | WITNESS_STORED_AUTHORIZATION
            ) {
                mark_required_offset(
                    "intent spend authority control offset",
                    capability.spend_authority_control_offset_or_none,
                    count,
                    &mut controls,
                )?;
            } else if capability.spend_authority_control_offset_or_none != NONE_INDEX {
                return Err(WireError::InvalidIndex {
                    field: "direct spend authority control offset",
                    index: capability.spend_authority_control_offset_or_none,
                    count,
                });
            }
            let unconstrained =
                capability.flags & SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT != 0;
            if unconstrained && snapshot.witness_kind != WITNESS_STORED_AUTHORIZATION {
                return Err(WireError::UnsupportedValue {
                    field: "unconstrained debit requires stored witness",
                    value: u64::from(snapshot.witness_kind),
                });
            }
        }
        if referenced_authorizations[..usize::from(self.header.intent_count)]
            .iter()
            .any(|referenced| !referenced)
        {
            return Err(WireError::UnsupportedValue {
                field: "unreferenced authorization snapshot",
                value: 0,
            });
        }
        require_all_offsets_consumed("authorization control offset", count, &controls)?;
        require_all_offsets_consumed(
            "inline identity index",
            self.header.inline_intent_row_count,
            &identities,
        )
    }

    fn validate_fee_controls(&self) -> WireResult<()> {
        let count = self.header.fee_control_account_count;
        let mut used = [false; MAX_FEE_CONTROL_ACCOUNTS];
        for row in &self.fee_shards {
            row.validate()?;
            mark_required_offset(
                "fee shard descriptor offset",
                row.descriptor_control_offset,
                count,
                &mut used,
            )?;
            mark_required_offset(
                "fee liability offset",
                row.liability_control_offset,
                count,
                &mut used,
            )?;
            require_index(
                "fee vault settlement capability index",
                row.vault_settlement_capability_index,
                self.header.settlement_capability_count,
            )?;
            require_index(
                "fee shard asset index",
                row.asset_index,
                self.header.asset_count,
            )?;
        }
        require_all_offsets_consumed("fee control offset", count, &used)
    }

    fn validate_settlement_rows(&self) -> WireResult<()> {
        for row in &self.settlement_capabilities {
            row.validate()?;
            row.validate_indices(
                self.header.asset_count,
                self.header.domain_count,
                self.header.intent_count,
                self.header.fee_shard_count,
            )?;
            validate_authority_shape(row)?;
        }
        for (shard_index, shard) in self.fee_shards.iter().enumerate() {
            let vault =
                &self.settlement_capabilities[usize::from(shard.vault_settlement_capability_index)];
            if vault.authority_class != AUTHORITY_CORE_RESERVED_FEE
                || vault.fee_shard_index_or_none != shard_index as u8
                || vault.asset_index != shard.asset_index
            {
                return Err(WireError::InvalidIndex {
                    field: "fee shard vault binding",
                    index: shard.vault_settlement_capability_index,
                    count: self.header.settlement_capability_count,
                });
            }
        }
        Ok(())
    }
}

pub fn decode_execute_envelope(data: &[u8]) -> WireResult<ExecuteEnvelopeCandidateV0> {
    if data.len() < 8 + EXECUTE_ENVELOPE_HEADER_LEN {
        return Err(WireError::InvalidLength {
            expected: 8 + EXECUTE_ENVELOPE_HEADER_LEN,
            actual: data.len(),
        });
    }
    if data.len() > MAX_EXECUTE_ENVELOPE_LEN {
        return Err(WireError::LimitExceeded {
            field: "execute envelope length",
            maximum: MAX_EXECUTE_ENVELOPE_LEN,
            actual: data.len(),
        });
    }
    let mut reader = Reader::new(data);
    if reader.read_array::<8>()? != CORE_EXECUTE_EFFECT_DISCRIMINATOR {
        return Err(WireError::InvalidDiscriminator);
    }
    let header = ExecuteEnvelopeHeaderCandidateV0::decode_exact(
        &reader.read_vec(EXECUTE_ENVELOPE_HEADER_LEN)?,
    )?;
    let expected = execute_envelope_encoded_length(&header)?;
    require_exact_length(data, expected)?;

    let mut domain_controls = Vec::with_capacity(usize::from(header.domain_count));
    for _ in 0..header.domain_count {
        domain_controls.push(DomainControlRowCandidateV0::decode_exact(
            &reader.read_vec(DOMAIN_CONTROL_ROW_LEN)?,
        )?);
    }
    let mut authorization_snapshots =
        Vec::with_capacity(usize::from(header.authorization_snapshot_row_count));
    for _ in 0..header.authorization_snapshot_row_count {
        authorization_snapshots.push(AuthorizationSnapshotRowCandidateV0::decode_exact(
            &reader.read_vec(AUTHORIZATION_SNAPSHOT_ROW_LEN)?,
        )?);
    }
    let mut inline_intent_identities =
        Vec::with_capacity(usize::from(header.inline_intent_row_count));
    for _ in 0..header.inline_intent_row_count {
        inline_intent_identities.push(InlineIntentIdentityRowCandidateV0::decode_exact(
            &reader.read_vec(INLINE_INTENT_IDENTITY_ROW_LEN)?,
        )?);
    }
    let mut fee_shards = Vec::with_capacity(usize::from(header.fee_shard_count));
    for _ in 0..header.fee_shard_count {
        fee_shards.push(FeeShardRowCandidateV0::decode_exact(
            &reader.read_vec(FEE_SHARD_ROW_LEN)?,
        )?);
    }
    let mut settlement_capabilities =
        Vec::with_capacity(usize::from(header.settlement_capability_count));
    for _ in 0..header.settlement_capability_count {
        settlement_capabilities.push(SettlementCapabilityRowCandidateV0::decode_exact(
            &reader.read_vec(SETTLEMENT_CAPABILITY_ROW_LEN)?,
        )?);
    }
    let payload = reader.read_vec(usize::from(header.payload_len))?;
    reader.finish()?;
    let envelope = ExecuteEnvelopeCandidateV0 {
        header,
        domain_controls,
        authorization_snapshots,
        inline_intent_identities,
        fee_shards,
        settlement_capabilities,
        payload,
    };
    envelope.validate()?;
    Ok(envelope)
}

fn execute_envelope_encoded_length(header: &ExecuteEnvelopeHeaderCandidateV0) -> WireResult<usize> {
    let length = checked_encoded_length(
        8 + EXECUTE_ENVELOPE_HEADER_LEN,
        &[
            (usize::from(header.domain_count), DOMAIN_CONTROL_ROW_LEN),
            (
                usize::from(header.authorization_snapshot_row_count),
                AUTHORIZATION_SNAPSHOT_ROW_LEN,
            ),
            (
                usize::from(header.inline_intent_row_count),
                INLINE_INTENT_IDENTITY_ROW_LEN,
            ),
            (usize::from(header.fee_shard_count), FEE_SHARD_ROW_LEN),
            (
                usize::from(header.settlement_capability_count),
                SETTLEMENT_CAPABILITY_ROW_LEN,
            ),
            (usize::from(header.payload_len), 1),
        ],
    )?;
    if length > MAX_EXECUTE_ENVELOPE_LEN {
        Err(WireError::LimitExceeded {
            field: "execute envelope length",
            maximum: MAX_EXECUTE_ENVELOPE_LEN,
            actual: length,
        })
    } else {
        Ok(length)
    }
}

fn validate_authority_shape(row: &SettlementCapabilityRowCandidateV0) -> WireResult<()> {
    let rights = row.rights_bits;
    let exact = |required: u16| {
        if rights == required {
            Ok(())
        } else {
            Err(WireError::UnknownFlags {
                field: "settlement authority rights shape",
                value: u64::from(rights),
            })
        }
    };
    let fee_funding = row.flags & SETTLEMENT_FLAG_FEE_FUNDING != 0;
    if fee_funding
        && (row.authority_class != AUTHORITY_INTENT_FUNDED
            || row.fee_class == FEE_CLASS_NONE
            || row.maximum_protocol_fee == 0
            || row.fee_shard_index_or_none == NONE_INDEX)
    {
        return Err(WireError::UnsupportedValue {
            field: "fee-funding capability shape",
            value: u64::from(row.flags),
        });
    }
    match row.authority_class {
        AUTHORITY_INTENT_FUNDED => {
            if row.authorization_slot_or_none == NONE_INDEX
                || row.intent_local_term_index_or_none == NONE_INDEX
                || row.domain_accounting_slot_or_none != NONE_INDEX
                || row.fee_class != FEE_CLASS_GROSS_DEBIT_RATE
                || row.maximum_engine_debit == 0
                || row.maximum_total_debit < row.maximum_engine_debit
                || row.minimum_credit != 0
                || row.maximum_protocol_fee > row.maximum_total_debit
                || (!fee_funding
                    && (row.fee_shard_index_or_none != NONE_INDEX
                        || row.maximum_protocol_fee != 0
                        || row.maximum_total_debit != row.maximum_engine_debit))
            {
                return Err(WireError::UnsupportedValue {
                    field: "intent-funded capability shape",
                    value: u64::from(row.authority_class),
                });
            }
            exact(RIGHT_DEBIT)
        }
        AUTHORITY_DOMAIN_ACCOUNTED => {
            if row.flags != 0
                || row.domain_index_or_none == NONE_INDEX
                || row.domain_accounting_slot_or_none == NONE_INDEX
                || row.authorization_slot_or_none != NONE_INDEX
                || row.intent_local_term_index_or_none != NONE_INDEX
                || row.spend_authority_control_offset_or_none != NONE_INDEX
                || row.fee_shard_index_or_none != NONE_INDEX
                || row.fee_class != FEE_CLASS_NONE
                || row.maximum_protocol_fee != 0
                || row.maximum_total_debit != row.maximum_engine_debit
            {
                return Err(WireError::UnsupportedValue {
                    field: "domain-accounted capability shape",
                    value: u64::from(row.authority_class),
                });
            }
            if rights == (RIGHT_DOMAIN_ACCOUNTED | RIGHT_DEBIT) {
                if row.maximum_engine_debit == 0 || row.minimum_credit != 0 {
                    Err(WireError::UnsupportedValue {
                        field: "domain debit amount shape",
                        value: row.minimum_credit,
                    })
                } else {
                    Ok(())
                }
            } else if rights == (RIGHT_DOMAIN_ACCOUNTED | RIGHT_CREDIT) {
                if row.maximum_engine_debit != 0
                    || row.maximum_total_debit != 0
                    || row.minimum_credit != 0
                {
                    Err(WireError::UnsupportedValue {
                        field: "domain credit amount shape",
                        value: row.maximum_engine_debit,
                    })
                } else {
                    Ok(())
                }
            } else {
                Err(WireError::UnknownFlags {
                    field: "domain-accounted capability rights",
                    value: u64::from(rights),
                })
            }
        }
        AUTHORITY_EXACT_EXTERNAL_CREDIT => {
            if row.flags != 0
                || row.domain_accounting_slot_or_none != NONE_INDEX
                || row.authorization_slot_or_none == NONE_INDEX
                || row.intent_local_term_index_or_none == NONE_INDEX
                || row.spend_authority_control_offset_or_none != NONE_INDEX
                || row.fee_shard_index_or_none != NONE_INDEX
                || row.fee_class != FEE_CLASS_NONE
                || row.maximum_engine_debit != 0
                || row.maximum_total_debit != 0
                || row.maximum_protocol_fee != 0
            {
                return Err(WireError::UnsupportedValue {
                    field: "exact external capability shape",
                    value: u64::from(row.authority_class),
                });
            }
            exact(RIGHT_EXACT_EXTERNAL_RECIPIENT | RIGHT_CREDIT)
        }
        AUTHORITY_CORE_RESERVED_FEE => {
            if row.domain_index_or_none != NONE_INDEX
                || row.domain_accounting_slot_or_none != NONE_INDEX
                || row.authorization_slot_or_none != NONE_INDEX
                || row.intent_local_term_index_or_none != NONE_INDEX
                || row.spend_authority_control_offset_or_none != NONE_INDEX
                || row.fee_shard_index_or_none == NONE_INDEX
                || row.fee_class != FEE_CLASS_NONE
                || row.flags != 0
                || row.maximum_engine_debit != 0
                || row.maximum_total_debit != 0
                || row.minimum_credit != 0
                || row.maximum_protocol_fee != 0
            {
                return Err(WireError::UnsupportedValue {
                    field: "reserved fee capability shape",
                    value: u64::from(row.authority_class),
                });
            }
            exact(RIGHT_CORE_RESERVED_FEE | RIGHT_CREDIT)
        }
        value => Err(WireError::UnsupportedValue {
            field: "settlement authority class",
            value: u64::from(value),
        }),
    }
}

fn mark_required_offset<const N: usize>(
    field: &'static str,
    index: u8,
    count: u8,
    used: &mut [bool; N],
) -> WireResult<()> {
    require_index(field, index, count)?;
    let position = usize::from(index);
    if used[position] {
        return Err(WireError::DuplicateIndex { field, index });
    }
    used[position] = true;
    Ok(())
}

fn require_index(field: &'static str, index: u8, count: u8) -> WireResult<()> {
    if index < count {
        Ok(())
    } else {
        Err(WireError::InvalidIndex {
            field,
            index,
            count,
        })
    }
}

fn require_all_offsets_consumed<const N: usize>(
    field: &'static str,
    count: u8,
    used: &[bool; N],
) -> WireResult<()> {
    for (index, is_used) in used.iter().enumerate().take(usize::from(count)) {
        if !is_used {
            return Err(WireError::MissingIndex {
                field,
                index: index as u8,
            });
        }
    }
    Ok(())
}

fn require_count(_field: &'static str, actual: usize, expected: u8) -> WireResult<()> {
    if actual == usize::from(expected) {
        Ok(())
    } else {
        Err(WireError::InvalidLength {
            expected: usize::from(expected),
            actual,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn singleton() -> ExecuteEnvelopeCandidateV0 {
        let payload = alloc::vec![0xaa, 0xbb];
        let payload_digest = compute_payload_digest(&payload).unwrap();
        ExecuteEnvelopeCandidateV0 {
            header: ExecuteEnvelopeHeaderCandidateV0 {
                wire_version: WIRE_VERSION,
                loader_policy_account_count: 1,
                domain_control_account_count: 0,
                authorization_account_count: 1,
                protected_profile_account_count: 2,
                fee_control_account_count: 0,
                settlement_capability_count: 2,
                opaque_capability_count: 0,
                domain_count: 0,
                intent_count: 1,
                inline_intent_row_count: 1,
                asset_count: 1,
                fee_shard_count: 0,
                authorization_snapshot_row_count: 1,
                maximum_engine_moves: 1,
                flags: 0,
                payload_len: payload.len() as u16,
                expires_at_slot_exclusive: 100,
                expected_engine_sequence: 7,
                intent_set_digest: [1; 32],
                domain_set_digest: [2; 32],
                protected_execution_root: [3; 32],
                expected_opaque_capability_root: [4; 32],
                fee_policy_digest: [5; 32],
                expected_engine_loader_state_snapshot_digest: [6; 32],
                payload_digest,
            },
            domain_controls: alloc::vec![],
            authorization_snapshots: alloc::vec![AuthorizationSnapshotRowCandidateV0 {
                authorization_slot: 0,
                witness_kind: WITNESS_DIRECT_ACTOR,
                authorization_control_offset_or_none: 0,
                inline_identity_index_or_none: 0,
                expected_fill_sequence: 0,
            }],
            inline_intent_identities: alloc::vec![InlineIntentIdentityRowCandidateV0 {
                actor: [8; 32],
                engine_terms_commitment: [9; 32],
                authorization_nonce: 8,
                expires_at_slot_exclusive: 100,
            }],
            fee_shards: alloc::vec![],
            settlement_capabilities: alloc::vec![
                SettlementCapabilityRowCandidateV0 {
                    asset_index: 0,
                    domain_index_or_none: NONE_INDEX,
                    authorization_slot_or_none: 0,
                    intent_local_term_index_or_none: 0,
                    authority_class: AUTHORITY_INTENT_FUNDED,
                    fee_shard_index_or_none: NONE_INDEX,
                    fee_class: FEE_CLASS_GROSS_DEBIT_RATE,
                    flags: 0,
                    rights_bits: RIGHT_DEBIT,
                    domain_accounting_slot_or_none: NONE_INDEX,
                    spend_authority_control_offset_or_none: NONE_INDEX,
                    reserved_0: 0,
                    maximum_engine_debit: 10,
                    maximum_total_debit: 10,
                    minimum_credit: 0,
                    maximum_protocol_fee: 0,
                },
                SettlementCapabilityRowCandidateV0 {
                    asset_index: 0,
                    domain_index_or_none: NONE_INDEX,
                    authorization_slot_or_none: 0,
                    intent_local_term_index_or_none: 1,
                    authority_class: AUTHORITY_EXACT_EXTERNAL_CREDIT,
                    fee_shard_index_or_none: NONE_INDEX,
                    fee_class: FEE_CLASS_NONE,
                    flags: 0,
                    rights_bits: RIGHT_EXACT_EXTERNAL_RECIPIENT | RIGHT_CREDIT,
                    domain_accounting_slot_or_none: NONE_INDEX,
                    spend_authority_control_offset_or_none: NONE_INDEX,
                    reserved_0: 0,
                    maximum_engine_debit: 0,
                    maximum_total_debit: 0,
                    minimum_credit: 9,
                    maximum_protocol_fee: 0,
                },
            ],
            payload,
        }
    }

    #[test]
    fn singleton_round_trips_exactly() {
        let envelope = singleton();
        let encoded = envelope.encode().unwrap();
        assert_eq!(encoded.len(), 458);
        assert_eq!(decode_execute_envelope(&encoded), Ok(envelope));
    }

    #[test]
    fn independent_all_axis_maximum_is_1424_and_is_not_a_packet_claim() {
        let mut header = singleton().header;
        header.domain_count = MAX_DOMAINS as u8;
        header.intent_count = MAX_INTENTS as u8;
        header.authorization_snapshot_row_count = MAX_INTENTS as u8;
        header.inline_intent_row_count = MAX_INLINE_INTENTS as u8;
        header.asset_count = MAX_ASSETS as u8;
        header.fee_shard_count = MAX_FEE_SHARDS as u8;
        header.settlement_capability_count = MAX_SETTLEMENT_CAPABILITIES as u8;
        header.payload_len = MAX_OPAQUE_PAYLOAD_LEN as u16;
        header.validate().unwrap();
        assert_eq!(execute_envelope_encoded_length(&header), Ok(1_424));
    }

    #[test]
    fn discriminator_reserved_counts_and_trailing_bytes_fail() {
        let encoded = singleton().encode().unwrap();
        let mut mutation = encoded.clone();
        mutation[0] ^= 1;
        assert_eq!(
            decode_execute_envelope(&mutation),
            Err(WireError::InvalidDiscriminator)
        );

        let mut mutation = encoded.clone();
        mutation[8 + 18] = 1;
        assert!(matches!(
            decode_execute_envelope(&mutation),
            Err(WireError::NonZeroReserved { .. })
        ));

        let mut mutation = encoded.clone();
        mutation[8 + 13] = 2;
        assert!(decode_execute_envelope(&mutation).is_err());

        let mut mutation = encoded;
        mutation.push(0);
        assert!(decode_execute_envelope(&mutation).is_err());
    }

    #[test]
    fn witness_control_and_identity_indices_are_exactly_consumed() {
        let mut envelope = singleton();
        envelope.authorization_snapshots[0].authorization_control_offset_or_none = 1;
        assert!(envelope.encode().is_err());

        let mut envelope = singleton();
        envelope.authorization_snapshots[0].inline_identity_index_or_none = NONE_INDEX;
        assert!(envelope.encode().is_err());

        let mut envelope = singleton();
        envelope.authorization_snapshots[0].witness_kind = WITNESS_STORED_AUTHORIZATION;
        assert!(envelope.encode().is_err());
    }

    #[test]
    fn payload_digest_and_protected_role_shape_fail_closed() {
        let mut envelope = singleton();
        envelope.payload[0] ^= 1;
        assert!(matches!(
            envelope.encode(),
            Err(WireError::DigestMismatch { .. })
        ));

        let mut envelope = singleton();
        envelope.settlement_capabilities[0].authority_class = AUTHORITY_DOMAIN_ACCOUNTED;
        assert!(envelope.encode().is_err());
    }
}
