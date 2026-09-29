use crate::generated::dto::{
    activation_dto::Activation as ActivationVariant,
    order_request_dto::OrderRequest as OrderRequestVariant,
    quantity_dto::Quantity as QuantityVariant,
};
use crate::{
    convert::{required::IsRequired, string::from_string},
    generated::dto::{
        ActivationDto, LimitOrderRequestDto, MarketBuyOrderRequestDto, MarketSellOrderRequestDto,
        OrderRequestDto, QuantityDto,
    },
};
use rust_decimal::Decimal;
use stock_trek::{
    errors::StockTrekError,
    types::{Activation, AssetId, OrderRequest, Quantity, Tag},
};

impl TryFrom<OrderRequestDto> for OrderRequest<AssetId, Decimal> {
    type Error = StockTrekError;

    fn try_from(value: OrderRequestDto) -> Result<Self, Self::Error> {
        let order_request = value.order_request.required()?;
        match order_request {
            OrderRequestVariant::Limit(limit_dto) => limit_dto.try_into(),
            OrderRequestVariant::MarketBuy(market_buy_dto) => market_buy_dto.try_into(),
            OrderRequestVariant::MarketSell(market_sell_dto) => market_sell_dto.try_into(),
        }
    }
}

impl TryFrom<LimitOrderRequestDto> for OrderRequest<AssetId, Decimal> {
    type Error = StockTrekError;

    fn try_from(value: LimitOrderRequestDto) -> Result<Self, Self::Error> {
        let LimitOrderRequestDto {
            base_dto,
            quote_dto,
            side,
            activation_dto,
            limit_price_dto,
            time_in_force,
            quantity_dto,
            tag,
        } = value;
        Ok(OrderRequest::Limit {
            base: base_dto.required()?.try_into()?,
            quote: quote_dto.required()?.try_into()?,
            side: from_string(&side)?,
            activation: activation_dto.required()?.try_into()?,
            limit_price: limit_price_dto.required()?.try_into()?,
            time_in_force: from_string(&time_in_force)?,
            quantity: quantity_dto.required()?.try_into()?,
            tag: Tag::new(&tag),
        })
    }
}

impl TryFrom<MarketBuyOrderRequestDto> for OrderRequest<AssetId, Decimal> {
    type Error = StockTrekError;

    fn try_from(value: MarketBuyOrderRequestDto) -> Result<Self, Self::Error> {
        let MarketBuyOrderRequestDto {
            base_dto,
            quote_dto,
            activation_dto,
            quote_quantity_dto,
            tag,
        } = value;
        Ok(OrderRequest::MarketBuy {
            base: base_dto.required()?.try_into()?,
            quote: quote_dto.required()?.try_into()?,
            activation: activation_dto.required()?.try_into()?,
            quote_quantity: quote_quantity_dto.required()?.try_into()?,
            tag: Tag::new(&tag),
        })
    }
}

impl TryFrom<MarketSellOrderRequestDto> for OrderRequest<AssetId, Decimal> {
    type Error = StockTrekError;

    fn try_from(value: MarketSellOrderRequestDto) -> Result<Self, Self::Error> {
        let MarketSellOrderRequestDto {
            base_dto,
            quote_dto,
            activation_dto,
            base_quantity_dto,
            tag,
        } = value;
        Ok(OrderRequest::MarketSell {
            base: base_dto.required()?.try_into()?,
            quote: quote_dto.required()?.try_into()?,
            activation: activation_dto.required()?.try_into()?,
            base_quantity: base_quantity_dto.required()?.try_into()?,
            tag: Tag::new(&tag),
        })
    }
}

impl TryFrom<ActivationDto> for Activation<Decimal> {
    type Error = StockTrekError;

    fn try_from(value: ActivationDto) -> Result<Self, Self::Error> {
        let activation = value.activation.required()?;
        match activation {
            ActivationVariant::Immediate(_) => Ok(Activation::Immediate),
            ActivationVariant::PriceTriggered(price_triggered_dto) => {
                Ok(Activation::PriceTriggered {
                    activation_price: price_triggered_dto
                        .activation_price_dto
                        .required()?
                        .try_into()?,
                    basis: from_string(&price_triggered_dto.basis)?,
                    direction: from_string(&price_triggered_dto.direction)?,
                    mode: from_string(&price_triggered_dto.mode)?,
                })
            }
            ActivationVariant::Trailing(trailing_dto) => Ok(Activation::Trailing {
                activation_price: trailing_dto.activation_price_dto.required()?.try_into()?,
                basis: from_string(&trailing_dto.basis)?,
                callback_rate_bps: trailing_dto.callback_rate_bps,
                direction: from_string(&trailing_dto.direction)?,
            }),
        }
    }
}

impl TryFrom<QuantityDto> for Quantity<Decimal> {
    type Error = StockTrekError;

    fn try_from(value: QuantityDto) -> Result<Self, Self::Error> {
        let quantity = value.quantity.required()?;
        match quantity {
            QuantityVariant::OfBase(of_base_dto) => Ok(Quantity::OfBase(of_base_dto.try_into()?)),
            QuantityVariant::OfQuote(of_quote_dto) => {
                Ok(Quantity::OfQuote(of_quote_dto.try_into()?))
            }
        }
    }
}
