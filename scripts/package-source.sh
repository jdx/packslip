#!/usr/bin/env bash
# Prepare a committed source snapshot with intact vendored Cargo checksums.
# This is offline build input for PPA/COPR, not source-level crate vendoring.
set -euo pipefail
out=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "${1:?usage: package-source.sh OUTPUT_DIRECTORY}")
version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "unsupported package version" >&2; exit 1; }
mkdir -p "$out"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
name="packslip-$version"
mkdir -p "$work/$name"
git archive HEAD | tar -xf - -C "$work/$name"
(
  cd "$work/$name"
  cargo vendor --locked vendor > .cargo/vendor-config.toml
  cat .cargo/vendor-config.toml >> .cargo/config.toml
  rm .cargo/vendor-config.toml
  # No additional package manager, network fetch, or rustup runs in the build.
  # Keep Cargo checksum files: modifying vendor contents must fail verification.
)
# BSD tar otherwise emits macOS AppleDouble metadata as binary source files.
COPYFILE_DISABLE=1 tar -C "$work" -czf "$out/$name.tar.gz" "$name"
cp "$out/$name.tar.gz" "$out/packslip_$version.orig.tar.gz"
python3 - "$version" "$out" <<'PY'
import pathlib, sys
version, out = sys.argv[1:]
spec = pathlib.Path('packaging/rpm/packslip.spec').read_text()
pathlib.Path(out, 'packslip.spec').write_text(spec.replace('@VERSION@', version))
PY
printf 'Prepared %s (complete installer, offline Cargo sources)\n' "$name"
