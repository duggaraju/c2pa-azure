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

## Manifest and assertsion.
The default manifest settings are stored in [manifest.json](manifest.json).  It can be edited to add or remove assertsion or ingredients as necessary.
## Command Line Utility

### Adding Content Credentials

```bash
az login
cargo run --bin cli -- -i input.png -o output.png -e https://eus.codesigning.azure.net -a signing_account -c certificate_profile [-m manifest.json]
```
## Azure Container App

Deploy the library as an Azure Container App to automate the signing process for media files uploaded to your Azure storage. It needs the following steps.

1. Edit common.bicepparm and container-app.bicepparm with the names of the resources you want to use.
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
