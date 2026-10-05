#!/usr/bin/env bash
# Install the packslip CLI for a composite action step, or adopt one the
# job already has. Shared by action.yml and releases/action.yml.
#
# Environment:
#   PACKSLIP_ACTION_ROOT  the checkout of jdx/packslip the action came from;
#                         its Cargo.toml gives the default version
#   PACKSLIP_VERSION      a release to download instead of that default
#   PACKSLIP_SHA256       optional SHA-256 for the downloaded release archive
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
  version="${PACKSLIP_VERSION:-$(sed -nE 's/^version *= *"([^"]+)".*/\1/p' "${PACKSLIP_ACTION_ROOT}/Cargo.toml" | head -n1)}"
  [ -n "$version" ] || { echo "could not read the packslip version from Cargo.toml" >&2; exit 1; }
  case "$(uname -m)" in
    x86_64|amd64) arch=x64 ;;
    aarch64|arm64) arch=arm64 ;;
    *) echo "unsupported arch: $(uname -m)" >&2; exit 1 ;;
  esac
  if [ "$os" = windows ]; then ext=zip; else ext=tar.xz; fi
  asset="packslip-v${version}-${os}-${arch}.${ext}"
  source_commit=""
  expected="${PACKSLIP_SHA256:-}"
  if [ -n "$expected" ] && ! [[ "$expected" =~ ^[0-9a-f]{64}$ ]]; then
    echo "packslip-sha256 must be a 64-character lowercase hexadecimal SHA-256 digest" >&2
    exit 1
  fi
  if [ -z "${PACKSLIP_VERSION:-}" ]; then
    # These hashes are in the pinned action checkout, never fetched alongside
    # mutable release assets. The release PR committed them before its tag.
    lock="${PACKSLIP_ACTION_ROOT}/action/release.json"
    [ -f "$lock" ] || { echo "the action checkout has no release digest map; use a published action commit" >&2; exit 1; }
    pinned="$(jq -er --arg version "$version" --arg asset "$asset" \
      'select(.schema == 1 and .version == $version) | .assets[$asset].sha256 | select(type == "string")' "$lock")"
    source_commit="$(jq -er '.source_commit | select(type == "string")' "$lock")"
    [[ "$pinned" =~ ^[0-9a-f]{64}$ ]] && [[ "$source_commit" =~ ^[0-9a-f]{40}$ ]] || {
      echo "invalid internal release digest or source commit" >&2; exit 1;
    }
    if [ -n "$expected" ] && [ "$expected" != "$pinned" ]; then
      echo "packslip-sha256 disagrees with the pinned action's release digest" >&2
      exit 1
    fi
    expected="$pinned"
  else
    echo "::warning::packslip-version overrides the internal release lock; using repository provenance and any explicit packslip-sha256" >&2
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
  policy=()
  if [ -n "$source_commit" ]; then
    policy=(--source-digest "$source_commit" --source-ref refs/heads/release-plz
      --signer-workflow jdx/packslip/.github/workflows/release.yml
      --signer-digest "$source_commit" --deny-self-hosted-runners)
  fi
  gh attestation verify "$dir/$asset" -R jdx/packslip "${policy[@]}"
  if [ "$ext" = zip ]; then
    # -j drops the archive's directory so the executable lands in $dir.
    (cd "$dir" && unzip -ojq "$asset")
  else
    tar -xJf "$dir/$asset" -C "$dir" --strip-components=1
  fi
fi
echo "$dir" >> "$GITHUB_PATH"
"$dir/packslip$exe" version
