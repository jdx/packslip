#!/usr/bin/env bash
# Install the packslip CLI for a composite action step, or adopt one the
# job already has. Shared by action.yml and releases/action.yml.
#
# Environment:
#   PACKSLIP_ACTION_ROOT  the checkout of jdx/packslip the action came from;
#                         its Cargo.toml gives the default version
#   PACKSLIP_VERSION      a release to download instead of that default
#   PACKSLIP_SHA256       optional SHA-256 for a version override
#   PACKSLIP_PATH         an executable to use instead of downloading
#   GH_TOKEN              for `gh release download` and `gh attestation verify`
# Uses the runner's RUNNER_TEMP and GITHUB_PATH.
set -euo pipefail
case "$(uname -s)" in
  Linux) os=linux; exe= ;;
  Darwin) os=darwin; exe= ;;
  MINGW*|MSYS*|CYGWIN*) os=windows; exe=.exe ;;
  *) echo "unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac
dir="${RUNNER_TEMP}/packslip-bin"
mkdir -p "$dir"
if [ -n "$PACKSLIP_PATH" ]; then
  if [ -n "${PACKSLIP_SHA256:-}" ]; then
    echo "::warning::packslip-path is set; ignoring packslip-sha256" >&2
  fi
  # A binary the job built or installed itself: every later step
  # runs it as `packslip`, whatever it is called here.
  if [ -n "${PACKSLIP_VERSION}" ]; then
    echo "::warning::packslip-path is set; ignoring packslip-version ${PACKSLIP_VERSION}"
  fi
  case "$PACKSLIP_PATH" in
    */*) bin="$PACKSLIP_PATH" ;;
    *) bin="$(command -v "$PACKSLIP_PATH")" || { echo "packslip-path not found on PATH: $PACKSLIP_PATH" >&2; exit 1; } ;;
  esac
  [ -x "$bin" ] || { echo "packslip-path is not an executable file: $PACKSLIP_PATH" >&2; exit 1; }
  cp "$bin" "$dir/packslip$exe"
else
  default_version="$(sed -nE 's/^version *= *"([^"]+)".*/\1/p' "${PACKSLIP_ACTION_ROOT}/Cargo.toml" | head -n1)"
  version="${PACKSLIP_VERSION:-$default_version}"
  [ -n "$version" ] || { echo "could not read the packslip version from Cargo.toml" >&2; exit 1; }
  case "$(uname -m)" in
    x86_64|amd64) arch=x64 ;;
    aarch64|arm64) arch=arm64 ;;
    *) echo "unsupported arch: $(uname -m)" >&2; exit 1 ;;
  esac
  if [ "$os" = windows ]; then ext=zip; else ext=tar.xz; fi
  asset="packslip-v${version}-${os}-${arch}.${ext}"
  expected=""
  if [ -z "$PACKSLIP_VERSION" ]; then
    # The major action tag points at an action-lock commit made after this
    # CLI release's draft contains its final assets. The archive digest is
    # therefore part of the action commit a workflow pins, not a release
    # sidecar that could later be replaced with the binary.
    lock="${PACKSLIP_ACTION_ROOT}/action/archives.sha256"
    [ -f "$lock" ] || {
      echo "this action checkout has no internal archive digest lock; pin the generated action-v${version} tag or its commit" >&2
      exit 1
    }
    expected="$(awk -v asset="$asset" '$2 == asset { print $1 }' "$lock")"
    if [ "$(printf '%s\n' "$expected" | sed '/^$/d' | wc -l | tr -d ' ')" != 1 ] || ! [[ "$expected" =~ ^[0-9a-f]{64}$ ]]; then
      echo "internal archive digest lock has no unique lowercase SHA-256 for $asset" >&2
      exit 1
    fi
    if [ -n "${PACKSLIP_SHA256:-}" ] && [ "$PACKSLIP_SHA256" != "$expected" ]; then
      echo "packslip-sha256 disagrees with the action's internal archive digest lock" >&2
      exit 1
    fi
  elif [ -n "${PACKSLIP_SHA256:-}" ]; then
    expected="$PACKSLIP_SHA256"
  else
    # Preserve the old explicit-version escape hatch. It is intentionally
    # noisier than the default: an action commit can lock only the CLI
    # release it was prepared for.
    echo "::warning::packslip-version overrides the action's internally pinned release; verifying build provenance without an archive SHA-256. Set packslip-sha256 for a strict override." >&2
  fi
  if ! gh release download "v${version}" -R jdx/packslip -p "$asset" -D "$dir" --clobber; then
    echo "could not download $asset from jdx/packslip v${version}" >&2
    if [ "$os" = darwin ] && [ "$arch" = x64 ]; then
      # Current macOS releases are arm64 only.
      echo "there is no macOS x64 release: build the CLI (cargo install packslip --version ${version} --locked --root \"\$RUNNER_TEMP/packslip\") and pass packslip-path" >&2
    fi
    exit 1
  fi
  if [ -n "$expected" ]; then
    if ! [[ "$expected" =~ ^[0-9a-f]{64}$ ]]; then
      echo "packslip-sha256 must be a 64-character lowercase hexadecimal SHA-256 digest" >&2
      exit 1
    fi
    if command -v sha256sum >/dev/null 2>&1; then
      actual="$(sha256sum "$dir/$asset" | awk '{print $1}')"
    elif command -v shasum >/dev/null 2>&1; then
      actual="$(shasum -a 256 "$dir/$asset" | awk '{print $1}')"
    else
      echo "packslip-sha256 was set but neither sha256sum nor shasum is available" >&2
      exit 1
    fi
    if [ "$actual" != "$expected" ]; then
      echo "packslip archive SHA-256 mismatch: expected $expected, got $actual" >&2
      exit 1
    fi
  fi
  # The archive was built by jdx/packslip's release workflow; check that before running it.
  gh attestation verify "$dir/$asset" -R jdx/packslip
  if [ "$ext" = zip ]; then
    # -j drops the archive's directory so the executable lands in $dir.
    (cd "$dir" && unzip -ojq "$asset")
  else
    tar -xJf "$dir/$asset" -C "$dir" --strip-components=1
  fi
fi
echo "$dir" >> "$GITHUB_PATH"
"$dir/packslip$exe" version
