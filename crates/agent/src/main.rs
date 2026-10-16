use crate::{
    env_var_config::EnvVarConfig,
    error::{BaError, BaResult},
    handler::Handler,
    tracing_setup::TracingSetup,
};
use axum::{
    Router,
    routing::{get, post},
};

mod env_var_config;
mod error;
mod handler;
mod tracing_setup;

const RUN_PATH: &str = "/run";
const HEALTH_PATH: &str = "/health";

#[tokio::main]
async fn main() -> BaResult<()> {
    let config = EnvVarConfig::from_env()?;
    TracingSetup::setup(config.log_filter())?;
    let handler = Handler::new(config.build_timeout());
    let app = Router::new().route(HEALTH_PATH, get(health)).route(
        RUN_PATH,
        post(move |body| {
            let handler = handler.clone();
            async move { handler.handle(body).await }
        }),
    );
    let address = config.address();
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|source| BaError::Bind {
            address: address.to_string(),
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
