use crate::generated::dto::MarketCandlesDto;
use std::collections::HashMap;
use stock_trek::{
    errors::{StockTrekError, ValueError},
    markets::{AlignedWindow, MarketAlignedWindow, MarketCandle},
};

pub struct MarketCandlesMap(pub HashMap<u32, MarketCandlesDto>);

impl TryFrom<MarketCandlesMap> for MarketAlignedWindow {
    type Error = StockTrekError;

    fn try_from(value: MarketCandlesMap) -> Result<Self, Self::Error> {
        let mut candles = HashMap::new();
        for (window_u32, candles_dto) in value.0 {
            let window = AlignedWindow::from_repr(window_u32 as u8).ok_or(
                StockTrekError::Value(ValueError::IncorrectType {
                    expected: "AlignedWindow".to_string(),
                    found: window_u32.to_string(),
                }),
            )?;
            let window_candles = candles_dto
                .candles_dto
                .into_iter()
                .map(MarketCandle::try_from)
                .collect::<Result<Vec<_>, _>>()?;
            candles.insert(window, window_candles);
        }
        Ok(MarketAlignedWindow::new(candles))
    }
}
