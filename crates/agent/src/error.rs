use axum::http::StatusCode;
use exit_codes::ExitCode;
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

    #[error("client error: {0}")]
    Client(ExitCode),

    #[error("build runner failed: {0}")]
    RunnerExit(ExitCode),

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

    #[error("failed to create temporary build directory")]
    CreateBuildDirectory(#[source] std::io::Error),

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
        let code = ExitCode::from_code(code)?;
        if code == ExitCode::Success {
            return None;
        }
        Some(if code.is_client_error() {
            BaError::Client(code)
        } else {
            BaError::RunnerExit(code)
        })
    }

    pub fn status(&self) -> StatusCode {
        match self {
            BaError::Client(code) | BaError::RunnerExit(code) if code.is_client_error() => {
                StatusCode::BAD_REQUEST
            }
            BaError::Timeout(_) => StatusCode::GATEWAY_TIMEOUT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn message(&self) -> String {
        match self {
            BaError::Client(code) | BaError::RunnerExit(code) => {
                format!("{self} (exit code {})", code.code())
            }
            _ => self.to_string(),
        }
    }

    pub fn detail(&self) -> String {
        self.message()
    }
}
