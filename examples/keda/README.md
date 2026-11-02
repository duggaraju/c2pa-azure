# KEDA worker

`keda-worker` adds content credentials to files dropped into cloud storage. A
run signs the pending files it can claim, writes them to the output under the
same name, deletes the inputs it signed and exits. Files that fail stay in the
input for a later run. [KEDA](https://keda.sh) starts runs while there is
work.

The cloud is picked at build time with a feature flag:

| Feature | Input | Claim | KEDA scaler |
|---------|-------|-------|-------------|
| `azure` (default) | Blob Storage container (`STORAGE_ACCOUNT`, `INPUT_CONTAINER`, `OUTPUT_CONTAINER`) | 60 second blob lease | `azure-blob` |
| `aws` | S3 notifications on SQS (`QUEUE_URL`, `OUTPUT_BUCKET`) | Message visibility timeout (`VISIBILITY_TIMEOUT`, default 300 seconds) | `aws-sqs-queue` |
| `gcp` | Cloud Storage bucket (`INPUT_BUCKET`, `OUTPUT_BUCKET`) | None, so run one worker at a time | `gcp-storage` |

The signer settings are listed in each cloud's deploy README.
`MANIFEST_DEFINITION` (a file path or inline JSON) replaces the default
manifest and `RUST_LOG` sets the log level.

```bash
docker build -f examples/Dockerfile --build-arg CLOUD=azure --target keda-worker -t <image> .
```

## Kubernetes

[k8s](k8s) is a cloud-agnostic [Kustomize](https://kustomize.io) base. It
runs the worker as a KEDA `ScaledJob`, one Kubernetes Job per batch.

| File | Purpose |
|------|---------|
| [scaledjob.yaml](k8s/scaledjob.yaml) | `ScaledJob` running the `keda-worker` container with the `c2pa-worker` ConfigMap as its environment and the optional `c2pa-certificate-chain` secret at `/etc/c2pa` |
| [serviceaccount.yaml](k8s/serviceaccount.yaml) | `c2pa-worker` service account for the jobs |
| [triggerauthentication.yaml](k8s/triggerauthentication.yaml) | How KEDA authenticates to the input |
| [kustomizeconfig.yaml](k8s/kustomizeconfig.yaml) | Name references in the KEDA resources, so name prefixes and suffixes work |

The base has no trigger, identity or image. Each cloud's overlay adds them:

| Overlay | Cluster | Workload identity | Scaler |
|---------|---------|-------------------|--------|
| [azure/deploy/keda](../../azure/deploy/keda) | AKS with the KEDA add-on | Microsoft Entra Workload ID | `azure-blob` |
| [aws/deploy/keda](../../aws/deploy/keda) | EKS with KEDA | IAM roles for service accounts | `aws-sqs-queue` |
| [gcp/deploy/keda](../../gcp/deploy/keda) | GKE with KEDA | Workload Identity Federation for GKE | `gcp-storage` |

An overlay has three files:

- `worker.env` holds the settings and becomes the worker's environment.
  Replacements in `kustomization.yaml` copy values such as the storage account,
  queue URL or identity into the scaler and the service account, so this is the
  only file with resource names.
- `patch.yaml` sets the identity provider and the KEDA trigger.
- `kustomization.yaml` sets the image and the namespace.

```bash
kubectl kustomize azure/deploy/keda   # preview
kubectl apply -k azure/deploy/keda
```

For another cluster, create an overlay with `resources: [<path to examples/keda/k8s>]`
and a patch with the trigger.
