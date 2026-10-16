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
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};
use tracing::info;

pub const SIGNED_POST_URL_ENV: &str = "SIGNED_POST_URL";
pub const SIGNED_POST_FIELDS_ENV: &str = "SIGNED_POST_FIELDS";

/// The form field that holds the object body in an S3 pre-signed POST.
pub const FILE_FIELD: &str = "file";

/// The S3 variable replaced with the uploaded file's name.
const FILENAME_VARIABLE: &str = "${filename}";

/// A pre-signed POST target for uploading build artifacts.
#[derive(Debug, Clone)]
pub struct SignedPost {
    url: String,
    fields: BTreeMap<String, String>,
    client: reqwest::Client,
}

impl SignedPost {
    /// Build the uploader from the environment.
    ///
    /// Returns `None` when neither variable is set, so the agent can still run
    /// outside of the artifact publishing flow. Setting only one of the two is
    /// treated as a configuration error.
    pub fn from_env() -> BaResult<Option<Self>> {
        let url = Self::optional_env(SIGNED_POST_URL_ENV)?;
        let fields = Self::optional_env(SIGNED_POST_FIELDS_ENV)?;
        match (url, fields) {
            (None, None) => Ok(None),
            (Some(_), None) => Err(BaError::Config(format!(
                "{SIGNED_POST_FIELDS_ENV} must be set when {SIGNED_POST_URL_ENV} is set"
            ))),
            (None, Some(_)) => Err(BaError::Config(format!(
                "{SIGNED_POST_URL_ENV} must be set when {SIGNED_POST_FIELDS_ENV} is set"
            ))),
            (Some(url), Some(fields)) => Ok(Some(Self {
                fields: Self::parse_fields(&fields)?,
                url,
                client: reqwest::Client::new(),
            })),
        }
    }

    /// Upload `path` using the configured pre-signed POST.
    pub async fn upload(&self, path: &Path) -> BaResult<()> {
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|source| BaError::ReadArtifact {
                path: path.to_path_buf(),
                source,
            })?;
        let file_name = Self::file_name(path)?;

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

        info!(artifact = %path.display(), "uploaded build artifact");
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

    fn parse_fields(json: &str) -> BaResult<BTreeMap<String, String>> {
        let value: Value = serde_json::from_str(json).map_err(|source| {
            BaError::Config(format!(
                "{SIGNED_POST_FIELDS_ENV} is not valid JSON: {source}"
            ))
        })?;
        let object = value.as_object().ok_or_else(|| {
            BaError::Config(format!("{SIGNED_POST_FIELDS_ENV} must be a JSON object"))
        })?;

        let mut fields = BTreeMap::new();
        for (name, value) in object {
            let text = match value {
                Value::String(text) => text.clone(),
                Value::Number(number) => number.to_string(),
                Value::Bool(boolean) => boolean.to_string(),
                _ => {
                    return Err(BaError::Config(format!(
                        "{SIGNED_POST_FIELDS_ENV}.{name} must be a string"
                    )));
                }
            };
            fields.insert(name.clone(), text);
        }
        Ok(fields)
    }

    fn optional_env(key: &str) -> BaResult<Option<String>> {
        match std::env::var(key) {
            Ok(value) if value.trim().is_empty() => Ok(None),
            Ok(value) => Ok(Some(value)),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(std::env::VarError::NotUnicode(_)) => {
                Err(BaError::Config(format!("{key} is not valid UTF-8")))
            }
        }
    }
}
