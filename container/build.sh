#!/usr/bin/env bash
# Build packslip's container image (container/Dockerfile) from the static
# Linux executables of one release, for linux/amd64 and linux/arm64, and
# hand the result to `docker buildx build` as the remaining arguments say:
# release.yml pushes it, and CI writes it to a file and loads one platform.
#
# Usage: container/build.sh VERSION AMD64 ARM64 [BUILDX ARGS...]
#   VERSION  the release version, 1.2.3
#   AMD64    the linux-x64 executable
#   ARM64    the linux-arm64 executable
# PLATFORMS overrides linux/amd64,linux/arm64, such as to --load one, and
# CA_BUNDLE the CA bundle to include. Building for both platforms at once
# needs a builder that can, such as one from
# `docker buildx create --use --driver docker-container`.
set -euo pipefail

if [ "$#" -lt 3 ]; then
  echo "usage: $0 VERSION AMD64 ARM64 [BUILDX ARGS...]" >&2
  exit 2
fi
version=$1
amd64=$2
arm64=$3
shift 3
here=$(cd "$(dirname "$0")" && pwd)

if ! [[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "not a release version: $version" >&2
  exit 1
fi
# Debian and Ubuntu, including the GitHub runners, keep the bundle here.
certs=${CA_BUNDLE:-/etc/ssl/certs/ca-certificates.crt}
if [ ! -s "$certs" ]; then
  echo "no CA bundle at $certs; set CA_BUNDLE" >&2
  exit 1
fi

context=$(mktemp -d)
trap 'rm -rf "$context"' EXIT
cp "$here/Dockerfile" "$context/Dockerfile"
cp "$certs" "$context/ca-certificates.crt"
cp "$amd64" "$context/packslip-amd64"
cp "$arm64" "$context/packslip-arm64"
chmod 755 "$context/packslip-amd64" "$context/packslip-arm64"

platforms=${PLATFORMS:-linux/amd64,linux/arm64}
revision=${GITHUB_SHA:-$(git -C "$here" rev-parse HEAD)}
args=()
for pair in \
  "title=packslip" \
  "description=The packslip CLI: verify signed release manifests and the artifacts they describe" \
  "source=https://github.com/jdx/packslip" \
  "url=https://packslip.dev" \
  "documentation=https://packslip.dev/docs/" \
  "licenses=MIT" \
  "version=$version" \
  "revision=$revision"; do
  # A label on each platform's image, which GHCR reads to link the package
  # to the repository, and the same annotation on the index it shows. A
  # build for one platform has no index.
  args+=(--label "org.opencontainers.image.$pair")
  if [[ $platforms == *,* ]]; then
    args+=(--annotation "index:org.opencontainers.image.$pair")
  fi
done

# buildx's own provenance and SBOM attestations would add unknown/unknown
# entries to the index; release.yml attests the image with GitHub instead.
docker buildx build \
  --platform "$platforms" \
  --provenance=false \
  --sbom=false \
  "${args[@]}" \
  "$@" \
  "$context"
