use crate::error::{ACError, ACResult};
use aws_sdk_s3::{
    Client as S3Client,
    types::{Delete, ObjectIdentifier},
};
use aws_smithy_types::byte_stream::ByteStream;
use std::path::Path;
use std::time::Duration;

#[derive(Debug)]
pub struct S3ObjectRef {
    pub bucket: String,
    pub key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadOutcome {
    Downloaded,
    NotFound,
}

pub struct S3 {
    pub client: S3Client,
    pub operation_timeout: Duration,
}

impl S3 {
    pub async fn upload(&self, object_ref: &S3ObjectRef, source_file_path: &Path) -> ACResult<()> {
        let byte_stream = ByteStream::from_path(source_file_path)
            .await
            .map_err(|e| ACError::ByteStream(Box::new(e)))?;
        self.put(object_ref, byte_stream).await
    }

    pub async fn upload_bytes(&self, object_ref: &S3ObjectRef, bytes: Vec<u8>) -> ACResult<()> {
        self.put(object_ref, ByteStream::from(bytes)).await
    }

    async fn put(&self, object_ref: &S3ObjectRef, body: ByteStream) -> ACResult<()> {
        self.client
            .put_object()
            .bucket(&object_ref.bucket)
            .key(&object_ref.key)
            .body(body)
            .send()
            .await
            .map_err(|e| ACError::S3PutObject(Box::new(e.into_service_error())))?;
        Ok(())
    }

    pub async fn download(
        &self,
        object_ref: &S3ObjectRef,
        sink_file_path: &Path,
    ) -> ACResult<DownloadOutcome> {
        let result = self
            .client
            .get_object()
            .bucket(&object_ref.bucket)
            .key(&object_ref.key)
            .send()
            .await;
        match result {
            Ok(output) => {
                if let Err(error) = Self::write_body(
                    object_ref,
                    output.body,
                    sink_file_path,
                    self.operation_timeout,
                )
                .await
                {
                    let _ = tokio::fs::remove_file(sink_file_path).await;
                    return Err(error);
                }
                Ok(DownloadOutcome::Downloaded)
            }
            Err(error) => {
                if error
                    .as_service_error()
                    .map(|e| e.is_no_such_key())
                    .unwrap_or(false)
                {
                    Ok(DownloadOutcome::NotFound)
                } else {
                    Err(ACError::S3GetObject(Box::new(error.into_service_error())))
                }
            }
        }
    }

    async fn write_body(
        object_ref: &S3ObjectRef,
        body: ByteStream,
        sink_file_path: &Path,
        operation_timeout: Duration,
    ) -> ACResult<()> {
        let mut body = body.into_async_read();
        let mut file = tokio::fs::File::create(sink_file_path)
            .await
            .map_err(ACError::FileSystem)?;
        tokio::time::timeout(operation_timeout, async {
            tokio::io::copy(&mut body, &mut file).await?;
            tokio::io::AsyncWriteExt::flush(&mut file).await
        })
        .await
        .map_err(|_| {
            ACError::Timeout(format!(
                "S3 download {}/{} timed out",
                object_ref.bucket, object_ref.key
            ))
        })?
        .map_err(ACError::FileSystem)?;
        Ok(())
    }

    pub async fn list_keys_with_prefix(&self, bucket: &str, prefix: &str) -> ACResult<Vec<String>> {
        let mut paginator = self
            .client
            .list_objects_v2()
            .bucket(bucket)
            .prefix(prefix)
            .into_paginator()
            .send();
        let mut keys = Vec::new();
        while let Some(page) = paginator.next().await {
            let page =
                page.map_err(|e| ACError::S3ListObjects(Box::new(e.into_service_error())))?;
            for object in page.contents() {
                if let Some(key) = object.key() {
                    keys.push(key.to_string());
                }
            }
        }
        Ok(keys)
    }

    pub async fn delete_objects_with_prefix(&self, bucket: &str, prefix: &str) -> ACResult<()> {
        let mut paginator = self
            .client
            .list_objects_v2()
            .bucket(bucket)
            .prefix(prefix)
            .into_paginator()
            .send();
        let mut objects_to_delete: Vec<ObjectIdentifier> = Vec::new();
        while let Some(page) = paginator.next().await {
            let page =
                page.map_err(|e| ACError::S3ListObjects(Box::new(e.into_service_error())))?;
            for obj in page.contents() {
                if let Some(key) = obj.key() {
                    objects_to_delete.push(
                        ObjectIdentifier::builder()
                            .key(key)
                            .build()
                            .map_err(ACError::Build)?,
                    );
                }
            }
        }
        if objects_to_delete.is_empty() {
            return Ok(());
        }
        for chunk in objects_to_delete.chunks(1000) {
            let delete = Delete::builder()
                .set_objects(Some(chunk.to_vec()))
                .build()
                .map_err(ACError::Build)?;
            let output = self
                .client
                .delete_objects()
                .bucket(bucket)
                .delete(delete)
                .send()
                .await
                .map_err(|e| ACError::S3DeleteObjects(Box::new(e.into_service_error())))?;
            let errors = output.errors();
            if !errors.is_empty() {
                let failures = errors
                    .iter()
                    .map(|failure| {
                        format!(
                            "key={:?} code={:?} message={:?}",
                            failure.key(),
                            failure.code(),
                            failure.message()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(ACError::S3DeleteObjectsPartial(format!(
                    "Failed to delete {} object(s) under {bucket}/{prefix}: {failures}",
                    errors.len()
                )));
            }
        }
        Ok(())
    }
}
