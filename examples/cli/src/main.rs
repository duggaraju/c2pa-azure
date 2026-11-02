use anyhow::{Context as _, Result};
use c2pa::{Builder, Context};
use clap::Parser;
use examples_common::{Cloud, Signer};
use std::{
    fs::{self, File, OpenOptions},
    path::PathBuf,
};
use url::Url;

#[derive(Parser, Debug)]
#[command(
    version,
    about,
    author = "Prakash Duggaraju<duggaraju@gmail.com>",
    long_about = "A command line tool to add content credentials to a file using a cloud signing service: Azure Trusted Signing, AWS KMS, or Google Cloud KMS."
)]
struct Arguments {
    #[arg(short, long)]
    input: PathBuf,

    #[arg(short, long)]
    output: PathBuf,

    #[arg(short, long)]
    manifest_definition: Option<PathBuf>,

    #[arg(short = 's', long, value_name = "PATH")]
    settings: Option<PathBuf>,

    /// Cloud to sign with (azure, aws, gcp). Defaults to the CLOUD environment
    /// variable, then to the cloud enabled by the build features.
    #[arg(long)]
    cloud: Option<String>,

    /// RFC 3161 timestamp authority.
    #[arg(long, env = "TIME_AUTHORITY_URL", default_value = c2pa_cloud::DEFAULT_TIME_AUTHORITY_URL)]
    time_authority_url: Url,

    /// Azure Trusted Signing account name.
    #[cfg(feature = "azure")]
    #[arg(short, long, env = "SIGNING_ACCOUNT")]
    account: Option<String>,

    /// Azure Trusted Signing endpoint, for example https://eus.codesigning.azure.net.
    #[cfg(feature = "azure")]
    #[arg(short, long, env = "SIGNING_ENDPOINT")]
    endpoint: Option<Url>,

    /// Azure Trusted Signing certificate profile.
    #[cfg(feature = "azure")]
    #[arg(short, long, env = "CERTIFICATE_PROFILE")]
    certificate_profile: Option<String>,

    /// AWS KMS key ID, key ARN, or alias.
    #[cfg(feature = "aws")]
    #[arg(long, env = "KMS_KEY_ID")]
    kms_key_id: Option<String>,

    /// AWS KMS signature algorithm (ps256, ps384, ps512, es256, es384, es512).
    #[cfg(feature = "aws")]
    #[arg(long, env = "ALGORITHM", default_value = "ps384")]
    algorithm: c2pa::SigningAlg,

    /// Google Cloud KMS key version resource name.
    #[cfg(feature = "gcp")]
    #[arg(long, env = "KMS_KEY_NAME")]
    kms_key_name: Option<String>,

    /// PEM certificate chain for the KMS key, inline or as a file path.
    #[cfg(any(feature = "aws", feature = "gcp"))]
    #[arg(long, env = "CERTIFICATE_CHAIN", value_name = "PEM_OR_PATH")]
    certificate_chain: Option<String>,
}

const DEFAULT_SETTINGS: &str = include_str!("../../../test_data/settings.toml");

fn required<T: Clone>(value: &Option<T>, flag: &str, env: &str) -> Result<T> {
    value
        .clone()
        .with_context(|| format!("missing {flag} (or the {env} environment variable)"))
}

impl Arguments {
    async fn signer(&self, cloud: Cloud) -> Result<Signer> {
        match cloud {
            #[cfg(feature = "azure")]
            Cloud::Azure => {
                let options = c2pa_cloud::azure::SigningOptions::new(
                    required(&self.endpoint, "--endpoint", "SIGNING_ENDPOINT")?,
                    required(&self.account, "--account", "SIGNING_ACCOUNT")?,
                    required(
                        &self.certificate_profile,
                        "--certificate-profile",
                        "CERTIFICATE_PROFILE",
                    )?,
                    Some(self.time_authority_url.as_str()),
                );
                examples_common::azure_signer(options).await
            }
            #[cfg(feature = "aws")]
            Cloud::Aws => {
                let options = c2pa_cloud::aws::KmsSigningOptions::new(
                    required(&self.kms_key_id, "--kms-key-id", "KMS_KEY_ID")?,
                    required(
                        &self.certificate_chain,
                        "--certificate-chain",
                        "CERTIFICATE_CHAIN",
                    )?,
                )
                .with_algorithm(self.algorithm)
                .with_time_authority_url(Some(self.time_authority_url.clone()));
                examples_common::aws_signer(options).await
            }
            #[cfg(feature = "gcp")]
            Cloud::Gcp => {
                let options = c2pa_cloud::gcp::KmsSigningOptions::new(
                    required(&self.kms_key_name, "--kms-key-name", "KMS_KEY_NAME")?,
                    required(
                        &self.certificate_chain,
                        "--certificate-chain",
                        "CERTIFICATE_CHAIN",
                    )?,
                )
                .with_time_authority_url(Some(self.time_authority_url.clone()));
                examples_common::gcp_signer(options).await
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Arguments::parse();
    let cloud = Cloud::select(args.cloud.as_deref())?;
    let signer = args.signer(cloud).await?;

    let mut input = File::open(&args.input)?;
    let mut output = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&args.output)?;
    let format = args
        .input
        .extension()
        .map(|x| x.to_str().unwrap())
        .unwrap_or("application/octet-stream");

    let settings = if let Some(path) = args.settings.as_ref() {
        fs::read_to_string(path)?
    } else {
        DEFAULT_SETTINGS.to_owned()
    };
    let context = Context::new().with_settings(settings)?;

    let manifest_definition = if let Some(path) = args.manifest_definition.as_ref() {
        fs::read_to_string(path)?
    } else {
        examples_common::DEFAULT_MANIFEST.to_owned()
    };

    let mut builder = Builder::from_context(context).with_definition(&manifest_definition)?;
    builder
        .sign_async(signer.as_ref(), format, &mut input, &mut output)
        .await?;
    log::info!("Successfully signed the file with {cloud}.");
    Ok(())
}
