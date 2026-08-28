use anchor_lang::prelude::*;

use crate::{constants::BASIS_POINTS_DENOMINATOR, error::CoreError};

pub fn fee_ceil(amount: u64, fee_bps: u16) -> Result<u64> {
    require!(amount > 0, CoreError::ZeroAmount);
    require!(fee_bps > 0, CoreError::ArithmeticOverflow);

    let numerator = u128::from(amount)
        .checked_mul(u128::from(fee_bps))
        .and_then(|value| value.checked_add(BASIS_POINTS_DENOMINATOR - 1))
        .ok_or(CoreError::ArithmeticOverflow)?;
    let fee = numerator
        .checked_div(BASIS_POINTS_DENOMINATOR)
        .ok_or(CoreError::ArithmeticOverflow)?;

    u64::try_from(fee).map_err(|_| error!(CoreError::IntegerConversionFailed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_dust_up_to_one_unit() {
        assert_eq!(fee_ceil(1, 30).unwrap(), 1);
        assert_eq!(fee_ceil(333, 30).unwrap(), 1);
        assert_eq!(fee_ceil(334, 30).unwrap(), 2);
    }

    #[test]
    fn computes_exact_multiples_without_extra_unit() {
        assert_eq!(fee_ceil(10_000, 30).unwrap(), 30);
        assert_eq!(fee_ceil(1_000_000, 30).unwrap(), 3_000);
    }

    #[test]
    fn rejects_zero_amount_and_overflow() {
        assert!(fee_ceil(0, 30).is_err());
        assert!(fee_ceil(u64::MAX, u16::MAX).is_err());
    }
}
