use alloc::vec::Vec;

use crate::{WireError, WireResult};

pub(crate) struct Reader<'a> {
    data: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    pub(crate) const fn new(data: &'a [u8]) -> Self {
        Self { data, cursor: 0 }
    }

    pub(crate) fn read_u8(&mut self) -> WireResult<u8> {
        Ok(self.read_array::<1>()?[0])
    }

    pub(crate) fn read_u16(&mut self) -> WireResult<u16> {
        Ok(u16::from_le_bytes(self.read_array()?))
    }

    pub(crate) fn read_u32(&mut self) -> WireResult<u32> {
        Ok(u32::from_le_bytes(self.read_array()?))
    }

    pub(crate) fn read_u64(&mut self) -> WireResult<u64> {
        Ok(u64::from_le_bytes(self.read_array()?))
    }

    pub(crate) fn read_u128(&mut self) -> WireResult<u128> {
        Ok(u128::from_le_bytes(self.read_array()?))
    }

    pub(crate) fn read_array<const N: usize>(&mut self) -> WireResult<[u8; N]> {
        let end = self
            .cursor
            .checked_add(N)
            .ok_or(WireError::LengthOverflow)?;
        let source = self
            .data
            .get(self.cursor..end)
            .ok_or(WireError::InvalidLength {
                expected: end,
                actual: self.data.len(),
            })?;
        let mut value = [0_u8; N];
        value.copy_from_slice(source);
        self.cursor = end;
        Ok(value)
    }

    pub(crate) fn read_vec(&mut self, length: usize) -> WireResult<Vec<u8>> {
        let end = self
            .cursor
            .checked_add(length)
            .ok_or(WireError::LengthOverflow)?;
        let value = self
            .data
            .get(self.cursor..end)
            .ok_or(WireError::InvalidLength {
                expected: end,
                actual: self.data.len(),
            })?
            .to_vec();
        self.cursor = end;
        Ok(value)
    }

    pub(crate) fn finish(self) -> WireResult<()> {
        if self.cursor == self.data.len() {
            Ok(())
        } else {
            Err(WireError::InvalidLength {
                expected: self.cursor,
                actual: self.data.len(),
            })
        }
    }
}

pub(crate) fn put_u8(output: &mut Vec<u8>, value: u8) {
    output.push(value);
}

pub(crate) fn put_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn put_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn put_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn put_u128(output: &mut Vec<u8>, value: u128) {
    output.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn put_bytes(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(value);
}

pub(crate) fn require_exact_length(data: &[u8], expected: usize) -> WireResult<()> {
    if data.len() == expected {
        Ok(())
    } else {
        Err(WireError::InvalidLength {
            expected,
            actual: data.len(),
        })
    }
}

pub(crate) fn require_zero(field: &'static str, bytes: &[u8]) -> WireResult<()> {
    if bytes.iter().all(|byte| *byte == 0) {
        Ok(())
    } else {
        Err(WireError::NonZeroReserved { field })
    }
}

pub(crate) fn checked_encoded_length(base: usize, terms: &[(usize, usize)]) -> WireResult<usize> {
    terms.iter().try_fold(base, |total, (count, width)| {
        let term = count.checked_mul(*width).ok_or(WireError::LengthOverflow)?;
        total.checked_add(term).ok_or(WireError::LengthOverflow)
    })
}

pub(crate) fn checked_u16(value: usize) -> WireResult<u16> {
    u16::try_from(value).map_err(|_| WireError::LengthOverflow)
}

pub(crate) fn checked_u32(value: usize) -> WireResult<u32> {
    u32::try_from(value).map_err(|_| WireError::LengthOverflow)
}
