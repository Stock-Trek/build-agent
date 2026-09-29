use crate::{
    aws::Aws,
    config::Config,
    dto::sqs_event::{SqsEvent, SqsEventResponse, SqsMessage},
    error::{ACError, ACResult},
    tasks::task::{Task, TaskTrait},
};
use lambda_runtime::{Error, LambdaEvent, run, service_fn};
use std::{sync::Arc, time::SystemTime};
use tracing_subscriber::{EnvFilter, fmt::Subscriber};

mod archive;
mod aws;
mod config;
mod constants;
mod dto;
mod dynamodb;
mod error;
mod fenced;
mod files;
mod git_local;
mod git_remote;
mod github;
mod program;
mod s3;
mod sqs;
mod timeouts;

struct Tracing;

impl Tracing {
    fn setup() -> Result<(), Error> {
        let filter = match std::env::var(EnvFilter::DEFAULT_ENV) {
            Ok(directives) => EnvFilter::try_new(&directives).map_err(|error| {
                ACError::Config(format!("{} is invalid: {error}", EnvFilter::DEFAULT_ENV))
            })?,
            Err(std::env::VarError::NotPresent) => EnvFilter::new("info"),
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(ACError::Config(format!(
                    "{} is not valid UTF-8",
                    EnvFilter::DEFAULT_ENV
                ))
                .into());
            }
        };
        let subscriber = Subscriber::builder()
            .with_ansi(false)
            .with_env_filter(filter)
            .finish();
        tracing::subscriber::set_global_default(subscriber).expect("Failed to set subscriber");
        Ok(())
    }
}

#[derive(Clone)]
struct Handler {
    aws: Arc<Aws>,
}

impl Handler {
    fn new(aws: Aws) -> Self {
        Self { aws: Arc::new(aws) }
    }

    async fn handle(&self, event: LambdaEvent<SqsEvent>) -> Result<SqsEventResponse, Error> {
        let deadline = event.context.deadline();
        let mut response = SqsEventResponse::default();
        for record in event.payload.records {
            if let Err(error) = self.process_record(&record.body, deadline).await {
                if error.is_retryable() {
                    tracing::error!(
                        message_id = %record.message_id,
                        %error,
                        "Failed to process SQS message, reporting batch item failure",
                    );
                    response.add_failure(record.message_id);
                } else {
                    tracing::error!(
                        message_id = %record.message_id,
                        %error,
                        "Failed to process SQS message, discarding it",
                    );
                }
            }
        }
        Ok(response)
    }

    async fn process_record(&self, body: &str, deadline: SystemTime) -> ACResult<()> {
        let message: SqsMessage = serde_json::from_str(body).map_err(|error| {
            ACError::InvalidMessage(format!("Failed to deserialize SQS message: {error}"))
        })?;
        let task: Task = message.try_into()?;
        task.handle(&self.aws, deadline).await
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    Tracing::setup()?;
    let config = Config::from_env()?;
    let handler = Handler::new(Aws::new(config).await?);
    run(service_fn(move |event| {
        let handler = handler.clone();
        async move { handler.handle(event).await }
    }))
    .await
}
