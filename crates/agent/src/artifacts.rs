use crate::{
    error::{BaError, BaResult},
    s3::SignedPost,
};
use std::path::{Path, PathBuf};
use tokio::{fs, process::Command};
use tracing::info;

const WASM_MANIFEST: &str = "runner/Cargo.toml";
const WASM_TARGET: &str = "wasm32-wasip1";
const WASM_ARTIFACT: &str = "/app/target/wasm32-wasip1/release/runner.wasm";
const CWASM_ARTIFACT: &str = "/tmp/runner.cwasm";
const USER_GENERATED_DIR: &str = "runner/src/user_generated";
const CARGO: &str = "cargo";
const WASMTIME: &str = "wasmtime";

#[derive(Clone)]
pub struct Artifacts {
    signed_post: SignedPost,
}

impl Artifacts {
    pub fn new(signed_post: SignedPost) -> Self {
        Self { signed_post }
    }

    pub async fn compile() -> BaResult<PathBuf> {
        Self::build_wasm().await?;
        Self::compile_wasm().await
    }

    async fn build_wasm() -> BaResult<()> {
        let module = PathBuf::from(WASM_ARTIFACT);
        let status = Command::new(CARGO)
            .arg("build")
            .arg("--manifest-path")
            .arg(WASM_MANIFEST)
            .arg("--target")
            .arg(WASM_TARGET)
            .arg("--release")
            .arg("--lib")
            .kill_on_drop(true)
            .status()
            .await
            .map_err(|source| BaError::CompileArtifact {
                path: module.clone(),
                source,
            })?;

        if !status.success() {
            return Err(BaError::CompileArtifactFailed {
                path: module,
                status,
            });
        }
        info!(
            manifest = WASM_MANIFEST,
            target = WASM_TARGET,
            "built wasm artifact"
        );
        Ok(())
    }

    async fn compile_wasm() -> BaResult<PathBuf> {
        let module = Path::new(WASM_ARTIFACT);
        let output = PathBuf::from(CWASM_ARTIFACT);
        let status = Command::new(WASMTIME)
            .arg("compile")
            .arg("-C")
            .arg("cache=no")
            .arg(module)
            .arg("-o")
            .arg(&output)
            .kill_on_drop(true)
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

    pub async fn publish(&self, cwasm: &Path) -> BaResult<()> {
        self.signed_post.upload(cwasm).await?;
        for source in Self::user_generated_files().await? {
            self.signed_post.upload(&source).await?;
        }
        info!("published build artifacts");
        Ok(())
    }

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
