#!/usr/bin/env bash
# Describe one packslip release as packslip.dev and publish the result:
# create and sign the bundle for the files in dist/, verify it under this
# repository's identity, and copy the files and the bundle to the download
# host. Run by release.yml for the release being cut, and by its backfill
# job for a release that shipped before packslip.dev served its own.
#
# Environment:
#   TAG           the release tag, v1.2.3
#   COMMIT        the commit the release was built from
#   PUBLISHED_AT  RFC 3339 publication time; defaults to now
#   GITHUB_REPOSITORY, GITHUB_API_URL  as the runner sets them
#   AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY  for the R2 bucket
# Expects `packslip` on PATH and the release files in dist/.
set -euo pipefail

host=packslip.dev
bucket=jdx-releases
prefix=packslip
immutable="public, max-age=31536000, immutable"

export AWS_REGION=auto
export AWS_ENDPOINT_URL=https://6e243906ff257b965bcae8025c2fc344.r2.cloudflarestorage.com

version="${TAG#v}"
args=()
if [ -n "${PUBLISHED_AT:-}" ]; then
  args+=(--published-at "$PUBLISHED_AT")
fi
# Every file was attested when it was built; GitHub serves that provenance
# by digest, so the link is the same whichever run made the file.
for f in dist/*; do
  digest=$(sha256sum "$f" | cut -d' ' -f1)
  args+=(--provenance "${f##*/}=${GITHUB_API_URL}/repos/${GITHUB_REPOSITORY}/attestations/sha256:${digest}")
done
# Older releases being backfilled may have only the CLI specification.
completion_artifacts=()
for shell in bash zsh fish powershell; do
  if [ -f "dist/packslip.$shell" ]; then
    args+=(--resource "completion/$shell=asset:dist/packslip.$shell")
    completion_artifacts+=(--artifact "dist/packslip.$shell")
  fi
done
packslip create \
  --project "$host" \
  --version "$version" \
  --out packslip \
  --url-base "https://${host}/${TAG}" \
  --notes-url "https://github.com/${GITHUB_REPOSITORY}/releases/tag/${TAG}" \
  --source-repo "https://github.com/${GITHUB_REPOSITORY}" \
  --tag "$TAG" \
  --commit "$COMMIT" \
  --bin packslip \
  --resource cli-spec/usage=asset:dist/packslip.usage.kdl \
  "${args[@]}" \
  dist/*
packslip verify packslip/packslip.sigstore.json \
  --identity-prefix "https://github.com/${GITHUB_REPOSITORY}/" \
  --issuer https://token.actions.githubusercontent.com \
  --artifact "dist/packslip-v${version}-linux-x64.tar.xz" \
  --artifact dist/packslip.usage.kdl \
  "${completion_artifacts[@]}"

# The files first and the bundle last: a release is only discoverable once
# its list names the bundle, and the bundle only lands when everything it
# describes is already in place.
aws s3 cp dist/ "s3://${bucket}/${prefix}/${TAG}/" --recursive --cache-control "$immutable"
aws s3 cp packslip/packslip.sigstore.json "s3://${bucket}/${prefix}/${TAG}/packslip.sigstore.json" \
  --content-type application/json --cache-control "$immutable"
