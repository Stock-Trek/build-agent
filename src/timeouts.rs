use crate::error::{ACError, ACResult};
use std::time::{Duration, SystemTime};

const COMMAND_TIMEOUT_ENV: &str = "COMMAND_TIMEOUT_SECONDS";
const AWS_CONNECT_TIMEOUT_ENV: &str = "AWS_CONNECT_TIMEOUT_SECONDS";
const AWS_OPERATION_TIMEOUT_ENV: &str = "AWS_OPERATION_TIMEOUT_SECONDS";

const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(600);
const DEFAULT_AWS_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_AWS_OPERATION_TIMEOUT: Duration = Duration::from_secs(120);
const COMMAND_DEADLINE_BUFFER: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    pub command: Duration,
    pub aws_connect: Duration,
    pub aws_operation: Duration,
}

impl Timeouts {
    pub fn from_env() -> ACResult<Self> {
        Ok(Self {
            command: Self::duration(COMMAND_TIMEOUT_ENV, DEFAULT_COMMAND_TIMEOUT)?,
            aws_connect: Self::duration(AWS_CONNECT_TIMEOUT_ENV, DEFAULT_AWS_CONNECT_TIMEOUT)?,
            aws_operation: Self::duration(
                AWS_OPERATION_TIMEOUT_ENV,
                DEFAULT_AWS_OPERATION_TIMEOUT,
            )?,
        })
    }

    pub fn command_for(&self, deadline: SystemTime) -> ACResult<Duration> {
        let remaining = deadline
            .duration_since(SystemTime::now())
            .unwrap_or_default()
            .saturating_sub(COMMAND_DEADLINE_BUFFER);
        if remaining.is_zero() {
            return Err(ACError::Timeout(
                "Lambda deadline reached before command could start".into(),
            ));
        }
        Ok(self.command.min(remaining))
    }

    fn duration(key: &str, default: Duration) -> ACResult<Duration> {
        match std::env::var(key) {
            Ok(value) => {
                let seconds = value.parse::<u64>().map_err(|_| {
                    ACError::Config(format!("{key} must be a whole number of seconds"))
                })?;
                if seconds == 0 {
                    return Err(ACError::Config(format!("{key} must be greater than 0")));
                }
                Ok(Duration::from_secs(seconds))
            }
            Err(std::env::VarError::NotPresent) => Ok(default),
            Err(std::env::VarError::NotUnicode(_)) => {
                Err(ACError::Config(format!("{key} is not valid UTF-8")))
            }
        }
    }
}
