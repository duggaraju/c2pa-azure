//! # c2pa-acs
//!
//! Rust helpers for issuing C2PA signatures backed by Azure Trusted Signing.
//! The [`TrustedSigner`] type bridges `c2pa-rs` with Azure so you can stamp
//! evidence from CLIs, Azure Functions, or containerized workers while reusing
//! the same manifest definition and verification settings.
//!
//! ## Crate features
//!
//! PDF support (`pdf`) and Rust-native crypto (`rust_native_crypto`) are enabled
//! by default and work on stable Rust; nightly is not required.
//! To disable PDF support, use `--no-default-features --features rust_native_crypto`.
//! To use OpenSSL while retaining PDF support, use
//! `--no-default-features --features pdf,openssl`.
//!
//! ## Using `TrustedSigner`
//!
//! - `TokenCredential`: supply any Azure credential (for example `AzureCliCredential`).
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
//! use azure_identity::AzureCliCredential;
//! use c2pa::{Builder, Context};
//! use c2pa_azure::{SigningOptions, TrustedSigner};
//! use url::Url;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let credential = AzureCliCredential::new(None)?;
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
    use c2pa::{Context, Reader, settings::TrustListKind};
    use serde_json::Value;
    use std::io::Cursor;

    async fn verify_file(context: Context) -> Value {
        let data = include_bytes!("../../../test_data/signed.png");
        let stream = Cursor::new(data);
        let result = Reader::from_context(context)
            .with_stream_async("png", stream)
            .await
            .unwrap();
        serde_json::from_str(&result.json()).unwrap()
    }

    #[tokio::test]
    async fn test_verify_file() {
        let settings = include_str!("../../../test_data/settings.toml");
        let context = Context::new().with_settings(settings).unwrap();
        let actual = verify_file(context).await;
        let expected: Value =
            serde_json::from_str(include_str!("../../../test_data/manifest.json")).unwrap();

        assert_eq!(actual["active_manifest"], expected["active_manifest"]);
        assert_eq!(actual["manifests"], expected["manifests"]);
        assert_eq!(
            actual["validation_state"], "Trusted",
            "Validation results: {}",
            actual["validation_results"]
        );
        assert_eq!(
            actual["validation_results"]["activeManifest"]["failure"],
            serde_json::json!([])
        );
        assert!(
            actual["validation_results"]["activeManifest"]["success"]
                .as_array()
                .unwrap()
                .iter()
                .any(|status| status["code"] == "timeStamp.trusted")
        );
    }

    #[tokio::test]
    async fn test_signer_anchors_do_not_trust_timestamps() {
        let settings = include_str!("../../../test_data/settings.toml");
        let mut context = Context::new().with_settings(settings).unwrap();
        for anchor in context.settings_mut().trust.anchors.as_mut().unwrap() {
            anchor.trust_kind = TrustListKind::Manifest;
        }

        let actual = verify_file(context).await;
        assert_eq!(actual["validation_state"], "Invalid");
        assert!(
            actual["validation_results"]["activeManifest"]["informational"]
                .as_array()
                .unwrap()
                .iter()
                .any(|status| status["code"] == "timeStamp.untrusted")
        );
    }
}
