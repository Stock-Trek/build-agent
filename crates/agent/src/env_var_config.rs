use crate::{
    error::{BaError, BaResult},
    s3::SignedPost,
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    env::{self, VarError},
    time::Duration,
};
use tracing_subscriber::EnvFilter;

const ADDRESS_ENV: &str = "BUILD_AGENT_ADDRESS";
const DEFAULT_ADDRESS: &str = "0.0.0.0:8080";
const BUILD_TIMEOUT_ENV: &str = "BUILD_AGENT_TIMEOUT_SECS";
const DEFAULT_BUILD_TIMEOUT_SECS: u64 = 300;
const DEFAULT_LOG_FILTER: &str = "info";
const SIGNED_POST_URL_ENV: &str = "SIGNED_POST_URL";
const SIGNED_POST_FIELDS_ENV: &str = "SIGNED_POST_FIELDS";

#[derive(Debug)]
pub struct EnvVarConfig {
    address: String,
    build_timeout: Duration,
    log_filter: EnvFilter,
    signed_post: SignedPost,
}

impl EnvVarConfig {
    pub fn from_env() -> BaResult<Self> {
        Ok(Self {
            address: Self::address_from_env()?,
            build_timeout: Self::build_timeout_from_env()?,
            log_filter: Self::log_filter_from_env()?,
            signed_post: Self::signed_post_from_env()?,
        })
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn build_timeout(&self) -> Duration {
        self.build_timeout
    }

    pub fn log_filter(&self) -> EnvFilter {
        self.log_filter.clone()
    }

    pub fn signed_post(&self) -> SignedPost {
        self.signed_post.clone()
    }

    fn address_from_env() -> BaResult<String> {
        Ok(Self::optional(ADDRESS_ENV)?.unwrap_or_else(|| DEFAULT_ADDRESS.to_string()))
    }

    fn build_timeout_from_env() -> BaResult<Duration> {
        match Self::optional(BUILD_TIMEOUT_ENV)? {
            Some(value) => value
                .parse::<u64>()
                .map(Duration::from_secs)
                .map_err(|error| {
                    BaError::Config(format!("{BUILD_TIMEOUT_ENV} is invalid: {error}"))
                }),
            None => Ok(Duration::from_secs(DEFAULT_BUILD_TIMEOUT_SECS)),
        }
    }

    fn log_filter_from_env() -> BaResult<EnvFilter> {
        match Self::optional(EnvFilter::DEFAULT_ENV)? {
            Some(directives) => EnvFilter::try_new(&directives).map_err(|error| {
                BaError::Config(format!("{} is invalid: {error}", EnvFilter::DEFAULT_ENV))
            }),
            None => Ok(EnvFilter::new(DEFAULT_LOG_FILTER)),
        }
    }

    fn signed_post_from_env() -> BaResult<SignedPost> {
        let url = Self::required(SIGNED_POST_URL_ENV)?;
        let fields = Self::required(SIGNED_POST_FIELDS_ENV)?;
        SignedPost::new(url, Self::parse_signed_post_fields(&fields)?)
    }

    fn parse_signed_post_fields(json: &str) -> BaResult<BTreeMap<String, String>> {
        let value: Value = serde_json::from_str(json).map_err(|source| {
            BaError::Config(format!(
                "{SIGNED_POST_FIELDS_ENV} is not valid JSON: {source}"
            ))
        })?;
        let object = value.as_object().ok_or_else(|| {
            BaError::Config(format!("{SIGNED_POST_FIELDS_ENV} must be a JSON object"))
        })?;

        let mut fields = BTreeMap::new();
        for (name, value) in object {
            let text = match value {
                Value::String(text) => text.clone(),
                Value::Number(number) => number.to_string(),
                Value::Bool(boolean) => boolean.to_string(),
                _ => {
                    return Err(BaError::Config(format!(
                        "{SIGNED_POST_FIELDS_ENV}.{name} must be a string"
                    )));
                }
            };
            fields.insert(name.clone(), text);
        }
        Ok(fields)
    }

    fn required(name: &str) -> BaResult<String> {
        match Self::optional(name)? {
            Some(value) if !value.trim().is_empty() => Ok(value),
            _ => Err(BaError::Config(format!("{name} must be set"))),
        }
    }

    fn optional(name: &str) -> BaResult<Option<String>> {
        match env::var(name) {
            Ok(value) => Ok(Some(value)),
            Err(VarError::NotPresent) => Ok(None),
            Err(VarError::NotUnicode(_)) => {
                Err(BaError::Config(format!("{name} is not valid UTF-8")))
            }
        }
    }
}
