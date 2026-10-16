use crate::{
    artifacts::Artifacts,
    error::{BaError, BaResult},
};
use axum::{body::Bytes, http::StatusCode};
use std::{
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    time::timeout,
};

const BUILD_RUNNER: &str = "/usr/local/bin/build-runner";

// `build-runner` prefixes the stderr lines it emits when it rejects a request
// as malformed. Because Unix truncates process exit codes to 8 bits, the runner
// signals the failure class with this marker instead of relying on its (much
// larger) exit-code constants.
const BAD_REQUEST_MARKER: &str = "build-runner: invalid request:";

#[derive(Clone)]
pub struct Handler {
    build_timeout: Duration,
    artifacts: Option<Artifacts>,
}

impl Handler {
    pub fn new(build_timeout: Duration, artifacts: Option<Artifacts>) -> Self {
        Self {
            build_timeout,
            artifacts,
        }
    }

    pub async fn handle(&self, body: Bytes) -> (StatusCode, &'static str) {
        let result = timeout(self.build_timeout, self.run_build(body))
            .await
            .unwrap_or(Err(BaError::Timeout(self.build_timeout)));
        match result {
            Ok(()) => match self.publish_artifacts().await {
                Ok(()) => (StatusCode::OK, "accepted"),
                Err(error) => {
                    eprintln!("artifact upload failed: {error}");
                    (error.status(), error.message())
                }
            },
            Err(error) => {
                eprintln!("build failed: {error}");
                (error.status(), error.message())
            }
        }
    }

    async fn publish_artifacts(&self) -> BaResult<()> {
        match &self.artifacts {
            Some(artifacts) => {
                let cwasm = Artifacts::compile().await?;
                artifacts.publish(&cwasm).await
            }
            None => Ok(()),
        }
    }

    async fn run_build(&self, body: Bytes) -> BaResult<()> {
        // Pass the raw request body through to the build runner, which is
        // responsible for parsing the algorithm invocation.
        let mut child = Command::new(BUILD_RUNNER)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|source| BaError::SpawnRunner {
                command: BUILD_RUNNER,
                source,
            })?;

        // Write the body to the child's stdin, then close it.
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin
                .write_all(&body)
                .await
                .map_err(BaError::WriteRequest)?;
            // Dropping stdin closes the pipe, signalling EOF to the child.
        }

        // Stream stdout and stderr line-by-line into our own stdout/stderr so
        // the container log driver captures them in real time.
        let stdout = child.stdout.take().ok_or(BaError::MissingPipe("stdout"))?;
        let stderr = child.stderr.take().ok_or(BaError::MissingPipe("stderr"))?;

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

        let status = child.wait().await.map_err(BaError::WaitForRunner)?;

        let _ = stdout_task.await;
        let _ = stderr_task.await;

        if !status.success() {
            return Err(if rejected.load(Ordering::Relaxed) {
                BaError::InvalidRequest
            } else {
                BaError::RunnerFailed(status)
            });
        }

        Ok(())
    }
}
