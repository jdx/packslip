---
title: packslip release/v1 specification
url: /release/v1/
---
# packslip: a signed release manifest

Version 1, draft. Predicate types `https://packslip.dev/release/v1` and
`https://packslip.dev/releases/v1`.

Author: Jeff Dickey ([@jdx](https://github.com/jdx)).

## Goal

A vendor publishes one signed, machine-readable document per release that
says what the artifacts are and how to verify them. The artifacts are the
files the release ships: usually an archive, installer, or bare executable
per platform for a command-line or desktop application, but any file the
vendor wants verified can be listed, from a source tarball to a data file.
A consumer, whether that is [mise](https://mise.jdx.dev),
[pacvamp](https://pacvamp.com), or a corporate mirror, verifies it against
one pinned identity or key. In return
it gets checksums, platform mapping, executables, provenance links, and
whatever else ships with the release: completions, man pages, a CLI spec, a
skill, a desktop entry, an SBOM.

There is no per-vendor recipe. A consumer pins one identity per vendor, or
one repackager host that describes many vendors on their behalf, and the
documents say the rest.

packslip invents as little as it can. The document is an
[in-toto statement](https://github.com/in-toto/attestation/blob/main/spec/v1/statement.md) in a
[sigstore bundle](https://docs.sigstore.dev/), the same shape
[GitHub artifact attestations](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations/using-artifact-attestations-to-establish-provenance-for-builds),
[npm provenance](https://docs.npmjs.com/generating-provenance-statements), and Homebrew bottles
use. Identity comes from sigstore's certificate authority and transparency
log. What packslip adds is the
predicate: the release manifest itself.

## Names

A project is named by a host, optionally followed by a path.
`github.com/jdx/mise`, `gitlab.com/group/tool`, `mise.jdx.dev`. No scheme,
lowercase host with at least one dot, no empty or dot segments, no trailing
slash.

The name is the location and, on a forge, the identity:

- `github.com/<owner>/<repo>`: releases and their packslips are GitHub
  release assets, and the packslip is expected to be signed by a workflow
  of that repository through GitHub's OIDC issuer
  (`https://token.actions.githubusercontent.com`).
- `gitlab.com/<path>`: likewise, signed by a pipeline of that project
  through `https://gitlab.com`. GitLab subgroups make paths arbitrary
  depth, so the whole path is the pin.
- Any other host: the vendor controls the domain and publishes a release
  list at the well-known URL below, signed with the key or identity the
  consumer pins.

A consumer needs nothing else to start verifying a project on a known
forge. A short-name alias table (`mise` for `github.com/jdx/mise`) is a
convenience a consumer may add; it is not part of the format.

### Monorepos

A repository that releases several tools names each one with a subpath:
`github.com/oxc-project/oxc/oxlint`,
`github.com/bazelbuild/buildtools/buildifier`,
`github.com/biomejs/biome/cli`. Each tool gets its own packslip per
release, with its own `version`, and `source.tag` carries the real tag
(`oxlint_v1.0.0`, `cli/v1.9.4`), so nobody has to guess how a tag maps to a
version. The identity pin is still the repository: any workflow of
`oxc-project/oxc` may sign a packslip for `oxc-project/oxc/oxlint`.

When several tools share one GitHub release, each ships its own bundle,
named `packslip.<subpath>.sigstore.json` with `/` in the subpath replaced
by `-` (`packslip.oxlint.sigstore.json`, `packslip.crates-cli.sigstore.json`).
A repository's own packslip stays `packslip.sigstore.json`. Consumers do
not trust the file name: they read the `packslip*.sigstore.json` assets of
a release and keep the one whose `predicateType` is `release/v1` and whose
`project` is the name they asked for.

## The file

A release ships one file per project: a
[sigstore bundle](https://github.com/sigstore/protobuf-specs) (v0.3). Its
content is a [DSSE](https://github.com/secure-systems-lab/dsse) envelope of type
`application/vnd.in-toto+json` carrying the statement below. Its
verification material is the signer's [Fulcio](https://docs.sigstore.dev/certificate_authority/overview/)
certificate or a public-key hint, plus the [Rekor](https://docs.sigstore.dev/logging/overview/)
transparency log entry for the signature. Only an air-gapped key-signed release omits the log entry;
see Signing.

The bundle carries the statement, so the signed bytes are exactly the
payload bytes and a consumer needs no canonicalization step. `packslip show`
prints them; so does `jq -r .dsseEnvelope.payload | base64 -d`.
[`cosign`](https://github.com/sigstore/cosign) and
[`gh attestation`](https://cli.github.com/manual/gh_attestation) understand the bundle as-is.

## The release statement

```json
{
  "_type": "https://in-toto.io/Statement/v1",
  "subject": [
    { "name": "mise-v2026.9.1-linux-x64.tar.xz",
      "digest": { "sha256": "...", "sha512": "..." } },
    { "name": "mise-v2026.9.1.cdx.json",
      "digest": { "sha256": "..." } }
  ],
  "predicateType": "https://packslip.dev/release/v1",
  "predicate": {
    "project": "github.com/jdx/mise",
    "version": "2026.9.1",
    "published_at": "2026-09-01T12:00:00Z",
    "source": { "repo": "https://github.com/jdx/mise", "commit": "...", "tag": "v2026.9.1" },
    "artifacts": [
      {
        "name": "mise-v2026.9.1-linux-x64.tar.xz",
        "os": "linux", "arch": "x86_64", "libc": "gnu",
        "size": 12345678,
        "url": "https://github.com/jdx/mise/releases/download/v2026.9.1/mise-v2026.9.1-linux-x64.tar.xz",
        "format": "tar.xz",
        "bin": ["mise/bin/mise"],
        "requires": { "glibc_min": "2.31", "libs": [] },
        "provenance": ["https://api.github.com/repos/jdx/mise/attestations/sha256:..."]
      }
    ],
    "resources": [
      { "kind": "completion", "shell": "zsh", "archive": "mise/share/zsh/site-functions/_mise" },
      { "kind": "man", "archive": "mise/man/man1/mise.1" },
      { "kind": "cli-spec", "format": "usage", "bin": "mise", "archive": "mise/share/usage/mise.kdl" },
      { "kind": "skill", "name": "mise", "repo": "skills/mise" },
      { "kind": "sbom", "format": "cyclonedx", "asset": "mise-v2026.9.1.cdx.json",
        "url": "https://github.com/jdx/mise/releases/download/v2026.9.1/mise-v2026.9.1.cdx.json" }
    ],
    "identity": {
      "scheme": "sigstore-oidc",
      "key_id": "https://github.com/jdx/mise/.github/workflows/release.yml@refs/tags/v2026.9.1",
      "issuer": "https://token.actions.githubusercontent.com"
    },
    "notes_url": "https://github.com/jdx/mise/releases/tag/v2026.9.1"
  }
}
```

Rules:

- `subject` lists every artifact by file name with its digests, plus any
  separate file a resource comes from. `artifacts` carries the same names
  with platform, size, download URL, format, executables, requirements, and
  provenance links. Every artifact is a subject, every subject is an
  artifact or a resource's `asset`, and neither list contains a duplicate.
  At least one artifact is required. `sha256` is required and is 64
  lowercase hex characters; `sha512` is optional and 128.
- `project` is a name as defined above. `version` is [semver 2.0.0](https://semver.org/spec/v2.0.0.html); its
  prerelease part, if any, says whether the release is a prerelease and
  which channel it is on. See Versions. `published_at` is [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339) UTC.
- `os`, `arch`, `libc`, `format`, and `variant` are lowercase words of
  letters, digits, `_`, `-`, and `.`, starting with a letter or digit.
  The documented values are the ones consumers know; see Vocabularies. A
  value outside them is well-formed but matches no host and unpacks with
  nothing, so a vendor uses one only for a platform or format this
  document has not named yet. An absent `os`, `arch`, or `libc` means the
  artifact does not depend on it: a universal macOS binary has `os` and
  no `arch`, a script or a jar has none of the three.
- `format` is the archive or installer type, or `raw` for a bare
  executable. Two artifacts that differ only in format carry the same
  build, and a consumer takes whichever it prefers. A vendor must not
  publish two artifacts that agree on `os`, `arch`, `libc`, `variant`,
  and `format`; `packslip create` refuses to, and a consumer that finds
  such a pair refuses to choose.
- `variant` tells apart builds that share `os`, `arch`, and `libc`:
  `fips`, `baseline`, `debug`, `installer`, `source`. A consumer selects
  only artifacts without a variant unless asked for one.
- `bin` lists the executables inside the artifact. Each entry is a path
  from the true archive root, with no top-level directory stripped, or,
  for a bare executable, the artifact's own name without its compression
  suffix. When the name to put on PATH differs from the file name, the
  entry is `{ "path": "bin/oxlint-x86_64", "name": "oxlint" }`. Two
  entries may share a path under different names, which is how a vendor
  declares an alias such as `pnpx` for `pnpm`; a consumer may link or
  copy. A name is the command as typed, without `.exe`: a Windows path
  carries its extension (`bin/tool.exe`) and the consumer puts that file
  on PATH as `name.exe`. `requires.bin` and a resource's `bin` are
  written the same way, so names compare without adding or stripping
  anything. The reference implementation reads an older document that
  wrote `name` with `.exe` as if it had not.
- `requires` states what the host must already provide: `os_min` in the
  OS's own terms (`12` for macOS Monterey, `10.0.17763` for Windows),
  `glibc_min` for a `gnu` Linux build, `libs`, the shared libraries the
  executables load, and `bin`, the commands they run. See Host
  requirements.
- `provenance` holds URLs of [SLSA build provenance](https://slsa.dev/spec/v1.0/provenance)
  statements for that artifact. The packslip proves the manifest; verified
  provenance proves the build, at whatever [SLSA build level](https://slsa.dev/spec/v1.0/levels)
  its builder establishes.
- `resources` lists what the release ships besides its executables, each
  entry a `kind` and one source. See Resources.
- `notes_url` points at the release notes.
- `extensions` carries what the specification has no field for, keyed by
  who defines it. See Extensions.
- `identity` says how the document is signed and by whom, so a consumer
  can check what it pinned against what it received. For `sigstore-oidc`,
  `key_id` is the certificate's subject identity (a workflow URI for CI, an
  email for a person) and `issuer` the OIDC issuer. For `sigstore-key`,
  `key_id` is the key id in uppercase hex.
- `attested_by` is `vendor` (default) or `repackager`. See below.

The JSON schema is printed by `packslip schema` and published at
`https://packslip.dev/schema/release-v1.json`. It enforces everything
above that a schema can: the value patterns, the semver grammar, the
digest lengths. The validator in the reference implementation enforces
the rest.

### Vocabularies

After Rust's target triples, so that a vendor's build matrix maps onto
them without a table:

- `os`: `linux`, `darwin`, `windows`, `freebsd`, `netbsd`, `openbsd`,
  `illumos`, `android`, `ios`.
- `arch`: `x86_64`, `aarch64`, `armv7`, `armv6`, `riscv64`, `i686`,
  `powerpc64le`, `s390x`, `loongarch64`.
- `libc`: `gnu`, `musl`, for Linux builds.
- `format`, archives: `tar.xz`, `tar.gz`, `tar.zst`, `tar.bz2`, `tgz`,
  `tar`, `zip`, `7z`. Single compressed executables: `gz`, `xz`, `zst`,
  `bz2`. Installers: `deb`, `rpm`, `dmg`, `pkg`, `msi`, `msix`, `exe`,
  `appimage`. A bare executable: `raw`.

A consumer unpacks archives and bare executables, decompressing a single
compressed executable to the file its `bin` names. Installers are listed
so a consumer can hand them to the platform's installer or a launcher;
a package manager that installs into its own directory does not unpack
them. A `.exe` that is the program itself is `raw`; `exe` means a
Windows installer.

### Field reference

| Field | Type | | Meaning |
|---|---|---|---|
| `_type` | string | required | Always `https://in-toto.io/Statement/v1`. |
| `subject[]` | array | required | One entry per artifact and per resource asset. |
| `subject[].name` | string | required | The file name. |
| `subject[].digest.sha256` | string | required | SHA-256 of the file, lowercase hex. |
| `subject[].digest.sha512` | string | optional | SHA-512 of the file, lowercase hex. |
| `predicateType` | string | required | Always `https://packslip.dev/release/v1`. |
| `predicate.project` | string | required | Host path naming the project, such as `github.com/jdx/mise` or `github.com/oxc-project/oxc/oxlint`. |
| `predicate.version` | string | required | Semver 2.0.0. Its prerelease part marks a prerelease and names the channel. |
| `predicate.published_at` | string | required | RFC 3339 UTC publish time. |
| `predicate.source` | object | optional | Where the release was built from. |
| `predicate.source.repo` | string | required | Source repository URL. |
| `predicate.source.commit` | string | optional | Commit the release was built from. |
| `predicate.source.tag` | string | optional | Tag the release was built from, as the vendor spells it. |
| `predicate.artifacts[]` | array | required | One entry per artifact; at least one. |
| `artifacts[].name` | string | required | File name, matching a `subject` entry. |
| `artifacts[].os` | string | optional | `linux`, `darwin`, `windows`, ... Absent: any OS. |
| `artifacts[].arch` | string | optional | `x86_64`, `aarch64`, ... Absent: any architecture. |
| `artifacts[].libc` | string | optional | `gnu` or `musl`; Linux only. Absent: no dependence on one. |
| `artifacts[].variant` | string | optional | Distinguishes builds sharing os, arch, and libc: `fips`, `baseline`, `debug`, `installer`, `source`. |
| `artifacts[].size` | integer | required | File size in bytes. Verified alongside the digest. |
| `artifacts[].url` | string | optional | Download URL. |
| `artifacts[].format` | string | optional | Archive, compression, or installer type, or `raw` for a bare executable. |
| `artifacts[].bin[]` | array of string or object | optional | Executables inside the artifact: a path from the archive root, or `{ path, name }` when the PATH name differs. A name is the command as typed, without `.exe`. |
| `artifacts[].requires` | object | optional | What the host must provide. See Host requirements. |
| `requires.os_min` | string | optional | Minimum OS version in the OS's own terms. |
| `requires.glibc_min` | string | optional | Minimum glibc for a `gnu` Linux build. |
| `requires.libs[]` | array of string | optional | Shared libraries loaded from the host, by loader name (`libssl.so.3`, `vcruntime140.dll`). Read from the executables by `packslip create`; empty means none needed, absent means unchecked. |
| `requires.bin[]` | array of object | optional | Commands the executables need on PATH: `{ name, min? }`, a bare name and the lowest version that works. |
| `artifacts[].provenance[]` | array of string | optional | URLs of SLSA build provenance statements for this artifact. |
| `predicate.resources[]` | array of object | optional | What ships besides the executables. See Resources. |
| `resources[].kind` | string | required | `completion`, `man`, `cli-spec`, `skill`, `sbom`, `desktop`, `icon`, `app`, or a kind consumers may not know yet. |
| `resources[].artifact` | string | optional | Exact artifact filename. Limits this resource to that artifact and outranks platform-only scope. |
| `resources[].os`, `arch`, `libc` | string | optional | Limit the entry to artifacts of that platform, when layouts differ. |
| `resources[].archive` | string | one source | Path inside the selected artifact, from the archive root. |
| `resources[].asset` | string | one source | Name of a separate release file, listed in `subject` with its digest. |
| `resources[].url` | string | optional | Download URL of the asset. Only with `asset`. |
| `resources[].repo` | string | one source | Path in the source repository at `source.commit`, which is then required. |
| `resources[].exec[]` | array of string | one source | An argv whose first element is a `bin` name and whose stdout is the file. See Running an exec entry. |
| `resources[].env` | object of string | optional | Environment variables for the command, with `{shell}` substituted in values as in the argv. Only with `exec`. |
| `resources[].shell` | string | completion | The shell a static completion is for. |
| `resources[].shells[]` | array of string | completion | Every shell an `exec` completion generates, substituted for `{shell}` in the argv. |
| `resources[].format` | string | cli-spec, sbom | The spec format (`usage`) or the SBOM format (`cyclonedx`, `spdx`). |
| `resources[].bin` | string | cli-spec, completion, man | The executable the entry is for, by its `bin` name. Required for a `cli-spec`; for a `completion` or `man`, required when the release has more than one executable, and meaning that one when it has one. |
| `resources[].name` | string | skill | The skill's name. |
| `predicate.identity.scheme` | string | required | `sigstore-oidc` or `sigstore-key`. |
| `predicate.identity.key_id` | string | required | The certificate identity, or the key id in uppercase hex. |
| `predicate.identity.issuer` | string | optional | The OIDC issuer, for `sigstore-oidc`. |
| `predicate.attested_by` | string | optional | `vendor` (default) or `repackager`. |
| `predicate.evidence[]` | array of object | optional | What a repackager checked: `{ kind, detail }`. |
| `predicate.notes_url` | string | optional | URL of the release notes. |
| `extensions` | object | optional | Vendor- or consumer-defined data, keyed by who defines it, on the release, each artifact, each resource, the release list, and each list entry. See Extensions. |

### Resources

A release usually ships more than its executables: shell completions, a man
page, a spec of the CLI, an agent skill, a software bill of materials, and,
for a desktop application, the entry, icons, or app bundle a launcher
needs. `resources` lists them. Each entry names a `kind` and exactly one
source. The sources differ in what a consumer can verify, and a consumer
prefers them in this order:

- `archive`: a path inside the artifact the consumer selected, from the
  true archive root. The artifact's digest already covers it.
- `asset`: a separate release file. It is listed in `subject` with its
  digest and the entry carries its download `url`, so it verifies exactly
  as an artifact does. A skill directory ships this way as an archive of
  its own.
- `repo`: a path in the source repository at `source.commit`, which pins
  its content. `source.commit` is required.
- `exec`: an argv whose first element is a `bin` name and whose stdout is
  the file, with `env` for anything the command reads from its
  environment. Nothing verifies the output beyond the executable itself,
  so it ranks last among sources, but for a completion it is the usual
  case: cobra, clap, oclif, and usage generate completions from the
  binary and ship no file. What matters is when it runs, and Running an
  exec entry says so. A vendor with a static file lists it first.

Layouts differ by platform more often than by file, so an entry may carry
`os`, `arch`, or `libc` to say which artifacts it describes; an entry
without them describes every artifact. A resource applies to the selected
artifact when each field it carries equals the artifact's. An entry may
also name `artifact`, the exact filename in `artifacts`, when archives for
the same platform have different layouts or a resource belongs to one
variant. That artifact must exist and match any platform scope on the
entry. An artifact-specific entry outranks every platform-only entry for
the same resource; among equally scoped entries the platform specificity
and source ordering below apply. For example, a man page inside
`tool-linux-x64.tar.xz` can name that artifact without claiming the bare
`tool-linux-x64` executable holds a man page. A TOML manifest spells this
as `artifact = "tool-linux-x64.tar.xz"` inside `[[resource]]`.

Entries compete only with entries for the same thing. Two entries are for
the same thing when they share a `kind` and an identity: `bin` and
`shell` for a completion, `bin` and `format` for a `cli-spec`, `name` for
a skill,
`format` for an SBOM, and for every other kind the file name of the
source, or the kind alone for an `exec` source. Among the entries for one
thing that apply to the selected artifact, a consumer takes the most
specific (the one naming the most of `os`, `arch`, and `libc`), and only
then applies the source order above. Entries for different things never
hide one another: a skill scoped to `linux` beside an unscoped skill of
another name leaves that skill in place, and a zsh completion for one
platform says nothing about the bash one. The reference implementation
provides this as `select_resources`.

Documented kinds:

- `completion`: a shell completion script. With a static source, `shell`
  names the shell: `bash`, `zsh`, `fish`, `powershell`, `nushell`,
  `elvish`. With `exec`, `shells` lists every shell the command generates
  and `{shell}` stands for each in the argv or in an `env` value:
  `["tool", "completion", "{shell}"]` for cobra and clap, `["tool"]` with
  `"env": { "COMPLETE": "{shell}" }` for clap's dynamic completions,
  `"env": { "_TOOL_COMPLETE": "{shell}_source" }` for click. `bin` names
  the executable the script completes; a release with one executable may
  leave it out, and an entry that does is for that executable, competing
  with one that names it.
- `man`: a man page. The section is the file's suffix, as in `mise.1`.
  `bin` names the executable it documents, as for a completion.
- `cli-spec`: a machine-readable description of the executable named by
  `bin`, in `format`. The documented format is `usage`, a
  [usage](https://usage.jdx.dev) spec. From it, the consumer's own copy of
  `usage` generates completions for every shell it supports, a man page,
  and markdown documentation, so nothing of the vendor's runs at install
  time.
  Completions generated this way call `usage complete-word` at shell
  runtime, so a consumer that generates them installs `usage` beside the
  tool. Man pages and documentation carry no such dependency. A vendor that
  would rather not put that dependency on its users' machines ships static
  completions as well.
- `skill`: an agent skill in the [Agent Skills](https://agentskills.io) format: a directory holding
  `SKILL.md` and whatever it references, named by `name`. From an
  `archive` or `repo` source the path is that directory. As an `asset` the
  skill is an archive of the directory's contents, with `SKILL.md` at the
  archive root or under one top-level directory, which the consumer
  strips as it does for an artifact; nothing else is at the root. With
  `exec`, the command prints a single `SKILL.md`. A consumer counts a
  skill as present only once `SKILL.md` is in place, so a half-fetched
  directory never passes for one.
- `sbom`: a software bill of materials in `format`, `cyclonedx` or `spdx`,
  from an `archive`, `asset`, or `repo` source so that its digest or
  commit covers it. Never from `exec`. A release with one SBOM per
  platform lists one entry per `os` or per artifact's archive.
- `desktop`: a freedesktop [desktop entry](https://specifications.freedesktop.org/desktop-entry-spec/latest/), for a Linux launcher. AppImage,
  deb, rpm, and the Windows installer formats carry their own.
- `icon`: an icon file. A hicolor path (`share/icons/hicolor/512x512/...`)
  gives its size.
- `app`: a macOS application bundle inside a `dmg` or `zip`, by its path
  in the archive, which a consumer copies to Applications. Archive only.

For any one need, a consumer takes the first source it can use in the
order above: an `archive` or `asset` entry, then `repo`, then one derived
from a `cli-spec`, and only then `exec`. It ignores kinds and sources it
does not know, so a vendor may ship a `font` or a kind of its own before
the specification names it.

A resource is an extra, and the executables are installed with or without
it. A consumer that cannot fetch one, because the asset or the repository
file is not there or the network fails, reports that and finishes the
install without it; a later attempt may fetch it. A resource that arrives
with a digest other than the one `subject` gives, or a repository file
that is not what `source.commit` holds, is another matter: that is a
broken release or a tampered one, and the consumer refuses it as it would
an artifact.

An artifact with `bin` is something a command-line package manager
installs. A release whose resources include `desktop` or `app` is something
a desktop launcher installs. Many applications are both, and the entries say
so without a category that would misfile them.

#### Running an exec entry

An `exec` entry runs the release's own executable, so the question is not
whether a consumer trusts its output but when the executable first runs.
A completion is asked for by a shell, the first time a user completes the
command, and by then the user has chosen to run that command: generating
the script at that moment is no more trust than the user has already
extended. So a consumer runs an `exec` completion on demand, when a shell
first asks, without any permission beyond the install itself, and caches
the output for that release and shell so the command runs once rather
than at every completion. An `exec` entry that a consumer would run at
install time, before the user has run anything, and whose output it
writes to disk, such as a skill, runs only when the user has said that
vendor code may run at install; a consumer that has not treats the entry
as absent rather than failing. A consumer may also generate completions
at install under that same permission, to have them ready.

Either way it runs the command with the release's executables on PATH, in
a directory of its own that is not the user's project, with no standard
input, with standard error discarded, and under a timeout of its choosing;
a few seconds suits a completion. `{shell}` in the argv and in `env`
values is replaced by the shell being asked for, and `env` is added to an
environment that is otherwise the consumer's. A non-zero exit, a timeout,
or empty output means the entry is absent this time and may be tried
again later.

### Host requirements

An executable that installs cleanly and fails on first use, because a
library or a program it expected is not there, is the failure a release
manifest is well placed to prevent. `requires` says what the host must
already provide, in names the operating system defines rather than names
a package manager defines, so any consumer can check them without a
registry. Beside `os_min` and `glibc_min`:

- `libs` lists the shared libraries the executables load from the host,
  each by the name the loader resolves: a soname on Linux and FreeBSD
  (`libssl.so.3`, `libstdc++.so.6`), a DLL name on Windows
  (`vcruntime140.dll`), the file name of a dylib on macOS
  (`libssl.3.dylib`). It leaves out the C runtime and loader that `libc`
  and `glibc_min` already describe (libc, libm, libdl, libpthread, librt,
  libgcc_s, and the dynamic loader; `/usr/lib` and `/System/Library` on
  macOS; the DLLs Windows ships), and any library the artifact carries
  itself and finds through its own rpath. `packslip create` reads the
  list from the executables named in `bin`, so it says what the bytes
  say, and a consumer holding the artifact can read the same list; the
  signed one lets it check before downloading. An empty list means the
  executables were read and need nothing beyond the baseline. An absent
  one means nothing was checked, as for an installer `create` does not
  open or an executable that is a script.
- `bin` lists the commands the executables run and cannot work without,
  each a bare name as the executable invokes it (`java`, `python3`,
  `git`; no directory, no `.exe`) with an optional `min`, the lowest
  version that works, matched as a prefix on dot-separated components
  like a requested version, so `17` means 17.0.0 and later. The vendor
  declares these; nothing in a binary says it runs `java`. Only hard
  requirements belong here; an optional integration goes under
  `extensions`. A required command may not be one the release itself
  provides.

A consumer checks `requires` before installing, and what it does with a
requirement the host does not meet depends on whether the executables can
run without it:

- A library in `libs` the loader will not find, a `glibc_min` above the
  host's glibc, or an `os_min` above the host's OS version means the
  executables will not start. The consumer refuses the install and says
  what is missing in its own terms: a distribution package for a soname,
  an OS upgrade. A user may override the refusal for that tool.
- A command in `bin` that is missing or below its `min` means only the
  paths that call it fail, and the user may be about to install it. The
  consumer installs, warns, and names the command and the version it
  needs, as a tool it can install where it can.

It fails on nothing it cannot check: a version it cannot read, a loader
it cannot ask, is a warning, not a refusal. Between two artifacts that fit
the host, it prefers the one whose requirements the host meets.

How to check, on the common hosts: a library by the loader's own search,
which is the dynamic linker's cache and search path on Linux
(`ldconfig -p`, `LD_LIBRARY_PATH`), the system library and framework
directories on macOS, and PATH with the system directory on Windows; a
command by looking its name up on PATH, with `.exe` on Windows, and
running it with `--version` to read a version, of which `min` must be a
prefix by dot-separated components. `glibc_min` compares the same way
against the host's glibc, and `os_min` against the version the OS
reports.

That is the boundary. `requires` names what the host must have, by names
the OS itself resolves. It does not name other projects, versions of
them, or where to get them: that needs a namespace only a registry has,
and a package manager's install hints go under `extensions`. Two things
this version leaves out on purpose, to be added if vendors need them: a
`bin` entry satisfied by any of several commands (`terraform` or `tofu`),
and a symbol version on a library (`libstdc++.so.6` at `GLIBCXX_3.4.29`),
which `glibc_min` covers for libc alone.

### Extensions

A vendor or a consumer sometimes has something to say that the
specification has no field for: a package manager's install hints, a
Homebrew tap, an end-of-life date, a build id. It goes in `extensions`, an
object that the release predicate, each artifact, each resource, the
release-list predicate, and each release entry may carry:

```json
"extensions": {
  "mise": { "postinstall": "mise reshim" },
  "example.com": { "build_id": "20260901.3" }
}
```

Each key names the party that defines its value: a consumer by its name
(`mise`, `pacvamp`), a vendor by a domain it controls (`example.com`). The
value is whatever that party documents. packslip assigns no meaning to
anything under `extensions` and never will, so nothing put there can
collide with a field a later revision adds. The signature covers it like
everything else, and `packslip show` prints it unchanged. A consumer reads
the keys it defines and ignores the rest.

Everywhere else, a consumer ignores fields it does not know, so a later
revision of this version can add fields without breaking older consumers.
A vendor does not use that room for its own data: a field it invents may
be claimed by a later revision with another meaning, while a key under
`extensions` never is. Something many vendors put under `extensions` is a
candidate for a field of its own; when that happens the extension key
keeps working beside it.

### Repackager attestation

A repository, registry, or mirror that describes a vendor's artifacts, and
whose vendor publishes no packslip, may sign one itself with
`"attested_by": "repackager"`. The `project` still names the vendor's
project and the artifacts are still the vendor's files, but `identity` is
the repackager's, and `evidence` says what it checked before signing:

```json
"attested_by": "repackager",
"evidence": [
  { "kind": "apt-release-gpg", "detail": "3FEF9748469ADBE15DA7CA80AC2D62742012EA22" },
  { "kind": "pkgbuild-checksums" }
]
```

Documented kinds: `pkgbuild-checksums` (digests matched the packaging the
repackager maintains), `checksum-file-over-tls` (the vendor's checksum
file, unsigned), `apt-release-gpg` (an apt index signed with the given
key), `vendor-signature` (a detached signature the vendor publishes),
`vendor-packslip` (the vendor's own packslip, verified; `detail` is its
sha256), `github-attestation` (GitHub artifact attestations verified),
`provenance-verified` (SLSA provenance verified against the builder),
`scan` (the artifacts were scanned; `detail` points at the report),
`none`.

A repackager document proves that the repackager published exactly these
digests and checked the listed evidence. It does not prove anything the
vendor did not sign. Consumers rank it below a vendor document, and a
consumer that already holds a vendor document for a project refuses to
replace it with a repackager one without a human's say-so.

A mirror is a repackager whose documents carry the vendor's digests with
the mirror's own URLs and `vendor-packslip` evidence. A consumer pointed at
a mirror gets the same bytes under the mirror's pin and can still fetch
and verify the vendor's document if it wants both.

## Signing

Both schemes produce the same file and are verified by the same code. A
vendor should prefer the first.

- `sigstore-oidc`: keyless. A CI job with an id-token permission signs
  with its own identity; Fulcio issues a short-lived certificate naming
  the workflow that ran, and Rekor logs the signature. There is no key to
  manage. `packslip create` does this by default when it finds an ambient
  CI credential (GitHub Actions, GitLab CI, and the others sigstore's
  clients know) or a token in `SIGSTORE_ID_TOKEN`.
- `sigstore-key`: a long-lived Ed25519 key from `packslip keygen`, kept
  in minisign's key-file format. The bundle carries a public-key hint and
  the Rekor entry, whose verifier is the public key. For vendors who
  release outside a CI system with OIDC, or who want a stable key their
  consumers pin. Consumers pin the public key, never the hint.

A key-signed bundle may be produced without a log entry
(`create --no-log`) for an air-gapped release. Consumers refuse such a
bundle unless they explicitly allow it (`verify --allow-unlogged`), and a
repository should record that choice per vendor.

A consumer that wants a dependency-free check has one: the DSSE signature
of a key-signed bundle is a raw Ed25519 signature over the
pre-authentication encoding of the payload.

## Discovery

Publish the bundle next to the artifacts: as a release asset, or under the
version directory of a download site.

Every project has a release list, and it is where consumers find what
was released and what was withdrawn. Two kinds exist.

### The signed list

A signed list is a bundle of the same shape as a packslip, with the
`releases/v1` predicate:

```json
{
  "_type": "https://in-toto.io/Statement/v1",
  "subject": [
    { "name": "https://dl.example.com/2026.9.1/packslip.sigstore.json",
      "digest": { "sha256": "...", "sha512": "..." } },
    { "name": "https://dl.example.com/2026.9.0/packslip.sigstore.json",
      "digest": { "sha256": "..." } }
  ],
  "predicateType": "https://packslip.dev/releases/v1",
  "predicate": {
    "project": "mise.jdx.dev",
    "generated_at": "2026-09-01T12:00:00Z",
    "expires_at": "2026-10-01T12:00:00Z",
    "sequence": 42,
    "identity": { "scheme": "sigstore-key", "key_id": "5A0A0B8B9C6D7E1F" },
    "releases": [
      { "version": "2026.9.1", "tag": "v2026.9.1", "published_at": "2026-09-01T12:00:00Z",
        "packslip": "https://dl.example.com/2026.9.1/packslip.sigstore.json",
        "security": true },
      { "version": "2026.9.0", "tag": "v2026.9.0", "published_at": "2026-08-20T12:00:00Z",
        "packslip": "https://dl.example.com/2026.9.0/packslip.sigstore.json",
        "status": "yanked", "status_reason": "CVE-2026-1234" }
    ]
  }
}
```

Each subject is a listed packslip's URL with the digest of that file, so
the list pins the exact documents it points at. `expires_at` and
`sequence` are borrowed from TUF's timestamp role: a consumer refuses a
list that has expired, or whose sequence is lower than one it has already
accepted, so a mirror cannot freeze or roll back the vendor's view.

Each entry carries the release's `version`, its `tag` as the vendor
spells it, and `published_at`, copied from the packslip. It may carry
`status: "yanked"` with a `status_reason` when the release was withdrawn,
`security: true` when it fixes a vulnerability, and, on a list from
someone other than the vendor, `evidence` saying what the publisher
checked. A consumer never selects a yanked release, warns when it holds
one, and may shorten its minimum release age for a security release.
Nothing else about a release lives on the list: its version says the rest.

The list is published at

```
https://<host>/.well-known/packslip/<path>.json
```

where `<host>` and `<path>` are the project name's host and the rest of
it, or `https://<host>/.well-known/packslip.json` when the name is a bare
host. For `mise.jdx.dev` that is
`https://mise.jdx.dev/.well-known/packslip.json`; for `jdx.dev/mise`,
`https://jdx.dev/.well-known/packslip/mise.json`; for
`github.com/jdx/mise`, `https://github.com/.well-known/packslip/jdx/mise.json`,
which github.com does not serve, and the next section says what a
consumer does there. A forge that does serve it needs nothing else.

The reference implementation gives the URL as `list_url`, for a project's
own list and for one another publisher keeps for it (see Lists from other
publishers), and the path of a GitHub repository's supplementary list as
`github_list_path`.

A project on its own domain must publish the list: a consumer that finds
none refuses the project rather than guessing at URLs. `packslip releases`
produces the list from local copies of the released bundles; the JSON
schema is at `https://packslip.dev/schema/releases-v1.json`.

The list separates the name from where the bytes live: the identity is
anchored to the domain, and the artifacts can be anywhere. A vendor with
only a git repository keeps the list and the bundles in it, at paths its
host serves as raw files, and points the artifact URLs wherever the bytes
are, an LFS store included. Nothing about a forge's release API is
required.

### GitHub

For `github.com/<owner>/<repo>[/<tool>]` the list is the repository's
releases endpoint, which every consumer can read without the vendor
publishing anything more:

- A release counts when it is not a draft and carries a packslip whose
  `project` matches.
- Its tag names its version, as Versions defines: the version, optionally
  after a `v`, and optionally after the tool's subpath, its last segment,
  or the repository name plus a separator. A consumer lists versions from
  tags without downloading a bundle per release, and on install verifies
  the packslip and refuses it when its `version` differs. A tag that names
  no version is invisible here.
- The endpoint's order and its prerelease flag are not consulted; the
  version says both.

The repository may also carry a signed list, at `.well-known/packslip.json`
or `.well-known/packslip/<tool>.json` on its default branch, which GitHub
serves at `https://raw.githubusercontent.com/<owner>/<repo>/HEAD/<path>`.
It is signed by the same identity the packslips are, and it is
supplementary: a version it names is taken as it says, yanked or flagged,
and pinned to the packslip digest it gives; a version it omits still comes
from the endpoint. So a vendor touches it only to withdraw a release, to
mark a security fix, or to list a release whose tag names no version, and
a vendor that never needs those never writes it. Withdrawing a release
this way works on a repository with immutable releases, where the release
and its packslip cannot be deleted.

### Lists from other publishers

A list need not come from the vendor. A registry, a mirror, or a scanning
service publishes lists under its own host, one per vendor project it
covers, at the well-known path with the vendor's project name:
`https://registry.example/.well-known/packslip/github.com/jdx/mise.json`,
which `list_url` gives for a publisher that is not the project's host.
A consumer configures which such hosts it trusts, one setting per host.

Each entry points at a packslip and pins its digest. The packslip may be
the vendor's own, signed by the vendor's identity, or a repackager
document the publisher signed; either is verified against the pin its
own `project` and `attested_by` imply. The list's signature proves who
selected these releases and what the entry's `evidence` says they
checked, not who built them. A consumer that trusts such a host selects
only versions the host lists, which is how a registry that scans releases
before admitting them, or an organization that curates versions, applies
its judgement without a per-tool recipe.

Such a list is a stamp on the releases it names. A consumer that trusts
one or more stamping hosts treats a version none of them lists as not
released: it is not offered and not installed, however valid the vendor's
own document is. Any one trusted host's stamp suffices. A user who
trusts a vendor outright says so for that project alone, and the
consumer then takes the vendor's document under the vendor's pin with no
stamp at all. A stamping host signs with whichever scheme fits it: a
service that stamps continuously holds a key, a registry that stamps
from a repository signs keylessly, and the consumer pins the host either
way.

## Versions

`version` is a semver 2.0.0 version: `MAJOR.MINOR.PATCH`, an optional
prerelease part after `-`, optional build metadata after `+`. Calver such
as `2026.9.1` qualifies. `packslip create` refuses anything else, and so
does a consumer. The tag can be spelled however the vendor likes
(`v2026.9.1`, `oxlint_v1.0.0`); `source.tag` carries it.

One required scheme is what lets everything about a release follow from
its signed version string, with no flag anywhere that could disagree with
it:

- Order is semver precedence. "Latest" is the highest eligible version, so
  a backport such as 20.19.1 published after 22.0.0 never masquerades as
  the newest release, and range constraints (`^1.2`) have meaning. Build
  metadata takes no part, as semver says. The order of the release list,
  GitHub's or the vendor's, is not consulted.
- A prerelease is a version with a prerelease part: `1.2.0-rc.1` is one,
  `1.2.0` is not. Consumers skip prereleases unless asked for them.
  Promoting a release candidate means cutting the final version, not
  editing a flag.
- A channel is the first identifier of the prerelease part, when that is
  not a number: `1.3.0-nightly.20260904` is on `nightly`, `1.2.0-beta.2` on
  `beta`, `1.2.0-rc.1` on `rc`. A release with no prerelease part is on no
  channel, and so is `1.2.0-1`. A consumer asked for a channel selects
  only releases on it, ranked by precedence. The names are the vendor's;
  packslip defines none.

Eligible means not yanked, not a prerelease unless prereleases were asked
for, and on the requested channel when one was given. A requested version
matches as a prefix on dot-separated components, so `20` and `3.12` mean
what people expect and `1.2.0-beta` means the betas of 1.2.0. A request
may also be the tag as the vendor spells it, with or without its leading
`v`; a consumer matches it against `source.tag` or the list entry's `tag`,
so a user who knows a release by the vendor's name still finds it.
Rollback protection comes from the release list's `sequence`, not from
anything a single release says.

A packslip carries none of this as a field, on purpose. It is signed once,
and on a GitHub repository with immutable releases it can never be
replaced, while GitHub's own prerelease flag stays editable after
publishing. A flag in the document would end up either frozen or
contradicted. So the packslip says what shipped, the version says how to
treat it, and the one thing that stays mutable, withdrawing a release, is
the release list's job. An earlier draft let a vendor declare list order
instead of semver; it went, because the prerelease and channel fields it
then needed had no honest home.

### Spelling a version

A vendor whose versions are not semver as written picks the semver
spelling once and keeps it; the tag keeps the vendor's own spelling.

- A prefix (`jq-1.7.1`, `release-1.2.3`) is the tag's business and is
  not part of the version.
- A missing patch component is `0`: `4.1` is `4.1.0`.
- Leading zeros go: `25.07.1` is `25.7.1`, and a date `2026.08.31` is
  the calver `2026.8.31`. A date with dashes, `2026-08-31`, is respelled
  the same way.
- A fourth ordered component has no semver spelling, so a scheme that
  needs one is not served by this document; build metadata does not order
  and a prerelease part means something else.

Two releases must not share a version. A vendor whose spelling would
collide, two releases on one day under a date scheme, adds a component it
can order rather than build metadata.

### Tags

A forge release's tag names its version when, after removing an optional
prefix and an optional `v`, what remains is the version, or spells it as
the rules above say. The prefix is the tool's subpath, the last segment
of the subpath, or the repository name, followed by `/`, `-`, `_`, or
`@`. So `v1.2.3` names `1.2.3`; `oxlint_v1.0.0` names `1.0.0` for
`github.com/oxc-project/oxc/oxlint`; `cli/v1.9.4` names `1.9.4` for
`github.com/biomejs/biome/crates/cli`; `jq-1.7.1` names `1.7.1` for
`github.com/jqlang/jq`; `v4.1` names `4.1.0`.

A consumer derives a version from a tag only to list releases without
fetching every bundle. The packslip is the authority: when its `version`
is not what the tag named, the consumer refuses the release. Of the
GitHub projects in the aqua registry, this names a version for 96 of every
100 from the latest tag alone; the rest publish a signed list, whose
entries carry the version outright.

## Selecting an artifact

A consumer selects one artifact for its host by the following rule,
which the reference implementation provides as `select_artifact`.

1. An artifact fits the host when each of its `os`, `arch`, and `libc`
   is either absent or equal to the host's. A host that does not know its
   libc takes only artifacts that name none. A consumer may add a
   fallback of its own, such as a `gnu` host taking a `musl` build or an
   `aarch64` macOS host taking `x86_64` under Rosetta, but it ranks the
   exact match first and says what it did.
2. With a requested variant, only artifacts carrying it fit; with none,
   only artifacts without one.
3. Only artifacts whose `format` the consumer handles fit.
4. Among those that fit, the artifact naming the most of `os`, `arch`,
   and `libc` wins, so a build for the host beats a portable one.
5. Among equally specific artifacts, the consumer's own format preference
   decides. A typical order is `tar.xz`, `tar.zst`, `tar.gz`, `tgz`,
   `tar.bz2`, `tar`, `zip`, `7z`, then the single compressed executables,
   then `raw`.
6. Two artifacts that still tie are a vendor error, and the consumer
   refuses to guess between them.

## Consumer rules

1. Pin the identity once. For a forge project, the name is the pin: accept
   only the forge's issuer and an identity under the repository. For other
   projects, pin the public key or identity from a list of pins you
   maintain, or from the well-known list on first use. A list from another
   publisher is trusted per host, by configuration. Never take a key from
   the document itself, and never trust a bundle's key hint.
2. Verify the bundle: signature, certificate chain and log entry as
   sigstore defines them, then the statement's structure, then the
   subject digest of every artifact or asset you downloaded, and the size
   of every artifact.
3. Enforce no-downgrade: refuse a release whose `identity.scheme` is
   weaker than the last accepted one, whose signer changed without a human
   saying so, whose `attested_by` went from vendor to repackager, or that
   dropped per-artifact provenance the last release carried. For a keyless
   signer, compare the workflow path, not the ref: a new tag of the same
   workflow is the same signer.
4. Apply any minimum release age to the log's integration time, falling
   back to `published_at` only for an unlogged bundle you chose to accept.
5. Use the project's release list: GitHub's releases endpoint with the
   repository's signed list if it has one, or the signed list at the
   well-known URL; refuse a project that has neither. Refuse a signed
   list that has expired or whose `sequence` is below the last one
   accepted; never select a yanked entry; skip prereleases unless asked
   for them; rank by semver precedence; refuse a packslip whose version is
   not the one its tag or list entry named. When you trust stamping
   hosts, select only versions one of them lists, unless the user chose
   the vendor alone for that project.
6. Select one artifact as Selecting an artifact says. Refuse to guess
   between two artifacts that tie.
7. Check `requires` against the host before installing, as Host
   requirements says: refuse when a library, glibc, or OS version means
   the executables cannot start, warn when a command is missing or too
   old, and report either in your own terms. Fail on nothing you cannot
   check. Between two artifacts that tie, prefer the one whose
   requirements the host meets.
8. For each thing the resources describe, as Resources defines one, keep
   the entries whose scope fits the selected artifact and the most
   specific of those, then take the most verifiable source offered in the
   order Resources gives; run an `exec` completion on demand, when a shell
   first asks, and cache it; run any other `exec` entry only if the user
   has chosen to run vendor code at install time; ignore kinds you do not
   know. A
   resource you cannot fetch is reported, not fatal; one whose digest is
   not the one the document signed fails the install.

## What a verified packslip proves

A verified packslip proves that the named signer published exactly this
list of artifacts, with these digests, at a time the log recorded. It does
not by itself prove anything about how the artifacts were built. That is
what SLSA provenance is for: an artifact whose linked provenance a
consumer verifies earns the SLSA build level its builder establishes
(GitHub-hosted runners with
[`actions/attest-build-provenance`](https://github.com/actions/attest-build-provenance) give
Build L2, or L3 when the build runs in a reusable workflow). Consumers
record what they verified as a
[SLSA Verification Summary](https://slsa.dev/spec/v1.0/verification_summary) or in their own
terms; packslip defines no level scale of its own.

A resource from an `archive` or `asset` is covered by the same digests;
one from `repo` by the commit; an `exec` entry by nothing beyond the
executable it runs.

`packslip verify` reports the scheme, the signer, who attested, the log
time, whether every artifact links provenance, the resources declared, and
what each artifact requires of the host. It does not fetch or verify the
provenance statements.

## Tooling

The reference implementation is the `packslip` crate and binary in
[jdx/packslip](https://github.com/jdx/packslip), also usable as a GitHub
Action.

- In a release job: `uses: jdx/packslip@v1` with `artifacts: dist/*`
  attests build provenance for the artifacts, signs the packslip
  keylessly, links the provenance from it, verifies the result, and
  uploads the bundle to the release. `bin` names the executables,
  `resources`, one `--resource` value per line, the rest, `require`, one
  `--require` value per line, the commands they need, and `manifest`
  points at a manifest for what those cannot say. A monorepo runs the
  step once per tool with `project: github.com/owner/repo/<tool>`.
- `packslip create --project NAME --version X --out dist --url-base URL
  --source-repo URL --tag vX --bin NAME artifact...` digests the artifacts,
  infers platforms from file names, and writes the signed bundle.
  - Platforms: `path:os/arch[/libc]` overrides what the file name implies;
    `path:any` marks an artifact that runs anywhere; `path@variant` marks
    a second build of one platform.
  - Executables: `--bin NAME` is looked up inside each archive, so it
    records the true path (`tool-1.2.3-linux-x64/tool`), and refuses a
    name the archive does not hold or holds twice at one depth. `--bin
    NAME=PATH` gives the path outright when the name on PATH differs from
    the file. For a bare executable, `--bin NAME` is the name the file
    gets on PATH.
  - URLs: `--url FILENAME=URL` sets one artifact's or asset's URL.
  - Provenance: `--provenance FILENAME=URL` links a provenance statement
    to one artifact; a bare URL applies to the artifact at the same
    position.
  - Metadata: `--notes-url`, `--no-sha512`, `--extension NAME=JSON` for a
    release-level extension, and `--attested-by repackager` with
    `--evidence KIND[=DETAIL]`.
  - Resources: `--resource KIND[/QUALIFIER]=SOURCE:VALUE`, as in
    `completion/zsh=archive:share/zsh/site-functions/_tool`,
    `completion/bash,zsh,fish=exec:tool completion {shell}`,
    `completion/bash,zsh,fish=exec:COMPLETE={shell} tool` (leading
    `NAME=value` words become `env`),
    `man=archive:man/man1/tool.1`, `cli-spec/usage=exec:tool usage`,
    `skill/NAME=repo:skills/tool`, `skill/NAME=asset:dist/tool-skill.tar.gz`,
    `sbom/cyclonedx=asset:dist/tool.cdx.json`, `desktop=archive:...`,
    `icon=archive:...`, or `app=archive:Tool.app`. A `cli-spec` describes
    the sole `--bin` unless named as `cli-spec/usage/NAME`. An `asset` is
    a local file digested into the subject; a file given as both an
    artifact and an asset is the asset.
  - Host requirements: `--require bin:NAME[@MIN]`, as in
    `--require bin:java@17`, for a command the executables need. What
    they load from the host is read from them and recorded as `libs`;
    `--no-libs` leaves the artifacts unopened. A manifest gives the same
    under `requires`, per artifact or for all.
  - A manifest: `--manifest release.toml` carries per-artifact
    executables, formats, requirements, platforms, variants, and URLs,
    plus resources with their scope, for a release the flags cannot
    describe. Its top-level `bin` and `requires` are the defaults every
    artifact inherits; an `[[artifact]]` entry overrides them for one
    file, `portable = true` clears the platform, and `format = "raw"`
    settles a `.exe`. Artifacts on the command line join those the
    manifest lists, the manifest's entry winning for a file both name,
    and flags win over the manifest's `project`, `version`, `url_base`,
    `notes_url`, `published_at`, `source`, and, key by key, `extensions`.
  - Signing: `--key release.key` signs with a key; `--no-log` skips Rekor.
  - On a forge project, `create` warns when `--tag` does not name
    `--version`, since consumers list from tags.
- `packslip keygen -o release.key` writes an Ed25519 secret seed (mode
  0600) and `release.pub`.
- `packslip verify BUNDLE [--artifact file...]` verifies and exits 1 on
  any failure; `--json` prints the result. A keyless bundle is checked
  against the policy its project name implies, or against `--identity`,
  `--identity-prefix`, and `--issuer`; a key-signed bundle against
  `--pubkey`. `--allow-unlogged` accepts a bundle with no log entry;
  `--trusted-root` replaces the embedded sigstore root. The same command
  verifies a release list.
- `packslip show BUNDLE` prints the statement.
- `packslip releases --project NAME --sequence N --valid-for 30d
  --release URL=PATH... [--yank URL=REASON] [--security URL]
  [--evidence URL=KIND[=DETAIL]] --key release.key` writes a signed
  release list, copying each bundle's version and tag.
- `packslip schema [--releases]` prints the JSON schemas.
