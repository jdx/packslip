#!/usr/bin/env bash
set -euo pipefail
repo=$(realpath "${1:?usage: publish-package-repositories.sh REPOSITORY_DIRECTORY}")
export AWS_REGION=auto
export AWS_ENDPOINT_URL=https://6e243906ff257b965bcae8025c2fc344.r2.cloudflarestorage.com
destination=s3://jdx-releases/packslip
immutable='public, max-age=31536000, immutable'
mutable='public, max-age=300'
# Retain previous content-addressed files: older authenticated indices must
# remain usable during concurrent publication and CDN cache transitions.
aws s3 cp "$repo/apt/pool/" "$destination/apt/pool/" --recursive --cache-control "$immutable"
for arch in amd64 arm64; do
  index="apt/dists/stable/main/binary-$arch"
  aws s3 cp "$repo/$index/by-hash/" "$destination/$index/by-hash/" --recursive --cache-control "$immutable"
done
for arch in x86_64 aarch64; do
  aws s3 cp "$repo/rpm/$arch/packages/" "$destination/rpm/$arch/packages/" --recursive --cache-control "$immutable"
  aws s3 cp "$repo/rpm/$arch/repodata/" "$destination/rpm/$arch/repodata/" --recursive \
    --exclude 'repomd.xml*' --cache-control "$immutable"
done
aws s3 cp "$repo/gpg-key.pub" "$destination/gpg-key.pub" --cache-control "$mutable"
aws s3 cp "$repo/rpm/packslip.repo" "$destination/rpm/packslip.repo" --cache-control "$mutable"
for arch in amd64 arm64; do
  index="apt/dists/stable/main/binary-$arch"
  aws s3 cp "$repo/$index/Packages" "$destination/$index/Packages" --cache-control "$mutable"
  aws s3 cp "$repo/$index/Packages.gz" "$destination/$index/Packages.gz" --cache-control "$mutable"
done
for arch in x86_64 aarch64; do
  index="rpm/$arch/repodata/repomd.xml"
  aws s3 cp "$repo/$index.asc" "$destination/$index.asc" --cache-control "$mutable"
  aws s3 cp "$repo/$index" "$destination/$index" --cache-control "$mutable"
done
index=apt/dists/stable
aws s3 cp "$repo/$index/Release.gpg" "$destination/$index/Release.gpg" --cache-control "$mutable"
aws s3 cp "$repo/$index/Release" "$destination/$index/Release" --cache-control "$mutable"
# Modern APT verifies this single signed object and fetches indices by hash.
aws s3 cp "$repo/$index/InRelease" "$destination/$index/InRelease" --cache-control "$mutable"
