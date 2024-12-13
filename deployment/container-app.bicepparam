using './container-app.bicep'

// container registry parameters
param containerRegistryName = 'c2paacsregistry'

// storage account parameters
param storageAccountName = 'c2paacsmediastorage'
param inputContainerName = 'input'
param outputContainerName = 'output'

// code signing account parameters
param codeSigningAccountName = 'c2paacscodesigning'
param certificateProfileName = 'media-provenance-sign'

// managed identity name.
param appManagedIdentityName = 'c2paacsidentity'

// container app paarmeters
param applicationInsightName = 'c2paacsappinsights'
param logAnalyticsWorkspaceName = 'c2paacsloganalytics'
param containerAppsEnvironmentName = 'c2paacsenvironment'
param containerAppName = 'c2paacsapp'
