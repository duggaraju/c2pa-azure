# AWS deployment

Signing uses an asymmetric [AWS KMS](https://docs.aws.amazon.com/kms/latest/developerguide/symmetric-asymmetric.html)
key (`SIGN_VERIFY`, RSA or ECC NIST) with a certificate chain you obtain for it
from a C2PA-trusted CA. Images are built from the repository root with
[examples/Dockerfile](../../examples/Dockerfile) and `CLOUD=aws`.

| File | Purpose |
|------|---------|
| [lambda/template.yaml](lambda/template.yaml) | SAM template running `http-service` on Lambda with the [Lambda Web Adapter](https://github.com/awslabs/aws-lambda-web-adapter) and a function URL |
| [keda/](keda) | Kustomize overlay running `keda-worker` on EKS, scaled by the KEDA `aws-sqs-queue` scaler (base: [examples/keda/k8s](../../examples/keda/k8s)) |

## Environment

| Variable | Description |
|----------|-------------|
| `KMS_KEY_ID` | Key id, ARN or alias |
| `CERTIFICATE_CHAIN` | PEM chain (inline or a file path) |
| `ALGORITHM` | `ps256`/`ps384`/`ps512` for RSA keys, `es256`/`es384`/`es512` for ECC keys (default `ps384`) |
| `TIME_AUTHORITY_URL` | RFC 3161 timestamp authority |

## Lambda

```bash
cd aws/deploy/lambda
sam build
sam deploy --guided
```

Lambda limits request and response payloads to 6 MB, so large media should
use the KEDA worker instead.

## KEDA worker

The worker reads S3 event notifications from SQS:

1. Create an input bucket, a different output bucket and an SQS queue (with a
   dead-letter queue for files that fail repeatedly).
2. Send the input bucket's `s3:ObjectCreated:*` notifications to the queue.
3. Create an IAM role for the worker (IRSA) that trusts the
   `system:serviceaccount:default:c2pa-worker` service account through the
   cluster's OIDC provider. Allow `sqs:ReceiveMessage`, `sqs:DeleteMessage`,
   `sqs:ChangeMessageVisibility` and `sqs:GetQueueAttributes` on the queue,
   `s3:GetObject` and `s3:DeleteObject` on the input bucket, `s3:PutObject` on
   the output bucket and `kms:Sign` on the key.
4. Install KEDA with IRSA (`--set podIdentity.aws.irsa.enabled=true
   --set podIdentity.aws.irsa.roleArn=<keda-role-arn>`), where
   `<keda-role-arn>` is an IRSA role for `system:serviceaccount:keda:keda-operator`
   that needs no policies. The TriggerAuthentication uses
   `identityOwner: workload`, so KEDA uses its token to assume the worker role
   (AssumeRoleWithWebIdentity) and read the queue. Add
   `system:serviceaccount:keda:keda-operator` to the worker role's trust policy
   (see [KEDA AWS authentication](https://keda.sh/docs/latest/authentication-providers/aws/)).
5. Build and push the image:

   ```bash
   docker build -f examples/Dockerfile --build-arg CLOUD=aws --target keda-worker \
     -t <account-id>.dkr.ecr.<region>.amazonaws.com/c2pa-keda-worker .
   ```

6. Fill in [keda/worker.env](keda/worker.env) and the image in
   [keda/kustomization.yaml](keda/kustomization.yaml), then deploy:

   ```bash
   kubectl create secret generic c2pa-certificate-chain --from-file=chain.pem
   kubectl apply -k aws/deploy/keda
   ```

Each job receives up to 10 messages. Signed files are written to
`OUTPUT_BUCKET` under the same key and the input object is deleted. Set
`VISIBILITY_TIMEOUT` (default 300 seconds) in `worker.env` above the time
needed to sign 10 files.
