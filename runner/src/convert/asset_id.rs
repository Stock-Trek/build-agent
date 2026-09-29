use crate::{convert::string::from_string, generated::dto::AssetIdDto};
use stock_trek::{errors::StockTrekError, types::AssetId};

impl TryFrom<AssetIdDto> for AssetId {
    type Error = StockTrekError;
    fn try_from(value: AssetIdDto) -> Result<Self, Self::Error> {
        from_string(&value.asset_id)
    }
}
