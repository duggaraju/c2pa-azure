//! AWS KMS signer.
//!
//! [`KmsSigner`] signs C2PA claims with an asymmetric AWS KMS key. AWS KMS only
//! stores the private key, so you also provide the certificate chain your C2PA
//! certificate authority issued for that key.
//!
//! | `ALGORITHM` | KMS key spec | KMS signing algorithm |
//! |-------------|--------------|-----------------------|
//! | `ps256` / `ps384` / `ps512` | `RSA_2048`, `RSA_3072`, `RSA_4096` | `RSASSA_PSS_SHA_256` / `_384` / `_512` |
//! | `es256` | `ECC_NIST_P256` | `ECDSA_SHA_256` |
//! | `es384` | `ECC_NIST_P384` | `ECDSA_SHA_384` |
//! | `es512` | `ECC_NIST_P521` | `ECDSA_SHA_512` |
//!
//! Environment variables read by [`KmsSigningOptions::init_from_env`](crate::Envconfig::init_from_env):
//!
//! - `KMS_KEY_ID`: key ID, key ARN, alias name, or alias ARN.
//! - `CERTIFICATE_CHAIN`: PEM certificate chain, inline or as a file path.
//! - `ALGORITHM` *(optional)*: signature algorithm (default `ps384`).
//! - `TIME_AUTHORITY_URL` *(optional)*: RFC 3161 timestamp authority.
//!
//! Credentials and region are resolved by the standard AWS SDK provider chain
//! (environment, shared config, IAM roles for service accounts, instance profile).
//! The caller needs the `kms:Sign` permission on the key.
//!
//! ```no_run
//! use std::fs::File;
//!
//! use c2pa::{Builder, Context};
//! use c2pa_cloud::aws::{KmsSigner, KmsSigningOptions};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let options = KmsSigningOptions::new("alias/c2pa-signing", "chain.pem");
//!     let signer = KmsSigner::from_default_config(options).await?;
//!
//!     let mut builder = Builder::from_context(Context::new())
//!         .with_definition(r#"{"title":"example"}"#)?;
//!     let mut input = File::open("sample1.png")?;
//!     let mut output = File::create("signed.png")?;
//!     builder
//!         .sign_async(&signer, "image/png", &mut input, &mut output)
//!         .await?;
//!     Ok(())
//! }
//! ```

use async_trait::async_trait;
use aws_sdk_kms::{
    Client,
    error::DisplayErrorContext,
    primitives::Blob,
    types::{MessageType, SigningAlgorithmSpec},
};
use c2pa::{AsyncSigner, SigningAlg};
use envconfig::Envconfig;
use url::Url;

pub use aws_config;
pub use aws_sdk_kms;

use crate::{DEFAULT_TIME_AUTHORITY_URL, certs, digest};

/// Settings for [`KmsSigner`].
#[derive(Clone, Debug, Envconfig)]
pub struct KmsSigningOptions {
    #[envconfig(from = "KMS_KEY_ID")]
    key_id: String,
    #[envconfig(from = "CERTIFICATE_CHAIN")]
    certificate_chain: String,
    #[envconfig(from = "ALGORITHM", default = "ps384")]
    algorithm: SigningAlg,
    #[envconfig(from = "TIME_AUTHORITY_URL")]
    time_authority_url: Option<Url>,
}

impl KmsSigningOptions {
    /// Creates options for the given KMS key and PEM certificate chain
    /// (inline PEM or a file path), using `ps384` and the default timestamp authority.
    pub fn new(key_id: impl Into<String>, certificate_chain: impl Into<String>) -> Self {
        Self {
            key_id: key_id.into(),
            certificate_chain: certificate_chain.into(),
            algorithm: SigningAlg::Ps384,
            time_authority_url: Url::parse(DEFAULT_TIME_AUTHORITY_URL).ok(),
        }
    }

    /// Sets the signature algorithm; it must match the KMS key spec.
    pub fn with_algorithm(mut self, algorithm: SigningAlg) -> Self {
        self.algorithm = algorithm;
        self
    }

    /// Sets the RFC 3161 timestamp authority, or disables timestamping with `None`.
    pub fn with_time_authority_url(mut self, time_authority_url: Option<Url>) -> Self {
        self.time_authority_url = time_authority_url;
        self
    }
}

/// A [`c2pa::AsyncSigner`] backed by an asymmetric AWS KMS key.
#[derive(Clone, Debug)]
pub struct KmsSigner {
    client: Client,
    options: KmsSigningOptions,
    signing_algorithm: SigningAlgorithmSpec,
    certificates: Vec<Vec<u8>>,
}

impl KmsSigner {
    /// Creates a signer from an existing KMS client.
    pub fn new(client: Client, options: KmsSigningOptions) -> c2pa::Result<Self> {
        let signing_algorithm = signing_algorithm(options.algorithm)?;
        let certificates = certs::load_certificate_chain(&options.certificate_chain)?;
        Ok(Self {
            client,
            options,
            signing_algorithm,
            certificates,
        })
    }

    /// Creates a signer using the default AWS credential and region provider chain.
    pub async fn from_default_config(options: KmsSigningOptions) -> c2pa::Result<Self> {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        Self::new(Client::new(&config), options)
    }
}

fn signing_algorithm(algorithm: SigningAlg) -> c2pa::Result<SigningAlgorithmSpec> {
    match algorithm {
        SigningAlg::Ps256 => Ok(SigningAlgorithmSpec::RsassaPssSha256),
        SigningAlg::Ps384 => Ok(SigningAlgorithmSpec::RsassaPssSha384),
        SigningAlg::Ps512 => Ok(SigningAlgorithmSpec::RsassaPssSha512),
        SigningAlg::Es256 => Ok(SigningAlgorithmSpec::EcdsaSha256),
        SigningAlg::Es384 => Ok(SigningAlgorithmSpec::EcdsaSha384),
        SigningAlg::Es512 => Ok(SigningAlgorithmSpec::EcdsaSha512),
        _ => Err(c2pa::Error::CoseSignatureAlgorithmNotSupported),
    }
}

#[async_trait]
impl AsyncSigner for KmsSigner {
    async fn sign(&self, data: Vec<u8>) -> c2pa::Result<Vec<u8>> {
        let digest = digest::digest(self.options.algorithm, &data)?;
        let output = self
            .client
            .sign()
            .key_id(&self.options.key_id)
            .message(Blob::new(digest))
            .message_type(MessageType::Digest)
            .signing_algorithm(self.signing_algorithm.clone())
            .send()
            .await
            .map_err(|err| {
                log::error!("AWS KMS sign failed: {}", DisplayErrorContext(&err));
                c2pa::Error::CoseSignature
            })?;
        // ECDSA signatures are DER encoded; c2pa converts them to the COSE format.
        output
            .signature()
            .map(|signature| signature.as_ref().to_vec())
            .ok_or(c2pa::Error::CoseSignature)
    }

    fn alg(&self) -> SigningAlg {
        self.options.algorithm
    }

    fn certs(&self) -> c2pa::Result<Vec<Vec<u8>>> {
        Ok(self.certificates.clone())
    }

    fn reserve_size(&self) -> usize {
        20000
    }

    fn time_authority_url(&self) -> Option<String> {
        self.options
            .time_authority_url
            .as_ref()
            .map(|url| url.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAIN: &str = include_str!("../../test_data/certs/unordered_chain.pem");

    fn offline_client() -> Client {
        let config = aws_sdk_kms::Config::builder()
            .behavior_version(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new("us-east-1"))
            .build();
        Client::from_conf(config)
    }

    #[test]
    fn maps_c2pa_algorithms_to_kms() {
        assert_eq!(
            signing_algorithm(SigningAlg::Ps384).unwrap(),
            SigningAlgorithmSpec::RsassaPssSha384
        );
        assert_eq!(
            signing_algorithm(SigningAlg::Es256).unwrap(),
            SigningAlgorithmSpec::EcdsaSha256
        );
        assert!(signing_algorithm(SigningAlg::Ed25519).is_err());
    }

    #[test]
    fn signer_exposes_chain_and_options() {
        let options = KmsSigningOptions::new("alias/test", CHAIN)
            .with_algorithm(SigningAlg::Es256)
            .with_time_authority_url(None);
        let signer = KmsSigner::new(offline_client(), options).unwrap();
        assert_eq!(signer.alg(), SigningAlg::Es256);
        assert_eq!(signer.certs().unwrap().len(), 2);
        assert_eq!(signer.time_authority_url(), None);
    }

    #[test]
    fn rejects_unsupported_algorithm() {
        let options =
            KmsSigningOptions::new("alias/test", CHAIN).with_algorithm(SigningAlg::Ed25519);
        assert!(KmsSigner::new(offline_client(), options).is_err());
    }
}
