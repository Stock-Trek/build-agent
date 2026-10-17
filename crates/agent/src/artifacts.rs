use crate::{
    error::{BaError, BaResult},
    s3::SignedPost,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tempfile::TempDir;
use tokio::{fs, process::Command};
use tracing::info;

const WASM_MANIFEST: &str = "runner/Cargo.toml";
const WASM_TARGET: &str = "wasm32-wasip1";
const WASM_ARTIFACT_FILE: &str = "runner.wasm";
const CWASM_ARTIFACT_FILE: &str = "runner.cwasm";
const TARGET_DIR: &str = "target";
const USER_GENERATED_DIR: &str = "runner/src/user_generated";
const CARGO: &str = "cargo";
const WASMTIME: &str = "wasmtime";

#[derive(Clone)]
pub struct Artifacts {
    signed_post: SignedPost,
    build_dir: Arc<TempDir>,
}

impl Artifacts {
    pub fn new(signed_post: SignedPost) -> BaResult<Self> {
        let build_dir = tempfile::tempdir().map_err(BaError::CreateBuildDirectory)?;
        Ok(Self {
            signed_post,
            build_dir: Arc::new(build_dir),
        })
    }

    pub async fn compile(&self) -> BaResult<PathBuf> {
        let wasm = self.build_wasm().await?;
        self.compile_wasm(&wasm).await
    }

    async fn build_wasm(&self) -> BaResult<PathBuf> {
        let target_dir = self.build_dir.path().join(TARGET_DIR);
        let module = target_dir
            .join(WASM_TARGET)
            .join("release")
            .join(WASM_ARTIFACT_FILE);
        let status = Command::new(CARGO)
            .arg("build")
            .arg("--manifest-path")
            .arg(WASM_MANIFEST)
            .arg("--target")
            .arg(WASM_TARGET)
            .arg("--target-dir")
            .arg(&target_dir)
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
            target_dir = %target_dir.display(),
            "built wasm artifact"
        );
        Ok(module)
    }

    async fn compile_wasm(&self, module: &Path) -> BaResult<PathBuf> {
        let output = self.build_dir.path().join(CWASM_ARTIFACT_FILE);
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
        let root = Path::new(USER_GENERATED_DIR);
        for source in Self::user_generated_files().await? {
            let name = source.strip_prefix(root).map_err(|_| {
                BaError::Config(format!(
                    "artifact path {} is outside {USER_GENERATED_DIR}",
                    source.display()
                ))
            })?;
            self.signed_post.upload_named(&source, name).await?;
        }
        info!("published build artifacts");
        Ok(())
    }

    async fn user_generated_files() -> BaResult<Vec<PathBuf>> {
        let root = fs::canonicalize(USER_GENERATED_DIR)
            .await
            .map_err(|source| BaError::ReadArtifactDirectory {
                path: PathBuf::from(USER_GENERATED_DIR),
                source,
            })?;
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
                let resolved =
                    fs::canonicalize(&path)
                        .await
                        .map_err(|source| BaError::ReadArtifact {
                            path: path.clone(),
                            source,
                        })?;
                if !resolved.starts_with(&root) {
                    return Err(BaError::Config(format!(
                        "artifact path {} resolves outside {USER_GENERATED_DIR}",
                        path.display()
                    )));
                }

                let target =
                    fs::metadata(&resolved)
                        .await
                        .map_err(|source| BaError::ReadArtifact {
                            path: path.clone(),
                            source,
                        })?;
                if target.is_dir() {
                    if file_type.is_symlink() {
                        return Err(BaError::Config(format!(
                            "artifact path {} is a symlinked directory",
                            path.display()
                        )));
                    }
                    pending.push(path);
                } else if target.is_file() {
                    files.push(path);
                } else {
                    return Err(BaError::Config(format!(
                        "artifact path {} is not a regular file or directory",
                        path.display()
                    )));
                }
            }
        }
        files.sort();
        Ok(files)
    }
}
