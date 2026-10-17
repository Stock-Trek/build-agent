use axum::http::StatusCode;
use exit_codes::ClientError;
use std::{path::PathBuf, process::ExitStatus, time::Duration};

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

    #[error("strategy not found")]
    StrategyNotFound,

    #[error("strategy context not found")]
    StrategyContextNotFound,

    #[error("strategy context could not be decoded")]
    StrategyContextNotDecoded,

    #[error("strategy context could not be created")]
    StrategyContextNotCreated,

    #[error("portfolio not found")]
    PortfolioNotFound,

    #[error("portfolio could not be decoded")]
    PortfolioNotDecoded,

    #[error("portfolio could not be created")]
    PortfolioNotCreated,

    #[error("build runner exited with {0}")]
    RunnerFailed(ExitStatus),

    #[error("build timed out after {0:?}")]
    Timeout(Duration),

    #[error("failed to read artifact {path}")]
    ReadArtifact {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to read artifact directory {path}")]
    ReadArtifactDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to build the upload request for {path}")]
    BuildUploadRequest {
        path: PathBuf,
        #[source]
        source: reqwest::Error,
    },

    #[error("failed to upload artifact {path}")]
    UploadArtifact {
        path: PathBuf,
        #[source]
        source: reqwest::Error,
    },

    #[error("upload of {path} was rejected with status {status}: {body}")]
    UploadRejected {
        path: PathBuf,
        status: u16,
        body: String,
    },

    #[error("failed to compile artifact {path}")]
    CompileArtifact {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("compilation of artifact {path} exited with {status}")]
    CompileArtifactFailed { path: PathBuf, status: ExitStatus },
}

impl BaError {
    pub fn from_exit_code(code: i32) -> Option<Self> {
        Some(match ClientError::from_code(code)? {
            ClientError::InvalidRequest => BaError::InvalidRequest,
            ClientError::StrategyNotFound => BaError::StrategyNotFound,
            ClientError::StrategyContextNotFound => BaError::StrategyContextNotFound,
            ClientError::StrategyContextNotDecoded => BaError::StrategyContextNotDecoded,
            ClientError::StrategyContextNotCreated => BaError::StrategyContextNotCreated,
            ClientError::PortfolioNotFound => BaError::PortfolioNotFound,
            ClientError::PortfolioNotDecoded => BaError::PortfolioNotDecoded,
            ClientError::PortfolioNotCreated => BaError::PortfolioNotCreated,
        })
    }

    pub fn status(&self) -> StatusCode {
        match self {
            BaError::InvalidRequest
            | BaError::StrategyNotFound
            | BaError::StrategyContextNotFound
            | BaError::StrategyContextNotDecoded
            | BaError::StrategyContextNotCreated
            | BaError::PortfolioNotFound
            | BaError::PortfolioNotDecoded
            | BaError::PortfolioNotCreated => StatusCode::BAD_REQUEST,
            BaError::Timeout(_) => StatusCode::GATEWAY_TIMEOUT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            BaError::InvalidRequest => "invalid request",
            BaError::StrategyNotFound => "strategy not found",
            BaError::StrategyContextNotFound => "strategy context not found",
            BaError::StrategyContextNotDecoded => "strategy context could not be decoded",
            BaError::StrategyContextNotCreated => "strategy context could not be created",
            BaError::PortfolioNotFound => "portfolio not found",
            BaError::PortfolioNotDecoded => "portfolio could not be decoded",
            BaError::PortfolioNotCreated => "portfolio could not be created",
            BaError::Timeout(_) => "build timed out",
            _ => "build failed",
        }
    }
}
