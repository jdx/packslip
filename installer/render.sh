#!/usr/bin/env bash
# Write install.sh and install.ps1 for one release into the directory that
# holds its files, filled in with the version and the SHA-256 of each
# executable the release ships uncompressed. release.yml runs this before
# it attests and uploads the release files, so both scripts are published
# with the release, and https://packslip.sh serves the latest release's.
#
# Usage: installer/render.sh VERSION DIR
#   VERSION  the release version, 1.2.3
#   DIR      the release files, including packslip-v1.2.3-linux-x64 and
#            the other uncompressed executables
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: $0 VERSION DIR" >&2
  exit 2
fi
version=$1
dir=$2
here=$(cd "$(dirname "$0")" && pwd)

# Releases are plain X.Y.Z (release.yml checks the tag against Cargo.toml).
if ! [[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "not a release version: $version" >&2
  exit 1
fi

subs=(-e "s/@PACKSLIP_VERSION@/${version}/g")
for asset in linux-x64 linux-arm64 darwin-arm64 windows-x64 windows-arm64; do
  file="$dir/packslip-v${version}-${asset}"
  case "$asset" in
    windows-*) file="$file.exe" ;;
  esac
  if [ ! -f "$file" ]; then
    echo "missing $file: every platform's executable must be in $dir" >&2
    exit 1
  fi
  digest=$(sha256sum "$file" | cut -d' ' -f1)
  key=$(printf '%s' "$asset" | tr 'a-z-' 'A-Z_')
  subs+=(-e "s/@SHA256_${key}@/${digest}/g")
done

for script in install.sh install.ps1; do
  sed "${subs[@]}" "$here/$script" >"$dir/$script"
  if grep -n '@[A-Z0-9_]*@' "$dir/$script" >&2; then
    echo "$script still has a placeholder; add it to $0" >&2
    exit 1
  fi
done
chmod 755 "$dir/install.sh"
echo "wrote $dir/install.sh and $dir/install.ps1 for packslip $version"
