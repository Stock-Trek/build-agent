use crate::error::{BaError, BaResult};
use tracing_subscriber::{EnvFilter, fmt::Subscriber};

pub struct TracingSetup;

impl TracingSetup {
    pub fn setup() -> BaResult<()> {
        let filter = match std::env::var(EnvFilter::DEFAULT_ENV) {
            Ok(directives) => EnvFilter::try_new(&directives).map_err(|error| {
                BaError::Config(format!("{} is invalid: {error}", EnvFilter::DEFAULT_ENV))
            })?,
            Err(std::env::VarError::NotPresent) => EnvFilter::new("info"),
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(BaError::Config(format!(
                    "{} is not valid UTF-8",
                    EnvFilter::DEFAULT_ENV
                )));
            }
        };
        let subscriber = Subscriber::builder()
            .with_ansi(false)
            .with_env_filter(filter)
            .finish();
        tracing::subscriber::set_global_default(subscriber)?;
        Ok(())
    }
}
