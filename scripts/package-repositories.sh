#!/usr/bin/env bash
# Sign packages, then build signed indices. Publish payloads before these indices.
set -euo pipefail
packages=$(realpath "${1:?usage: package-repositories.sh PACKAGE_DIRECTORY OUTPUT_DIRECTORY}")
out=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "${2:?}")
fingerprint=${PACKSLIP_GPG_FINGERPRINT:?Full signing-key fingerprint required}
[[ "$fingerprint" =~ ^[0-9A-Fa-f]{40}$ ]] || exit 1
test ! -e "$out" || { echo 'repository output already exists' >&2; exit 1; }
mkdir -p "$out/apt/pool/main"
gpg --batch --armor --export "$fingerprint" > "$out/gpg-key.pub"
test -s "$out/gpg-key.pub"
for package in "$packages/"*.deb; do
  arch=$(dpkg-deb --field "$package" Architecture)
  [[ "$arch" = amd64 || "$arch" = arm64 ]] || exit 1
  checksum=$(sha256sum "$package" | cut -d' ' -f1)
  # dpkg-scanpackages --arch selects filenames ending in _ARCH.deb.
  cp "$package" "$out/apt/pool/main/packslip-${checksum}_${arch}.deb"
done
for arch in amd64 arm64; do
  index="$out/apt/dists/stable/main/binary-$arch"
  mkdir -p "$index"
  dpkg-scanpackages --arch "$arch" "$out/apt/pool/main" /dev/null \
    | sed "s|^Filename: $out/apt/|Filename: |" > "$index/Packages"
  test -s "$index/Packages"
  gzip -n -9 -c "$index/Packages" > "$index/Packages.gz"
done
release="$out/apt/dists/stable/Release"
apt-ftparchive -o APT::FTPArchive::Release::Origin=Packslip \
  -o APT::FTPArchive::Release::Label=Packslip \
  -o APT::FTPArchive::Release::Suite=stable \
  -o APT::FTPArchive::Release::Codename=stable \
  -o 'APT::FTPArchive::Release::Architectures=amd64 arm64' \
  -o APT::FTPArchive::Release::Components=main \
  release "$out/apt/dists/stable" > "$out/apt/Release"
# Do not hash the Release file while it is still being written.
mv "$out/apt/Release" "$release"
python3 - "$release" <<'PY'
from datetime import datetime, timedelta, timezone
from email.utils import format_datetime
from pathlib import Path
import sys
path = Path(sys.argv[1])
expiry = format_datetime(datetime.now(timezone.utc) + timedelta(days=14), usegmt=True)
path.write_text(f'Acquire-By-Hash: yes\nValid-Until: {expiry}\n' + path.read_text())
PY
for index in "$out"/apt/dists/stable/main/binary-*; do
  mkdir -p "$index/by-hash/SHA256"
  for file in "$index/Packages" "$index/Packages.gz"; do
    checksum=$(sha256sum "$file" | cut -d' ' -f1)
    cp "$file" "$index/by-hash/SHA256/$checksum"
  done
done
gpg --batch --yes --local-user "$fingerprint" --digest-algo SHA256 \
  --armor --detach-sign --output "$release.gpg" "$release"
gpg --batch --yes --local-user "$fingerprint" --digest-algo SHA256 \
  --clearsign --output "${release%Release}InRelease" "$release"
for package in "$packages/"*.rpm; do
  arch=$(rpm -qp --queryformat '%{ARCH}' "$package")
  [[ "$arch" = x86_64 || "$arch" = aarch64 ]] || exit 1
  work=$(mktemp -d)
  cp "$package" "$work/package.rpm"
  rpm --define "_gpg_name $fingerprint" --define "_gpg_path ${GNUPGHOME:-$HOME/.gnupg}" \
    --define '_gpgbin /usr/bin/gpg' --define '_gpg_digest_algo sha256' \
    --addsign "$work/package.rpm"
  checksum=$(sha256sum "$work/package.rpm" | cut -d' ' -f1)
  mkdir -p "$out/rpm/$arch/packages"
  cp "$work/package.rpm" "$out/rpm/$arch/packages/packslip-$checksum.rpm"
  rm -rf "$work"
done
for arch in x86_64 aarch64; do
  repo="$out/rpm/$arch"
  test -d "$repo/packages"
  createrepo_c --checksum sha256 "$repo"
  gpg --batch --yes --local-user "$fingerprint" --digest-algo SHA256 --armor \
    --detach-sign "$repo/repodata/repomd.xml"
done
cp packaging/packslip.repo "$out/rpm/packslip.repo"
