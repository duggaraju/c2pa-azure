# Google Cloud deployment

Signing uses an asymmetric [Cloud KMS](https://cloud.google.com/kms/docs/create-validate-signatures)
key (`ASYMMETRIC_SIGN`, RSA PSS or EC) with a certificate chain you obtain for
it from a C2PA-trusted CA. The signing algorithm is read from the key version.
Images are built from the repository root with
[examples/Dockerfile](../../examples/Dockerfile) and `CLOUD=gcp`.

| File | Purpose |
|------|---------|
| [cloud-run/service.yaml](cloud-run/service.yaml) | `http-service` on Cloud Run, with the chain mounted from Secret Manager |
| [keda/](keda) | Kustomize overlay running `keda-worker` on GKE, scaled by the KEDA `gcp-storage` scaler (base: [examples/keda/k8s](../../examples/keda/k8s)) |

## Environment

| Variable | Description |
|----------|-------------|
| `KMS_KEY_NAME` | `projects/*/locations/*/keyRings/*/cryptoKeys/*/cryptoKeyVersions/*` |
| `CERTIFICATE_CHAIN` | PEM chain (inline or a file path) |
| `TIME_AUTHORITY_URL` | RFC 3161 timestamp authority |

## Cloud Run

```bash
gcloud secrets create c2pa-certificate-chain --data-file=chain.pem
docker build -f examples/Dockerfile --build-arg CLOUD=gcp --target http-service \
  -t <region>-docker.pkg.dev/<project-id>/<repository>/c2pa-http-service .
docker push <region>-docker.pkg.dev/<project-id>/<repository>/c2pa-http-service
gcloud run services replace gcp/deploy/cloud-run/service.yaml --region <region>
```

Cloud Run limits HTTP/1 request bodies to 32 MiB.

## KEDA worker

The worker signs every object in `INPUT_BUCKET`, writes it to `OUTPUT_BUCKET`
under the same name and deletes the input (only if it has not been overwritten
meanwhile). Cloud Storage has no leases, so the job runs one worker at a time.

1. On a cluster with Workload Identity Federation for GKE and KEDA, grant IAM
   roles directly to the Kubernetes service accounts (the buckets need uniform
   bucket-level access), where `PRINCIPAL` is
   `principal://iam.googleapis.com/projects/<project-number>/locations/global/workloadIdentityPools/<project-id>.svc.id.goog/subject`:
   - `PRINCIPAL/ns/default/sa/c2pa-worker`: `roles/storage.objectUser` on both
     buckets and `roles/cloudkms.signerVerifier` on the key.
   - `PRINCIPAL/ns/keda/sa/keda-operator`: `roles/storage.objectViewer` on the
     input bucket, so KEDA can count the objects.
2. Build and push the `keda-worker` target with `CLOUD=gcp`.
3. Fill in [keda/worker.env](keda/worker.env) and the image in
   [keda/kustomization.yaml](keda/kustomization.yaml), then deploy:

   ```bash
   kubectl create secret generic c2pa-certificate-chain --from-file=chain.pem
   kubectl apply -k gcp/deploy/keda
   ```

To impersonate a Google service account instead, uncomment the
`iam.gke.io/gcp-service-account` annotation in [keda/patch.yaml](keda/patch.yaml).
