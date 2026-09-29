use crate::{
    dynamodb::{DynamoDb, DynamoDbDatumRef, DynamoDbLock},
    error::ACResult,
    s3::{S3, S3ObjectRef},
};
use std::path::Path;

pub struct FencedS3<'a> {
    dynamodb: &'a DynamoDb,
    s3: &'a S3,
    datum_ref: &'a DynamoDbDatumRef,
    lock: &'a DynamoDbLock,
}

impl<'a> FencedS3<'a> {
    pub fn new(
        dynamodb: &'a DynamoDb,
        s3: &'a S3,
        datum_ref: &'a DynamoDbDatumRef,
        lock: &'a DynamoDbLock,
    ) -> Self {
        Self {
            dynamodb,
            s3,
            datum_ref,
            lock,
        }
    }

    pub async fn upload(&self, object_ref: &S3ObjectRef, source_file_path: &Path) -> ACResult<()> {
        self.assert_lock_held().await?;
        self.s3.upload(object_ref, source_file_path).await
    }

    pub async fn upload_bytes(&self, object_ref: &S3ObjectRef, bytes: Vec<u8>) -> ACResult<()> {
        self.assert_lock_held().await?;
        self.s3.upload_bytes(object_ref, bytes).await
    }

    pub async fn delete_objects_with_prefix(&self, bucket: &str, prefix: &str) -> ACResult<()> {
        self.assert_lock_held().await?;
        self.s3.delete_objects_with_prefix(bucket, prefix).await
    }

    async fn assert_lock_held(&self) -> ACResult<()> {
        self.dynamodb
            .assert_lock_held(self.datum_ref, self.lock)
            .await
    }
}
