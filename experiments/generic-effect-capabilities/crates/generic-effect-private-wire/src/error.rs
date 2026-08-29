//! Fail-closed errors for the private candidate codec.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WireError {
    InvalidLength {
        expected: usize,
        actual: usize,
    },
    LengthOverflow,
    UnsupportedVersion {
        expected: u8,
        actual: u8,
    },
    InvalidDiscriminator,
    InvalidMagic,
    NonZeroReserved {
        field: &'static str,
    },
    UnknownFlags {
        field: &'static str,
        value: u64,
    },
    UnsupportedValue {
        field: &'static str,
        value: u64,
    },
    LimitExceeded {
        field: &'static str,
        maximum: usize,
        actual: usize,
    },
    InvalidIndex {
        field: &'static str,
        index: u8,
        count: u8,
    },
    DuplicateIndex {
        field: &'static str,
        index: u8,
    },
    MissingIndex {
        field: &'static str,
        index: u8,
    },
    NonCanonicalOrder {
        field: &'static str,
    },
    InvalidLabel,
    DigestMismatch {
        field: &'static str,
    },
}

pub type WireResult<T> = core::result::Result<T, WireError>;
