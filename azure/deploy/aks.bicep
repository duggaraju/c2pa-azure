// Identity and permissions for running keda-worker on an existing AKS cluster
// (see keda/ for the Kubernetes manifests). The cluster needs the OIDC issuer,
// workload identity and the KEDA add-on:
//   az aks update -g <rg> -n <cluster> --enable-oidc-issuer --enable-workload-identity --enable-keda
//   az aks update -g <rg> -n <cluster> --attach-acr <registry>

@description('Existing AKS cluster')
param aksClusterName string

@description('Resource group of the AKS cluster')
param aksResourceGroupName string = resourceGroup().name

@description('User-assigned identity the worker and the KEDA scaler authenticate as')
param workerIdentityName string

@description('The storage account where media files are stored')
param storageAccountName string

@description('Name of the code signing account')
param codeSigningAccountName string

@description('Namespace of the worker service account (namespace in keda/kustomization.yaml)')
param serviceAccountNamespace string = 'default'

@description('Name of the worker service account')
param serviceAccountName string = 'c2pa-worker'

@description('Namespace of the KEDA operator (kube-system for the AKS add-on)')
param kedaOperatorNamespace string = 'kube-system'

@description('Service account of the KEDA operator')
param kedaOperatorServiceAccountName string = 'keda-operator'

resource aks 'Microsoft.ContainerService/managedClusters@2024-09-01' existing = {
  name: aksClusterName
  scope: resourceGroup(aksResourceGroupName)
}

resource workerIdentity 'Microsoft.ManagedIdentity/userAssignedIdentities@2023-01-31' = {
  name: workerIdentityName
  location: resourceGroup().location
}

// Lets the worker pods authenticate as the identity.
resource workerCredential 'Microsoft.ManagedIdentity/userAssignedIdentities/federatedIdentityCredentials@2023-01-31' = {
  parent: workerIdentity
  name: 'c2pa-worker'
  properties: {
    issuer: aks.properties.oidcIssuerProfile.issuerURL
    subject: 'system:serviceaccount:${serviceAccountNamespace}:${serviceAccountName}'
    audiences: [
      'api://AzureADTokenExchange'
    ]
  }
}

// Lets the KEDA operator authenticate as the identity to count the input blobs
// (identityId in the TriggerAuthentication).
resource kedaCredential 'Microsoft.ManagedIdentity/userAssignedIdentities/federatedIdentityCredentials@2023-01-31' = {
  parent: workerIdentity
  name: 'keda-operator'
  properties: {
    issuer: aks.properties.oidcIssuerProfile.issuerURL
    subject: 'system:serviceaccount:${kedaOperatorNamespace}:${kedaOperatorServiceAccountName}'
    audiences: [
      'api://AzureADTokenExchange'
    ]
  }
  // Federated credentials of one identity can't be created concurrently.
  dependsOn: [
    workerCredential
  ]
}

resource storageAccount 'Microsoft.Storage/storageAccounts@2021-06-01' existing = {
  name: storageAccountName
}

// Storage Blob Data Contributor
var blobRoleDefinitionResourceID = resourceId('Microsoft.Authorization/roleDefinitions', 'ba92f5b4-2d11-453d-a403-e96b0029c9fe')
resource blobRoleAssignment 'Microsoft.Authorization/roleAssignments@2022-04-01' = {
  scope: storageAccount
  name: guid(storageAccount.id, workerIdentity.id, blobRoleDefinitionResourceID)
  properties: {
    roleDefinitionId: blobRoleDefinitionResourceID
    principalId: workerIdentity.properties.principalId
    principalType: 'ServicePrincipal'
  }
}

resource codeSigningAccount 'Microsoft.CodeSigning/codeSigningAccounts@2024-09-30-preview' existing = {
  name: codeSigningAccountName
}

// Trusted Signing Certificate Profile Signer
var codeSigningRoleDefinitionResourceID = resourceId('Microsoft.Authorization/roleDefinitions', '2837e146-70d7-4cfd-ad55-7efa6464f958')
resource codeSigningRoleAssignment 'Microsoft.Authorization/roleAssignments@2022-04-01' = {
  scope: codeSigningAccount
  name: guid(codeSigningAccount.id, workerIdentity.id, codeSigningRoleDefinitionResourceID)
  properties: {
    roleDefinitionId: codeSigningRoleDefinitionResourceID
    principalId: workerIdentity.properties.principalId
    principalType: 'ServicePrincipal'
  }
}

@description('AZURE_CLIENT_ID in keda/worker.env')
output workerClientId string = workerIdentity.properties.clientId

@description('SIGNING_ENDPOINT in keda/worker.env')
output signingEndpoint string = codeSigningAccount.properties.accountUri
