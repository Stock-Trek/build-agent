use crate::error::{ACError, ACResult};
use std::{
    path::Path,
    process::{ExitStatus, Output, Stdio},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, ChildStdout, Command},
};

/// Environment variables that are safe to expose to an untrusted build
/// process. Everything else, including the AWS credentials injected by the
/// Lambda runtime, is cleared so that attacker-controlled algorithm code
/// cannot read secrets at compile time via `env!`, `option_env!`,
/// `include_str!`, `include_bytes!`, build scripts, or on-disk files
/// reachable through `HOME`.
const CLEAN_ENV_ALLOWLIST: [&str; 11] = [
    "PATH",
    "CARGO_HOME",
    "RUSTUP_HOME",
    "RUSTUP_TOOLCHAIN",
    "CARGO_TARGET_DIR",
    "RUSTFLAGS",
    "TMPDIR",
    "LANG",
    "LC_ALL",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
];

pub struct Program;

impl Program {
    pub async fn run_with_timeout(
        program: &str,
        args: &[&str],
        cwd: &Path,
        timeout: Duration,
    ) -> ACResult<String> {
        let output = Self::output_with_timeout(program, args, cwd, timeout).await?;
        Self::output_to_string(output)
    }

    /// Runs an untrusted command with a minimal, credential-free environment.
    ///
    /// Used for processing attacker-controlled sources and build artifacts so
    /// that the Lambda's credentials and other secrets are never visible to
    /// the spawned process.
    pub async fn run_with_clean_env(
        program: &str,
        args: &[&str],
        cwd: &Path,
        timeout: Duration,
    ) -> ACResult<String> {
        let output = Self::output_with_clean_env(program, args, cwd, timeout).await?;
        Self::output_to_string(output)
    }

    pub async fn output_with_timeout(
        program: &str,
        args: &[&str],
        cwd: &Path,
        timeout: Duration,
    ) -> ACResult<Output> {
        Self::output(Command::new(program), program, args, cwd, timeout).await
    }

    pub async fn output_with_clean_env(
        program: &str,
        args: &[&str],
        cwd: &Path,
        timeout: Duration,
    ) -> ACResult<Output> {
        let mut command = Command::new(program);
        command.env_clear().env("HOME", cwd);
        for key in CLEAN_ENV_ALLOWLIST {
            if let Ok(value) = std::env::var(key) {
                command.env(key, value);
            }
        }
        Self::output(command, program, args, cwd, timeout).await
    }

    fn output_to_string(output: Output) -> ACResult<String> {
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into())
        } else {
            Err(ACError::CommandOutput(
                String::from_utf8_lossy(&output.stderr).into(),
            ))
        }
    }

    async fn output(
        mut command: Command,
        program: &str,
        args: &[&str],
        cwd: &Path,
        timeout: Duration,
    ) -> ACResult<Output> {
        command
            .args(args)
            .current_dir(cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        Self::set_process_group(&mut command);
        let child = command.spawn().map_err(ACError::CommandRun)?;
        Self::collect_output(child, program, timeout).await
    }

    pub async fn pipe_with_timeout(
        commands: &mut [Command],
        timeout: Duration,
    ) -> ACResult<Output> {
        if commands.is_empty() {
            return Err(ACError::InternalServer(
                "pipe needs at least one command".into(),
            ));
        }
        let command_len = commands.len();
        let mut children: Vec<Child> = Vec::with_capacity(command_len);
        let mut programs: Vec<String> = Vec::with_capacity(command_len);
        let mut prev_stdout: Option<ChildStdout> = None;
        for cmd in commands.iter_mut() {
            programs.push(cmd.as_std().get_program().to_string_lossy().into_owned());
            if let Some(stdout) = prev_stdout.take() {
                let stdin: Stdio = stdout.try_into().map_err(ACError::CommandRun)?;
                cmd.stdin(stdin);
            }
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
            cmd.kill_on_drop(true);
            Self::set_process_group(cmd);
            match cmd.spawn() {
                Ok(mut child) => {
                    prev_stdout = child.stdout.take();
                    children.push(child);
                }
                Err(error) => {
                    for child in children.iter_mut() {
                        Self::kill_process_group(child);
                        let _ = child.start_kill();
                    }
                    return Err(ACError::CommandRun(error));
                }
            }
        }

        let mut stderr_handles = Vec::with_capacity(command_len);
        for child in children.iter_mut() {
            let stderr = child
                .stderr
                .take()
                .ok_or_else(|| ACError::InternalServer("missing stderr pipe".into()))?;
            stderr_handles.push(tokio::spawn(Self::read_to_end(stderr)));
        }
        let last_stdout_handle = prev_stdout.map(|stdout| tokio::spawn(Self::read_to_end(stdout)));

        let statuses = tokio::select! {
            result = async {
                let mut statuses: Vec<ExitStatus> = Vec::with_capacity(command_len);
                for child in children.iter_mut() {
                    statuses.push(child.wait().await.map_err(ACError::CommandRun)?);
                }
                Ok::<Vec<ExitStatus>, ACError>(statuses)
            } => result?,
            _ = tokio::time::sleep(timeout) => {
                for child in children.iter_mut() {
                    Self::kill_process_tree(child).await;
                }
                return Err(ACError::Timeout(format!(
                    "{} exceeded timeout of {timeout:?}",
                    programs.join(" | ")
                )));
            }
        };

        let mut last_stdout = match last_stdout_handle {
            Some(handle) => Self::join_reader(handle).await?,
            None => Vec::new(),
        };
        let mut stderrs = Vec::with_capacity(command_len);
        for handle in stderr_handles {
            stderrs.push(Self::join_reader(handle).await?);
        }

        let last = command_len - 1;
        let mut outputs = Vec::with_capacity(command_len);
        for (index, (status, stderr)) in statuses.into_iter().zip(stderrs).enumerate() {
            let stdout = if index == last {
                std::mem::take(&mut last_stdout)
            } else {
                Vec::new()
            };
            outputs.push(Output {
                status,
                stdout,
                stderr,
            });
        }
        for (program, output) in programs.iter().zip(&outputs) {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(ACError::CommandOutput(format!(
                    "{program} failed with {}: {}",
                    output.status,
                    stderr.trim()
                )));
            }
        }
        outputs
            .pop()
            .ok_or_else(|| ACError::InternalServer("pipe produced no output".into()))
    }

    async fn collect_output(
        mut child: Child,
        program: &str,
        timeout: Duration,
    ) -> ACResult<Output> {
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ACError::InternalServer("missing stdout pipe".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| ACError::InternalServer("missing stderr pipe".into()))?;
        let stdout_handle = tokio::spawn(Self::read_to_end(stdout));
        let stderr_handle = tokio::spawn(Self::read_to_end(stderr));
        let status = Self::wait_with_timeout(&mut child, program, timeout).await?;
        let stdout = Self::join_reader(stdout_handle).await?;
        let stderr = Self::join_reader(stderr_handle).await?;
        Ok(Output {
            status,
            stdout,
            stderr,
        })
    }

    async fn wait_with_timeout(
        child: &mut Child,
        program: &str,
        timeout: Duration,
    ) -> ACResult<ExitStatus> {
        tokio::select! {
            status = child.wait() => status.map_err(ACError::CommandRun),
            _ = tokio::time::sleep(timeout) => {
                Self::kill_process_tree(child).await;
                Err(ACError::Timeout(format!(
                    "{program} exceeded timeout of {timeout:?}"
                )))
            }
        }
    }

    fn set_process_group(command: &mut Command) {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }

    fn kill_process_group(child: &Child) {
        if let Some(pid) = child.id() {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }

    async fn kill_process_tree(child: &mut Child) {
        Self::kill_process_group(child);
        let _ = child.start_kill();
        let _ = child.wait().await;
    }

    async fn read_to_end<R: AsyncRead + Unpin>(mut reader: R) -> std::io::Result<Vec<u8>> {
        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer).await?;
        Ok(buffer)
    }

    async fn join_reader(
        handle: tokio::task::JoinHandle<std::io::Result<Vec<u8>>>,
    ) -> ACResult<Vec<u8>> {
        handle
            .await
            .map_err(|_| ACError::InternalServer("process output reader panicked".into()))?
            .map_err(ACError::CommandRun)
    }
}
