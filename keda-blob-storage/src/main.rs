use std::{
    env, fs,
    io::{Seek, Write},
    path::Path,
    sync::Arc,
};

use azure_core::{
    auth::TokenCredential, date::duration_from_minutes, tokio::fs::FileStreamBuilder,
};
use azure_identity::DefaultAzureCredentialBuilder;
use azure_storage::prelude::StorageCredentials;
use azure_storage_blobs::{
    container::operations::BlobItem,
    prelude::{BlobClient, ClientBuilder, ContainerClient},
};
use c2pa_acs::{Envconfig, SigningOptions, TrustedSigner};
use futures::StreamExt;

const DEFAULT_MANIFEST: &str = r##"
{
}
"##;

async fn sign_blob(
    input_blob: &BlobClient,
    output_blob: &BlobClient,
    signer: &mut TrustedSigner,
    content_type: &str,
) -> anyhow::Result<()> {
    let mut input = tempfile::tempfile()?;
    let mut stream = input_blob.get().into_stream();
    log::info!("Downloading blob {} ...", input_blob.blob_name());
    while let Some(res) = stream.next().await {
        let mut data = res?.data;
        while let Some(chunk) = data.next().await {
            input.write_all(&chunk?)?;
        }
    }

    input.rewind()?;
    let mut output = tempfile::NamedTempFile::new()?;
    signer
        .sign(input, output.as_file_mut(), content_type)
        .await?;

    output.rewind()?;
    let output_file = tokio::fs::File::open(output.path()).await?;
    const CAP: usize = 1024 * 1024;
    let builder = FileStreamBuilder::new(output_file)
        .buffer_size(CAP)
        .build()
        .await?;

    log::info!(
        "Successfully signed blob {}. Uploading to output container...",
        output_blob.blob_name()
    );
    output_blob.put_block_blob(builder).await?;
    log::info!("Successuflly uploaded blob {}", output_blob.blob_name());
    Ok(())
}

async fn process_blob(
    input_blob: BlobClient,
    output_blob: BlobClient,
    signer: &mut TrustedSigner,
) -> anyhow::Result<()> {
    log::info!("Procesing blob {}", input_blob.blob_name());
    let properties = input_blob.get_properties().await?;

    let lease = input_blob.acquire_lease(duration_from_minutes(1)).await?;
    let lease_client = input_blob.blob_lease_client(lease.lease_id);
    let result = sign_blob(
        &input_blob,
        &output_blob,
        signer,
        &properties.blob.properties.content_type,
    ).await;

    lease_client.release().await?;
    if result.is_ok() {
        input_blob.delete().await?;
    }
    result
}

// Process the first page of blobs.
async fn process_blobs(
    input_container: ContainerClient,
    output_container: ContainerClient,
    signer: &mut TrustedSigner,
) -> anyhow::Result<()> {
    let mut blobs = input_container.list_blobs().into_stream();
    let page = blobs.next().await;
    if let Some(page) = page {
        let page = page?;
        for item in page.blobs.items {
            if let BlobItem::Blob(blob) = item {
                let input_blob = input_container.blob_client(&blob.name);
                let output_blob = output_container.blob_client(&blob.name);
                let result = process_blob(input_blob, output_blob, signer).await;
                if let Err(err) = result {
                    log::error!("Error processing blob: {:?}", err);
                } else {
                    log::info!("Blob {} processed successfully", blob.name);
                }
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    env_logger::init();
    let mut builder = DefaultAzureCredentialBuilder::new();
    if cfg!(debug_assertions) {
        builder.exclude_managed_identity_credential();
    }
    let credential: Arc<dyn TokenCredential> = Arc::new(builder.build()?);
    let manifest_definition = env::var("MANIFEST_DEFINITION").ok();
    let manifest_definition = if let Some(manifest) = manifest_definition {
        let path = Path::new(&manifest);
        if path.exists() {
            fs::read_to_string(path)?
        } else {
            manifest
        }
    } else {
        DEFAULT_MANIFEST.to_owned()
    };

    let account = std::env::var("STORAGE_ACCOUNT").expect("missing STORAGE_ACCOUNT");
    let input_container_name = std::env::var("INPUT_CONTAINER").expect("missing INPUT_CONTAINER");
    let output_container_name =
        std::env::var("OUTPUT_CONTAINER").expect("missing OUTPUT_CONTAINER");
    let storage_credentials = StorageCredentials::token_credential(credential.clone());
    let input_container = ClientBuilder::new(account.clone(), storage_credentials.clone())
        .container_client(&input_container_name);
    let output_container =
        ClientBuilder::new(account, storage_credentials).container_client(&output_container_name);
    let options = SigningOptions::init_from_env()?;
    let mut signer = TrustedSigner::new(credential, options, manifest_definition).await?;
    process_blobs(input_container, output_container, &mut signer).await?;
    Ok(())
}
