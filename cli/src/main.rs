use anyhow::Result;
use azure_core::Url;
use azure_identity::DefaultAzureCredentialBuilder;
use c2pa_acs::{SigningOptions, TrustedSigner};
use clap::{arg, command, Parser};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Parser, Debug)]
#[command(
    version,
    about,
    author = "Prakash Duggaraju<duggaraju@gmail.com>",
    long_about = "A command line tool to add content credentials to a file using the Azure Code Signing service."
)]
struct Arguments {
    #[arg(short, long)]
    input: PathBuf,

    #[arg(short, long)]
    output: PathBuf,

    #[arg(short, long)]
    manifest_definition: Option<String>,

    #[arg(short, long)]
    account: String,

    #[arg(short, long)]
    endpoint: Url,

    #[arg(short, long)]
    certificate_profile: String,
}

impl Arguments {
    fn signing_options(&self) -> SigningOptions {
        SigningOptions::new(
            self.endpoint.clone(),
            self.account.clone(),
            self.certificate_profile.clone(),
        )
    }
}

const DEFAULT_MANIFEST: &str = r##"
{
}
"##;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Arguments::parse();
    let mut builder = DefaultAzureCredentialBuilder::new();
    if cfg!(debug_assertions) {
        builder.exclude_managed_identity_credential();
    }
    let credentials = Arc::new(builder.build()?);

    let options = args.signing_options();
    let mut input = File::open(&args.input)?;
    let mut output = File::create(args.output)?;
    let format = args
        .input
        .extension()
        .map(|x| x.to_str().unwrap())
        .unwrap_or("application/octet-stream");
    let manifest_definition = if let Some(manifest) = args.manifest_definition {
        let path = Path::new(&manifest);
        if path.exists() {
            fs::read_to_string(path)?
        } else {
            manifest
        }
    } else {
        DEFAULT_MANIFEST.to_owned()
    };

    let mut signer = TrustedSigner::new(credentials, options, manifest_definition).await?;
    signer.sign(&mut input, &mut output, format).await?;
    log::info!("Successfully signed the file.");
    Ok(())
}
