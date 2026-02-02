use std::{
    env, fs,
    io::{Read, Seek, Write},
    path::Path,
    sync::Arc,
};

use azure_core::{
    credentials::TokenCredential,
    http::{RequestContent, headers::HeaderName},
};
use azure_identity::{
    AzureCliCredential, ManagedIdentityCredential, ManagedIdentityCredentialOptions, UserAssignedId,
};
use azure_storage_blob::{
    BlobClient, clients::BlobContainerClient, models::BlobClientAcquireLeaseResultHeaders,
};
use c2pa::{AsyncSigner, Builder, Context, ManifestDefinition};
use c2pa_azure::{Envconfig, SigningOptions, TrustedSigner};
use futures::StreamExt;

const DEFAULT_MANIFEST: &str = include_str!("../../../test_data/manifest_definition.json");

async fn sign_blob(
    input_blob: &BlobClient,
    output_blob: &BlobClient,
    builder: &mut Builder,
    signer: &dyn AsyncSigner,
    content_type: &str,
) -> anyhow::Result<()> {
    let mut input = tempfile::tempfile()?;
    log::info!("Downloading blob {} ...", input_blob.url());
    let response = input_blob.download(None).await?;
    let mut stream = response.into_body();
    while let Some(res) = stream.next().await {
        let data = res?;
        input.write_all(&data)?;
    }

    input.rewind()?;
    let mut output = tempfile::NamedTempFile::new()?;
    builder
        .sign_async(signer, content_type, &mut input, output.as_file_mut())
        .await?;

    output.rewind()?;
    let size = output.as_file().metadata()?.len();
    let mut data = Vec::with_capacity(size as usize);
    output.read_to_end(&mut data)?;

    log::info!(
        "Successfully signed blob {}. Uploading to output container...",
        output_blob.url()
    );
    let content = RequestContent::from(data);
    output_blob.upload(content, true, size, None).await?;
    log::info!("Successuflly uploaded blob {}", output_blob.url());
    Ok(())
}

async fn process_blob(
    input_blob: BlobClient,
    output_blob: BlobClient,
    builder: &mut Builder,
    signer: &dyn AsyncSigner,
) -> anyhow::Result<()> {
    log::info!("Procesing blob {}", input_blob.url());
    let properties = input_blob.get_properties(None).await?;
    let content_type = properties
        .headers()
        .get_str(&HeaderName::from_static("Content-Type"))?;

    let lease = input_blob.acquire_lease(60, None).await?;
    let lease_id = lease.lease_id()?.unwrap();
    let result = sign_blob(&input_blob, &output_blob, builder, signer, content_type).await;

    input_blob.release_lease(lease_id, None).await?;
    if result.is_ok() {
        input_blob.delete(None).await?;
    }
    result
}

// Process the first page of blobs.
async fn process_blobs(
    input_container: BlobContainerClient,
    output_container: BlobContainerClient,
    builder: &mut Builder,
    signer: &dyn AsyncSigner,
) -> anyhow::Result<()> {
    let mut blobs = input_container.list_blobs(None)?;
    while let Some(result) = blobs.next().await {
        let blob = result?;
        let name = blob.name.as_ref().unwrap().content.as_ref().unwrap();
        let input_blob = input_container.blob_client(name);
        let output_blob = output_container.blob_client(name);
        let result = process_blob(input_blob, output_blob, builder, signer).await;
        if let Err(err) = result {
            log::error!("Error processing blob: {err:?}");
        } else {
            log::info!("Blob {name} processed successfully");
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    env_logger::init();
    let credential: Arc<dyn TokenCredential> = if cfg!(debug_assertions) {
        AzureCliCredential::new(None)?
    } else {
        let options = ManagedIdentityCredentialOptions {
            user_assigned_id: Some(UserAssignedId::ClientId(
                env::var("AZURE_CLIENT_ID").expect("missing AZURE_CLIENT_ID"),
            )),
            ..Default::default()
        };
        ManagedIdentityCredential::new(Some(options))?
    };

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
    let manifest_definition = ManifestDefinition::try_from(manifest_definition)?;
    let account = std::env::var("STORAGE_ACCOUNT").expect("missing STORAGE_ACCOUNT");
    let input_container_name = std::env::var("INPUT_CONTAINER").expect("missing INPUT_CONTAINER");
    let output_container_name =
        std::env::var("OUTPUT_CONTAINER").expect("missing OUTPUT_CONTAINER");
    let input_container = BlobContainerClient::new(
        &account,
        &input_container_name,
        Some(credential.clone()),
        None,
    )?;
    let output_container = BlobContainerClient::new(
        &account,
        &output_container_name,
        Some(credential.clone()),
        None,
    )?;

    let options = SigningOptions::init_from_env()?;
    let signer = TrustedSigner::new(credential, options).await?;
    let context = Context::new();
    let mut builder = Builder::from_context(context).with_definition(manifest_definition)?;
    process_blobs(input_container, output_container, &mut builder, &signer).await?;
    Ok(())
}
