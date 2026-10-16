use axum::{body::Bytes, http::StatusCode};
use std::{
    io,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};

const BUILD_RUNNER: &str = "/usr/local/bin/build-runner";

// `build-runner` prefixes the stderr lines it emits when it rejects a request
// as malformed. Because Unix truncates process exit codes to 8 bits, the runner
// signals the failure class with this marker instead of relying on its (much
// larger) exit-code constants.
const BAD_REQUEST_MARKER: &str = "build-runner: invalid request:";

#[derive(Clone)]
pub struct Handler;

/// A failure while serving a `POST /run` request.
#[derive(Debug, thiserror::Error)]
enum BuildError {
    /// The runner could not be spawned or communicated with.
    #[error("failed to run build runner: {0}")]
    Runner(#[from] io::Error),
    /// The runner rejected the request as malformed.
    #[error("invalid request")]
    InvalidRequest,
    /// The runner ran but exited unsuccessfully for another reason.
    #[error("build runner exited with {0}")]
    Exit(std::process::ExitStatus),
}

impl BuildError {
    fn status(&self) -> StatusCode {
        match self {
            BuildError::InvalidRequest => StatusCode::BAD_REQUEST,
            BuildError::Runner(_) | BuildError::Exit(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn message(&self) -> &'static str {
        match self {
            BuildError::InvalidRequest => "invalid request",
            BuildError::Runner(_) | BuildError::Exit(_) => "build failed",
        }
    }
}

impl Handler {
    pub async fn handle(&self, body: Bytes) -> (StatusCode, &'static str) {
        match self.run_build(body).await {
            Ok(()) => (StatusCode::OK, "accepted"),
            Err(error) => {
                eprintln!("build failed: {error}");
                (error.status(), error.message())
            }
        }
    }

    async fn run_build(&self, body: Bytes) -> Result<(), BuildError> {
        // Pass the raw request body through to the build runner, which is
        // responsible for parsing the algorithm invocation.
        let mut child = Command::new(BUILD_RUNNER)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Write the body to the child's stdin, then close it.
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(&body).await?;
            // Dropping stdin closes the pipe, signalling EOF to the child.
        }

        // Stream stdout and stderr line-by-line into our own stdout/stderr so
        // the container log driver captures them in real time.
        let stdout = child.stdout.take().expect("child stdout");
        let stderr = child.stderr.take().expect("child stderr");

        let stdout_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                println!("{line}");
            }
        });

        let rejected = Arc::new(AtomicBool::new(false));
        let stderr_task = {
            let rejected = Arc::clone(&rejected);
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if line.starts_with(BAD_REQUEST_MARKER) {
                        rejected.store(true, Ordering::Relaxed);
                    }
                    eprintln!("{line}");
                }
            })
        };

        let status = child.wait().await?;

        let _ = stdout_task.await;
        let _ = stderr_task.await;

        if !status.success() {
            return Err(if rejected.load(Ordering::Relaxed) {
                BuildError::InvalidRequest
            } else {
                BuildError::Exit(status)
            });
        }

        Ok(())
    }
}
