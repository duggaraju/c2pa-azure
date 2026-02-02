use async_trait::async_trait;
use azure_core::{credentials::TokenCredential, error::ErrorKind, http::Url};
use c2pa::{AsyncSigner, SigningAlg};
use envconfig::Envconfig;
use sha2::{Digest, Sha384};
use std::sync::Arc;

use crate::acs::{TrustedSigningClient, TrustedSigningClientOptions};

const TIME_AUTHORITY_URL: &str = "http://timestamp.acs.microsoft.com";
// const TIME_AUTHORITY_URL: &str = "http://timestamp.digicert.com";
const DEFAULT_ALGORITHM: SigningAlg = SigningAlg::Ps384;

#[derive(Clone, Debug, Envconfig)]
pub struct SigningOptions {
    #[envconfig(from = "SIGNING_ACCOUNT")]
    account: String,
    #[envconfig(from = "SIGNING_ENDPOINT")]
    endpoint: Url,
    #[envconfig(from = "CERTIFICATE_PROFILE")]
    certificate_profile: String,
    time_authority_url: Option<Url>,
    #[envconfig(from = "ALGORITHM", default = "ps384")]
    algorithm: c2pa::SigningAlg,
}

impl SigningOptions {
    pub fn new(
        endpoint: Url,
        account: String,
        certificate_profile: String,
        time_authority_url: Option<&str>,
    ) -> Self {
        Self {
            account,
            endpoint,
            certificate_profile,
            time_authority_url: Url::parse(time_authority_url.unwrap_or(TIME_AUTHORITY_URL)).ok(),
            algorithm: DEFAULT_ALGORITHM,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TrustedSigner {
    options: SigningOptions,
    client: TrustedSigningClient,
    certificates: Vec<Vec<u8>>,
}

impl TrustedSigner {
    pub async fn new(
        credential: Arc<dyn TokenCredential>,
        options: SigningOptions,
    ) -> azure_core::Result<Self> {
        let client_options =
            TrustedSigningClientOptions::new(&options.account, &options.certificate_profile);
        let client =
            TrustedSigningClient::new(options.endpoint.clone(), credential, client_options);
        let certificates = client.get_certificates().await?;

        Ok(Self {
            options,
            client,
            certificates,
        })
    }

    fn get_digest(&self, data: Vec<u8>) -> azure_core::Result<Vec<u8>> {
        if SigningAlg::Ps384 == self.options.algorithm {
            let mut hasher = Sha384::new();
            hasher.update(&data);
            let result = hasher.finalize();
            Ok(result.to_vec())
        } else {
            Err(azure_core::Error::new(
                ErrorKind::Other,
                "Unsupported algorithm",
            ))
        }
    }
}

#[async_trait]
impl AsyncSigner for TrustedSigner {
    async fn sign(&self, data: Vec<u8>) -> c2pa::Result<Vec<u8>> {
        // make a rest API call to azure code signing to get the signature
        // and return it.
        // get the digest of the data.
        let digest = self
            .get_digest(data)
            .map_err(|_| c2pa::Error::CoseSignatureAlgorithmNotSupported)?;
        let result = self
            .client
            .sign(&digest)
            .await
            .inspect_err(|x| log::error!("Error signing data: {x:?}"))
            .map_err(|_| c2pa::Error::CoseSignature)?;
        Ok(result)
    }

    fn alg(&self) -> c2pa::SigningAlg {
        self.options.algorithm
    }

    fn certs(&self) -> c2pa::Result<Vec<Vec<u8>>> {
        Ok(self.certificates.clone())
    }

    fn reserve_size(&self) -> usize {
        20000
    }

    #[doc = " URL for time authority to time stamp the signature"]
    fn time_authority_url(&self) -> Option<String> {
        self.options
            .time_authority_url
            .as_ref()
            .map(|x| x.to_string())
    }
}
