//! Compile the build agent's WebAssembly module and publish it to S3.
//!
//! The Docker image builds the `runner` crate to the exact module at
//! [`WASM_ARTIFACT`]. Publishing compiles that known module with `wasmtime
//! compile` into [`CWASM_ARTIFACT`] and uploads it, together with the contents
//! of [`USER_GENERATED_DIR`], using the pre-signed POST configured in
//! [`crate::s3`].

use crate::{
    error::{BaError, BaResult},
    s3::SignedPost,
};
use std::path::{Path, PathBuf};
use tokio::{fs, process::Command};
use tracing::info;

/// The `wasm32-wasip1` module the image builds from the `runner` crate.
const WASM_ARTIFACT: &str = "/app/target/wasm32-wasip1/release/runner.wasm";

/// The native `wasmtime` module compiled from [`WASM_ARTIFACT`].
const CWASM_ARTIFACT: &str = "/tmp/runner.cwasm";

/// Directory holding the sources generated for the algorithm being built.
const USER_GENERATED_DIR: &str = "runner/src/user_generated";

const WASMTIME: &str = "wasmtime";

/// Publishes the compiled artifact and the generated sources to S3.
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

    /// Compile the known module and upload it with the generated sources.
    pub async fn publish(&self) -> BaResult<()> {
        let cwasm = Self::compile().await?;
        self.signed_post.upload(&cwasm).await?;
        for source in Self::user_generated_files().await? {
            self.signed_post.upload(&source).await?;
        }
        info!("published build artifacts");
        Ok(())
    }

    /// Compile the exact WebAssembly module the image produced.
    async fn compile() -> BaResult<PathBuf> {
        let module = Path::new(WASM_ARTIFACT);
        let output = PathBuf::from(CWASM_ARTIFACT);
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

    /// Return every file under [`USER_GENERATED_DIR`].
    async fn user_generated_files() -> BaResult<Vec<PathBuf>> {
        let mut files = Vec::new();
        let mut pending = vec![PathBuf::from(USER_GENERATED_DIR)];
        while let Some(directory) = pending.pop() {
            let mut entries = fs::read_dir(&directory).await.map_err(|source| {
                BaError::ReadArtifactDirectory {
                    path: directory.clone(),
                    source,
                }
            })?;
            while let Some(entry) =
                entries
                    .next_entry()
                    .await
                    .map_err(|source| BaError::ReadArtifactDirectory {
                        path: directory.clone(),
                        source,
                    })?
            {
                let path = entry.path();
                let file_type =
                    entry
                        .file_type()
                        .await
                        .map_err(|source| BaError::ReadArtifactDirectory {
                            path: directory.clone(),
                            source,
                        })?;
                if file_type.is_dir() {
                    pending.push(path);
                } else {
                    files.push(path);
                }
            }
        }
        files.sort();
        Ok(files)
    }
}
