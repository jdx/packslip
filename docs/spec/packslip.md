# packslip: a signed release manifest

Version 1, draft. Predicate types `https://packslip.dev/release/v1` and
`https://packslip.dev/releases/v1`.

Author: Jeff Dickey ([@jdx](https://github.com/jdx)).

## Goal

A vendor publishes one signed, machine-readable document per release that
says what the artifacts are and how to verify them. A consumer, whether that
is [mise](https://mise.jdx.dev), [pacvamp](https://pacvamp.com), or a
corporate mirror, verifies it against one pinned identity or key. In return
it gets checksums, platform mapping, executables, provenance links, and
whatever else ships with the release: completions, man pages, a CLI spec, a
skill, a desktop entry.

packslip invents as little as it can. The document is an in-toto statement
in a sigstore bundle, the same shape GitHub artifact attestations, npm
provenance, and Homebrew bottles use. Identity comes from sigstore's
certificate authority and transparency log. What packslip adds is the
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
content is a DSSE envelope of type `application/vnd.in-toto+json` carrying
the statement below. Its verification material is the signer's Fulcio
certificate or a public-key hint, plus the Rekor transparency log entry for
the signature. Only an air-gapped key-signed release omits the log entry;
see Signing.

The bundle carries the statement, so the signed bytes are exactly the
payload bytes and a consumer needs no canonicalization step. `packslip show`
prints them; so does `jq -r .dsseEnvelope.payload | base64 -d`. `cosign`
and `gh attestation` understand the bundle as-is.

## The release statement

```json
{
  "_type": "https://in-toto.io/Statement/v1",
  "subject": [
    { "name": "mise-v2026.9.1-linux-x64.tar.xz",
      "digest": { "sha256": "...", "sha512": "..." } }
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
        "requires": { "glibc_min": "2.31" },
        "provenance": ["https://api.github.com/repos/jdx/mise/attestations/sha256:..."]
      }
    ],
    "resources": [
      { "kind": "completion", "shell": "zsh", "archive": "mise/share/zsh/site-functions/_mise" },
      { "kind": "man", "archive": "mise/man/man1/mise.1" },
      { "kind": "cli-spec", "format": "usage", "bin": "mise", "archive": "mise/share/usage/mise.kdl" },
      { "kind": "skill", "name": "mise", "repo": "skills/mise" }
    ],
    "identity": {
      "scheme": "sigstore-oidc",
      "key_id": "https://github.com/jdx/mise/.github/workflows/release.yml@refs/tags/v2026.9.1",
      "issuer": "https://token.actions.githubusercontent.com"
    },
    "notes_url": "https://github.com/jdx/mise/releases/tag/v2026.9.1",
    "sbom": "https://.../sbom.cdx.json"
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
- `project` is a name as defined above. `version` is semver 2.0.0; its
  prerelease part, if any, says whether the release is a prerelease and
  which channel it is on. See Versions. `published_at` is RFC 3339 UTC.
- `os`, `arch`, and `libc` use the values `linux`, `darwin`, `windows`,
  `freebsd`; `x86_64`, `aarch64`, `armv7`, `riscv64`, `i686`; `gnu`,
  `musl`. `format` is the archive or installer type: `tar.xz`, `tar.gz`,
  `tar.zst`, `tar.bz2`, `tgz`, `zip`, `7z`, `deb`, `rpm`, `dmg`, `pkg`,
  `msi`, `msix`, `exe`, `appimage`, or `raw` for a bare executable.
- `variant` tells apart artifacts that share os, arch, libc, and format:
  `fips`, `baseline`, `debug`, `installer`, `source`. A vendor must not
  publish two artifacts that agree on all five; `packslip create` refuses
  to. Consumers that find such a pair refuse to choose.
- `bin` lists the executables inside the artifact. Each entry is a path
  relative to the archive root, or the artifact's own name when it is a
  bare executable. When the name to put on PATH differs from the file name,
  the entry is `{ "path": "bin/oxlint-x86_64", "name": "oxlint" }`.
  Windows entries carry their `.exe`.
- `requires` states what the host needs: `os_min` in the OS's own terms
  (`12` for macOS Monterey, `10.0.17763` for Windows) and `glibc_min` for a
  `gnu` Linux build.
- `provenance` holds URLs of SLSA build provenance statements for that
  artifact. The packslip proves the manifest; verified provenance proves
  the build, at whatever SLSA build level its builder establishes.
- `resources` lists what the release ships besides its executables, each
  entry a `kind` and one source. See Resources.
- `notes_url` points at the release notes. `sbom` points at a software
  bill of materials for the release.
- `identity` says how the document is signed and by whom, so a consumer
  can check what it pinned against what it received. For `sigstore-oidc`,
  `key_id` is the certificate's subject identity (a workflow URI for CI, an
  email for a person) and `issuer` the OIDC issuer. For `sigstore-key`,
  `key_id` is the key id in uppercase hex.
- `attested_by` is `vendor` (default) or `repackager`. See below.

The JSON schema is printed by `packslip schema` and published at
`https://packslip.dev/schema/release-v1.json`.

### Resources

A release usually ships more than its executables: shell completions, a man
page, a spec of the CLI, an agent skill, and, for a desktop application, the
entry, icons, or app bundle a launcher needs. `resources` lists them. Each
entry names a `kind` and exactly one source. The sources differ in what a
consumer can verify, and a consumer prefers them in this order:

- `archive`: a path inside the artifact the consumer selected, relative to
  the archive root. The artifact's digest already covers it. Most vendors
  use one layout on every platform; one that does not uses another source
  for the platforms that differ.
- `asset`: a separate release file. It is listed in `subject` with its
  digest and the entry carries its download `url`, so it verifies exactly
  as an artifact does. A skill directory ships this way as an archive of
  its own.
- `repo`: a path in the source repository at `source.commit`, which pins
  its content. `source.commit` is required.
- `exec`: an argv whose first element is a `bin` name and whose stdout is
  the file, run once the executable is installed. Nothing verifies it
  beyond the executable itself, and it runs a freshly downloaded binary at
  install time rather than at first use. A consumer may decline to run
  anything; one that declines treats the entry as absent rather than
  failing. A vendor lists a static source first and an `exec` entry last.

Documented kinds:

- `completion`: a shell completion script. With a static source, `shell`
  names the shell: `bash`, `zsh`, `fish`, `powershell`, `nushell`,
  `elvish`. With `exec`, `shells` lists every shell the command generates
  and the argv carries a `{shell}` placeholder.
- `man`: a man page. The section is the file's suffix, as in `mise.1`.
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
- `skill`: an agent skill in the Agent Skills format: a directory holding
  `SKILL.md` and whatever it references, named by `name`. With `exec`, the
  command prints a single `SKILL.md`.
- `desktop`: a freedesktop desktop entry, for a Linux launcher. AppImage,
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

An artifact with `bin` is something a command-line package manager
installs. A release whose resources include `desktop` or `app` is something
a desktop launcher installs. Many applications are both, and the entries say
so without a category that would misfile them.

### Repackager attestation

A repository or mirror that redistributes a vendor's artifacts, and whose
vendor publishes no packslip, may sign one itself with
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
`github-attestation` (GitHub artifact attestations verified), `none`.

A repackager document proves that the repackager published exactly these
digests and checked the listed evidence. It does not prove anything the
vendor did not sign. Consumers rank it below a vendor document, and a
consumer that already holds a vendor document for a project refuses to
replace it with a repackager one without a human's say-so.

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
was released and what was withdrawn:

- For `github.com/<owner>/<repo>[/<tool>]` the list is the repository's
  releases endpoint. A release counts when it is not a draft and carries
  a packslip whose `project` matches. The endpoint's order and its
  prerelease flag are not consulted; the version says both. To yank a
  release, delete it, or delete its packslip asset where the repository
  still permits that.
- Any other project publishes a signed list at

```
https://<host>/.well-known/packslip/<path>.json
```

where `<path>` is the project name after the host, or `packslip.json`
directly under `.well-known` when the name is a bare host. The list is
required: a consumer that finds none refuses the project rather than
guessing at URLs. It is a bundle of the same shape as a packslip, with the
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
      { "version": "2026.9.1", "published_at": "2026-09-01T12:00:00Z",
        "packslip": "https://dl.example.com/2026.9.1/packslip.sigstore.json",
        "security": true },
      { "version": "2026.9.0", "published_at": "2026-08-20T12:00:00Z",
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

Each entry may carry `status: "yanked"` with a `status_reason` when the
vendor withdrew the release, and `security: true` when it fixes a
vulnerability. A consumer never selects a yanked release, warns when it
holds one, and may shorten its minimum release age for a security release.
Nothing else about a release lives on the list: its version says the rest.

`packslip releases` produces the list from local copies of the released
bundles; the JSON schema is at
`https://packslip.dev/schema/releases-v1.json`.

The list separates the name from where the bytes live: the identity is
anchored to the domain, and the artifacts can be anywhere.

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
what people expect and `1.2.0-beta` means the betas of 1.2.0. Rollback
protection comes from the release list's `sequence`, not from anything a
single release says.

A packslip carries none of this as a field, on purpose. It is signed once,
and on a GitHub repository with immutable releases it can never be
replaced, while GitHub's own prerelease flag stays editable after
publishing. A flag in the document would end up either frozen or
contradicted. So the packslip says what shipped, the version says how to
treat it, and the one thing that stays mutable, withdrawing a release, is
the release list's job. An earlier draft let a vendor declare list order
instead of semver; it went, because the prerelease and channel fields it
then needed had no honest home.

## Consumer rules

1. Pin the identity once. For a forge project, the name is the pin: accept
   only the forge's issuer and an identity under the repository. For other
   projects, pin the public key or identity from a list of pins you
   maintain, or from the well-known list on first use. Never take a key
   from the document itself, and never trust a bundle's key hint.
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
5. Use the project's release list (GitHub's releases endpoint, or the
   signed list) and refuse a project that has neither. Refuse a signed
   list that has expired or whose `sequence` is below the last one
   accepted; never select a yanked entry; skip prereleases unless asked
   for them; rank by semver precedence.
6. Select one artifact by os, arch, libc, format, and, when needed,
   variant. Refuse to guess between two artifacts that match.
7. Take each resource from the most verifiable source offered, in the
   order Resources gives; run an `exec` entry only if you have chosen to
   run vendor code at install time; ignore kinds you do not know.

## What a verified packslip proves

A verified packslip proves that the named signer published exactly this
list of artifacts, with these digests, at a time the log recorded. It does
not by itself prove anything about how the artifacts were built. That is
what SLSA provenance is for: an artifact whose linked provenance a
consumer verifies earns the SLSA build level its builder establishes
(GitHub-hosted runners with `actions/attest-build-provenance` give Build
L2, or L3 when the build runs in a reusable workflow). Consumers record
what they verified as a SLSA Verification Summary or in their own terms;
packslip defines no level scale of its own.

A resource from an `archive` or `asset` is covered by the same digests;
one from `repo` by the commit; an `exec` entry by nothing beyond the
executable it runs.

`packslip verify` reports the scheme, the signer, who attested, the log
time, whether every artifact links provenance, and the resources declared.
It does not fetch or verify the provenance statements.

## Tooling

The reference implementation is the `packslip` crate and binary in
[jdx/packslip](https://github.com/jdx/packslip), also usable as a GitHub
Action.

- In a release job: `uses: jdx/packslip@v1` with `artifacts: dist/*`
  attests build provenance for the artifacts, signs the packslip
  keylessly, links the provenance from it, verifies the result, and
  uploads the bundle to the release. `bin` names the executables and
  `resources`, one `--resource` value per line, the rest. A monorepo runs
  the step once per tool with `project: github.com/owner/repo/<tool>`.
- `packslip create --project NAME --version X --out dist --url-base URL
  --source-repo URL --tag vX --bin NAME artifact...` digests the artifacts,
  infers platforms from file names, and writes the signed bundle.
  - Platforms: `path:os/arch[/libc]` overrides what the file name implies;
    `path@variant` marks a second build of one platform.
  - Executables: `--bin NAME=PATH` when the name on PATH differs from the
    file.
  - URLs: `--url FILENAME=URL` sets one artifact's or asset's URL.
  - Metadata: `--notes-url`, `--no-sha512`, and `--attested-by repackager`
    with `--evidence KIND[=DETAIL]`.
  - Resources: `--resource KIND[/QUALIFIER]=SOURCE:VALUE`, as in
    `completion/zsh=archive:share/zsh/site-functions/_tool`,
    `completion/bash,zsh,fish=exec:tool completion {shell}`,
    `man=archive:man/man1/tool.1`, `cli-spec/usage=exec:tool usage`,
    `skill/NAME=repo:skills/tool`, `skill/NAME=asset:dist/tool-skill.tar.gz`,
    `desktop=archive:...`, `icon=archive:...`, or `app=archive:Tool.app`. A
    `cli-spec` describes the sole `--bin` unless named as
    `cli-spec/usage/NAME`. An `asset` is a local file digested into the
    subject; a file given as both an artifact and an asset is the asset.
  - Signing: `--key release.key` signs with a key; `--no-log` skips Rekor.
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
  --release URL=PATH... [--yank URL=REASON] [--security URL] --key
  release.key` writes a signed release list.
- `packslip schema [--releases]` prints the JSON schemas.
