# c2pa-azure

[![c2pa-azure crate version](https://img.shields.io/crates/v/c2pa-azure.svg?label=c2pa-azure)](https://crates.io/crates/c2pa-azure)

Rust helpers for signing C2PA content credentials using `c2pa-rs` and Azure Trusted Signing.

`TrustedSigner` implements `c2pa::AsyncSigner`, using an Azure credential and signing account to sign manifests. The certificate chain is issued and returned by Azure Trusted Signing.

## Installation

```toml
[dependencies]
c2pa-azure = "0.91.7"
```

See the [API documentation](https://docs.rs/c2pa-azure) for `SigningOptions`, `TrustedSigner`, and a signing example.

## Features

- `pdf`: enables PDF support.
- `rust_native_crypto`: enables the Rust-native crypto backend.
- `openssl`: enables the OpenSSL backend.

`pdf` and `rust_native_crypto` are enabled by default and work on stable Rust.

To use OpenSSL instead while retaining PDF support:

```toml
[dependencies]
c2pa-azure = { version = "0.91.7", default-features = false, features = ["pdf", "openssl"] }
```

## Configuration

Supply an Azure `TokenCredential` and construct `SigningOptions` directly, or use `Envconfig::init_from_env` with:

| Variable | Purpose |
|----------|---------|
| `SIGNING_ENDPOINT` | Azure Trusted Signing endpoint |
| `SIGNING_ACCOUNT` | Signing account name |
| `CERTIFICATE_PROFILE` | Certificate profile name |
| `ALGORITHM` | Optional signature algorithm; defaults to `ps384` |
| `TIME_AUTHORITY_URL` | Optional RFC 3161 timestamp authority |

## Examples and Multi-Cloud Support

The [repository](https://github.com/duggaraju/c2pa-azure) includes CLI, HTTP service, and storage-worker examples, plus Azure deployment instructions.

It also contains the preview `c2pa-cloud` crate for Azure, AWS KMS, and Google Cloud KMS. That crate is not yet published; its `azure` feature re-exports this crate's signer. Existing `c2pa-azure` users do not need to migrate.

## License

MIT.
