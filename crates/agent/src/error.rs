use axum::http::StatusCode;
use std::process::ExitStatus;

/// Convenient result alias used throughout the build agent.
pub type BaResult<T> = Result<T, BaError>;

/// Errors that can occur while running the build agent.
#[derive(Debug, thiserror::Error)]
pub enum BaError {
    /// Invalid or missing configuration.
    #[error("invalid configuration: {0}")]
    Config(String),

    /// Failed to install the global tracing subscriber.
    #[error("failed to install tracing subscriber")]
    Tracing(#[from] tracing::subscriber::SetGlobalDefaultError),

    /// Failed to bind the HTTP listener.
    #[error("failed to bind build agent to {address}")]
    Bind {
        address: String,
        #[source]
        source: std::io::Error,
    },

    /// The HTTP server failed while serving requests.
    #[error("build agent server failed")]
    Serve(#[source] std::io::Error),

    /// Failed to spawn the build runner process.
    #[error("failed to spawn build runner `{command}`")]
    SpawnRunner {
        command: &'static str,
        #[source]
        source: std::io::Error,
    },

    /// Failed to write the request body to the build runner's stdin.
    #[error("failed to write build request to build runner")]
    WriteRequest(#[source] std::io::Error),

    /// Failed to read a captured stdout/stderr pipe from the build runner.
    #[error("build runner did not provide a captured {0} pipe")]
    MissingPipe(&'static str),

    /// Failed to wait for the build runner process to exit.
    #[error("failed to wait for build runner")]
    WaitForRunner(#[source] std::io::Error),

    /// The build runner rejected the request as malformed.
    #[error("invalid request")]
    InvalidRequest,

    /// The build runner exited with a non-zero status.
    #[error("build runner exited with {0}")]
    RunnerFailed(ExitStatus),
}

impl BaError {
    /// The HTTP status code that should be returned to the client.
    pub fn status(&self) -> StatusCode {
        match self {
            BaError::InvalidRequest => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// The client-facing message for this error.
    pub fn message(&self) -> &'static str {
        match self {
            BaError::InvalidRequest => "invalid request",
            _ => "build failed",
        }
    }
}
