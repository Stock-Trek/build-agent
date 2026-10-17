use crate::error::{BaError, BaResult};
use std::{
    io::{Cursor, Read},
    path::{Component, Path, PathBuf},
};
use tokio::{fs, io::AsyncWriteExt};
use tracing::info;
use zip::ZipArchive;

pub const USER_GENERATED_DIR: &str = "runner/src/user_generated";

pub struct UserGenerated {
    url: String,
    root: PathBuf,
    client: reqwest::Client,
}

impl UserGenerated {
    pub fn new(url: String) -> Self {
        Self {
            url,
            root: PathBuf::from(USER_GENERATED_DIR),
            client: reqwest::Client::new(),
        }
    }

    pub async fn download(&self) -> BaResult<()> {
        let response = self
            .client
            .get(&self.url)
            .send()
            .await
            .map_err(BaError::DownloadUserGenerated)?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(BaError::DownloadUserGeneratedRejected {
                status: status.as_u16(),
                body,
            });
        }
        let archive = response
            .bytes()
            .await
            .map_err(BaError::ReadUserGeneratedResponse)?;

        self.extract(&archive).await?;
        info!(
            directory = %self.root.display(),
            "downloaded user generated code"
        );
        Ok(())
    }

    async fn extract(&self, archive: &[u8]) -> BaResult<()> {
        fs::create_dir_all(&self.root)
            .await
            .map_err(|source| BaError::WriteUserGenerated {
                path: self.root.clone(),
                source,
            })?;

        let mut archive =
            ZipArchive::new(Cursor::new(archive)).map_err(BaError::OpenUserGeneratedArchive)?;
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(BaError::OpenUserGeneratedArchive)?;
            let Some(relative) = entry.enclosed_name() else {
                return Err(BaError::Config(format!(
                    "user generated archive entry {index} has an unsafe path"
                )));
            };
            let relative = Self::relative_path(&relative)?;
            if relative.as_os_str().is_empty() {
                if entry.is_dir() {
                    continue;
                }
                return Err(BaError::Config(
                    "user generated archive contains an empty path".to_string(),
                ));
            }
            let destination = self.root.join(relative);

            if entry.is_dir() {
                fs::create_dir_all(&destination).await.map_err(|source| {
                    BaError::WriteUserGenerated {
                        path: destination,
                        source,
                    }
                })?;
                continue;
            }

            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .await
                    .map_err(|source| BaError::WriteUserGenerated {
                        path: parent.to_path_buf(),
                        source,
                    })?;
            }

            let mut contents = Vec::new();
            entry
                .read_to_end(&mut contents)
                .map_err(|source| BaError::ExtractUserGenerated {
                    path: destination.clone(),
                    source,
                })?;
            let mut file = fs::File::create(&destination).await.map_err(|source| {
                BaError::WriteUserGenerated {
                    path: destination.clone(),
                    source,
                }
            })?;
            file.write_all(&contents)
                .await
                .map_err(|source| BaError::WriteUserGenerated {
                    path: destination,
                    source,
                })?;
        }
        Ok(())
    }

    fn relative_path(path: &Path) -> BaResult<PathBuf> {
        let path = path.strip_prefix(USER_GENERATED_DIR).unwrap_or(path);
        let mut relative = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Normal(part) => relative.push(part),
                _ => {
                    return Err(BaError::Config(format!(
                        "user generated archive entry {} has an unsafe path",
                        path.display()
                    )));
                }
            }
        }
        Ok(relative)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::{ZipWriter, write::SimpleFileOptions};

    fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, contents) in files {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn source(root: PathBuf) -> UserGenerated {
        UserGenerated {
            url: String::new(),
            root,
            client: reqwest::Client::new(),
        }
    }

    #[tokio::test]
    async fn extracts_files_and_overwrites_existing_contents() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join(USER_GENERATED_DIR);
        fs::create_dir_all(root.join("nested")).await.unwrap();
        fs::write(root.join("algorithm.rs"), b"stub").await.unwrap();

        source(root.clone())
            .extract(&archive(&[
                ("algorithm.rs", b"generated"),
                ("nested/custom.rs", b"custom"),
            ]))
            .await
            .unwrap();

        assert_eq!(
            fs::read_to_string(root.join("algorithm.rs")).await.unwrap(),
            "generated"
        );
        assert_eq!(
            fs::read_to_string(root.join("nested/custom.rs"))
                .await
                .unwrap(),
            "custom"
        );
    }

    #[tokio::test]
    async fn strips_the_user_generated_directory_prefix() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join(USER_GENERATED_DIR);

        source(root.clone())
            .extract(&archive(&[(
                "runner/src/user_generated/algorithm.rs",
                b"generated",
            )]))
            .await
            .unwrap();

        assert_eq!(
            fs::read_to_string(root.join("algorithm.rs")).await.unwrap(),
            "generated"
        );
    }

    #[test]
    fn rejects_parent_directory_components() {
        assert!(UserGenerated::relative_path(Path::new("../algorithm.rs")).is_err());
    }
}
