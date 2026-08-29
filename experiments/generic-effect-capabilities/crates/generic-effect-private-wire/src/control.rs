use alloc::vec::Vec;

use crate::codec::{
    put_bytes, put_u32, put_u64, put_u8, require_exact_length, require_zero, Reader,
};
use crate::{
    compute_intent_core_terms_root, compute_intent_digest, CreditConstraintRowCandidateV0,
    EngineAdmissionPolicyCandidateV0, InlineIntentIdentityRowCandidateV0,
    IntentCapabilityTermRowCandidateV0, IntentCoreTermsDigestInputs, IntentDigestInputs, WireError,
    WireResult, AUTHORIZATION_LIFECYCLE_ACTIVE, AUTHORIZATION_LIFECYCLE_CANCELLED,
    AUTHORIZATION_LIFECYCLE_CONSUMED, AUTHORIZATION_LIFECYCLE_DRAFT,
    AUTHORIZATION_LIFECYCLE_EXECUTING, CREDIT_CONSTRAINT_ROW_LEN, DISPOSABLE_CORE_PROGRAM_ID,
    ENGINE_ADMISSION_POLICY_LEN, ENGINE_POLICY_IMMUTABLE, INLINE_INTENT_IDENTITY_ROW_LEN,
    INTENT_CAPABILITY_TERM_ROW_LEN, MAX_SETTLEMENT_CAPABILITIES, WIRE_VERSION,
};

pub const CORE_CAPTURE_IMMUTABLE_ENGINE_RELEASE_DISCRIMINATOR: [u8; 8] =
    [0xe3, 0x64, 0x6e, 0x8c, 0x56, 0xa7, 0xc3, 0x12];
pub const CORE_APPROVE_EXACT_DELEGATE_DISCRIMINATOR: [u8; 8] =
    [0x04, 0xcf, 0x33, 0xc3, 0x5d, 0x50, 0x33, 0x75];
pub const CORE_INITIALIZE_STORED_AUTHORIZATION_DISCRIMINATOR: [u8; 8] =
    [0x76, 0x98, 0x7d, 0xb8, 0xb7, 0x40, 0x0e, 0x4e];
pub const CORE_WRITE_STORED_AUTHORIZATION_CHUNK_DISCRIMINATOR: [u8; 8] =
    [0xbb, 0x97, 0x76, 0x1e, 0x70, 0xf0, 0x0a, 0xd6];
pub const CORE_ACTIVATE_STORED_AUTHORIZATION_DISCRIMINATOR: [u8; 8] =
    [0x91, 0x4d, 0x2e, 0x63, 0x37, 0x52, 0x7a, 0x33];
pub const CORE_REPLACE_STORED_AUTHORIZATION_DISCRIMINATOR: [u8; 8] =
    [0x5f, 0x1f, 0x92, 0x77, 0x3e, 0xd9, 0x3c, 0x7d];
pub const CORE_CANCEL_STORED_AUTHORIZATION_DISCRIMINATOR: [u8; 8] =
    [0x5b, 0x1e, 0xda, 0x99, 0x1f, 0x52, 0x46, 0xe7];

pub const APPROVE_EXACT_DELEGATE_ARGS_LEN: usize = 40;
pub const STORED_AUTHORIZATION_HEADER_LEN: usize = 16;
pub const INITIALIZE_STORED_AUTHORIZATION_ARGS_LEN: usize = 312;
pub const STORED_AUTHORIZATION_CHUNK_HEADER_LEN: usize = 8;
pub const MAX_STORED_AUTHORIZATION_CHUNK_ROWS: usize = 4;

pub const STORED_AUTHORIZATION_CHUNK_KIND_TERM: u8 = 0;
pub const STORED_AUTHORIZATION_CHUNK_KIND_CONSTRAINT: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoredAuthorizationHeaderCandidateV0 {
    pub wire_version: u8,
    pub lifecycle: u8,
    pub bump: u8,
    pub term_count: u8,
    pub constraint_count: u8,
    pub fee_state_count: u8,
    pub flags: u8,
    pub reserved: u8,
    pub term_written_bitmap: u16,
    pub constraint_written_bitmap: u16,
    pub fill_sequence: u32,
}

impl StoredAuthorizationHeaderCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; STORED_AUTHORIZATION_HEADER_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(STORED_AUTHORIZATION_HEADER_LEN);
        put_u8(&mut output, self.wire_version);
        put_u8(&mut output, self.lifecycle);
        put_u8(&mut output, self.bump);
        put_u8(&mut output, self.term_count);
        put_u8(&mut output, self.constraint_count);
        put_u8(&mut output, self.fee_state_count);
        put_u8(&mut output, self.flags);
        put_u8(&mut output, self.reserved);
        crate::codec::put_u16(&mut output, self.term_written_bitmap);
        crate::codec::put_u16(&mut output, self.constraint_written_bitmap);
        put_u32(&mut output, self.fill_sequence);
        Ok(output
            .try_into()
            .expect("stored-authorization header has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, STORED_AUTHORIZATION_HEADER_LEN)?;
        let mut reader = Reader::new(data);
        let header = Self {
            wire_version: reader.read_u8()?,
            lifecycle: reader.read_u8()?,
            bump: reader.read_u8()?,
            term_count: reader.read_u8()?,
            constraint_count: reader.read_u8()?,
            fee_state_count: reader.read_u8()?,
            flags: reader.read_u8()?,
            reserved: reader.read_u8()?,
            term_written_bitmap: reader.read_u16()?,
            constraint_written_bitmap: reader.read_u16()?,
            fill_sequence: reader.read_u32()?,
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
        if self.lifecycle > AUTHORIZATION_LIFECYCLE_CANCELLED {
            return Err(WireError::UnsupportedValue {
                field: "stored authorization lifecycle",
                value: u64::from(self.lifecycle),
            });
        }
        if self.flags != 0 {
            return Err(WireError::UnknownFlags {
                field: "stored authorization header flags",
                value: u64::from(self.flags),
            });
        }
        if self.reserved != 0 {
            return Err(WireError::NonZeroReserved {
                field: "stored authorization header reserved",
            });
        }
        for (field, count) in [
            ("stored authorization term count", self.term_count),
            (
                "stored authorization constraint count",
                self.constraint_count,
            ),
            ("stored authorization fee-state count", self.fee_state_count),
        ] {
            if usize::from(count) > MAX_SETTLEMENT_CAPABILITIES {
                return Err(WireError::LimitExceeded {
                    field,
                    maximum: MAX_SETTLEMENT_CAPABILITIES,
                    actual: usize::from(count),
                });
            }
        }
        if self.term_count == 0 {
            return Err(WireError::UnsupportedValue {
                field: "stored authorization term count",
                value: 0,
            });
        }
        let term_mask = complete_bitmap(self.term_count);
        let constraint_mask = complete_bitmap(self.constraint_count);
        if self.term_written_bitmap & !term_mask != 0
            || self.constraint_written_bitmap & !constraint_mask != 0
        {
            return Err(WireError::UnsupportedValue {
                field: "stored authorization written bitmap",
                value: u64::from(self.term_written_bitmap),
            });
        }
        match self.lifecycle {
            AUTHORIZATION_LIFECYCLE_DRAFT => {
                if self.fill_sequence != 0 || self.fee_state_count != 0 {
                    return Err(WireError::UnsupportedValue {
                        field: "draft stored authorization derived state",
                        value: u64::from(self.fill_sequence),
                    });
                }
            }
            AUTHORIZATION_LIFECYCLE_ACTIVE
            | AUTHORIZATION_LIFECYCLE_EXECUTING
            | AUTHORIZATION_LIFECYCLE_CONSUMED => {
                if self.term_written_bitmap != term_mask
                    || self.constraint_written_bitmap != constraint_mask
                {
                    return Err(WireError::UnsupportedValue {
                        field: "complete stored authorization written bitmap",
                        value: u64::from(self.term_written_bitmap),
                    });
                }
                if matches!(
                    self.lifecycle,
                    AUTHORIZATION_LIFECYCLE_ACTIVE | AUTHORIZATION_LIFECYCLE_EXECUTING
                ) && self.fill_sequence == u32::MAX
                {
                    return Err(WireError::UnsupportedValue {
                        field: "nonterminal stored authorization fill sequence",
                        value: u64::from(self.fill_sequence),
                    });
                }
                if self.lifecycle == AUTHORIZATION_LIFECYCLE_CONSUMED && self.fill_sequence == 0 {
                    return Err(WireError::UnsupportedValue {
                        field: "consumed stored authorization fill sequence",
                        value: 0,
                    });
                }
            }
            AUTHORIZATION_LIFECYCLE_CANCELLED => {
                // A cancelled tombstone preserves either a partial Draft or a
                // complete previously-Active account; both remain canonical.
                let partial = self.term_written_bitmap != term_mask
                    || self.constraint_written_bitmap != constraint_mask;
                if partial && (self.fill_sequence != 0 || self.fee_state_count != 0) {
                    return Err(WireError::UnsupportedValue {
                        field: "cancelled partial stored authorization state",
                        value: u64::from(self.fill_sequence),
                    });
                }
            }
            _ => unreachable!("lifecycle was bounded above"),
        }
        Ok(())
    }
}

fn complete_bitmap(count: u8) -> u16 {
    if count == 0 {
        0
    } else {
        (1_u16 << count) - 1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApproveExactDelegateArgsCandidateV0 {
    pub intent_digest: [u8; 32],
    pub amount: u64,
}

impl ApproveExactDelegateArgsCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; APPROVE_EXACT_DELEGATE_ARGS_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(APPROVE_EXACT_DELEGATE_ARGS_LEN);
        put_bytes(&mut output, &self.intent_digest);
        put_u64(&mut output, self.amount);
        Ok(output
            .try_into()
            .expect("exact-delegate arguments have a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, APPROVE_EXACT_DELEGATE_ARGS_LEN)?;
        let mut reader = Reader::new(data);
        let args = Self {
            intent_digest: reader.read_array()?,
            amount: reader.read_u64()?,
        };
        reader.finish()?;
        args.validate()?;
        Ok(args)
    }

    fn validate(&self) -> WireResult<()> {
        if self.intent_digest == [0; 32] || self.amount == 0 {
            return Err(WireError::UnsupportedValue {
                field: "exact-delegate approval digest or amount",
                value: self.amount,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InitializeStoredAuthorizationArgsCandidateV0 {
    pub wire_version: u8,
    pub term_count: u8,
    pub constraint_count: u8,
    pub flags: u8,
    pub maximum_successful_fills: u32,
    pub identity: InlineIntentIdentityRowCandidateV0,
    pub market_binding_digest: [u8; 32],
    pub engine_loader_state_snapshot_digest: [u8; 32],
    pub fee_policy_digest: [u8; 32],
    pub intent_capability_terms_root: [u8; 32],
    pub credit_constraints_root: [u8; 32],
    pub core_terms_root: [u8; 32],
    pub intent_digest: [u8; 32],
}

impl InitializeStoredAuthorizationArgsCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; INITIALIZE_STORED_AUTHORIZATION_ARGS_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(INITIALIZE_STORED_AUTHORIZATION_ARGS_LEN);
        put_u8(&mut output, self.wire_version);
        put_u8(&mut output, self.term_count);
        put_u8(&mut output, self.constraint_count);
        put_u8(&mut output, self.flags);
        put_u32(&mut output, self.maximum_successful_fills);
        put_bytes(&mut output, &self.identity.encode()?);
        put_bytes(&mut output, &self.market_binding_digest);
        put_bytes(&mut output, &self.engine_loader_state_snapshot_digest);
        put_bytes(&mut output, &self.fee_policy_digest);
        put_bytes(&mut output, &self.intent_capability_terms_root);
        put_bytes(&mut output, &self.credit_constraints_root);
        put_bytes(&mut output, &self.core_terms_root);
        put_bytes(&mut output, &self.intent_digest);
        Ok(output
            .try_into()
            .expect("stored-authorization init arguments have a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, INITIALIZE_STORED_AUTHORIZATION_ARGS_LEN)?;
        let mut reader = Reader::new(data);
        let args = Self {
            wire_version: reader.read_u8()?,
            term_count: reader.read_u8()?,
            constraint_count: reader.read_u8()?,
            flags: reader.read_u8()?,
            maximum_successful_fills: reader.read_u32()?,
            identity: InlineIntentIdentityRowCandidateV0::decode_exact(
                &reader.read_vec(INLINE_INTENT_IDENTITY_ROW_LEN)?,
            )?,
            market_binding_digest: reader.read_array()?,
            engine_loader_state_snapshot_digest: reader.read_array()?,
            fee_policy_digest: reader.read_array()?,
            intent_capability_terms_root: reader.read_array()?,
            credit_constraints_root: reader.read_array()?,
            core_terms_root: reader.read_array()?,
            intent_digest: reader.read_array()?,
        };
        reader.finish()?;
        args.validate()?;
        Ok(args)
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
                field: "stored-authorization init flags",
                value: u64::from(self.flags),
            });
        }
        for (field, count) in [
            ("stored-authorization term count", self.term_count),
            (
                "stored-authorization constraint count",
                self.constraint_count,
            ),
        ] {
            if usize::from(count) > MAX_SETTLEMENT_CAPABILITIES {
                return Err(WireError::LimitExceeded {
                    field,
                    maximum: MAX_SETTLEMENT_CAPABILITIES,
                    actual: usize::from(count),
                });
            }
        }
        if self.maximum_successful_fills == 0 {
            return Err(WireError::UnsupportedValue {
                field: "stored-authorization maximum fills",
                value: 0,
            });
        }
        if self.term_count == 0 {
            return Err(WireError::UnsupportedValue {
                field: "stored-authorization term count",
                value: 0,
            });
        }
        self.identity.encode()?;
        for (field, value) in [
            ("stored market binding", self.market_binding_digest),
            (
                "stored engine loader-state snapshot",
                self.engine_loader_state_snapshot_digest,
            ),
            ("stored fee policy", self.fee_policy_digest),
            (
                "stored intent capability terms root",
                self.intent_capability_terms_root,
            ),
            (
                "stored credit constraints root",
                self.credit_constraints_root,
            ),
            ("stored core terms root", self.core_terms_root),
            ("stored intent digest", self.intent_digest),
        ] {
            if value == [0; 32] {
                return Err(WireError::UnsupportedValue { field, value: 0 });
            }
        }
        let expected_core_terms = compute_intent_core_terms_root(IntentCoreTermsDigestInputs {
            maximum_successful_fills: self.maximum_successful_fills,
            capability_terms_root: &self.intent_capability_terms_root,
            credit_constraints_root: &self.credit_constraints_root,
        })?;
        if self.core_terms_root != expected_core_terms {
            return Err(WireError::DigestMismatch {
                field: "stored core terms root",
            });
        }
        let expected_intent = compute_intent_digest(IntentDigestInputs {
            core_program: &DISPOSABLE_CORE_PROGRAM_ID.to_bytes(),
            market_binding_digest: &self.market_binding_digest,
            loader_state_snapshot_digest: &self.engine_loader_state_snapshot_digest,
            fee_policy_digest: &self.fee_policy_digest,
            identity: &self.identity,
            core_terms_root: &self.core_terms_root,
        })?;
        if self.intent_digest != expected_intent {
            return Err(WireError::DigestMismatch {
                field: "stored intent digest",
            });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoredAuthorizationChunkHeaderCandidateV0 {
    pub wire_version: u8,
    pub chunk_kind: u8,
    pub start_index: u8,
    pub row_count: u8,
}

impl StoredAuthorizationChunkHeaderCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; STORED_AUTHORIZATION_CHUNK_HEADER_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(STORED_AUTHORIZATION_CHUNK_HEADER_LEN);
        put_u8(&mut output, self.wire_version);
        put_u8(&mut output, self.chunk_kind);
        put_u8(&mut output, self.start_index);
        put_u8(&mut output, self.row_count);
        put_bytes(&mut output, &[0; 4]);
        Ok(output
            .try_into()
            .expect("stored chunk header has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, STORED_AUTHORIZATION_CHUNK_HEADER_LEN)?;
        let mut reader = Reader::new(data);
        let header = Self {
            wire_version: reader.read_u8()?,
            chunk_kind: reader.read_u8()?,
            start_index: reader.read_u8()?,
            row_count: reader.read_u8()?,
        };
        let reserved = reader.read_array::<4>()?;
        reader.finish()?;
        require_zero("stored chunk header reserved", &reserved)?;
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
        if !matches!(
            self.chunk_kind,
            STORED_AUTHORIZATION_CHUNK_KIND_TERM | STORED_AUTHORIZATION_CHUNK_KIND_CONSTRAINT
        ) {
            return Err(WireError::UnsupportedValue {
                field: "stored chunk kind",
                value: u64::from(self.chunk_kind),
            });
        }
        if self.row_count == 0 || usize::from(self.row_count) > MAX_STORED_AUTHORIZATION_CHUNK_ROWS
        {
            return Err(WireError::LimitExceeded {
                field: "stored chunk row count",
                maximum: MAX_STORED_AUTHORIZATION_CHUNK_ROWS,
                actual: usize::from(self.row_count),
            });
        }
        let end = usize::from(self.start_index)
            .checked_add(usize::from(self.row_count))
            .ok_or(WireError::LengthOverflow)?;
        if end > MAX_SETTLEMENT_CAPABILITIES {
            return Err(WireError::InvalidIndex {
                field: "stored chunk range",
                index: self.start_index,
                count: MAX_SETTLEMENT_CAPABILITIES as u8,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoredAuthorizationChunkRowsCandidateV0 {
    Terms(Vec<IntentCapabilityTermRowCandidateV0>),
    Constraints(Vec<CreditConstraintRowCandidateV0>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredAuthorizationChunkCandidateV0 {
    pub header: StoredAuthorizationChunkHeaderCandidateV0,
    pub rows: StoredAuthorizationChunkRowsCandidateV0,
}

impl StoredAuthorizationChunkCandidateV0 {
    pub fn encode_args(&self) -> WireResult<Vec<u8>> {
        self.validate()?;
        let row_len = self.row_len();
        let mut output = Vec::with_capacity(
            STORED_AUTHORIZATION_CHUNK_HEADER_LEN + usize::from(self.header.row_count) * row_len,
        );
        put_bytes(&mut output, &self.header.encode()?);
        match &self.rows {
            StoredAuthorizationChunkRowsCandidateV0::Terms(rows) => {
                for row in rows {
                    put_bytes(&mut output, &row.encode()?);
                }
            }
            StoredAuthorizationChunkRowsCandidateV0::Constraints(rows) => {
                for row in rows {
                    put_bytes(&mut output, &row.encode()?);
                }
            }
        }
        Ok(output)
    }

    pub fn decode_args_exact(data: &[u8]) -> WireResult<Self> {
        if data.len() < STORED_AUTHORIZATION_CHUNK_HEADER_LEN {
            return Err(WireError::InvalidLength {
                expected: STORED_AUTHORIZATION_CHUNK_HEADER_LEN,
                actual: data.len(),
            });
        }
        let header = StoredAuthorizationChunkHeaderCandidateV0::decode_exact(
            &data[..STORED_AUTHORIZATION_CHUNK_HEADER_LEN],
        )?;
        let row_len = match header.chunk_kind {
            STORED_AUTHORIZATION_CHUNK_KIND_TERM => INTENT_CAPABILITY_TERM_ROW_LEN,
            STORED_AUTHORIZATION_CHUNK_KIND_CONSTRAINT => CREDIT_CONSTRAINT_ROW_LEN,
            _ => return Err(WireError::InvalidDiscriminator),
        };
        let expected = STORED_AUTHORIZATION_CHUNK_HEADER_LEN
            .checked_add(
                usize::from(header.row_count)
                    .checked_mul(row_len)
                    .ok_or(WireError::LengthOverflow)?,
            )
            .ok_or(WireError::LengthOverflow)?;
        require_exact_length(data, expected)?;
        let payload = &data[STORED_AUTHORIZATION_CHUNK_HEADER_LEN..];
        let rows = match header.chunk_kind {
            STORED_AUTHORIZATION_CHUNK_KIND_TERM => {
                let mut rows = Vec::with_capacity(usize::from(header.row_count));
                for row in payload.chunks_exact(INTENT_CAPABILITY_TERM_ROW_LEN) {
                    rows.push(IntentCapabilityTermRowCandidateV0::decode_exact(row)?);
                }
                StoredAuthorizationChunkRowsCandidateV0::Terms(rows)
            }
            STORED_AUTHORIZATION_CHUNK_KIND_CONSTRAINT => {
                let mut rows = Vec::with_capacity(usize::from(header.row_count));
                for row in payload.chunks_exact(CREDIT_CONSTRAINT_ROW_LEN) {
                    rows.push(CreditConstraintRowCandidateV0::decode_exact(row)?);
                }
                StoredAuthorizationChunkRowsCandidateV0::Constraints(rows)
            }
            _ => return Err(WireError::InvalidDiscriminator),
        };
        let chunk = Self { header, rows };
        chunk.validate()?;
        Ok(chunk)
    }

    pub fn validate(&self) -> WireResult<()> {
        self.header.validate()?;
        let (kind, len) = match &self.rows {
            StoredAuthorizationChunkRowsCandidateV0::Terms(rows) => {
                (STORED_AUTHORIZATION_CHUNK_KIND_TERM, rows.len())
            }
            StoredAuthorizationChunkRowsCandidateV0::Constraints(rows) => {
                (STORED_AUTHORIZATION_CHUNK_KIND_CONSTRAINT, rows.len())
            }
        };
        if self.header.chunk_kind != kind || usize::from(self.header.row_count) != len {
            return Err(WireError::InvalidLength {
                expected: usize::from(self.header.row_count),
                actual: len,
            });
        }
        match &self.rows {
            StoredAuthorizationChunkRowsCandidateV0::Terms(rows) => {
                for (offset, row) in rows.iter().enumerate() {
                    if usize::from(row.intent_local_term_index)
                        != usize::from(self.header.start_index) + offset
                    {
                        return Err(WireError::NonCanonicalOrder {
                            field: "stored term chunk row indices",
                        });
                    }
                    row.encode()?;
                }
            }
            StoredAuthorizationChunkRowsCandidateV0::Constraints(rows) => {
                for (offset, row) in rows.iter().enumerate() {
                    if usize::from(row.constraint_index)
                        != usize::from(self.header.start_index) + offset
                    {
                        return Err(WireError::NonCanonicalOrder {
                            field: "stored constraint chunk row indices",
                        });
                    }
                    row.encode()?;
                }
            }
        }
        Ok(())
    }

    fn row_len(&self) -> usize {
        match self.header.chunk_kind {
            STORED_AUTHORIZATION_CHUNK_KIND_TERM => INTENT_CAPABILITY_TERM_ROW_LEN,
            STORED_AUTHORIZATION_CHUNK_KIND_CONSTRAINT => CREDIT_CONSTRAINT_ROW_LEN,
            _ => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreControlInstructionCandidateV0 {
    CaptureImmutableEngineRelease(EngineAdmissionPolicyCandidateV0),
    ApproveExactDelegate(ApproveExactDelegateArgsCandidateV0),
    InitializeStoredAuthorization(InitializeStoredAuthorizationArgsCandidateV0),
    WriteStoredAuthorizationChunk(StoredAuthorizationChunkCandidateV0),
    ActivateStoredAuthorization,
    ReplaceStoredAuthorization,
    CancelStoredAuthorization,
}

impl CoreControlInstructionCandidateV0 {
    pub fn encode(&self) -> WireResult<Vec<u8>> {
        let mut output = Vec::new();
        match self {
            Self::CaptureImmutableEngineRelease(policy) => {
                if policy.policy_kind != ENGINE_POLICY_IMMUTABLE {
                    return Err(WireError::UnsupportedValue {
                        field: "capture engine policy kind",
                        value: u64::from(policy.policy_kind),
                    });
                }
                put_bytes(
                    &mut output,
                    &CORE_CAPTURE_IMMUTABLE_ENGINE_RELEASE_DISCRIMINATOR,
                );
                put_bytes(&mut output, &policy.encode()?);
            }
            Self::ApproveExactDelegate(args) => {
                put_bytes(&mut output, &CORE_APPROVE_EXACT_DELEGATE_DISCRIMINATOR);
                put_bytes(&mut output, &args.encode()?);
            }
            Self::InitializeStoredAuthorization(args) => {
                put_bytes(
                    &mut output,
                    &CORE_INITIALIZE_STORED_AUTHORIZATION_DISCRIMINATOR,
                );
                put_bytes(&mut output, &args.encode()?);
            }
            Self::WriteStoredAuthorizationChunk(chunk) => {
                put_bytes(
                    &mut output,
                    &CORE_WRITE_STORED_AUTHORIZATION_CHUNK_DISCRIMINATOR,
                );
                put_bytes(&mut output, &chunk.encode_args()?);
            }
            Self::ActivateStoredAuthorization => put_bytes(
                &mut output,
                &CORE_ACTIVATE_STORED_AUTHORIZATION_DISCRIMINATOR,
            ),
            Self::ReplaceStoredAuthorization => put_bytes(
                &mut output,
                &CORE_REPLACE_STORED_AUTHORIZATION_DISCRIMINATOR,
            ),
            Self::CancelStoredAuthorization => {
                put_bytes(&mut output, &CORE_CANCEL_STORED_AUTHORIZATION_DISCRIMINATOR)
            }
        }
        Ok(output)
    }
}

pub fn decode_core_control_instruction_exact(
    data: &[u8],
) -> WireResult<CoreControlInstructionCandidateV0> {
    if data.len() < 8 {
        return Err(WireError::InvalidLength {
            expected: 8,
            actual: data.len(),
        });
    }
    let discriminator: [u8; 8] = data[..8]
        .try_into()
        .expect("the preceding length check guarantees eight bytes");
    let args = &data[8..];
    match discriminator {
        CORE_CAPTURE_IMMUTABLE_ENGINE_RELEASE_DISCRIMINATOR => {
            require_exact_length(args, ENGINE_ADMISSION_POLICY_LEN)?;
            let policy = EngineAdmissionPolicyCandidateV0::decode_exact(args)?;
            if policy.policy_kind != ENGINE_POLICY_IMMUTABLE {
                return Err(WireError::UnsupportedValue {
                    field: "capture engine policy kind",
                    value: u64::from(policy.policy_kind),
                });
            }
            Ok(CoreControlInstructionCandidateV0::CaptureImmutableEngineRelease(policy))
        }
        CORE_APPROVE_EXACT_DELEGATE_DISCRIMINATOR => {
            Ok(CoreControlInstructionCandidateV0::ApproveExactDelegate(
                ApproveExactDelegateArgsCandidateV0::decode_exact(args)?,
            ))
        }
        CORE_INITIALIZE_STORED_AUTHORIZATION_DISCRIMINATOR => Ok(
            CoreControlInstructionCandidateV0::InitializeStoredAuthorization(
                InitializeStoredAuthorizationArgsCandidateV0::decode_exact(args)?,
            ),
        ),
        CORE_WRITE_STORED_AUTHORIZATION_CHUNK_DISCRIMINATOR => Ok(
            CoreControlInstructionCandidateV0::WriteStoredAuthorizationChunk(
                StoredAuthorizationChunkCandidateV0::decode_args_exact(args)?,
            ),
        ),
        CORE_ACTIVATE_STORED_AUTHORIZATION_DISCRIMINATOR => {
            require_exact_length(args, 0)?;
            Ok(CoreControlInstructionCandidateV0::ActivateStoredAuthorization)
        }
        CORE_REPLACE_STORED_AUTHORIZATION_DISCRIMINATOR => {
            require_exact_length(args, 0)?;
            Ok(CoreControlInstructionCandidateV0::ReplaceStoredAuthorization)
        }
        CORE_CANCEL_STORED_AUTHORIZATION_DISCRIMINATOR => {
            require_exact_length(args, 0)?;
            Ok(CoreControlInstructionCandidateV0::CancelStoredAuthorization)
        }
        _ => Err(WireError::InvalidDiscriminator),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        compute_intent_capability_terms_root, compute_intent_credit_constraints_root,
        AUTHORITY_INTENT_FUNDED, FEE_CLASS_GROSS_DEBIT_RATE, RIGHT_DEBIT,
        SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT,
    };

    fn term(index: u8) -> IntentCapabilityTermRowCandidateV0 {
        IntentCapabilityTermRowCandidateV0 {
            intent_local_term_index: index,
            authority_class: AUTHORITY_INTENT_FUNDED,
            fee_class: FEE_CLASS_GROSS_DEBIT_RATE,
            flags: SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT,
            rights_bits: RIGHT_DEBIT,
            endpoint_key: [index.saturating_add(1); 32],
            asset_binding_digest: [2; 32],
            required_domain_descriptor_digest_or_zero: [0; 32],
            maximum_engine_debit: 10,
            maximum_total_debit: 10,
            minimum_credit: 0,
            maximum_protocol_fee: 0,
        }
    }

    fn init() -> InitializeStoredAuthorizationArgsCandidateV0 {
        let terms = [term(0)];
        let intent_capability_terms_root = compute_intent_capability_terms_root(&terms).unwrap();
        let credit_constraints_root = compute_intent_credit_constraints_root(&[]).unwrap();
        let maximum_successful_fills = 2;
        let core_terms_root = compute_intent_core_terms_root(IntentCoreTermsDigestInputs {
            maximum_successful_fills,
            capability_terms_root: &intent_capability_terms_root,
            credit_constraints_root: &credit_constraints_root,
        })
        .unwrap();
        let identity = InlineIntentIdentityRowCandidateV0 {
            actor: [3; 32],
            engine_terms_commitment: [4; 32],
            authorization_nonce: 5,
            expires_at_slot_exclusive: 6,
        };
        let market_binding_digest = [7; 32];
        let engine_loader_state_snapshot_digest = [8; 32];
        let fee_policy_digest = [9; 32];
        let intent_digest = compute_intent_digest(IntentDigestInputs {
            core_program: &DISPOSABLE_CORE_PROGRAM_ID.to_bytes(),
            market_binding_digest: &market_binding_digest,
            loader_state_snapshot_digest: &engine_loader_state_snapshot_digest,
            fee_policy_digest: &fee_policy_digest,
            identity: &identity,
            core_terms_root: &core_terms_root,
        })
        .unwrap();
        InitializeStoredAuthorizationArgsCandidateV0 {
            wire_version: WIRE_VERSION,
            term_count: 1,
            constraint_count: 0,
            flags: 0,
            maximum_successful_fills,
            identity,
            market_binding_digest,
            engine_loader_state_snapshot_digest,
            fee_policy_digest,
            intent_capability_terms_root,
            credit_constraints_root,
            core_terms_root,
            intent_digest,
        }
    }

    #[test]
    fn frozen_discriminators_and_fixed_argument_lengths_are_exact() {
        assert_eq!(
            CORE_CAPTURE_IMMUTABLE_ENGINE_RELEASE_DISCRIMINATOR,
            hex("e3646e8c56a7c312")
        );
        assert_eq!(
            CORE_APPROVE_EXACT_DELEGATE_DISCRIMINATOR,
            hex("04cf33c35d503375")
        );
        assert_eq!(
            CORE_INITIALIZE_STORED_AUTHORIZATION_DISCRIMINATOR,
            hex("76987db8b7400e4e")
        );
        assert_eq!(
            CORE_WRITE_STORED_AUTHORIZATION_CHUNK_DISCRIMINATOR,
            hex("bb97761e70f00ad6")
        );
        assert_eq!(
            CORE_ACTIVATE_STORED_AUTHORIZATION_DISCRIMINATOR,
            hex("914d2e6337527a33")
        );
        assert_eq!(
            CORE_REPLACE_STORED_AUTHORIZATION_DISCRIMINATOR,
            hex("5f1f92773ed93c7d")
        );
        assert_eq!(
            CORE_CANCEL_STORED_AUTHORIZATION_DISCRIMINATOR,
            hex("5b1eda991f5246e7")
        );
        assert_eq!(
            init().encode().unwrap().len(),
            INITIALIZE_STORED_AUTHORIZATION_ARGS_LEN
        );
    }

    #[test]
    fn stored_header_accepts_partial_draft_and_complete_active_only() {
        let draft = StoredAuthorizationHeaderCandidateV0 {
            wire_version: WIRE_VERSION,
            lifecycle: AUTHORIZATION_LIFECYCLE_DRAFT,
            bump: 7,
            term_count: 3,
            constraint_count: 2,
            fee_state_count: 0,
            flags: 0,
            reserved: 0,
            term_written_bitmap: 0b001,
            constraint_written_bitmap: 0,
            fill_sequence: 0,
        };
        let encoded = draft.encode().unwrap();
        assert_eq!(encoded.len(), STORED_AUTHORIZATION_HEADER_LEN);
        assert_eq!(
            StoredAuthorizationHeaderCandidateV0::decode_exact(&encoded),
            Ok(draft)
        );

        let mut active = draft;
        active.lifecycle = AUTHORIZATION_LIFECYCLE_ACTIVE;
        assert!(active.encode().is_err());
        active.term_written_bitmap = 0b111;
        active.constraint_written_bitmap = 0b11;
        assert!(active.encode().is_ok());

        let mut out_of_range = draft;
        out_of_range.term_written_bitmap = 1 << 3;
        assert!(out_of_range.encode().is_err());
        let mut invalid_lifecycle = draft;
        invalid_lifecycle.lifecycle = AUTHORIZATION_LIFECYCLE_CANCELLED + 1;
        assert!(invalid_lifecycle.encode().is_err());
        let mut nonzero_reserved = draft;
        nonzero_reserved.reserved = 1;
        assert!(nonzero_reserved.encode().is_err());
    }

    #[test]
    fn every_control_route_round_trips_and_rejects_trailing_bytes() {
        let immutable = EngineAdmissionPolicyCandidateV0 {
            policy_kind: ENGINE_POLICY_IMMUTABLE,
            engine_program: [1; 32],
            loader_program: crate::LOADER_V3_PROGRAM_ID.to_bytes(),
            program_data_or_zero: [3; 32],
            expected_controller_or_zero: [0; 32],
            captured_programdata_slot_or_zero: 4,
        };
        let routes = [
            CoreControlInstructionCandidateV0::CaptureImmutableEngineRelease(immutable),
            CoreControlInstructionCandidateV0::ApproveExactDelegate(
                ApproveExactDelegateArgsCandidateV0 {
                    intent_digest: [5; 32],
                    amount: 6,
                },
            ),
            CoreControlInstructionCandidateV0::InitializeStoredAuthorization(init()),
            CoreControlInstructionCandidateV0::WriteStoredAuthorizationChunk(
                StoredAuthorizationChunkCandidateV0 {
                    header: StoredAuthorizationChunkHeaderCandidateV0 {
                        wire_version: WIRE_VERSION,
                        chunk_kind: STORED_AUTHORIZATION_CHUNK_KIND_TERM,
                        start_index: 0,
                        row_count: 1,
                    },
                    rows: StoredAuthorizationChunkRowsCandidateV0::Terms(alloc::vec![term(0)]),
                },
            ),
            CoreControlInstructionCandidateV0::ActivateStoredAuthorization,
            CoreControlInstructionCandidateV0::ReplaceStoredAuthorization,
            CoreControlInstructionCandidateV0::CancelStoredAuthorization,
        ];
        for route in routes {
            let encoded = route.encode().unwrap();
            assert_eq!(decode_core_control_instruction_exact(&encoded), Ok(route));
            let mut trailing = encoded;
            trailing.push(0);
            assert!(decode_core_control_instruction_exact(&trailing).is_err());
        }
    }

    #[test]
    fn chunk_header_and_rows_fail_closed() {
        let chunk = StoredAuthorizationChunkCandidateV0 {
            header: StoredAuthorizationChunkHeaderCandidateV0 {
                wire_version: WIRE_VERSION,
                chunk_kind: STORED_AUTHORIZATION_CHUNK_KIND_TERM,
                start_index: 0,
                row_count: 1,
            },
            rows: StoredAuthorizationChunkRowsCandidateV0::Terms(alloc::vec![term(0)]),
        };
        let encoded = chunk.encode_args().unwrap();
        assert_eq!(
            StoredAuthorizationChunkCandidateV0::decode_args_exact(&encoded),
            Ok(chunk)
        );

        let mut reserved = encoded.clone();
        reserved[7] = 1;
        assert!(StoredAuthorizationChunkCandidateV0::decode_args_exact(&reserved).is_err());
        let mut kind = encoded.clone();
        kind[1] = 2;
        assert!(StoredAuthorizationChunkCandidateV0::decode_args_exact(&kind).is_err());
        let mut zero = encoded.clone();
        zero[3] = 0;
        assert!(StoredAuthorizationChunkCandidateV0::decode_args_exact(&zero).is_err());
        let mut too_many = encoded.clone();
        too_many[3] = 5;
        assert!(StoredAuthorizationChunkCandidateV0::decode_args_exact(&too_many).is_err());
        let mut range = encoded;
        range[2] = MAX_SETTLEMENT_CAPABILITIES as u8;
        assert!(StoredAuthorizationChunkCandidateV0::decode_args_exact(&range).is_err());
    }

    #[test]
    fn init_recomputes_root_chain_and_approval_is_nonzero_exact40() {
        let value = init();
        let encoded = value.encode().unwrap();
        assert_eq!(
            InitializeStoredAuthorizationArgsCandidateV0::decode_exact(&encoded),
            Ok(value)
        );
        let mut changed = value;
        changed.intent_digest[0] ^= 1;
        assert!(changed.encode().is_err());

        let approval = ApproveExactDelegateArgsCandidateV0 {
            intent_digest: [1; 32],
            amount: 2,
        };
        assert_eq!(
            approval.encode().unwrap().len(),
            APPROVE_EXACT_DELEGATE_ARGS_LEN
        );
        let mut zero = approval;
        zero.amount = 0;
        assert!(zero.encode().is_err());
    }

    fn hex(value: &str) -> [u8; 8] {
        let mut output = [0_u8; 8];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            output[index] = (nibble(pair[0]) << 4) | nibble(pair[1]);
        }
        output
    }

    fn nibble(value: u8) -> u8 {
        match value {
            b'0'..=b'9' => value - b'0',
            b'a'..=b'f' => value - b'a' + 10,
            _ => unreachable!(),
        }
    }
}
