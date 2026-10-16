//! Discover the compiled WebAssembly artifacts produced by a build and publish
//! them to S3.
//!
//! The Docker image ships the `runner` crate and compiles it to a
//! `wasm32-wasip1` module at image build time. Build agents also run the
//! compiler at request time, producing a native `wasmtime` compiled module
//! (`.cwasm`). This module finds every `.cwasm` file, compiling any raw `.wasm`
//! module that does not already have a matching `.cwasm`, and uploads it along
//! with the contents of `runner/src/user_generated` using the pre-signed POST
//! configured in [`crate::s3`].

use crate::{
    error::{BaError, BaResult},
    s3::SignedPost,
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use tokio::{fs, process::Command};
use tracing::{info, warn};

/// Overrides the directories searched for build artifacts.
pub const ARTIFACT_DIR_ENV: &str = "BUILD_AGENT_ARTIFACT_DIR";

const CWASM_EXTENSION: &str = "cwasm";
const WASM_EXTENSION: &str = "wasm";
const WASMTIME: &str = "wasmtime";

/// Directory holding the sources generated for the algorithm being built.
const USER_GENERATED_DIR: &str = "runner/src/user_generated";

/// Locations the build artifacts are known to end up in. Missing directories
/// are skipped, so it is safe to list every layout the project has used.
const DEFAULT_ARTIFACT_DIRS: &[&str] = &[
    // The native compiler build directory.
    "/tmp/algorithm-compiler/build",
    // Pre-compiled artifacts baked into the image.
    "/usr/local/share/build-agent",
    // `cargo build --target=wasm32-wasip1` output when the image is built with
    // the toolbox.
    "/app/target/wasm32-wasip1/release",
];

/// Publishes the compiled artifacts and their generated sources to S3.
pub struct Artifacts {
    signed_post: SignedPost,
}

impl Artifacts {
    /// Build the publisher from the environment.
    ///
    /// Returns `None` when no signed post is configured, so the agent can still
    /// run without artifact publishing.
    pub fn from_env() -> BaResult<Option<Self>> {
        match SignedPost::from_env()? {
            Some(signed_post) => Ok(Some(Self { signed_post })),
            None => {
                info!("no signed post configured, skipping artifact upload");
                Ok(None)
            }
        }
    }

    /// Upload the compiled `.cwasm` and the generated sources.
    pub async fn publish(&self) -> BaResult<()> {
        let artifacts = Self::collect().await?;
        if artifacts.is_empty() {
            warn!("no artifacts found to upload");
            return Ok(());
        }
        for artifact in &artifacts {
            self.signed_post.upload(artifact).await?;
        }
        info!(count = artifacts.len(), "published build artifacts");
        Ok(())
    }

    /// Return the compiled `.cwasm` files and the generated sources.
    async fn collect() -> BaResult<Vec<PathBuf>> {
        let mut artifacts = BTreeSet::new();
        for directory in Self::artifact_directories() {
            Self::collect_compiled(&directory, &mut artifacts).await?;
        }
        Self::collect_all(Path::new(USER_GENERATED_DIR), &mut artifacts).await?;
        Ok(artifacts.into_iter().collect())
    }

    fn artifact_directories() -> Vec<PathBuf> {
        match std::env::var(ARTIFACT_DIR_ENV) {
            Ok(value) if !value.trim().is_empty() => vec![PathBuf::from(value)],
            _ => DEFAULT_ARTIFACT_DIRS.iter().map(PathBuf::from).collect(),
        }
    }

    /// Collect every `.cwasm` under `directory`, compiling `.wasm` modules that
    /// do not already have a matching compiled module.
    async fn collect_compiled(directory: &Path, artifacts: &mut BTreeSet<PathBuf>) -> BaResult<()> {
        let Some(entries) = Self::read_dir(directory).await? else {
            return Ok(());
        };

        let mut raw_modules = Vec::new();
        for path in entries {
            match path.extension().and_then(|extension| extension.to_str()) {
                Some(CWASM_EXTENSION) => {
                    artifacts.insert(path);
                }
                Some(WASM_EXTENSION) => raw_modules.push(path),
                _ => {}
            }
        }

        for module in raw_modules {
            if artifacts
                .iter()
                .any(|compiled| Self::same_stem(compiled, &module))
            {
                continue;
            }
            artifacts.insert(Self::compile(&module).await?);
        }
        Ok(())
    }

    /// Collect every file under `directory`, walking nested directories.
    async fn collect_all(directory: &Path, files: &mut BTreeSet<PathBuf>) -> BaResult<()> {
        let mut pending = vec![directory.to_path_buf()];
        while let Some(directory) = pending.pop() {
            let Some(entries) = Self::read_dir(&directory).await? else {
                continue;
            };
            for path in entries {
                if path.is_dir() {
                    pending.push(path);
                } else {
                    files.insert(path);
                }
            }
        }
        Ok(())
    }

    async fn read_dir(directory: &Path) -> BaResult<Option<Vec<PathBuf>>> {
        let mut entries = match fs::read_dir(directory).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(BaError::ReadArtifactDirectory {
                    path: directory.to_path_buf(),
                    source,
                });
            }
        };

        let mut paths = Vec::new();
        while let Some(entry) =
            entries
                .next_entry()
                .await
                .map_err(|source| BaError::ReadArtifactDirectory {
                    path: directory.to_path_buf(),
                    source,
                })?
        {
            paths.push(entry.path());
        }
        Ok(Some(paths))
    }

    fn same_stem(left: &Path, right: &Path) -> bool {
        left.file_stem() == right.file_stem()
    }

    /// Compile a `wasm32-wasip1` module into a native `wasmtime` artifact. The
    /// output lives in the temporary directory because the container root
    /// filesystem is read-only.
    async fn compile(module: &Path) -> BaResult<PathBuf> {
        let file_name = module
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| BaError::Config(format!("artifact path {module:?} has no file name")))?;
        let output = std::env::temp_dir().join(format!("{file_name}.{CWASM_EXTENSION}"));
        // Reuse a previously compiled module so repeated publishes do not recompile.
        if fs::try_exists(&output).await.unwrap_or(false) {
            return Ok(output);
        }

        let status = Command::new(WASMTIME)
            .arg("compile")
            .arg("-C")
            .arg("cache=no")
            .arg(module)
            .arg("-o")
            .arg(&output)
            .status()
            .await
            .map_err(|source| BaError::CompileArtifact {
                path: module.to_path_buf(),
                source,
            })?;

        if !status.success() {
            return Err(BaError::CompileArtifactFailed {
                path: module.to_path_buf(),
                status,
            });
        }
        info!(module = %module.display(), output = %output.display(), "compiled wasm artifact");
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_directories_are_used_without_override() {
        // `ARTIFACT_DIR_ENV` is not set in the test environment.
        let directories = Artifacts::artifact_directories();
        assert_eq!(directories.len(), DEFAULT_ARTIFACT_DIRS.len());
    }

    #[test]
    fn matching_stems_are_detected() {
        assert!(Artifacts::same_stem(
            Path::new("/a/runner.cwasm"),
            Path::new("/b/runner.wasm")
        ));
        assert!(!Artifacts::same_stem(
            Path::new("/a/other.cwasm"),
            Path::new("/b/runner.wasm")
        ));
    }
}
