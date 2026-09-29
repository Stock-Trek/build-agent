use stock_trek::errors::{StockTrekError, StockTrekResult, ValueError};

pub trait IsRequired {
    type Output;
    fn required(self) -> StockTrekResult<Self::Output>;
}

impl<T> IsRequired for Option<T> {
    type Output = T;

    fn required(self) -> StockTrekResult<T> {
        self.ok_or_else(|| {
            StockTrekError::Value(ValueError::NotFound {
                name: "Option".to_string(),
                key: "Option".to_string(),
            })
        })
    }
}
