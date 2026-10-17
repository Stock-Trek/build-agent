//! Upload build artifacts to S3 using a pre-signed POST policy.
//!
//! `ref-sync` starts a build agent with a short-lived pre-signed POST so the
//! agent can write its artifacts without any long-lived AWS credentials. The
//! url and form fields are provided through the `SIGNED_POST_URL` and
//! `SIGNED_POST_FIELDS` environment variables. `SIGNED_POST_FIELDS` is a JSON
//! object holding the policy fields that must be sent alongside the file (for
//! example `key`, `policy`, `x-amz-signature`, ...).
//!
//! A single pre-signed POST can upload more than one artifact when the `key`
//! field contains the `${filename}` variable, which S3 replaces with the name
//! of the uploaded file.

use crate::error::{BaError, BaResult};
use std::{
    collections::BTreeMap,
    path::{Component, Path},
};
use tracing::info;

/// The form field that holds the object body in an S3 pre-signed POST.
pub const FILE_FIELD: &str = "file";

/// The S3 variable replaced with the uploaded file's name.
const FILENAME_VARIABLE: &str = "${filename}";

/// The form field that selects the S3 object key.
const KEY_FIELD: &str = "key";

/// A pre-signed POST target for uploading build artifacts.
#[derive(Debug, Clone)]
pub struct SignedPost {
    url: String,
    fields: BTreeMap<String, String>,
    client: reqwest::Client,
}

impl SignedPost {
    pub fn new(url: String, fields: BTreeMap<String, String>) -> BaResult<Self> {
        let key = fields.get(KEY_FIELD).ok_or_else(|| {
            BaError::Config(format!(
                "SIGNED_POST_FIELDS must include a {KEY_FIELD} field containing {FILENAME_VARIABLE}"
            ))
        })?;
        if !key.contains(FILENAME_VARIABLE) {
            return Err(BaError::Config(format!(
                "SIGNED_POST_FIELDS.{KEY_FIELD} must contain {FILENAME_VARIABLE} so artifacts do not overwrite each other"
            )));
        }

        Ok(Self {
            url,
            fields,
            client: reqwest::Client::new(),
        })
    }

    /// Upload `path` using the configured pre-signed POST.
    pub async fn upload(&self, path: &Path) -> BaResult<()> {
        let file_name = Self::file_name(path)?;
        self.upload_named(path, Path::new(&file_name)).await
    }

    /// Upload `path` using `name` as the S3 object name.
    pub async fn upload_named(&self, path: &Path, name: &Path) -> BaResult<()> {
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|source| BaError::ReadArtifact {
                path: path.to_path_buf(),
                source,
            })?;
        let file_name = Self::upload_name(name)?;

        let file = reqwest::multipart::Part::bytes(bytes)
            .file_name(file_name.clone())
            .mime_str("application/octet-stream")
            .map_err(|source| BaError::BuildUploadRequest {
                path: path.to_path_buf(),
                source,
            })?;

        // S3 requires the file to be the last field in the multipart body.
        let mut form = reqwest::multipart::Form::new();
        for (name, value) in &self.fields {
            form = form.text(name.clone(), Self::field_value(value, &file_name));
        }
        form = form.part(FILE_FIELD, file);

        let response = self
            .client
            .post(&self.url)
            .multipart(form)
            .send()
            .await
            .map_err(|source| BaError::UploadArtifact {
                path: path.to_path_buf(),
                source,
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(BaError::UploadRejected {
                path: path.to_path_buf(),
                status: status.as_u16(),
                body,
            });
        }

        info!(artifact = %path.display(), object = %file_name, "uploaded build artifact");
        Ok(())
    }

    fn field_value(value: &str, file_name: &str) -> String {
        value.replace(FILENAME_VARIABLE, file_name)
    }

    fn file_name(path: &Path) -> BaResult<String> {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .ok_or_else(|| BaError::Config(format!("artifact path {path:?} has no file name")))
    }

    fn upload_name(path: &Path) -> BaResult<String> {
        let mut parts = Vec::new();
        for component in path.components() {
            match component {
                Component::Normal(part) => {
                    let part = part.to_str().ok_or_else(|| {
                        BaError::Config(format!("artifact name {path:?} is not valid UTF-8"))
                    })?;
                    parts.push(part);
                }
                _ => {
                    return Err(BaError::Config(format!(
                        "artifact name {path:?} must be relative"
                    )));
                }
            }
        }

        if parts.is_empty() {
            return Err(BaError::Config(format!("artifact name {path:?} is empty")));
        }
        Ok(parts.join("/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(key: &str) -> BTreeMap<String, String> {
        BTreeMap::from([(KEY_FIELD.to_string(), key.to_string())])
    }

    #[test]
    fn rejects_fields_without_key() {
        let result = SignedPost::new("https://example.com".to_string(), BTreeMap::new());
        assert!(matches!(result, Err(BaError::Config(_))));
    }

    #[test]
    fn rejects_key_without_filename_variable() {
        let result = SignedPost::new(
            "https://example.com".to_string(),
            fields("artifacts/runner.cwasm"),
        );
        assert!(matches!(result, Err(BaError::Config(_))));
    }

    #[test]
    fn accepts_key_with_filename_variable() {
        let result = SignedPost::new(
            "https://example.com".to_string(),
            fields("artifacts/${filename}"),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn upload_names_are_relative() {
        assert_eq!(
            SignedPost::upload_name(Path::new("nested/artifact.rs")).unwrap(),
            "nested/artifact.rs"
        );
        assert!(SignedPost::upload_name(Path::new("/absolute.rs")).is_err());
        assert!(SignedPost::upload_name(Path::new("../escape.rs")).is_err());
    }
}
