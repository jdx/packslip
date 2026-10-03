#!/bin/sh
# Install packslip @PACKSLIP_VERSION@ on Linux or macOS:
#
#   curl -fsSL https://packslip.sh | sh
#
# This script downloads the packslip executable built for this host and
# refuses it unless its SHA-256 is the one written below when the release
# was built. Then it puts the executable in a directory for PATH. It
# downloads and runs nothing else. Each release publishes its own copy,
# attested by packslip's release workflow:
#
#   gh attestation verify install.sh --repo jdx/packslip
#
# One copy covers every platform, so its own SHA-256 pins the executable
# for all of them: https://packslip.sh/v@PACKSLIP_VERSION@ always serves
# this file.
#
# Environment:
#   PACKSLIP_BIN_DIR      the directory to install into: ~/.local/bin, or
#                         /usr/local/bin as root, when unset
#   PACKSLIP_RELEASE_URL  where the release files are, such as a mirror,
#                         instead of https://packslip.dev/v@PACKSLIP_VERSION@;
#                         the checksums below hold whatever serves them
#
# Everything runs from main at the end, so a download cut short runs nothing.
set -eu

version='@PACKSLIP_VERSION@'

say() {
  printf 'packslip install: %s\n' "$*" >&2
}

die() {
  say "$*"
  exit 1
}

# The release asset for this host.
platform() {
  case "$(uname -s)/$(uname -m)" in
    Linux/x86_64 | Linux/amd64) echo linux-x64 ;;
    Linux/aarch64 | Linux/arm64) echo linux-arm64 ;;
    Darwin/arm64) echo darwin-arm64 ;;
    Darwin/x86_64)
      # A shell under Rosetta on Apple silicon reports x86_64; the arm64
      # build is the right one there. An Intel Mac has none.
      if [ "$(sysctl -n sysctl.proc_translated 2>/dev/null)" = 1 ]; then
        echo darwin-arm64
      else
        die "packslip publishes no build for Intel Macs; build it with: cargo install packslip --locked"
      fi
      ;;
    MINGW* | MSYS* | CYGWIN*)
      die "on Windows, run this in PowerShell instead: irm https://packslip.sh/install.ps1 | iex"
      ;;
    *)
      die "packslip publishes no build for $(uname -s) $(uname -m); see https://packslip.dev/docs/getting-started/#install-packslip"
      ;;
  esac
}

checksum() {
  case "$1" in
    linux-x64) echo '@SHA256_LINUX_X64@' ;;
    linux-arm64) echo '@SHA256_LINUX_ARM64@' ;;
    darwin-arm64) echo '@SHA256_DARWIN_ARM64@' ;;
  esac
}

download() {
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 3 -o "$2" "$1"
  else
    wget -q -O "$2" "$1"
  fi
}

# SHA-256 of standard input, with whichever tool the host has: coreutils
# and busybox ship sha256sum, macOS ships shasum, and openssl is the last
# resort. Reading stdin keeps the file name out of the output.
sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256
  else
    openssl dgst -sha256 -r
  fi | cut -d' ' -f1
}

main() {
  [ "$#" -eq 0 ] || die "this script installs packslip itself and takes no arguments"
  case "$version" in
    [0-9]*) ;;
    *) die "this is the template in packslip's repository; run the copy a release publishes, from https://packslip.sh" ;;
  esac

  # Check for the tools before downloading anything.
  command -v curl >/dev/null 2>&1 || command -v wget >/dev/null 2>&1 ||
    die "needs curl or wget to download packslip"
  command -v sha256sum >/dev/null 2>&1 || command -v shasum >/dev/null 2>&1 ||
    command -v openssl >/dev/null 2>&1 ||
    die "needs sha256sum, shasum, or openssl to check the download"

  asset=$(platform)
  want=$(checksum "$asset")
  base=${PACKSLIP_RELEASE_URL:-https://packslip.dev/v$version}
  url="${base%/}/packslip-v$version-$asset"

  if [ -n "${PACKSLIP_BIN_DIR:-}" ]; then
    bin_dir=$PACKSLIP_BIN_DIR
  elif [ "$(id -u)" = 0 ]; then
    bin_dir=/usr/local/bin
  else
    [ -n "${HOME:-}" ] || die "HOME is not set; set PACKSLIP_BIN_DIR to the directory to install into"
    bin_dir=$HOME/.local/bin
  fi
  mkdir -p "$bin_dir" || die "cannot create $bin_dir; set PACKSLIP_BIN_DIR to a directory you can write"

  # Download beside the destination, so the final step is a rename on one
  # filesystem and an interrupted install leaves no partial executable.
  tmp=$(mktemp "$bin_dir/.packslip.XXXXXX") ||
    die "cannot write to $bin_dir; set PACKSLIP_BIN_DIR to a directory you can write"
  trap 'rm -f "$tmp"' EXIT
  trap 'exit 1' HUP INT TERM

  say "downloading $url"
  download "$url" "$tmp" || die "could not download $url"
  got=$(sha256 <"$tmp")
  [ "$got" = "$want" ] ||
    die "refusing $url: its SHA-256 is $got, but packslip $version's $asset build is $want"

  chmod 755 "$tmp"
  mv -f "$tmp" "$bin_dir/packslip"
  installed=$("$bin_dir/packslip" version) || die "installed $bin_dir/packslip, but it does not run on this host"
  say "installed $installed at $bin_dir/packslip"

  case ":${PATH:-}:" in
    *":$bin_dir:"*)
      # By file, not name: Fedora's /usr/local/sbin, ahead of
      # /usr/local/bin on PATH, links to it.
      found=$(command -v packslip || true)
      if [ -n "$found" ] && ! [ "$found" -ef "$bin_dir/packslip" ]; then
        say "note: packslip on PATH is $found, which comes before $bin_dir"
      fi
      ;;
    *)
      say "$bin_dir is not on PATH; add it to run packslip by name:"
      say "  export PATH=\"$bin_dir:\$PATH\""
      ;;
  esac
}

main "$@"
