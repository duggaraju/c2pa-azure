# Builds the example binaries for one cloud. Run from the repository root:
#
#   docker build -f examples/Dockerfile --build-arg CLOUD=<azure|aws|gcp> --target <target> .
#
# Targets:
#   http-service    signing service for any container host (Google Cloud Run, ...)
#   aws-lambda      http-service with the AWS Lambda Web Adapter
#   azure-function  http-service as an Azure Functions custom handler
#   keda-worker     storage worker scaled by KEDA

ARG CLOUD=azure

FROM rust:1-bookworm AS builder
ARG CLOUD
WORKDIR /usr/src/app
COPY . .
RUN cargo build --release --locked -p http-service -p keda-worker \
    --no-default-features --features "${CLOUD}"

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

FROM runtime AS http-service
COPY --from=builder /usr/src/app/target/release/http-service /usr/local/bin/
ENV PORT=8080
EXPOSE 8080
ENTRYPOINT ["http-service"]

FROM http-service AS aws-lambda
COPY --from=public.ecr.aws/awsguru/aws-lambda-adapter:1.1.0 /lambda-adapter /opt/extensions/lambda-adapter
ENV AWS_LWA_READINESS_CHECK_PATH=/healthz

# The Functions host image is Debian bookworm, matching the builder's glibc.
FROM mcr.microsoft.com/azure-functions/dotnet:4 AS azure-function
ENV AzureWebJobsScriptRoot=/home/site/wwwroot \
    AzureFunctionsJobHost__Logging__Console__IsEnabled=true \
    FUNCTIONS_WORKER_RUNTIME=custom
COPY azure/deploy/function/ /home/site/wwwroot/
COPY --from=builder /usr/src/app/target/release/http-service /home/site/wwwroot/

FROM runtime AS keda-worker
COPY --from=builder /usr/src/app/target/release/keda-worker /usr/local/bin/
ENTRYPOINT ["keda-worker"]
