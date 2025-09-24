use azure_core::credentials::TokenCredential;
use azure_identity::{AzureCliCredential, ManagedIdentityCredential};
use c2pa_acs::{Envconfig, SigningOptions, TrustedSigner};
use futures::StreamExt;
use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::net::Ipv4Addr;
use std::path::Path;
use std::sync::Arc;
use std::{env, io::Seek};
use tempfile::NamedTempFile;
use warp::{Buf, Error, Filter, Rejection, Reply, Stream, reject::Reject};

#[allow(dead_code)]
#[derive(Debug)]
enum ApiError {
    Azure(azure_core::Error),
    Io(std::io::Error),
    C2pa(c2pa_acs::Error),
    Warp(Error),
}

impl Reject for ApiError {}

async fn copy_to_file(
    mut file: &File,
    mut stream: impl Stream<Item = Result<impl Buf, warp::Error>> + Unpin + Send + Sync,
) -> Result<(), ApiError> {
    while let Some(value) = stream.next().await {
        match value {
            Ok(mut buf) => {
                while buf.has_remaining() {
                    let chunk = buf.chunk();
                    file.write_all(chunk).unwrap();
                    buf.advance(chunk.len());
                }
            }
            Err(e) => {
                log::error!("Error copying the body to file: {e:?}");
                return Err(ApiError::Warp(e));
            }
        }
    }
    file.rewind().map_err(ApiError::Io)?;
    Ok(())
}

async fn sign_file(
    mut signer: TrustedSigner,
    content_type: String,
    stream: impl Stream<Item = Result<impl Buf, warp::Error>> + Unpin + Send + Sync,
) -> Result<impl Reply, Rejection> {
    let mut file = NamedTempFile::new().map_err(|x| warp::reject::custom(ApiError::Io(x)))?;
    copy_to_file(file.as_file_mut(), stream)
        .await
        .map_err(warp::reject::custom)?;

    let mut output = Cursor::new(Vec::new());
    signer
        .sign(&mut file.as_file_mut(), &mut output, &content_type)
        .await
        .map_err(|x| warp::reject::custom(ApiError::Azure(x)))?;
    log::info!("Successfully signed the file.");
    Ok(warp::reply::with_header(
        output.into_inner(),
        "content-type",
        content_type,
    ))
}

async fn verify_file(
    content_type: String,
    stream: impl Stream<Item = Result<impl Buf, warp::Error>> + Unpin + Send + Sync,
) -> Result<impl Reply, Rejection> {
    let mut file = NamedTempFile::new().map_err(|x| warp::reject::custom(ApiError::Io(x)))?;
    copy_to_file(file.as_file_mut(), stream)
        .await
        .map_err(warp::reject::custom)?;

    let manifest = c2pa_acs::verify_file(&content_type, file)
        .await
        .map_err(|x| warp::reject::custom(ApiError::C2pa(x)))?;
    Ok(warp::reply::with_header(
        manifest,
        "content-type",
        "application/json",
    ))
}

const DEFAULT_MANIFEST: &str = include_str!("../../manifest.json");

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    env_logger::init();
    for (key, value) in std::env::vars() {
        log::info!("{key}: {value}");
    }
    let credentials: Arc<dyn TokenCredential> = if cfg!(debug_assertions) {
        AzureCliCredential::new(None)?
    } else {
        ManagedIdentityCredential::new(None)?
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

    let content_type = warp::header::<String>("content-type");

    let verify = warp::path("verify")
        .and(warp::path::end())
        .and(content_type)
        .and(warp::filters::body::stream())
        .and_then(verify_file);

    let options = SigningOptions::init_from_env()?;
    let signer = TrustedSigner::new(credentials, options, manifest_definition).await?;
    let sign = warp::path("sign")
        .and(warp::path::end())
        .map(move || signer.clone())
        .and(content_type)
        .and(warp::filters::body::stream())
        .and_then(sign_file);

    let routes = warp::post().and(warp::path("api")).and(verify.or(sign));
    let port_key = "FUNCTIONS_CUSTOMHANDLER_PORT";
    let port: u16 = match env::var(port_key) {
        Ok(val) => val.parse().expect("Custom Handler port is not a number!"),
        Err(_) => 3000,
    };

    warp::serve(routes).run((Ipv4Addr::UNSPECIFIED, port)).await;
    Ok(())
}
