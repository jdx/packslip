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
cp "$work/"*.dsc "$work/"*.debian.tar.* "$work/"*.changes "$out/"
printf 'Prepared source package for %s\n' "$distribution"
