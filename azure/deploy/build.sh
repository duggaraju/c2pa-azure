#!/bin/bash
# Builds an Azure example image from the repository root and pushes it to ACR.
#   ./build.sh -n <registry> [-t keda-worker|azure-function] [-i <image name>]
set -euo pipefail

OPTSTRING="n:i:t:"
registry_name=""
image_name=""
target="keda-worker"
while getopts ${OPTSTRING} opt; do
  case ${opt} in
    n)
      registry_name=$OPTARG
      ;;
    i)
      image_name=$OPTARG
      ;;
    t)
      target=$OPTARG
      ;;
    ?)
      echo "Invalid option: -${OPTARG}."
      exit 1
      ;;
  esac
done

if [ -z "$registry_name" ]; then
  echo "Registry name is required"
  exit 1
fi

case "$target" in
  keda-worker) image_name=${image_name:-c2pa-acs-app} ;;
  azure-function) image_name=${image_name:-c2pa-function} ;;
  *)
    echo "Unknown target $target (expected keda-worker or azure-function)"
    exit 1
    ;;
esac

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
image="$registry_name.azurecr.io/$image_name"
echo "Building $target as $image"

az acr login -n "$registry_name"
docker build -f "$repo_root/examples/Dockerfile" --build-arg CLOUD=azure --target "$target" -t "$image" "$repo_root"
docker push "$image"
