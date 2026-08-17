# c2pa-azure

[![Crates.io](https://img.shields.io/crates/v/c2pa-azure.svg)](https://crates.io/crates/c2pa-azure)
[![Rust docs check](https://github.com/duggaraju/c2pa-azure/actions/workflows/docs.yml/badge.svg)](https://github.com/duggaraju/c2pa-azure/actions/workflows/docs.yml)
**|** [![c2pa-rs crate version](https://img.shields.io/crates/v/c2pa.svg)](https://crates.io/crates/c2pa)

`c2pa-azure` is a Rust library that uses `c2pa-rs` and Azure Trusted Signing to add C2PA content credentials to media files. It helps ensure authenticity and integrity by embedding signed provenance m[...]

## Features

- **Content Credentials**: Uses `c2pa-rs` to embed C2PA claims into files.
- **Thumbnail Generation**: Generates thumbnails for supported image inputs.
- **Ingredient Support**: Can include the original asset as an ingredient.
- **Custom Assertions**: Supports additional manifest assertions.
- **Azure Trusted Signing Integration**: Signs content using Azure services.
- **CLI Example**: Includes a command-line example app for local or containerized signing.
- **Container App Example**: Includes deployment templates for Azure Container Apps + KEDA.

## Crate Features

`c2pa-azure` defines feature flags in [lib/Cargo.toml](lib/Cargo.toml):

- `pdf`: Enables PDF support (`c2pa/pdf`).
- `openssl`: Enables OpenSSL backend (`c2pa/openssl`).
- `rust_native_crypto`: Enables Rust-native crypto backend (`c2pa/rust_native_crypto`).

Default behavior:

- `pdf` is disabled by default.
- `rust_native_crypto` is enabled by default.

Enable PDF explicitly when needed. The `pdf` feature currently requires the Rust nightly toolchain:

```bash
cargo +nightly build -p c2pa-azure --features pdf
```

Use OpenSSL backend instead of defaults:

```bash
cargo build -p c2pa-azure --no-default-features --features openssl
```

## Building Workspace

```bash
cargo build
```

## Manifest and Assertions

Default manifest settings are in [test_data/manifest.json](test_data/manifest.json). You can edit it to add or remove assertions and ingredients.

## Command Line Utility

### Adding Content Credentials

```bash
az login
cargo run --bin cli -- -i input.png -o output.png -e https://eus.codesigning.azure.net -a signing_account -c certificate_profile [-m manifest.json]
```

## Azure Container App

Deploy as an Azure Container App to automate signing for files uploaded to Azure Storage.

1. Edit [common.bicepparam](examples/deployment/common.bicepparam) and [container-app.bicepparam](examples/deployment/container-app.bicepparam) with your resource names.
2. Create the common resoures like azure code signing account, container registry etc.

```bash
cd deployment
az group create group-name -location 'WestUS'
az deployment group create --resource-group group-name  --template-file common.bicep --parameters common.bicepparam
```

3. Build the container and push to the ACR. The registry name is same as what you entered in step 1.

```bash
./build.sh -n registry-name
```

4. Deploy the continer app with a managed identity and give the required permissions.

```bash
az deployment group create --resource-group group-name  --template-file container-app.bicep --parameters container-app.bicepparam
```

## Contributing

Contributions are welcome! Please open an issue or submit a pull request on GitHub.

## License

This project is licensed under the MIT License.
