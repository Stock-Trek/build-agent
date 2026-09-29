use crate::{convert::required::IsRequired, generated::dto::MarketQuoteDto};
use stock_trek::{errors::StockTrekError, markets::MarketQuote};

impl TryFrom<MarketQuoteDto> for MarketQuote {
    type Error = StockTrekError;

    fn try_from(value: MarketQuoteDto) -> Result<Self, Self::Error> {
        let MarketQuoteDto {
            price_dto,
            quantity_dto,
        } = value;
        let price = price_dto.required()?.try_into()?;
        let quantity = quantity_dto.required()?.try_into()?;
        Ok(MarketQuote { price, quantity })
    }
}
