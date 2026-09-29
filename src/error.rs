use aws_sdk_dynamodb::operation::{
    delete_item::DeleteItemError, put_item::PutItemError, update_item::UpdateItemError,
};
use aws_sdk_s3::operation::{
    delete_objects::DeleteObjectsError, get_object::GetObjectError,
    list_objects_v2::ListObjectsV2Error, put_object::PutObjectError,
};
use aws_sdk_sqs::operation::send_message::SendMessageError;
use aws_smithy_types::byte_stream;
use std::fmt::Debug;

pub type ACResult<T> = Result<T, ACError>;

#[derive(Debug, thiserror::Error)]
pub enum ACError {
    #[error("Build: {0}")]
    Build(aws_smithy_types::error::operation::BuildError),
    #[error("ByteStream: {0}")]
    ByteStream(Box<byte_stream::error::Error>),
    #[error("CommandRun: {0}")]
    CommandRun(std::io::Error),
    #[error("CommandOutput: {0}")]
    CommandOutput(String),
    #[error("Config: {0}")]
    Config(String),
    #[error("DynamoDbDeleteItem: {0}")]
    DynamoDbDeleteItem(Box<DeleteItemError>),
    #[error("DynamoDbPutItem: {0}")]
    DynamoDbPutItem(Box<PutItemError>),
    #[error("DynamoDbUpdateItem: {0}")]
    DynamoDbUpdateItem(Box<UpdateItemError>),
    #[error("FileSystem: {0}")]
    FileSystem(std::io::Error),
    #[error("GitHub: {0}")]
    GitHub(String),
    #[error("InternalServer: {0}")]
    InternalServer(String),
    #[error("InvalidMessage: {0}")]
    InvalidMessage(String),
    #[error("LockLost: {0}")]
    LockLost(String),
    #[error("LockTimeout: {0}")]
    LockTimeout(String),
    #[error("S3DeleteObjects: {0}")]
    S3DeleteObjects(Box<DeleteObjectsError>),
    #[error("S3DeleteObjectsPartial: {0}")]
    S3DeleteObjectsPartial(String),
    #[error("S3GetObject: {0}")]
    S3GetObject(Box<GetObjectError>),
    #[error("S3ListObjects: {0}")]
    S3ListObjects(Box<ListObjectsV2Error>),
    #[error("S3PutObject: {0}")]
    S3PutObject(Box<PutObjectError>),
    #[error("SqsSendMessage: {0}")]
    SqsSendMessage(Box<SendMessageError>),
    #[error("TaskJoin: {0}")]
    TaskJoin(tokio::task::JoinError),
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("UserError: {0}")]
    UserError(String),
}

impl ACError {
    pub fn is_retryable(&self) -> bool {
        !matches!(self, Self::InvalidMessage(_))
    }
}
