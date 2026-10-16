use crate::{
    error::{BaError, BaResult},
    handler::Handler,
    tracing_setup::TracingSetup,
};
use axum::{
    Router,
    routing::{get, post},
};

mod error;
mod handler;
mod tracing_setup;

const RUN_PATH: &str = "/run";
const HEALTH_PATH: &str = "/health";
const DEFAULT_ADDRESS: &str = "0.0.0.0:8080";
const ADDRESS_ENV: &str = "BUILD_AGENT_ADDRESS";

#[tokio::main]
async fn main() -> BaResult<()> {
    TracingSetup::setup()?;
    let app = Router::new()
        .route(HEALTH_PATH, get(health))
        .route(RUN_PATH, post(|body| Handler.handle(body)));
    let address = std::env::var(ADDRESS_ENV).unwrap_or_else(|_| DEFAULT_ADDRESS.to_string());
    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .map_err(|source| BaError::Bind {
            address: address.clone(),
            source,
        })?;
    println!("build agent listening on {address}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(BaError::Serve)?;
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
