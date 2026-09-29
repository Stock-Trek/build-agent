use serde::de::DeserializeOwned;
use stock_trek::errors::{StockTrekError, ValueError};

/// Deserializes a stock-trek enum-like type (e.g. [`AssetId`], [`CexId`]) from its
/// serialized string representation.
///
/// The stock-trek types serialize unit enum variants as plain JSON strings, so the
/// incoming value is wrapped in a JSON string before being handed to `serde`.
pub fn from_string<T: DeserializeOwned>(value: &str) -> Result<T, StockTrekError> {
    serde_json::from_value(serde_json::Value::String(value.to_string())).map_err(|_| {
        StockTrekError::Value(ValueError::IncorrectType {
            expected: std::any::type_name::<T>().to_string(),
            found: value.to_string(),
        })
    })
}
