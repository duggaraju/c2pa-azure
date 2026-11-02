// Hosts examples/http_service as an Azure Functions custom handler in a container.
// Build and push the image first: ./build.sh -n <registry> -t azure-function

@description('Container registry that holds the azure-function image')
param containerRegistryName string
param imageName string

@description('Storage account used by the Functions host (AzureWebJobsStorage)')
param storageAccountName string

@description('Name of the code signing account')
param codeSigningAccountName string

@description('certificate profile in the signing account')
param certificateProfileName string

@description('managed identity for the function app')
param functionManagedIdentityName string

@description('Elastic Premium plan; custom containers are not supported on the Consumption plan')
param hostingPlanName string

param functionAppName string

param location string = resourceGroup().location

resource functionManagedIdentity 'Microsoft.ManagedIdentity/userAssignedIdentities@2023-01-31' = {
  name: functionManagedIdentityName
  location: location
}

resource storageAccount 'Microsoft.Storage/storageAccounts@2021-06-01' existing = {
  name: storageAccountName
}

resource containerRegistry 'Microsoft.ContainerRegistry/registries@2023-07-01' existing = {
  name: containerRegistryName
}

resource codeSigningAccount 'Microsoft.CodeSigning/codeSigningAccounts@2024-09-30-preview' existing = {
  name: codeSigningAccountName
}

// AcrPull
var acrRoleDefinitionResourceID = resourceId('Microsoft.Authorization/roleDefinitions', '7f951dda-4ed3-4680-a7ca-43fe172d538d')
resource acrRoleAssignment 'Microsoft.Authorization/roleAssignments@2022-04-01' = {
  scope: containerRegistry
  name: guid(containerRegistry.id, functionManagedIdentity.id, acrRoleDefinitionResourceID)
  properties: {
    roleDefinitionId: acrRoleDefinitionResourceID
    principalId: functionManagedIdentity.properties.principalId
    principalType: 'ServicePrincipal'
  }
}

// Storage Blob Data Owner, for the identity based AzureWebJobsStorage connection
var blobRoleDefinitionResourceID = resourceId('Microsoft.Authorization/roleDefinitions', 'b7e6dc6d-f1e8-4753-8033-0f276bb0955b')
resource blobRoleAssignment 'Microsoft.Authorization/roleAssignments@2022-04-01' = {
  scope: storageAccount
  name: guid(storageAccount.id, functionManagedIdentity.id, blobRoleDefinitionResourceID)
  properties: {
    roleDefinitionId: blobRoleDefinitionResourceID
    principalId: functionManagedIdentity.properties.principalId
    principalType: 'ServicePrincipal'
  }
}

// Trusted Signing Certificate Profile Signer
var codeSigningRoleDefinitionResourceID = resourceId('Microsoft.Authorization/roleDefinitions', '2837e146-70d7-4cfd-ad55-7efa6464f958')
resource codeSigningRoleAssignment 'Microsoft.Authorization/roleAssignments@2022-04-01' = {
  scope: codeSigningAccount
  name: guid(codeSigningAccount.id, functionManagedIdentity.id, codeSigningRoleDefinitionResourceID)
  properties: {
    roleDefinitionId: codeSigningRoleDefinitionResourceID
    principalId: functionManagedIdentity.properties.principalId
    principalType: 'ServicePrincipal'
  }
}

resource hostingPlan 'Microsoft.Web/serverfarms@2024-04-01' = {
  name: hostingPlanName
  location: location
  kind: 'elastic'
  sku: {
    name: 'EP1'
    tier: 'ElasticPremium'
  }
  properties: {
    reserved: true
  }
}

resource functionApp 'Microsoft.Web/sites@2024-04-01' = {
  name: functionAppName
  location: location
  kind: 'functionapp,linux,container'
  identity: {
    type: 'UserAssigned'
    userAssignedIdentities: {
      '${functionManagedIdentity.id}': {}
    }
  }
  properties: {
    serverFarmId: hostingPlan.id
    httpsOnly: true
    siteConfig: {
      linuxFxVersion: 'DOCKER|${containerRegistry.properties.loginServer}/${imageName}:latest'
      acrUseManagedIdentityCreds: true
      acrUserManagedIdentityID: functionManagedIdentity.properties.clientId
      appSettings: [
        {
          name: 'FUNCTIONS_EXTENSION_VERSION'
          value: '~4'
        }
        {
          name: 'FUNCTIONS_WORKER_RUNTIME'
          value: 'custom'
        }
        {
          name: 'WEBSITES_ENABLE_APP_SERVICE_STORAGE'
          value: 'false'
        }
        {
          name: 'AzureWebJobsStorage__accountName'
          value: storageAccount.name
        }
        {
          name: 'AzureWebJobsStorage__credential'
          value: 'managedidentity'
        }
        {
          name: 'AzureWebJobsStorage__clientId'
          value: functionManagedIdentity.properties.clientId
        }
        {
          name: 'AZURE_CLIENT_ID'
          value: functionManagedIdentity.properties.clientId
        }
        {
          name: 'SIGNING_ENDPOINT'
          value: codeSigningAccount.properties.accountUri
        }
        {
          name: 'SIGNING_ACCOUNT'
          value: codeSigningAccountName
        }
        {
          name: 'CERTIFICATE_PROFILE'
          value: certificateProfileName
        }
      ]
    }
  }
  dependsOn: [
    acrRoleAssignment
    blobRoleAssignment
    codeSigningRoleAssignment
  ]
}

output functionAppHostName string = functionApp.properties.defaultHostName
