// parameters for container registry name and storage account name
param containerRegistryName string
param storageAccountName string

// create a managed identity.
resource functionManagedIdentity 'Microsoft.ManagedIdentity/userAssignedIdentities@2023-01-31' = {
  name: 'functionManagedIdentity'
  location: resourceGroup().location
}

// get resource for existing storage account
resource storageAccount 'Microsoft.Storage/storageAccounts@2021-06-01' existing = {
  name: storageAccountName
}

// Get the container registry for uploading the docker images.
resource containerRegistry 'Microsoft.ContainerRegistry/registries@2023-07-01' existing = {
  name: containerRegistryName
}

// get the role definition ID for the ACR pull role
var roleDefinitionResourceID = resourceId('Microsoft.Authorization/roleDefinitions', '7f951dda-4ed3-4680-a7ca-43fe172d538d')

// give reader access on the container registry to the managed identity.
resource roleAssignment 'Microsoft.Authorization/roleAssignments@2022-04-01' = {
  scope: containerRegistry
  name: guid(containerRegistry.id, functionManagedIdentity.id, roleDefinitionResourceID)
  properties: {
    roleDefinitionId: roleDefinitionResourceID
    principalId: functionManagedIdentity.properties.principalId
    principalType: 'servicePrincipal'
  }
}

// create a container based azure function for custom runtime
resource functionApp 'Microsoft.Web/sites@2024-04-01' = {
  name: 'functionApp'
  location: resourceGroup().location
  kind: 'functionApp'
  identity: {
    type: 'UserAssigned'
    userAssignedIdentities: {
      functionManagedIdentity: functionManagedIdentity
    }
  }
  properties: {
    serverFarmId: '/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Web/serverfarms/plan'
    siteConfig: {
      appSettings: [
        {
          name: 'FUNCTIONS_WORKER_RUNTIME'
          value: 'custom'
        }
        {
          name: 'FUNCTIONS_CUSTOM_HANDLER'
          value: 'function'
        }
        {
          name: 'WEBSITES_ENABLE_APP_SERVICE_STORAGE'
          value: 'false'
        }
        {
          name: 'WEBSITES_PORT'
          value: '80'
        }
      ]
    }
  }
  dependsOn: [
    functionManagedIdentity
    storageAccount
    containerRegistry
  ]
}
