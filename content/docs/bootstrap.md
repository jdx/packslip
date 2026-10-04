---
title: Install a tool with packslip
weight: 52
group: consume
description: Install signed upstream tools directly, choose a version and installation scope, or bootstrap mise with a pinned verifier.
---
# Install a tool with packslip {#bootstrap-a-package-manager}

`packslip install` downloads, verifies, and installs a tool's signed upstream
release. Use it for a CLI you want on PATH, to bootstrap a package manager such
as mise, or to install a release in a container. Any tool that publishes a
packslip can use this path; it is not limited to package managers.

Install [packslip 1.5.1 or newer](/docs/getting-started/#install-packslip) first.
The command checks the release's signer, project, version, digest, and size,
keeps the complete artifact tree, and exposes its declared commands. It runs
no downloaded code and changes no shell files.

## Install an upstream release

For example, install [hk](https://hk.jdx.dev), which publishes signed GitHub
releases:

```sh
packslip install github.com/jdx/hk
~/.local/bin/hk --version
```

These commands assume an ordinary Unix user. packslip prints every command
path it creates, so you can invoke that path even before adding its directory
to PATH. Root uses `/usr/local/bin` instead.

The default version request is `latest`. Use `--version` for an exact release,
a version prefix, or a release tag:

```sh
packslip install github.com/jdx/mise --version 2026.10.1 \
  --pin ps1_nlhmwtfeufglxv5myvwvronk7a
```

The publisher must provide a packslip. GitHub releases may also carry a signed
supplementary list; a host project needs its signed well-known list.
Other forge APIs are not supported by this bootstrap command yet. The
[installer library guide](/docs/installers/) describes the general format.
For a key-signed project on its own domain, pass its public key with `--pubkey`.
For a forge project, use a signer `--pin` from trusted publisher instructions
when available. Without a caller pin, the first installation trusts the
repository the forge reports for the name and remembers it for later installs.

## Pin the bootstrapper and let mise float

Pinning packslip and pinning the tool it installs are separate decisions.
A CI configuration or Dockerfile can keep one reviewed packslip version while
requesting the current mise release every time installation runs:

```sh
packslip version
packslip install github.com/jdx/mise \
  --pin ps1_nlhmwtfeufglxv5myvwvronk7a
~/.local/bin/mise --version
```

Obtain the packslip version you chose through a
[versioned install script or pinned container image](/docs/getting-started/#install-packslip).
The mise signer pin above identifies its GitHub repository, not a version:
new tags still match it. Omitting `--version` lets mise float; adding
`--version 2026.10.1` fixes mise too. Rerun `packslip install` to fetch a later
release, or let mise manage its own updates with `mise self-update`. No
background updater runs on packslip's behalf.

This is useful when you want a small bootstrapper in a base image or a
distribution package, while allowing mise to track the upstream releases it
needs to work with changing tool registries. A stable version 1 manifest format
supports that separation. It is not a promise to freeze the verifier forever:
security fixes and new signing formats may require a packslip update. See
[Compatibility and support](/docs/compatibility/#maintaining-packaged-verifiers).

### Bootstrap mise in Docker

This Debian example pins packslip's multi-platform image by digest, then asks
it to verify and install the current mise release. The CA bundle enables HTTPS;
no shell installer, curl, or external archive extractor is needed:

```dockerfile
FROM ghcr.io/jdx/packslip:1.5.1@sha256:fcbbcb85ab02d433d6108c212ffc7eaeda0bbafca4b82111c452568ac680b9c4 AS bootstrap
FROM debian:13-slim

COPY --from=bootstrap /packslip /usr/local/bin/packslip
COPY --from=bootstrap /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

ARG MISE_VERSION=latest
RUN packslip install github.com/jdx/mise --version "$MISE_VERSION" \
      --pin ps1_nlhmwtfeufglxv5myvwvronk7a \
    && mise --version

CMD ["mise", "--version"]
```

Build with `docker build --no-cache -t mise-bootstrap .` to resolve `latest`
again: an unchanged cached `RUN` layer does not check for new releases.
Pass `--build-arg MISE_VERSION=2026.10.1` to fix mise's release as well.
Keep `/opt/packslip` if you copy this installation into another stage;
`/usr/local/bin/mise` points into that tree. The
[mise Docker cookbook](https://mise.jdx.dev/mise-cookbook/docker.html#bootstrap-with-packslip)
shows how to add project tools and use mise in the container.

## Choose an installation scope

| Scope | Tree and command directory | State |
| --- | --- | --- |
| Unix user | `$XDG_DATA_HOME/packslip/`; `~/.local/bin` | `$XDG_STATE_HOME/packslip/` |
| Unix system | `/opt/packslip/`; `/usr/local/bin` | `/var/lib/packslip/` |
| Windows user | LocalAppData `packslip\installs`; `packslip\bin` | LocalAppData `packslip\state` |
| Windows system | Program Files `packslip`; `packslip\bin` | ProgramData `packslip\state` |

Unix XDG defaults are `~/.local/share` and `~/.local/state`. Root selects system
scope and ignores HOME, XDG variables, and user configuration. Windows defaults
to user scope; `--system` selects shared paths and requires suitable permissions.
`--user` and `--system` are mutually exclusive. Linux x64/ARM64, macOS ARM64,
and Windows x64/ARM64 are the bootstrap targets. Intel macOS is unsupported.

```sh
sudo packslip install github.com/jdx/hk --system
packslip install owner/repo --install-dir /absolute/tree --bin-dir /absolute/bin
```

Overrides move output destinations without moving trust state. Repository IDs
and monorepo subpaths identify forge installations, so renames and transfers
retain history. A deleted and recreated repository name is refused. Unix
exports are symlinks; Windows exports are native executables requiring no
Developer Mode. Packslip reports the paths and warns when the command directory
is absent from PATH; it does not edit PATH itself.

## Trust, replacement, and recovery

Caller pins and matching TOML files under `/etc/packslip/pins.d/` and the user's
config directory are independent requirements. Windows uses ProgramData
`packslip\pins.d` and LocalAppData `packslip\config\pins.d`. User Unix config is
`$XDG_CONFIG_HOME/packslip` (default `~/.config/packslip`). A file looks like:

```toml
[projects."github.com/jdx/mise"]
pins = ["ps1_nlhmwtfeufglxv5myvwvronk7a"]
```

Ordinary releases retain signing-workflow continuity by default. A publisher's
`pin_workflow: false` permits repository-level continuity; changing a remembered
value requires approval. A refusal displays `--accept-trust-change=<id>` binding
the exact proposal to the current policy. Lists and releases have independent
continuity records. Key-signed host projects retain their public key.

`--force` permits replacing conflicting commands and unmarked directories.
It replaces the specified entry, never a symlink's target, and does not bypass
trust, freshness, extraction safety, protected paths, or host checks. Replacements
are staged, journaled, and recovered before another install modifies the scope.
An entry changed by another tool is no longer treated as Packslip's old export.

Signed-list sequence history is persisted before download or installation, so
a failed attempt cannot make an older list acceptable. Expired, withdrawn,
missing-after-acceptance, or rolled-back metadata fails even with `--force`.
`--offline` makes no network requests and requires cached metadata and artifacts,
including an unexpired authenticated TUF cache. `--trusted-root` is an explicit
administrator override. `--allow-unlogged` is intended for publishers the caller
has separately agreed to trust without transparency-log entries.

The deterministic artifact choice precedes host checks. Missing OS/glibc or
known loader-library requirements refuse installation; missing commands and
uncheckable requirements warn. `--allow-incompatible-host` accepts the chosen
artifact for one attempt. It does not select a different build or install dependencies.
Local extraction limits default to 10 GiB and 100,000 entries; use
`--max-extracted-size` and `--max-archive-entries` to change them.

HTTP credentials in `config.toml` are scoped to an exact HTTPS origin:

```toml
[http.auth."https://downloads.example.com"]
bearer_token_env = "TOOL_DOWNLOAD_TOKEN"
```

`GH_TOKEN` and `GITHUB_TOKEN` apply to GitHub. Tokens and signed URL queries stay
out of error reports. No resources are executed or separately exported; the
bootstrap installs commands and keeps the full archive layout.

## Build for a distribution

[Distribution packaging](/docs/distributions/) provides offline source
recipes and interim PPA/COPR publication configuration for this build.

```sh
cargo build --locked --release --no-default-features --features install-cli
```

This includes installation, verification, discovery, host checks, extraction,
and native exports, while excluding publishing, signing, executable decoding,
and schema generation. `verify-cli` remains available for a verifier-only
consumer. Distributions needing bootstrap installation should use `install-cli`.

Fixtures in CI check installation and relocation, runtime files beside commands,
argument and exit-status forwarding, retained keys, list rollback and withdrawal,
and refusal of signature, digest, version, and policy conflicts. These fixtures
do not establish that any publisher's setup or self-update behavior works with
the layout.

## Publisher adoption

The [compatibility and support matrix](/docs/compatibility/) records required CI
coverage and the checks needed before publishing instructions for a real tool.

Before recommending a tool on a new platform or scope, record the exact
upstream release, platform, scope, tree, and command paths; then run the tool's
normal setup and self-update in an isolated account. Check runtime lookup,
argument and exit-status forwarding, whether self-update replaces the executable
or its export, and whether a later Packslip install respects that changed ownership.
The compatibility matrix distinguishes checked publisher handoffs from pending
ones. Synthetic fixtures are evidence for the installation mechanism only.

See the [`install` reference](/cli/install/) for every flag. For project
configuration, version switching, and the tool's man pages and completions,
use [mise's packslip backend](/docs/mise/#install-a-tool).
