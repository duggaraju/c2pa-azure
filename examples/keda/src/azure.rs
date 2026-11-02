//! Azure Blob Storage input, claimed with a blob lease.
//!
//! Environment: `STORAGE_ACCOUNT`, `INPUT_CONTAINER`, `OUTPUT_CONTAINER`, and
//! optionally `AZURE_CLIENT_ID` for a user-assigned managed identity.

use std::{
    env, fmt,
    io::Write,
    path::Path,
    pin::Pin,
    sync::Arc,
    task::{Context as TaskContext, Poll},
};

use anyhow::{Context as _, Result};
use azure_core::{
    error::ErrorKind,
    http::{Body, RequestContent, headers::HeaderName},
    stream::SeekableStream,
};
use azure_storage_blob::{
    BlobClient, clients::BlobContainerClient, models::BlobClientAcquireLeaseResultHeaders,
};
use futures::{StreamExt, io::AsyncRead};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt},
    sync::Mutex,
};

use crate::{WorkSource, media_type};

const LEASE_SECONDS: i32 = 60;

#[derive(Debug, Clone)]
struct SeekableFileStream {
    handle: Arc<Mutex<File>>,
    len: u64,
    buffer_size: usize,
}

impl SeekableFileStream {
    async fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).await?;
        let len = file.metadata().await?.len();
        Ok(Self {
            handle: Arc::new(Mutex::new(file)),
            len,
            buffer_size: azure_core::stream::DEFAULT_BUFFER_SIZE,
        })
    }

    async fn read(&self, slice: &mut [u8]) -> std::io::Result<usize> {
        let mut handle = self.handle.lock().await;
        handle.read(slice).await
    }
}

#[async_trait::async_trait]
impl SeekableStream for SeekableFileStream {
    async fn reset(&mut self) -> azure_core::Result<()> {
        let mut handle = self.handle.lock().await;
        handle
            .seek(std::io::SeekFrom::Start(0))
            .await
            .map_err(|err| {
                azure_core::Error::with_error(ErrorKind::Io, err, "failed to reset file stream")
            })?;
        Ok(())
    }

    fn len(&self) -> Option<u64> {
        Some(self.len)
    }

    fn buffer_size(&self) -> usize {
        self.buffer_size
    }
}

impl AsyncRead for SeekableFileStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        slice: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        let mut fut = std::pin::pin!(this.read(slice));
        fut.as_mut().poll(cx)
    }
}

pub struct BlobSource {
    input: BlobContainerClient,
    output: BlobContainerClient,
}

impl BlobSource {
    pub fn from_env() -> Result<Self> {
        let credential = examples_common::azure_credential()?;
        let account = env::var("STORAGE_ACCOUNT").context("missing STORAGE_ACCOUNT")?;
        let container_client = |variable: &str| -> Result<BlobContainerClient> {
            let name = env::var(variable).with_context(|| format!("missing {variable}"))?;
            let url = format!("https://{account}.blob.core.windows.net/{name}").parse()?;
            Ok(BlobContainerClient::new(
                url,
                Some(credential.clone()),
                None,
            )?)
        };
        Ok(Self {
            input: container_client("INPUT_CONTAINER")?,
            output: container_client("OUTPUT_CONTAINER")?,
        })
    }
}

pub struct LeasedBlob {
    name: String,
    input: BlobClient,
    output: BlobClient,
    lease_id: String,
    content_type: Option<String>,
}

impl fmt::Display for LeasedBlob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.input.url())
    }
}

#[async_trait::async_trait(?Send)]
impl WorkSource for BlobSource {
    type Item = String;
    type Claim = LeasedBlob;

    async fn list(&self) -> Result<Vec<String>> {
        let mut names = Vec::new();
        let mut blobs = self.input.list_blobs(None)?;
        while let Some(blob) = blobs.next().await {
            names.extend(blob?.name);
        }
        Ok(names)
    }

    async fn claim(&self, name: String) -> Result<LeasedBlob> {
        let input = self.input.blob_client(&name);
        let output = self.output.blob_client(&name);
        let properties = input.get_properties(None).await?;
        let content_type = properties
            .headers()
            .get_optional_str(&HeaderName::from_static("content-type"))
            .map(str::to_owned);
        let lease = input.acquire_lease(LEASE_SECONDS, None).await?;
        let lease_id = lease.lease_id()?.context("lease response without a lease id")?;
        Ok(LeasedBlob {
            name,
            input,
            output,
            lease_id,
            content_type,
        })
    }

    async fn download(&self, claim: &LeasedBlob, file: &mut std::fs::File) -> Result<String> {
        let response = claim.input.download(None).await?;
        let mut stream = response.body;
        while let Some(data) = stream.next().await {
            file.write_all(&data?)?;
        }
        Ok(media_type(claim.content_type.as_deref(), &claim.name))
    }

    async fn upload(&self, claim: &LeasedBlob, signed: &Path, _media_type: &str) -> Result<()> {
        let stream = SeekableFileStream::open(signed).await?;
        let content: RequestContent<azure_core::Bytes, azure_core::http::NoFormat> =
            Body::SeekableStream(Box::new(stream)).into();
        claim.output.upload(content, None).await?;
        log::info!("Successfully uploaded blob {}", claim.output.url());
        Ok(())
    }

    async fn finish(&self, claim: LeasedBlob, signed: bool) -> Result<()> {
        claim.input.release_lease(claim.lease_id, None).await?;
        if signed {
            claim.input.delete(None).await?;
        }
        Ok(())
    }
}
