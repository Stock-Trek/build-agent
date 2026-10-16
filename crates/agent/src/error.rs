use axum::http::StatusCode;
use std::process::ExitStatus;

pub type BaResult<T> = Result<T, BaError>;

#[derive(Debug, thiserror::Error)]
pub enum BaError {
    #[error("invalid configuration: {0}")]
    Config(String),

    #[error("failed to install tracing subscriber")]
    Tracing(#[from] tracing::subscriber::SetGlobalDefaultError),

    #[error("failed to bind build agent to {address}")]
    Bind {
        address: String,
        #[source]
        source: std::io::Error,
    },

    #[error("build agent server failed")]
    Serve(#[source] std::io::Error),

    #[error("failed to spawn build runner `{command}`")]
    SpawnRunner {
        command: &'static str,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to write build request to build runner")]
    WriteRequest(#[source] std::io::Error),

    #[error("build runner did not provide a captured {0} pipe")]
    MissingPipe(&'static str),

    #[error("failed to wait for build runner")]
    WaitForRunner(#[source] std::io::Error),

    #[error("invalid request")]
    InvalidRequest,

    #[error("build runner exited with {0}")]
    RunnerFailed(ExitStatus),
}

impl BaError {
    pub fn status(&self) -> StatusCode {
        match self {
            BaError::InvalidRequest => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            BaError::InvalidRequest => "invalid request",
            _ => "build failed",
        }
    }
}
