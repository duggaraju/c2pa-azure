using './aks.bicep'

// existing AKS cluster (in this resource group unless aksResourceGroupName is set)
param aksClusterName = 'c2paacsaks'

// identity created for the worker and the KEDA scaler
param workerIdentityName = 'c2paacsworkeridentity'

// resources created by common.bicep
param storageAccountName = 'c2paacsmediastorage'
param codeSigningAccountName = 'c2paacscodesigning'
