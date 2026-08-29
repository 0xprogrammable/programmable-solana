use alloc::vec::Vec;

use solana_pubkey::Pubkey;

use crate::codec::{
    checked_encoded_length, put_bytes, put_u16, put_u64, put_u8, require_exact_length,
    require_zero, Reader,
};
use crate::hashes::{
    compute_payload_digest, hash_private, LABEL_CALLBACK_SEED, LABEL_ENGINE_REQUEST,
};
use crate::rows::{
    require_optional_index, require_present_index, InlineIntentIdentityRowCandidateV0,
    ENGINE_CONTEXT_RIGHTS_MASK, INLINE_INTENT_IDENTITY_ROW_LEN,
};
use crate::{
    compute_asset_binding_digest, compute_fee_policy_digest, compute_intent_set_digest,
    AssetBindingRowCandidateV0, IntentSetRowCandidateV0, WireError, WireResult,
    CORE_EXPERIMENTAL_MAJOR, DISPOSABLE_CORE_PROGRAM_ID, DISPOSABLE_ENGINE_PROGRAM_ID,
    ENGINE_REQUEST_MAGIC, ENGINE_TRANSITION_DISCRIMINATOR, MAX_ASSETS, MAX_CONTEXT_ROWS,
    MAX_DOMAINS, MAX_ENGINE_MOVES, MAX_ENGINE_REQUEST_LEN, MAX_INTENTS, MAX_OPAQUE_CAPABILITIES,
    MAX_OPAQUE_PAYLOAD_LEN, MAX_SETTLEMENT_CAPABILITIES, PHASE_TRANSITION, WIRE_VERSION,
};

pub const ENGINE_REQUEST_HEADER_LEN: usize = 312;
pub const ENGINE_ASSET_ROW_LEN: usize = 100;
pub const ENGINE_DOMAIN_ROW_LEN: usize = 112;
pub const ENGINE_INTENT_ROW_LEN: usize = 120;
pub const ENGINE_FEE_POLICY_ROW_LEN: usize = 32;
pub const ENGINE_CONTEXT_ROW_LEN: usize = 88;

pub const ROUNDING_FLOOR: u8 = 0;
pub const ROUNDING_CEILING: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineRequestHeaderCandidateV0 {
    pub magic: [u8; 8],
    pub wire_version: u8,
    pub phase: u8,
    pub settlement_capability_count: u8,
    pub opaque_capability_count: u8,
    pub intent_count: u8,
    pub domain_count: u8,
    pub asset_count: u8,
    pub context_row_count: u8,
    pub payload_len: u16,
    pub maximum_engine_moves: u8,
    pub market_binding_digest: [u8; 32],
    pub engine_instance_id: [u8; 32],
    pub engine_interface_id: [u8; 32],
    pub intent_set_digest: [u8; 32],
    pub domain_set_digest: [u8; 32],
    pub protected_execution_root: [u8; 32],
    pub opaque_capability_root: [u8; 32],
    pub engine_loader_state_snapshot_digest: [u8; 32],
    pub fee_policy_digest: [u8; 32],
}

impl EngineRequestHeaderCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; ENGINE_REQUEST_HEADER_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(ENGINE_REQUEST_HEADER_LEN);
        put_bytes(&mut output, &self.magic);
        put_u8(&mut output, self.wire_version);
        put_u8(&mut output, self.phase);
        put_u8(&mut output, self.settlement_capability_count);
        put_u8(&mut output, self.opaque_capability_count);
        put_u8(&mut output, self.intent_count);
        put_u8(&mut output, self.domain_count);
        put_u8(&mut output, self.asset_count);
        put_u8(&mut output, self.context_row_count);
        put_u16(&mut output, self.payload_len);
        put_u8(&mut output, self.maximum_engine_moves);
        put_bytes(&mut output, &[0; 5]);
        put_bytes(&mut output, &self.market_binding_digest);
        put_bytes(&mut output, &self.engine_instance_id);
        put_bytes(&mut output, &self.engine_interface_id);
        put_bytes(&mut output, &self.intent_set_digest);
        put_bytes(&mut output, &self.domain_set_digest);
        put_bytes(&mut output, &self.protected_execution_root);
        put_bytes(&mut output, &self.opaque_capability_root);
        put_bytes(&mut output, &self.engine_loader_state_snapshot_digest);
        put_bytes(&mut output, &self.fee_policy_digest);
        Ok(output
            .try_into()
            .expect("engine request header has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, ENGINE_REQUEST_HEADER_LEN)?;
        let mut reader = Reader::new(data);
        let magic = reader.read_array()?;
        let wire_version = reader.read_u8()?;
        let phase = reader.read_u8()?;
        let settlement_capability_count = reader.read_u8()?;
        let opaque_capability_count = reader.read_u8()?;
        let intent_count = reader.read_u8()?;
        let domain_count = reader.read_u8()?;
        let asset_count = reader.read_u8()?;
        let context_row_count = reader.read_u8()?;
        let payload_len = reader.read_u16()?;
        let maximum_engine_moves = reader.read_u8()?;
        let reserved = reader.read_array::<5>()?;
        require_zero("engine request header reserved", &reserved)?;
        let header = Self {
            magic,
            wire_version,
            phase,
            settlement_capability_count,
            opaque_capability_count,
            intent_count,
            domain_count,
            asset_count,
            context_row_count,
            payload_len,
            maximum_engine_moves,
            market_binding_digest: reader.read_array()?,
            engine_instance_id: reader.read_array()?,
            engine_interface_id: reader.read_array()?,
            intent_set_digest: reader.read_array()?,
            domain_set_digest: reader.read_array()?,
            protected_execution_root: reader.read_array()?,
            opaque_capability_root: reader.read_array()?,
            engine_loader_state_snapshot_digest: reader.read_array()?,
            fee_policy_digest: reader.read_array()?,
        };
        reader.finish()?;
        header.validate()?;
        Ok(header)
    }

    pub fn validate(&self) -> WireResult<()> {
        if self.magic != ENGINE_REQUEST_MAGIC {
            return Err(WireError::InvalidMagic);
        }
        if self.wire_version != WIRE_VERSION {
            return Err(WireError::UnsupportedVersion {
                expected: WIRE_VERSION,
                actual: self.wire_version,
            });
        }
        if self.phase != PHASE_TRANSITION {
            return Err(WireError::UnsupportedValue {
                field: "engine request phase",
                value: u64::from(self.phase),
            });
        }
        require_limit(
            "engine request settlement capability count",
            self.settlement_capability_count,
            MAX_SETTLEMENT_CAPABILITIES,
        )?;
        require_limit(
            "engine request opaque capability count",
            self.opaque_capability_count,
            MAX_OPAQUE_CAPABILITIES,
        )?;
        require_limit(
            "engine request intent count",
            self.intent_count,
            MAX_INTENTS,
        )?;
        require_limit(
            "engine request domain count",
            self.domain_count,
            MAX_DOMAINS,
        )?;
        require_limit("engine request asset count", self.asset_count, MAX_ASSETS)?;
        require_limit(
            "engine request context row count",
            self.context_row_count,
            MAX_CONTEXT_ROWS,
        )?;
        require_limit(
            "engine request maximum moves",
            self.maximum_engine_moves,
            MAX_ENGINE_MOVES,
        )?;
        if usize::from(self.payload_len) > MAX_OPAQUE_PAYLOAD_LEN {
            return Err(WireError::LimitExceeded {
                field: "engine request payload length",
                maximum: MAX_OPAQUE_PAYLOAD_LEN,
                actual: usize::from(self.payload_len),
            });
        }
        for (field, value) in [
            ("engine request market binding", self.market_binding_digest),
            ("engine request instance", self.engine_instance_id),
            ("engine request interface", self.engine_interface_id),
            ("engine request intent set", self.intent_set_digest),
            ("engine request domain set", self.domain_set_digest),
            (
                "engine request protected execution",
                self.protected_execution_root,
            ),
            (
                "engine request opaque capability root",
                self.opaque_capability_root,
            ),
            (
                "engine request loader-state snapshot",
                self.engine_loader_state_snapshot_digest,
            ),
            ("engine request fee policy", self.fee_policy_digest),
        ] {
            if value == [0; 32] {
                return Err(WireError::UnsupportedValue { field, value: 0 });
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineAssetRowCandidateV0 {
    pub asset_index: u8,
    pub asset_flags: u8,
    pub decimals: u8,
    pub reserved: u8,
    pub asset_identity: [u8; 32],
    pub asset_program: [u8; 32],
    pub settlement_profile_digest: [u8; 32],
}

impl EngineAssetRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; ENGINE_ASSET_ROW_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(ENGINE_ASSET_ROW_LEN);
        put_u8(&mut output, self.asset_index);
        put_u8(&mut output, self.asset_flags);
        put_u8(&mut output, self.decimals);
        put_u8(&mut output, self.reserved);
        put_bytes(&mut output, &self.asset_identity);
        put_bytes(&mut output, &self.asset_program);
        put_bytes(&mut output, &self.settlement_profile_digest);
        Ok(output
            .try_into()
            .expect("engine asset row has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, ENGINE_ASSET_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let row = Self {
            asset_index: reader.read_u8()?,
            asset_flags: reader.read_u8()?,
            decimals: reader.read_u8()?,
            reserved: reader.read_u8()?,
            asset_identity: reader.read_array()?,
            asset_program: reader.read_array()?,
            settlement_profile_digest: reader.read_array()?,
        };
        reader.finish()?;
        row.validate()?;
        Ok(row)
    }

    fn validate(&self) -> WireResult<()> {
        if self.asset_flags != 0 {
            return Err(WireError::UnknownFlags {
                field: "engine asset flags",
                value: u64::from(self.asset_flags),
            });
        }
        if self.reserved != 0 {
            return Err(WireError::NonZeroReserved {
                field: "engine asset row reserved",
            });
        }
        if self.asset_identity == [0; 32]
            || self.asset_program == [0; 32]
            || self.settlement_profile_digest == [0; 32]
        {
            return Err(WireError::UnsupportedValue {
                field: "engine asset identity",
                value: 0,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineDomainRowCandidateV0 {
    pub domain_index: u8,
    pub domain_descriptor: [u8; 32],
    pub domain_revision: u64,
    pub admission_digest: [u8; 32],
    pub accounting_profile_digest: [u8; 32],
}

impl EngineDomainRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; ENGINE_DOMAIN_ROW_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(ENGINE_DOMAIN_ROW_LEN);
        put_u8(&mut output, self.domain_index);
        put_bytes(&mut output, &[0; 7]);
        put_bytes(&mut output, &self.domain_descriptor);
        put_u64(&mut output, self.domain_revision);
        put_bytes(&mut output, &self.admission_digest);
        put_bytes(&mut output, &self.accounting_profile_digest);
        Ok(output
            .try_into()
            .expect("engine domain row has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, ENGINE_DOMAIN_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let domain_index = reader.read_u8()?;
        let reserved = reader.read_array::<7>()?;
        require_zero("engine domain row reserved", &reserved)?;
        let row = Self {
            domain_index,
            domain_descriptor: reader.read_array()?,
            domain_revision: reader.read_u64()?,
            admission_digest: reader.read_array()?,
            accounting_profile_digest: reader.read_array()?,
        };
        reader.finish()?;
        row.validate()?;
        Ok(row)
    }

    fn validate(&self) -> WireResult<()> {
        if self.domain_descriptor == [0; 32]
            || self.admission_digest == [0; 32]
            || self.accounting_profile_digest == [0; 32]
        {
            return Err(WireError::UnsupportedValue {
                field: "engine domain identity",
                value: 0,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineIntentRowCandidateV0 {
    pub authorization_slot: u8,
    pub identity: InlineIntentIdentityRowCandidateV0,
    pub intent_digest: [u8; 32],
}

impl EngineIntentRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; ENGINE_INTENT_ROW_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(ENGINE_INTENT_ROW_LEN);
        put_u8(&mut output, self.authorization_slot);
        put_bytes(&mut output, &[0; 7]);
        put_bytes(&mut output, &self.identity.encode()?);
        put_bytes(&mut output, &self.intent_digest);
        Ok(output
            .try_into()
            .expect("engine intent row has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, ENGINE_INTENT_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let authorization_slot = reader.read_u8()?;
        let reserved = reader.read_array::<7>()?;
        require_zero("engine intent row reserved", &reserved)?;
        let identity = InlineIntentIdentityRowCandidateV0::decode_exact(
            &reader.read_vec(INLINE_INTENT_IDENTITY_ROW_LEN)?,
        )?;
        let intent_digest = reader.read_array()?;
        reader.finish()?;
        let row = Self {
            authorization_slot,
            identity,
            intent_digest,
        };
        row.validate()?;
        Ok(row)
    }

    fn validate(&self) -> WireResult<()> {
        self.identity.encode()?;
        if self.intent_digest == [0; 32] {
            return Err(WireError::UnsupportedValue {
                field: "engine intent digest",
                value: 0,
            });
        }
        Ok(())
    }

    fn validate_index(&self, intent_count: u8) -> WireResult<()> {
        require_present_index(
            "engine intent authorization slot",
            self.authorization_slot,
            intent_count,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeePolicyRowCandidateV0 {
    pub wire_version: u8,
    pub rounding_mode: u8,
    pub flags: u8,
    pub revision: u64,
    pub rate_numerator: u64,
    pub nonzero_denominator: u64,
}

impl FeePolicyRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; ENGINE_FEE_POLICY_ROW_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(ENGINE_FEE_POLICY_ROW_LEN);
        put_u8(&mut output, self.wire_version);
        put_u8(&mut output, self.rounding_mode);
        put_u8(&mut output, self.flags);
        put_bytes(&mut output, &[0; 5]);
        put_u64(&mut output, self.revision);
        put_u64(&mut output, self.rate_numerator);
        put_u64(&mut output, self.nonzero_denominator);
        Ok(output
            .try_into()
            .expect("engine fee policy row has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, ENGINE_FEE_POLICY_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let row = Self {
            wire_version: reader.read_u8()?,
            rounding_mode: reader.read_u8()?,
            flags: reader.read_u8()?,
            revision: {
                let reserved = reader.read_array::<5>()?;
                require_zero("fee policy reserved", &reserved)?;
                reader.read_u64()?
            },
            rate_numerator: reader.read_u64()?,
            nonzero_denominator: reader.read_u64()?,
        };
        reader.finish()?;
        row.validate()?;
        Ok(row)
    }

    pub fn validate(&self) -> WireResult<()> {
        if self.wire_version != WIRE_VERSION {
            return Err(WireError::UnsupportedVersion {
                expected: WIRE_VERSION,
                actual: self.wire_version,
            });
        }
        if !matches!(self.rounding_mode, ROUNDING_FLOOR | ROUNDING_CEILING) {
            return Err(WireError::UnsupportedValue {
                field: "fee rounding mode",
                value: u64::from(self.rounding_mode),
            });
        }
        if self.flags != 0 {
            return Err(WireError::UnknownFlags {
                field: "fee policy flags",
                value: u64::from(self.flags),
            });
        }
        if self.rate_numerator == 0
            || self.nonzero_denominator == 0
            || self.rate_numerator > self.nonzero_denominator
        {
            return Err(WireError::UnsupportedValue {
                field: "fee rate or denominator",
                value: self.rate_numerator,
            });
        }
        Ok(())
    }
}

/// The request carries the canonical protocol fee-policy row unchanged.
pub type EngineFeePolicyRowCandidateV0 = FeePolicyRowCandidateV0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineContextRowCandidateV0 {
    pub settlement_capability_index: u8,
    pub asset_index: u8,
    pub domain_index_or_none: u8,
    pub authorization_slot_or_none: u8,
    pub rights_bits: u16,
    pub fee_class: u8,
    pub context_flags: u8,
    pub endpoint_key: [u8; 32],
    pub observed_before: u64,
    pub accounted_before_or_zero: u64,
    pub remaining_maximum_engine_debit: u64,
    pub remaining_maximum_total_debit: u64,
    pub remaining_minimum_credit: u64,
    pub remaining_maximum_protocol_fee: u64,
}

impl EngineContextRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; ENGINE_CONTEXT_ROW_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(ENGINE_CONTEXT_ROW_LEN);
        put_u8(&mut output, self.settlement_capability_index);
        put_u8(&mut output, self.asset_index);
        put_u8(&mut output, self.domain_index_or_none);
        put_u8(&mut output, self.authorization_slot_or_none);
        crate::codec::put_u16(&mut output, self.rights_bits);
        put_u8(&mut output, self.fee_class);
        put_u8(&mut output, self.context_flags);
        put_bytes(&mut output, &self.endpoint_key);
        put_u64(&mut output, self.observed_before);
        put_u64(&mut output, self.accounted_before_or_zero);
        put_u64(&mut output, self.remaining_maximum_engine_debit);
        put_u64(&mut output, self.remaining_maximum_total_debit);
        put_u64(&mut output, self.remaining_minimum_credit);
        put_u64(&mut output, self.remaining_maximum_protocol_fee);
        Ok(output
            .try_into()
            .expect("engine context row has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, ENGINE_CONTEXT_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let row = Self {
            settlement_capability_index: reader.read_u8()?,
            asset_index: reader.read_u8()?,
            domain_index_or_none: reader.read_u8()?,
            authorization_slot_or_none: reader.read_u8()?,
            rights_bits: reader.read_u16()?,
            fee_class: reader.read_u8()?,
            context_flags: reader.read_u8()?,
            endpoint_key: reader.read_array()?,
            observed_before: reader.read_u64()?,
            accounted_before_or_zero: reader.read_u64()?,
            remaining_maximum_engine_debit: reader.read_u64()?,
            remaining_maximum_total_debit: reader.read_u64()?,
            remaining_minimum_credit: reader.read_u64()?,
            remaining_maximum_protocol_fee: reader.read_u64()?,
        };
        reader.finish()?;
        row.validate()?;
        Ok(row)
    }

    fn validate(&self) -> WireResult<()> {
        if self.rights_bits == 0 || self.rights_bits & !ENGINE_CONTEXT_RIGHTS_MASK != 0 {
            return Err(WireError::UnknownFlags {
                field: "engine context rights",
                value: u64::from(self.rights_bits),
            });
        }
        if self.context_flags != 0 {
            return Err(WireError::UnknownFlags {
                field: "engine context flags",
                value: u64::from(self.context_flags),
            });
        }
        if self.fee_class > crate::FEE_CLASS_GROSS_DEBIT_RATE {
            return Err(WireError::UnsupportedValue {
                field: "engine context fee class",
                value: u64::from(self.fee_class),
            });
        }
        let objective_intent_debit = self.rights_bits == crate::RIGHT_DEBIT;
        if objective_intent_debit != (self.fee_class == crate::FEE_CLASS_GROSS_DEBIT_RATE) {
            return Err(WireError::UnsupportedValue {
                field: "engine context fee-role shape",
                value: u64::from(self.fee_class),
            });
        }
        if self.endpoint_key == [0; 32] {
            return Err(WireError::UnsupportedValue {
                field: "engine context endpoint",
                value: 0,
            });
        }
        match self.rights_bits {
            crate::RIGHT_DEBIT => {
                if self.authorization_slot_or_none == crate::NONE_INDEX
                    || self.accounted_before_or_zero != 0
                    || self.remaining_maximum_total_debit < self.remaining_maximum_engine_debit
                    || self.remaining_minimum_credit != 0
                    || self.remaining_maximum_protocol_fee > self.remaining_maximum_total_debit
                {
                    return Err(WireError::UnsupportedValue {
                        field: "engine intent-debit context shape",
                        value: self.remaining_maximum_engine_debit,
                    });
                }
            }
            rights if rights == (crate::RIGHT_DOMAIN_ACCOUNTED | crate::RIGHT_DEBIT) => {
                if self.domain_index_or_none == crate::NONE_INDEX
                    || self.authorization_slot_or_none != crate::NONE_INDEX
                    || self.remaining_maximum_engine_debit == 0
                    || self.remaining_maximum_total_debit != self.remaining_maximum_engine_debit
                    || self.remaining_minimum_credit != 0
                    || self.remaining_maximum_protocol_fee != 0
                {
                    return Err(WireError::UnsupportedValue {
                        field: "engine domain-debit context shape",
                        value: self.remaining_maximum_engine_debit,
                    });
                }
            }
            rights if rights == (crate::RIGHT_DOMAIN_ACCOUNTED | crate::RIGHT_CREDIT) => {
                if self.domain_index_or_none == crate::NONE_INDEX
                    || self.authorization_slot_or_none != crate::NONE_INDEX
                    || self.remaining_maximum_engine_debit != 0
                    || self.remaining_maximum_total_debit != 0
                    || self.remaining_minimum_credit != 0
                    || self.remaining_maximum_protocol_fee != 0
                {
                    return Err(WireError::UnsupportedValue {
                        field: "engine domain-credit context shape",
                        value: self.remaining_maximum_engine_debit,
                    });
                }
            }
            rights if rights == (crate::RIGHT_EXACT_EXTERNAL_RECIPIENT | crate::RIGHT_CREDIT) => {
                if self.authorization_slot_or_none == crate::NONE_INDEX
                    || self.accounted_before_or_zero != 0
                    || self.remaining_maximum_engine_debit != 0
                    || self.remaining_maximum_total_debit != 0
                    || self.remaining_maximum_protocol_fee != 0
                {
                    return Err(WireError::UnsupportedValue {
                        field: "engine external-credit context shape",
                        value: self.remaining_maximum_engine_debit,
                    });
                }
            }
            _ => {
                return Err(WireError::UnsupportedValue {
                    field: "engine context rights shape",
                    value: u64::from(self.rights_bits),
                });
            }
        }
        Ok(())
    }

    fn validate_indices(&self, header: &EngineRequestHeaderCandidateV0) -> WireResult<()> {
        require_present_index(
            "engine context settlement capability index",
            self.settlement_capability_index,
            header.settlement_capability_count,
        )?;
        require_present_index(
            "engine context asset index",
            self.asset_index,
            header.asset_count,
        )?;
        require_optional_index(
            "engine context domain index",
            self.domain_index_or_none,
            header.domain_count,
        )?;
        require_optional_index(
            "engine context authorization slot",
            self.authorization_slot_or_none,
            header.intent_count,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineRequestCandidateV0 {
    pub header: EngineRequestHeaderCandidateV0,
    pub assets: Vec<EngineAssetRowCandidateV0>,
    pub domains: Vec<EngineDomainRowCandidateV0>,
    pub intents: Vec<EngineIntentRowCandidateV0>,
    pub fee_policy: EngineFeePolicyRowCandidateV0,
    pub contexts: Vec<EngineContextRowCandidateV0>,
    pub payload: Vec<u8>,
}

impl EngineRequestCandidateV0 {
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub fn encode(&self) -> WireResult<Vec<u8>> {
        self.validate()?;
        let expected = engine_request_encoded_length(&self.header)?;
        let mut output = Vec::with_capacity(expected);
        put_bytes(&mut output, &ENGINE_TRANSITION_DISCRIMINATOR);
        put_bytes(&mut output, &self.header.encode()?);
        for row in &self.assets {
            put_bytes(&mut output, &row.encode()?);
        }
        for row in &self.domains {
            put_bytes(&mut output, &row.encode()?);
        }
        for row in &self.intents {
            put_bytes(&mut output, &row.encode()?);
        }
        put_bytes(&mut output, &self.fee_policy.encode()?);
        for row in &self.contexts {
            put_bytes(&mut output, &row.encode()?);
        }
        put_bytes(&mut output, &self.payload);
        debug_assert_eq!(output.len(), expected);
        Ok(output)
    }

    pub fn digest(&self) -> WireResult<[u8; 32]> {
        self.encode_and_digest().map(|(_, digest)| digest)
    }

    /// Encodes and hashes the same validated canonical request without
    /// materializing the full encoding twice in callers that need both for a
    /// CPI. The returned digest is exactly `Self::digest()`.
    #[inline(never)]
    pub fn encode_and_digest(&self) -> WireResult<(Vec<u8>, [u8; 32])> {
        let encoded = self.encode()?;
        let digest = hash_canonical_engine_request_data(&encoded)?;
        Ok((encoded, digest))
    }

    /// Encodes and hashes a validated request, then derives the callback seed
    /// from that same validation pass. This avoids revalidating every request
    /// row in callers that need all three values for one engine CPI.
    #[inline(never)]
    pub fn encode_digest_and_callback_seed(
        &self,
        engine_program: &Pubkey,
    ) -> WireResult<(Vec<u8>, [u8; 32], [u8; 32])> {
        let encoded = self.encode()?;
        let digest = hash_canonical_engine_request_data(&encoded)?;
        let callback_seed = compute_callback_seed_for_validated_request(self, engine_program)?;
        Ok((encoded, digest, callback_seed))
    }

    pub fn validate(&self) -> WireResult<()> {
        self.header.validate()?;
        require_vector_count(
            "engine asset rows",
            self.assets.len(),
            self.header.asset_count,
        )?;
        require_vector_count(
            "engine domain rows",
            self.domains.len(),
            self.header.domain_count,
        )?;
        require_vector_count(
            "engine intent rows",
            self.intents.len(),
            self.header.intent_count,
        )?;
        require_vector_count(
            "engine context rows",
            self.contexts.len(),
            self.header.context_row_count,
        )?;
        if self.payload.len() != usize::from(self.header.payload_len) {
            return Err(WireError::InvalidLength {
                expected: usize::from(self.header.payload_len),
                actual: self.payload.len(),
            });
        }
        let mut asset_binding_digests = Vec::with_capacity(self.assets.len());
        for (index, row) in self.assets.iter().enumerate() {
            row.validate()?;
            if usize::from(row.asset_index) != index {
                return Err(WireError::InvalidIndex {
                    field: "engine asset row position",
                    index: row.asset_index,
                    count: self.header.asset_count,
                });
            }
            asset_binding_digests.push(compute_asset_binding_digest(
                &AssetBindingRowCandidateV0 {
                    wire_version: WIRE_VERSION,
                    flags: row.asset_flags,
                    decimals: row.decimals,
                    reserved: row.reserved,
                    asset_identity: row.asset_identity,
                    asset_program: row.asset_program,
                    settlement_profile_digest: row.settlement_profile_digest,
                },
            )?);
        }
        if asset_binding_digests
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        {
            return Err(WireError::NonCanonicalOrder {
                field: "engine asset binding digests",
            });
        }
        for (index, row) in self.domains.iter().enumerate() {
            row.validate()?;
            if usize::from(row.domain_index) != index {
                return Err(WireError::InvalidIndex {
                    field: "engine domain row position",
                    index: row.domain_index,
                    count: self.header.domain_count,
                });
            }
        }
        // Core assigns these positions by strictly increasing authenticated
        // domain-descriptor *digest*.  The engine row deliberately carries
        // only the descriptor account key, so Wire cannot reconstruct or
        // independently prove that ordering from this row alone.
        for (index, row) in self.intents.iter().enumerate() {
            row.validate()?;
            row.validate_index(self.header.intent_count)?;
            if usize::from(row.authorization_slot) != index {
                return Err(WireError::InvalidIndex {
                    field: "engine intent row position",
                    index: row.authorization_slot,
                    count: self.header.intent_count,
                });
            }
        }
        self.fee_policy.validate()?;
        let observed_fee_policy =
            compute_fee_policy_digest(&DISPOSABLE_CORE_PROGRAM_ID.to_bytes(), &self.fee_policy)?;
        if observed_fee_policy != self.header.fee_policy_digest {
            return Err(WireError::DigestMismatch {
                field: "engine fee policy digest",
            });
        }
        let intent_rows = self
            .intents
            .iter()
            .map(|row| IntentSetRowCandidateV0 {
                intent_digest: row.intent_digest,
            })
            .collect::<Vec<_>>();
        let observed_intent_set =
            compute_intent_set_digest(&self.header.domain_set_digest, &intent_rows)?;
        if observed_intent_set != self.header.intent_set_digest {
            return Err(WireError::DigestMismatch {
                field: "engine intent set digest",
            });
        }
        let mut previous = None;
        let mut endpoint_keys = Vec::with_capacity(self.contexts.len());
        for row in &self.contexts {
            row.validate()?;
            row.validate_indices(&self.header)?;
            if previous.is_some_and(|value| row.settlement_capability_index <= value) {
                return Err(WireError::NonCanonicalOrder {
                    field: "engine context rows",
                });
            }
            previous = Some(row.settlement_capability_index);
            if endpoint_keys.contains(&row.endpoint_key) {
                return Err(WireError::NonCanonicalOrder {
                    field: "engine context endpoint keys",
                });
            }
            endpoint_keys.push(row.endpoint_key);
        }
        Ok(())
    }
}

pub fn decode_engine_request(data: &[u8]) -> WireResult<EngineRequestCandidateV0> {
    if data.len() > MAX_ENGINE_REQUEST_LEN {
        return Err(WireError::LimitExceeded {
            field: "engine request length",
            maximum: MAX_ENGINE_REQUEST_LEN,
            actual: data.len(),
        });
    }
    if data.len() < 8 + ENGINE_REQUEST_HEADER_LEN {
        return Err(WireError::InvalidLength {
            expected: 8 + ENGINE_REQUEST_HEADER_LEN,
            actual: data.len(),
        });
    }
    let mut reader = Reader::new(data);
    if reader.read_array::<8>()? != ENGINE_TRANSITION_DISCRIMINATOR {
        return Err(WireError::InvalidDiscriminator);
    }
    let header =
        EngineRequestHeaderCandidateV0::decode_exact(&reader.read_vec(ENGINE_REQUEST_HEADER_LEN)?)?;
    let expected = engine_request_encoded_length(&header)?;
    require_exact_length(data, expected)?;

    let mut assets = Vec::with_capacity(usize::from(header.asset_count));
    for _ in 0..header.asset_count {
        assets.push(EngineAssetRowCandidateV0::decode_exact(
            &reader.read_vec(ENGINE_ASSET_ROW_LEN)?,
        )?);
    }
    let mut domains = Vec::with_capacity(usize::from(header.domain_count));
    for _ in 0..header.domain_count {
        domains.push(EngineDomainRowCandidateV0::decode_exact(
            &reader.read_vec(ENGINE_DOMAIN_ROW_LEN)?,
        )?);
    }
    let mut intents = Vec::with_capacity(usize::from(header.intent_count));
    for _ in 0..header.intent_count {
        intents.push(EngineIntentRowCandidateV0::decode_exact(
            &reader.read_vec(ENGINE_INTENT_ROW_LEN)?,
        )?);
    }
    let fee_policy =
        EngineFeePolicyRowCandidateV0::decode_exact(&reader.read_vec(ENGINE_FEE_POLICY_ROW_LEN)?)?;
    let mut contexts = Vec::with_capacity(usize::from(header.context_row_count));
    for _ in 0..header.context_row_count {
        contexts.push(EngineContextRowCandidateV0::decode_exact(
            &reader.read_vec(ENGINE_CONTEXT_ROW_LEN)?,
        )?);
    }
    let payload = reader.read_vec(usize::from(header.payload_len))?;
    reader.finish()?;
    let request = EngineRequestCandidateV0 {
        header,
        assets,
        domains,
        intents,
        fee_policy,
        contexts,
        payload,
    };
    request.validate()?;
    Ok(request)
}

fn hash_canonical_engine_request_data(data: &[u8]) -> WireResult<[u8; 32]> {
    hash_private(LABEL_ENGINE_REQUEST, &[data])
}

pub fn compute_callback_seed_for_engine(
    request: &EngineRequestCandidateV0,
    engine_program: &Pubkey,
) -> WireResult<[u8; 32]> {
    request.validate()?;
    compute_callback_seed_for_validated_request(request, engine_program)
}

fn compute_callback_seed_for_validated_request(
    request: &EngineRequestCandidateV0,
    engine_program: &Pubkey,
) -> WireResult<[u8; 32]> {
    let major = CORE_EXPERIMENTAL_MAJOR.to_le_bytes();
    let phase = [request.header.phase];
    let payload_digest = compute_payload_digest(request.payload())?;
    hash_private(
        LABEL_CALLBACK_SEED,
        &[
            DISPOSABLE_CORE_PROGRAM_ID.as_ref(),
            &major,
            engine_program.as_ref(),
            &request.header.engine_interface_id,
            &request.header.engine_instance_id,
            &request.header.engine_loader_state_snapshot_digest,
            &request.header.market_binding_digest,
            &request.header.intent_set_digest,
            &request.header.domain_set_digest,
            &request.header.protected_execution_root,
            &request.header.opaque_capability_root,
            &payload_digest,
            &phase,
        ],
    )
}

pub fn derive_callback_authority_for_engine(
    request: &EngineRequestCandidateV0,
    engine_program: &Pubkey,
) -> WireResult<(Pubkey, u8)> {
    let seed = compute_callback_seed_for_engine(request, engine_program)?;
    Ok(Pubkey::find_program_address(
        &[&seed],
        &DISPOSABLE_CORE_PROGRAM_ID,
    ))
}

pub fn derive_callback_authority(request: &EngineRequestCandidateV0) -> WireResult<(Pubkey, u8)> {
    derive_callback_authority_for_engine(request, &DISPOSABLE_ENGINE_PROGRAM_ID)
}

fn engine_request_encoded_length(header: &EngineRequestHeaderCandidateV0) -> WireResult<usize> {
    let length = checked_encoded_length(
        8 + ENGINE_REQUEST_HEADER_LEN,
        &[
            (usize::from(header.asset_count), ENGINE_ASSET_ROW_LEN),
            (usize::from(header.domain_count), ENGINE_DOMAIN_ROW_LEN),
            (usize::from(header.intent_count), ENGINE_INTENT_ROW_LEN),
            (1, ENGINE_FEE_POLICY_ROW_LEN),
            (
                usize::from(header.context_row_count),
                ENGINE_CONTEXT_ROW_LEN,
            ),
            (usize::from(header.payload_len), 1),
        ],
    )?;
    if length > MAX_ENGINE_REQUEST_LEN {
        Err(WireError::LimitExceeded {
            field: "engine request length",
            maximum: MAX_ENGINE_REQUEST_LEN,
            actual: length,
        })
    } else {
        Ok(length)
    }
}

fn require_limit(field: &'static str, count: u8, maximum: usize) -> WireResult<()> {
    if usize::from(count) <= maximum {
        Ok(())
    } else {
        Err(WireError::LimitExceeded {
            field,
            maximum,
            actual: usize::from(count),
        })
    }
}

fn require_vector_count(_field: &'static str, actual: usize, expected: u8) -> WireResult<()> {
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
    use crate::rows::{
        FEE_CLASS_GROSS_DEBIT_RATE, FEE_CLASS_NONE, RIGHT_CREDIT, RIGHT_DEBIT,
        RIGHT_DOMAIN_ACCOUNTED,
    };
    use crate::NONE_INDEX;

    pub(crate) fn request() -> EngineRequestCandidateV0 {
        let mut request = EngineRequestCandidateV0 {
            header: EngineRequestHeaderCandidateV0 {
                magic: ENGINE_REQUEST_MAGIC,
                wire_version: WIRE_VERSION,
                phase: PHASE_TRANSITION,
                settlement_capability_count: 2,
                opaque_capability_count: 0,
                intent_count: 1,
                domain_count: 1,
                asset_count: 1,
                context_row_count: 2,
                payload_len: 3,
                maximum_engine_moves: 2,
                market_binding_digest: [1; 32],
                engine_instance_id: [2; 32],
                engine_interface_id: [3; 32],
                intent_set_digest: [4; 32],
                domain_set_digest: [5; 32],
                protected_execution_root: [6; 32],
                opaque_capability_root: [7; 32],
                engine_loader_state_snapshot_digest: [8; 32],
                fee_policy_digest: [9; 32],
            },
            assets: alloc::vec![EngineAssetRowCandidateV0 {
                asset_index: 0,
                asset_flags: 0,
                decimals: 6,
                reserved: 0,
                asset_identity: [10; 32],
                asset_program: [11; 32],
                settlement_profile_digest: [12; 32],
            }],
            domains: alloc::vec![EngineDomainRowCandidateV0 {
                domain_index: 0,
                domain_descriptor: [13; 32],
                domain_revision: 14,
                admission_digest: [15; 32],
                accounting_profile_digest: [16; 32],
            }],
            intents: alloc::vec![EngineIntentRowCandidateV0 {
                authorization_slot: 0,
                identity: InlineIntentIdentityRowCandidateV0 {
                    actor: [17; 32],
                    engine_terms_commitment: [18; 32],
                    authorization_nonce: 19,
                    expires_at_slot_exclusive: 20,
                },
                intent_digest: [21; 32],
            }],
            fee_policy: EngineFeePolicyRowCandidateV0 {
                wire_version: WIRE_VERSION,
                rounding_mode: ROUNDING_FLOOR,
                flags: 0,
                revision: 1,
                rate_numerator: 3,
                nonzero_denominator: 1_000,
            },
            contexts: alloc::vec![
                EngineContextRowCandidateV0 {
                    settlement_capability_index: 0,
                    asset_index: 0,
                    domain_index_or_none: NONE_INDEX,
                    authorization_slot_or_none: 0,
                    rights_bits: RIGHT_DEBIT,
                    fee_class: FEE_CLASS_GROSS_DEBIT_RATE,
                    context_flags: 0,
                    endpoint_key: [22; 32],
                    observed_before: 23,
                    accounted_before_or_zero: 0,
                    remaining_maximum_engine_debit: 24,
                    remaining_maximum_total_debit: 25,
                    remaining_minimum_credit: 0,
                    remaining_maximum_protocol_fee: 0,
                },
                EngineContextRowCandidateV0 {
                    settlement_capability_index: 1,
                    asset_index: 0,
                    domain_index_or_none: 0,
                    authorization_slot_or_none: NONE_INDEX,
                    rights_bits: RIGHT_DOMAIN_ACCOUNTED | RIGHT_CREDIT,
                    fee_class: FEE_CLASS_NONE,
                    context_flags: 0,
                    endpoint_key: [26; 32],
                    observed_before: 27,
                    accounted_before_or_zero: 28,
                    remaining_maximum_engine_debit: 0,
                    remaining_maximum_total_debit: 0,
                    remaining_minimum_credit: 0,
                    remaining_maximum_protocol_fee: 0,
                },
            ],
            payload: alloc::vec![23, 24, 25],
        };
        request.header.fee_policy_digest =
            compute_fee_policy_digest(&DISPOSABLE_CORE_PROGRAM_ID.to_bytes(), &request.fee_policy)
                .unwrap();
        request.header.intent_set_digest = compute_intent_set_digest(
            &request.header.domain_set_digest,
            &[IntentSetRowCandidateV0 {
                intent_digest: request.intents[0].intent_digest,
            }],
        )
        .unwrap();
        request
    }

    #[test]
    fn request_round_trips_exactly_and_binds_callback() {
        let request = request();
        let encoded = request.encode().unwrap();
        assert_eq!(encoded.len(), 8 + 312 + 100 + 112 + 120 + 32 + 176 + 3);
        assert_eq!(decode_engine_request(&encoded), Ok(request.clone()));
        assert_eq!(
            request.encode_and_digest().unwrap(),
            (
                encoded.clone(),
                hash_canonical_engine_request_data(&encoded).unwrap(),
            )
        );
        assert_eq!(
            request.digest().unwrap(),
            hash_canonical_engine_request_data(&encoded).unwrap()
        );
        let (callback, _) = derive_callback_authority(&request).unwrap();
        let mut changed = request;
        changed.header.market_binding_digest[0] ^= 1;
        assert_ne!(callback, derive_callback_authority(&changed).unwrap().0);
    }

    #[test]
    fn independent_all_axis_request_maximum_is_3744() {
        let mut header = request().header;
        header.asset_count = MAX_ASSETS as u8;
        header.domain_count = MAX_DOMAINS as u8;
        header.intent_count = MAX_INTENTS as u8;
        header.context_row_count = MAX_CONTEXT_ROWS as u8;
        header.settlement_capability_count = MAX_SETTLEMENT_CAPABILITIES as u8;
        header.payload_len = MAX_OPAQUE_PAYLOAD_LEN as u16;
        header.validate().unwrap();
        assert_eq!(engine_request_encoded_length(&header), Ok(3_744));
    }

    #[test]
    fn request_rejects_every_structural_boundary() {
        let encoded = request().encode().unwrap();
        assert!(decode_engine_request(&encoded[..encoded.len() - 1]).is_err());
        let mut trailing = encoded.clone();
        trailing.push(0);
        assert!(decode_engine_request(&trailing).is_err());

        let mut bad = encoded.clone();
        bad[0] ^= 1;
        assert_eq!(
            decode_engine_request(&bad),
            Err(WireError::InvalidDiscriminator)
        );
        let mut bad = encoded.clone();
        bad[8] ^= 1;
        assert_eq!(decode_engine_request(&bad), Err(WireError::InvalidMagic));
        let mut bad = encoded;
        bad[8 + 19] = 1;
        assert!(matches!(
            decode_engine_request(&bad),
            Err(WireError::NonZeroReserved { .. })
        ));
    }

    #[test]
    fn request_rejects_reordered_contexts_and_noncontiguous_assets() {
        let mut reordered = request();
        reordered.contexts.swap(0, 1);
        assert!(matches!(
            reordered.encode(),
            Err(WireError::NonCanonicalOrder { .. })
        ));
        let mut noncontiguous = request();
        noncontiguous.assets[0].asset_index = 1;
        assert!(noncontiguous.encode().is_err());
    }

    #[test]
    fn request_accepts_an_exhausted_source_in_an_active_stored_intent() {
        let mut exhausted = request();
        exhausted.contexts[0].remaining_maximum_engine_debit = 0;
        exhausted.contexts[0].remaining_maximum_total_debit = 0;
        exhausted.contexts[0].remaining_maximum_protocol_fee = 0;
        assert!(exhausted.encode().is_ok());
    }
}
