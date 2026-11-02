//! Azure Trusted Signing (Azure Artifact Signing) signer.
//!
//! This module re-exports the [`c2pa-azure`](https://docs.rs/c2pa-azure) crate
//! so Azure users get the same implementation whether they depend on
//! `c2pa-azure` directly or on `c2pa-cloud` with the `azure` feature.
//!
//! Environment variables read by [`SigningOptions::init_from_env`](crate::Envconfig::init_from_env):
//!
//! - `SIGNING_ENDPOINT`: Trusted Signing endpoint (for example `https://eus.codesigning.azure.net`).
//! - `SIGNING_ACCOUNT`: Trusted Signing account name.
//! - `CERTIFICATE_PROFILE`: certificate profile used for signing.
//! - `ALGORITHM` *(optional)*: signature algorithm (default `ps384`).
//! - `TIME_AUTHORITY_URL` *(optional)*: RFC 3161 timestamp authority.

pub use c2pa_azure::{SigningOptions, TrustedSigner};
