use anyhow::Result;
use azure_core::Url;
use azure_identity::create_default_credential;
use c2pa_acs::{SigningOptions, TrustedSigner};
use clap::{arg, command, Parser};
use std::{fs::File, path::PathBuf};

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

    #[arg(short = 'g', long)]
    algorithm: Option<c2pa::SigningAlg>,
}

impl Arguments {
    fn signing_options(&self) -> SigningOptions {
        SigningOptions::new(
            self.endpoint.clone(),
            self.account.clone(),
            self.certificate_profile.clone(),
            self.algorithm,
        )
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Arguments::parse();
    let credentials = create_default_credential()?;

    let options = args.signing_options();
    let mut input = File::open(&args.input)?;
    let mut output = File::create(args.output)?;
    let format = args
        .input
        .extension()
        .map(|x| x.to_str().unwrap())
        .unwrap_or("application/octet-stream");
    let mut signer = TrustedSigner::new(credentials, options).await?;
    signer.sign(&mut input, &mut output, format).await?;
    Ok(())
}
