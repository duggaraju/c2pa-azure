//! KEDA-scaled worker that adds content credentials to files dropped into
//! cloud storage.
//!
//! The signing loop is cloud-agnostic; each cloud provides a [`WorkSource`]:
//!
//! - `azure`: Azure Blob Storage container, claimed with blob leases
//!   (KEDA `azure-blob` scaler).
//! - `aws`: S3 "object created" notifications delivered through SQS, claimed
//!   with the message visibility timeout (KEDA `aws-sqs-queue` scaler).
//! - `gcp`: Google Cloud Storage bucket, best effort without a lock
//!   (KEDA `gcp-storage` scaler).
//!
//! Signed files are written to the output container/bucket under the same
//! name and the input is deleted once it has been signed.

#[cfg(feature = "aws")]
mod aws;
#[cfg(feature = "azure")]
mod azure;
#[cfg(feature = "gcp")]
mod gcp;

use std::{fmt::Display, fs::File, io::Seek, path::Path};

use anyhow::Result;
use async_trait::async_trait;
use c2pa::{AsyncSigner, Builder, Context, ManifestDefinition};
use examples_common::Cloud;

/// Storage-specific half of the worker. One KEDA job processes one batch.
#[async_trait(?Send)]
pub trait WorkSource {
    /// A pending input as listed or received.
    type Item;
    /// An input this worker has claimed so other workers skip it.
    type Claim: Display;

    async fn list(&self) -> Result<Vec<Self::Item>>;
    async fn claim(&self, item: Self::Item) -> Result<Self::Claim>;
    /// Writes the input into `file` and returns its media type.
    async fn download(&self, claim: &Self::Claim, file: &mut File) -> Result<String>;
    async fn upload(&self, claim: &Self::Claim, signed: &Path, media_type: &str) -> Result<()>;
    /// Releases the claim and deletes the input when it was signed.
    async fn finish(&self, claim: Self::Claim, signed: bool) -> Result<()>;
}

/// Uses the stored content type, or the file extension when the content type
/// is missing or generic (c2pa accepts either).
pub fn media_type(content_type: Option<&str>, name: &str) -> String {
    match content_type {
        Some(value)
            if !value.is_empty()
                && value != "application/octet-stream"
                && value != "binary/octet-stream" =>
        {
            value.to_owned()
        }
        _ => Path::new(name)
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("application/octet-stream")
            .to_owned(),
    }
}

async fn sign_one<S: WorkSource>(
    source: &S,
    claim: &S::Claim,
    builder: &mut Builder,
    signer: &dyn AsyncSigner,
) -> Result<()> {
    let mut input = tempfile::tempfile()?;
    log::info!("Downloading {claim} ...");
    let media_type = source.download(claim, &mut input).await?;
    input.rewind()?;

    let mut output = tempfile::NamedTempFile::new()?;
    builder
        .sign_async(signer, &media_type, &mut input, output.as_file_mut())
        .await?;
    log::info!("Successfully signed {claim}. Uploading ...");
    source.upload(claim, output.path(), &media_type).await
}

async fn process_all<S: WorkSource>(
    source: &S,
    builder: &mut Builder,
    signer: &dyn AsyncSigner,
) -> Result<()> {
    for item in source.list().await? {
        let claim = match source.claim(item).await {
            Ok(claim) => claim,
            Err(err) => {
                log::warn!("Skipping an input that could not be claimed: {err:?}");
                continue;
            }
        };
        log::info!("Processing {claim}");
        let result = sign_one(source, &claim, builder, signer).await;
        let name = claim.to_string();
        match &result {
            Ok(()) => log::info!("{name} processed successfully"),
            Err(err) => log::error!("Error processing {name}: {err:?}"),
        }
        if let Err(err) = source.finish(claim, result.is_ok()).await {
            log::error!("Error completing {name}: {err:?}");
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let cloud = Cloud::select(None)?;
    let manifest_definition =
        ManifestDefinition::try_from(examples_common::manifest_definition_from_env()?)?;
    let signer = examples_common::signer_from_env(cloud).await?;
    let mut builder = Builder::from_context(Context::new()).with_definition(manifest_definition)?;
    let signer = signer.as_ref();

    match cloud {
        #[cfg(feature = "azure")]
        Cloud::Azure => process_all(&azure::BlobSource::from_env()?, &mut builder, signer).await,
        #[cfg(feature = "aws")]
        Cloud::Aws => {
            process_all(&aws::SqsS3Source::from_env().await?, &mut builder, signer).await
        }
        #[cfg(feature = "gcp")]
        Cloud::Gcp => process_all(&gcp::GcsSource::from_env().await?, &mut builder, signer).await,
    }
}

#[cfg(test)]
mod tests {
    use super::media_type;

    #[test]
    fn media_type_falls_back_to_extension() {
        assert_eq!(media_type(Some("image/png"), "a.jpg"), "image/png");
        assert_eq!(media_type(Some("binary/octet-stream"), "dir/a.jpg"), "jpg");
        assert_eq!(media_type(None, "a.mp4"), "mp4");
        assert_eq!(media_type(Some(""), "noext"), "application/octet-stream");
    }
}
