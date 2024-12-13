# c2pa-azure

`c2pa-azure` is a Rust library that leverages the `c2pa-rs` library and Azure Code Signing service to add content credentials to media files. This library provides a robust solution for ensuring the authenticity and integrity of digital media by embedding cryptographic signatures and metadata.

## Features

- **Content Credentials**: Utilize `c2pa-rs` to embed content credentials into media files, ensuring their authenticity.
- **Thumbnail Generation**: Automatically generates thumbnails for image files.
- **Ingredient support**: Add the original file as an ingredient.
- **Custom Assertions**: Ability to add custom assertions.
- **Azure Code Signing**: Integrate with Azure Code Signing service to securely sign media content.
- **Command Line Utility**: A command line tools for running locally or in a container in azure to sign a file.
- **Azure Container App Support**: Support to create a container and deploy to Azure Container Apps for running and scaling using Keda.

## Building

Add the following to your `Cargo.toml`:

```bash
cargo build
```

## Command Line Utility

### Adding Content Credentials

```bash
cargo run --bin cli -- -i input.png -o output.png -e https://eus.codesigning.azure.net -a signing_account -c certificate_profile [-m manifest.json]
```
## Azure Container App

Deploy the library as an Azure Container App to automate the signing process for media files uploaded to your Azure storage.

## Contributing

Contributions are welcome! Please open an issue or submit a pull request on GitHub.

## License

This project is licensed under the MIT License.
