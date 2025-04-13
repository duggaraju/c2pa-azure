use async_lock::RwLock;
use azure_core::{
    credentials::{AccessToken, Secret, TokenCredential},
    error::{Error, ErrorKind},
    http::{HttpClient, Method, Request, StatusCode, Url, headers::HeaderName},
};
use azure_identity::TokenCredentialOptions;
use log::trace;
use serde::{
    Deserialize,
    de::{self, Deserializer},
};
use std::{collections::HashMap, env, future::Future, str, sync::Arc, time::Duration};
use time::OffsetDateTime;

#[derive(Debug)]
pub(crate) struct TokenCache(RwLock<HashMap<Vec<String>, AccessToken>>);

#[derive(Debug)]
enum ImdsId {
    #[allow(dead_code)]
    SystemAssigned,
    ClientId(String),
    #[allow(dead_code)]
    ObjectId(String),
    #[allow(dead_code)]
    MsiResId(String),
}

/// Attempts authentication using a managed identity that has been assigned to the deployment environment.
///
/// This authentication type works in Azure VMs, App Service and Azure Functions applications, as well as the Azure Cloud Shell
///
/// Built up from docs at [https://docs.microsoft.com/azure/app-service/overview-managed-identity#using-the-rest-protocol](https://docs.microsoft.com/azure/app-service/overview-managed-identity#using-the-rest-protocol)
#[derive(Debug)]
pub struct ManagedIdentityCredential {
    http_client: Arc<dyn HttpClient>,
    endpoint: Url,
    api_version: String,
    secret_header: HeaderName,
    secret_env: String,
    id: ImdsId,
    cache: TokenCache,
}

const ENDPOINT: &str = "IDENTITY_ENDPOINT";
const API_VERSION: &str = "2019-08-01";
const SECRET_HEADER: HeaderName = HeaderName::from_static("x-identity-header");
const SECRET_ENV: &str = "IDENTITY_HEADER";

impl ManagedIdentityCredential {
    fn new(
        options: impl Into<TokenCredentialOptions>,
        endpoint: Url,
        api_version: &str,
        secret_header: HeaderName,
        secret_env: &str,
        id: ImdsId,
    ) -> Self {
        let options = options.into();
        Self {
            http_client: options.http_client(),
            endpoint,
            api_version: api_version.to_owned(),
            secret_header: secret_header.to_owned(),
            secret_env: secret_env.to_owned(),
            id,
            cache: TokenCache::new(),
        }
    }

    pub fn create_with_user_assigned(options: impl Into<TokenCredentialOptions>) -> Self {
        Self::new(
            options,
            Url::parse(&env::var(ENDPOINT).expect("IDENTITY_ENDPOINT not set"))
                .expect("invalid identity endpoint"),
            API_VERSION,
            SECRET_HEADER,
            SECRET_ENV,
            ImdsId::ClientId(env::var("IDENTITY_CLIENT_ID").unwrap()),
        )
    }

    async fn get_token(&self, scopes: &[&str]) -> azure_core::Result<AccessToken> {
        let resource = scopes_to_resource(scopes)?;

        let mut query_items = vec![
            ("api-version", self.api_version.as_str()),
            ("resource", resource),
        ];

        match self.id {
            ImdsId::SystemAssigned => (),
            ImdsId::ClientId(ref client_id) => query_items.push(("client_id", client_id)),
            ImdsId::ObjectId(ref object_id) => query_items.push(("object_id", object_id)),
            ImdsId::MsiResId(ref msi_res_id) => query_items.push(("msi_res_id", msi_res_id)),
        }

        let mut url = self.endpoint.clone();
        url.query_pairs_mut().extend_pairs(query_items);

        let mut req = Request::new(url, Method::Get);

        req.insert_header("metadata", "true");

        let msi_secret = std::env::var(&self.secret_env);
        if let Ok(val) = msi_secret {
            req.insert_header(self.secret_header.clone(), val);
        };

        let rsp = self.http_client.execute_request(&req).await?;

        let (rsp_status, rsp_headers, rsp_body) = rsp.deconstruct();
        let rsp_body = rsp_body.collect().await?;

        if !rsp_status.is_success() {
            match rsp_status {
                StatusCode::BadRequest => {
                    return Err(Error::message(
                        ErrorKind::Credential,
                        "the requested identity has not been assigned to this resource",
                    ));
                }
                StatusCode::BadGateway | StatusCode::GatewayTimeout => {
                    return Err(Error::message(
                        ErrorKind::Credential,
                        "the request failed due to a gateway error",
                    ));
                }
                rsp_status => {
                    return Err(ErrorKind::http_response(
                        rsp_status,
                        Some("Error from MI client.".into()),
                    )
                    .into_error());
                }
            }
        }

        let token_response: MsiTokenResponse = rsp_body.json().await?;
        Ok(AccessToken::new(
            token_response.access_token,
            token_response.expires_on,
        ))
    }
}

#[async_trait::async_trait]
impl TokenCredential for ManagedIdentityCredential {
    async fn get_token(&self, scopes: &[&str]) -> azure_core::Result<AccessToken> {
        self.cache.get_token(scopes, self.get_token(scopes)).await
    }
}

fn expires_on_string<'de, D>(deserializer: D) -> std::result::Result<OffsetDateTime, D::Error>
where
    D: Deserializer<'de>,
{
    let v = String::deserialize(deserializer)?;
    let as_i64 = v.parse::<i64>().map_err(de::Error::custom)?;
    OffsetDateTime::from_unix_timestamp(as_i64).map_err(de::Error::custom)
}

/// Convert a `AADv2` scope to an `AADv1` resource
///
/// Directly based on the `azure-sdk-for-python` implementation:
/// ref: <https://github.com/Azure/azure-sdk-for-python/blob/d6aeefef46c94b056419613f1a5cc9eaa3af0d22/sdk/identity/azure-identity/azure/identity/_internal/__init__.py#L22>
fn scopes_to_resource<'a>(scopes: &'a [&'a str]) -> azure_core::Result<&'a str> {
    if scopes.len() != 1 {
        return Err(Error::message(
            ErrorKind::Credential,
            "only one scope is supported for IMDS authentication",
        ));
    }

    let Some(scope) = scopes.first() else {
        return Err(Error::message(
            ErrorKind::Credential,
            "no scopes were provided",
        ));
    };

    Ok(scope.strip_suffix("/.default").unwrap_or(*scope))
}

// NOTE: expires_on is a String version of unix epoch time, not an integer.
// https://docs.microsoft.com/en-us/azure/app-service/overview-managed-identity?tabs=dotnet#rest-protocol-examples
#[derive(Debug, Clone, Deserialize)]
#[allow(unused)]
struct MsiTokenResponse {
    pub access_token: Secret,
    #[serde(deserialize_with = "expires_on_string")]
    pub expires_on: OffsetDateTime,
    pub token_type: String,
    pub resource: String,
}

fn is_expired(token: &AccessToken) -> bool {
    token.expires_on < OffsetDateTime::now_utc() + Duration::from_secs(20)
}

impl TokenCache {
    fn new() -> Self {
        Self(RwLock::new(HashMap::new()))
    }

    async fn clear(&self) -> azure_core::Result<()> {
        let mut token_cache = self.0.write().await;
        token_cache.clear();
        Ok(())
    }

    async fn get_token(
        &self,
        scopes: &[&str],
        callback: impl Future<Output = azure_core::Result<AccessToken>>,
    ) -> azure_core::Result<AccessToken> {
        // if the current cached token for this resource is good, return it.
        let token_cache = self.0.read().await;
        let scopes = scopes.iter().map(ToString::to_string).collect::<Vec<_>>();
        if let Some(token) = token_cache.get(&scopes) {
            if !is_expired(token) {
                trace!("returning cached token");
                return Ok(token.clone());
            }
        }

        // otherwise, drop the read lock and get a write lock to refresh the token
        drop(token_cache);
        let mut token_cache = self.0.write().await;

        // check again in case another thread refreshed the token while we were
        // waiting on the write lock
        if let Some(token) = token_cache.get(&scopes) {
            if !is_expired(token) {
                trace!("returning token that was updated while waiting on write lock");
                return Ok(token.clone());
            }
        }

        trace!("falling back to callback");
        let token = callback.await?;

        // NOTE: we do not check to see if the token is expired here, as at
        // least one credential, `AzureCliCredential`, specifies the token is
        // immediately expired after it is returned, which indicates the token
        // should always be refreshed upon use.
        token_cache.insert(scopes, token.clone());
        Ok(token)
    }
}
