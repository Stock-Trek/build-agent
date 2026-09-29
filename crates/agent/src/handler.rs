use axum::{body::Bytes, http::StatusCode};
use std::process::Stdio;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};

const BUILD_RUNNER: &str = "/usr/local/bin/build-runner";

#[derive(Clone)]
pub struct Handler;

impl Handler {
    pub async fn handle(&self, body: Bytes) -> (StatusCode, &'static str) {
        match self.run_build(body).await {
            Err(e) => {
                eprintln!("build failed: {e}");
                (StatusCode::OK, "accepted")
            }
            Ok(result) => {
                println!("result: {:?}", result);
                (StatusCode::OK, "accepted")
            }
        }
    }

    async fn run_build(&self, body: Bytes) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // The envelope is JSON: { "microvmId": "...", "runHookPayload": "..." }.
        // We pass the raw body through to the build runner, which is responsible
        // for extracting whatever it needs from runHookPayload.
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
        // the MicroVM's log capture picks them up in real time.
        let stdout = child.stdout.take().expect("child stdout");
        let stderr = child.stderr.take().expect("child stderr");

        let stdout_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                println!("{line}");
            }
        });

        let stderr_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                eprintln!("{line}");
            }
        });

        let status = child.wait().await?;

        let _ = stdout_task.await;
        let _ = stderr_task.await;

        if !status.success() {
            return Err(format!("build runner exited with {status}").into());
        }

        Ok(())
    }
}
