use crate::{
    artifacts::Artifacts,
    error::{BaError, BaResult},
};
use axum::{body::Bytes, http::StatusCode};
use std::{
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::Command,
    time::timeout,
};
use tracing::warn;

const BUILD_RUNNER: &str = "/usr/local/bin/build-runner";

#[derive(Clone)]
pub struct Handler {
    build_timeout: Duration,
    artifacts: Artifacts,
}

impl Handler {
    pub fn new(build_timeout: Duration, artifacts: Artifacts) -> Self {
        Self {
            build_timeout,
            artifacts,
        }
    }

    pub async fn handle(&self, body: Bytes) -> (StatusCode, String) {
        let mut stderr_output = String::new();
        let result = timeout(self.build_timeout, self.run_build(body, &mut stderr_output))
            .await
            .unwrap_or(Err(BaError::Timeout(self.build_timeout)));
        match result {
            Ok(()) => match self.publish_artifacts().await {
                Ok(()) => (StatusCode::OK, "success".to_string()),
                Err(error) => {
                    eprintln!("artifact upload failed: {error}");
                    (error.status(), error.detail())
                }
            },
            Err(error) => {
                eprintln!("build failed: {error}");
                (error.status(), Self::failure_body(&error, &stderr_output))
            }
        }
    }

    fn failure_body(error: &BaError, stderr_output: &str) -> String {
        let detail = error.detail();
        if stderr_output.is_empty() {
            detail
        } else {
            format!("{detail}\n{stderr_output}")
        }
    }

    async fn publish_artifacts(&self) -> BaResult<()> {
        let cwasm = self.artifacts.compile().await?;
        self.artifacts.publish(&cwasm).await
    }

    async fn run_build(&self, body: Bytes, stderr_output: &mut String) -> BaResult<()> {
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

        let stdout_task = tokio::spawn(Self::forward_output(stdout, |line| println!("{line}")));
        let stderr_lines = Arc::new(Mutex::new(Vec::new()));
        let stderr_buffer = Arc::clone(&stderr_lines);
        let stderr_task = tokio::spawn(Self::forward_output(stderr, move |line| {
            eprintln!("{line}");
            if let Ok(mut lines) = stderr_buffer.lock() {
                lines.push(line.to_string());
            }
        }));

        let status = child.wait().await.map_err(BaError::WaitForRunner)?;

        let _ = stdout_task.await;
        let _ = stderr_task.await;

        if !status.success() {
            if let Ok(lines) = stderr_lines.lock() {
                *stderr_output = lines.join("\n");
            }
            return Err(status
                .code()
                .and_then(BaError::from_exit_code)
                .unwrap_or(BaError::RunnerFailed(status)));
        }

        Ok(())
    }

    /// Forward child output line-by-line without requiring it to be valid UTF-8.
    ///
    /// Reading bytes up to each newline avoids the `BufReader::lines()` behaviour
    /// of terminating on the first invalid UTF-8 sequence, which would leave the
    /// pipe unread and let the child block once the pipe buffer fills.
    async fn forward_output<R, F>(reader: R, mut write: F)
    where
        R: AsyncRead + Unpin + Send,
        F: FnMut(&str) + Send,
    {
        let mut reader = BufReader::new(reader);
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            match reader.read_until(b'\n', &mut buffer).await {
                Ok(0) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buffer);
                    let line = line.strip_suffix('\n').unwrap_or(&line);
                    let line = line.strip_suffix('\r').unwrap_or(line);
                    write(line);
                }
                Err(error) => {
                    warn!("failed to read build runner output: {error}");
                    break;
                }
            }
        }
    }
}
