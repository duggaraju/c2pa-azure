//! S3 input delivered as "object created" notifications through SQS.
//!
//! Receiving a message hides it from other workers for the visibility timeout,
//! which acts as the lease. On success the input object and the message are
//! deleted; on failure the message becomes visible again for a retry (configure
//! a dead-letter queue on the queue to stop poison messages).
//!
//! Environment: `QUEUE_URL`, `OUTPUT_BUCKET` (must differ from the input bucket
//! so signed files do not trigger new notifications), and optionally
//! `VISIBILITY_TIMEOUT` in seconds (default 300). Credentials and region come
//! from the standard AWS provider chain.

use std::{env, fmt, io::Write, path::Path};

use anyhow::{Context as _, Result, bail};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_sqs::types::Message;
use serde::Deserialize;

use crate::{WorkSource, media_type};

pub struct SqsS3Source {
    s3: aws_sdk_s3::Client,
    sqs: aws_sdk_sqs::Client,
    queue_url: String,
    output_bucket: String,
    visibility_timeout: i32,
}

impl SqsS3Source {
    pub async fn from_env() -> Result<Self> {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let visibility_timeout = match env::var("VISIBILITY_TIMEOUT") {
            Ok(value) => value
                .parse()
                .context("VISIBILITY_TIMEOUT must be a number of seconds")?,
            Err(_) => 300,
        };
        Ok(Self {
            s3: aws_sdk_s3::Client::new(&config),
            sqs: aws_sdk_sqs::Client::new(&config),
            queue_url: env::var("QUEUE_URL").context("missing QUEUE_URL")?,
            output_bucket: env::var("OUTPUT_BUCKET").context("missing OUTPUT_BUCKET")?,
            visibility_timeout,
        })
    }

    async fn delete_message(&self, receipt_handle: &str) -> Result<()> {
        self.sqs
            .delete_message()
            .queue_url(&self.queue_url)
            .receipt_handle(receipt_handle)
            .send()
            .await?;
        Ok(())
    }
}

pub struct S3Upload {
    bucket: String,
    key: String,
    receipt_handle: String,
}

impl fmt::Display for S3Upload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "s3://{}/{}", self.bucket, self.key)
    }
}

#[derive(Deserialize)]
struct S3Event {
    #[serde(rename = "Records", default)]
    records: Vec<S3EventRecord>,
}

#[derive(Deserialize)]
struct S3EventRecord {
    s3: S3Entity,
}

#[derive(Deserialize)]
struct S3Entity {
    bucket: S3Bucket,
    object: S3Object,
}

#[derive(Deserialize)]
struct S3Bucket {
    name: String,
}

#[derive(Deserialize)]
struct S3Object {
    key: String,
}

/// Returns the bucket and decoded key of an S3 event notification, or `None`
/// for messages without records such as `s3:TestEvent`.
fn parse_notification(body: &str) -> Result<Option<(String, String)>> {
    let event: S3Event =
        serde_json::from_str(body).context("message is not an S3 event notification")?;
    match event.records.as_slice() {
        [] => Ok(None),
        [record] => {
            // Keys are URL-encoded, with `+` for spaces.
            let key = record.s3.object.key.replace('+', " ");
            let key = percent_encoding::percent_decode_str(&key).decode_utf8()?;
            Ok(Some((record.s3.bucket.name.clone(), key.into_owned())))
        }
        records => bail!("expected one S3 record per message, got {}", records.len()),
    }
}

#[async_trait::async_trait(?Send)]
impl WorkSource for SqsS3Source {
    type Item = Message;
    type Claim = S3Upload;

    async fn list(&self) -> Result<Vec<Message>> {
        let output = self
            .sqs
            .receive_message()
            .queue_url(&self.queue_url)
            .max_number_of_messages(10)
            .wait_time_seconds(20)
            .visibility_timeout(self.visibility_timeout)
            .send()
            .await?;
        Ok(output.messages().to_vec())
    }

    async fn claim(&self, message: Message) -> Result<S3Upload> {
        let receipt_handle = message
            .receipt_handle()
            .context("message without a receipt handle")?
            .to_owned();
        match parse_notification(message.body().unwrap_or_default())? {
            Some((bucket, key)) => Ok(S3Upload {
                bucket,
                key,
                receipt_handle,
            }),
            None => {
                self.delete_message(&receipt_handle).await?;
                bail!("deleted a message without S3 records")
            }
        }
    }

    async fn download(&self, claim: &S3Upload, file: &mut std::fs::File) -> Result<String> {
        let mut object = self
            .s3
            .get_object()
            .bucket(&claim.bucket)
            .key(&claim.key)
            .send()
            .await?;
        while let Some(chunk) = object.body.next().await {
            file.write_all(&chunk?)?;
        }
        Ok(media_type(object.content_type(), &claim.key))
    }

    async fn upload(&self, claim: &S3Upload, signed: &Path, media_type: &str) -> Result<()> {
        let mut request = self
            .s3
            .put_object()
            .bucket(&self.output_bucket)
            .key(&claim.key)
            .body(ByteStream::from_path(signed).await?);
        if media_type.contains('/') {
            request = request.content_type(media_type);
        }
        request.send().await?;
        log::info!("Uploaded s3://{}/{}", self.output_bucket, claim.key);
        Ok(())
    }

    async fn finish(&self, claim: S3Upload, signed: bool) -> Result<()> {
        if signed {
            self.s3
                .delete_object()
                .bucket(&claim.bucket)
                .key(&claim.key)
                .send()
                .await?;
            self.delete_message(&claim.receipt_handle).await
        } else {
            self.sqs
                .change_message_visibility()
                .queue_url(&self.queue_url)
                .receipt_handle(&claim.receipt_handle)
                .visibility_timeout(0)
                .send()
                .await?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_notification;

    #[test]
    fn parses_object_created_notification() {
        let body = r#"{"Records":[{"eventName":"ObjectCreated:Put","s3":{"bucket":{"name":"input"},"object":{"key":"photos/my+photo%281%29.jpg","size":10}}}]}"#;
        assert_eq!(
            parse_notification(body).unwrap(),
            Some(("input".to_owned(), "photos/my photo(1).jpg".to_owned()))
        );
    }

    #[test]
    fn ignores_test_events_and_rejects_garbage() {
        let test_event = r#"{"Service":"Amazon S3","Event":"s3:TestEvent","Bucket":"input"}"#;
        assert_eq!(parse_notification(test_event).unwrap(), None);
        assert!(parse_notification("not json").is_err());
    }
}
