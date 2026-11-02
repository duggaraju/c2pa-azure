//! Google Cloud Storage input.
//!
//! Cloud Storage has no leases, so concurrent workers may sign the same object
//! twice; that is harmless because the output is overwritten and the input is
//! only deleted if its generation is unchanged. Run one job at a time
//! (`parallelism: 1`) to avoid duplicate work.
//!
//! Environment: `INPUT_BUCKET`, `OUTPUT_BUCKET` (bucket names). Credentials
//! come from Application Default Credentials.

use std::{env, fmt, io::Write, path::Path};

use anyhow::{Context as _, Result};
use google_cloud_storage::{
    client::{Storage, StorageControl},
    model::Object,
};

use crate::{WorkSource, media_type};

pub struct GcsSource {
    storage: Storage,
    control: StorageControl,
    input_bucket: String,
    output_bucket: String,
}

fn bucket_resource(variable: &str) -> Result<String> {
    let name = env::var(variable).with_context(|| format!("missing {variable}"))?;
    Ok(format!("projects/_/buckets/{name}"))
}

impl GcsSource {
    pub async fn from_env() -> Result<Self> {
        Ok(Self {
            storage: Storage::builder().build().await?,
            control: StorageControl::builder().build().await?,
            input_bucket: bucket_resource("INPUT_BUCKET")?,
            output_bucket: bucket_resource("OUTPUT_BUCKET")?,
        })
    }
}

pub struct GcsObject {
    name: String,
    generation: i64,
    content_type: String,
    bucket: String,
}

impl fmt::Display for GcsObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bucket = self.bucket.trim_start_matches("projects/_/buckets/");
        write!(f, "gs://{bucket}/{}", self.name)
    }
}

#[async_trait::async_trait(?Send)]
impl WorkSource for GcsSource {
    type Item = Object;
    type Claim = GcsObject;

    async fn list(&self) -> Result<Vec<Object>> {
        let mut objects = Vec::new();
        let mut page_token = String::new();
        loop {
            let response = self
                .control
                .list_objects()
                .set_parent(&self.input_bucket)
                .set_page_token(page_token)
                .send()
                .await?;
            objects.extend(response.objects);
            if response.next_page_token.is_empty() {
                return Ok(objects);
            }
            page_token = response.next_page_token;
        }
    }

    async fn claim(&self, object: Object) -> Result<GcsObject> {
        Ok(GcsObject {
            name: object.name,
            generation: object.generation,
            content_type: object.content_type,
            bucket: self.input_bucket.clone(),
        })
    }

    async fn download(&self, claim: &GcsObject, file: &mut std::fs::File) -> Result<String> {
        let mut response = self
            .storage
            .read_object(&self.input_bucket, &claim.name)
            .set_generation(claim.generation)
            .send()
            .await?;
        while let Some(chunk) = response.next().await.transpose()? {
            file.write_all(&chunk)?;
        }
        Ok(media_type(Some(&claim.content_type), &claim.name))
    }

    async fn upload(&self, claim: &GcsObject, signed: &Path, media_type: &str) -> Result<()> {
        let file = tokio::fs::File::open(signed).await?;
        let mut request = self
            .storage
            .write_object(&self.output_bucket, &claim.name, file);
        if media_type.contains('/') {
            request = request.set_content_type(media_type);
        }
        request.send_unbuffered().await?;
        Ok(())
    }

    async fn finish(&self, claim: GcsObject, signed: bool) -> Result<()> {
        if signed {
            self.control
                .delete_object()
                .set_bucket(&self.input_bucket)
                .set_object(&claim.name)
                .set_if_generation_match(claim.generation)
                .send()
                .await?;
        }
        Ok(())
    }
}
