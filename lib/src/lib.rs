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
//! - [`SigningOptions`]: describe the Trusted Signing account, certificate profile, and optional timestamping authority.
//! - [`Context`](c2pa::Context) + [`Builder`](c2pa::Builder): supply TOML settings and your manifest definition before invoking the signer.
//!
//! ### Minimal example
//!
//! ```no_run
//! use std::{
//!     fs::File,
//!     sync::Arc,
//! };
//!
//! use azure_identity::DefaultAzureCredential;
//! use c2pa::{Builder, Context};
//! use c2pa_azure::{SigningOptions, TrustedSigner};
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
//!     );
//!
//!     let settings = r#"[verify]\nverify_after_sign = true"#;
//!     let manifest_definition = r#"{"label":"example"}"#;
//!
//!     let context = Context::new().with_settings(settings)?;
//!     let mut builder = Builder::from_context(context).with_definition(manifest_definition)?;
//!     let signer = TrustedSigner::new(credential, options).await?;
//!
//!     let mut input = File::open("sample1.png")?;
//!     let mut output = File::create("signed.png")?;
//!
//!     builder
//!         .sign_async(&signer, "image/png", &mut input, &mut output)
//!         .await?;
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
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

pub use c2pa::Error;
pub use envconfig::Envconfig;
pub use sign::{SigningOptions, TrustedSigner};

#[cfg(test)]
mod tests {
    use c2pa::Reader;
    use std::io::Cursor;

    #[tokio::test]
    async fn test_verify_file() {
        let data = include_bytes!("../../test_data/signed.png");
        let stream = Cursor::new(data);
        let result = Reader::from_stream_async("png", stream).await.unwrap();
        assert_eq!(
            &result.json(),
            include_str!("../../test_data/manifest.json")
        );
    }
}
