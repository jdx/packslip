#!/usr/bin/env bash
# Test install.sh end to end: render it for a stand-in release whose
# executables are small shell scripts, serve that release over HTTP on
# localhost, and install from it with each POSIX shell this host has.
# The refusals are tested too: a tampered download, a host without a
# download or hash tool, and arguments. None of them may leave anything in
# the install directory.
#
# Usage: installer/test.sh
# Needs python3 for the HTTP server. With INSTALLER_TEST_IMAGES set to
# container images, such as "alpine:3 busybox:latest", it also installs
# inside each with docker, over the host network.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
server=
cleanup() {
  if [ -n "$server" ]; then kill "$server" 2>/dev/null || true; fi
  rm -rf "$work"
}
trap cleanup EXIT

failures=0
fail() {
  echo "FAIL: $*" >&2
  failures=$((failures + 1))
}
pass() {
  echo "ok: $*"
}
skip() {
  echo "skip: $*"
}

# A release of version 0.0.0 whose executables print which build they
# are, so installing the wrong platform's build shows.
version=0.0.0
mkdir -p "$work/release" "$work/tampered"
for asset in linux-x64 linux-arm64 darwin-arm64 windows-x64 windows-arm64; do
  name="packslip-v$version-$asset"
  case "$asset" in
    windows-*) name="$name.exe" ;;
  esac
  printf '#!/bin/sh\necho "packslip %s (%s)"\n' "$version" "$asset" >"$work/release/$name"
  printf '#!/bin/sh\necho "not packslip"\n' >"$work/tampered/$name"
done
"$here/render.sh" "$version" "$work/release" >/dev/null
script="$work/release/install.sh"
case "$(uname -s)/$(uname -m)" in
  Linux/x86_64) asset=linux-x64 ;;
  Linux/aarch64) asset=linux-arm64 ;;
  Darwin/arm64) asset=darwin-arm64 ;;
  *)
    echo "no packslip build for this host; nothing to test" >&2
    exit 1
    ;;
esac

port=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
python3 -m http.server --bind 127.0.0.1 --directory "$work" "$port" >/dev/null 2>&1 &
server=$!
for _ in $(seq 50); do
  curl -fsS -o /dev/null "http://127.0.0.1:$port/release/install.sh" 2>/dev/null && break
  sleep 0.1
done
base="http://127.0.0.1:$port"

# A PATH holding only the named commands, to take tools away.
only() {
  local dir="$work/path-$1"
  shift
  rm -rf "$dir"
  mkdir -p "$dir"
  for tool in uname mkdir mktemp cut chmod mv rm id cat "$@"; do
    local found
    found=$(command -v "$tool") || continue
    ln -s "$found" "$dir/$tool"
  done
  echo "$dir"
}

# try_install NAME SHELL [VAR=VALUE...] [-- ARGS...]: run the rendered script
# with SHELL into a fresh directory, and leave its output in $out.
try_install() {
  local name=$1 shell=$2
  shift 2
  dest="$work/bin-$name"
  rm -rf "$dest"
  local envs=() args=()
  while [ "$#" -gt 0 ]; do
    if [ "$1" = -- ]; then
      shift
      args=("$@")
      break
    fi
    envs+=("$1")
    shift
  done
  status=0
  # The ${a[@]+...} form keeps an empty array from tripping set -u in bash 3.2.
  out=$(env PACKSLIP_RELEASE_URL="$base/release" PACKSLIP_BIN_DIR="$dest" ${envs[@]+"${envs[@]}"} \
    "$shell" "$script" ${args[@]+"${args[@]}"} 2>&1) || status=$?
}

# The install directory holds nothing but, when wanted, the executable.
left_clean() {
  local stray
  stray=$(find "$dest" -name '.packslip.*' 2>/dev/null)
  [ -z "$stray" ] || fail "$1: left $stray behind"
}

# /bin/sh first: it is what `curl ... | sh` runs, dash on Debian and
# Ubuntu and bash 3.2 on macOS.
shells=(/bin/sh)
for shell in dash bash ksh mksh zsh; do
  if command -v "$shell" >/dev/null 2>&1; then shells+=("$(command -v "$shell")"); fi
done
# Wrappers name their shell by path: some checks run with a reduced PATH.
if command -v busybox >/dev/null 2>&1; then
  printf '#!/bin/sh\nexec %s sh "$@"\n' "$(command -v busybox)" >"$work/busybox-sh"
  chmod +x "$work/busybox-sh"
  shells+=("$work/busybox-sh")
fi

for shell in "${shells[@]}"; do
  name=$(basename "$shell")
  # zsh runs a script as zsh unless told to behave as sh.
  if [ "$name" = zsh ]; then
    printf '#!/bin/sh\nexec %s --emulate sh "$@"\n' "$shell" >"$work/zsh-sh"
    chmod +x "$work/zsh-sh"
    shell="$work/zsh-sh"
  fi

  try_install "$name" "$shell"
  if [ "$status" -eq 0 ] && [ "$("$dest/packslip")" = "packslip $version ($asset)" ]; then
    pass "$name: installs the $asset build"
  else
    fail "$name: install failed ($status): $out"
  fi
  case "$out" in
    *"is not on PATH"*) pass "$name: says the directory is not on PATH" ;;
    *) fail "$name: no PATH hint: $out" ;;
  esac
  left_clean "$name"

  # Again over the installed copy.
  status=0
  env PACKSLIP_RELEASE_URL="$base/release" PACKSLIP_BIN_DIR="$dest" "$shell" "$script" >/dev/null 2>&1 || status=$?
  if [ "$status" -eq 0 ]; then
    pass "$name: reinstalls"
  else
    fail "$name: reinstall failed ($status)"
  fi

  # As `curl ... | sh` runs it: the script on standard input.
  rm -rf "$dest"
  status=0
  env PACKSLIP_RELEASE_URL="$base/release" PACKSLIP_BIN_DIR="$dest" "$shell" <"$script" >/dev/null 2>&1 || status=$?
  if [ "$status" -eq 0 ] && [ -x "$dest/packslip" ]; then
    pass "$name: installs from standard input"
  else
    fail "$name: install from standard input failed ($status)"
  fi

  try_install "$name-tampered" "$shell" PACKSLIP_RELEASE_URL="$base/tampered"
  case "$status/$out" in
    0/*) fail "$name: installed a tampered download" ;;
    *"refusing $base/tampered/"*) pass "$name: refuses a tampered download" ;;
    *) fail "$name: tampered download failed the wrong way: $out" ;;
  esac
  [ ! -e "$dest/packslip" ] || fail "$name: a tampered download was left installed"
  left_clean "$name-tampered"

  try_install "$name-args" "$shell" -- jdx/mise
  case "$status/$out" in
    0/*) fail "$name: accepted arguments" ;;
    *"takes no arguments"*) pass "$name: refuses arguments" ;;
    *) fail "$name: arguments failed the wrong way: $out" ;;
  esac

  status=0
  out=$("$shell" "$here/install.sh" 2>&1) || status=$?
  case "$status/$out" in
    0/*) fail "$name: the unrendered template ran" ;;
    *"this is the template"*) pass "$name: refuses to run the unrendered template" ;;
    *) fail "$name: the template failed the wrong way: $out" ;;
  esac

  # Taking a tool off PATH only removes it from a shell that has no copy
  # of its own: busybox sh runs its built-in sha256sum and wget regardless.
  nohash=$(only nohash curl wget)
  if env PATH="$nohash" "$shell" -c 'command -v sha256sum || command -v shasum || command -v openssl' >/dev/null 2>&1; then
    skip "$name: has a hash tool built in"
  else
    try_install "$name-nohash" "$shell" PATH="$nohash"
    case "$status/$out" in
      0/*) fail "$name: installed without a hash tool" ;;
      *"needs sha256sum, shasum, or openssl"*) pass "$name: refuses without a hash tool" ;;
      *) fail "$name: no hash tool failed the wrong way: $out" ;;
    esac
    [ ! -e "$dest/packslip" ] || fail "$name: a download was left installed without a hash tool"
  fi

  nofetch=$(only nofetch sha256sum shasum)
  if env PATH="$nofetch" "$shell" -c 'command -v curl || command -v wget' >/dev/null 2>&1; then
    skip "$name: has a download tool built in"
  else
    try_install "$name-nofetch" "$shell" PATH="$nofetch"
    case "$status/$out" in
      0/*) fail "$name: installed without curl or wget" ;;
      *"needs curl or wget"*) pass "$name: refuses without curl or wget" ;;
      *) fail "$name: no download tool failed the wrong way: $out" ;;
    esac
  fi

  # Each hash tool and each download tool alone.
  for tools in "curl sha256sum" "curl shasum" "curl openssl" "wget sha256sum"; do
    # shellcheck disable=SC2086 # split into the two tool names
    set -- $tools
    if ! command -v "$1" >/dev/null 2>&1 || ! command -v "$2" >/dev/null 2>&1; then
      continue
    fi
    try_install "$name-$1-$2" "$shell" PATH="$(only "$1-$2" "$1" "$2")"
    if [ "$status" -eq 0 ] && [ -x "$dest/packslip" ]; then
      pass "$name: installs with only $1 and $2"
    else
      fail "$name: with only $1 and $2 ($status): $out"
    fi
  done
done

# Inside containers, as root: the default directory is /usr/local/bin.
for image in ${INSTALLER_TEST_IMAGES:-}; do
  status=0
  out=$(docker run --rm --network host -v "$work/release:/release:ro" \
    -e PACKSLIP_RELEASE_URL="$base/release" "$image" \
    sh -c 'sh /release/install.sh && packslip' 2>&1) || status=$?
  case "$status/$out" in
    0/*"packslip $version ($asset)"*) pass "$image: installs into /usr/local/bin" ;;
    *) fail "$image: install failed ($status): $out" ;;
  esac
done

if [ "$failures" -gt 0 ]; then
  echo "$failures check(s) failed" >&2
  exit 1
fi
echo "all install.sh checks passed"
