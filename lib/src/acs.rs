/// Azure Code Signing.
/// This module provides the functionality to sign a file using Azure Code Signing.
use azure_core::{
    Result, base64,
    credentials::TokenCredential,
    error::ErrorKind,
    http::{
        ClientOptions, Context, ExponentialRetryOptions, Method, Pipeline, RawResponse, Request,
        Response, RetryOptions, Url, UserAgentOptions,
    },
    sleep::sleep,
    time::Duration,
};
use bytes::Bytes;
use c2pa::SigningAlg;
use std::sync::Arc;

use crate::{auth::AuthorizationPolicy, p7b::CertificateChain};
const DEFAULT_API_VERSION: &str = "2022-06-15-preview";
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
        let user_agent = UserAgentOptions {
            application_id: Some(format!("c2pa-azure/{}", env!("CARGO_PKG_VERSION"))),
        };
        Self {
            api_version: DEFAULT_API_VERSION.to_owned(),
            account: account.to_owned(),
            certificate_profile: certificate_profile.to_owned(),
            scope: DEFAULT_SCOPE.to_owned(),
            client_options: ClientOptions {
                retry: RetryOptions::exponential(ExponentialRetryOptions {
                    max_retries: 5,
                    max_delay: Duration::seconds(10),
                    ..Default::default()
                }),
                user_agent,
                ..Default::default()
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct TrustedSigningClient {
    endpoint: Url,
    options: TrustedSigningClientOptions,
    pipeline: Pipeline,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SigningRequest {
    signature_algorithm: String,
    digest: String,
}

#[derive(serde::Deserialize, Clone, Eq, PartialEq, Debug)]
enum Status {
    InProgress,
    Succeeded,
    Failed,
    TimedOut,
    NotFound,
    Running,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct SigningStatus {
    pub operation_id: String,
    pub status: Status,
    pub signature: Option<String>,
    pub signing_certificate: Option<String>,
}

impl SigningRequest {
    pub fn new(alg: SigningAlg, digest: &[u8]) -> Self {
        Self {
            signature_algorithm: alg.to_string(),
            digest: base64::encode(digest),
        }
    }
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
                None,
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
        let response: RawResponse = self
            .pipeline
            .send(&context, &mut request, None)
            .await?;
        let body = response.into_body();
        let bytes = Bytes::from(body);
        let cert = CertificateChain::from_cert_chain(bytes);
        let pem = cert
            .get_pem_certificates()
            .map_err(|x| azure_core::Error::new(ErrorKind::DataConversion, x))?;
        Ok(pem)
    }

    pub async fn sign(&self, data: &[u8]) -> Result<Vec<u8>> {
        let url = self.endpoint.join(&format!(
            "/codesigningaccounts/{}/certificateprofiles/{}/sign?api-version={}",
            self.options.account, self.options.certificate_profile, self.options.api_version
        ))?;
        let context = Context::new();
        let mut request = Request::new(url, Method::Post);
        request.insert_header("content-type", "application/json");
        let data = SigningRequest::new(SigningAlg::Ps384, data);
        request.set_json(&data)?;

        for _ in 0..5 {
            let response: Response<SigningStatus> = self
                .pipeline
                .send(&context, &mut request, None)
                .await?
                .into();
            let status: SigningStatus = response.into_body()?;
            log::info!(
                "Signing operation: {}, status: {:?}",
                status.operation_id,
                status.status
            );
            if status.status == Status::Succeeded {
                log::info!(
                    "Signing request succeeded operation: {}",
                    status.operation_id
                );
                let signature = base64::decode(status.signature.unwrap())?;
                return Ok(signature);
            } else if status.status != Status::InProgress {
                return Err(azure_core::Error::new(
                    ErrorKind::Other,
                    format!("Signing request failed with status: {:?}", status.status),
                ));
            }
            sleep(Duration::milliseconds(250)).await;
            let url = self.endpoint.join(&format!(
                "/codesigningaccounts/{}/certificateprofiles/{}/sign/{}?api-version={}",
                self.options.account,
                self.options.certificate_profile,
                status.operation_id,
                self.options.api_version,
            ))?;
            request = Request::new(url, Method::Get);
        }

        Err(azure_core::Error::new(
            ErrorKind::Other,
            "Signing request did not succeed after 5 iterations".to_owned(),
        ))
    }
}
