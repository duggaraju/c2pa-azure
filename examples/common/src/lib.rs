//! Code shared by the c2pa-cloud examples: picking the cloud, building the
//! matching signer, and loading the manifest definition.
//!
//! Each example has `azure` (default), `aws`, and `gcp` features. The cloud
//! used at runtime is chosen by [`Cloud::select`].

use std::{env, fmt, fs, path::Path, str::FromStr};

use anyhow::{Context as _, Result, bail};
use c2pa::AsyncSigner;
use c2pa_cloud::Envconfig;

#[cfg(not(any(feature = "azure", feature = "aws", feature = "gcp")))]
compile_error!("enable at least one cloud feature: `azure`, `aws`, or `gcp`");

/// Manifest definition used when `MANIFEST_DEFINITION` is not set.
pub const DEFAULT_MANIFEST: &str = include_str!("../../../test_data/manifest_definition.json");

/// A boxed signer for whichever cloud was selected.
pub type Signer = Box<dyn AsyncSigner + Send + Sync>;

/// A cloud signing provider compiled into this binary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cloud {
    #[cfg(feature = "azure")]
    Azure,
    #[cfg(feature = "aws")]
    Aws,
    #[cfg(feature = "gcp")]
    Gcp,
}

impl Cloud {
    /// Clouds compiled into this binary, in declaration order.
    pub const ENABLED: &'static [Cloud] = &[
        #[cfg(feature = "azure")]
        Cloud::Azure,
        #[cfg(feature = "aws")]
        Cloud::Aws,
        #[cfg(feature = "gcp")]
        Cloud::Gcp,
    ];

    pub fn name(self) -> &'static str {
        match self {
            #[cfg(feature = "azure")]
            Cloud::Azure => "azure",
            #[cfg(feature = "aws")]
            Cloud::Aws => "aws",
            #[cfg(feature = "gcp")]
            Cloud::Gcp => "gcp",
        }
    }

    /// Picks the cloud: an explicit choice (for example `--cloud` or `CLOUD`)
    /// wins; otherwise [`Cloud::from_features`] decides.
    pub fn select(explicit: Option<&str>) -> Result<Self> {
        match explicit {
            Some(name) => name.parse(),
            None => match env::var("CLOUD") {
                Ok(name) => name.parse(),
                Err(env::VarError::NotPresent) => Self::from_features(),
                Err(err) => Err(err).context("invalid CLOUD environment variable"),
            },
        }
    }

    /// Chooses the cloud from the enabled features. `azure` is on by default,
    /// so enabling exactly one of `aws` or `gcp` selects that cloud.
    pub fn from_features() -> Result<Self> {
        let selected: Vec<Cloud> = Self::ENABLED
            .iter()
            .copied()
            .filter(|cloud| cloud.name() != "azure")
            .collect();
        match selected.as_slice() {
            [] => Ok(Self::ENABLED[0]),
            [only] => Ok(*only),
            _ => bail!(
                "several clouds are enabled ({}); set CLOUD to choose one",
                Self::enabled_names()
            ),
        }
    }

    fn enabled_names() -> String {
        Self::ENABLED
            .iter()
            .map(|cloud| cloud.name())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl fmt::Display for Cloud {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Cloud {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        let value = value.trim().to_ascii_lowercase();
        Self::ENABLED
            .iter()
            .copied()
            .find(|cloud| cloud.name() == value)
            .with_context(|| {
                format!(
                    "cloud `{value}` is not enabled in this build (enabled: {}); rebuild with `--features {value}`",
                    Self::enabled_names()
                )
            })
    }
}

/// Creates the signer for `cloud`, reading its settings from the environment.
///
/// - Azure: `SIGNING_ENDPOINT`, `SIGNING_ACCOUNT`, `CERTIFICATE_PROFILE`.
/// - AWS: `KMS_KEY_ID`, `CERTIFICATE_CHAIN`.
/// - GCP: `KMS_KEY_NAME`, `CERTIFICATE_CHAIN`.
///
/// All clouds also accept `ALGORITHM` (Azure/AWS) and `TIME_AUTHORITY_URL`.
pub async fn signer_from_env(cloud: Cloud) -> Result<Signer> {
    log::info!("Using the {cloud} signer");
    match cloud {
        #[cfg(feature = "azure")]
        Cloud::Azure => {
            let options = c2pa_cloud::azure::SigningOptions::init_from_env()
                .context("missing Azure Trusted Signing settings")?;
            azure_signer(options).await
        }
        #[cfg(feature = "aws")]
        Cloud::Aws => {
            let options = c2pa_cloud::aws::KmsSigningOptions::init_from_env()
                .context("missing AWS KMS settings")?;
            aws_signer(options).await
        }
        #[cfg(feature = "gcp")]
        Cloud::Gcp => {
            let options = c2pa_cloud::gcp::KmsSigningOptions::init_from_env()
                .context("missing Google Cloud KMS settings")?;
            gcp_signer(options).await
        }
    }
}

#[cfg(feature = "azure")]
pub async fn azure_signer(options: c2pa_cloud::azure::SigningOptions) -> Result<Signer> {
    let signer = c2pa_cloud::azure::TrustedSigner::new(azure_credential()?, options).await?;
    Ok(Box::new(signer))
}

#[cfg(feature = "aws")]
pub async fn aws_signer(options: c2pa_cloud::aws::KmsSigningOptions) -> Result<Signer> {
    let signer = c2pa_cloud::aws::KmsSigner::from_default_config(options).await?;
    Ok(Box::new(signer))
}

#[cfg(feature = "gcp")]
pub async fn gcp_signer(options: c2pa_cloud::gcp::KmsSigningOptions) -> Result<Signer> {
    let signer = c2pa_cloud::gcp::KmsSigner::from_default_client(options).await?;
    Ok(Box::new(signer))
}

/// Azure credential: workload identity when the AKS webhook has injected a
/// federated token, otherwise Azure CLI in debug builds and managed identity in
/// release builds (user-assigned when `AZURE_CLIENT_ID` is set).
#[cfg(feature = "azure")]
pub fn azure_credential() -> Result<std::sync::Arc<dyn azure_core::credentials::TokenCredential>>
{
    use azure_identity::{
        AzureCliCredential, ManagedIdentityCredential, ManagedIdentityCredentialOptions,
        UserAssignedId, WorkloadIdentityCredential,
    };

    if env::var_os("AZURE_FEDERATED_TOKEN_FILE").is_some() {
        return Ok(WorkloadIdentityCredential::new(None)?);
    }
    if cfg!(debug_assertions) {
        return Ok(AzureCliCredential::new(None)?);
    }
    let options = env::var("AZURE_CLIENT_ID")
        .ok()
        .map(|client_id| ManagedIdentityCredentialOptions {
            user_assigned_id: Some(UserAssignedId::ClientId(client_id)),
            ..Default::default()
        });
    Ok(ManagedIdentityCredential::new(options)?)
}

/// Reads the manifest definition from `MANIFEST_DEFINITION` (a file path or
/// inline JSON), falling back to [`DEFAULT_MANIFEST`].
pub fn manifest_definition_from_env() -> Result<String> {
    match env::var("MANIFEST_DEFINITION") {
        Ok(value) if Path::new(&value).exists() => {
            fs::read_to_string(&value).with_context(|| format!("failed to read {value}"))
        }
        Ok(value) => Ok(value),
        Err(_) => Ok(DEFAULT_MANIFEST.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_enabled_clouds_case_insensitively() {
        for cloud in Cloud::ENABLED {
            let upper = cloud.name().to_ascii_uppercase();
            assert_eq!(upper.parse::<Cloud>().unwrap(), *cloud);
        }
        assert!("other".parse::<Cloud>().is_err());
    }

    #[test]
    fn explicit_choice_wins() {
        let cloud = Cloud::ENABLED[Cloud::ENABLED.len() - 1];
        assert_eq!(Cloud::select(Some(cloud.name())).unwrap(), cloud);
    }

    #[test]
    fn features_pick_the_non_default_cloud() {
        let non_azure = Cloud::ENABLED
            .iter()
            .filter(|cloud| cloud.name() != "azure")
            .count();
        match non_azure {
            0 => assert_eq!(Cloud::from_features().unwrap().name(), "azure"),
            1 => assert_ne!(Cloud::from_features().unwrap().name(), "azure"),
            _ => assert!(Cloud::from_features().is_err()),
        }
    }
}
