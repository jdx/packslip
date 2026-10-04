#!/usr/bin/env bash
# Requires package-source.sh output. Build an unsigned source package locally;
# the publish workflow signs the resulting .dsc/.changes with the release key.
set -euo pipefail
out=$(realpath "${1:?usage: package-debian-source.sh SOURCE_DIRECTORY [DISTRIBUTION]}")
distribution=${2:-resolute}
revision=${PACKAGE_REVISION:-1}
[[ "$distribution" =~ ^[a-z][a-z0-9-]*$ && "$revision" =~ ^[1-9][0-9]*$ ]] || exit 1
version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
tar -xf "$out/packslip-$version.tar.gz" -C "$work"
cp "$out/packslip_$version.orig.tar.gz" "$work/"
(
  cd "$work/packslip-$version"
  cp -R packaging/debian debian
  chmod +x debian/rules
  cat > debian/changelog <<EOF
packslip ($version-0ppa$revision~$distribution) $distribution; urgency=medium

  * New upstream installer release.

 -- Jeff Dickey <jdx@jdx.dev>  $(date -R)
EOF
  dpkg-buildpackage -S -us -uc -d
)
cp "$work/"*.dsc "$work/"*.debian.tar.* "$work/"*.changes "$work/"*.buildinfo "$out/"
# dput needs every file named by .changes, including source-only buildinfo.
# Check the uploaded file closure before the temporary build tree disappears.
python3 - "$out/"*.changes <<'PYFILES'
import hashlib
import pathlib
import sys
from email.parser import Parser

for name in sys.argv[1:]:
    changes = pathlib.Path(name)
    fields = Parser().parsestr(changes.read_text())
    checksums = fields.get("Checksums-Sha256", "").split()
    if not checksums or len(checksums) % 3:
        raise SystemExit(f"missing or malformed checksums in {changes.name}")
    for digest, size, filename in zip(checksums[::3], checksums[1::3], checksums[2::3]):
        if pathlib.Path(filename).name != filename:
            raise SystemExit(f"invalid source filename: {filename}")
        path = changes.parent / filename
        if not path.is_file() or path.stat().st_size != int(size):
            raise SystemExit(f"missing or incomplete source file: {filename}")
        with path.open("rb") as source:
            actual = hashlib.file_digest(source, "sha256").hexdigest()
        if actual != digest:
            raise SystemExit(f"source checksum mismatch: {filename}")
PYFILES
printf 'Prepared source package for %s\n' "$distribution"
