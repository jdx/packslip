---
title: Bootstrap a tool
description: Replace curl | sh with packslip install, which verifies a tool's signed release and exports its declared commands to a command directory.
group: consume
weight: 50
---
# Bootstrap a tool

Use `packslip install` where a tool's instructions say `curl | sh`. It
downloads the tool's signed release, verifies it against the project name
you give, and exports its declared commands to a directory such as
`~/.local/bin`. To install mise, see
[Use packslip with mise](https://packslip.dev/docs/mise/); to check a file
you already downloaded, see
[Verify a release](https://packslip.dev/docs/verifying/).

Before installing, it checks the release's signer, project, and version, and
the download's digest and size. It keeps the complete archive tree and
remembers the signer, holding later installs of the project to it. It runs
no downloaded code, installs no dependencies, and changes no shell files.
Afterwards the tool can update itself, or you run `packslip install` again;
nothing runs in the background.

## Check what install supports

[Install packslip](https://packslip.dev/docs/install/) 1.5.0 or newer first,
or 1.5.1 or newer under a group-writable umask, such as Debian's default
`002`.
[Supported platforms](https://packslip.dev/docs/compatibility/#supported-platforms)
lists where install runs.

Install takes any tool, from a CLI to a package manager such as mise, whose
release meets these conditions:

- The project is a GitHub project (`github.com/owner/repo`, `owner/repo`
  for short, or `github.com/owner/repo/subpath` for one tool in a monorepo)
  or a domain project (`host` or `host/path`, such as
  `mytool.example.com`). Install refuses `gitlab.com` and reads any other
  host name as a domain project.
- The publisher publishes a release bundle. A domain project also needs its
  signed release list at the well-known location. A GitHub project may add
  a signed supplementary list on its default branch, which install uses.
- The release has an archive, a compressed executable, or a bare executable
  for this host that declares its commands. Installer formats (deb, rpm,
  msi, dmg) and 7z archives are not installed.

`packslip install` is not a package manager. It has no uninstall, upgrade, or
list command; [Remove a tool](https://packslip.dev/docs/bootstrap/#remove-a-tool)
shows manual removal. It exports only the declared commands. Other
resources, such as completions and man pages, stay unlinked in the tree if
the archive holds them, or only in the release metadata if not. Nothing runs
to generate them.
[mise's packslip backend](https://packslip.dev/docs/mise/#install-a-tool)
provides them, plus project configuration and version switching.

Releases up to 1.10.1 differ from this page:

- They print `Signer pin:` for `Signer fingerprint:`, and their pin errors
  read `invalid signer pin in constraint N` and
  `signer does not match constraint N's repository pins`.
- `latest` does not fall back past an uninstallable recommended release; it
  fails, for example with
  `release has no authenticated packslip for the requested project`.
- `--version X.Y.Z` does not select `X.Y.Z+build.5`; it fails, for example
  with `no eligible release found`.
- GitHub owner and repository names, in the project argument or a pin file,
  must match the repository's letter case exactly.
- They hold a keyless domain project to its signer's workflow URL, not to
  the signing repository's ID. Once a newer release has remembered that
  ID, they refuse the project with
  `repository name now resolves to a different remembered repository ID`.
- They do not warn when a project's release list withdraws the release
  they installed for it.

## Install an upstream release

[hk](https://hk.jdx.dev) publishes signed GitHub releases. Install it and run
it by the path packslip reports:

```sh
packslip install github.com/jdx/hk
~/.local/bin/hk --version
```

For hk 2.8.0 it prints this. Here and below, packslip prints your home
directory in full where this page shows `~`.

```text
Installed github.com/jdx/hk 2.8.0 at ~/.local/share/packslip/github-922514152-e3b0c44298fc1c14
Signer fingerprint: ps1_snirenkjwr7m5ozgcufameodnm
Command: ~/.local/bin/hk
```

- `Installed` names the project, the version, and the installation tree.
- `Signer fingerprint` identifies the repository that signed the release;
  only GitHub projects print one. Compare it with the fingerprint the
  publisher publishes, then pass it to `--pin` on other machines and in CI;
  see [Pin the signer](https://packslip.dev/docs/bootstrap/#pin-the-signer).
- `Command` names each exported command, which runs by this path before its
  directory is on PATH.

Warnings go to standard error, each starting with `warning:`. Install warns
once for each resource it does not install; hk 2.8.0 prints seven such
lines, for its completions, CLI spec, and skills. When the command directory
is missing from PATH, install says so and leaves PATH alone. The first
resource warning and the PATH warning:

```text
warning: resource completion is retained in the release metadata; bootstrap installs only declared commands
warning: ~/.local/bin is absent from PATH; invoke the reported command paths or add this directory yourself
```

These paths are for an ordinary Unix user. As root, commands go to
`/usr/local/bin`;
[Choose an installation scope](https://packslip.dev/docs/bootstrap/#choose-an-installation-scope)
lists every location. On Windows, the same command in PowerShell puts a
native executable in your user's `packslip\bin` directory:

```powershell
packslip install github.com/jdx/hk
& "$env:LOCALAPPDATA\packslip\bin\hk.exe" --version
```

## Choose a version

Without `--version`, install asks for `latest`: the vendor's signed
recommendation, else GitHub's latest release for a GitHub project, else the
highest stable version. To fix the tool's release, name it:

```sh
packslip install github.com/jdx/hk --version 2.8.0
```

`--version X.Y.Z` selects that release, including one versioned
`X.Y.Z+build.5`. A prefix (`X.Y` or `X`) or a semver range (`^X.Y`) selects
the highest match, and a tag (`mytool-vX.Y.Z`) only its own release. Only a
request that names a prerelease, such as `X.Y.Z-rc.1`, selects one;
[Matching a request](https://packslip.dev/release/v1/#matching-a-request)
gives the full rules. `--variant NAME` installs a publisher's named variant
instead of the default build for this host.

### Read skipped-release warnings

A `latest` request tries the recommended release, then the other stable
releases from the highest down
([Latest](https://packslip.dev/release/v1/#latest)). It skips a release
when none of its bundles describes this tool in a format this packslip
reads, or when it has no artifact for this host or `--variant`. It warns
once per skipped release, or once per run of skips that share a reason:

```text
warning: skipped 3.0.0, GitHub's latest release: its release carries no packslip for github.com/owner/mytool
warning: skipped 2.9.0 to 2.6.0 (4 releases): no artifact fits this host (linux aarch64 gnu)
```

A version, prefix, or range skips only releases with no bundle for the tool
that install can read; a missing host artifact is an error. Unmet host
requirements, verification or trust failures, and failed downloads never
move install to another release.

### Read withdrawn-release warnings

Install never selects a release that the project's signed release list
withdraws (yanks). When the list withdraws the release you last installed
for the project in this
[scope](https://packslip.dev/docs/bootstrap/#choose-an-installation-scope),
install also warns. The warning names that version and the list's reason,
and says whether this run installed a different release:

```text
warning: the github.com/owner/mytool 1.0.0 you installed was withdrawn in its signed release list: CVE-2026-1234; this run installed 1.1.0 instead
```

When the run installs nothing, for example because `--version 1.0.0` names
the withdrawn release or no other release is eligible, install fails as it
would without the warning. The warning comes first and ends with
`this run did not install a different release`, and 1.0.0 stays installed.
Install checks only when you run it for the project, and mise does not
warn.

## Pin the signer

Without a pin, the first install trusts the signer that the project's name or
host reports, and later installs must keep it. To choose the signer
yourself, pass it on every install, such as in a Dockerfile or a CI
configuration:

```sh
packslip install github.com/jdx/hk --pin ps1_snirenkjwr7m5ozgcufameodnm
```

| Project | Pass |
| --- | --- |
| GitHub project | `--pin` with the signer fingerprint from the publisher's instructions |
| Keyless domain project | `--pin`, or `--issuer` with `--identity` or `--identity-prefix` |
| Key-signed domain project | `--pubkey` with the publisher's public-key file or its base64 line, or `pubkey` in a pin file |

- `--pin` can be repeated; any one value may match. A signer fingerprint
  names a repository, not a version, so new tags still match it.
- `--issuer` requires this OIDC issuer, `--identity` this exact certificate
  identity, and `--identity-prefix` an identity with this prefix. For a
  GitHub project they narrow the policy the name implies, never replace it.
- A key-signed domain project's first install needs its key. Install
  remembers that key in the scope's state directory and verifies later
  installs against it.
- Install also remembers the signer it accepted, including the signing
  repository's ID for a GitHub project or a keyless domain project. It
  refuses another repository under a GitHub project's name, and asks for
  [approval](https://packslip.dev/docs/bootstrap/#approve-a-trust-change)
  before a domain project's list or release from another repository.
- Install does not remember `--pin`, `--issuer`, `--identity`, or
  `--identity-prefix`: pass them on every run, or put them in a
  [pin file](https://packslip.dev/docs/bootstrap/#require-signers-with-pin-files),
  which applies to every run.

[First use and signer fingerprints](https://packslip.dev/docs/trust/#first-use-and-signer-fingerprints)
explains what an unpinned first install trusts for each kind of project and
the gap a pin closes.

## Pin the bootstrapper and let the tool float

Pinning packslip's version and the tool's are separate decisions. A
bootstrapper in a base image, distribution package, or CI configuration can
hold one reviewed packslip while the tool tracks upstream releases. Hold
packslip with a
[versioned install script](https://packslip.dev/docs/install/#pin-a-versioned-install-script)
or a [digest-pinned container image](https://packslip.dev/docs/install/#copy-packslip-from-the-container-image);
`packslip version` prints the version that ran. The command in
[Pin the signer](https://packslip.dev/docs/bootstrap/#pin-the-signer) fixes
hk's publisher but not its release, so each run fetches hk's current
release. Add `--version` to fix that too:

```sh
packslip install github.com/jdx/hk --version 2.8.0 --pin ps1_snirenkjwr7m5ozgcufameodnm
```

[Maintaining packaged verifiers](https://packslip.dev/docs/compatibility/#maintaining-packaged-verifiers)
says when an older packslip needs an update.

## Install in a container

A container build needs no shell installer, curl, or archive extractor:

1. Pin packslip's multi-platform image, `ghcr.io/jdx/packslip`, by digest,
   and copy `/packslip` out of it;
   [Copy packslip from the container image](https://packslip.dev/docs/install/#copy-packslip-from-the-container-image)
   shows how to find the digest.
2. If your image has no CA certificates, copy the image's
   `/etc/ssl/certs/ca-certificates.crt` to the same path, or HTTPS fails.
3. Run `packslip install` in a `RUN` step; as root, it installs in system
   scope.
4. Build with `docker build --no-cache` to resolve `latest` again: Docker
   reuses an unchanged cached `RUN` layer without checking for releases.
5. To copy the tool into another stage, copy `/opt/packslip` too; the
   command in `/usr/local/bin` points into it.

[Bootstrap mise in Docker](https://packslip.dev/docs/mise/#bootstrap-mise-in-docker)
is a complete Dockerfile following these steps.

## Choose an installation scope

| Scope | Installation trees | Commands | State | Configuration |
| --- | --- | --- | --- | --- |
| Unix user | `$XDG_DATA_HOME/packslip/` | `~/.local/bin` | `$XDG_STATE_HOME/packslip/` | `$XDG_CONFIG_HOME/packslip/` |
| Unix system | `/opt/packslip/` | `/usr/local/bin` | `/var/lib/packslip/` | `/etc/packslip/` |
| Windows user | `%LOCALAPPDATA%\packslip\installs\` | `%LOCALAPPDATA%\packslip\bin` | `%LOCALAPPDATA%\packslip\state\` | `%LOCALAPPDATA%\packslip\config\` |
| Windows system | `%ProgramFiles%\packslip\` | `%ProgramFiles%\packslip\bin` | `%ProgramData%\packslip\state\` | `%ProgramData%\packslip\` |

Pin files go in the configuration directory's `pins.d`, and user scope also
reads the system `pins.d`. Only user scope reads a `config.toml`, from its
configuration directory. The XDG variables default to `~/.local/share`,
`~/.local/state`, and `~/.config`; relative values are ignored.

On Unix, root always gets system scope, so this installs hk for all users:

```sh
sudo packslip install github.com/jdx/hk
```

- Root ignores `HOME`, the XDG variables, and user configuration, and cannot
  select `--user`.
- `--system` selects system scope without root, for an account that can
  write the system paths.
- On Windows, user scope is the default; run
  `packslip install github.com/jdx/hk --system` in an elevated shell to
  install for all users.
- `--user` and `--system` are mutually exclusive.

To choose the directories, give either or both as absolute paths:

```sh
packslip install owner/mytool --install-dir /absolute/tree --bin-dir /absolute/bin
```

`--install-dir` sets this project's installation tree and `--bin-dir` the
command directory; neither moves trust state. On Unix, an exported command
is a symlink into the tree. On Windows, it is a small native executable that
starts the command in the tree, so it needs no Developer Mode.

## Remove a tool

Delete each command a `Command` line named and the tree the `Installed` line
named. For the hk install above:

```sh
rm ~/.local/bin/hk
rm -rf ~/.local/share/packslip/github-922514152-e3b0c44298fc1c14
```

A later install then needs no `--force`. Keep the state directory: it
records each project's signer, release-list sequence, and, for a key-signed
project, public key, so a later install is held to the same signer.

## Require signers with pin files

A pin file requires a signer for every install of a project, whatever the
command line says. Administrators put pin files in the system `pins.d`, and
users can add their own in user scope;
[Choose an installation scope](https://packslip.dev/docs/bootstrap/#choose-an-installation-scope)
lists both.

```toml
# /etc/packslip/pins.d/hk.toml
[projects."github.com/jdx/hk"]
pins = ["ps1_snirenkjwr7m5ozgcufameodnm"]
```

Each `[projects."NAME"]` entry takes these keys:

- `pins`: signer fingerprints; any one may match, like repeated `--pin`
  options
- `issuer`, `identity`, `identity_prefix`: like the matching flags; all
  given must match
- `pubkey`: a key-signed domain project's minisign public key, as its
  base64 line

Install reads every `*.toml` file in each `pins.d`, in name order; an
unknown key is an error. `NAME` is the full project name, and a GitHub owner
and repository match in any letter case. A file may name a project only
once, so two spellings of it in one file are refused. Each matching file is
a separate requirement, and so is the command line; install must satisfy
all of them.

## Download from a private host

Install downloads each artifact from the HTTPS `url` in the signed
statement or, without one, from beside the bundle: the bundle's URL with the
signed artifact name as the last path segment. Either way, it checks the
signed size and digest.

For a host that needs a token, add a credential to `config.toml` in user
scope; system scope, including root and container builds, reads none:

```toml
[http.auth."https://downloads.example.com"]
bearer_token_env = "TOOL_DOWNLOAD_TOKEN"
```

- The table name is one exact HTTPS origin: scheme, host, and optional port,
  with no path.
- Install sends the named environment variable's value as a bearer token to
  that origin only, not across a redirect to another origin.
- `GH_TOKEN`, or else `GITHUB_TOKEN`, applies to GitHub requests.
- Error messages leave out tokens and signed URL query strings.

## Approve a trust change

When a release or release list is signed or attested differently from what
install accepted before, install refuses it and describes the change:

```text
trust change <id>: [...]; review this proposal and repeat with --accept-trust-change=<id>
```

Confirm the change with the publisher, then repeat the command with
`--accept-trust-change=<id>`. The ID approves only that exact proposal
against the current policy; repeat the flag for several.
[No-downgrade and approvals](https://packslip.dev/docs/trust/#no-downgrade-and-approvals)
lists the changes that need approval.

## Replace conflicting files

Install refuses to overwrite a command it did not export, or an installation
directory it did not mark as its own:

```text
/usr/local/bin/hk conflicts with an existing command; use --force to replace it
```

`--force` replaces such command entries and unmarked directories: the entry
itself, never a symlink's target. It does not bypass trust, freshness,
extraction safety, protected paths, or host checks, so expired, withdrawn,
rolled-back, or missing-after-acceptance metadata still fails. An entry that
another tool changed is no longer treated as packslip's own export.
Replacements are staged and journaled; the next install recovers an
interrupted one before changing anything else in that scope.

## Override host checks

Install picks the artifact for this host first, then checks its declared host
requirements:

- An unmet minimum OS or glibc version refuses the install, as does a
  required shared library that install confirms is missing.
- A missing required command, or a requirement install cannot check, gives
  a warning.
- `--allow-incompatible-host` accepts the chosen artifact for this attempt,
  without selecting a different build or installing dependencies.

## Raise extraction limits

Extraction refuses an archive that expands past 10 GiB or holds more than
100,000 entries. `--max-extracted-size` (in bytes) and
`--max-archive-entries` change these limits.

## Install offline or with an administrator's trust root

- `--offline` makes no network requests. It needs the metadata and artifacts
  an earlier install cached in the same scope, including an unexpired,
  authenticated Sigstore trust-root (TUF) cache.
- `--trusted-root FILE` uses an administrator-supplied Sigstore
  `trusted_root.json` instead of refreshing the trust root over TUF.

## Accept unlogged signatures

`--allow-unlogged` accepts signatures without transparency-log entries, for
this attempt only; install does not remember the choice.

**Warning:** Use `--allow-unlogged` only for a publisher you have separately
agreed to trust without transparency-log entries.

## Troubleshoot an install

| Error | What to do |
| --- | --- |
| `unsupported bootstrap host <os>/<arch>` | Install does not run on this platform; Intel macOS (`darwin/x86_64`) is one example. See [Supported platforms](https://packslip.dev/docs/compatibility/#supported-platforms). Download the release and follow [Verify a release](https://packslip.dev/docs/verifying/). |
| `bootstrap forge discovery currently supports github.com; other forges require a host-named project` | Install refuses `gitlab.com`. Download the release and follow [Verify a release](https://packslip.dev/docs/verifying/). |
| `host project has no signed release list` | The domain project serves no list at `https://HOST/.well-known/packslip.json` (`.well-known/packslip/PATH.json` for `HOST/PATH`), or removed it after an earlier install. Ask the publisher for one. |
| `release list is stale, missing, or rolled back` | The list expired, fell below an accepted `sequence`, or, for a GitHub project, disappeared after install accepted one. Ask the publisher for a current list; keep your state directory meanwhile. `packslip verify` does not check freshness; see [Release-list freshness](https://packslip.dev/docs/trust/#release-list-freshness). |
| `trust change <id>: ...` | See [Approve a trust change](https://packslip.dev/docs/bootstrap/#approve-a-trust-change). |
| `signer does not match constraint N's signer fingerprints` | A pin file or `--pin` names another signer. N counts from 0: matching system pin files, then user pin files, then the command line. |
| `signer does not match constraint N's identity policy` | A pin file's `issuer`, `identity`, or `identity_prefix`, or the matching flag, does not match. N counts as above. |
| `invalid signer fingerprint in constraint N` | A `--pin` or `pins` value is not a `ps1_` signer fingerprint. Copy it again from the publisher. |
| `repository name now resolves to a different remembered repository ID` | The name now belongs to another repository, as when a deleted repository's name is reused. See [Repository identity, renames, and transfers](https://packslip.dev/docs/trust/#repository-identity-renames-and-transfers). |
| `no eligible release found` | No release matches the request. Check the version or tag against the publisher's releases, or omit `--version`. |
| `no artifact fits this host` | The release has no artifact for this platform or `--variant` in a format install unpacks. Check what the publisher ships, or name another release with `--version`. |
| `unsafe archive: artifact declares no commands` | Ask the publisher to declare the artifact's commands. |
| `selected artifact is incompatible with this host: ...` | See [Override host checks](https://packslip.dev/docs/bootstrap/#override-host-checks). |
| `archive exceeds local extraction limits` | The archive passes the size or entry limit. If you expect an archive that large, see [Raise extraction limits](https://packslip.dev/docs/bootstrap/#raise-extraction-limits). |
| `<path> conflicts with an existing command; use --force to replace it` | Remove the other file, or see [Replace conflicting files](https://packslip.dev/docs/bootstrap/#replace-conflicting-files). |
| `<path> is unmarked or belongs to another project; use --force to replace it` | Install did not create this directory for this project. Choose another `--install-dir`, or see [Replace conflicting files](https://packslip.dev/docs/bootstrap/#replace-conflicting-files). |
| `no installable release of <project>; skipped ...` | Every candidate was skipped, for the reasons listed. Check that the publisher ships an artifact for this host, or name a release with `--version`. |
| `stopped looking for an installable release of <project> after fetching ...` | Install stops before a release would take it past 32 fetched bundles. Name an older release with `--version`. |
| `offline trust-root refresh requires an authenticated cache` | Run once without `--offline`. |
| `run without root privileges to select the user scope` | Run as your own user, or drop `--user`. |
| `installation configuration requires absolute paths` | Give `--install-dir` and `--bin-dir` as absolute paths. |

The [`packslip install` reference](https://packslip.dev/cli/install/) lists
every flag.
