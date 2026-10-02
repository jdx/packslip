# packslip: a signed release manifest

Version 1. Predicate types `https://packslip.dev/release/v1` and
`https://packslip.dev/releases/v1`.

Authors: Jeff Dickey ([@jdx](https://github.com/jdx)) and Shunsuke Suzuki
([@suzuki-shunsuke](https://github.com/suzuki-shunsuke)).

## Reading this specification

This document defines two predicates: `release/v1` describes one release;
`releases/v1` indexes releases and carries mutable discovery metadata.
Both are signed in-toto statements in sigstore bundles.

A *vendor* is the project whose releases a packslip describes, which
normally builds and signs them itself. A *consumer* is anything that
verifies or installs releases, such as a package manager or an install
script. A *packslip*, or release manifest, is the sigstore bundle
published with a release; its *statement* is the in-toto statement
inside it. A *release list*, or signed list, is a bundle whose statement
has the `releases/v1` predicate. A *repackager* is a third party that
signs a packslip for a vendor's files, as
[Repackager attestation](#repackager-attestation) describes.

For a working example, start with the [guides](https://packslip.dev/docs/).
Implementers should read the [consumer rules](#consumer-rules) alongside
the field definitions. JSON examples use abbreviated digests and commits
for readability; those placeholders are not valid release data.

- [Identity and signing](#names): project names, bundle encoding, signers,
  forge identity, signer fingerprints, and what a verified packslip proves.
- [Release data](#the-release-statement): artifacts, resources, host
  requirements, extensions, and repackager documents.
- [Discovery](#discovery) and [versions](#versions): finding and
  selecting releases.
- [Selecting an artifact](#selecting-an-artifact): choosing the file for
  a host.
- [Consumer rules](#consumer-rules): verification, remembered trust, and
  installation.
- [Stability](#stability): what version 1 fixes and what a revision may
  add.
- [Tooling](#tooling): the reference implementation, task guides, and the
  [conformance vectors](https://github.com/jdx/packslip/tree/main/tests/conformance).

## Goal

A publisher describes a release once, in a signed document that any
consumer can verify against a trusted identity or key. The document lists
artifacts and their digests, platforms, executable paths, resources, and
provenance links. Artifacts may be archives, installers, bare executables,
source tarballs, or other release files.

The release manifest lets consumers interpret the release without a
per-vendor file name recipe or registry entry. It does not prescribe a
package manager, download host, or installation directory.

packslip uses an
[in-toto statement](https://github.com/in-toto/attestation/blob/main/spec/v1/statement.md)
inside a [sigstore bundle](https://github.com/sigstore/protobuf-specs).
The predicate carries the release metadata; the bundle carries the
signature and verification material.

## Names

A project name is a host with an optional path, such as
`github.com/jdx/mise`, `gitlab.com/group/tool`, or `mise.jdx.dev`.
It has no URL scheme or trailing slash. The host is lowercase and contains
at least one dot; path segments cannot be empty, `.` or `..`.
A short-name alias such as `mise` for `github.com/jdx/mise` is a consumer
convenience, not part of the format.

The name is the location and, on a forge, says who may sign:

- `github.com/<owner>/<repo>`: releases and their packslips are GitHub
  release assets, and the packslip is expected to be signed by a workflow
  of that repository through GitHub's OIDC issuer
  (`https://token.actions.githubusercontent.com`).
- `gitlab.com/<path>`: likewise, signed by a pipeline of that project
  through `https://gitlab.com`. GitLab subgroups make paths arbitrary
  depth, so the whole path names the repository.
- Any other host, including a forge this version does not name: the
  vendor controls the domain and publishes a release list at the
  well-known URL [The signed list](#the-signed-list) defines, signed with
  the key or identity the consumer pins.

For a known forge, a consumer derives the initial signer policy from the
project it intends to install, not from the project an untrusted
statement claims: a policy taken from the statement accepts whatever
repository the statement's author names, and so does not check the
user's intended identity. After verifying, the consumer must also check
that the statement names that project, or the same repository under
another name as [Forge identity](#forge-identity) allows. A forge name
can change hands, so the name locates the project and the forge's
repository ID pins it.

### Monorepos

On `github.com`, a repository that releases several tools names each one
with a subpath: `github.com/oxc-project/oxc/oxlint`,
`github.com/bazelbuild/buildtools/buildifier`,
`github.com/biomejs/biome/crates/cli`. On `gitlab.com` the whole path
names the repository, as [Names](#names) says, so a GitLab project has no
monorepo subpath.

Each tool gets its own packslip per release, with its own `version`, and
`source.tag` carries the real tag (`oxlint_v1.0.0`, `cli/v1.9.4`), so
nobody has to guess how a tag maps to a version. The identity pin is
still the repository: any workflow of `oxc-project/oxc` may sign a
packslip for `oxc-project/oxc/oxlint`.

When several tools share one GitHub release, each ships its own bundle,
named `packslip.<subpath>.sigstore.json` with `/` in the subpath replaced
by `-` (`packslip.oxlint.sigstore.json`, `packslip.crates-cli.sigstore.json`).
A repository's own packslip stays `packslip.sigstore.json`. Consumers do
not trust the file name: they read the `packslip*.sigstore.json` assets
of a release and keep the one whose `predicateType` is `release/v1` and
whose `project` is the name they asked for, or the same tool of the repository renamed. A tool keeps its subpath
across a rename: `github.com/old/repo/tool` becomes
`github.com/new/repo/tool`, never another tool of the repository.

## The file

A release ships one file per project: a
[sigstore bundle](https://github.com/sigstore/protobuf-specs) (v0.3). Its
content is a [DSSE](https://github.com/secure-systems-lab/dsse) envelope of type
`application/vnd.in-toto+json` carrying the statement that
[The release statement](#the-release-statement) defines. Its
verification material is the signer's [Fulcio](https://docs.sigstore.dev/certificate_authority/overview/)
certificate or a public-key hint, plus the [Rekor](https://docs.sigstore.dev/logging/overview/)
transparency log entry for the signature. The log entry may be left out
only for a key-signed release made without network access; see
[Signing](#signing).

The signature covers the DSSE payload bytes, so consumers do not
canonicalize the JSON before verification. `packslip show` decodes and
pretty-prints the statement without verifying it; `packslip show --raw`
prints the signed payload followed by a newline. The payload can also be
decoded with `jq -r .dsseEnvelope.payload BUNDLE | base64 -d`.
General-purpose sigstore tools can read the bundle; consumers must also
validate the packslip predicate and apply this specification's rules.

## Signing

A packslip is signed under one of two schemes, named in
`identity.scheme`. Both produce the same bundle format. Prefer keyless
signing when a supported CI identity is available.

- `sigstore-oidc`: keyless. A CI job with an id-token permission signs
  with its own identity; Fulcio issues a short-lived certificate naming
  the workflow that ran, and Rekor logs the signature. There is no key to
  manage. `packslip create` signs this way unless given `--key`. It takes
  the token from `SIGSTORE_ID_TOKEN` when that is set, or else from the
  CI job's ambient credential (GitHub Actions, GitLab CI, and the other
  providers sigstore's clients detect), and fails when it finds neither.
- `sigstore-key`: a long-lived Ed25519 key. `packslip keygen` writes the
  secret key as a hex seed and the public key in minisign's public-key
  format, which is the file consumers pin. The bundle carries a
  public-key hint and the Rekor entry, whose verifier is the public key.
  This scheme suits vendors who release outside a CI system with OIDC, or
  who want a stable key their consumers pin. Consumers pin the public
  key, never the hint.

A key-signed bundle may be produced without a log entry
(`packslip create --no-log`) for an air-gapped release. A consumer refuses
such a bundle unless it explicitly allows unlogged bundles
(`packslip verify --allow-unlogged` in the reference CLI), and a
repository should record that choice per vendor.

A consumer that wants a dependency-free check has one: the DSSE signature
of a key-signed bundle is a raw Ed25519 signature over the
pre-authentication encoding of the payload, with payload type
`application/vnd.in-toto+json`. The consumer still verifies the Rekor
entry as [consumer rule 2](#consumer-rules) requires, unless it chose to
accept an unlogged bundle.

### Reusable workflows

For a keylessly signed project, a consumer compares the workflow that
signed a release with the one that signed the last release it accepted.
It treats a different workflow of the same repository as a changed signer
and refuses the release until a person approves it
([consumer rule 3](#consumer-rules)). A vendor whose releases are signed
by more than one workflow of its repository, such as a reusable workflow
shared by several release jobs, would make every consumer ask a person to
approve each change of signing workflow. Such a vendor can declare in
its signed `identity` that consumers should hold it to the repository
instead:

```json
"identity": {
  "scheme": "sigstore-oidc",
  "key_id": "https://github.com/example/tool/.github/workflows/release.yml@refs/tags/v1.2.0",
  "issuer": "https://token.actions.githubusercontent.com",
  "pin_workflow": false
}
```

`pin_workflow` defaults to `true`, so a document that omits it is held to
its signing workflow. `packslip create --no-pin-workflow` and
`packslip releases --no-pin-workflow` write `false`; both refuse the flag
with `--key`. The field is a signed statement of the vendor's own, in a
release or a release list, and it means nothing for `sigstore-key`, which
has no workflow.

With `pin_workflow: false`, a consumer still requires everything else of
the signer: the issuer, a certificate whose repository is the one pinned
(by repository ID where the forge records one, as
[Forge identity](#forge-identity) describes), and a workflow of that
repository. It stops comparing the workflow's path with the last
accepted signer.

Declaring it lowers what the consumer enforces, so it is a reduction in
remembered trust like those in [consumer rule 3](#consumer-rules). A
consumer remembers the value beside the signer. A release that declares
`false` when the remembered value is `true` is refused until a person
approves it, and the consumer then remembers `false` and does not ask
again. With no remembered signer, as on a first install, there is nothing
to reduce. A release that declares `true` or nothing when the remembered
value is `false` is not a reduction: the consumer holds it to the
workflow of the last accepted signer again, and a different workflow is a
changed signer as usual.

A consumer written before this field existed ignores it, as it ignores
any field it does not know, and keeps holding the vendor to its workflow.
Those consumers therefore still refuse a release from a changed workflow
until a person approves it, even when the vendor sets the field.

### Forge identity

A repository can be renamed or moved to another owner, and the forge
redirects the old name to the new one: `github.com/jdx/rtx` became
`github.com/jdx/mise`, and GitHub answers for the first with the second.
Once the old name is free, anyone can create a repository under it, with
a workflow at the same path. The name cannot tell these apart. The
forge's repository ID can: it never changes, survives a rename and a
transfer to another owner, and is never reused.

Fulcio records the IDs in every certificate it issues to a GitHub Actions
or GitLab CI job, from the job's OIDC token:

| Extension | OID | GitHub claim | GitLab claim |
| --- | --- | --- | --- |
| Source Repository URI | `1.3.6.1.4.1.57264.1.12` | server URL + `repository` | server URL + `project_path` |
| Source Repository Identifier | `1.3.6.1.4.1.57264.1.15` | `repository_id` | `project_id` |
| Source Repository Owner URI | `1.3.6.1.4.1.57264.1.16` | server URL + `repository_owner` | server URL + `namespace_path` |
| Source Repository Owner Identifier | `1.3.6.1.4.1.57264.1.17` | `repository_owner_id` | `namespace_id` |

Each value is a DER UTF8String; an empty value means the forge supplied
none. The repository ID survives a rename and a transfer. The owner
extensions record who owned the repository when the certificate was
issued. They are not part of a project's identity, and a consumer neither
remembers nor compares them: only a repository's current owner can
transfer it, and that owner already controls what the repository signs.
The IDs speak for a packslip only when the Source Repository URI is
the repository its `project` names and the signer is a workflow of that
repository.

A consumer remembers the repository ID of the first release it accepts,
with its signer (see [Consumer rules](#consumer-rules)), and compares each
later release with it:

- **Same.** The statement names the requested project and the certificate
  carries the pinned repository ID. Accept it.
- **Renamed.** The statement names another repository on the same forge,
  with the same monorepo subpath, and the certificate carries the pinned
  repository ID. Accept it and report the name it was signed under. That
  covers a repository renamed or transferred to another owner: a release
  after the move, under the new name, and one from before it, under the
  old name, whichever name the user asks for. Signer continuity compares
  the workflow's path inside the repository: `.github/workflows/release.yml`
  of `jdx/rtx` continues as the same file of `jdx/mise`, and of `jdx/hk`
  as the same file of `acme/hk`.
- **Different repository.** A repository ID other than the pinned one.
  Refuse it, even under the requested name: that is what a name
  recreated after a rename, a transfer, or a deletion looks like.
- **No ID to compare.** A certificate that carries no IDs, from before
  Fulcio recorded them, is checked by name alone. A statement for another
  name with no ID to compare, on either side, is refused, as it would be
  with no pin.

A pin taken from an accepted release records the name it was signed under
and its repository ID. A pin recorded by a consumer that also stored the
owner's ID, as an earlier revision of this section asked, is read the
same way, with that ID ignored.

A consumer can hold more than one pin for a project, such as its own
record and a lockfile's commitment. It holds each release to every one of
them, and a refusal says which one the release disagreed with.

With no pin, the consumer has only the forge's word, and first use is
trust on first use. GitHub's `GET /repos/<owner>/<repo>` redirects a
renamed or transferred repository's old name to the repository and
returns its `id`, and a consumer may take that as the expected repository
ID. If the name was recreated before the consumer first saw it, the forge
answers for the new repository and nothing in the release says otherwise.
Two things limit that exposure. A lockfile that records the repository ID carries the pin
to every machine that reads it, and a
[signer fingerprint](#signer-fingerprint) the project publishes lets a
consumer pin the repository before its first install.

The reference implementation reads the extensions as
`sigstore::source_repository`, classifies a release into these cases in
`forge::check`, and verifies and classifies a bundle in one call with
`verify_forge`. `forge::same_workflow` compares two signers a consumer
recorded, with the repository ID recorded alongside each, for
[consumer rule 3](#consumer-rules) when no release is at hand.

### Signer fingerprint

A signer fingerprint is a short, fixed name for the repository a keyless
project is signed from. A project can publish it where users read it
independently of its releases, such as its README or install
instructions, and a user can record it in a Dockerfile or a consumer's
lockfile.
A consumer that starts from it does not have to trust the first release
it sees (see [Consumer rules](#consumer-rules)). It looks like this:

```text
ps1_snirenkjwr7m5ozgcufameodnm
```

It is `ps1_` followed by the first 128 bits of

```text
SHA-256( "packslip-signer-v1" 0x00 issuer 0x00 repository_id )
```

written as 26 characters of RFC 4648 base32, in lowercase and without
padding (`a`–`z`, `2`–`7`). `0x00` is a single zero byte and the quoted
string is its ASCII bytes. The two inputs are the ones the certificate
records, taken as they are, with no case or slash normalization:

- `issuer` is the certificate's OIDC issuer URL, such as
  `https://token.actions.githubusercontent.com` or `https://gitlab.com`.
- `repository_id` is the Source Repository Identifier (OID
  `1.3.6.1.4.1.57264.1.15`, see [Forge identity](#forge-identity)) as the
  string of decimal digits it is.

For GitHub repository `jdx/hk`, whose ID is `922514152`, the input is
`packslip-signer-v1`, a zero byte, `https://token.actions.githubusercontent.com`,
a zero byte, and `922514152`, and the fingerprint is
`ps1_snirenkjwr7m5ozgcufameodnm`.

Twenty-six base32 characters hold 130 bits, so the last character carries
two padding bits, which are zero. Text whose last character sets either
bit is not a fingerprint, so each fingerprint has exactly one spelling.

The fingerprint leaves out the repository's name, its monorepo subpaths,
and everything about the signer that can change while the repository
stays the same:

- **Owner.** A repository keeps its ID when it moves to another owner, so
  its fingerprint does too. A consumer does not remember or compare
  owners, as [Forge identity](#forge-identity) says, and the fingerprint
  adds none.
- **Name.** The ID survives a rename, so a pin does not go stale with one.
- **Workflow and ref.** A replaced release workflow, or a release signed
  from a new tag, still comes from the same repository. Whether the
  workflow changed is the signer-continuity check of
  [Consumer rules](#consumer-rules), which the fingerprint neither
  performs nor replaces.
- **Monorepo subpath.** The subpath is part of the project, which says
  which tool, and not of the signer: every tool of a repository is signed
  from the same repository, so one fingerprint covers them, and the
  project name in the statement says which one a release is.

When a repository is deleted and someone else creates one under its
name, the new repository has a new ID and so a different fingerprint.
That is what makes the fingerprint a pin.

A release has no fingerprint when it is signed with a key, when its
certificate records no repository ID (because it predates the extension
or the issuer does not give one), or when the recorded ID is not a string
of decimal digits. A project whose releases have no fingerprint is pinned
with an identity and issuer, or a public key, as
[Consumer rules](#consumer-rules) says.

A consumer that holds a fingerprint checks it against a release after
verifying the bundle ([consumer rule 2](#consumer-rules)), using only
what the verified certificate records, never what the statement says of
itself:

1. Read the issuer and the Source Repository Identifier from the
   certificate. A certificate that records no repository ID matches no
   fingerprint, and the release is refused.
2. Compute the fingerprint of the two and compare it with the one it
   holds. If they differ, the release is from another repository or
   through another issuer, and it is refused.
3. Apply [Forge identity](#forge-identity) as for any release: the
   certificate's Source Repository URI must be the repository the
   statement names, and the signer a workflow of it. The fingerprint says
   which repository; these say the statement is about it.

A pin that does not start with `ps1_`, is not 26 lowercase base32
characters after it, or whose last character sets a padding bit, is
refused as a pin, not ignored and not treated as "no pin". A consumer that
sees a prefix it does not know, such as `ps2_`, does not guess at what it
commits to.

The fingerprint is a commitment, not a secret. It is long enough that
nobody can find another repository with the same one, and short enough to
read in a README. It carries no authority by itself: the project that
publishes it, and the place it is published, are what the consumer
trusts. It does not belong in a packslip, which cannot vouch for its own
signer.

The [conformance vectors](https://github.com/jdx/packslip/tree/main/tests/conformance)
for the fingerprint give inputs, their fingerprints, texts that are not
fingerprints, and matches and mismatches. The reference implementation
computes one with `Fingerprint::of_signer`, checks one with
`Fingerprint::verify`, prints a release's with `packslip pin`, and checks
a release against one with `packslip verify --pin`.

## What a verified packslip proves

A verified packslip authenticates the named signer's statement about the
release. A downloaded file whose digest matches the statement is the file
the signer described. A logged signature also has a verified integration
time. An explicitly accepted unlogged signature has no such log evidence.

The release manifest does not establish how an artifact was built or
whether it is safe. Linked SLSA provenance must be fetched and verified
separately against the builder's identity and the consumer's policy. A
provenance URL alone establishes no build level.

Resources from `archive` and `asset` sources are covered by signed
digests. A `repo` resource is pinned by the source commit. An `exec`
resource's output is not separately signed; the consumer verifies the
executable and controls when it runs.

A single release manifest does not establish freshness, detect a
withdrawal, or prevent rollback. Those checks require discovery metadata
and remembered consumer state, as [Discovery](#discovery) and
[Consumer rules](#consumer-rules) define.

`packslip verify` checks the supplied bundle and local files. It reports
signing information, provenance links, resources, and host requirements,
but does not fetch provenance, install resources, or maintain trust
history across invocations. Without `--pubkey`, `--identity`,
`--identity-prefix`, or `--issuer`, it derives the signer policy from the
GitHub or GitLab project the statement claims. That is a policy taken
from an untrusted statement, which [Names](#names) warns does not check
the user's intended identity: compare the reported project and version
with the request, or pass the expected signer explicitly.

## The release statement

A release statement is an in-toto statement with `predicateType`
`https://packslip.dev/release/v1`. This one describes a mise release with
one Linux x86_64 archive, its resources, and an SBOM asset:

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

The following rules apply to the decoded release statement:

- `subject` lists every artifact by file name with its digests, plus any
  separate file a resource comes from. `artifacts` carries the same names
  with platform, size, download URL, format, executables, requirements, and
  provenance links. Every artifact is a subject, every subject is an
  artifact or a resource's `asset`, and neither list contains a duplicate.
  At least one artifact is required. `sha256` is required and is 64
  lowercase hex characters; `sha512` is optional and 128.
- `project` is a name as [Names](#names) defines. `version` is
  [semver 2.0.0](https://semver.org/spec/v2.0.0.html); its prerelease
  part, if any, says whether the release is a prerelease and which
  channel it is on. See [Versions](#versions). `published_at` is
  [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339) UTC.
- `source` says where the release was built from: `repo`, the repository
  URL (required when `source` is present); `commit`; and `tag`, as the
  vendor spells it. A `repo` resource requires `commit`, and a consumer
  matches a requested tag against `tag` or the release-list entry's `tag`
  (see [Matching a request](#matching-a-request)).
- `os`, `arch`, `libc`, `format`, and `variant` are lowercase words of
  letters, digits, `_`, `-`, and `.`, starting with a letter or digit.
  The documented values are the ones consumers know; see
  [Vocabularies](#vocabularies). A value outside them is well-formed but
  matches no host and unpacks with nothing, so a vendor uses one only for
  a platform or format this document has not named yet. An absent `os`,
  `arch`, or `libc` means the artifact does not depend on it: a universal
  macOS binary has `os` but no `arch`; a statically linked Linux
  executable, which loads no C library from the host, has no `libc`; and
  a script or a jar has none of the three.
- `format` is the archive or installer type, or `raw` for a bare
  executable. Two artifacts that differ only in format carry the same
  build, and a consumer takes whichever it prefers. A vendor must not
  publish two artifacts that agree on `os`, `arch`, `libc`, `variant`,
  and `format`; `packslip create` refuses to, and a consumer that finds
  such a pair refuses to choose.
- `size` is the artifact's length in bytes, checked with its digest;
  `url`, when present, is where to download it.
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
- `requires` states what the host must already provide: `os_min`, the
  lowest OS version in the OS's own terms (`12` for macOS Monterey,
  `10.0.17763` for Windows); `glibc_min`, for a `gnu` Linux build;
  `libs`, the shared libraries the executables load; and `bin`, the
  commands they run. See [Host requirements](#host-requirements).
- `provenance` holds URLs of [SLSA build provenance](https://slsa.dev/spec/v1.0/provenance)
  statements for that artifact. The packslip proves what the release
  statement says; verified provenance proves the build, at whatever [SLSA build level](https://slsa.dev/spec/v1.0/levels)
  its builder establishes.
- `resources` lists what the release ships besides its executables, each
  entry a `kind` and one source. See [Resources](#resources).
- `notes_url` points at the release notes.
- `extensions` carries what the specification has no field for, keyed by
  who defines it. See [Extensions](#extensions).
- `identity` says how the document is signed and by whom, so a consumer
  can check what it pinned against what it received. For `sigstore-oidc`,
  `key_id` is the certificate's subject identity (a workflow URI for CI, an
  email for a person) and `issuer` the OIDC issuer. For `sigstore-key`,
  `key_id` is the key id in uppercase hex. See [Signing](#signing).
- `attested_by` is `vendor` (default) or `repackager`; a repackager says
  what it checked in `evidence`. See
  [Repackager attestation](#repackager-attestation).

The JSON schema is printed by `packslip schema` and published at
`https://packslip.dev/schema/release-v1.json`. It checks field types and
required fields, the token patterns, the semver grammar, and the digest
lengths. The validator in the reference implementation enforces the
rest, including the project-name grammar, the fixed `_type` and
`predicateType`, at least one artifact, and the cross-references between
`subject`, `artifacts`, and resources.

### Vocabularies

The documented `os`, `arch`, and `libc` values follow the components of
Rust's target triples, so a vendor's build matrix maps onto them without
a translation table.

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

Every artifact carries a `format`. Selecting an artifact keeps only the
formats the consumer handles, and no consumer handles an absent one, so
an artifact without it could never be selected: it would claim a place
in the release while being unusable. A vendor whose file name says
nothing states the format outright, and a file that is not an artifact
at all (a shared library, a header, a checksum sidecar) is left out of
`artifacts` rather than published as one. The value is not a closed set:
a vendor may name a type this specification does not, as the token rule
in [The release statement](#the-release-statement) says, and consumers
skip what they cannot open.

### Field reference

Required fields inside an optional object are required when that object is
present. For example, `source` is optional, but a supplied `source` must
include `repo`. Resource entries must choose exactly one source field.

| Field | Type | Presence | Meaning |
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
| `artifacts[].format` | string | required | Archive, compression, or installer type, or `raw` for a bare executable. |
| `artifacts[].bin[]` | array of string or object | optional | Executables inside the artifact: a path from the archive root, or `{ path, name }` when the PATH name differs. A name is the command as typed, without `.exe`. |
| `artifacts[].requires` | object | optional | What the host must provide. See [Host requirements](#host-requirements). |
| `requires.os_min` | string | optional | Minimum OS version in the OS's own terms. |
| `requires.glibc_min` | string | optional | Minimum glibc for a `gnu` Linux build. |
| `requires.libs[]` | array of string | optional | Shared libraries loaded from the host, by loader name (`libssl.so.3`, `vcruntime140.dll`). Read from the executables by `packslip create`; empty means none needed, absent means unchecked. |
| `requires.bin[]` | array of object | optional | Commands the executables need on PATH: `{ name, min? }`, a bare name and the lowest version that works. |
| `artifacts[].provenance[]` | array of string | optional | URLs of SLSA build provenance statements for this artifact. |
| `predicate.resources[]` | array of object | optional | What ships besides the executables. See [Resources](#resources). |
| `resources[].kind` | string | required | `completion`, `man`, `cli-spec`, `skill`, `sbom`, `desktop`, `icon`, `app`, or a kind consumers may not know yet. |
| `resources[].artifact` | string | optional | Exact artifact file name. Limits this resource to that artifact and outranks platform-only scope. |
| `resources[].os`, `arch`, `libc` | string | optional | Limit the entry to artifacts of that platform, when layouts differ. |
| `resources[].archive` | string | one source | Path inside the selected artifact, from the archive root. |
| `resources[].asset` | string | one source | Name of a separate release file, listed in `subject` with its digest. |
| `resources[].url` | string | optional | Download URL of the asset. Only with `asset`. |
| `resources[].repo` | string | one source | Path in the source repository at `source.commit`, which is then required. |
| `resources[].exec[]` | array of string | one source | An argv whose first element is a `bin` name and whose stdout is the file. See [Running an exec entry](#running-an-exec-entry). |
| `resources[].env` | object of string | optional | Environment variables for the command, with `{shell}` substituted in values as in the argv. Only with `exec`. |
| `resources[].shell` | string | completion | The shell a static completion is for. |
| `resources[].shells[]` | array of string | completion | Every shell an `exec` completion generates, substituted for `{shell}` in the argv. |
| `resources[].format` | string | cli-spec, sbom | The spec format (`usage`) or the SBOM format (`cyclonedx`, `spdx`). |
| `resources[].bin` | string | cli-spec, completion, man | The executable the entry is for, by its `bin` name. Required for a `cli-spec`; for a `completion` or `man`, required when the release has more than one executable, and meaning that one when it has one. |
| `resources[].name` | string | skill | The skill's name. |
| `predicate.identity.scheme` | string | required | `sigstore-oidc` or `sigstore-key`. |
| `predicate.identity.key_id` | string | required | The certificate identity, or the key id in uppercase hex. |
| `predicate.identity.issuer` | string | optional | The OIDC issuer, for `sigstore-oidc`. |
| `predicate.identity.pin_workflow` | boolean | optional | For `sigstore-oidc`: `false` asks consumers to hold later releases to the signing repository, not to the workflow that signed this one. `true` when absent. See [Reusable workflows](#reusable-workflows). |
| `predicate.attested_by` | string | optional | `vendor` (default) or `repackager`. See [Repackager attestation](#repackager-attestation). |
| `predicate.evidence[]` | array of object | optional | What a repackager checked: `{ kind, detail? }`. |
| `predicate.notes_url` | string | optional | URL of the release notes. |
| `extensions` | object | optional | Vendor- or consumer-defined data, keyed by who defines it, on the release, each artifact, each resource, the release list, and each list entry. See [Extensions](#extensions). |

## Resources

`resources` describes files and generated content associated with the
release: completions, man pages, CLI specifications, agent skills, SBOMs,
and desktop integration files. Each entry has a `kind` and exactly one
source.

An artifact with `bin` is something a command-line package manager
installs. A release whose resources include `desktop` or `app` is something
a desktop launcher installs. Many applications are both, and the entries say
so without a category that would misfile them.

### Source types

A resource comes from one of four sources, listed in the order a consumer
prefers them once scope and specificity have narrowed the entries (see
[Scope and identity](#scope-and-identity)):

- `archive`: a path inside the artifact the consumer selected, from the
  true archive root. The artifact's digest already covers it. Only for an
  artifact that has paths inside it, which a bare format does not; see
  [Scope and identity](#scope-and-identity).
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
  binary and ship no file. [Running an exec entry](#running-an-exec-entry)
  says when a consumer runs it. A vendor with a static file lists it first.

### Scope and identity

Layouts differ by platform more often than by file, so an entry may carry
`os`, `arch`, or `libc` to say which artifacts it describes; an entry
without them describes every artifact. A resource applies to the selected
artifact when each field it carries equals the artifact's. An entry may
also name `artifact`, the exact file name in `artifacts`, when archives for
the same platform have different layouts or a resource belongs to one
variant. That artifact must exist and match any platform scope on the
entry. For example, a man page that a `fips` variant keeps at a different
path from the ordinary `tool-linux-x64.tar.xz` can name each archive
rather than claiming one layout for the platform. The
[TOML manifest](https://packslip.dev/docs/describing-releases/#use-a-toml-manifest)
that `packslip create` reads spells this as
`artifact = "tool-linux-x64.tar.xz"` inside `[[resource]]`.

An `archive` entry never applies to an artifact whose `format` is bare:
`raw`, `gz`, `xz`, `zst`, or `bz2`. Such an artifact is the executable
itself, so it holds no path for the entry to name. A vendor that publishes
`tool-linux-x64.tar.xz` beside a bare `tool-linux-x64` therefore needs no
scope to keep an archive entry off the bare one; the entry's source
already says which artifacts it can describe. This bears on the source and
not on the resource, so the same man page may still reach a bare artifact
through an `asset` or `repo` entry.

Entries compete only with entries for the same thing. Two entries are for
the same thing when they share a `kind` and an identity: `bin` and
`shell` for a completion, `bin` and `format` for a `cli-spec`, `name` for
a skill,
`format` for an SBOM, and for every other kind the file name of the
source, or the kind alone for an `exec` source. Entries for different
things never hide one another: a skill scoped to `linux` beside an
unscoped skill of another name leaves that skill in place, and a zsh
completion for one platform says nothing about the bash one.

For each thing, a consumer chooses among its entries in this order. The
reference implementation provides this as `select_resources`.

1. Keep the entries that apply to the selected artifact.
2. Of those, keep the most specific. An entry that names `artifact`
   outranks every entry scoped only by platform; among entries equally
   scoped by `artifact`, the entries naming the most of `os`, `arch`, and
   `libc` win.
3. Try what is left by source, in the order [Source types](#source-types)
   and [Fallback and verification](#fallback-and-verification) give.
4. Try entries with the same source type in their order in `resources`.
   This final tie-breaker is an ordered list of alternatives: the first
   usable entry wins, and a consumer stops fetching or running
   lower-priority entries once it has that thing.

For example, two equally scoped `repo` entries for one skill try the
first directory, then the second only if the first is unavailable. A
verification failure refuses the release; it is never a reason to try the
next entry.

### Resource kinds

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
  skill is an archive of the directory's contents, with `SKILL.md` either
  at the archive root or inside a single top-level directory, which the
  consumer strips; nothing else is at the root. With
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

### Fallback and verification

For any one thing, a consumer takes the first source it can use in the
order [Source types](#source-types) gives: an `archive` or `asset` entry,
then `repo`, then one derived from a `cli-spec`, and only then `exec`.
Static `cli-spec` sources use the same `archive`, `asset`, `repo`
ordering, with document order breaking ties within each source type. A
consumer ignores kinds and sources it does not know, so a vendor may ship
a `font` or a kind of its own before the specification names it.

A resource is an extra, and the executables are installed with or without
it. A consumer that cannot fetch one, because the asset or the repository
file is not there or the network fails, reports that and finishes the
install without it; a later attempt may fetch it. A resource that arrives
with a digest other than the one `subject` gives, or a repository file
that is not what `source.commit` holds, is another matter: that is a
broken release or a tampered one, and the consumer refuses the release,
as it would for an artifact with a wrong digest.

### Running an exec entry

An `exec` entry runs a release executable. For completions, consumers run
it on demand when the shell first requests completion, without additional
permission beyond installation. A consumer caches successful output per
release, executable, and shell, so repeated requests do not rerun the
command.

Other exec resources, such as a generated skill written to disk, run at
install time only if the user has allowed vendor code to run then. Without
that permission, the consumer treats the entry as absent rather than
failing the installation. Consumers may also generate completions at
install time under the same permission.

In either case, the consumer runs the command with the release's
executables on PATH, in a directory of its own that is not the user's
project, with no standard input, with standard error discarded, and under
a timeout it chooses; a few seconds suits a completion. `{shell}` in the
argv and in `env` values is replaced by the shell being asked for, and
`env` is added to an environment that is otherwise the consumer's. A
non-zero exit, a timeout, or empty output means the entry is absent this
time and may be tried again later.

## Host requirements

`requires` describes what the host must provide before the software can
run. It uses operating-system loader names and command names, rather than
package names, so consumers can check requirements without a shared
registry. It has four fields: `os_min`, the lowest OS version in the OS's
own terms; `glibc_min`, the lowest glibc for a `gnu` Linux build; and two
lists, `libs` and `bin`:

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
  executables were read and need no library beyond those left out above.
  An absent one means nothing was checked, as for an installer `create`
  does not open or an executable that is a script.
- `bin` lists the commands the executables run and cannot work without,
  each a bare name as the executable invokes it (`java`, `python3`, `git`;
  no directory, no `.exe`) with an optional `min`, the lowest version that
  works. The vendor declares these; nothing in a binary says it runs
  `java`. Only hard requirements belong here; an optional integration goes
  under `extensions`. A required command may not be one the release itself
  provides.

`min`, `glibc_min`, and `os_min` are numeric lower bounds, so a `min` of
`17` means 17.0.0 and later, including 21. A consumer compares
dot-separated nonnegative integer components numerically, padding missing
components with zero; `2.10` exceeds `2.9`. If either spelling cannot be
compared this way, the check is unknown and the consumer warns rather
than guessing.

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
  needs; where the consumer can install that command itself, it presents
  the command as something it can install.

An unknown check result produces a warning, not a refusal. This includes
unreadable versions and loaders the consumer cannot query. Select the
artifact first, as [Selecting an artifact](#selecting-an-artifact) defines,
then check its requirements. Requirements do not break selection ties or
silently redirect installation to a different build.

How to check, on the common hosts: a library by the loader's own search,
which is the dynamic linker's cache and search path on Linux
(`ldconfig -p`, `LD_LIBRARY_PATH`), the system library and framework
directories on macOS, and PATH with the system directory on Windows; a
command by looking its name up on PATH, with `.exe` on Windows, and
running it with `--version` to read a version. Compare `min`,
`glibc_min`, and `os_min` against the command, glibc, and OS versions
respectively. A version probe that fails or has an unrecognized spelling
leaves the check unknown.

`requires` names what the host must have, by names the OS itself
resolves. It does not name other projects, versions of
them, or where to get them: that needs a namespace only a registry has,
and a package manager's install hints go under `extensions`. Two things
this version leaves out on purpose, to be added if vendors need them: a
`bin` entry satisfied by any of several commands (`terraform` or `tofu`),
and a symbol version on a library (`libstdc++.so.6` at `GLIBCXX_3.4.29`),
which `glibc_min` covers for libc alone.

## Extensions

Use `extensions` for metadata this specification does not define, such
as install hints, an end-of-life date, or a build ID. The release
predicate, each artifact, each resource, the release-list predicate,
and each release-list entry may carry an extensions object:

```json
"extensions": {
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

## Repackager attestation

A package repository (such as an apt or AUR repository), registry, or
mirror that describes a vendor's artifacts, and whose vendor publishes no
packslip, may sign one itself with `"attested_by": "repackager"`. The
`project` still names the vendor's project and the artifacts are still
the vendor's files, but `identity` is the repackager's, and `evidence`
says what it checked before signing:

```json
"attested_by": "repackager",
"evidence": [
  { "kind": "apt-release-gpg", "detail": "3FEF9748469ADBE15DA7CA80AC2D62742012EA22" },
  { "kind": "pkgbuild-checksums" }
]
```

Documented kinds:

- `pkgbuild-checksums`: digests matched the packaging the repackager
  maintains.
- `checksum-file-over-tls`: the vendor's checksum file, unsigned.
- `apt-release-gpg`: an apt index signed with the given key.
- `vendor-signature`: a detached signature the vendor publishes.
- `vendor-packslip`: the vendor's own packslip, verified; `detail` is the
  sha256 of the vendor's bundle file.
- `github-attestation`: GitHub artifact attestations verified.
- `provenance-verified`: SLSA provenance verified against the builder.
- `scan`: the artifacts were scanned; `detail` points at the report.
- `none`: the repackager checked nothing, and the document rests on its
  signature alone.

A repackager document proves that the repackager published exactly these
digests and checked the listed evidence. It does not prove anything the
vendor did not sign. Consumers rank it below a vendor document, and a
consumer that already holds a vendor document for a project refuses to
replace it with a repackager one without a person's approval.

A mirror is a repackager whose documents carry the vendor's digests with
the mirror's own URLs and `vendor-packslip` evidence. A consumer pointed at
a mirror gets the same bytes under the mirror's pin and can still fetch
and verify the vendor's document if it wants both.

## Discovery

Publish the bundle next to the artifacts: as a release asset, or in that
release's own directory on a download site.

Consumers find a project's releases through its
[signed list](#the-signed-list) or, for a GitHub project, its
[releases endpoint](#github). A signed list can also withdraw releases and
recommend a version, so a GitHub project may publish one beside its
endpoint.

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
    "latest": "2026.9.1",
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

Each entry carries the release's `version` and `published_at` and, when
the packslip has a `source.tag`, its `tag` as the vendor spells it, all
copied from the packslip. It may carry
`status: "yanked"` with a `status_reason` when the release was withdrawn,
`security: true` when it fixes a vulnerability, and, on a list from
someone other than the vendor, `evidence` saying what the publisher
checked. A consumer never selects a yanked release, warns when it holds
one, and may shorten its minimum release age for a security release.
The optional list-level `latest` is the vendor's recommended default, as
an exact semver string matching an entry's `version`, including any build
metadata. It is not a tag, range, or per-release flag. A pointer to a
version absent from the list makes the list invalid. Its target may be
yanked or otherwise ineligible; that does not invalidate the list, and
[Latest](#latest) defines the fallback.

The fields of a release list:

| Field | Type | Presence | Meaning |
|---|---|---|---|
| `_type` | string | required | Always `https://in-toto.io/Statement/v1`. |
| `subject[]` | array | required | One entry per listed packslip. |
| `subject[].name` | string | required | The packslip's URL, as its entry's `packslip` gives it. |
| `subject[].digest.sha256` | string | required | SHA-256 of the bundle file, lowercase hex. |
| `subject[].digest.sha512` | string | optional | SHA-512 of the bundle file, lowercase hex. |
| `predicateType` | string | required | Always `https://packslip.dev/releases/v1`. |
| `predicate.project` | string | required | The project the list is for. See [Names](#names). |
| `predicate.generated_at` | string | required | When the list was produced, RFC 3339 UTC. |
| `predicate.expires_at` | string | required | When the list goes stale, RFC 3339 UTC. A consumer refuses an expired list. |
| `predicate.sequence` | integer | required | A nonnegative counter. A consumer refuses a list whose sequence is lower than one it has accepted. |
| `predicate.latest` | string | optional | The vendor's recommended default: the exact `version` of an entry. See [Latest](#latest). |
| `predicate.identity` | object | required | How the list is signed, with the fields a release statement's `identity` has. See [Field reference](#field-reference). |
| `predicate.releases[]` | array | required | The listed releases, in any order. |
| `releases[].version` | string | required | Semver 2.0.0, copied from the packslip. |
| `releases[].tag` | string | optional | The packslip's `source.tag`. |
| `releases[].published_at` | string | required | RFC 3339 UTC, copied from the packslip. |
| `releases[].packslip` | string | required | URL of the release's bundle. The subject of that name carries its digest. |
| `releases[].status` | string | optional | `yanked` when the release was withdrawn. |
| `releases[].status_reason` | string | optional | Why it was withdrawn. |
| `releases[].security` | boolean | optional | The release fixes a vulnerability. |
| `releases[].evidence[]` | array of object | optional | On a list from someone other than the vendor, what the publisher checked: `{ kind, detail? }`, with the kinds [Repackager attestation](#repackager-attestation) documents. |
| `extensions` | object | optional | On the list and on each entry. See [Extensions](#extensions). |

Each subject is a listed packslip's URL with the digest of that file, so
the list pins the exact documents it points at. `expires_at` and
`sequence` are borrowed from TUF's timestamp role: a consumer refuses a
list that has expired, or whose sequence is lower than one it has already
accepted, so a mirror cannot freeze or roll back the vendor's view.

The list is published at

```text
https://<host>/.well-known/packslip/<path>.json
```

where `<host>` and `<path>` are the project name's host and the rest of
it, or `https://<host>/.well-known/packslip.json` when the name is a bare
host. For `mise.jdx.dev` that is
`https://mise.jdx.dev/.well-known/packslip.json`; for `jdx.dev/mise`,
`https://jdx.dev/.well-known/packslip/mise.json`; for
`github.com/jdx/mise`, `https://github.com/.well-known/packslip/jdx/mise.json`,
which github.com does not serve, and [GitHub](#github) says what a
consumer does there. A forge that does serve it needs nothing else.

A project on its own domain must publish the list: a consumer that finds
none refuses the project rather than guessing at URLs.

The list separates the name from where the bytes live: the identity is
anchored to the domain, and the artifacts can be anywhere. A vendor with
only a git repository keeps the list and the bundles in it, at paths its
host serves as raw files, and points the artifact URLs wherever the bytes
are, an LFS store included. Nothing about a forge's release API is
required.

The reference implementation gives the URL as `list_url`, for a project's
own list and for one another publisher keeps for it (see
[Lists from other publishers](#lists-from-other-publishers)), and the path
of a GitHub repository's supplementary list as `github_list_path`.
`packslip releases` produces the list from local copies of the released
bundles, and its `--latest` option sets the list-level `latest`; without
the option, the list has none. The JSON schema is at
`https://packslip.dev/schema/releases-v1.json`.

### GitHub

For `github.com/<owner>/<repo>[/<subpath>]`, the repository's releases
endpoint takes the place of a signed list, and every consumer can read it
without the vendor publishing anything more:

- A release counts when it is not a draft and carries a packslip whose
  `project` matches, or names the same repository renamed, as
  [Forge identity](#forge-identity) allows. GitHub redirects a renamed
  repository's API and release URLs from the old name to the new one, so
  a consumer asking for the old name reaches the renamed repository's
  releases.
- Its tag names its version as [Tags](#tags) defines. A consumer lists
  versions from tags without downloading a bundle per release, and on
  install verifies the packslip and refuses it when its `version` differs.
  A tag that names no version is invisible here.
- The endpoint's order and its prerelease flag are not consulted; the
  version says both. GitHub's latest-release pointer is used only as an
  unsigned recommendation, as [Latest](#latest) says.

The repository may also carry a signed list on its default branch:
`.well-known/packslip.json`, or `.well-known/packslip/<subpath>.json` for
a monorepo tool (`.well-known/packslip/crates/cli.json` for
`github.com/biomejs/biome/crates/cli`). GitHub serves the file at
`https://raw.githubusercontent.com/<owner>/<repo>/HEAD/<path>`. It is
signed by the same identity the packslips are, and it is
supplementary. For a version it lists, the entry decides: its `tag`,
`status`, and `security` apply, and the consumer accepts only the packslip
with the digest the list gives. A version it omits still comes from the
endpoint.

A vendor writes one only to withdraw a release, mark a security fix,
recommend a default with `latest`, or list a release whose tag names no
version; a vendor that needs none of those never writes it. Withdrawing a
release this way works on a repository with immutable releases, where the
release and its packslip cannot be deleted.

Once a consumer has accepted a supplementary signed list for a project,
a missing list is an error, not a return to endpoint-only discovery.
Otherwise removing the list would undo its withdrawals without a newer
signed statement. Because a consumer also refuses an expired list
([The signed list](#the-signed-list)), a published list has to be
re-signed before each `expires_at`, whether or not a release shipped. A
vendor retiring its entries publishes a fresh, unexpired list with a
nondecreasing sequence; a user may explicitly forget the remembered list
policy for that project.

### Lists from other publishers

A list need not come from the vendor. A registry, a mirror, or a scanning
service publishes lists under its own host, one per vendor project it
covers, at the well-known path with the vendor's project name:
`https://registry.example/.well-known/packslip/github.com/jdx/mise.json`,
which `list_url` gives for a publisher that is not the project's host.
A consumer trusts such a host only by configuration, with a pin for each
host it trusts.

Each entry points at a packslip and pins its digest. The packslip may be
the vendor's own, signed by the vendor's identity, or a repackager
document the publisher signed. A vendor document is verified against the
vendor's pin, which its `project` implies, and a repackager document
against the pin its own `project` and `attested_by` imply. The list's
signature proves who selected these releases and what the entry's
`evidence` says they checked, not who built them. A consumer that trusts
one or more such hosts selects only versions at least one of them lists,
which is how a registry that scans releases before admitting them, or an
organization that curates versions, applies its judgment without a
per-tool recipe.

Such a list is a stamp on the releases it names, and a host that
publishes one is a stamping host. A consumer that trusts stamping hosts
applies these rules:

- A version none of them lists is not released: it is not offered and
  not installed, however valid the vendor's own document is.
- Any one trusted host's non-yanked stamp suffices. A host withdrawing its
  own stamp does not veto another trusted host's approval; an operator
  that needs one host to control admission configures that host alone.
- A vendor withdrawal still excludes the version regardless of stamps.
- A user who trusts a vendor outright says so for that project alone, and
  the consumer then takes the vendor's document under the vendor's pin
  with no stamp at all.

A stamping host signs with whichever scheme fits it: a service that
stamps continuously holds a key, a registry that stamps from a repository
signs keylessly, and the consumer pins the host either way.

## Versions

`version` is a semver 2.0.0 version: `MAJOR.MINOR.PATCH`, an optional
prerelease part after `-`, optional build metadata after `+`. Calver such
as `2026.9.1` qualifies. `packslip create` refuses anything else, and so
does a consumer. The tag can be spelled however the vendor likes
(`v2026.9.1`, `oxlint_v1.0.0`); `source.tag` carries it.

Version ordering, prerelease status, and channels follow from the signed
version string:

- Order is semver precedence, so a range such as `^1.2` has a meaning. A
  backport such as 20.19.1, published after 22.0.0, still ranks below it.
  Build metadata takes no part, as semver says. The order of GitHub's
  releases endpoint or of a signed list is not consulted. A vendor may
  separately recommend a default for `latest`, as [Latest](#latest)
  defines.
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

Prerelease and channel metadata are derived rather than stored as
separate fields. This avoids contradictions with the signed version or
editable forge metadata. Withdrawals and default recommendations belong
in discovery metadata, where they can change without replacing a signed
release manifest.

### Spelling a version

A vendor whose versions are not semver as written picks the semver
spelling once and keeps it; the tag keeps the vendor's own spelling.

- A prefix belongs to the tag, not the version: `jq-1.7.1` is version
  `1.7.1`. [Tags](#tags) lists the prefixes a consumer strips on its own;
  a tag with any other prefix, such as `release-1.2.3`, needs a signed
  list entry to map it.
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
[Spelling a version](#spelling-a-version) says. The prefix is the tool's
subpath, the last segment of the subpath, or the repository name,
followed by `/`, `-`, `_`, or `@`. So `v1.2.3` names `1.2.3`;
`oxlint_v1.0.0` names `1.0.0` for `github.com/oxc-project/oxc/oxlint`;
`cli/v1.9.4` names `1.9.4` for `github.com/biomejs/biome/crates/cli`;
`jq-1.7.1` names `1.7.1` for `github.com/jqlang/jq`; `v4.1` names `4.1.0`.

A consumer reads a version from a tag only to list releases without
fetching every bundle. The packslip is the authority: when its `version`
is not what the tag named, the consumer refuses the release. A tag that
names no version, such as `nightly` or `release-1.2.3`, hides its release
from tag listing. To list that release, the vendor publishes a signed
list whose entry maps its tag to its version.

### Matching a request

A requested version matches as a prefix on dot-separated components, so
`20` matches every `20.x.y` release, `3.12` every `3.12.x`, and
`1.2.0-beta` the betas of 1.2.0. A request may also be the tag as the
vendor spells it, with or without its leading `v`; a consumer matches it
against `source.tag` or the list entry's `tag`, so a user who knows a
release by the vendor's name still finds it. Eligible means not yanked,
not a prerelease unless prereleases were asked for, and on the requested
channel when one was given. Rollback protection comes from the release
list's `sequence`, not from anything a single release says.

### Latest

An unconstrained `latest` request (including a consumer's default install
request) asks for the vendor's recommended eligible release. Ordering
and recommendation are separate: a vendor may publish `3.0.0` while
recommending `2.8.4`. Exact versions, version prefixes, ranges, and channel
requests use their own [matching rules](#matching-a-request) and semver
precedence; the recommendation does not reorder them.

Consumers choose a recommendation in this order:

1. Use `latest` from the vendor's accepted signed release list, if it has
   one; on GitHub, that is the repository's supplementary list. The list
   must pass its normal signature, identity, expiry, and sequence checks
   first.
2. For a GitHub project without a signed `latest`, use the release returned
   by GitHub's [latest release endpoint](https://docs.github.com/en/rest/releases/releases#get-the-latest-release).
   Resolve its tag through the normal discovery rules, including a signed
   list entry's `tag` mapping. It must belong to the requested project;
   a repository-wide pointer to another tool is not a recommendation for
   this one. This is an unsigned discovery hint, not proof of authenticity
   or a sequence-protected recommendation.
3. Elsewhere, without a signed pointer there is no recommendation.

A recommendation is verified like any other candidate: its release
signature and identity, and its release manifest's version and digest
consistency. A verification failure is an error, not a reason to fall
back. A verified recommendation is selected only if it also passes the
same checks as any other candidate: no vendor yank, prerelease policy,
minimum release age, configured stamping policy, and artifact/host
eligibility. A pointer
never admits a release a consumer would otherwise refuse. Pointers on
third-party stamping lists do not replace the vendor's recommendation;
those lists determine which candidates are admitted.

If there is no recommendation, or its target is absent from discovery or
excluded by eligibility policy, select the eligible release with the
highest precedence from the normal candidate set. An ineligible signed
pointer falls directly back to semver selection, not to GitHub's pointer.
Report when a declared recommendation is skipped and why; if no eligible
release exists, fail. For example, if `latest` is `2.8.4` but `2.8.4` is
yanked or too young, `3.0.0` may be selected if it is eligible. A
vendor that must exclude `3.0.0` must withdraw it or use an admission
policy ([Lists from other publishers](#lists-from-other-publishers)),
rather than relying on `latest`; `latest` recommends a release but never
excludes one.

Fallback does not turn verification failures into missing metadata.
An invalid, expired, rolled-back, or unexpectedly missing signed list
remains an error; a candidate with a bad signature or inconsistent digest
or version remains an error. A GitHub latest endpoint response indicating
no latest release supplies no pointer; other fetch errors remain errors.
Changing or removing a signed recommendation requires publishing a new
list with an increased sequence, without replacing any release manifest.

## Selecting an artifact

Select one artifact using the following ordered rules. The reference
implementation exposes this selection as `select_artifact`.

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

These rules apply to the complete consumer workflow, not just signature
verification. Consumers must preserve enough state to enforce signer
continuity, no-downgrade policy, and release-list sequences across
installs. The rules, and the sections they cite, refer to the following
state:

- the pins: the signer and its repository ID, any signer fingerprint or
  lockfile commitment the consumer holds, and the remembered
  `pin_workflow` value;
- the trust properties of the last accepted release: its
  `identity.scheme`, `attested_by`, and provenance;
- the highest release-list `sequence` accepted, and whether a signed list
  was accepted at all;
- the choices a person made, such as an approved signer change, an
  allowed unlogged bundle, or trusting the vendor without a stamp.

1. Pin the identity once.
   - For a forge project, the name gives the first pin: accept only the
     forge's issuer and an identity under the repository, and remember the
     repository ID its certificate records. From then on that ID pins the
     project, as [Forge identity](#forge-identity) says: a renamed or
     transferred repository keeps its pin, and a recreated name does not
     inherit it.
   - Where the project publishes a [signer fingerprint](#signer-fingerprint)
     that the consumer has read apart from its releases, that fingerprint
     is also part of the first pin: the consumer checks releases against
     it as [Signer fingerprint](#signer-fingerprint) says, so no release
     has to be trusted on first use.
   - For other projects, pin the public key or identity from a list of
     pins the consumer maintains, or from the well-known list on first use.
   - A list from another publisher is trusted per host, by configuration.
   - Never take a key from the document itself, and never trust a bundle's
     key hint.
2. Verify the bundle: signature, certificate chain and log entry as
   sigstore defines them, then the statement's structure, then, for a
   forge project, that the statement names the requested project or the
   same repository renamed, as [Names](#names) and
   [Forge identity](#forge-identity) say, then the subject digest of every
   artifact or asset the consumer downloaded, and the size of every
   artifact. Refuse a bundle without a log entry unless unlogged bundles
   were explicitly allowed, as [Signing](#signing) says.
3. Enforce no-downgrade. Compared with the last release accepted for the
   project, refuse a release:
   - whose `identity.scheme` is weaker;
   - whose signer changed without a person's approval;
   - whose `attested_by` went from vendor to repackager;
   - that dropped per-artifact provenance the last release carried.

   For a keyless signer, compare the workflow path, not the ref: a new tag
   of the same workflow is the same signer. For a renamed or transferred
   repository, compare the workflow's path inside the repository, and only
   when the repository IDs show the same repository: the one the release's
   certificate carries or, comparing two recorded signers such as a
   lockfile entry and the one it replaces, the ID recorded with each. A
   changed repository ID is a changed signer, even under the same
   identity. A release that declares `identity.pin_workflow: false` is
   compared by repository alone; declaring it where the remembered value
   is `true` is a reduction that is refused until a person approves it, as
   [Reusable workflows](#reusable-workflows) says.
4. Apply any minimum release age to the log's integration time, falling
   back to `published_at` only for an unlogged bundle the consumer chose
   to accept.
5. Use the project's discovery metadata:
   - For a GitHub project, read its releases endpoint and, if the
     repository has one, its supplementary list; otherwise read the signed
     list at the well-known URL. Refuse a project that has neither, and
     refuse when a signed list accepted before is now missing, as
     [GitHub](#github) says.
   - Refuse a signed list that has expired or whose `sequence` is below
     the last one accepted.
   - Never select a yanked entry; skip prereleases unless asked for them;
     rank by semver precedence, and take the vendor's recommendation for
     an unconstrained request as [Latest](#latest) says.
   - Refuse a packslip whose digest is not the one the list pins, or whose
     version is not the one its tag or list entry named.
   - When the consumer trusts stamping hosts, select only versions one of
     them lists, unless the user chose the vendor alone for that project,
     as [Lists from other publishers](#lists-from-other-publishers) says.
6. Select one artifact as [Selecting an artifact](#selecting-an-artifact)
   says. Refuse to guess between two artifacts that tie.
7. Check `requires` against the host before installing, as
   [Host requirements](#host-requirements) says: refuse when a library,
   glibc, or OS version means the executables cannot start, warn when a
   command is missing or too old, and report either in the consumer's own
   terms. Treat a requirement the consumer cannot check as a warning,
   never a refusal. Check the artifact selected by rule 6; requirements do
   not resolve an ambiguous selection.
8. Install resources as [Resources](#resources) says:
   - For each thing the resources describe, keep the entries whose scope
     fits the selected artifact and the most specific of those, as
     [Scope and identity](#scope-and-identity) says, then take the most
     verifiable source offered, in the order
     [Source types](#source-types) and
     [Fallback and verification](#fallback-and-verification) give.
   - Run an `exec` completion on demand, when a shell first asks, and
     cache it; run any other `exec` entry only if the user has chosen to
     run vendor code at install time, as
     [Running an exec entry](#running-an-exec-entry) says.
   - Ignore kinds the consumer does not know.
   - Report a resource the consumer cannot fetch, and finish the install
     without it; fail the install for one whose digest is not the one the
     document signed.

## Stability

This document is final for version 1. The predicate types
`https://packslip.dev/release/v1` and `https://packslip.dev/releases/v1`
name it, and a document that verifies under them today keeps verifying
under them for as long as its signing material holds.

Version 1 fixes:

- The meaning of every field defined here. A field is never redefined,
  narrowed, or given a different default.
- Field names and types, and the vocabularies that constrain them.
- The rules that decide which release and which artifact a consumer
  installs: [Versions](#versions), [Discovery](#discovery),
  [Selecting an artifact](#selecting-an-artifact), [Resources](#resources),
  [Host requirements](#host-requirements), and
  [Consumer rules](#consumer-rules).
- That this specification assigns no meaning to anything under
  [`extensions`](#extensions), and never will.

A later revision of version 1 may:

- Add an optional field at any level. Consumers ignore fields they do not
  know, so an older consumer reads a newer document correctly and a newer
  consumer reads an older one.
- Add a member to a vocabulary — a resource kind, an evidence kind, a
  format, an OS or architecture token. A consumer ignores a member it does
  not know, except where a rule here says otherwise.
- Settle wording that was ambiguous, where implementations already agreed
  on the reading, and add examples or guidance.

Anything outside those bounds is version 2: removing a field, making an
optional field required, or changing what a rule selects. Version 2 gets
its own predicate type, published beside this one, and a consumer may
accept both. No revision of this document invalidates a release already
signed under `release/v1`, and nothing obliges a vendor to move.

Two versions are in play and they are not the same number. The format is
version 1, as the predicate types say. The reference implementation is a
crate, a CLI, and two GitHub Actions that share one semver version of
their own; that version may reach 2 while the format stays at 1.

The [conformance vectors](https://github.com/jdx/packslip/tree/main/tests/conformance)
are the executable form of the rules they cover: artifact and resource
selection, tag versions, statement validity, forge identity, and the
signer fingerprint. An implementation that disagrees with a vector
disagrees with this specification. Apart from the forge identity
vectors, which carry the remembered pin in each case, they do not cover
the consumer rules that depend on state a consumer keeps between
installs, such as no-downgrade and release-list sequence.

## Tooling

The [packslip repository](https://github.com/jdx/packslip) contains the
reference Rust library, the CLI, and two composite GitHub Actions: one
publishes a release's packslip
([action.yml](https://github.com/jdx/packslip/blob/main/action.yml)) and
one builds and signs a release list
([releases/action.yml](https://github.com/jdx/packslip/blob/main/releases/action.yml)).
The CLI creates and verifies release and list bundles and prints signer
fingerprints; consumers implement discovery, selection, installation, and
persistent policy around those operations.

| Task | Guide | Command reference |
| --- | --- | --- |
| Create a first packslip | [Getting started](https://packslip.dev/docs/getting-started/) | [create](https://packslip.dev/cli/create/) |
| Generate an Ed25519 key | [Getting started](https://packslip.dev/docs/getting-started/#create-a-sample-release) | [keygen](https://packslip.dev/cli/keygen/) |
| Publish from GitHub | [Publish with GitHub Actions](https://packslip.dev/docs/publishing/) | [action.yml](https://github.com/jdx/packslip/blob/main/action.yml) |
| Describe release files | [Artifact configuration](https://packslip.dev/docs/describing-releases/), [Resources](https://packslip.dev/docs/resources/), [Host requirements](https://packslip.dev/docs/host-requirements/), [Release recipes](https://packslip.dev/docs/recipes/) | [create](https://packslip.dev/cli/create/) |
| Publish discovery metadata | [Manage release lists](https://packslip.dev/docs/release-lists/) | [releases](https://packslip.dev/cli/releases/) |
| Host releases on your own domain | [Host releases on your own domain](https://packslip.dev/docs/self-hosting/) | [releases/action.yml](https://github.com/jdx/packslip/blob/main/releases/action.yml) |
| Verify downloads | [Verify a release](https://packslip.dev/docs/verifying/) | [verify](https://packslip.dev/cli/verify/) |
| Pin a keyless signer by fingerprint | [Verify a release](https://packslip.dev/docs/verifying/#pin-a-signer-with-its-fingerprint) | [pin](https://packslip.dev/cli/pin/), [verify](https://packslip.dev/cli/verify/) `--pin` |
| Inspect a statement without verification | [Verify a release](https://packslip.dev/docs/verifying/#understand-the-result) | [show](https://packslip.dev/cli/show/) |
| Build a consumer or mirror | [Build an installer or mirror](https://packslip.dev/docs/installers/) | — |
| Export JSON schemas | [Documentation](https://packslip.dev/docs/#reference) | [schema](https://packslip.dev/cli/schema/) |
| Check another implementation | [Conformance vectors](https://github.com/jdx/packslip/tree/main/tests/conformance) | — |

The CLI reference is generated from command help. The specification page
is generated from this file; see
[Contributing](https://github.com/jdx/packslip/blob/main/CONTRIBUTING.md)
for the documentation workflow.
