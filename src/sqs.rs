use crate::{
    dto::sqs_event::SqsMessage,
    error::{ACError, ACResult},
};
use aws_sdk_sqs::Client as SqsClient;

pub struct Sqs {
    pub client: SqsClient,
    pub url: String,
}

impl Sqs {
    pub async fn send(&self, message: &SqsMessage) -> ACResult<()> {
        let body = serde_json::to_string(message).map_err(|error| {
            ACError::InternalServer(format!("Failed to serialize SQS message: {error}"))
        })?;
        self.client
            .send_message()
            .queue_url(&self.url)
            .message_body(body)
            .send()
            .await
            .map_err(|error| ACError::SqsSendMessage(Box::new(error.into_service_error())))?;
        Ok(())
    }
}
