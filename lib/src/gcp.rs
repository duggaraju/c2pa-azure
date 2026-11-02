//! Google Cloud KMS signer.
//!
//! [`KmsSigner`] signs C2PA claims with an asymmetric Cloud KMS (or Cloud HSM)
//! key version. Cloud KMS only stores the private key, so you also provide the
//! certificate chain your C2PA certificate authority issued for that key.
//!
//! The signature algorithm is read from the key version, so only these
//! C2PA-compatible algorithms are accepted:
//!
//! | Cloud KMS algorithm | C2PA algorithm |
//! |---------------------|----------------|
//! | `RSA_SIGN_PSS_2048_SHA256`, `RSA_SIGN_PSS_3072_SHA256`, `RSA_SIGN_PSS_4096_SHA256` | `ps256` |
//! | `RSA_SIGN_PSS_4096_SHA512` | `ps512` |
//! | `EC_SIGN_P256_SHA256` | `es256` |
//! | `EC_SIGN_P384_SHA384` | `es384` |
//!
//! Environment variables read by [`KmsSigningOptions::init_from_env`](crate::Envconfig::init_from_env):
//!
//! - `KMS_KEY_NAME`: key version resource name,
//!   `projects/{project}/locations/{location}/keyRings/{ring}/cryptoKeys/{key}/cryptoKeyVersions/{version}`.
//! - `CERTIFICATE_CHAIN`: PEM certificate chain, inline or as a file path.
//! - `TIME_AUTHORITY_URL` *(optional)*: RFC 3161 timestamp authority.
//!
//! Credentials are resolved with Application Default Credentials. The caller
//! needs `cloudkms.cryptoKeyVersions.useToSign` and
//! `cloudkms.cryptoKeyVersions.viewPublicKey` on the key version
//! (for example the `roles/cloudkms.signerVerifier` role).
//!
//! ```no_run
//! use std::fs::File;
//!
//! use c2pa::{Builder, Context};
//! use c2pa_cloud::gcp::{KmsSigner, KmsSigningOptions};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let options = KmsSigningOptions::new(
//!         "projects/my-project/locations/global/keyRings/c2pa/cryptoKeys/signer/cryptoKeyVersions/1",
//!         "chain.pem",
//!     );
//!     let signer = KmsSigner::from_default_client(options).await?;
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
use c2pa::{AsyncSigner, SigningAlg};
use envconfig::Envconfig;
use google_cloud_kms_v1::{
    client::KeyManagementService,
    model::{Digest, crypto_key_version::CryptoKeyVersionAlgorithm},
};
use url::Url;

pub use google_cloud_kms_v1;

use crate::{DEFAULT_TIME_AUTHORITY_URL, certs, digest};

/// Settings for [`KmsSigner`].
#[derive(Clone, Debug, Envconfig)]
pub struct KmsSigningOptions {
    #[envconfig(from = "KMS_KEY_NAME")]
    key_name: String,
    #[envconfig(from = "CERTIFICATE_CHAIN")]
    certificate_chain: String,
    #[envconfig(from = "TIME_AUTHORITY_URL")]
    time_authority_url: Option<Url>,
}

impl KmsSigningOptions {
    /// Creates options for the given key version resource name and PEM
    /// certificate chain (inline PEM or a file path), using the default
    /// timestamp authority.
    pub fn new(key_name: impl Into<String>, certificate_chain: impl Into<String>) -> Self {
        Self {
            key_name: key_name.into(),
            certificate_chain: certificate_chain.into(),
            time_authority_url: Url::parse(DEFAULT_TIME_AUTHORITY_URL).ok(),
        }
    }

    /// Sets the RFC 3161 timestamp authority, or disables timestamping with `None`.
    pub fn with_time_authority_url(mut self, time_authority_url: Option<Url>) -> Self {
        self.time_authority_url = time_authority_url;
        self
    }
}

/// A [`c2pa::AsyncSigner`] backed by an asymmetric Google Cloud KMS key version.
#[derive(Clone, Debug)]
pub struct KmsSigner {
    client: KeyManagementService,
    options: KmsSigningOptions,
    algorithm: SigningAlg,
    certificates: Vec<Vec<u8>>,
}

impl KmsSigner {
    /// Creates a signer from an existing Cloud KMS client. The key version's
    /// algorithm is fetched once to select the matching C2PA algorithm.
    pub async fn new(
        client: KeyManagementService,
        options: KmsSigningOptions,
    ) -> c2pa::Result<Self> {
        let certificates = certs::load_certificate_chain(&options.certificate_chain)?;
        let public_key = client
            .get_public_key()
            .set_name(&options.key_name)
            .send()
            .await
            .map_err(|err| other_error("Cloud KMS GetPublicKey failed", err))?;
        let algorithm = signing_algorithm(&public_key.algorithm)?;
        Ok(Self {
            client,
            options,
            algorithm,
            certificates,
        })
    }

    /// Creates a signer using a Cloud KMS client with Application Default Credentials.
    pub async fn from_default_client(options: KmsSigningOptions) -> c2pa::Result<Self> {
        let client = KeyManagementService::builder()
            .build()
            .await
            .map_err(|err| other_error("Failed to create the Cloud KMS client", err))?;
        Self::new(client, options).await
    }
}

fn other_error(context: &str, err: impl std::fmt::Display) -> c2pa::Error {
    c2pa::Error::OtherError(format!("{context}: {err}").into())
}

fn signing_algorithm(algorithm: &CryptoKeyVersionAlgorithm) -> c2pa::Result<SigningAlg> {
    match algorithm {
        CryptoKeyVersionAlgorithm::RsaSignPss2048Sha256
        | CryptoKeyVersionAlgorithm::RsaSignPss3072Sha256
        | CryptoKeyVersionAlgorithm::RsaSignPss4096Sha256 => Ok(SigningAlg::Ps256),
        CryptoKeyVersionAlgorithm::RsaSignPss4096Sha512 => Ok(SigningAlg::Ps512),
        CryptoKeyVersionAlgorithm::EcSignP256Sha256 => Ok(SigningAlg::Es256),
        CryptoKeyVersionAlgorithm::EcSignP384Sha384 => Ok(SigningAlg::Es384),
        other => {
            log::error!("Cloud KMS algorithm {other:?} is not supported by C2PA");
            Err(c2pa::Error::CoseSignatureAlgorithmNotSupported)
        }
    }
}

fn kms_digest(algorithm: SigningAlg, data: &[u8]) -> c2pa::Result<Digest> {
    let value = digest::digest(algorithm, data)?;
    match algorithm {
        SigningAlg::Ps256 | SigningAlg::Es256 => Ok(Digest::new().set_sha256(value)),
        SigningAlg::Es384 => Ok(Digest::new().set_sha384(value)),
        SigningAlg::Ps512 => Ok(Digest::new().set_sha512(value)),
        _ => Err(c2pa::Error::CoseSignatureAlgorithmNotSupported),
    }
}

#[async_trait]
impl AsyncSigner for KmsSigner {
    async fn sign(&self, data: Vec<u8>) -> c2pa::Result<Vec<u8>> {
        let digest = kms_digest(self.algorithm, &data)?;
        let response = self
            .client
            .asymmetric_sign()
            .set_name(&self.options.key_name)
            .set_digest(digest)
            .send()
            .await
            .map_err(|err| {
                log::error!("Cloud KMS AsymmetricSign failed: {err}");
                c2pa::Error::CoseSignature
            })?;
        // ECDSA signatures are DER encoded; c2pa converts them to the COSE format.
        Ok(response.signature.to_vec())
    }

    fn alg(&self) -> SigningAlg {
        self.algorithm
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

    #[test]
    fn maps_kms_algorithms_to_c2pa() {
        assert_eq!(
            signing_algorithm(&CryptoKeyVersionAlgorithm::RsaSignPss3072Sha256).unwrap(),
            SigningAlg::Ps256
        );
        assert_eq!(
            signing_algorithm(&CryptoKeyVersionAlgorithm::RsaSignPss4096Sha512).unwrap(),
            SigningAlg::Ps512
        );
        assert_eq!(
            signing_algorithm(&CryptoKeyVersionAlgorithm::EcSignP384Sha384).unwrap(),
            SigningAlg::Es384
        );
        assert!(signing_algorithm(&CryptoKeyVersionAlgorithm::RsaSignPkcs12048Sha256).is_err());
        assert!(signing_algorithm(&CryptoKeyVersionAlgorithm::EcSignSecp256K1Sha256).is_err());
    }

    #[test]
    fn digest_uses_matching_hash() {
        let digest = kms_digest(SigningAlg::Es384, b"abc").unwrap();
        assert_eq!(digest.sha384().map(|d| d.len()), Some(48));
        let digest = kms_digest(SigningAlg::Ps512, b"abc").unwrap();
        assert_eq!(digest.sha512().map(|d| d.len()), Some(64));
    }
}
