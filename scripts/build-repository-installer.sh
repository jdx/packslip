#!/usr/bin/env bash
# Build a native, static installer and the resources its packages install.
set -euo pipefail
target=${1:?usage: build-repository-installer.sh TARGET OUTPUT_DIRECTORY}
out=${2:?}
case "$target:$(uname -m)" in
  x86_64-unknown-linux-musl:x86_64|aarch64-unknown-linux-musl:aarch64) ;;
  *) echo 'repository installers must be built on their native Linux architecture' >&2; exit 1 ;;
esac
test ! -e "$out"
rustup target add "$target"
export CC_x86_64_unknown_linux_musl=musl-gcc
export CC_aarch64_unknown_linux_musl=musl-gcc
cargo build --release --locked --target "$target" --no-default-features --features install-cli
mkdir -p "$out"
cp "target/$target/release/packslip" "$out/"
"$out/packslip" install --help
for shell in bash zsh fish; do
  "$out/packslip" completion "$shell" > "$out/packslip.$shell"
done
