//! # c2pa-cloud
//!
//! Rust helpers for issuing [C2PA](https://c2pa.org) signatures with cloud
//! signing services. Each cloud lives behind a feature flag and exposes a
//! signer that implements [`c2pa::AsyncSigner`], so the rest of your
//! `c2pa-rs` code stays the same whichever cloud you sign with.
//!
//! | Feature | Module | Service | Certificate chain |
//! |---------|--------|---------|-------------------|
//! | `azure` | [`azure`] | Azure Trusted Signing (Artifact Signing) | Issued and returned by the service |
//! | `aws`   | [`aws`]   | AWS KMS asymmetric keys | Supplied by you (PEM) |
//! | `gcp`   | [`gcp`]   | Google Cloud KMS / Cloud HSM | Supplied by you (PEM) |
//!
//! No cloud is enabled by default; turn on the ones you need:
//!
//! ```toml
//! c2pa-cloud = { version = "0.91", features = ["aws"] }
//! ```
//!
//! AWS KMS and Cloud KMS only hold the private key. Have a C2PA-conformant
//! certificate authority issue a certificate for the KMS public key and pass
//! the chain to the signer; [`load_certificate_chain`] accepts inline PEM or a
//! file path and orders it the way C2PA expects.
//!
//! ## Other crate features
//!
//! PDF support (`pdf`) and Rust-native crypto (`rust_native_crypto`) are
//! enabled by default. Use `--no-default-features --features pdf,openssl` to
//! use OpenSSL instead.
//!
//! ## Verifying a signed file
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

mod certs;
#[cfg(any(feature = "aws", feature = "gcp"))]
mod digest;

#[cfg(feature = "aws")]
pub mod aws;
#[cfg(feature = "azure")]
pub mod azure;
#[cfg(feature = "gcp")]
pub mod gcp;

pub use c2pa::Error;
pub use certs::{certificate_chain_from_pem, load_certificate_chain};
pub use envconfig::Envconfig;

/// RFC 3161 timestamp authority used by the KMS signers unless overridden.
pub const DEFAULT_TIME_AUTHORITY_URL: &str = "http://timestamp.digicert.com";
