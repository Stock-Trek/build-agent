use crate::error::{BaError, BaResult};
use std::{
    env::{self, VarError},
    time::Duration,
};
use tracing_subscriber::EnvFilter;

const ADDRESS_ENV: &str = "BUILD_AGENT_ADDRESS";
const DEFAULT_ADDRESS: &str = "0.0.0.0:8080";
const BUILD_TIMEOUT_ENV: &str = "BUILD_AGENT_TIMEOUT_SECS";
const DEFAULT_BUILD_TIMEOUT_SECS: u64 = 300;
const DEFAULT_LOG_FILTER: &str = "info";

#[derive(Debug)]
pub struct EnvVarConfig {
    address: String,
    build_timeout: Duration,
    log_filter: EnvFilter,
}

impl EnvVarConfig {
    pub fn from_env() -> BaResult<Self> {
        Ok(Self {
            address: Self::address_from_env()?,
            build_timeout: Self::build_timeout_from_env()?,
            log_filter: Self::log_filter_from_env()?,
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
