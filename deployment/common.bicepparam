using  './common.bicep'

param containerRegistryName = 'c2paacsregistry'
param storageAccountName = 'c2paacsmediastorage'
param inputContainerName = 'input'
param outputContainerName = 'output'
param codeSigningAccountName = 'c2paacscodesigning'
param certificateProfileName = 'media-provenance-sign'
