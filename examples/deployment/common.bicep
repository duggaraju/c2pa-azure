// create a storage account with aad authentication and two containers
// create parameters for the storage account name and container registry name
@description('Name of the storage account')
param storageAccountName string

@description('Name of the input container')
param inputContainerName string

@description('Name of the output container')
param outputContainerName string

@description('Name of the container registry')
param containerRegistryName string

@description('Name of the code signing account')
param codeSigningAccountName string

@description('Name of the certificate profile')
param certificateProfileName string

resource storageAccount 'Microsoft.Storage/storageAccounts@2021-06-01' = {
  name: storageAccountName
  location: resourceGroup().location
  kind: 'StorageV2'
  sku: {
    name: 'Standard_LRS'
  }
  properties: {
    allowSharedKeyAccess: false
  }

  resource blobServices 'blobServices' = {
    name: 'default'
    resource inputContaier 'containers' = {
      name: inputContainerName
    }
    resource outputContainer 'containers' = {
      name: outputContainerName
    }
  }
}

// create a container regitstry to store the images.
resource containerRegistry 'Microsoft.ContainerRegistry/registries@2023-07-01' = {
  name: containerRegistryName
  location: resourceGroup().location
  sku: {
    name: 'Basic'
  }
  properties: {
    adminUserEnabled: false
  }
}

// azure code signing account.
resource codeSigningAccount 'Microsoft.CodeSigning/codeSigningAccounts@2024-09-30-preview' = {
  name: codeSigningAccountName
  location: resourceGroup().location
  properties: {
   sku: {
     name: 'Basic'
   }  
  }
  // TODO: create a certificate profile 
  // resource certificateProfile 'certificateProfiles' = {
  //   name: certificateProfileName
  //   properties: {
  //     profileType: 'PrivateTrust'
  //     identityValidationId: '93972ae4-3158-42e4-a156-f79de75f1414'
  //   }
  // }
}
