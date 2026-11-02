# Azure deployment

Signing uses [Azure Trusted Signing](https://learn.microsoft.com/azure/trusted-signing/)
through the `c2pa-azure` crate. Images are built from the repository root with
[examples/Dockerfile](../../examples/Dockerfile) and `CLOUD=azure`.

| File | Purpose |
|------|---------|
| [common.bicep](common.bicep) | Storage account (input/output containers), container registry, Trusted Signing account |
| [aks.bicep](aks.bicep) | Worker identity, federated credentials and roles for `keda-worker` on an existing AKS cluster |
| [keda/](keda) | Kustomize overlay running `keda-worker` on AKS (base: [examples/keda/k8s](../../examples/keda/k8s)) |
| [container-app.bicep](container-app.bicep) | `keda-worker` as a Container App scaled by the KEDA `azure-blob` scaler, without a Kubernetes cluster |
| [function.bicep](function.bicep) | `http-service` as an Azure Functions custom handler (Elastic Premium, container) |
| [function/](function) | Function app root copied into the `azure-function` image (`host.json` + `sign`/`verify` HTTP triggers) |
| [build.sh](build.sh) | Builds and pushes an image to ACR |

```bash
az deployment group create -g <rg> --parameters common.bicepparam
./build.sh -n <registry> -t keda-worker
az deployment group create -g <rg> --parameters container-app.bicepparam

./build.sh -n <registry> -t azure-function
az deployment group create -g <rg> --parameters function.bicepparam
```

The managed identity's client id is passed to the apps as `AZURE_CLIENT_ID`.
The function endpoints are `POST https://<app>.azurewebsites.net/api/sign` and
`/api/verify` and require a function key (`x-functions-key` header).

To run the function host locally, build `http-service` and copy it into
`function/`, then run `func start` from that folder with `SIGNING_ENDPOINT`,
`SIGNING_ACCOUNT` and `CERTIFICATE_PROFILE` set.

## AKS

The worker runs as a KEDA `ScaledJob` on a cluster with the OIDC issuer,
workload identity and the KEDA add-on. One user-assigned identity is federated
with the worker's service account, which signs and moves the blobs, and with the
`keda-operator` service account, which counts the input blobs. That identity
has the Storage Blob Data Contributor and Trusted Signing Certificate Profile
Signer roles.

1. Prepare the cluster and create the identity (set the names in
   [aks.bicepparam](aks.bicepparam)):

   ```bash
   az aks update -g <rg> -n <cluster> --enable-oidc-issuer --enable-workload-identity --enable-keda
   az aks update -g <rg> -n <cluster> --attach-acr <registry>
   az deployment group create -g <rg> --parameters aks.bicepparam \
     --query properties.outputs
   ```

2. Set `AZURE_CLIENT_ID` and `SIGNING_ENDPOINT` in [keda/worker.env](keda/worker.env)
   from the `workerClientId` and `signingEndpoint` outputs, check the other
   settings, and set the image in [keda/kustomization.yaml](keda/kustomization.yaml).

3. Build the image and deploy:

   ```bash
   ./build.sh -n <registry> -t keda-worker
   az aks get-credentials -g <rg> -n <cluster>
   # Once, if workload identity was enabled after KEDA, so the operator gets a token.
   kubectl rollout restart deploy keda-operator -n kube-system
   kubectl apply -k keda
   ```

The TriggerAuthentication passes the identity to KEDA as `identityId`. The
`keda-operator` service account keeps its own default identity, so it isn't
annotated. The namespace in [keda/kustomization.yaml](keda/kustomization.yaml)
must match `serviceAccountNamespace` in `aks.bicepparam`.
