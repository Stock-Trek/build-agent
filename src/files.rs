use crate::{
    dto::compile_result::{CodeLocation, CompileMessage, CompileResult, CompileStatus},
    error::{ACError, ACResult},
    program::Program,
    timeouts::Timeouts,
};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::LazyLock,
    time::SystemTime,
};
use tokio::{fs, process::Command};
use tracing::{info, warn};

const BASE: &str = "/tmp/algorithm-compiler";
const SOURCE: &str = "./algorithm-runner";
const REPO_FOLDER: &str = "repo";
const ARCHIVE_FILE: &str = "archive.tar.gz";
pub const ALGORITHMS_ARCHIVE_FILE: &str = "algorithms.tar.gz";
const BUILD_FOLDER: &str = "build";
const TARGET_FOLDER: &str = "target";
const ALGORITHMS_FOLDER: &str = "src/algorithms";
const BUILT_WASM: &str = "target/wasm32-wasip1/release/algorithm_runner.wasm";
const BUILT_CWASM: &str = "algorithm-runner.cwasm";
const COMPILE_OUTPUT_FILE: &str = "compile-output.txt";
const COMPILER_MESSAGE: &str = "compiler-message";
const BUILD_FINISHED: &str = "build-finished";
const LEVEL_ERROR: &str = "error";
const IGNORED_DIRECTORIES: [&str; 2] = [".git", "target"];

struct CompileOutput {
    success: Option<bool>,
    errors: Vec<String>,
    compile_messages: Vec<CompileMessage>,
}

pub struct Files {
    pub base: PathBuf,
    pub archive: PathBuf,
    pub repo: PathBuf,
    pub build: PathBuf,
    pub algorithms: PathBuf,
    pub algorithms_archive: PathBuf,
}

impl Files {
    pub fn new() -> &'static Self {
        static FILES: LazyLock<Files> = LazyLock::new(|| {
            let base = PathBuf::from(BASE);
            let build = base.join(BUILD_FOLDER);
            Files {
                archive: base.join(ARCHIVE_FILE),
                repo: base.join(REPO_FOLDER),
                algorithms: build.join(ALGORITHMS_FOLDER),
                algorithms_archive: base.join(ALGORITHMS_ARCHIVE_FILE),
                build,
                base,
            }
        });
        &FILES
    }

    pub async fn clean(&self) -> ACResult<()> {
        let target = self.build.join(TARGET_FOLDER);
        let cache = self.target_cache();
        if fs::try_exists(&target).await.map_err(ACError::FileSystem)? {
            Self::remove_dir_all_if_exists(&cache).await?;
            fs::rename(&target, &cache)
                .await
                .map_err(ACError::FileSystem)?;
        }
        let restore_target = fs::try_exists(&cache).await.map_err(ACError::FileSystem)?;
        Self::remove_dir_all_if_exists(&self.base).await?;
        fs::create_dir_all(&self.repo)
            .await
            .map_err(ACError::FileSystem)?;
        fs::create_dir_all(&self.build)
            .await
            .map_err(ACError::FileSystem)?;
        if restore_target {
            fs::rename(&cache, &target)
                .await
                .map_err(ACError::FileSystem)?;
        }
        Ok(())
    }

    pub async fn prepare(&self) -> ACResult<()> {
        self.clean().await?;
        Self::copy_dir(Path::new(SOURCE), &self.build).await?;
        self.seed_target().await
    }

    async fn seed_target(&self) -> ACResult<()> {
        let target = self.build.join(TARGET_FOLDER);
        if fs::try_exists(&target).await.map_err(ACError::FileSystem)? {
            return Ok(());
        }
        let source = Path::new(SOURCE).join(TARGET_FOLDER);
        if fs::metadata(&source)
            .await
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false)
        {
            Self::copy_dir(&source, &target).await?;
        }
        Ok(())
    }

    fn target_cache(&self) -> PathBuf {
        PathBuf::from(format!("{BASE}-target"))
    }

    async fn remove_dir_all_if_exists(path: &Path) -> ACResult<()> {
        match fs::remove_dir_all(path).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(ACError::FileSystem(error)),
        }
    }

    pub async fn copy_algorithms(
        &self,
        revision: &str,
        timeouts: &Timeouts,
        deadline: SystemTime,
    ) -> ACResult<()> {
        if self
            .algorithms_path_missing(revision, timeouts, deadline)
            .await?
        {
            return Err(ACError::UserError(format!(
                "The repository does not contain a '{ALGORITHMS_FOLDER}' directory at revision '{revision}'. Add your algorithms under '{ALGORITHMS_FOLDER}' and commit them."
            )));
        }
        fs::create_dir_all(&self.algorithms)
            .await
            .map_err(ACError::FileSystem)?;
        let git_dir = Self::path_str(&self.repo)?;
        let algorithms = Self::path_str(&self.algorithms)?;
        let mut git = Command::new("git");
        git.args([
            &format!("--git-dir={git_dir}"),
            "archive",
            "--format=tar",
            "--",
            &format!("{revision}:{ALGORITHMS_FOLDER}"),
        ]);
        let mut tar = Command::new("tar");
        tar.args(["-x", "-C", algorithms]);
        let mut commands = [git, tar];
        Program::pipe_with_timeout(&mut commands, timeouts.command_for(deadline)?).await?;
        Ok(())
    }

    async fn algorithms_path_missing(
        &self,
        revision: &str,
        timeouts: &Timeouts,
        deadline: SystemTime,
    ) -> ACResult<bool> {
        let git_dir = Self::path_str(&self.repo)?;
        let output = Program::run_with_timeout(
            "git",
            &[
                &format!("--git-dir={git_dir}"),
                "ls-tree",
                revision,
                "--",
                ALGORITHMS_FOLDER,
            ],
            &self.base,
            timeouts.command_for(deadline)?,
        )
        .await?;
        Ok(output.trim().is_empty())
    }

    pub async fn compile(
        &self,
        timeouts: &Timeouts,
        deadline: SystemTime,
    ) -> ACResult<CompileResult> {
        info!("compile");
        let compile_result = self.build_wasm(timeouts, deadline).await?;
        info!("Build result {:?}", compile_result);
        if compile_result.result != CompileStatus::Success {
            return Ok(compile_result);
        }
        self.compile_cwasm(timeouts, deadline).await?;
        Ok(compile_result)
    }

    pub fn cwasm_file(&self) -> PathBuf {
        self.build.join(BUILT_CWASM)
    }

    pub fn compile_output_file(&self) -> PathBuf {
        self.build.join(COMPILE_OUTPUT_FILE)
    }

    pub fn sanitize_path(value: &str) -> ACResult<String> {
        if value.is_empty() || value == "." || value == ".." {
            return Err(ACError::InvalidMessage(format!(
                "Invalid repository path value: {value:?}"
            )));
        }
        let mut sanitized = String::with_capacity(value.len());
        for character in value.chars() {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                sanitized.push(character);
            } else {
                let mut buffer = [0; 4];
                for byte in character.encode_utf8(&mut buffer).bytes() {
                    sanitized.push('%');
                    sanitized.push_str(&format!("{byte:02X}"));
                }
            }
        }
        Ok(sanitized)
    }

    pub fn path_str(path: &Path) -> ACResult<&str> {
        path.to_str()
            .ok_or_else(|| ACError::InternalServer(format!("Path {:?} is not valid utf-8", path)))
    }

    async fn build_wasm(
        &self,
        timeouts: &Timeouts,
        deadline: SystemTime,
    ) -> ACResult<CompileResult> {
        info!("Building wasm");
        let _ = fs::remove_file(self.build.join(BUILT_WASM)).await;
        let output = Program::output_with_clean_env(
            "cargo",
            &[
                "build",
                "--frozen",
                "--target=wasm32-wasip1",
                "--release",
                "--message-format=json",
                "--quiet",
            ],
            &self.build,
            timeouts.command_for(deadline)?,
        )
        .await?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let compile_output = CompileOutput::from_stdout(&stdout);
        let error_count = compile_output.errors.len();
        if compile_output.success == Some(true) && error_count > 0 {
            return Err(ACError::InternalServer(
                "Build succeeded but detected compile errors".into(),
            ));
        }
        if compile_output.success != Some(true) {
            self.save_compile_output(&Self::raw_compile_output(&stdout, &stderr))
                .await?;
            let result = match compile_output.success {
                Some(false) => CompileStatus::Failure,
                _ => CompileStatus::Error,
            };
            let errors = if compile_output.errors.is_empty() {
                stderr
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
                    .collect()
            } else {
                compile_output.errors
            };
            return Ok(CompileResult {
                result,
                errors,
                compile_messages: compile_output.compile_messages,
            });
        }
        if !fs::try_exists(self.build.join(BUILT_WASM))
            .await
            .map_err(ACError::FileSystem)?
        {
            return Err(ACError::InternalServer("WASM file was not built".into()));
        }
        Ok(CompileResult {
            result: CompileStatus::Success,
            errors: compile_output.errors,
            compile_messages: vec![],
        })
    }

    fn raw_compile_output(stdout: &str, stderr: &str) -> String {
        format!("--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}\n")
    }

    async fn save_compile_output(&self, raw: &str) -> ACResult<()> {
        fs::write(self.compile_output_file(), raw)
            .await
            .map_err(ACError::FileSystem)
    }

    async fn compile_cwasm(&self, timeouts: &Timeouts, deadline: SystemTime) -> ACResult<()> {
        info!("Compiling cwasm");
        let _ = fs::remove_file(self.build.join(BUILT_CWASM)).await;
        Program::run_with_clean_env(
            "wasmtime",
            &["compile", "-C", "cache=no", BUILT_WASM, "-o", BUILT_CWASM],
            &self.build,
            timeouts.command_for(deadline)?,
        )
        .await?;
        Ok(())
    }

    async fn copy_dir(source: &Path, destination: &Path) -> ACResult<()> {
        fs::create_dir_all(destination)
            .await
            .map_err(ACError::FileSystem)?;
        let mut entries = fs::read_dir(source).await.map_err(ACError::FileSystem)?;
        while let Some(entry) = entries.next_entry().await.map_err(ACError::FileSystem)? {
            let file_type = entry.file_type().await.map_err(ACError::FileSystem)?;
            if file_type.is_dir()
                && IGNORED_DIRECTORIES.contains(&entry.file_name().to_string_lossy().as_ref())
            {
                continue;
            }
            let target = destination.join(entry.file_name());
            if file_type.is_dir() {
                let path = entry.path();
                Box::pin(Self::copy_dir(&path, &target)).await?;
            } else {
                fs::copy(entry.path(), target)
                    .await
                    .map_err(ACError::FileSystem)?;
            }
        }
        Ok(())
    }
}

impl CompileOutput {
    fn from_stdout(stdout: &str) -> Self {
        info!("Get compile output from stdout");
        let mut success = None;
        let mut errors = Vec::new();
        let mut compile_messages = Vec::new();
        for raw_line in stdout.lines() {
            let cleaned_line = raw_line.trim();
            if cleaned_line.is_empty() {
                continue;
            }
            let Ok(values) = serde_json::from_str::<HashMap<String, Value>>(cleaned_line) else {
                warn!("Ignoring non-JSON line in compile output: {cleaned_line}");
                continue;
            };
            let Some(reason) = values.get("reason").and_then(|value| value.as_str()) else {
                continue;
            };
            match reason {
                COMPILER_MESSAGE => {
                    if let Some(error) = CompileMessage::error(&values) {
                        errors.push(error);
                    }
                    compile_messages.extend(CompileMessage::from_values(&values));
                }
                BUILD_FINISHED => {
                    success = Some(
                        values
                            .get("success")
                            .and_then(|value| value.as_bool())
                            .unwrap_or(false),
                    );
                }
                _ => {}
            }
        }
        Self {
            success,
            errors,
            compile_messages,
        }
    }
}

impl CompileMessage {
    fn error(values: &HashMap<String, Value>) -> Option<String> {
        let message_dict = values.get("message").and_then(|value| value.as_object())?;
        let level = message_dict.get("level").and_then(|value| value.as_str())?;
        if level != LEVEL_ERROR {
            return None;
        }
        message_dict
            .get("message")
            .and_then(|value| value.as_str())
            .map(str::to_string)
    }

    fn from_values(values: &HashMap<String, Value>) -> Vec<Self> {
        let Some(message_dict) = values.get("message").and_then(|value| value.as_object()) else {
            return Vec::new();
        };
        let Some(level) = message_dict.get("level").and_then(|value| value.as_str()) else {
            return Vec::new();
        };
        let Some(message) = message_dict.get("message").and_then(|value| value.as_str()) else {
            return Vec::new();
        };
        let Some(spans) = message_dict.get("spans").and_then(|value| value.as_array()) else {
            return Vec::new();
        };
        spans
            .iter()
            .filter_map(|span| Self::from_span(level, message, span))
            .collect()
    }

    fn from_span(level: &str, message: &str, span: &Value) -> Option<Self> {
        let is_primary = span
            .get("is_primary")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if !is_primary {
            return None;
        }
        let file_name = span
            .get("file_name")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let Some(file) = Self::user_file(file_name) else {
            warn!(
                "Compile message from external file '{}': {} - {}",
                file_name, level, message
            );
            return None;
        };
        Some(Self {
            file,
            start: CodeLocation {
                line: CodeLocation::int(span, "line_start"),
                column: CodeLocation::int(span, "column_start"),
            },
            end: CodeLocation {
                line: CodeLocation::int(span, "line_end"),
                column: CodeLocation::int(span, "column_end"),
            },
            level: level.to_string(),
            message: message.to_string(),
        })
    }

    fn user_file(file_name: &str) -> Option<String> {
        let normalized = file_name.replace('\\', "/");
        let mut matched = None;
        for (index, _) in normalized.match_indices(ALGORITHMS_FOLDER) {
            let starts_at_separator = index == 0 || normalized.as_bytes()[index - 1] == b'/';
            let ends_at_separator = normalized
                .as_bytes()
                .get(index + ALGORITHMS_FOLDER.len())
                .is_some_and(|byte| *byte == b'/');
            if starts_at_separator && ends_at_separator {
                matched = Some(index);
            }
        }
        let index = matched?;
        Some(normalized[index..].to_string())
    }
}

impl CodeLocation {
    fn int(span: &Value, key: &str) -> i64 {
        span.get(key).and_then(|value| value.as_i64()).unwrap_or(0)
    }
}
