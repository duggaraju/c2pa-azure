//! # c2pa-acs
//!
//! Rust helpers for issuing C2PA signatures backed by Azure Trusted Signing.
//! The [`TrustedSigner`] type bridges `c2pa-rs` with Azure so you can stamp
//! evidence from CLIs, Azure Functions, or containerized workers while reusing
//! the same manifest definition and verification settings.
//!
//! ## Using `TrustedSigner`
//!
//! - `TokenCredential`: supply any Azure credential (for example `DefaultAzureCredential`).
//! - [`SigningOptions`]: describe the Trusted Signing account, certificate profile, and optional TOML settings.
//! - Manifest definition: JSON string consumed by `c2pa::Builder::from_json`.
//!
//! ### Minimal example
//!
//! ```no_run
//! use std::{
//!     fs::{File, OpenOptions},
//!     io::BufReader,
//!     sync::Arc,
//! };
//!
//! use azure_identity::DefaultAzureCredential;
//! use c2pa_acs::{SigningOptions, TrustedSigner};
//! use url::Url;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let credential = Arc::new(DefaultAzureCredential::default());
//!
//!     let options = SigningOptions::new(
//!         Url::parse("https://eus.codesigning.azure.net")?,
//!         "signing_account".to_string(),
//!         "certificate_profile".to_string(),
//!         None,
//!         None,
//!     );
//!
//!     let manifest_definition = r#"{"label":"example"}"#.to_string();
//!     let mut signer = TrustedSigner::new(credential, options, manifest_definition).await?;
//!
//!     let mut input = BufReader::new(File::open("sample1.png")?);
//!     let mut output = OpenOptions::new()
//!         .read(true)
//!         .write(true)
//!         .create(true)
//!         .truncate(true)
//!         .open("signed.png")?;
//!
//!     signer.sign(&mut input, &mut output, "image/png").await?;
//!     Ok(())
//! }
//! ```
//!
//! ### Environment variables
//!
//! - `SIGNING_ENDPOINT`: Trusted Signing endpoint (for example `https://eus.codesigning.azure.net`).
//! - `SIGNING_ACCOUNT`: Trusted Signing account name.
//! - `CERTIFICATE_PROFILE`: certificate profile used for signing.
//! - `ALGORITHM` *(optional)*: override the default signature algorithm (`ps384`).
//! - `TIME_AUTHORITY_URL` *(optional)*: RFC3161 timestamp authority.
//! - `SETTINGS` *(optional)*: TOML string, e.g. contents of `cli/src/settings.toml`, to align signing and verification policies.
//!
//! ### Verifying a signed file
//!
//! The library exposes a thin wrapper around [`c2pa::Reader`] so you can inspect
//! manifests produced by `TrustedSigner` or any other C2PA producer.
//!
//! ```no_run
//! use std::fs::File;
//!
//! use c2pa::Reader;
//! use c2pa_acs::verify_file;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let signed = File::open("signed.png")?;
//!
//!     // Option 1: leverage the helper to get the manifest JSON as a string.
//!     let manifest_json = verify_file("png", signed).await?;
//!     println!("{manifest_json}");
//!
//!     // Option 2: use the underlying reader for fine-grained inspection.
//!     let signed = File::open("signed.png")?;
//!     let reader = Reader::from_stream_async("png", signed).await?;
//!     println!("Manifest Store: {}", reader.json());
//!     Ok(())
//! }
//! ```
//!
mod acs;
mod auth;
mod p7b;
mod sign;

use std::io::{Read, Seek};

pub use c2pa::Error;
use c2pa::Reader;
pub use envconfig::Envconfig;
pub use sign::{SigningOptions, TrustedSigner};

pub async fn verify_file(
    format: &str,
    stream: impl Read + Seek + Send,
) -> Result<String, c2pa::Error> {
    let reader = Reader::from_stream_async(format, stream).await?;
    Ok(reader.json())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[tokio::test]
    async fn test_verify_file() {
        let data = include_bytes!("../../test_data/signed.png");
        let stream = Cursor::new(data);
        let result = verify_file("png", stream).await.unwrap();
        assert_eq!(&result, include_str!("../../test_data/manifest.json"));
    }
}
