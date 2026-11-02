//! Cloud-agnostic HTTP signing service.
//!
//! `POST /api/sign` returns the request body with content credentials added;
//! `POST /api/verify` returns the manifest store of the request body as JSON;
//! `GET /healthz` is a readiness probe.
//!
//! The same binary runs as an Azure Functions custom handler, on AWS Lambda
//! behind the Lambda Web Adapter, or on Google Cloud Run. The signer is chosen
//! with the `azure`/`aws`/`gcp` features and the `CLOUD` environment variable.

use c2pa::{Builder, Context, Reader};
use examples_common::Cloud;
use futures::StreamExt;
use std::env;
use std::fs::File;
use std::io::{Cursor, Seek, Write};
use std::net::Ipv4Addr;
use std::sync::Arc;
use tempfile::NamedTempFile;
use warp::{Buf, Error, Filter, Rejection, Reply, Stream, reject::Reject};

#[allow(dead_code)]
#[derive(Debug)]
enum ApiError {
    Io(std::io::Error),
    C2pa(c2pa::Error),
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
                    file.write_all(chunk).map_err(ApiError::Io)?;
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
    context: Arc<Context>,
    manifest_definition: Arc<String>,
    content_type: String,
    stream: impl Stream<Item = Result<impl Buf, warp::Error>> + Unpin + Send + Sync,
) -> Result<impl Reply, Rejection> {
    let mut file = NamedTempFile::new().map_err(|x| warp::reject::custom(ApiError::Io(x)))?;
    copy_to_file(file.as_file_mut(), stream)
        .await
        .map_err(warp::reject::custom)?;

    let mut output = Cursor::new(Vec::new());
    let mut builder = Builder::from_shared_context(&context)
        .with_definition(&*manifest_definition)
        .map_err(|x| warp::reject::custom(ApiError::C2pa(x)))?;
    let signer = context
        .async_signer()
        .map_err(|x| warp::reject::custom(ApiError::C2pa(x)))?;
    builder
        .sign_async(signer, &content_type, &mut file.as_file_mut(), &mut output)
        .await
        .map_err(|x| warp::reject::custom(ApiError::C2pa(x)))?;
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

    let reader = Reader::from_context(Context::new())
        .with_stream_async(&content_type, file.as_file_mut())
        .await
        .map_err(|x| warp::reject::custom(ApiError::C2pa(x)))?;
    let manifest = reader.json();
    Ok(warp::reply::with_header(
        manifest,
        "content-type",
        "application/json",
    ))
}

/// Azure Functions passes the port in `FUNCTIONS_CUSTOMHANDLER_PORT`; Cloud
/// Run and the Lambda Web Adapter use `PORT`.
fn listen_port() -> anyhow::Result<u16> {
    for key in ["FUNCTIONS_CUSTOMHANDLER_PORT", "PORT"] {
        if let Ok(value) = env::var(key) {
            return value
                .parse()
                .map_err(|err| anyhow::anyhow!("{key}={value} is not a valid port: {err}"));
        }
    }
    Ok(3000)
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    env_logger::init();
    // Only names: values can hold credentials (Lambda sets AWS_SECRET_ACCESS_KEY).
    for (key, _) in env::vars() {
        log::debug!("env: {key}");
    }

    let cloud = Cloud::select(None)?;
    let signer = examples_common::signer_from_env(cloud).await?;
    let manifest_definition = Arc::new(examples_common::manifest_definition_from_env()?);
    let context = Context::new().with_async_signer(signer).into_shared();

    let content_type = warp::header::<String>("content-type");

    let verify = warp::path("verify")
        .and(warp::path::end())
        .and(content_type)
        .and(warp::filters::body::stream())
        .and_then(verify_file);

    let sign = warp::path("sign")
        .and(warp::path::end())
        .and(warp::any().map(move || context.clone()))
        .and(warp::any().map(move || manifest_definition.clone()))
        .and(content_type)
        .and(warp::filters::body::stream())
        .and_then(sign_file);

    let health = warp::get()
        .and(warp::path("healthz"))
        .and(warp::path::end())
        .map(|| "ok");

    let routes = warp::post()
        .and(warp::path("api"))
        .and(verify.or(sign))
        .or(health);

    let port = listen_port()?;
    log::info!("Listening on port {port} with the {cloud} signer");
    warp::serve(routes).run((Ipv4Addr::UNSPECIFIED, port)).await;
    Ok(())
}
