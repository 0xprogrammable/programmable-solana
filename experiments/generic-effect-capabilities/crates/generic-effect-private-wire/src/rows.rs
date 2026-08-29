use alloc::vec::Vec;

use crate::codec::{
    put_bytes, put_u16, put_u32, put_u64, put_u8, require_exact_length, require_zero, Reader,
};
use crate::{WireError, WireResult, MAX_ASSETS, MAX_SETTLEMENT_CAPABILITIES, NONE_INDEX};

pub const DOMAIN_CONTROL_ROW_LEN: usize = 8;
pub const AUTHORIZATION_SNAPSHOT_ROW_LEN: usize = 8;
pub const INLINE_INTENT_IDENTITY_ROW_LEN: usize = 80;
pub const FEE_SHARD_ROW_LEN: usize = 8;
pub const SETTLEMENT_CAPABILITY_ROW_LEN: usize = 48;

pub const WITNESS_DIRECT_ACTOR: u8 = 0;
pub const WITNESS_EXACT_DELEGATE: u8 = 1;
pub const WITNESS_STORED_AUTHORIZATION: u8 = 2;

pub const AUTHORITY_INTENT_FUNDED: u8 = 0;
pub const AUTHORITY_DOMAIN_ACCOUNTED: u8 = 1;
pub const AUTHORITY_EXACT_EXTERNAL_CREDIT: u8 = 2;
pub const AUTHORITY_CORE_RESERVED_FEE: u8 = 3;

pub const RIGHT_DEBIT: u16 = 1 << 0;
pub const RIGHT_CREDIT: u16 = 1 << 1;
pub const RIGHT_DOMAIN_ACCOUNTED: u16 = 1 << 2;
pub const RIGHT_EXACT_EXTERNAL_RECIPIENT: u16 = 1 << 3;
pub const RIGHT_CORE_RESERVED_FEE: u16 = 1 << 4;
pub const SETTLEMENT_RIGHTS_MASK: u16 = RIGHT_DEBIT
    | RIGHT_CREDIT
    | RIGHT_DOMAIN_ACCOUNTED
    | RIGHT_EXACT_EXTERNAL_RECIPIENT
    | RIGHT_CORE_RESERVED_FEE;
pub const ENGINE_CONTEXT_RIGHTS_MASK: u16 =
    RIGHT_DEBIT | RIGHT_CREDIT | RIGHT_DOMAIN_ACCOUNTED | RIGHT_EXACT_EXTERNAL_RECIPIENT;

pub const FEE_CLASS_NONE: u8 = 0;
pub const FEE_CLASS_GROSS_DEBIT_RATE: u8 = 1;
pub const SETTLEMENT_FLAG_FEE_FUNDING: u8 = 1 << 0;
pub const SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT: u8 = 1 << 1;
pub const SETTLEMENT_FLAGS_MASK: u8 =
    SETTLEMENT_FLAG_FEE_FUNDING | SETTLEMENT_FLAG_ALLOW_UNCONSTRAINED_STORED_DEBIT;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DomainControlRowCandidateV0 {
    pub descriptor_control_offset: u8,
    pub admission_control_offset_or_none: u8,
    pub accounting_control_offset: u8,
    pub flags: u8,
}

impl DomainControlRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; DOMAIN_CONTROL_ROW_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(DOMAIN_CONTROL_ROW_LEN);
        put_u8(&mut output, self.descriptor_control_offset);
        put_u8(&mut output, self.admission_control_offset_or_none);
        put_u8(&mut output, self.accounting_control_offset);
        put_u8(&mut output, self.flags);
        put_bytes(&mut output, &[0; 4]);
        Ok(output
            .try_into()
            .expect("domain control row has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, DOMAIN_CONTROL_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let row = Self {
            descriptor_control_offset: reader.read_u8()?,
            admission_control_offset_or_none: reader.read_u8()?,
            accounting_control_offset: reader.read_u8()?,
            flags: reader.read_u8()?,
        };
        let reserved = reader.read_array::<4>()?;
        reader.finish()?;
        require_zero("domain control row reserved", &reserved)?;
        row.validate()?;
        Ok(row)
    }

    pub fn validate(&self) -> WireResult<()> {
        require_flags_zero("domain control row flags", self.flags)
    }

    pub fn offsets(&self) -> [u8; 3] {
        [
            self.descriptor_control_offset,
            self.admission_control_offset_or_none,
            self.accounting_control_offset,
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthorizationSnapshotRowCandidateV0 {
    pub authorization_slot: u8,
    pub witness_kind: u8,
    pub authorization_control_offset_or_none: u8,
    pub inline_identity_index_or_none: u8,
    pub expected_fill_sequence: u32,
}

impl AuthorizationSnapshotRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; AUTHORIZATION_SNAPSHOT_ROW_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(AUTHORIZATION_SNAPSHOT_ROW_LEN);
        put_u8(&mut output, self.authorization_slot);
        put_u8(&mut output, self.witness_kind);
        put_u8(&mut output, self.authorization_control_offset_or_none);
        put_u8(&mut output, self.inline_identity_index_or_none);
        put_u32(&mut output, self.expected_fill_sequence);
        Ok(output
            .try_into()
            .expect("authorization snapshot row has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, AUTHORIZATION_SNAPSHOT_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let row = Self {
            authorization_slot: reader.read_u8()?,
            witness_kind: reader.read_u8()?,
            authorization_control_offset_or_none: reader.read_u8()?,
            inline_identity_index_or_none: reader.read_u8()?,
            expected_fill_sequence: reader.read_u32()?,
        };
        reader.finish()?;
        row.validate()?;
        Ok(row)
    }

    pub fn validate(&self) -> WireResult<()> {
        if !matches!(
            self.witness_kind,
            WITNESS_DIRECT_ACTOR | WITNESS_EXACT_DELEGATE | WITNESS_STORED_AUTHORIZATION
        ) {
            return Err(WireError::UnsupportedValue {
                field: "intent witness kind",
                value: u64::from(self.witness_kind),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlineIntentIdentityRowCandidateV0 {
    pub actor: [u8; 32],
    pub engine_terms_commitment: [u8; 32],
    pub authorization_nonce: u64,
    pub expires_at_slot_exclusive: u64,
}

impl InlineIntentIdentityRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; INLINE_INTENT_IDENTITY_ROW_LEN]> {
        if self.engine_terms_commitment == [0; 32] {
            return Err(WireError::UnsupportedValue {
                field: "inline engine terms commitment",
                value: 0,
            });
        }
        let mut output = Vec::with_capacity(INLINE_INTENT_IDENTITY_ROW_LEN);
        put_bytes(&mut output, &self.actor);
        put_bytes(&mut output, &self.engine_terms_commitment);
        put_u64(&mut output, self.authorization_nonce);
        put_u64(&mut output, self.expires_at_slot_exclusive);
        Ok(output
            .try_into()
            .expect("inline identity row has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, INLINE_INTENT_IDENTITY_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let row = Self {
            actor: reader.read_array()?,
            engine_terms_commitment: reader.read_array()?,
            authorization_nonce: reader.read_u64()?,
            expires_at_slot_exclusive: reader.read_u64()?,
        };
        reader.finish()?;
        if row.engine_terms_commitment == [0; 32] {
            return Err(WireError::UnsupportedValue {
                field: "inline engine terms commitment",
                value: 0,
            });
        }
        Ok(row)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeeShardRowCandidateV0 {
    pub descriptor_control_offset: u8,
    pub liability_control_offset: u8,
    pub vault_settlement_capability_index: u8,
    pub asset_index: u8,
    pub flags: u8,
}

impl FeeShardRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; FEE_SHARD_ROW_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(FEE_SHARD_ROW_LEN);
        put_u8(&mut output, self.descriptor_control_offset);
        put_u8(&mut output, self.liability_control_offset);
        put_u8(&mut output, self.vault_settlement_capability_index);
        put_u8(&mut output, self.asset_index);
        put_u8(&mut output, self.flags);
        put_bytes(&mut output, &[0; 3]);
        Ok(output
            .try_into()
            .expect("fee shard row has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, FEE_SHARD_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let row = Self {
            descriptor_control_offset: reader.read_u8()?,
            liability_control_offset: reader.read_u8()?,
            vault_settlement_capability_index: reader.read_u8()?,
            asset_index: reader.read_u8()?,
            flags: reader.read_u8()?,
        };
        let reserved = reader.read_array::<3>()?;
        reader.finish()?;
        require_zero("fee shard row reserved", &reserved)?;
        row.validate()?;
        Ok(row)
    }

    pub fn validate(&self) -> WireResult<()> {
        require_flags_zero("fee shard row flags", self.flags)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettlementCapabilityRowCandidateV0 {
    pub asset_index: u8,
    pub domain_index_or_none: u8,
    pub authorization_slot_or_none: u8,
    pub intent_local_term_index_or_none: u8,
    pub authority_class: u8,
    pub fee_shard_index_or_none: u8,
    pub fee_class: u8,
    pub flags: u8,
    pub rights_bits: u16,
    pub domain_accounting_slot_or_none: u8,
    pub spend_authority_control_offset_or_none: u8,
    pub reserved_0: u8,
    pub maximum_engine_debit: u64,
    pub maximum_total_debit: u64,
    pub minimum_credit: u64,
    pub maximum_protocol_fee: u64,
}

impl SettlementCapabilityRowCandidateV0 {
    pub fn encode(&self) -> WireResult<[u8; SETTLEMENT_CAPABILITY_ROW_LEN]> {
        self.validate()?;
        let mut output = Vec::with_capacity(SETTLEMENT_CAPABILITY_ROW_LEN);
        put_u8(&mut output, self.asset_index);
        put_u8(&mut output, self.domain_index_or_none);
        put_u8(&mut output, self.authorization_slot_or_none);
        put_u8(&mut output, self.intent_local_term_index_or_none);
        put_u8(&mut output, self.authority_class);
        put_u8(&mut output, self.fee_shard_index_or_none);
        put_u8(&mut output, self.fee_class);
        put_u8(&mut output, self.flags);
        put_u16(&mut output, self.rights_bits);
        put_u8(&mut output, self.domain_accounting_slot_or_none);
        put_u8(&mut output, self.spend_authority_control_offset_or_none);
        put_u8(&mut output, self.reserved_0);
        put_bytes(&mut output, &[0; 3]);
        put_u64(&mut output, self.maximum_engine_debit);
        put_u64(&mut output, self.maximum_total_debit);
        put_u64(&mut output, self.minimum_credit);
        put_u64(&mut output, self.maximum_protocol_fee);
        Ok(output
            .try_into()
            .expect("settlement row has a fixed encoded length"))
    }

    pub fn decode_exact(data: &[u8]) -> WireResult<Self> {
        require_exact_length(data, SETTLEMENT_CAPABILITY_ROW_LEN)?;
        let mut reader = Reader::new(data);
        let row = Self {
            asset_index: reader.read_u8()?,
            domain_index_or_none: reader.read_u8()?,
            authorization_slot_or_none: reader.read_u8()?,
            intent_local_term_index_or_none: reader.read_u8()?,
            authority_class: reader.read_u8()?,
            fee_shard_index_or_none: reader.read_u8()?,
            fee_class: reader.read_u8()?,
            flags: reader.read_u8()?,
            rights_bits: reader.read_u16()?,
            domain_accounting_slot_or_none: reader.read_u8()?,
            spend_authority_control_offset_or_none: reader.read_u8()?,
            reserved_0: reader.read_u8()?,
            maximum_engine_debit: {
                let reserved = reader.read_array::<3>()?;
                require_zero("settlement capability row reserved", &reserved)?;
                reader.read_u64()?
            },
            maximum_total_debit: reader.read_u64()?,
            minimum_credit: reader.read_u64()?,
            maximum_protocol_fee: reader.read_u64()?,
        };
        reader.finish()?;
        row.validate()?;
        Ok(row)
    }

    pub fn validate(&self) -> WireResult<()> {
        if self.flags & !SETTLEMENT_FLAGS_MASK != 0 {
            return Err(WireError::UnknownFlags {
                field: "settlement capability row flags",
                value: u64::from(self.flags),
            });
        }
        if self.rights_bits == 0 || self.rights_bits & !SETTLEMENT_RIGHTS_MASK != 0 {
            return Err(WireError::UnknownFlags {
                field: "settlement capability rights",
                value: u64::from(self.rights_bits),
            });
        }
        if self.authority_class > AUTHORITY_CORE_RESERVED_FEE {
            return Err(WireError::UnsupportedValue {
                field: "settlement authority class",
                value: u64::from(self.authority_class),
            });
        }
        if self.fee_class > FEE_CLASS_GROSS_DEBIT_RATE {
            return Err(WireError::UnsupportedValue {
                field: "settlement fee class",
                value: u64::from(self.fee_class),
            });
        }
        if self.reserved_0 != 0 {
            return Err(WireError::NonZeroReserved {
                field: "settlement capability reserved_0",
            });
        }
        Ok(())
    }

    pub fn validate_indices(
        &self,
        asset_count: u8,
        domain_count: u8,
        intent_count: u8,
        fee_shard_count: u8,
    ) -> WireResult<()> {
        require_present_index("settlement asset index", self.asset_index, asset_count)?;
        require_optional_index(
            "settlement domain index",
            self.domain_index_or_none,
            domain_count,
        )?;
        require_optional_index(
            "settlement authorization slot",
            self.authorization_slot_or_none,
            intent_count,
        )?;
        require_optional_index(
            "settlement intent local term index",
            self.intent_local_term_index_or_none,
            MAX_SETTLEMENT_CAPABILITIES as u8,
        )?;
        require_optional_index(
            "settlement domain accounting slot",
            self.domain_accounting_slot_or_none,
            MAX_ASSETS as u8,
        )?;
        require_optional_index(
            "settlement fee shard index",
            self.fee_shard_index_or_none,
            fee_shard_count,
        )?;
        Ok(())
    }
}

pub(crate) fn require_flags_zero(field: &'static str, flags: u8) -> WireResult<()> {
    if flags == 0 {
        Ok(())
    } else {
        Err(WireError::UnknownFlags {
            field,
            value: u64::from(flags),
        })
    }
}

pub(crate) fn require_present_index(field: &'static str, index: u8, count: u8) -> WireResult<()> {
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

pub(crate) fn require_optional_index(field: &'static str, index: u8, count: u8) -> WireResult<()> {
    if index == NONE_INDEX || index < count {
        Ok(())
    } else {
        Err(WireError::InvalidIndex {
            field,
            index,
            count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_row_lengths_are_frozen() {
        assert_eq!(
            DomainControlRowCandidateV0 {
                descriptor_control_offset: 0,
                admission_control_offset_or_none: NONE_INDEX,
                accounting_control_offset: 1,
                flags: 0,
            }
            .encode()
            .unwrap()
            .len(),
            8
        );
        assert_eq!(
            AuthorizationSnapshotRowCandidateV0 {
                authorization_slot: 0,
                witness_kind: WITNESS_DIRECT_ACTOR,
                authorization_control_offset_or_none: 0,
                inline_identity_index_or_none: 0,
                expected_fill_sequence: 0,
            }
            .encode()
            .unwrap()
            .len(),
            8
        );
        assert_eq!(
            InlineIntentIdentityRowCandidateV0 {
                actor: [2; 32],
                engine_terms_commitment: [3; 32],
                authorization_nonce: 1,
                expires_at_slot_exclusive: 10,
            }
            .encode()
            .unwrap()
            .len(),
            80
        );
        assert_eq!(
            FeeShardRowCandidateV0 {
                descriptor_control_offset: 0,
                liability_control_offset: 1,
                vault_settlement_capability_index: 2,
                asset_index: 0,
                flags: 0,
            }
            .encode()
            .unwrap()
            .len(),
            8
        );
        assert_eq!(
            SettlementCapabilityRowCandidateV0 {
                asset_index: 0,
                domain_index_or_none: NONE_INDEX,
                authorization_slot_or_none: 0,
                intent_local_term_index_or_none: 0,
                authority_class: AUTHORITY_INTENT_FUNDED,
                fee_shard_index_or_none: NONE_INDEX,
                rights_bits: RIGHT_DEBIT,
                fee_class: FEE_CLASS_GROSS_DEBIT_RATE,
                flags: 0,
                domain_accounting_slot_or_none: NONE_INDEX,
                spend_authority_control_offset_or_none: 0,
                reserved_0: 0,
                maximum_engine_debit: 20,
                maximum_total_debit: 21,
                minimum_credit: 0,
                maximum_protocol_fee: 1,
            }
            .encode()
            .unwrap()
            .len(),
            48
        );
    }

    #[test]
    fn rows_reject_padding_flags_unknown_rights_and_trailing_bytes() {
        let row = FeeShardRowCandidateV0 {
            descriptor_control_offset: 0,
            liability_control_offset: 1,
            vault_settlement_capability_index: 2,
            asset_index: 0,
            flags: 0,
        };
        let mut encoded = row.encode().unwrap().to_vec();
        encoded[7] = 1;
        assert!(FeeShardRowCandidateV0::decode_exact(&encoded).is_err());
        encoded = row.encode().unwrap().to_vec();
        encoded.push(0);
        assert!(FeeShardRowCandidateV0::decode_exact(&encoded).is_err());

        let mut settlement = SettlementCapabilityRowCandidateV0 {
            asset_index: 0,
            domain_index_or_none: NONE_INDEX,
            authorization_slot_or_none: 0,
            intent_local_term_index_or_none: 0,
            authority_class: AUTHORITY_INTENT_FUNDED,
            fee_shard_index_or_none: NONE_INDEX,
            rights_bits: RIGHT_DEBIT,
            fee_class: FEE_CLASS_NONE,
            flags: 0,
            domain_accounting_slot_or_none: NONE_INDEX,
            spend_authority_control_offset_or_none: 0,
            reserved_0: 0,
            maximum_engine_debit: 1,
            maximum_total_debit: 1,
            minimum_credit: 0,
            maximum_protocol_fee: 0,
        };
        settlement.rights_bits = 1 << 15;
        assert!(settlement.encode().is_err());
    }
}
