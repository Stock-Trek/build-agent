use crate::{handler::Handler, tracing_setup::TracingSetup};
use lambda_runtime::{LambdaEvent, run, service_fn};

mod error;
mod handler;
mod tracing_setup;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    TracingSetup::setup()?;
    run(service_fn(|_event: LambdaEvent<String>| async move {
        Handler.handle().await
    }))
    .await
}
