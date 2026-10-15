use crate::{handler::Handler, tracing_setup::TracingSetup};
use axum::{
    Router,
    routing::{get, post},
};

mod error;
mod handler;
mod tracing_setup;

const RUN_HOOK_PATH: &str = "/aws/lambda-microvms/runtime/v1/run";
const HEALTH_PATH: &str = "/health";
const DEFAULT_ADDRESS: &str = "0.0.0.0:8080";
const ADDRESS_ENV: &str = "BUILD_AGENT_ADDRESS";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    TracingSetup::setup()?;
    let app = Router::new()
        .route(HEALTH_PATH, get(health))
        .route(RUN_HOOK_PATH, post(|body| Handler.handle(body)));
    let address = std::env::var(ADDRESS_ENV).unwrap_or_else(|_| DEFAULT_ADDRESS.to_string());
    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .unwrap_or_else(|error| panic!("failed to bind {address}: {error}"));
    println!("build agent listening on {address}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
    Ok(())
}

async fn health() -> &'static str {
    "ok"
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
