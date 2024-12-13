#!/bin/bash
# getopt to get the registry name
OPTSTRING="n:"

while getopts ${OPTSTRING} opt; do
  case ${opt} in
    n)
      echo "registry name is $OPTARG"
      registry_name=$OPTARG
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

az acr login -n "$registry_name"
docker build --target keda-blob-storage -t "$registry_name.azurecr.io/keda-blob-storage" ..
docker push "$registry_name.azurecr.io/keda-blob-storage"
 