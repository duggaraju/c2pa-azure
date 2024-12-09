use async_trait::async_trait;
use azure_core::{auth::TokenCredential, error::ErrorKind, Url};
use c2pa::{AsyncSigner, Builder, SigningAlg};
use serde_json::json;
use std::{
    io::{Read, Seek, Write},
    sync::Arc,
};

use crate::acs::{TrustedSigningClient, TrustedSigningClientOptions};

// const TIME_AUTHORITY_URL: &str = "http://timestamp.acs.microsoft.com";
const TIME_AUTHORITY_URL: &str = "http://timestamp.digicert.com";
const DEFAULT_ALGORITHM: SigningAlg = SigningAlg::Ps384;

#[derive(Debug)]
pub struct SigningOptions {
    account: String,
    endpoint: Url,
    certificate_profile: String,
    time_authority_url: Option<Url>,
    algorithm: c2pa::SigningAlg,
}

impl SigningOptions {
    pub fn new(
        endpoint: Url,
        account: String,
        certificate_profile: String,
        algorithm: Option<c2pa::SigningAlg>,
    ) -> Self {
        Self {
            account,
            endpoint,
            certificate_profile,
            time_authority_url: Some(Url::parse(TIME_AUTHORITY_URL).unwrap()),
            algorithm: algorithm.unwrap_or(DEFAULT_ALGORITHM),
        }
    }
}

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

    pub async fn sign<T, U>(
        &mut self,
        mut input: T,
        mut output: U,
        format: &str,
    ) -> Result<(), azure_core::Error>
    where
        T: Read + Seek + Send,
        U: Read + Write + Seek + Send,
    {
        let anchors = include_str!("trust_anchors.pem").to_owned();
        let store = include_str!("store.cfg");
        let settings = json!({
            "trust": {
                "trust_anchors": anchors,
                "trust_config": store,
            },
        });
        c2pa::settings::load_settings_from_str(&settings.to_string(), "json")
            .map_err(|x| azure_core::Error::new(ErrorKind::Other, x))?;
        let json = r##"
        {
          "format": "png"
        }
        "##;

        let mut builder =
            Builder::from_json(json).map_err(|x| azure_core::Error::new(ErrorKind::Other, x))?;
        builder
            .sign_async(self, format, &mut input, &mut output)
            .await
            .map_err(|x| azure_core::Error::new(ErrorKind::Other, x))?;
        Ok(())
    }
}

#[async_trait]
impl AsyncSigner for TrustedSigner {
    async fn sign(&self, data: Vec<u8>) -> c2pa::Result<Vec<u8>> {
        // make a rest API call to azure code signing to get the signature
        // and return it.
        let result = self
            .client
            .sign(data)
            .await
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
