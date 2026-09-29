use crate::{handler::Handler, tracing_setup::TracingSetup};
use axum::{Router, routing::post};

mod error;
mod handler;
mod tracing_setup;

const RUN_HOOK_PATH: &str = "/aws/lambda-microvms/runtime/v1/run";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    TracingSetup::setup()?;
    let app = Router::new().route(RUN_HOOK_PATH, post(|body| Handler.handle(body)));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("failed to bind port 8080");
    println!("hook server listening on 0.0.0.0:8080");
    axum::serve(listener, app).await.expect("server error");
    Ok(())
}
