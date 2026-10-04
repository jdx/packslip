---
title: Bootstrap a package manager
weight: 52
group: consume
description: Install an authenticated upstream release with user or system scope, without running an installer script.
---
# Bootstrap a package manager

`packslip install` installs a publisher's signed upstream release without
requiring another package manager beyond the distribution's own. The command
is available in the development branch; it has not shipped in a release yet.
It verifies the release, keeps the complete artifact tree, and exposes only
its declared commands. It runs no downloaded code and changes no shell files.

```sh
packslip install github.com/jdx/mise
packslip install owner/repo --version 1.2
packslip install tool.example.com --pubkey /path/to/vendor.pub
```

The publisher must provide a packslip. GitHub releases may also carry a signed
supplementary list; a host project needs its signed well-known list.
Other forge APIs are not supported by this bootstrap command yet. The
[installer library guide](/docs/installers/) describes the general format.
Use a signer `--pin` from trusted publisher instructions when available.

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
sudo packslip install owner/repo --system
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
pins = ["ps1_aaaaaaaaaaaaaaaaaaaaaaaaaa"] # illustrative; obtain the real pin separately
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

Before recommending mise, rustup, uv, or pnpm through this path, record the exact
upstream release, platform, scope, tree, and command paths; then run the tool's
normal setup and self-update in an isolated account. Check runtime lookup,
argument and exit-status forwarding, whether self-update replaces the executable
or its export, and whether a later Packslip install respects that changed ownership.
These real publisher checks remain outstanding; the synthetic fixture passes
are evidence for the installation mechanism only.
