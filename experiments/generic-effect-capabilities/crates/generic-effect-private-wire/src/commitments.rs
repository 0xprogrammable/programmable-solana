//! Typed canonical encoders for every security-critical private commitment.
//!
//! Core and fixture programs call these helpers instead of reconstructing hash
//! preimages locally. Each helper owns the frozen field order, scalar width,
//! reserved-byte policy, list framing, and canonical ordering rule.

use alloc::vec::Vec;

use solana_pubkey::Pubkey;

use crate::codec::{
    put_bytes, put_u128, put_u16, put_u32, put_u64, put_u8, require_exact_length, require_zero,
    Reader,
};
use crate::hashes::{
    hash_asset_set_rows, hash_authorization_view_set_rows, hash_domain_set_rows,
    hash_fee_shard_set_rows, hash_market_binding_row, hash_private,
    hash_protected_capability_set_rows, LABEL_AUTHORIZATION_CAPABILITY_STATE,
    LABEL_AUTHORIZATION_FEE_STATE, LABEL_AUTHORIZATION_STATE, LABEL_CLASSIC_SPL_ENDPOINT_STATE,
    LABEL_CORE_VERIFIED_EVIDENCE, LABEL_DOMAIN_DESCRIPTOR, LABEL_DOMAIN_EXECUTION,
    LABEL_ENGINE_ATTESTED_EVIDENCE, LABEL_FEE_ASSESSMENT, LABEL_FEE_ASSESSMENT_SET,
    LABEL_FEE_COLLECTION, LABEL_FEE_POLICY, LABEL_FEE_PRINCIPAL, LABEL_FEE_ROUNDING_GROUP,
    LABEL_INTENT, LABEL_INTENT_CAPABILITY_TERMS, LABEL_INTENT_CORE_TERMS,
    LABEL_INTENT_CREDIT_CONSTRAINTS, LABEL_INTENT_DEBIT_GROUP, LABEL_INTENT_SET,
    LABEL_OBSERVED_PROTECTED_DELTA_SET,
};
use crate::request::FeePolicyRowCandidateV0;
use crate::rows::{
    InlineIntentIdentityRowCandidateV0, AUTHORITY_CORE_RESERVED_FEE, AUTHORITY_INTENT_FUNDED,
    FEE_CLASS_GROSS_DEBIT_RATE, FEE_CLASS_NONE, RIGHT_DEBIT, SETTLEMENT_FLAG_FEE_FUNDING,
    SETTLEMENT_RIGHTS_MASK,
};
use crate::{
    WireError, WireResult, CORE_EXPERIMENTAL_MAJOR, MAX_ASSETS, MAX_AUTHORIZATION_ACCOUNTS,
    MAX_DOMAINS, MAX_FEE_SHARDS, MAX_INTENTS, MAX_SETTLEMENT_CAPABILITIES, NONE_INDEX,
    WIRE_VERSION,
};

pub const MARKET_BINDING_ROW_LEN: usize = 332;
pub const ASSET_BINDING_ROW_LEN: usize = 100;
pub const DOMAIN_EXECUTION_ROW_LEN: usize = 208;
pub const DOMAIN_DESCRIPTOR_ROW_LEN: usize = 304;
pub const CLASSIC_SPL_ENDPOINT_STATE_ROW_LEN: usize = 224;
pub const OBSERVED_PROTECTED_DELTA_ROW_LEN: usize = 40;
pub const IMMUTABLE_ENGINE_RELEASE_OBSERVATION_ROW_LEN: usize = 208;
pub const AUTHORIZATION_VIEW_ROW_LEN: usize = 72;
pub const AUTHORIZATION_CAPABILITY_STATE_ROW_LEN: usize = 88;
pub const AUTHORIZATION_FEE_STATE_ROW_LEN: usize = 80;
pub const INTENT_SET_ROW_LEN: usize = 32;
pub const INTENT_CAPABILITY_TERM_ROW_LEN: usize = 136;
pub const CREDIT_CONSTRAINT_ROW_LEN: usize = 64;
pub const FEE_ROUNDING_GROUP_ROW_LEN: usize = 176;
pub const FEE_ASSESSMENT_SET_ROW_LEN: usize = 64;
pub const FEE_SHARD_DIGEST_ROW_LEN: usize = 256;
pub const PROTECTED_CAPABILITY_DIGEST_ROW_LEN: usize = 368;

pub const AUTHORIZATION_LIFECYCLE_DRAFT: u8 = 0;
pub const AUTHORIZATION_LIFECYCLE_ACTIVE: u8 = 1;
pub const AUTHORIZATION_LIFECYCLE_EXECUTING: u8 = 2;
pub const AUTHORIZATION_LIFECYCLE_CONSUMED: u8 = 3;
pub const AUTHORIZATION_LIFECYCLE_CANCELLED: u8 = 4;
pub const INTENT_CAPABILITY_TERM_FLAG_FEE_FUNDING: u8 = SETTLEMENT_FLAG_FEE_FUNDING;
pub const INTENT_CAPABILITY_TERM_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT: u8 =
    crate::SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT;
pub const INTENT_CAPABILITY_TERM_FLAGS_MASK: u8 = INTENT_CAPABILITY_TERM_FLAG_FEE_FUNDING
    | INTENT_CAPABILITY_TERM_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT;
pub const AUTHORIZATION_CAPABILITY_STATE_FLAG_FEE_FUNDING: u8 = SETTLEMENT_FLAG_FEE_FUNDING;
pub const AUTHORIZATION_CAPABILITY_STATE_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT: u8 =
    crate::SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT;
pub const AUTHORIZATION_CAPABILITY_STATE_FLAGS_MASK: u8 =
    AUTHORIZATION_CAPABILITY_STATE_FLAG_FEE_FUNDING
        | AUTHORIZATION_CAPABILITY_STATE_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarketBindingRowCandidateV0 {
    pub core_program: [u8; 32],
    pub core_experimental_major: u32,
    pub market_descriptor_key: [u8; 32],
    pub market_descriptor_revision: u64,
    pub engine_program: [u8; 32],
    pub engine_interface_id: [u8; 32],
    pub engine_instance_id: [u8; 32],
    pub engine_admission_policy_digest: [u8; 32],
    pub domain_admission_profile_digest: [u8; 32],
    pub protected_profile_digest: [u8; 32],
    pub fee_policy_digest: [u8; 32],
    pub opaque_schema_digest: [u8; 32],
}

impl MarketBindingRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; MARKET_BINDING_ROW_LEN]> {
        if self.core_program == [0; 32]
            || self.market_descriptor_key == [0; 32]
            || self.engine_program == [0; 32]
            || self.engine_interface_id == [0; 32]
            || self.engine_instance_id == [0; 32]
            || self.engine_admission_policy_digest == [0; 32]
            || self.domain_admission_profile_digest == [0; 32]
            || self.protected_profile_digest == [0; 32]
            || self.fee_policy_digest == [0; 32]
            || self.opaque_schema_digest == [0; 32]
            || self.core_experimental_major != CORE_EXPERIMENTAL_MAJOR
        {
            return Err(WireError::UnsupportedValue {
                field: "market binding identity",
                value: u64::from(self.core_experimental_major),
            });
        }
        let mut output = Vec::with_capacity(MARKET_BINDING_ROW_LEN);
        put_bytes(&mut output, &self.core_program);
        put_u32(&mut output, self.core_experimental_major);
        put_bytes(&mut output, &self.market_descriptor_key);
        put_u64(&mut output, self.market_descriptor_revision);
        put_bytes(&mut output, &self.engine_program);
        put_bytes(&mut output, &self.engine_interface_id);
        put_bytes(&mut output, &self.engine_instance_id);
        put_bytes(&mut output, &self.engine_admission_policy_digest);
        put_bytes(&mut output, &self.domain_admission_profile_digest);
        put_bytes(&mut output, &self.protected_profile_digest);
        put_bytes(&mut output, &self.fee_policy_digest);
        put_bytes(&mut output, &self.opaque_schema_digest);
        Ok(output
            .try_into()
            .expect("market binding row has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, MARKET_BINDING_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let row = Self {
            core_program: reader.read_array()?,
            core_experimental_major: reader.read_u32()?,
            market_descriptor_key: reader.read_array()?,
            market_descriptor_revision: reader.read_u64()?,
            engine_program: reader.read_array()?,
            engine_interface_id: reader.read_array()?,
            engine_instance_id: reader.read_array()?,
            engine_admission_policy_digest: reader.read_array()?,
            domain_admission_profile_digest: reader.read_array()?,
            protected_profile_digest: reader.read_array()?,
            fee_policy_digest: reader.read_array()?,
            opaque_schema_digest: reader.read_array()?,
        };
        reader.finish()?;
        row.encode()?;
        Ok(row)
    }

    pub fn digest(&self) -> WireResult<[u8; 32]> {
        hash_market_binding_row(&self.encode()?)
    }
}

pub fn compute_market_binding_digest(row: &MarketBindingRowCandidateV0) -> WireResult<[u8; 32]> {
    row.digest()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DomainDescriptorRowCandidateV0 {
    pub wire_version: u8,
    pub rule_kind: u8,
    pub controller_program: [u8; 32],
    pub controller_identity: [u8; 32],
    pub domain_revision: u64,
    pub namespace_or_instance: [u8; 32],
    pub custody_profile_digest: [u8; 32],
    pub asset_profile_digest: [u8; 32],
    pub accounting_profile_digest: [u8; 32],
    pub exit_class_digest: [u8; 32],
    pub admission_rule_digest: [u8; 32],
    pub protected_profile_digest: [u8; 32],
}

impl DomainDescriptorRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; DOMAIN_DESCRIPTOR_ROW_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(DOMAIN_DESCRIPTOR_ROW_LEN);
        put_u8(&mut output, self.wire_version);
        put_u8(&mut output, self.rule_kind);
        put_bytes(&mut output, &[0; 6]);
        put_bytes(&mut output, &self.controller_program);
        put_bytes(&mut output, &self.controller_identity);
        put_u64(&mut output, self.domain_revision);
        put_bytes(&mut output, &self.namespace_or_instance);
        put_bytes(&mut output, &self.custody_profile_digest);
        put_bytes(&mut output, &self.asset_profile_digest);
        put_bytes(&mut output, &self.accounting_profile_digest);
        put_bytes(&mut output, &self.exit_class_digest);
        put_bytes(&mut output, &self.admission_rule_digest);
        put_bytes(&mut output, &self.protected_profile_digest);
        Ok(output
            .try_into()
            .expect("domain descriptor row has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, DOMAIN_DESCRIPTOR_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let wire_version = reader.read_u8()?;
        let rule_kind = reader.read_u8()?;
        let reserved = reader.read_array::<6>()?;
        require_zero("domain descriptor reserved", &reserved)?;
        let row = Self {
            wire_version,
            rule_kind,
            controller_program: reader.read_array()?,
            controller_identity: reader.read_array()?,
            domain_revision: reader.read_u64()?,
            namespace_or_instance: reader.read_array()?,
            custody_profile_digest: reader.read_array()?,
            asset_profile_digest: reader.read_array()?,
            accounting_profile_digest: reader.read_array()?,
            exit_class_digest: reader.read_array()?,
            admission_rule_digest: reader.read_array()?,
            protected_profile_digest: reader.read_array()?,
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
        if self.controller_program == [0; 32]
            || self.controller_identity == [0; 32]
            || self.custody_profile_digest == [0; 32]
            || self.asset_profile_digest == [0; 32]
            || self.accounting_profile_digest == [0; 32]
            || self.exit_class_digest == [0; 32]
            || self.protected_profile_digest == [0; 32]
        {
            return Err(WireError::UnsupportedValue {
                field: "domain descriptor identity",
                value: 0,
            });
        }
        let open_rule = crate::compute_open_domain_rule_digest()?;
        match self.rule_kind {
            crate::DOMAIN_RULE_OPEN if self.admission_rule_digest == open_rule => {}
            crate::DOMAIN_RULE_CLOSED
                if self.admission_rule_digest != [0; 32]
                    && self.admission_rule_digest != open_rule => {}
            crate::DOMAIN_RULE_OPEN | crate::DOMAIN_RULE_CLOSED => {
                return Err(WireError::DigestMismatch {
                    field: "domain descriptor admission rule digest",
                });
            }
            value => {
                return Err(WireError::UnsupportedValue {
                    field: "domain descriptor rule kind",
                    value: u64::from(value),
                });
            }
        }
        Ok(())
    }

    pub fn digest(&self, core_program: &[u8; 32]) -> WireResult<[u8; 32]> {
        let major = CORE_EXPERIMENTAL_MAJOR.to_le_bytes();
        hash_private(
            LABEL_DOMAIN_DESCRIPTOR,
            &[core_program, &major, &self.encode()?],
        )
    }
}

pub fn compute_domain_descriptor_digest(
    core_program: &[u8; 32],
    row: &DomainDescriptorRowCandidateV0,
) -> WireResult<[u8; 32]> {
    row.digest(core_program)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssetBindingRowCandidateV0 {
    pub wire_version: u8,
    pub flags: u8,
    pub decimals: u8,
    pub reserved: u8,
    pub asset_identity: [u8; 32],
    pub asset_program: [u8; 32],
    pub settlement_profile_digest: [u8; 32],
}

impl AssetBindingRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; ASSET_BINDING_ROW_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(ASSET_BINDING_ROW_LEN);
        put_u8(&mut output, self.wire_version);
        put_u8(&mut output, self.flags);
        put_u8(&mut output, self.decimals);
        put_u8(&mut output, self.reserved);
        put_bytes(&mut output, &self.asset_identity);
        put_bytes(&mut output, &self.asset_program);
        put_bytes(&mut output, &self.settlement_profile_digest);
        Ok(output
            .try_into()
            .expect("asset binding row has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, ASSET_BINDING_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let row = Self {
            wire_version: reader.read_u8()?,
            flags: reader.read_u8()?,
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

    pub fn validate(&self) -> WireResult<()> {
        if self.wire_version != WIRE_VERSION {
            return Err(WireError::UnsupportedVersion {
                expected: WIRE_VERSION,
                actual: self.wire_version,
            });
        }
        if self.flags != 0 {
            return Err(WireError::UnknownFlags {
                field: "asset binding flags",
                value: u64::from(self.flags),
            });
        }
        if self.reserved != 0 {
            return Err(WireError::NonZeroReserved {
                field: "asset binding reserved",
            });
        }
        if self.asset_identity == [0; 32]
            || self.asset_program == [0; 32]
            || self.settlement_profile_digest == [0; 32]
        {
            return Err(WireError::UnsupportedValue {
                field: "asset binding identity",
                value: 0,
            });
        }
        Ok(())
    }

    pub fn digest(&self) -> WireResult<[u8; 32]> {
        hash_private(crate::hashes::LABEL_ASSET, &[&self.encode()?])
    }
}

pub fn compute_asset_binding_digest(row: &AssetBindingRowCandidateV0) -> WireResult<[u8; 32]> {
    row.digest()
}

pub fn compute_asset_set_digest(rows: &[AssetBindingRowCandidateV0]) -> WireResult<[u8; 32]> {
    require_count_limit("asset binding rows", rows.len(), MAX_ASSETS)?;
    let encoded = rows
        .iter()
        .map(AssetBindingRowCandidateV0::encode)
        .collect::<WireResult<Vec<_>>>()?;
    let digests = rows
        .iter()
        .map(AssetBindingRowCandidateV0::digest)
        .collect::<WireResult<Vec<_>>>()?;
    require_strictly_increasing("asset binding digests", &digests)?;
    let slices = encoded.iter().map(|row| row.as_slice()).collect::<Vec<_>>();
    hash_asset_set_rows(&slices)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DomainExecutionRowCandidateV0 {
    pub domain_index: u8,
    pub admission_kind: u8,
    pub domain_descriptor_key: [u8; 32],
    pub domain_descriptor_digest: [u8; 32],
    pub domain_revision: u64,
    pub admission_account_or_zero: [u8; 32],
    pub admission_digest: [u8; 32],
    pub accounting_account: [u8; 32],
    pub accounting_profile_digest: [u8; 32],
}

impl DomainExecutionRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; DOMAIN_EXECUTION_ROW_LEN]> {
        if self.domain_descriptor_key == [0; 32]
            || self.domain_descriptor_digest == [0; 32]
            || self.accounting_account == [0; 32]
            || self.accounting_profile_digest == [0; 32]
        {
            return Err(WireError::UnsupportedValue {
                field: "domain execution identity",
                value: 0,
            });
        }
        match self.admission_kind {
            crate::ADMISSION_OPEN if self.admission_account_or_zero == [0; 32] => {
                if self.admission_digest == [0; 32] {
                    return Err(WireError::UnsupportedValue {
                        field: "open domain admission digest",
                        value: 0,
                    });
                }
            }
            crate::ADMISSION_CLOSED
                if self.admission_account_or_zero != [0; 32]
                    && self.admission_digest != [0; 32] => {}
            crate::ADMISSION_OPEN | crate::ADMISSION_CLOSED => {
                return Err(WireError::UnsupportedValue {
                    field: "domain execution admission shape",
                    value: u64::from(self.admission_kind),
                });
            }
            value => {
                return Err(WireError::UnsupportedValue {
                    field: "domain execution admission kind",
                    value: u64::from(value),
                });
            }
        }
        let mut output = Vec::with_capacity(DOMAIN_EXECUTION_ROW_LEN);
        put_u8(&mut output, self.domain_index);
        put_u8(&mut output, self.admission_kind);
        put_bytes(&mut output, &[0; 6]);
        put_bytes(&mut output, &self.domain_descriptor_key);
        put_bytes(&mut output, &self.domain_descriptor_digest);
        put_u64(&mut output, self.domain_revision);
        put_bytes(&mut output, &self.admission_account_or_zero);
        put_bytes(&mut output, &self.admission_digest);
        put_bytes(&mut output, &self.accounting_account);
        put_bytes(&mut output, &self.accounting_profile_digest);
        Ok(output
            .try_into()
            .expect("domain execution row has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, DOMAIN_EXECUTION_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let domain_index = reader.read_u8()?;
        let admission_kind = reader.read_u8()?;
        let reserved = reader.read_array::<6>()?;
        require_zero("domain execution reserved", &reserved)?;
        let row = Self {
            domain_index,
            admission_kind,
            domain_descriptor_key: reader.read_array()?,
            domain_descriptor_digest: reader.read_array()?,
            domain_revision: reader.read_u64()?,
            admission_account_or_zero: reader.read_array()?,
            admission_digest: reader.read_array()?,
            accounting_account: reader.read_array()?,
            accounting_profile_digest: reader.read_array()?,
        };
        reader.finish()?;
        row.encode()?;
        Ok(row)
    }

    pub fn digest(&self, market_binding_digest: &[u8; 32]) -> WireResult<[u8; 32]> {
        if self.admission_kind == crate::ADMISSION_OPEN
            && self.admission_digest
                != crate::compute_open_domain_admission_digest(
                    &self.domain_descriptor_digest,
                    market_binding_digest,
                )?
        {
            return Err(WireError::DigestMismatch {
                field: "open domain admission digest",
            });
        }
        hash_private(
            LABEL_DOMAIN_EXECUTION,
            &[market_binding_digest, &self.encode()?],
        )
    }
}

pub fn compute_domain_execution_digest(
    market_binding_digest: &[u8; 32],
    row: &DomainExecutionRowCandidateV0,
) -> WireResult<[u8; 32]> {
    row.digest(market_binding_digest)
}

pub fn compute_domain_set_digest(
    market_binding_digest: &[u8; 32],
    rows: &[DomainExecutionRowCandidateV0],
) -> WireResult<[u8; 32]> {
    require_count_limit("domain execution rows", rows.len(), MAX_DOMAINS)?;
    let mut digests = Vec::with_capacity(rows.len());
    let mut previous_descriptor_digest = None;
    for (position, row) in rows.iter().enumerate() {
        require_contiguous_index(
            "domain execution index",
            row.domain_index,
            position,
            rows.len(),
        )?;
        if previous_descriptor_digest
            .is_some_and(|previous| row.domain_descriptor_digest <= previous)
        {
            return Err(WireError::NonCanonicalOrder {
                field: "domain descriptor digests",
            });
        }
        previous_descriptor_digest = Some(row.domain_descriptor_digest);
        digests.push(row.digest(market_binding_digest)?);
    }
    let slices = digests.iter().map(|row| row.as_slice()).collect::<Vec<_>>();
    hash_domain_set_rows(&slices)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthorizationCapabilityStateRowCandidateV0 {
    pub local_term_index: u8,
    pub reserved_0: u8,
    pub flags: u8,
    pub initial_maximum_engine_debit: u64,
    pub initial_minimum_credit: u64,
    pub initial_maximum_total_debit: u64,
    pub remaining_total_debit: u64,
    pub cumulative_engine_debit: u128,
    pub cumulative_fee_debit: u128,
    pub cumulative_credit: u128,
}

impl AuthorizationCapabilityStateRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; AUTHORIZATION_CAPABILITY_STATE_ROW_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(AUTHORIZATION_CAPABILITY_STATE_ROW_LEN);
        put_u8(&mut output, self.local_term_index);
        put_u8(&mut output, self.reserved_0);
        put_u8(&mut output, self.flags);
        put_bytes(&mut output, &[0; 5]);
        put_u64(&mut output, self.initial_maximum_engine_debit);
        put_u64(&mut output, self.initial_minimum_credit);
        put_u64(&mut output, self.initial_maximum_total_debit);
        put_u64(&mut output, self.remaining_total_debit);
        put_u128(&mut output, self.cumulative_engine_debit);
        put_u128(&mut output, self.cumulative_fee_debit);
        put_u128(&mut output, self.cumulative_credit);
        Ok(output
            .try_into()
            .expect("authorization capability state has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, AUTHORIZATION_CAPABILITY_STATE_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let local_term_index = reader.read_u8()?;
        let reserved_0 = reader.read_u8()?;
        let flags = reader.read_u8()?;
        let reserved = reader.read_array::<5>()?;
        require_zero("authorization capability state reserved", &reserved)?;
        let row = Self {
            local_term_index,
            reserved_0,
            flags,
            initial_maximum_engine_debit: reader.read_u64()?,
            initial_minimum_credit: reader.read_u64()?,
            initial_maximum_total_debit: reader.read_u64()?,
            remaining_total_debit: reader.read_u64()?,
            cumulative_engine_debit: reader.read_u128()?,
            cumulative_fee_debit: reader.read_u128()?,
            cumulative_credit: reader.read_u128()?,
        };
        reader.finish()?;
        row.validate()?;
        Ok(row)
    }

    pub fn validate(&self) -> WireResult<()> {
        if self.flags & !AUTHORIZATION_CAPABILITY_STATE_FLAGS_MASK != 0 {
            return Err(WireError::UnknownFlags {
                field: "authorization capability state flags",
                value: u64::from(self.flags),
            });
        }
        if self.reserved_0 != 0 {
            return Err(WireError::NonZeroReserved {
                field: "authorization capability state reserved_0",
            });
        }
        let consumed = self
            .cumulative_engine_debit
            .checked_add(self.cumulative_fee_debit)
            .ok_or(WireError::LengthOverflow)?;
        if self.initial_maximum_total_debit < self.initial_maximum_engine_debit
            || self.cumulative_engine_debit > u128::from(self.initial_maximum_engine_debit)
            || consumed > u128::from(self.initial_maximum_total_debit)
            || u128::from(self.remaining_total_debit)
                != u128::from(self.initial_maximum_total_debit) - consumed
        {
            return Err(WireError::UnsupportedValue {
                field: "authorization capability remaining total debit",
                value: self.remaining_total_debit,
            });
        }
        if self.initial_maximum_engine_debit != 0 {
            if self.initial_minimum_credit != 0 || self.cumulative_credit != 0 {
                return Err(WireError::UnsupportedValue {
                    field: "authorization debit capability state shape",
                    value: self.initial_minimum_credit,
                });
            }
        } else if self.initial_maximum_total_debit != 0
            || self.remaining_total_debit != 0
            || self.cumulative_engine_debit != 0
            || self.cumulative_fee_debit != 0
            || self.flags != 0
        {
            return Err(WireError::UnsupportedValue {
                field: "authorization credit capability state shape",
                value: self.initial_maximum_total_debit,
            });
        }
        Ok(())
    }
}

pub fn compute_authorization_capability_state_root(
    rows: &[AuthorizationCapabilityStateRowCandidateV0],
) -> WireResult<[u8; 32]> {
    require_count_limit(
        "authorization capability state rows",
        rows.len(),
        MAX_SETTLEMENT_CAPABILITIES,
    )?;
    for (position, row) in rows.iter().enumerate() {
        require_contiguous_index(
            "authorization capability state local term index",
            row.local_term_index,
            position,
            rows.len(),
        )?;
    }
    let encoded = rows
        .iter()
        .map(AuthorizationCapabilityStateRowCandidateV0::encode)
        .collect::<WireResult<Vec<_>>>()?;
    let slices = encoded.iter().map(|row| row.as_slice()).collect::<Vec<_>>();
    crate::hashes::hash_list(LABEL_AUTHORIZATION_CAPABILITY_STATE, &slices)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthorizationFeeStateRowCandidateV0 {
    pub rounding_group_digest: [u8; 32],
    pub funding_local_term_index: u8,
    pub fee_class: u8,
    pub flags: u8,
    pub cumulative_basis: u128,
    pub cumulative_assessed_fee: u128,
    pub maximum_fee: u64,
}

impl AuthorizationFeeStateRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; AUTHORIZATION_FEE_STATE_ROW_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(AUTHORIZATION_FEE_STATE_ROW_LEN);
        put_bytes(&mut output, &self.rounding_group_digest);
        put_u8(&mut output, self.funding_local_term_index);
        put_u8(&mut output, self.fee_class);
        put_u8(&mut output, self.flags);
        put_bytes(&mut output, &[0; 5]);
        put_u128(&mut output, self.cumulative_basis);
        put_u128(&mut output, self.cumulative_assessed_fee);
        put_u64(&mut output, self.maximum_fee);
        Ok(output
            .try_into()
            .expect("authorization fee state has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, AUTHORIZATION_FEE_STATE_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let rounding_group_digest = reader.read_array()?;
        let funding_local_term_index = reader.read_u8()?;
        let fee_class = reader.read_u8()?;
        let flags = reader.read_u8()?;
        let reserved = reader.read_array::<5>()?;
        require_zero("authorization fee state reserved", &reserved)?;
        let row = Self {
            rounding_group_digest,
            funding_local_term_index,
            fee_class,
            flags,
            cumulative_basis: reader.read_u128()?,
            cumulative_assessed_fee: reader.read_u128()?,
            maximum_fee: reader.read_u64()?,
        };
        reader.finish()?;
        row.validate()?;
        Ok(row)
    }

    pub fn validate(&self) -> WireResult<()> {
        if self.rounding_group_digest == [0; 32] {
            return Err(WireError::UnsupportedValue {
                field: "authorization fee state group",
                value: 0,
            });
        }
        if self.flags != 0 {
            return Err(WireError::UnknownFlags {
                field: "authorization fee state flags",
                value: u64::from(self.flags),
            });
        }
        if self.fee_class != FEE_CLASS_GROSS_DEBIT_RATE
            || usize::from(self.funding_local_term_index) >= MAX_SETTLEMENT_CAPABILITIES
            || self.maximum_fee == 0
            || self.cumulative_assessed_fee > u128::from(self.maximum_fee)
            || self.cumulative_assessed_fee > self.cumulative_basis
        {
            return Err(WireError::UnsupportedValue {
                field: "authorization fee state shape",
                value: u64::from(self.fee_class),
            });
        }
        Ok(())
    }
}

pub fn compute_authorization_fee_state_root(
    rows: &[AuthorizationFeeStateRowCandidateV0],
) -> WireResult<[u8; 32]> {
    require_count_limit(
        "authorization fee state rows",
        rows.len(),
        MAX_SETTLEMENT_CAPABILITIES,
    )?;
    let encoded = rows
        .iter()
        .map(AuthorizationFeeStateRowCandidateV0::encode)
        .collect::<WireResult<Vec<_>>>()?;
    let groups = rows
        .iter()
        .map(|row| row.rounding_group_digest)
        .collect::<Vec<_>>();
    require_strictly_increasing("authorization fee state groups", &groups)?;
    let slices = encoded.iter().map(|row| row.as_slice()).collect::<Vec<_>>();
    crate::hashes::hash_list(LABEL_AUTHORIZATION_FEE_STATE, &slices)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthorizationStateDigestInputs<'a> {
    pub intent_digest: &'a [u8; 32],
    pub lifecycle: u8,
    pub fill_sequence: u32,
    pub successful_fills: u32,
    pub remaining_fills: u32,
    pub capability_state_root: &'a [u8; 32],
    pub fee_state_root: &'a [u8; 32],
    pub stored_authorization_key_or_zero: &'a [u8; 32],
}

pub fn compute_authorization_state_digest(
    inputs: AuthorizationStateDigestInputs<'_>,
) -> WireResult<[u8; 32]> {
    if inputs.lifecycle > AUTHORIZATION_LIFECYCLE_CANCELLED {
        return Err(WireError::UnsupportedValue {
            field: "authorization lifecycle",
            value: u64::from(inputs.lifecycle),
        });
    }
    let lifecycle = [inputs.lifecycle];
    let fill_sequence = inputs.fill_sequence.to_le_bytes();
    let successful_fills = inputs.successful_fills.to_le_bytes();
    let remaining_fills = inputs.remaining_fills.to_le_bytes();
    hash_private(
        LABEL_AUTHORIZATION_STATE,
        &[
            inputs.intent_digest,
            &lifecycle,
            &fill_sequence,
            &successful_fills,
            &remaining_fills,
            inputs.capability_state_root,
            inputs.fee_state_root,
            inputs.stored_authorization_key_or_zero,
        ],
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthorizationViewRowCandidateV0 {
    pub authorization_slot: u8,
    pub intent_digest: [u8; 32],
    pub authorization_state_digest: [u8; 32],
}

impl AuthorizationViewRowCandidateV0 {
    pub fn encode(&self) -> [u8; AUTHORIZATION_VIEW_ROW_LEN] {
        let mut output = Vec::with_capacity(AUTHORIZATION_VIEW_ROW_LEN);
        put_u8(&mut output, self.authorization_slot);
        put_bytes(&mut output, &[0; 7]);
        put_bytes(&mut output, &self.intent_digest);
        put_bytes(&mut output, &self.authorization_state_digest);
        output
            .try_into()
            .expect("authorization view row has a fixed encoded length")
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, AUTHORIZATION_VIEW_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let authorization_slot = reader.read_u8()?;
        let reserved = reader.read_array::<7>()?;
        require_zero("authorization view reserved", &reserved)?;
        let row = Self {
            authorization_slot,
            intent_digest: reader.read_array()?,
            authorization_state_digest: reader.read_array()?,
        };
        reader.finish()?;
        Ok(row)
    }
}

pub fn compute_authorization_view_set_digest(
    rows: &[AuthorizationViewRowCandidateV0],
) -> WireResult<[u8; 32]> {
    require_count_limit("authorization view rows", rows.len(), MAX_INTENTS)?;
    for (position, row) in rows.iter().enumerate() {
        require_contiguous_index(
            "authorization view slot",
            row.authorization_slot,
            position,
            rows.len(),
        )?;
    }
    let intent_digests = rows.iter().map(|row| row.intent_digest).collect::<Vec<_>>();
    require_strictly_increasing("authorization view intent digests", &intent_digests)?;
    let encoded = rows.iter().map(|row| row.encode()).collect::<Vec<_>>();
    let slices = encoded.iter().map(|row| row.as_slice()).collect::<Vec<_>>();
    hash_authorization_view_set_rows(&slices)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntentCoreTermsDigestInputs<'a> {
    pub maximum_successful_fills: u32,
    pub capability_terms_root: &'a [u8; 32],
    pub credit_constraints_root: &'a [u8; 32],
}

pub fn compute_intent_core_terms_root(
    inputs: IntentCoreTermsDigestInputs<'_>,
) -> WireResult<[u8; 32]> {
    if inputs.maximum_successful_fills == 0 {
        return Err(WireError::UnsupportedValue {
            field: "maximum successful fills",
            value: 0,
        });
    }
    let maximum_successful_fills = inputs.maximum_successful_fills.to_le_bytes();
    hash_private(
        LABEL_INTENT_CORE_TERMS,
        &[
            &maximum_successful_fills,
            inputs.capability_terms_root,
            inputs.credit_constraints_root,
        ],
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntentDigestInputs<'a> {
    pub core_program: &'a [u8; 32],
    pub market_binding_digest: &'a [u8; 32],
    pub loader_state_snapshot_digest: &'a [u8; 32],
    pub fee_policy_digest: &'a [u8; 32],
    pub identity: &'a InlineIntentIdentityRowCandidateV0,
    pub core_terms_root: &'a [u8; 32],
}

pub fn compute_intent_digest(inputs: IntentDigestInputs<'_>) -> WireResult<[u8; 32]> {
    let major = CORE_EXPERIMENTAL_MAJOR.to_le_bytes();
    let identity = inputs.identity.encode()?;
    hash_private(
        LABEL_INTENT,
        &[
            inputs.core_program,
            &major,
            inputs.market_binding_digest,
            inputs.loader_state_snapshot_digest,
            inputs.fee_policy_digest,
            &identity,
            inputs.core_terms_root,
        ],
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntentSetRowCandidateV0 {
    pub intent_digest: [u8; 32],
}

impl IntentSetRowCandidateV0 {
    pub fn encode(&self) -> [u8; INTENT_SET_ROW_LEN] {
        let mut output = Vec::with_capacity(INTENT_SET_ROW_LEN);
        put_bytes(&mut output, &self.intent_digest);
        output
            .try_into()
            .expect("intent set row has a fixed encoded length")
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, INTENT_SET_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let row = Self {
            intent_digest: reader.read_array()?,
        };
        reader.finish()?;
        Ok(row)
    }
}

pub fn compute_intent_set_digest(
    domain_set_digest: &[u8; 32],
    rows: &[IntentSetRowCandidateV0],
) -> WireResult<[u8; 32]> {
    require_count_limit("intent set rows", rows.len(), MAX_INTENTS)?;
    let encoded = rows.iter().map(|row| row.encode()).collect::<Vec<_>>();
    require_strictly_increasing("intent set rows", &encoded)?;
    let count = u32::try_from(rows.len())
        .map_err(|_| WireError::LengthOverflow)?
        .to_le_bytes();
    let mut parts = Vec::with_capacity(encoded.len().saturating_add(2));
    parts.push(domain_set_digest.as_slice());
    parts.push(count.as_slice());
    parts.extend(encoded.iter().map(|row| row.as_slice()));
    hash_private(LABEL_INTENT_SET, &parts)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntentCapabilityTermRowCandidateV0 {
    pub intent_local_term_index: u8,
    pub authority_class: u8,
    pub fee_class: u8,
    pub flags: u8,
    pub rights_bits: u16,
    pub endpoint_key: [u8; 32],
    pub asset_binding_digest: [u8; 32],
    pub required_domain_descriptor_digest_or_zero: [u8; 32],
    pub maximum_engine_debit: u64,
    pub maximum_total_debit: u64,
    pub minimum_credit: u64,
    pub maximum_protocol_fee: u64,
}

impl IntentCapabilityTermRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; INTENT_CAPABILITY_TERM_ROW_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(INTENT_CAPABILITY_TERM_ROW_LEN);
        put_u8(&mut output, self.intent_local_term_index);
        put_u8(&mut output, self.authority_class);
        put_u8(&mut output, self.fee_class);
        put_u8(&mut output, self.flags);
        put_u16(&mut output, self.rights_bits);
        put_bytes(&mut output, &[0; 2]);
        put_bytes(&mut output, &self.endpoint_key);
        put_bytes(&mut output, &self.asset_binding_digest);
        put_bytes(&mut output, &self.required_domain_descriptor_digest_or_zero);
        put_u64(&mut output, self.maximum_engine_debit);
        put_u64(&mut output, self.maximum_total_debit);
        put_u64(&mut output, self.minimum_credit);
        put_u64(&mut output, self.maximum_protocol_fee);
        Ok(output
            .try_into()
            .expect("intent capability term has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, INTENT_CAPABILITY_TERM_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let intent_local_term_index = reader.read_u8()?;
        let authority_class = reader.read_u8()?;
        let fee_class = reader.read_u8()?;
        let flags = reader.read_u8()?;
        let rights_bits = reader.read_u16()?;
        let reserved = reader.read_array::<2>()?;
        require_zero("intent capability term reserved", &reserved)?;
        let row = Self {
            intent_local_term_index,
            authority_class,
            fee_class,
            flags,
            rights_bits,
            endpoint_key: reader.read_array()?,
            asset_binding_digest: reader.read_array()?,
            required_domain_descriptor_digest_or_zero: reader.read_array()?,
            maximum_engine_debit: reader.read_u64()?,
            maximum_total_debit: reader.read_u64()?,
            minimum_credit: reader.read_u64()?,
            maximum_protocol_fee: reader.read_u64()?,
        };
        reader.finish()?;
        row.validate()?;
        Ok(row)
    }

    pub fn validate(&self) -> WireResult<()> {
        if self.authority_class > AUTHORITY_CORE_RESERVED_FEE
            || self.fee_class > FEE_CLASS_GROSS_DEBIT_RATE
        {
            return Err(WireError::UnsupportedValue {
                field: "intent capability authority or fee class",
                value: u64::from(self.authority_class),
            });
        }
        if !matches!(
            self.authority_class,
            AUTHORITY_INTENT_FUNDED | crate::AUTHORITY_EXACT_EXTERNAL_CREDIT
        ) {
            return Err(WireError::UnsupportedValue {
                field: "persistent intent capability authority class",
                value: u64::from(self.authority_class),
            });
        }
        if self.flags & !INTENT_CAPABILITY_TERM_FLAGS_MASK != 0 {
            return Err(WireError::UnknownFlags {
                field: "intent capability term flags",
                value: u64::from(self.flags),
            });
        }
        if self.flags & INTENT_CAPABILITY_TERM_FLAG_FEE_FUNDING != 0
            && (self.authority_class != AUTHORITY_INTENT_FUNDED
                || self.fee_class == FEE_CLASS_NONE
                || self.rights_bits != RIGHT_DEBIT
                || self.maximum_engine_debit == 0
                || self.maximum_protocol_fee == 0)
        {
            return Err(WireError::UnsupportedValue {
                field: "intent fee-funding term shape",
                value: u64::from(self.flags),
            });
        }
        let is_intent_debit =
            self.authority_class == AUTHORITY_INTENT_FUNDED && self.rights_bits == RIGHT_DEBIT;
        if is_intent_debit {
            if self.fee_class != FEE_CLASS_GROSS_DEBIT_RATE
                || self.maximum_engine_debit == 0
                || self.maximum_total_debit < self.maximum_engine_debit
                || self.maximum_protocol_fee > self.maximum_total_debit
            {
                return Err(WireError::UnsupportedValue {
                    field: "intent-funded debit fee shape",
                    value: u64::from(self.fee_class),
                });
            }
            if self.flags & INTENT_CAPABILITY_TERM_FLAG_FEE_FUNDING == 0
                && (self.maximum_protocol_fee != 0
                    || self.maximum_total_debit != self.maximum_engine_debit)
            {
                return Err(WireError::UnsupportedValue {
                    field: "non-funding intent debit fee bounds",
                    value: self.maximum_protocol_fee,
                });
            }
        } else if self.fee_class != FEE_CLASS_NONE
            || self.flags & INTENT_CAPABILITY_TERM_FLAG_FEE_FUNDING != 0
            || self.flags & INTENT_CAPABILITY_TERM_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT != 0
            || self.maximum_protocol_fee != 0
        {
            return Err(WireError::UnsupportedValue {
                field: "non-user-debit fee shape",
                value: u64::from(self.fee_class),
            });
        }
        if self.rights_bits == 0 || self.rights_bits & !SETTLEMENT_RIGHTS_MASK != 0 {
            return Err(WireError::UnknownFlags {
                field: "intent capability term rights",
                value: u64::from(self.rights_bits),
            });
        }
        match self.authority_class {
            AUTHORITY_INTENT_FUNDED => {
                if self.rights_bits != RIGHT_DEBIT || self.minimum_credit != 0 {
                    return Err(WireError::UnsupportedValue {
                        field: "intent-funded term role shape",
                        value: u64::from(self.rights_bits),
                    });
                }
            }
            crate::AUTHORITY_EXACT_EXTERNAL_CREDIT => {
                if self.rights_bits != (crate::RIGHT_EXACT_EXTERNAL_RECIPIENT | crate::RIGHT_CREDIT)
                    || self.maximum_engine_debit != 0
                    || self.maximum_total_debit != 0
                {
                    return Err(WireError::UnsupportedValue {
                        field: "external-credit term role shape",
                        value: u64::from(self.rights_bits),
                    });
                }
            }
            _ => unreachable!("persistent authority class was restricted above"),
        }
        if self.endpoint_key == [0; 32] || self.asset_binding_digest == [0; 32] {
            return Err(WireError::UnsupportedValue {
                field: "intent capability term identity",
                value: 0,
            });
        }
        Ok(())
    }
}

pub fn compute_intent_capability_terms_root(
    rows: &[IntentCapabilityTermRowCandidateV0],
) -> WireResult<[u8; 32]> {
    require_count_limit(
        "intent capability term rows",
        rows.len(),
        MAX_SETTLEMENT_CAPABILITIES,
    )?;
    for (position, row) in rows.iter().enumerate() {
        require_contiguous_index(
            "intent capability local index",
            row.intent_local_term_index,
            position,
            rows.len(),
        )?;
    }
    let encoded = rows
        .iter()
        .map(IntentCapabilityTermRowCandidateV0::encode)
        .collect::<WireResult<Vec<_>>>()?;
    let slices = encoded.iter().map(|row| row.as_slice()).collect::<Vec<_>>();
    crate::hashes::hash_list(LABEL_INTENT_CAPABILITY_TERMS, &slices)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreditConstraintRowCandidateV0 {
    pub constraint_index: u8,
    pub credit_local_term_index: u8,
    pub flags: u8,
    pub debit_source_bitmap: u16,
    pub debit_group_root: [u8; 32],
    pub minimum_credit_numerator: u64,
    pub nonzero_debit_denominator: u64,
    pub terminal_absolute_minimum: u64,
}

impl CreditConstraintRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; CREDIT_CONSTRAINT_ROW_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(CREDIT_CONSTRAINT_ROW_LEN);
        put_u8(&mut output, self.constraint_index);
        put_u8(&mut output, self.credit_local_term_index);
        put_u8(&mut output, self.flags);
        put_bytes(&mut output, &[0; 3]);
        put_u16(&mut output, self.debit_source_bitmap);
        put_bytes(&mut output, &self.debit_group_root);
        put_u64(&mut output, self.minimum_credit_numerator);
        put_u64(&mut output, self.nonzero_debit_denominator);
        put_u64(&mut output, self.terminal_absolute_minimum);
        Ok(output
            .try_into()
            .expect("credit constraint has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, CREDIT_CONSTRAINT_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let constraint_index = reader.read_u8()?;
        let credit_local_term_index = reader.read_u8()?;
        let flags = reader.read_u8()?;
        let reserved = reader.read_array::<3>()?;
        require_zero("credit constraint reserved", &reserved)?;
        let row = Self {
            constraint_index,
            credit_local_term_index,
            flags,
            debit_source_bitmap: reader.read_u16()?,
            debit_group_root: reader.read_array()?,
            minimum_credit_numerator: reader.read_u64()?,
            nonzero_debit_denominator: reader.read_u64()?,
            terminal_absolute_minimum: reader.read_u64()?,
        };
        reader.finish()?;
        row.validate()?;
        Ok(row)
    }

    pub fn validate(&self) -> WireResult<()> {
        if self.flags != 0 {
            return Err(WireError::UnknownFlags {
                field: "credit constraint flags",
                value: u64::from(self.flags),
            });
        }
        if self.nonzero_debit_denominator == 0 {
            return Err(WireError::UnsupportedValue {
                field: "credit constraint denominator",
                value: 0,
            });
        }
        if self.minimum_credit_numerator == 0 && self.terminal_absolute_minimum == 0 {
            return Err(WireError::UnsupportedValue {
                field: "zero-ratio credit constraint terminal minimum",
                value: 0,
            });
        }
        let allowed_mask = (1_u16 << MAX_SETTLEMENT_CAPABILITIES) - 1;
        if self.debit_source_bitmap == 0 || self.debit_source_bitmap & !allowed_mask != 0 {
            return Err(WireError::UnsupportedValue {
                field: "credit constraint debit source bitmap",
                value: u64::from(self.debit_source_bitmap),
            });
        }
        if usize::from(self.credit_local_term_index) >= MAX_SETTLEMENT_CAPABILITIES {
            return Err(WireError::InvalidIndex {
                field: "credit constraint local credit term",
                index: self.credit_local_term_index,
                count: MAX_SETTLEMENT_CAPABILITIES as u8,
            });
        }
        if self.debit_source_bitmap & (1_u16 << self.credit_local_term_index) != 0 {
            return Err(WireError::UnsupportedValue {
                field: "credit constraint includes credit term as debit source",
                value: u64::from(self.credit_local_term_index),
            });
        }
        let sources = (0..MAX_SETTLEMENT_CAPABILITIES)
            .filter(|index| self.debit_source_bitmap & (1_u16 << index) != 0)
            .map(|index| index as u8)
            .collect::<Vec<_>>();
        if self.debit_group_root != compute_intent_debit_group_root(&sources)? {
            return Err(WireError::DigestMismatch {
                field: "credit constraint debit group root",
            });
        }
        Ok(())
    }
}

pub fn compute_intent_credit_constraints_root(
    rows: &[CreditConstraintRowCandidateV0],
) -> WireResult<[u8; 32]> {
    require_count_limit(
        "credit constraint rows",
        rows.len(),
        MAX_SETTLEMENT_CAPABILITIES,
    )?;
    for (position, row) in rows.iter().enumerate() {
        require_contiguous_index(
            "credit constraint index",
            row.constraint_index,
            position,
            rows.len(),
        )?;
    }
    let encoded = rows
        .iter()
        .map(CreditConstraintRowCandidateV0::encode)
        .collect::<WireResult<Vec<_>>>()?;
    let slices = encoded.iter().map(|row| row.as_slice()).collect::<Vec<_>>();
    crate::hashes::hash_list(LABEL_INTENT_CREDIT_CONSTRAINTS, &slices)
}

pub fn compute_intent_debit_group_root(local_source_indices: &[u8]) -> WireResult<[u8; 32]> {
    if local_source_indices.is_empty() {
        return Err(WireError::InvalidLength {
            expected: 1,
            actual: 0,
        });
    }
    require_count_limit(
        "intent debit group source indices",
        local_source_indices.len(),
        MAX_SETTLEMENT_CAPABILITIES,
    )?;
    if local_source_indices
        .windows(2)
        .any(|pair| pair[0] >= pair[1])
    {
        return Err(WireError::NonCanonicalOrder {
            field: "intent debit group local source indices",
        });
    }
    if let Some(index) = local_source_indices
        .iter()
        .copied()
        .find(|index| usize::from(*index) >= MAX_SETTLEMENT_CAPABILITIES)
    {
        return Err(WireError::InvalidIndex {
            field: "intent debit group local source index",
            index,
            count: MAX_SETTLEMENT_CAPABILITIES as u8,
        });
    }
    let rows = local_source_indices
        .iter()
        .map(core::slice::from_ref)
        .collect::<Vec<_>>();
    crate::hashes::hash_list(LABEL_INTENT_DEBIT_GROUP, &rows)
}

pub fn compute_fee_principal_digest(
    actor: &[u8; 32],
    intent_digest: &[u8; 32],
) -> WireResult<[u8; 32]> {
    hash_private(LABEL_FEE_PRINCIPAL, &[actor, intent_digest])
}

pub fn compute_fee_policy_digest(
    core_program: &[u8; 32],
    fee_policy: &FeePolicyRowCandidateV0,
) -> WireResult<[u8; 32]> {
    let major = CORE_EXPERIMENTAL_MAJOR.to_le_bytes();
    hash_private(
        LABEL_FEE_POLICY,
        &[core_program, &major, &fee_policy.encode()?],
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeeRoundingGroupRowCandidateV0 {
    pub fee_principal_digest: [u8; 32],
    pub fee_policy_digest: [u8; 32],
    pub asset_identity: [u8; 32],
    pub asset_program: [u8; 32],
    pub settlement_profile_digest: [u8; 32],
    pub fee_class: u8,
    pub fee_policy_revision: u64,
}

impl FeeRoundingGroupRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; FEE_ROUNDING_GROUP_ROW_LEN]> {
        if self.fee_principal_digest == [0; 32]
            || self.fee_policy_digest == [0; 32]
            || self.asset_identity == [0; 32]
            || self.asset_program == [0; 32]
            || self.settlement_profile_digest == [0; 32]
            || self.fee_class != FEE_CLASS_GROSS_DEBIT_RATE
        {
            return Err(WireError::UnsupportedValue {
                field: "fee rounding group identity",
                value: 0,
            });
        }
        let mut output = Vec::with_capacity(FEE_ROUNDING_GROUP_ROW_LEN);
        put_bytes(&mut output, &self.fee_principal_digest);
        put_bytes(&mut output, &self.fee_policy_digest);
        put_bytes(&mut output, &self.asset_identity);
        put_bytes(&mut output, &self.asset_program);
        put_bytes(&mut output, &self.settlement_profile_digest);
        put_u8(&mut output, self.fee_class);
        put_bytes(&mut output, &[0; 7]);
        put_u64(&mut output, self.fee_policy_revision);
        Ok(output
            .try_into()
            .expect("fee rounding group has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, FEE_ROUNDING_GROUP_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let row = Self {
            fee_principal_digest: reader.read_array()?,
            fee_policy_digest: reader.read_array()?,
            asset_identity: reader.read_array()?,
            asset_program: reader.read_array()?,
            settlement_profile_digest: reader.read_array()?,
            fee_class: reader.read_u8()?,
            fee_policy_revision: {
                let reserved = reader.read_array::<7>()?;
                require_zero("fee rounding group reserved", &reserved)?;
                reader.read_u64()?
            },
        };
        reader.finish()?;
        row.encode()?;
        Ok(row)
    }

    pub fn digest(&self) -> WireResult<[u8; 32]> {
        hash_private(LABEL_FEE_ROUNDING_GROUP, &[&self.encode()?])
    }
}

pub fn compute_fee_rounding_group_digest(
    row: &FeeRoundingGroupRowCandidateV0,
) -> WireResult<[u8; 32]> {
    row.digest()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeeCollectionDigestInputs<'a> {
    pub assessment_group_digest: &'a [u8; 32],
    pub designated_funding_endpoint_key: &'a [u8; 32],
    pub maximum_protocol_fee: u64,
    pub maximum_total_debit: u64,
    pub fee_shard_index: u8,
    pub fee_delta: u64,
}

pub fn compute_fee_collection_digest(
    inputs: FeeCollectionDigestInputs<'_>,
) -> WireResult<[u8; 32]> {
    if usize::from(inputs.fee_shard_index) >= MAX_FEE_SHARDS {
        return Err(WireError::InvalidIndex {
            field: "fee collection shard index",
            index: inputs.fee_shard_index,
            count: MAX_FEE_SHARDS as u8,
        });
    }
    let maximum_protocol_fee = inputs.maximum_protocol_fee.to_le_bytes();
    let maximum_total_debit = inputs.maximum_total_debit.to_le_bytes();
    let fee_shard_index = [inputs.fee_shard_index];
    let fee_delta = inputs.fee_delta.to_le_bytes();
    hash_private(
        LABEL_FEE_COLLECTION,
        &[
            inputs.assessment_group_digest,
            inputs.designated_funding_endpoint_key,
            &maximum_protocol_fee,
            &maximum_total_debit,
            &fee_shard_index,
            &fee_delta,
        ],
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeeAssessmentDigestInputs<'a> {
    pub core_program: &'a [u8; 32],
    pub market_binding_digest: &'a [u8; 32],
    pub fee_policy_digest: &'a [u8; 32],
    pub fee_policy_revision: u64,
    pub intent_set_digest: &'a [u8; 32],
    pub protected_execution_root: &'a [u8; 32],
    pub effect_digest: &'a [u8; 32],
    pub rounding_group_digest: &'a [u8; 32],
    pub fee_collection_digest: &'a [u8; 32],
    pub fill_sequence: u32,
    pub cumulative_before: u128,
    pub fill_basis: u128,
    pub cumulative_after: u128,
    pub fee_delta: u64,
}

pub fn compute_fee_assessment_digest(
    inputs: FeeAssessmentDigestInputs<'_>,
) -> WireResult<[u8; 32]> {
    let major = CORE_EXPERIMENTAL_MAJOR.to_le_bytes();
    let revision = inputs.fee_policy_revision.to_le_bytes();
    let fill_sequence = inputs.fill_sequence.to_le_bytes();
    let cumulative_before = inputs.cumulative_before.to_le_bytes();
    let fill_basis = inputs.fill_basis.to_le_bytes();
    let cumulative_after = inputs.cumulative_after.to_le_bytes();
    let fee_delta = inputs.fee_delta.to_le_bytes();
    hash_private(
        LABEL_FEE_ASSESSMENT,
        &[
            inputs.core_program,
            &major,
            inputs.market_binding_digest,
            inputs.fee_policy_digest,
            &revision,
            inputs.intent_set_digest,
            inputs.protected_execution_root,
            inputs.effect_digest,
            inputs.rounding_group_digest,
            inputs.fee_collection_digest,
            &fill_sequence,
            &cumulative_before,
            &fill_basis,
            &cumulative_after,
            &fee_delta,
        ],
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeeAssessmentSetRowCandidateV0 {
    pub assessment_group_digest: [u8; 32],
    pub assessment_digest: [u8; 32],
}

impl FeeAssessmentSetRowCandidateV0 {
    pub fn encode(&self) -> [u8; FEE_ASSESSMENT_SET_ROW_LEN] {
        let mut output = Vec::with_capacity(FEE_ASSESSMENT_SET_ROW_LEN);
        put_bytes(&mut output, &self.assessment_group_digest);
        put_bytes(&mut output, &self.assessment_digest);
        output
            .try_into()
            .expect("fee assessment set row has a fixed encoded length")
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, FEE_ASSESSMENT_SET_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let row = Self {
            assessment_group_digest: reader.read_array()?,
            assessment_digest: reader.read_array()?,
        };
        reader.finish()?;
        Ok(row)
    }
}

pub fn compute_fee_assessment_set_root(
    rows: &[FeeAssessmentSetRowCandidateV0],
) -> WireResult<[u8; 32]> {
    require_count_limit(
        "fee assessment set rows",
        rows.len(),
        MAX_SETTLEMENT_CAPABILITIES,
    )?;
    let groups = rows
        .iter()
        .map(|row| row.assessment_group_digest)
        .collect::<Vec<_>>();
    require_strictly_increasing("fee assessment set groups", &groups)?;
    let encoded = rows.iter().map(|row| row.encode()).collect::<Vec<_>>();
    let slices = encoded.iter().map(|row| row.as_slice()).collect::<Vec<_>>();
    crate::hashes::hash_list(LABEL_FEE_ASSESSMENT_SET, &slices)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeeShardDigestRowCandidateV0 {
    pub shard_index: u8,
    pub asset_index: u8,
    pub vault_settlement_capability_index: u8,
    pub flags: u8,
    pub descriptor_key: [u8; 32],
    pub descriptor_digest: [u8; 32],
    pub liability_key: [u8; 32],
    pub vault_key: [u8; 32],
    pub asset_binding_digest: [u8; 32],
    pub fee_policy_digest: [u8; 32],
    pub recipient_policy_digest: [u8; 32],
    pub fee_policy_revision: u64,
    pub liability_before: u128,
}

impl FeeShardDigestRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; FEE_SHARD_DIGEST_ROW_LEN]> {
        if self.flags != 0 {
            return Err(WireError::UnknownFlags {
                field: "fee shard digest flags",
                value: u64::from(self.flags),
            });
        }
        if usize::from(self.shard_index) >= MAX_FEE_SHARDS {
            return Err(WireError::InvalidIndex {
                field: "fee shard digest index",
                index: self.shard_index,
                count: MAX_FEE_SHARDS as u8,
            });
        }
        if usize::from(self.asset_index) >= MAX_ASSETS
            || usize::from(self.vault_settlement_capability_index) >= MAX_SETTLEMENT_CAPABILITIES
        {
            return Err(WireError::InvalidIndex {
                field: "fee shard digest asset or vault index",
                index: self.asset_index,
                count: MAX_ASSETS as u8,
            });
        }
        if self.descriptor_key == [0; 32]
            || self.descriptor_digest == [0; 32]
            || self.liability_key == [0; 32]
            || self.vault_key == [0; 32]
            || self.asset_binding_digest == [0; 32]
            || self.fee_policy_digest == [0; 32]
            || self.recipient_policy_digest == [0; 32]
        {
            return Err(WireError::UnsupportedValue {
                field: "fee shard digest identity",
                value: 0,
            });
        }
        let mut output = Vec::with_capacity(FEE_SHARD_DIGEST_ROW_LEN);
        put_u8(&mut output, self.shard_index);
        put_u8(&mut output, self.asset_index);
        put_u8(&mut output, self.vault_settlement_capability_index);
        put_u8(&mut output, self.flags);
        put_bytes(&mut output, &[0; 4]);
        put_bytes(&mut output, &self.descriptor_key);
        put_bytes(&mut output, &self.descriptor_digest);
        put_bytes(&mut output, &self.liability_key);
        put_bytes(&mut output, &self.vault_key);
        put_bytes(&mut output, &self.asset_binding_digest);
        put_bytes(&mut output, &self.fee_policy_digest);
        put_bytes(&mut output, &self.recipient_policy_digest);
        put_u64(&mut output, self.fee_policy_revision);
        put_u128(&mut output, self.liability_before);
        Ok(output
            .try_into()
            .expect("fee shard digest row has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, FEE_SHARD_DIGEST_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let shard_index = reader.read_u8()?;
        let asset_index = reader.read_u8()?;
        let vault_settlement_capability_index = reader.read_u8()?;
        let flags = reader.read_u8()?;
        let reserved = reader.read_array::<4>()?;
        require_zero("fee shard digest reserved", &reserved)?;
        let row = Self {
            shard_index,
            asset_index,
            vault_settlement_capability_index,
            flags,
            descriptor_key: reader.read_array()?,
            descriptor_digest: reader.read_array()?,
            liability_key: reader.read_array()?,
            vault_key: reader.read_array()?,
            asset_binding_digest: reader.read_array()?,
            fee_policy_digest: reader.read_array()?,
            recipient_policy_digest: reader.read_array()?,
            fee_policy_revision: reader.read_u64()?,
            liability_before: reader.read_u128()?,
        };
        reader.finish()?;
        row.encode()?;
        Ok(row)
    }
}

pub fn compute_fee_shard_set_digest(rows: &[FeeShardDigestRowCandidateV0]) -> WireResult<[u8; 32]> {
    require_count_limit("fee shard digest rows", rows.len(), MAX_FEE_SHARDS)?;
    for (position, row) in rows.iter().enumerate() {
        require_contiguous_index(
            "fee shard digest index",
            row.shard_index,
            position,
            rows.len(),
        )?;
    }
    let encoded = rows
        .iter()
        .map(FeeShardDigestRowCandidateV0::encode)
        .collect::<WireResult<Vec<_>>>()?;
    let slices = encoded.iter().map(|row| row.as_slice()).collect::<Vec<_>>();
    hash_fee_shard_set_rows(&slices)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtectedCapabilityDigestRowCandidateV0 {
    pub capability_position: u8,
    pub asset_index: u8,
    pub domain_index_or_none: u8,
    pub authorization_slot_or_none: u8,
    pub authority_class: u8,
    pub fee_class: u8,
    pub fee_shard_index_or_none: u8,
    pub flags: u8,
    pub rights_bits: u16,
    pub domain_accounting_slot_or_none: u8,
    pub spend_control_offset_or_none: u8,
    pub endpoint_executable: bool,
    pub effective_signer: bool,
    pub effective_writable: bool,
    pub endpoint_key: [u8; 32],
    pub endpoint_owner: [u8; 32],
    pub transfer_authority_key_or_zero: [u8; 32],
    pub asset_identity: [u8; 32],
    pub asset_program: [u8; 32],
    pub settlement_profile_digest: [u8; 32],
    pub domain_descriptor_or_zero: [u8; 32],
    pub domain_admission_digest_or_zero: [u8; 32],
    pub lifecycle_digest: [u8; 32],
    pub domain_revision: u64,
    pub maximum_engine_debit: u64,
    pub maximum_total_debit: u64,
    pub minimum_credit: u64,
    pub maximum_protocol_fee: u64,
    pub fee_policy_revision: u64,
    pub accounted_before_or_zero: u128,
}

impl ProtectedCapabilityDigestRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; PROTECTED_CAPABILITY_DIGEST_ROW_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(PROTECTED_CAPABILITY_DIGEST_ROW_LEN);
        put_u8(&mut output, self.capability_position);
        put_u8(&mut output, self.asset_index);
        put_u8(&mut output, self.domain_index_or_none);
        put_u8(&mut output, self.authorization_slot_or_none);
        put_u8(&mut output, self.authority_class);
        put_u8(&mut output, self.fee_class);
        put_u8(&mut output, self.fee_shard_index_or_none);
        put_u8(&mut output, self.flags);
        put_u16(&mut output, self.rights_bits);
        put_u8(&mut output, self.domain_accounting_slot_or_none);
        put_u8(&mut output, self.spend_control_offset_or_none);
        put_u8(&mut output, u8::from(self.endpoint_executable));
        put_u8(&mut output, u8::from(self.effective_signer));
        put_u8(&mut output, u8::from(self.effective_writable));
        put_u8(&mut output, 0);
        put_bytes(&mut output, &self.endpoint_key);
        put_bytes(&mut output, &self.endpoint_owner);
        put_bytes(&mut output, &self.transfer_authority_key_or_zero);
        put_bytes(&mut output, &self.asset_identity);
        put_bytes(&mut output, &self.asset_program);
        put_bytes(&mut output, &self.settlement_profile_digest);
        put_bytes(&mut output, &self.domain_descriptor_or_zero);
        put_bytes(&mut output, &self.domain_admission_digest_or_zero);
        put_bytes(&mut output, &self.lifecycle_digest);
        put_u64(&mut output, self.domain_revision);
        put_u64(&mut output, self.maximum_engine_debit);
        put_u64(&mut output, self.maximum_total_debit);
        put_u64(&mut output, self.minimum_credit);
        put_u64(&mut output, self.maximum_protocol_fee);
        put_u64(&mut output, self.fee_policy_revision);
        put_u128(&mut output, self.accounted_before_or_zero);
        Ok(output
            .try_into()
            .expect("protected capability digest row has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, PROTECTED_CAPABILITY_DIGEST_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let capability_position = reader.read_u8()?;
        let asset_index = reader.read_u8()?;
        let domain_index_or_none = reader.read_u8()?;
        let authorization_slot_or_none = reader.read_u8()?;
        let authority_class = reader.read_u8()?;
        let fee_class = reader.read_u8()?;
        let fee_shard_index_or_none = reader.read_u8()?;
        let flags = reader.read_u8()?;
        let rights_bits = reader.read_u16()?;
        let domain_accounting_slot_or_none = reader.read_u8()?;
        let spend_control_offset_or_none = reader.read_u8()?;
        let endpoint_executable = decode_bool("protected endpoint executable", reader.read_u8()?)?;
        let effective_signer = decode_bool("protected effective signer", reader.read_u8()?)?;
        let effective_writable = decode_bool("protected effective writable", reader.read_u8()?)?;
        let reserved = reader.read_u8()?;
        if reserved != 0 {
            return Err(WireError::NonZeroReserved {
                field: "protected capability digest reserved",
            });
        }
        let row = Self {
            capability_position,
            asset_index,
            domain_index_or_none,
            authorization_slot_or_none,
            authority_class,
            fee_class,
            fee_shard_index_or_none,
            flags,
            rights_bits,
            domain_accounting_slot_or_none,
            spend_control_offset_or_none,
            endpoint_executable,
            effective_signer,
            effective_writable,
            endpoint_key: reader.read_array()?,
            endpoint_owner: reader.read_array()?,
            transfer_authority_key_or_zero: reader.read_array()?,
            asset_identity: reader.read_array()?,
            asset_program: reader.read_array()?,
            settlement_profile_digest: reader.read_array()?,
            domain_descriptor_or_zero: reader.read_array()?,
            domain_admission_digest_or_zero: reader.read_array()?,
            lifecycle_digest: reader.read_array()?,
            domain_revision: reader.read_u64()?,
            maximum_engine_debit: reader.read_u64()?,
            maximum_total_debit: reader.read_u64()?,
            minimum_credit: reader.read_u64()?,
            maximum_protocol_fee: reader.read_u64()?,
            fee_policy_revision: reader.read_u64()?,
            accounted_before_or_zero: reader.read_u128()?,
        };
        reader.finish()?;
        row.validate()?;
        Ok(row)
    }

    pub fn validate(&self) -> WireResult<()> {
        if usize::from(self.capability_position) >= MAX_SETTLEMENT_CAPABILITIES {
            return Err(WireError::InvalidIndex {
                field: "protected capability position",
                index: self.capability_position,
                count: MAX_SETTLEMENT_CAPABILITIES as u8,
            });
        }
        if self.flags & !crate::SETTLEMENT_FLAGS_MASK != 0 {
            return Err(WireError::UnknownFlags {
                field: "protected capability digest flags",
                value: u64::from(self.flags),
            });
        }
        if self.rights_bits == 0 || self.rights_bits & !SETTLEMENT_RIGHTS_MASK != 0 {
            return Err(WireError::UnknownFlags {
                field: "protected capability digest rights",
                value: u64::from(self.rights_bits),
            });
        }
        if usize::from(self.asset_index) >= MAX_ASSETS {
            return Err(WireError::InvalidIndex {
                field: "protected capability asset index",
                index: self.asset_index,
                count: MAX_ASSETS as u8,
            });
        }
        require_optional_bounded_index(
            "protected capability domain index",
            self.domain_index_or_none,
            MAX_DOMAINS,
        )?;
        require_optional_bounded_index(
            "protected capability authorization slot",
            self.authorization_slot_or_none,
            MAX_INTENTS,
        )?;
        require_optional_bounded_index(
            "protected capability fee shard index",
            self.fee_shard_index_or_none,
            MAX_FEE_SHARDS,
        )?;
        require_optional_bounded_index(
            "protected capability spend control offset",
            self.spend_control_offset_or_none,
            MAX_AUTHORIZATION_ACCOUNTS,
        )?;
        if self.authority_class > AUTHORITY_CORE_RESERVED_FEE
            || self.fee_class > FEE_CLASS_GROSS_DEBIT_RATE
        {
            return Err(WireError::UnsupportedValue {
                field: "protected capability authority or fee class",
                value: u64::from(self.authority_class),
            });
        }
        let fee_funding = self.flags & SETTLEMENT_FLAG_FEE_FUNDING != 0;
        match self.authority_class {
            AUTHORITY_INTENT_FUNDED => {
                if self.rights_bits != RIGHT_DEBIT
                    || self.authorization_slot_or_none == NONE_INDEX
                    || self.transfer_authority_key_or_zero == [0; 32]
                    || self.fee_class != FEE_CLASS_GROSS_DEBIT_RATE
                    || self.maximum_engine_debit == 0
                    || self.maximum_total_debit < self.maximum_engine_debit
                    || self.minimum_credit != 0
                    || self.maximum_protocol_fee > self.maximum_total_debit
                    || (fee_funding
                        && (self.fee_shard_index_or_none == NONE_INDEX
                            || self.maximum_protocol_fee == 0))
                    || (!fee_funding
                        && (self.fee_shard_index_or_none != NONE_INDEX
                            || self.maximum_protocol_fee != 0
                            || self.maximum_total_debit != self.maximum_engine_debit))
                {
                    return Err(WireError::UnsupportedValue {
                        field: "protected intent-funded role shape",
                        value: u64::from(self.authority_class),
                    });
                }
            }
            crate::AUTHORITY_DOMAIN_ACCOUNTED => {
                let debit = self.rights_bits == (crate::RIGHT_DOMAIN_ACCOUNTED | RIGHT_DEBIT);
                let credit =
                    self.rights_bits == (crate::RIGHT_DOMAIN_ACCOUNTED | crate::RIGHT_CREDIT);
                if self.flags != 0
                    || (!debit && !credit)
                    || self.domain_index_or_none == NONE_INDEX
                    || self.authorization_slot_or_none != NONE_INDEX
                    || self.spend_control_offset_or_none != NONE_INDEX
                    || self.fee_shard_index_or_none != NONE_INDEX
                    || self.fee_class != FEE_CLASS_NONE
                    || self.maximum_protocol_fee != 0
                    || self.maximum_total_debit != self.maximum_engine_debit
                    || (debit
                        && (self.maximum_engine_debit == 0
                            || self.minimum_credit != 0
                            || self.transfer_authority_key_or_zero == [0; 32]))
                    || (credit
                        && (self.maximum_engine_debit != 0
                            || self.maximum_total_debit != 0
                            || self.minimum_credit != 0
                            || self.transfer_authority_key_or_zero != [0; 32]))
                {
                    return Err(WireError::UnsupportedValue {
                        field: "protected domain-accounted role shape",
                        value: u64::from(self.authority_class),
                    });
                }
            }
            crate::AUTHORITY_EXACT_EXTERNAL_CREDIT => {
                if self.flags != 0
                    || self.rights_bits
                        != (crate::RIGHT_EXACT_EXTERNAL_RECIPIENT | crate::RIGHT_CREDIT)
                    || self.authorization_slot_or_none == NONE_INDEX
                    || self.spend_control_offset_or_none != NONE_INDEX
                    || self.fee_shard_index_or_none != NONE_INDEX
                    || self.fee_class != FEE_CLASS_NONE
                    || self.maximum_engine_debit != 0
                    || self.maximum_total_debit != 0
                    || self.maximum_protocol_fee != 0
                    || self.transfer_authority_key_or_zero != [0; 32]
                {
                    return Err(WireError::UnsupportedValue {
                        field: "protected external-credit role shape",
                        value: u64::from(self.authority_class),
                    });
                }
            }
            AUTHORITY_CORE_RESERVED_FEE => {
                if self.flags != 0
                    || self.rights_bits != (crate::RIGHT_CORE_RESERVED_FEE | crate::RIGHT_CREDIT)
                    || self.domain_index_or_none != NONE_INDEX
                    || self.authorization_slot_or_none != NONE_INDEX
                    || self.spend_control_offset_or_none != NONE_INDEX
                    || self.fee_shard_index_or_none == NONE_INDEX
                    || self.fee_class != FEE_CLASS_NONE
                    || self.maximum_engine_debit != 0
                    || self.maximum_total_debit != 0
                    || self.minimum_credit != 0
                    || self.maximum_protocol_fee != 0
                    || self.transfer_authority_key_or_zero != [0; 32]
                {
                    return Err(WireError::UnsupportedValue {
                        field: "protected reserved-fee role shape",
                        value: u64::from(self.authority_class),
                    });
                }
            }
            _ => unreachable!("authority class was bounded above"),
        }
        if self.domain_index_or_none == NONE_INDEX {
            if self.domain_accounting_slot_or_none != NONE_INDEX
                || self.domain_descriptor_or_zero != [0; 32]
                || self.domain_admission_digest_or_zero != [0; 32]
                || self.domain_revision != 0
                || self.accounted_before_or_zero != 0
            {
                return Err(WireError::UnsupportedValue {
                    field: "protected no-domain fields",
                    value: u64::from(self.domain_accounting_slot_or_none),
                });
            }
        } else {
            if self.domain_descriptor_or_zero == [0; 32]
                || self.domain_admission_digest_or_zero == [0; 32]
            {
                return Err(WireError::UnsupportedValue {
                    field: "protected domain proof fields",
                    value: u64::from(self.domain_accounting_slot_or_none),
                });
            }
            if self.authority_class == crate::AUTHORITY_DOMAIN_ACCOUNTED {
                if usize::from(self.domain_accounting_slot_or_none) >= MAX_ASSETS {
                    return Err(WireError::UnsupportedValue {
                        field: "protected domain accounting slot",
                        value: u64::from(self.domain_accounting_slot_or_none),
                    });
                }
            } else if matches!(
                self.authority_class,
                AUTHORITY_INTENT_FUNDED | crate::AUTHORITY_EXACT_EXTERNAL_CREDIT
            ) {
                if self.domain_accounting_slot_or_none != NONE_INDEX
                    || self.accounted_before_or_zero != 0
                {
                    return Err(WireError::UnsupportedValue {
                        field: "protected required-domain predicate fields",
                        value: u64::from(self.domain_accounting_slot_or_none),
                    });
                }
            } else {
                return Err(WireError::UnsupportedValue {
                    field: "protected authority cannot carry domain predicate",
                    value: u64::from(self.authority_class),
                });
            }
        }
        if self.endpoint_key == [0; 32]
            || self.endpoint_owner == [0; 32]
            || self.asset_identity == [0; 32]
            || self.asset_program == [0; 32]
            || self.settlement_profile_digest == [0; 32]
            || self.lifecycle_digest == [0; 32]
        {
            return Err(WireError::UnsupportedValue {
                field: "protected capability digest identity",
                value: 0,
            });
        }
        Ok(())
    }
}

pub fn compute_protected_capability_set_digest(
    rows: &[ProtectedCapabilityDigestRowCandidateV0],
) -> WireResult<[u8; 32]> {
    require_count_limit(
        "protected capability digest rows",
        rows.len(),
        MAX_SETTLEMENT_CAPABILITIES,
    )?;
    for (position, row) in rows.iter().enumerate() {
        require_contiguous_index(
            "protected capability position",
            row.capability_position,
            position,
            rows.len(),
        )?;
    }
    let endpoint_keys = rows.iter().map(|row| row.endpoint_key).collect::<Vec<_>>();
    for (index, key) in endpoint_keys.iter().enumerate() {
        if endpoint_keys[..index].contains(key) {
            return Err(WireError::DuplicateIndex {
                field: "protected capability endpoint key",
                index: index as u8,
            });
        }
    }
    let encoded = rows
        .iter()
        .map(ProtectedCapabilityDigestRowCandidateV0::encode)
        .collect::<WireResult<Vec<_>>>()?;
    let slices = encoded.iter().map(|row| row.as_slice()).collect::<Vec<_>>();
    hash_protected_capability_set_rows(&slices)
}

pub const CLASSIC_SPL_ACCOUNT_STATE_UNINITIALIZED: u8 = 0;
pub const CLASSIC_SPL_ACCOUNT_STATE_INITIALIZED: u8 = 1;
pub const CLASSIC_SPL_ACCOUNT_STATE_FROZEN: u8 = 2;
pub const CLASSIC_SPL_TOKEN_PROGRAM_ID: Pubkey =
    solana_pubkey::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassicSplEndpointStateRowCandidateV0 {
    pub wire_version: u8,
    pub account_state: u8,
    pub delegate_present: bool,
    pub native_present: bool,
    pub close_authority_present: bool,
    pub endpoint_key: [u8; 32],
    pub token_program: [u8; 32],
    pub mint: [u8; 32],
    pub token_owner_authority: [u8; 32],
    pub delegate_or_zero: [u8; 32],
    pub close_authority_or_zero: [u8; 32],
    pub amount: u64,
    pub delegated_amount: u64,
    pub native_reserve_or_zero: u64,
}

impl ClassicSplEndpointStateRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; CLASSIC_SPL_ENDPOINT_STATE_ROW_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(CLASSIC_SPL_ENDPOINT_STATE_ROW_LEN);
        put_u8(&mut output, self.wire_version);
        put_u8(&mut output, self.account_state);
        put_u8(&mut output, u8::from(self.delegate_present));
        put_u8(&mut output, u8::from(self.native_present));
        put_u8(&mut output, u8::from(self.close_authority_present));
        put_bytes(&mut output, &[0; 3]);
        put_bytes(&mut output, &self.endpoint_key);
        put_bytes(&mut output, &self.token_program);
        put_bytes(&mut output, &self.mint);
        put_bytes(&mut output, &self.token_owner_authority);
        put_bytes(&mut output, &self.delegate_or_zero);
        put_bytes(&mut output, &self.close_authority_or_zero);
        put_u64(&mut output, self.amount);
        put_u64(&mut output, self.delegated_amount);
        put_u64(&mut output, self.native_reserve_or_zero);
        Ok(output
            .try_into()
            .expect("classic SPL endpoint state has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, CLASSIC_SPL_ENDPOINT_STATE_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let wire_version = reader.read_u8()?;
        let account_state = reader.read_u8()?;
        let delegate_present = decode_bool("classic SPL delegate present", reader.read_u8()?)?;
        let native_present = decode_bool("classic SPL native present", reader.read_u8()?)?;
        let close_authority_present =
            decode_bool("classic SPL close authority present", reader.read_u8()?)?;
        let reserved = reader.read_array::<3>()?;
        require_zero("classic SPL endpoint reserved", &reserved)?;
        let row = Self {
            wire_version,
            account_state,
            delegate_present,
            native_present,
            close_authority_present,
            endpoint_key: reader.read_array()?,
            token_program: reader.read_array()?,
            mint: reader.read_array()?,
            token_owner_authority: reader.read_array()?,
            delegate_or_zero: reader.read_array()?,
            close_authority_or_zero: reader.read_array()?,
            amount: reader.read_u64()?,
            delegated_amount: reader.read_u64()?,
            native_reserve_or_zero: reader.read_u64()?,
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
        if !matches!(
            self.account_state,
            CLASSIC_SPL_ACCOUNT_STATE_UNINITIALIZED
                | CLASSIC_SPL_ACCOUNT_STATE_INITIALIZED
                | CLASSIC_SPL_ACCOUNT_STATE_FROZEN
        ) {
            return Err(WireError::UnsupportedValue {
                field: "classic SPL account state",
                value: u64::from(self.account_state),
            });
        }
        if self.endpoint_key == [0; 32]
            || self.token_program != CLASSIC_SPL_TOKEN_PROGRAM_ID.to_bytes()
            || (self.account_state != CLASSIC_SPL_ACCOUNT_STATE_UNINITIALIZED
                && (self.mint == [0; 32] || self.token_owner_authority == [0; 32]))
        {
            return Err(WireError::UnsupportedValue {
                field: "classic SPL endpoint identity",
                value: 0,
            });
        }
        if (!self.delegate_present
            && (self.delegate_or_zero != [0; 32] || self.delegated_amount != 0))
            || (!self.close_authority_present && self.close_authority_or_zero != [0; 32])
            || (!self.native_present && self.native_reserve_or_zero != 0)
        {
            return Err(WireError::UnsupportedValue {
                field: "classic SPL optional state",
                value: 0,
            });
        }
        Ok(())
    }

    pub fn digest(&self) -> WireResult<[u8; 32]> {
        hash_private(LABEL_CLASSIC_SPL_ENDPOINT_STATE, &[&self.encode()?])
    }
}

pub fn compute_classic_spl_endpoint_state_digest(
    row: &ClassicSplEndpointStateRowCandidateV0,
) -> WireResult<[u8; 32]> {
    row.digest()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObservedProtectedDeltaRowCandidateV0 {
    pub capability_index: u8,
    pub before: u64,
    pub after: u64,
    pub gross_debit: u64,
    pub gross_credit: u64,
}

impl ObservedProtectedDeltaRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; OBSERVED_PROTECTED_DELTA_ROW_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(OBSERVED_PROTECTED_DELTA_ROW_LEN);
        put_u8(&mut output, self.capability_index);
        put_bytes(&mut output, &[0; 7]);
        put_u64(&mut output, self.before);
        put_u64(&mut output, self.after);
        put_u64(&mut output, self.gross_debit);
        put_u64(&mut output, self.gross_credit);
        Ok(output
            .try_into()
            .expect("observed protected delta has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, OBSERVED_PROTECTED_DELTA_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let capability_index = reader.read_u8()?;
        let reserved = reader.read_array::<7>()?;
        require_zero("observed protected delta reserved", &reserved)?;
        let row = Self {
            capability_index,
            before: reader.read_u64()?,
            after: reader.read_u64()?,
            gross_debit: reader.read_u64()?,
            gross_credit: reader.read_u64()?,
        };
        reader.finish()?;
        row.validate()?;
        Ok(row)
    }

    pub fn validate(&self) -> WireResult<()> {
        if usize::from(self.capability_index) >= MAX_SETTLEMENT_CAPABILITIES {
            return Err(WireError::InvalidIndex {
                field: "observed protected delta capability",
                index: self.capability_index,
                count: MAX_SETTLEMENT_CAPABILITIES as u8,
            });
        }
        if self.gross_debit == 0 && self.gross_credit == 0 {
            return Err(WireError::UnsupportedValue {
                field: "unchanged protected delta",
                value: 0,
            });
        }
        if self.gross_debit != 0 && self.gross_credit != 0 {
            return Err(WireError::UnsupportedValue {
                field: "simultaneous protected debit and credit",
                value: self.gross_debit,
            });
        }
        let expected_after = u128::from(self.before)
            .checked_add(u128::from(self.gross_credit))
            .and_then(|value| value.checked_sub(u128::from(self.gross_debit)))
            .ok_or(WireError::LengthOverflow)?;
        if expected_after != u128::from(self.after) {
            return Err(WireError::UnsupportedValue {
                field: "observed protected delta equation",
                value: self.after,
            });
        }
        Ok(())
    }
}

pub fn compute_observed_protected_delta_set_root(
    rows: &[ObservedProtectedDeltaRowCandidateV0],
) -> WireResult<[u8; 32]> {
    require_count_limit(
        "observed protected delta rows",
        rows.len(),
        MAX_SETTLEMENT_CAPABILITIES,
    )?;
    if rows
        .windows(2)
        .any(|pair| pair[0].capability_index >= pair[1].capability_index)
    {
        return Err(WireError::NonCanonicalOrder {
            field: "observed protected delta capability indices",
        });
    }
    let encoded = rows
        .iter()
        .map(ObservedProtectedDeltaRowCandidateV0::encode)
        .collect::<WireResult<Vec<_>>>()?;
    let slices = encoded.iter().map(|row| row.as_slice()).collect::<Vec<_>>();
    crate::hashes::hash_list(LABEL_OBSERVED_PROTECTED_DELTA_SET, &slices)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImmutableEngineReleaseObservationCandidateV0 {
    pub engine_program: [u8; 32],
    pub loader_program: [u8; 32],
    pub canonical_program_data: [u8; 32],
    pub captured_programdata_slot: u64,
    pub observed_controller_or_zero: [u8; 32],
    pub captured_programdata_data_len: u64,
    pub engine_admission_policy_digest: [u8; 32],
    pub loader_state_snapshot_digest: [u8; 32],
}

impl ImmutableEngineReleaseObservationCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; IMMUTABLE_ENGINE_RELEASE_OBSERVATION_ROW_LEN]> {
        if self.engine_program == [0; 32]
            || self.loader_program != crate::LOADER_V3_PROGRAM_ID.to_bytes()
            || self.canonical_program_data == [0; 32]
            || self.observed_controller_or_zero != [0; 32]
            || self.captured_programdata_data_len <= 45
        {
            return Err(WireError::UnsupportedValue {
                field: "immutable engine release observation",
                value: self.captured_programdata_data_len,
            });
        }
        let mut output = Vec::with_capacity(IMMUTABLE_ENGINE_RELEASE_OBSERVATION_ROW_LEN);
        put_bytes(&mut output, &self.engine_program);
        put_bytes(&mut output, &self.loader_program);
        put_bytes(&mut output, &self.canonical_program_data);
        put_u64(&mut output, self.captured_programdata_slot);
        put_bytes(&mut output, &self.observed_controller_or_zero);
        put_u64(&mut output, self.captured_programdata_data_len);
        put_bytes(&mut output, &self.engine_admission_policy_digest);
        put_bytes(&mut output, &self.loader_state_snapshot_digest);
        Ok(output
            .try_into()
            .expect("immutable engine release observation has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, IMMUTABLE_ENGINE_RELEASE_OBSERVATION_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let row = Self {
            engine_program: reader.read_array()?,
            loader_program: reader.read_array()?,
            canonical_program_data: reader.read_array()?,
            captured_programdata_slot: reader.read_u64()?,
            observed_controller_or_zero: reader.read_array()?,
            captured_programdata_data_len: reader.read_u64()?,
            engine_admission_policy_digest: reader.read_array()?,
            loader_state_snapshot_digest: reader.read_array()?,
        };
        reader.finish()?;
        row.encode()?;
        Ok(row)
    }

    pub fn digest(&self, core_program: &[u8; 32]) -> WireResult<[u8; 32]> {
        let major = CORE_EXPERIMENTAL_MAJOR.to_le_bytes();
        hash_private(
            crate::hashes::LABEL_IMMUTABLE_ENGINE_RELEASE_OBSERVATION,
            &[core_program, &major, &self.encode()?],
        )
    }
}

pub fn compute_immutable_engine_release_observation_digest(
    core_program: &[u8; 32],
    observation: &ImmutableEngineReleaseObservationCandidateV0,
) -> WireResult<[u8; 32]> {
    observation.digest(core_program)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreVerifiedEvidenceDigestInputs<'a> {
    pub core_program: &'a [u8; 32],
    pub market_binding_digest: &'a [u8; 32],
    pub loader_state_snapshot_digest: &'a [u8; 32],
    pub intent_set_digest: &'a [u8; 32],
    pub domain_set_digest: &'a [u8; 32],
    pub protected_execution_root: &'a [u8; 32],
    pub opaque_capability_root: &'a [u8; 32],
    pub request_digest: &'a [u8; 32],
    pub effect_digest: &'a [u8; 32],
    pub fee_assessment_set_root: &'a [u8; 32],
    pub observed_delta_root: &'a [u8; 32],
}

pub fn compute_core_verified_evidence_digest(
    inputs: CoreVerifiedEvidenceDigestInputs<'_>,
) -> WireResult<[u8; 32]> {
    let major = CORE_EXPERIMENTAL_MAJOR.to_le_bytes();
    hash_private(
        LABEL_CORE_VERIFIED_EVIDENCE,
        &[
            inputs.core_program,
            &major,
            inputs.market_binding_digest,
            inputs.loader_state_snapshot_digest,
            inputs.intent_set_digest,
            inputs.domain_set_digest,
            inputs.protected_execution_root,
            inputs.opaque_capability_root,
            inputs.request_digest,
            inputs.effect_digest,
            inputs.fee_assessment_set_root,
            inputs.observed_delta_root,
        ],
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineAttestedEvidenceDigestInputs<'a> {
    pub engine_program: &'a [u8; 32],
    pub engine_interface_id: &'a [u8; 32],
    pub engine_instance_id: &'a [u8; 32],
    pub request_digest: &'a [u8; 32],
    pub engine_supplied_digest: &'a [u8; 32],
}

pub fn compute_engine_attested_evidence_digest(
    inputs: EngineAttestedEvidenceDigestInputs<'_>,
) -> WireResult<[u8; 32]> {
    hash_private(
        LABEL_ENGINE_ATTESTED_EVIDENCE,
        &[
            inputs.engine_program,
            inputs.engine_interface_id,
            inputs.engine_instance_id,
            inputs.request_digest,
            inputs.engine_supplied_digest,
        ],
    )
}

fn decode_bool(field: &'static str, value: u8) -> WireResult<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(WireError::UnsupportedValue {
            field,
            value: u64::from(value),
        }),
    }
}

fn require_contiguous_index(
    field: &'static str,
    index: u8,
    expected: usize,
    count: usize,
) -> WireResult<()> {
    if usize::from(index) == expected {
        Ok(())
    } else {
        Err(WireError::InvalidIndex {
            field,
            index,
            count: u8::try_from(count).map_err(|_| WireError::LengthOverflow)?,
        })
    }
}

fn require_count_limit(field: &'static str, actual: usize, maximum: usize) -> WireResult<()> {
    if actual <= maximum {
        Ok(())
    } else {
        Err(WireError::LimitExceeded {
            field,
            maximum,
            actual,
        })
    }
}

fn require_optional_bounded_index(
    field: &'static str,
    index: u8,
    maximum: usize,
) -> WireResult<()> {
    if index == NONE_INDEX || usize::from(index) < maximum {
        Ok(())
    } else {
        Err(WireError::InvalidIndex {
            field,
            index,
            count: maximum as u8,
        })
    }
}

fn require_strictly_increasing<const N: usize>(
    field: &'static str,
    rows: &[[u8; N]],
) -> WireResult<()> {
    if rows.windows(2).any(|pair| pair[0] >= pair[1]) {
        Err(WireError::NonCanonicalOrder { field })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_row_lengths_round_trip_and_reject_reserved_mutations() {
        let market = MarketBindingRowCandidateV0 {
            core_program: [1; 32],
            core_experimental_major: CORE_EXPERIMENTAL_MAJOR,
            market_descriptor_key: [2; 32],
            market_descriptor_revision: 3,
            engine_program: [4; 32],
            engine_interface_id: [5; 32],
            engine_instance_id: [6; 32],
            engine_admission_policy_digest: [7; 32],
            domain_admission_profile_digest: [8; 32],
            protected_profile_digest: [9; 32],
            fee_policy_digest: [10; 32],
            opaque_schema_digest: [11; 32],
        };
        let encoded = market.encode().unwrap();
        assert_eq!(encoded.len(), 332);
        assert_eq!(
            MarketBindingRowCandidateV0::decode_exact(&encoded),
            Ok(market)
        );

        let fee_shard = FeeShardDigestRowCandidateV0 {
            shard_index: 0,
            asset_index: 0,
            vault_settlement_capability_index: 1,
            flags: 0,
            descriptor_key: [1; 32],
            descriptor_digest: [2; 32],
            liability_key: [3; 32],
            vault_key: [4; 32],
            asset_binding_digest: [5; 32],
            fee_policy_digest: [6; 32],
            recipient_policy_digest: [7; 32],
            fee_policy_revision: 8,
            liability_before: 9,
        };
        let mut encoded = fee_shard.encode().unwrap();
        assert_eq!(encoded.len(), 256);
        assert_eq!(
            FeeShardDigestRowCandidateV0::decode_exact(&encoded),
            Ok(fee_shard)
        );
        encoded[7] = 1;
        assert!(FeeShardDigestRowCandidateV0::decode_exact(&encoded).is_err());

        let capability_state = AuthorizationCapabilityStateRowCandidateV0 {
            local_term_index: 0,
            reserved_0: 0,
            flags: AUTHORIZATION_CAPABILITY_STATE_FLAG_FEE_FUNDING,
            initial_maximum_engine_debit: 80,
            initial_minimum_credit: 0,
            initial_maximum_total_debit: 100,
            remaining_total_debit: 70,
            cumulative_engine_debit: 20,
            cumulative_fee_debit: 10,
            cumulative_credit: 0,
        };
        let encoded = capability_state.encode().unwrap();
        assert_eq!(encoded.len(), 88);
        assert_eq!(
            AuthorizationCapabilityStateRowCandidateV0::decode_exact(&encoded),
            Ok(capability_state)
        );

        let credit_state = AuthorizationCapabilityStateRowCandidateV0 {
            local_term_index: 1,
            reserved_0: 0,
            flags: 0,
            initial_maximum_engine_debit: 0,
            initial_minimum_credit: 70,
            initial_maximum_total_debit: 0,
            remaining_total_debit: 0,
            cumulative_engine_debit: 0,
            cumulative_fee_debit: 0,
            cumulative_credit: 15,
        };
        assert_eq!(
            AuthorizationCapabilityStateRowCandidateV0::decode_exact(
                &credit_state.encode().unwrap()
            ),
            Ok(credit_state)
        );

        let fee_state = AuthorizationFeeStateRowCandidateV0 {
            rounding_group_digest: [12; 32],
            funding_local_term_index: 0,
            fee_class: 1,
            flags: 0,
            cumulative_basis: 100,
            cumulative_assessed_fee: 2,
            maximum_fee: 5,
        };
        let encoded = fee_state.encode().unwrap();
        assert_eq!(encoded.len(), 80);
        assert_eq!(
            AuthorizationFeeStateRowCandidateV0::decode_exact(&encoded),
            Ok(fee_state)
        );
    }

    #[test]
    fn typed_hashes_bind_major_prestates_and_part_boundaries() {
        let identity = InlineIntentIdentityRowCandidateV0 {
            actor: [1; 32],
            engine_terms_commitment: [2; 32],
            authorization_nonce: 3,
            expires_at_slot_exclusive: 4,
        };
        let base = IntentDigestInputs {
            core_program: &[5; 32],
            market_binding_digest: &[6; 32],
            loader_state_snapshot_digest: &[7; 32],
            fee_policy_digest: &[8; 32],
            identity: &identity,
            core_terms_root: &[9; 32],
        };
        let digest = compute_intent_digest(base).unwrap();
        let mut changed_identity = identity;
        changed_identity.authorization_nonce += 1;
        let changed = compute_intent_digest(IntentDigestInputs {
            identity: &changed_identity,
            ..base
        })
        .unwrap();
        assert_ne!(digest, changed);

        let before = compute_authorization_state_digest(AuthorizationStateDigestInputs {
            intent_digest: &[1; 32],
            lifecycle: AUTHORIZATION_LIFECYCLE_ACTIVE,
            fill_sequence: 2,
            successful_fills: 3,
            remaining_fills: 4,
            capability_state_root: &[5; 32],
            fee_state_root: &[6; 32],
            stored_authorization_key_or_zero: &[7; 32],
        })
        .unwrap();
        let after = compute_authorization_state_digest(AuthorizationStateDigestInputs {
            fill_sequence: 3,
            intent_digest: &[1; 32],
            lifecycle: AUTHORIZATION_LIFECYCLE_ACTIVE,
            successful_fills: 3,
            remaining_fills: 4,
            capability_state_root: &[5; 32],
            fee_state_root: &[6; 32],
            stored_authorization_key_or_zero: &[7; 32],
        })
        .unwrap();
        assert_ne!(before, after);
    }

    #[test]
    fn list_helpers_reject_duplicates_and_reordering() {
        let duplicate = [IntentSetRowCandidateV0 {
            intent_digest: [1; 32],
        }; 2];
        assert!(matches!(
            compute_intent_set_digest(&[9; 32], &duplicate),
            Err(WireError::NonCanonicalOrder { .. })
        ));
        assert!(compute_intent_debit_group_root(&[0, 2, 1]).is_err());
        assert!(compute_intent_debit_group_root(&[0, 0]).is_err());
    }

    #[test]
    fn market_binding_digest_matches_frozen_golden_vector() {
        let row = MarketBindingRowCandidateV0 {
            core_program: [1; 32],
            core_experimental_major: CORE_EXPERIMENTAL_MAJOR,
            market_descriptor_key: [2; 32],
            market_descriptor_revision: 3,
            engine_program: [4; 32],
            engine_interface_id: [5; 32],
            engine_instance_id: [6; 32],
            engine_admission_policy_digest: [7; 32],
            domain_admission_profile_digest: [8; 32],
            protected_profile_digest: [9; 32],
            fee_policy_digest: [10; 32],
            opaque_schema_digest: [11; 32],
        };
        assert_eq!(
            row.digest().unwrap(),
            [
                247, 59, 219, 56, 20, 62, 231, 14, 93, 130, 140, 62, 64, 116, 166, 248, 149, 17,
                112, 176, 97, 247, 192, 235, 148, 171, 226, 121, 243, 191, 70, 65,
            ]
        );
        for invalid in [
            MarketBindingRowCandidateV0 {
                engine_interface_id: [0; 32],
                ..row
            },
            MarketBindingRowCandidateV0 {
                engine_instance_id: [0; 32],
                ..row
            },
            MarketBindingRowCandidateV0 {
                engine_admission_policy_digest: [0; 32],
                ..row
            },
            MarketBindingRowCandidateV0 {
                domain_admission_profile_digest: [0; 32],
                ..row
            },
            MarketBindingRowCandidateV0 {
                protected_profile_digest: [0; 32],
                ..row
            },
            MarketBindingRowCandidateV0 {
                fee_policy_digest: [0; 32],
                ..row
            },
            MarketBindingRowCandidateV0 {
                opaque_schema_digest: [0; 32],
                ..row
            },
        ] {
            assert!(invalid.encode().is_err());
        }
    }

    #[test]
    fn intent_set_digest_matches_domain_bound_golden_vector() {
        let rows = [
            IntentSetRowCandidateV0 {
                intent_digest: [1; 32],
            },
            IntentSetRowCandidateV0 {
                intent_digest: [2; 32],
            },
        ];
        assert_eq!(
            compute_intent_set_digest(&[9; 32], &rows).unwrap(),
            [
                211, 238, 214, 106, 223, 79, 125, 147, 165, 237, 220, 233, 70, 17, 195, 126, 223,
                63, 238, 190, 30, 181, 24, 162, 146, 237, 34, 7, 85, 232, 229, 224,
            ]
        );
    }

    #[test]
    fn typed_descriptor_lifecycle_and_delta_rows_are_exact() {
        let descriptor = DomainDescriptorRowCandidateV0 {
            wire_version: WIRE_VERSION,
            rule_kind: 0,
            controller_program: [1; 32],
            controller_identity: [2; 32],
            domain_revision: 3,
            namespace_or_instance: [4; 32],
            custody_profile_digest: [5; 32],
            asset_profile_digest: [6; 32],
            accounting_profile_digest: [7; 32],
            exit_class_digest: [8; 32],
            admission_rule_digest: crate::compute_open_domain_rule_digest().unwrap(),
            protected_profile_digest: [10; 32],
        };
        let mut encoded = descriptor.encode().unwrap();
        assert_eq!(encoded.len(), 304);
        assert_eq!(
            DomainDescriptorRowCandidateV0::decode_exact(&encoded),
            Ok(descriptor)
        );
        for invalid in [
            DomainDescriptorRowCandidateV0 {
                custody_profile_digest: [0; 32],
                ..descriptor
            },
            DomainDescriptorRowCandidateV0 {
                asset_profile_digest: [0; 32],
                ..descriptor
            },
            DomainDescriptorRowCandidateV0 {
                exit_class_digest: [0; 32],
                ..descriptor
            },
        ] {
            assert!(invalid.encode().is_err());
        }
        encoded[7] = 1;
        assert!(DomainDescriptorRowCandidateV0::decode_exact(&encoded).is_err());

        let endpoint = ClassicSplEndpointStateRowCandidateV0 {
            wire_version: WIRE_VERSION,
            account_state: CLASSIC_SPL_ACCOUNT_STATE_INITIALIZED,
            delegate_present: true,
            native_present: false,
            close_authority_present: false,
            endpoint_key: [11; 32],
            token_program: CLASSIC_SPL_TOKEN_PROGRAM_ID.to_bytes(),
            mint: [13; 32],
            token_owner_authority: [14; 32],
            delegate_or_zero: [15; 32],
            close_authority_or_zero: [0; 32],
            amount: 16,
            delegated_amount: 17,
            native_reserve_or_zero: 0,
        };
        let mut encoded = endpoint.encode().unwrap();
        assert_eq!(encoded.len(), 224);
        assert_eq!(
            ClassicSplEndpointStateRowCandidateV0::decode_exact(&encoded),
            Ok(endpoint)
        );
        encoded[2] = 2;
        assert!(ClassicSplEndpointStateRowCandidateV0::decode_exact(&encoded).is_err());
        let uninitialized = ClassicSplEndpointStateRowCandidateV0 {
            account_state: CLASSIC_SPL_ACCOUNT_STATE_UNINITIALIZED,
            delegate_present: false,
            mint: [0; 32],
            token_owner_authority: [0; 32],
            delegate_or_zero: [0; 32],
            amount: 0,
            delegated_amount: 0,
            ..endpoint
        };
        assert!(uninitialized.encode().is_ok());
        let frozen = ClassicSplEndpointStateRowCandidateV0 {
            account_state: CLASSIC_SPL_ACCOUNT_STATE_FROZEN,
            ..endpoint
        };
        assert!(frozen.encode().is_ok());
        let mut unknown = endpoint.encode().unwrap();
        unknown[1] = 3;
        assert!(ClassicSplEndpointStateRowCandidateV0::decode_exact(&unknown).is_err());

        let delta = ObservedProtectedDeltaRowCandidateV0 {
            capability_index: 2,
            before: 20,
            after: 15,
            gross_debit: 5,
            gross_credit: 0,
        };
        let encoded = delta.encode().unwrap();
        assert_eq!(encoded.len(), 40);
        assert_eq!(
            ObservedProtectedDeltaRowCandidateV0::decode_exact(&encoded),
            Ok(delta)
        );
        let mut impossible = delta;
        impossible.after = 16;
        assert!(impossible.encode().is_err());
    }
}
