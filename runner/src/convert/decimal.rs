use crate::{convert::required::IsRequired, generated::dto::DecimalDto};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use stock_trek::errors::{StockTrekError, StockTrekResult, ValueError};

pub trait ToDecimal {
    fn to_decimal(self) -> StockTrekResult<Decimal>;
}

impl ToDecimal for DecimalDto {
    fn to_decimal(self) -> StockTrekResult<Decimal> {
        if self.scale > Decimal::MAX_SCALE {
            return Err(StockTrekError::Value(ValueError::IncorrectType {
                expected: format!("scale <= {}", Decimal::MAX_SCALE),
                found: self.scale.to_string(),
            }));
        }
        Ok(Decimal::from_parts(
            self.lo,
            self.mid,
            self.hi,
            self.negative,
            self.scale,
        ))
    }
}

impl TryFrom<DecimalDto> for Decimal {
    type Error = StockTrekError;
    fn try_from(value: DecimalDto) -> Result<Self, Self::Error> {
        value.to_decimal()
    }
}

impl TryFrom<DecimalDto> for f64 {
    type Error = StockTrekError;
    fn try_from(value: DecimalDto) -> Result<Self, Self::Error> {
        value.to_decimal()?.to_f64().required()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dto(lo: u32, mid: u32, hi: u32, negative: bool, scale: u32) -> DecimalDto {
        DecimalDto {
            lo,
            mid,
            hi,
            scale,
            negative,
        }
    }

    #[test]
    fn converts_valid_decimal() {
        assert_eq!(
            dto(3141, 0, 0, false, 3).to_decimal().unwrap(),
            Decimal::new(3141, 3)
        );
    }

    #[test]
    fn converts_negative_decimal() {
        assert_eq!(
            dto(3141, 0, 0, true, 3).to_decimal().unwrap(),
            Decimal::new(-3141, 3)
        );
    }

    #[test]
    fn accepts_max_scale() {
        assert!(dto(1, 0, 0, false, Decimal::MAX_SCALE).to_decimal().is_ok());
    }

    #[test]
    fn rejects_scale_above_max() {
        assert!(
            dto(1, 0, 0, false, Decimal::MAX_SCALE + 1)
                .to_decimal()
                .is_err()
        );
    }
}
