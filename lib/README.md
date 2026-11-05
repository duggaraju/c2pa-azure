# c2pa-cloud

> **Preview.** `c2pa-cloud` is published as a pre-release (`<version>-preview`)
> alongside each [`c2pa-azure`](../azure/lib) release, and its API may still
> change. For Azure Trusted Signing alone, `c2pa-azure` remains the stable choice.

Rust helpers for issuing [C2PA](https://c2pa.org) signatures with cloud
signing services. Each cloud lives behind a feature flag and exposes a signer
that implements `c2pa::AsyncSigner`, so the rest of your `c2pa-rs` code stays
the same whichever cloud you sign with.

| Feature | Module | Service | Certificate chain |
|---------|--------|---------|-------------------|
| `azure` | `c2pa_cloud::azure` | Azure Trusted Signing (re-exports `c2pa-azure`) | Issued and returned by the service |
| `aws`   | `c2pa_cloud::aws`   | AWS KMS asymmetric keys | Supplied by you (PEM) |
| `gcp`   | `c2pa_cloud::gcp`   | Google Cloud KMS / Cloud HSM | Supplied by you (PEM) |

No cloud is enabled by default. `pdf` and `rust_native_crypto` are default
features; use `--no-default-features --features pdf,openssl` for OpenSSL.

Cargo only selects pre-release versions you ask for explicitly, so name the
preview version (see [crates.io](https://crates.io/crates/c2pa-cloud/versions)
for the latest):

```toml
c2pa-cloud = { version = "<version>-preview", features = ["aws"] }
```

AWS KMS and Cloud KMS only hold the private key. Have a C2PA-conformant
certificate authority issue a certificate for the KMS public key and pass the
chain (inline PEM or a file path) to the signer; it is ordered leaf first and
the root is dropped automatically.

## Example

```rust
use std::fs::File;

use c2pa::{Builder, Context};
use c2pa_cloud::aws::{KmsSigner, KmsSigningOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = KmsSigningOptions::new("alias/c2pa-signing", "chain.pem");
    let signer = KmsSigner::from_default_config(options).await?;

    let mut builder = Builder::from_context(Context::new())
        .with_definition(r#"{"title":"example"}"#)?;
    let mut input = File::open("sample1.png")?;
    let mut output = File::create("signed.png")?;
    builder
        .sign_async(&signer, "image/png", &mut input, &mut output)
        .await?;
    Ok(())
}
```

`c2pa_cloud::gcp::KmsSigner::from_default_client` and
`c2pa_cloud::azure::TrustedSigner::new` work the same way. Every options type
can also be read from environment variables with `Envconfig::init_from_env`:

| Cloud | Variables |
|-------|-----------|
| Azure | `SIGNING_ENDPOINT`, `SIGNING_ACCOUNT`, `CERTIFICATE_PROFILE`, `ALGORITHM`, `TIME_AUTHORITY_URL` |
| AWS   | `KMS_KEY_ID`, `CERTIFICATE_CHAIN`, `ALGORITHM` (default `ps384`), `TIME_AUTHORITY_URL` |
| GCP   | `KMS_KEY_NAME`, `CERTIFICATE_CHAIN`, `TIME_AUTHORITY_URL` (algorithm read from the key) |

See the [examples](../examples) for a CLI, an HTTP signing service and a KEDA
storage worker built on this crate.
