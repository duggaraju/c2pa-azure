# C2PA Cloud

[![c2pa-azure crate version](https://img.shields.io/crates/v/c2pa-azure.svg?label=c2pa-azure)](https://crates.io/crates/c2pa-azure)
[![Rust docs check](https://github.com/duggaraju/c2pa-azure/actions/workflows/docs.yml/badge.svg)](https://github.com/duggaraju/c2pa-azure/actions/workflows/docs.yml)
**|** [![c2pa-rs 0.91.10](https://img.shields.io/badge/c2pa--rs-0.91.10-blue)](https://crates.io/crates/c2pa/0.91.10)

Rust libraries and examples for signing C2PA content credentials using Azure Trusted Signing, AWS KMS, or Google Cloud KMS, built on `c2pa-rs`.

The repository contains two crates:

- **[`c2pa-azure`](azure/lib/README.md)**: the published Azure Trusted Signing crate. Existing users can continue using it without changes.
- **[`c2pa-cloud`](lib/README.md)**: a preview multi-cloud crate, not yet published. Its `azure` feature re-exports the same Azure signer from `c2pa-azure`; its `aws` and `gcp` features provide KMS signers.

## Features

- **Content Credentials**: Uses `c2pa-rs` to embed C2PA claims into files.
- **Thumbnail Generation**: Generates thumbnails for supported image inputs.
- **Ingredient Support**: Can include the original asset as an ingredient.
- **Custom Assertions**: Supports additional manifest assertions.
- **Cloud Signing**: Azure Trusted Signing, AWS KMS, and Google Cloud KMS signers implementing `c2pa::AsyncSigner`.
- **CLI Example**: Includes a command-line example app for local or containerized signing.
- **HTTP Service Example**: Signing service hosted on Azure Functions, AWS Lambda or Google Cloud Run.
- **KEDA Example**: Storage worker that signs uploaded files, with Azure, AWS and Google Cloud backends.

## Repository Layout

| Path | Contents |
|------|----------|
| [azure/lib](azure/lib) | The published `c2pa-azure` crate (Azure Trusted Signing) |
| [lib](lib) | `c2pa-cloud` (preview, not yet published): one crate with an Azure, AWS KMS or Google Cloud KMS signer per feature flag. See [lib/README.md](lib/README.md). |
| [examples](examples) | Cloud-agnostic examples; the cloud is picked with the `azure` (default), `aws` or `gcp` feature |
| [azure/deploy](azure/deploy), [aws/deploy](aws/deploy), [gcp/deploy](gcp/deploy) | Per-cloud hosting and deployment files for the examples |

## Crate Features

### Cloud Selection

`c2pa-cloud` defines optional provider features in [lib/Cargo.toml](lib/Cargo.toml). No provider is enabled by default; enable one or more as needed.

| Feature | Signing service | Certificate chain |
|---------|-----------------|-------------------|
| `azure` | Azure Trusted Signing | Issued and returned by the service |
| `aws` | AWS KMS | Supplied by you (PEM) |
| `gcp` | Google Cloud KMS / Cloud HSM | Supplied by you (PEM) |

Build the preview crate from this workspace with a provider enabled:

```bash
cargo build -p c2pa-cloud --features azure
cargo build -p c2pa-cloud --features aws
cargo build -p c2pa-cloud --features gcp
```

The published `c2pa-azure` crate is always Azure-specific and does not need a provider feature.

### PDF and Crypto Backends

Both crates support the following features:

- `pdf`: Enables PDF support (`c2pa/pdf`).
- `openssl`: Enables OpenSSL backend (`c2pa/openssl`).
- `rust_native_crypto`: Enables Rust-native crypto backend (`c2pa/rust_native_crypto`).

`c2pa-cloud` forwards these features to `c2pa-azure` when its `azure` feature is enabled.

Default behavior:

- `pdf` is enabled by default.
- `rust_native_crypto` is enabled by default.

PDF support works on stable Rust; nightly is not required. Build with the default features:

```bash
cargo build -p c2pa-azure
```

Disable PDF support while keeping the Rust-native crypto backend:

```bash
cargo build -p c2pa-azure --no-default-features --features rust_native_crypto
```

Use the OpenSSL backend instead of Rust-native crypto, retaining PDF support:

```bash
cargo build -p c2pa-azure --no-default-features --features pdf,openssl
```

For `c2pa-cloud`, select the provider as well, for example:

```bash
cargo build -p c2pa-cloud --no-default-features --features azure,pdf,openssl
```

## Building Workspace

```bash
cargo build
```

## Manifest and Assertions

The examples use the manifest definition in [test_data/manifest_definition.json](test_data/manifest_definition.json) unless one is passed with `-m` (CLI) or `MANIFEST_DEFINITION` (services). You can edit it to add or remove assertions and ingredients.

## Examples

Each example is built for one cloud with a feature flag: `azure` (default), `aws` or `gcp`.

| Example | Description |
|---------|-------------|
| [examples/cli](examples/cli) | Signs a local file |
| [examples/http_service](examples/http_service) | `POST /api/sign` and `POST /api/verify` HTTP service |
| [examples/keda](examples/keda) | Worker that signs files dropped into cloud storage, scaled by KEDA |

### Command Line Utility

Azure Trusted Signing:

```bash
az login
cargo run -p cli -- -i input.png -o output.png -e https://eus.codesigning.azure.net -a signing_account -c certificate_profile [-m manifest.json]
```

AWS KMS (credentials from the standard AWS provider chain):

```bash
cargo run -p cli --no-default-features --features aws -- -i input.png -o output.png \
  --kms-key-id alias/c2pa-signing --algorithm ps384 --certificate-chain chain.pem
```

Google Cloud KMS (Application Default Credentials):

```bash
cargo run -p cli --no-default-features --features gcp -- -i input.png -o output.png \
  --kms-key-name projects/p/locations/l/keyRings/r/cryptoKeys/k/cryptoKeyVersions/1 --certificate-chain chain.pem
```

AWS and Google Cloud KMS only hold the private key, so you need a certificate chain for that key from a C2PA-trusted certificate authority. Every option can also be set with its environment variable (see `cli --help`).

### HTTP Service and KEDA Worker

[examples/Dockerfile](examples/Dockerfile) builds the services for one cloud from the repository root:

```bash
docker build -f examples/Dockerfile --build-arg CLOUD=aws --target aws-lambda .
```

| Target | Hosting |
|--------|---------|
| `azure-function` | Azure Functions custom handler ([azure/deploy](azure/deploy/README.md)) |
| `aws-lambda` | AWS Lambda with the Lambda Web Adapter ([aws/deploy](aws/deploy/README.md)) |
| `http-service` | Google Cloud Run or any container host ([gcp/deploy](gcp/deploy/README.md)) |
| `keda-worker` | Kubernetes with KEDA on AKS, EKS or GKE ([examples/keda](examples/keda/README.md)), or Azure Container Apps |

## Kubernetes with KEDA

The worker runs as a KEDA `ScaledJob` from a cloud-agnostic Kustomize base in
[examples/keda/k8s](examples/keda/k8s). Each cloud has an overlay with the image,
settings, workload identity and KEDA trigger:

```bash
kubectl apply -k azure/deploy/keda   # AKS; aws/deploy/keda for EKS, gcp/deploy/keda for GKE
```

See [examples/keda/README.md](examples/keda/README.md) and the deploy README of each cloud.

## Azure Container Apps Deployment

Deploy as an Azure Container App to automate signing for files uploaded to Azure Storage.

1. Edit [common.bicepparam](azure/deploy/common.bicepparam) and [container-app.bicepparam](azure/deploy/container-app.bicepparam) with your resource names.
2. Create the common resources like the Azure code signing account, container registry etc.

```bash
cd azure/deploy
az group create --name group-name --location 'WestUS'
az deployment group create --resource-group group-name --template-file common.bicep --parameters common.bicepparam
```

3. Build the container and push to the ACR. The registry name is same as what you entered in step 1.

```bash
./build.sh -n registry-name
```

4. Deploy the container app with a managed identity and give the required permissions.

```bash
az deployment group create --resource-group group-name --template-file container-app.bicep --parameters container-app.bicepparam
```

See [azure/deploy/README.md](azure/deploy/README.md) for the AKS and Azure Functions deployments.

## Contributing

Contributions are welcome! Please open an issue or submit a pull request on GitHub.

## License

This project is licensed under the MIT License.
