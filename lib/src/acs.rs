/// Azure Code Signing.
/// This module provides the functionality to sign a file using Azure Code Signing.
use azure_core::{
    auth::TokenCredential, error::ErrorKind, ClientOptions, Context, ExponentialRetryOptions,
    Method, Pipeline, Request, Result, RetryOptions, TelemetryOptions, Url,
};
use std::sync::Arc;

use crate::{auth::AuthorizationPolicy, p7b::P7bCertificate};
const DEFAULT_API_VERSION: &str = "2023-06-15-preview";
const DEFAULT_SCOPE: &str = "https://codesigning.azure.net/.default";

#[derive(Clone, Debug)]
pub struct TrustedSigningClientOptions {
    pub api_version: String,
    pub account: String,
    pub certificate_profile: String,
    pub client_options: ClientOptions,
    pub scope: String,
}

impl TrustedSigningClientOptions {
    pub fn new(account: &str, certificate_profile: &str) -> Self {
        Self {
            api_version: DEFAULT_API_VERSION.to_owned(),
            account: account.to_owned(),
            certificate_profile: certificate_profile.to_owned(),
            scope: DEFAULT_SCOPE.to_owned(),
            client_options: ClientOptions::default()
                .retry(RetryOptions::exponential(
                    ExponentialRetryOptions::default().max_retries(5u32),
                ))
                .telemetry(TelemetryOptions::default().application_id("c2pa-prss")),
        }
    }
}

pub struct TrustedSigningClient {
    endpoint: Url,
    options: TrustedSigningClientOptions,
    pipeline: Pipeline,
}

impl TrustedSigningClient {
    pub fn new(
        endpoint: Url,
        credential: Arc<dyn TokenCredential>,
        options: TrustedSigningClientOptions,
    ) -> Self {
        let client_options = options.client_options.clone();
        let scope = options.scope.clone();
        Self {
            endpoint,
            options,
            pipeline: Pipeline::new(
                option_env!("CARGO_PKG_NAME"),
                option_env!("CARGO_PKG_VERSION"),
                client_options,
                vec![Arc::new(AuthorizationPolicy::new(credential, scope))],
                vec![],
            ),
        }
    }

    pub async fn get_certificates(&self) -> Result<Vec<Vec<u8>>> {
        let url = self.endpoint.join(&format!(
            "/codesigningaccounts/{}/certificateprofiles/{}/sign/certchain?api-version={}",
            self.options.account, self.options.certificate_profile, self.options.api_version
        ))?;
        let context = Context::new();
        let mut request = Request::new(url, Method::Get);
        request.insert_header("accept", "application/pkcs7-mime");
        let response = self.pipeline.send(&context, &mut request).await?;
        let body = response.into_body();
        let bytes = body.collect().await?;
        let cert = CertificateChain::from_cert_chain(bytes);
        let pem = cert
            .get_pem_certificates()
            .map_err(|x| azure_core::Error::new(ErrorKind::DataConversion, x))?;
        Ok(pem)
    }

    pub async fn sign(&self, data: Vec<u8>) -> Result<Vec<u8>> {
        let url = self.endpoint.join(&format!(
            "/codesigningaccounts/{}/certificateprofiles/{}/sign?api-version={}",
            self.options.account, self.options.certificate_profile, self.options.api_version
        ))?;
        let context = Context::new();
        let mut request = Request::new(url, Method::Post);
        request.set_body(data);
        let response = self.pipeline.send(&context, &mut request).await?;
        let body = response.into_body();
        let bytes = body.collect().await?;
        return Ok(bytes.to_vec());
    }
}
