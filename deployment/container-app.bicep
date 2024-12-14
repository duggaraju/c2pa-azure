// parameters for container registry name and storage account name
@description('Container registry name for the docker images')
param containerRegistryName string
param imageName string

@description('The storage account where media files are stored')
param storageAccountName string
param inputContainerName string
param outputContainerName string

@description('managed identity for the container app')
param appManagedIdentityName string

@description('app insights for the container app')
param applicationInsightName string

@description('log analytics workspace for the container app')
param logAnalyticsWorkspaceName string

@description('environment for the container app')
param containerAppsEnvironmentName string

@description('name of the container app')
param containerAppName string

// create a managed identity.
resource appManagedIdentity 'Microsoft.ManagedIdentity/userAssignedIdentities@2023-01-31' = {
  name: appManagedIdentityName
  location: resourceGroup().location
}

// get resource for existing storage account
resource storageAccount 'Microsoft.Storage/storageAccounts@2021-06-01' existing = {
  name: storageAccountName
}

// role defintion for blob data conttributor
var blobRoleDefinitionResourceID = resourceId('Microsoft.Authorization/roleDefinitions', 'ba92f5b4-2d11-453d-a403-e96b0029c9fe')

// give blob data contributor access to the managed identity
resource blobRoleAssignment 'Microsoft.Authorization/roleAssignments@2022-04-01' = {
  scope: storageAccount
  name: guid(storageAccount.id, appManagedIdentity.id, blobRoleDefinitionResourceID)
  properties: {
    roleDefinitionId: blobRoleDefinitionResourceID
    principalId: appManagedIdentity.properties.principalId
    principalType: 'servicePrincipal'
  }
}

// Get the container registry for uploading the docker images.
resource containerRegistry 'Microsoft.ContainerRegistry/registries@2023-07-01' existing = {
  name: containerRegistryName
}

// get the role definition ID for the ACR pull role
var acrRoleDefinitionResourceID = resourceId('Microsoft.Authorization/roleDefinitions', '7f951dda-4ed3-4680-a7ca-43fe172d538d')

// give reader access on the container registry to the managed identity.
resource acrRoleAssignment 'Microsoft.Authorization/roleAssignments@2022-04-01' = {
  scope: containerRegistry
  name: guid(containerRegistry.id, appManagedIdentity.id, acrRoleDefinitionResourceID)
  properties: {
    roleDefinitionId: acrRoleDefinitionResourceID
    principalId: appManagedIdentity.properties.principalId
    principalType: 'servicePrincipal'
  }
}

@description('Name of the code signing account')
param codeSigningAccountName string

@description('certificate profile in the signing account')
param certificateProfileName string

resource codeSigningAccount 'Microsoft.CodeSigning/codeSigningAccounts@2024-09-30-preview' existing = {
  name: codeSigningAccountName
}

// role definition for Trusted Signing Certificate Profile Signer
var codeSigningRoleDefinitionResourceID = resourceId('Microsoft.Authorization/roleDefinitions', '2837e146-70d7-4cfd-ad55-7efa6464f958')
resource codeSigningRoleAssignment 'Microsoft.Authorization/roleAssignments@2022-04-01' = {
  scope: codeSigningAccount
  name: guid(codeSigningAccount.id, appManagedIdentity.id, codeSigningRoleDefinitionResourceID)
  properties: {
    roleDefinitionId: codeSigningRoleDefinitionResourceID
    principalId: appManagedIdentity.properties.principalId
    principalType: 'servicePrincipal'
  }
}


resource logAnalyticsWorkspace 'Microsoft.OperationalInsights/workspaces@2021-06-01' = {
  name: logAnalyticsWorkspaceName
  location: resourceGroup().location
  properties: {
    sku: {
      name: 'PerGB2018'
    }
    retentionInDays: 30
  }
}

resource applicationInsights 'Microsoft.Insights/components@2020-02-02' = {
  name: applicationInsightName
  location: resourceGroup().location
  kind: 'other'
  properties: {
    Application_Type: 'other'
    WorkspaceResourceId: logAnalyticsWorkspace.id
  }
}

resource containerAppsEnvironment 'Microsoft.App/managedEnvironments@2022-10-01' = {
  name: containerAppsEnvironmentName
  location: resourceGroup().location
  sku: {
    name: 'Consumption'
  }
  properties: {
    daprAIInstrumentationKey: applicationInsights.properties.InstrumentationKey
    appLogsConfiguration: {
      destination: 'log-analytics'
      logAnalyticsConfiguration: {
        customerId: logAnalyticsWorkspace.properties.customerId
        sharedKey:  logAnalyticsWorkspace.listKeys().primarySharedKey
      }
    }
  }
}

// create a container app with keda trigger
resource containerapp 'Microsoft.App/containerApps@2024-10-02-preview' = {
  name: containerAppName
  location: resourceGroup().location
  identity: {
    type: 'UserAssigned'
    userAssignedIdentities: {
      '${appManagedIdentity.id}': {}
    }
  }
  properties: {
    managedEnvironmentId: containerAppsEnvironment.id
    configuration: {
      registries: [
        {
          identity: appManagedIdentity.id
          server: containerRegistry.properties.loginServer
        }
      ]
    }
    template: {
      containers: [
        {
          name: containerAppName
          image: '${containerRegistryName}.azurecr.io/${imageName}:latest'
          imageType: 'ContainerImage'
          resources: {
            cpu: 1
            memory: '2Gi'
          }
          env: [
            {
              name: 'INPUT_CONTAINER'
              value: inputContainerName
            }
            {
              name: 'OUTPUT_CONTAINER'
              value: outputContainerName
            }
            {
              name: 'STORAGE_ACCOUNT'
              value: storageAccountName
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
            {
              name: 'IDENTITY_CLIENT_ID'
              value: appManagedIdentity.properties.clientId
            }
            /* uncomment for tracing.
            {
              name: 'RUST_LOG'
              value: 'info'
            }
            {
              name: 'RUST_BACKTRACE'
              value: 'full'
            }
            */
          ]
        }
      ]
      scale: {
        minReplicas: 0
        maxReplicas: 5
        pollingInterval: 60
        rules: [
          {
            name: 'storage-blob-scaling'
            custom: {
              type: 'azure-blob'
              metadata: {
                blobStorageAccount: storageAccount.name
                blobContainer: inputContainerName
              }
            }
          }
        ]
      }
    }
  }
}
