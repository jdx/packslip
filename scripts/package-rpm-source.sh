#!/usr/bin/env bash
set -euo pipefail
out=$(realpath "${1:?usage: package-rpm-source.sh SOURCE_DIRECTORY}")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}
cp "$out/"packslip-*.tar.gz "$work/SOURCES/"
cp "$out/packslip.spec" "$work/SPECS/"
rpmbuild --define "_topdir $work" -bs "$work/SPECS/packslip.spec"
cp "$work/SRPMS/"*.src.rpm "$out/"
