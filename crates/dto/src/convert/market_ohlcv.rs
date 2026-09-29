use crate::{convert::required::IsRequired, generated::dto::MarketOhlcvDto};
use stock_trek::{errors::StockTrekError, markets::MarketOhlcv};

impl TryFrom<MarketOhlcvDto> for MarketOhlcv {
    type Error = StockTrekError;

    fn try_from(value: MarketOhlcvDto) -> Result<Self, Self::Error> {
        let MarketOhlcvDto {
            open_dto,
            high_dto,
            low_dto,
            close_dto,
            volume_dto,
            quote_volume_dto,
            vwap_dto,
        } = value;
        Ok(MarketOhlcv::new(
            open_dto.required()?.try_into()?,
            high_dto.required()?.try_into()?,
            low_dto.required()?.try_into()?,
            close_dto.required()?.try_into()?,
            volume_dto.required()?.try_into()?,
            quote_volume_dto.required()?.try_into()?,
            vwap_dto.required()?.try_into()?,
        ))
    }
}
