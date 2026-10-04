# packslip: a signed release manifest

Version 1. Predicate types `https://packslip.dev/release/v1` and
`https://packslip.dev/releases/v1`.

Authors: Jeff Dickey ([@jdx](https://github.com/jdx)) and Shunsuke Suzuki
([@suzuki-shunsuke](https://github.com/suzuki-shunsuke)).

## Purpose and scope

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
The statement's predicate carries the release metadata; the bundle carries
the signature and verification material. This specification defines two
predicate types:

| Predicate | Describes |
| --- | --- |
| `https://packslip.dev/release/v1` | One release and its files. |
| `https://packslip.dev/releases/v1` | A release index with mutable discovery metadata. |

## Reading this specification

This document defines the format and the rules for consumers that verify
or install releases. For a working example or CLI instructions, start with
the [guides](https://packslip.dev/docs/). Implementers should read the
[consumer rules](#consumer-rules) alongside the field definitions: valid
JSON alone is not enough to accept a release.

The following terms distinguish the documents and the parties using them:

| Term | Meaning |
| --- | --- |
| *Vendor* | The project whose releases a packslip describes. It normally builds and signs its own releases. |
| *Consumer* | Anything that verifies or installs releases, such as a package manager, install script, or verification tool. |
| *Packslip*, or *release manifest* | The sigstore bundle published with a release. |
| *Statement* | The in-toto statement inside a bundle. |
| *Release list*, or *signed list* | A bundle whose statement has the `releases/v1` predicate. |
| *Repackager* | A third party that signs a packslip for a vendor's files; see [Repackager attestation](#repackager-attestation). |

Use these sections according to the task:

- **Identify and authenticate a project:** [project names](#project-names),
  [bundle format](#bundle-format), [signing](#signing), and
  [verification scope](#what-a-verified-packslip-proves).
- **Describe a release:** [the release statement](#the-release-statement),
  including artifacts, resources, host requirements, extensions, and
  repackager documents.
- **Find a release and choose a file:** [discovery](#discovery),
  [versions](#versions), and [artifact selection](#selecting-an-artifact).
- **Implement a consumer:** [consumer rules](#consumer-rules) for
  verification, remembered trust, and installation; [tooling](#tooling)
  for the reference implementation and
  [conformance vectors](https://github.com/jdx/packslip/tree/main/tests/conformance).
- **Propose a format change:** [stability](#stability) defines what
  version 1 fixes and what a revision may add.

JSON examples abbreviate digests and commits for readability. Those
placeholders are not valid release data.

## JSON schemas

The schemas describe the decoded in-toto statements inside bundles:

| Document | Download | CLI |
| --- | --- | --- |
| One release (`release/v1`) | [release-v1.json](https://packslip.dev/schema/release-v1.json) | `packslip schema` |
| A release list (`releases/v1`) | [releases-v1.json](https://packslip.dev/schema/releases-v1.json) | `packslip schema --releases` |

Use them to check field shapes, required values, and value patterns. They do
not validate the enclosing sigstore bundle, authenticate its signer, or apply
rules that require comparing fields, downloaded bytes, or remembered state.
The reference implementation validates those relationships; consumers also
apply the [consumer rules](#consumer-rules).

To inspect a bundle's statement, use `packslip show packslip.sigstore.json`.
That command decodes the payload without verifying it. See
[Verify a release](https://packslip.dev/docs/verifying/) for an authenticated
check, and the [CLI overview](https://packslip.dev/cli/) to install packslip.

## Project names

A project name is a host with an optional path, such as
`github.com/jdx/mise`, `gitlab.com/group/tool`, or `mise.jdx.dev`.
Its syntax is:

- No URL scheme or trailing slash.
- A lowercase host containing at least one dot.
- Path segments that are neither empty, `.` nor `..`.

A consumer may offer a short-name alias such as `mise` for
`github.com/jdx/mise`. That alias is a convenience outside the format.

The name is the location and, on a forge, says who may sign:

- **GitHub:** `github.com/<owner>/<repo>`. Releases and their packslips are
  GitHub release assets, and the packslip is expected to be signed by a workflow
  of that repository through GitHub's OIDC issuer
  (`https://token.actions.githubusercontent.com`).
- **GitLab:** `gitlab.com/<path>`. Likewise, packslips are signed by a
  pipeline of that project through `https://gitlab.com`. GitLab subgroups
  make paths arbitrary depth, so the whole path names the repository.
- **Other hosts, including forges this version does not name:** the
  vendor controls the domain and publishes a release list at the
  well-known URL [The signed list](#the-signed-list) defines, signed with
  the key or identity the consumer pins.

For a known forge, a consumer derives its initial signer policy from the
project the user intends to install. Deriving it from an untrusted
statement would accept whichever repository the statement's author names,
without checking the user's intended identity.

After verification, the consumer must also check that the statement names
the requested project, or the same repository under another name as
[Repository identity and renames](#repository-identity-and-renames) allows.
A forge name can change hands: the name locates the project, while the
forge's repository ID pins its identity.

### Monorepos

On `github.com`, a repository that releases several tools names each one
with a subpath: `github.com/oxc-project/oxc/oxlint`,
`github.com/bazelbuild/buildtools/buildifier`,
`github.com/biomejs/biome/crates/cli`. On `gitlab.com` the whole path
names the repository, as [Project names](#project-names) says, so a GitLab project has no
monorepo subpath.

Each tool gets its own packslip per release, with its own `version`, and
`source.tag` carries the real tag (`oxlint_v1.0.0`, `cli/v1.9.4`), so
consumers do not have to infer the tag from the version. The identity pin is
still the repository: any workflow of `oxc-project/oxc` may sign a
packslip for `oxc-project/oxc/oxlint`.

When several tools share one GitHub release, each ships its own bundle,
named `packslip.<subpath>.sigstore.json` with `/` in the subpath replaced
by `-`:

| Project within the repository | Bundle name |
| --- | --- |
| The repository itself | `packslip.sigstore.json` |
| `oxlint` | `packslip.oxlint.sigstore.json` |
| `crates/cli` | `packslip.crates-cli.sigstore.json` |

Consumers select by the signed statement, not by trusting the file name.
They read the release's `packslip*.sigstore.json` assets and keep the one
whose `predicateType` is `release/v1` and whose `project` matches the
requested name, or the same tool in the renamed repository. A tool keeps
its subpath across a rename: `github.com/old/repo/tool` becomes
`github.com/new/repo/tool`, never another tool in the repository.

## Bundle format

A release ships one [sigstore bundle](https://github.com/sigstore/protobuf-specs)
(v0.3) per project. Each bundle contains:

- A [DSSE](https://github.com/secure-systems-lab/dsse) envelope with payload
  type `application/vnd.in-toto+json`, carrying
  [the release statement](#the-release-statement).
- Verification material: the signer's
  [Fulcio](https://docs.sigstore.dev/certificate_authority/overview/)
  certificate or a public-key hint, plus the
  [Rekor](https://docs.sigstore.dev/logging/overview/)
  transparency log entry for the signature.

The log entry may be omitted only for a key-signed release made without
network access; see [Signing](#signing). The signature covers the DSSE
payload bytes. Consumers do not canonicalize the JSON before verification.
General-purpose sigstore tools can read the bundle, but consumers must also
validate the packslip predicate and apply this specification's rules.

To inspect the payload without verifying it:

```sh
packslip show packslip.sigstore.json       # Decode and pretty-print the statement.
packslip show --raw packslip.sigstore.json # Print the signed payload, then a newline.
```

The payload can also be decoded with
`jq -r .dsseEnvelope.payload BUNDLE | base64 -d`.

## Signing

A packslip is signed under one of two schemes, named in
`identity.scheme`. Both produce the same bundle format. Prefer keyless
signing when a supported CI identity is available.

| Scheme | Signing credential | Consumer's trust anchor |
| --- | --- | --- |
| `sigstore-oidc` | A CI job's OIDC identity. | The expected identity and issuer, optionally pinned by [repository fingerprint](#signer-fingerprints). |
| `sigstore-key` | A long-lived Ed25519 key. | The public key. |

### Keyless signing

With `sigstore-oidc`, a CI job with id-token permission signs under its
own identity. Fulcio issues a short-lived certificate naming the workflow
that ran, and Rekor logs the signature. There is no key for the vendor to
manage.

`packslip create` uses this scheme unless given `--key`. It takes the token
from `SIGSTORE_ID_TOKEN` when set, otherwise from the CI job's ambient
credential. Supported ambient credentials include GitHub Actions, GitLab
CI, and the other providers sigstore's clients detect. The command fails
if neither token source is available.

### Key signing

With `sigstore-key`, the vendor signs with a long-lived Ed25519 key. This
scheme suits vendors who release outside a CI system with OIDC, or who
want consumers to pin a stable key.

`packslip keygen` writes the secret key as a hex seed and the public key in
minisign's public-key format. Consumers pin that public-key file, never the
hint carried by the bundle. The bundle also carries the Rekor entry, whose
verifier is the public key.

A key-signed bundle also supports a dependency-free signature check: its
DSSE signature is a raw Ed25519 signature over the pre-authentication
encoding of the payload, with payload type `application/vnd.in-toto+json`.
That signature check does not replace verification of the Rekor entry
required by [consumer rule 2](#consumer-rules), unless the consumer chose
to accept an unlogged bundle.

### Unlogged bundles

A key-signed bundle may be produced without a log entry
(`packslip create --no-log`) for an air-gapped release. A consumer refuses
such a bundle unless it explicitly allows unlogged bundles
(`packslip verify --allow-unlogged` in the reference CLI), and a
repository should record that choice per vendor.

### Workflow pinning

By default, a consumer of a keylessly signed project compares the signing
workflow with the workflow of the last release it accepted. A different
workflow in the same repository is a changed signer: the consumer refuses
the release until a person approves it
([consumer rule 3](#consumer-rules)).

This default requires approval each time a vendor switches between release
workflows, including when several jobs share a reusable workflow. A vendor
can instead declare in its signed `identity` that consumers should pin the
repository while allowing its signing workflow to change:

```json
"identity": {
  "scheme": "sigstore-oidc",
  "key_id": "https://github.com/example/tool/.github/workflows/release.yml@refs/tags/v1.2.0",
  "issuer": "https://token.actions.githubusercontent.com",
  "pin_workflow": false
}
```

`pin_workflow` defaults to `true`. Omitting it therefore pins the signing
workflow. The vendor declares this field in a signed release or release
list. `packslip create --no-pin-workflow` and
`packslip releases --no-pin-workflow` write `false`; both refuse the flag
with `--key`. The field has no meaning for `sigstore-key`, which has no
workflow.

With `pin_workflow: false`, a consumer still requires everything else of
the signer: the issuer, a certificate whose repository is the one pinned
(by repository ID where the forge records one, as
[Repository identity and renames](#repository-identity-and-renames)
describes), and a workflow of that repository. It stops comparing the
workflow's path with the last accepted signer.

The consumer remembers `pin_workflow` beside the signer. Disabling a
previously pinned workflow reduces remembered trust and requires approval,
as [consumer rule 3](#consumer-rules) defines:

| Remembered value | Incoming value | Consumer behavior |
| --- | --- | --- |
| No remembered signer | `true` or omitted | Pin the signing workflow; there is no remembered trust to reduce. |
| No remembered signer | `false` | Pin the repository without pinning the workflow; there is no remembered trust to reduce. |
| `true` | `true` or omitted | Compare the workflow with the last accepted signer; a change requires approval. |
| `true` | `false` | Refuse until a person approves the reduction. Then remember `false`, without asking again for that reduction. |
| `false` | `false` | Continue without comparing workflow paths. |
| `false` | `true` or omitted | Resume comparison with the last accepted signer's workflow. This is not a reduction, but a different workflow is a changed signer and requires approval. |

A consumer written before this field existed ignores it, as it ignores
any field it does not know, and keeps holding the vendor to its workflow.
Those consumers therefore still refuse a release from a changed workflow
until a person approves it, even when the vendor sets the field.

### Repository identity and renames

A repository can be renamed or moved to another owner, and the forge
redirects the old name to the new one: `github.com/jdx/rtx` became
`github.com/jdx/mise`, and GitHub answers for the first with the second.
Once the old name is free, anyone can create another repository under it,
including a workflow at the same path. The name alone cannot distinguish
the original repository from its replacement. The forge's repository ID
can: it survives renames and transfers, never changes, and is never reused.

Fulcio records the IDs in every certificate it issues to a GitHub Actions
or GitLab CI job, from the job's OIDC token:

| Extension | OID | GitHub claim | GitLab claim |
| --- | --- | --- | --- |
| Source Repository URI | `1.3.6.1.4.1.57264.1.12` | server URL + `repository` | server URL + `project_path` |
| Source Repository Identifier | `1.3.6.1.4.1.57264.1.15` | `repository_id` | `project_id` |
| Source Repository Owner URI | `1.3.6.1.4.1.57264.1.16` | server URL + `repository_owner` | server URL + `namespace_path` |
| Source Repository Owner Identifier | `1.3.6.1.4.1.57264.1.17` | `repository_owner_id` | `namespace_id` |

Each extension value is a DER UTF8String. An empty value means the forge
supplied none. For these IDs to authenticate a packslip, the Source
Repository URI must identify the repository named by `project`, and the
signer must be a workflow of that repository.

The owner extensions record who owned the repository when the certificate
was issued. They are not part of the project's identity: consumers neither
remember nor compare them. Only the current owner can transfer a
repository, and that owner already controls what it signs.

A consumer remembers the repository ID of the first release it accepts,
with its signer (see [Consumer rules](#consumer-rules)), and compares each
later release with it:

| Case | Statement and certificate | Consumer behavior |
| --- | --- | --- |
| Same project | The statement names the requested project, and the certificate carries the pinned repository ID. | Accept it. |
| Renamed or transferred | The statement names another repository on the same forge, with the same monorepo subpath, and the certificate carries the pinned repository ID. | Accept it and report the name it was signed under. |
| Different repository | The certificate carries a repository ID other than the pinned one. | Refuse it, even if the statement names the requested project. |
| No ID to compare | The certificate or remembered pin lacks a repository ID, as with certificates issued before Fulcio recorded them. | Check by name alone. Refuse a statement for another name when either side has no ID to compare, as with no pin. |

A rename or transfer can be checked in either direction: the user may ask
for the old name while verifying a release signed after the move, or ask
for the new name while verifying a release signed before it. A replacement
repository under the old name has a different ID and is refused.

Signer continuity compares the workflow's path inside the repository.
For example, `.github/workflows/release.yml` in `jdx/rtx` continues as the
same workflow in `jdx/mise`; the same path in `jdx/hk` continues in
`acme/hk` after a transfer.

A pin taken from an accepted release records its signed project name and
repository ID. Consumers reading pins that also store an owner ID, as an
earlier revision of this section required, ignore that owner ID.

A consumer may hold several pins for a project, such as its own record and
a lockfile's commitment. Each release must match every pin; a refusal says
which pin the release disagreed with.

With no pin, the consumer has only the forge's word, and first use is
trust on first use. GitHub's `GET /repos/<owner>/<repo>` redirects a
renamed or transferred repository's old name to the repository and
returns its `id`, and a consumer may take that as the expected repository
ID. If the name was recreated before the consumer first saw it, the forge
answers for the new repository, and the release cannot reveal that the
name previously belonged to another repository.

Two mechanisms limit that first-use exposure:

- A lockfile that records the repository ID carries that pin to every
  machine that reads it.
- A project publishes a [signer fingerprint](#signer-fingerprints), which
  lets a consumer pin the repository before its first install.

The reference implementation reads the extensions as
`sigstore::source_repository`, classifies a release into these cases in
`forge::check`, and verifies and classifies a bundle in one call with
`verify_forge`. `forge::same_workflow` compares two signers a consumer
recorded, with the repository ID recorded alongside each, for
[consumer rule 3](#consumer-rules) when no release is at hand.

### Signer fingerprints

A signer fingerprint identifies the repository and issuer used to sign a
keyless project's releases. It lets a consumer pin that repository before
accepting its first release, rather than trusting the first release it
finds (see [Consumer rules](#consumer-rules)).

A project can publish its fingerprint independently of its releases, for
example in its README or install instructions. Users can then record it in
a Dockerfile or a consumer's lockfile. A fingerprint looks like this:

```text
ps1_snirenkjwr7m5ozgcufameodnm
```

#### Computing a fingerprint

The fingerprint is `ps1_` followed by the first 128 bits of

```text
SHA-256( "packslip-signer-v1" 0x00 issuer 0x00 repository_id )
```

encoded as 26 characters of RFC 4648 base32, in lowercase and without
padding (`a`–`z`, `2`–`7`). In the hash input, `0x00` is a single zero byte
and the quoted string is its ASCII bytes. The other two inputs come from
the certificate, exactly as recorded, with no case or slash normalization:

- `issuer` is the certificate's OIDC issuer URL, such as
  `https://token.actions.githubusercontent.com` or `https://gitlab.com`.
- `repository_id` is the Source Repository Identifier (OID
  `1.3.6.1.4.1.57264.1.15`, see
  [Repository identity and renames](#repository-identity-and-renames)) as
  the string of decimal digits it is.

For GitHub repository `jdx/hk`, whose ID is `922514152`, the input is
`packslip-signer-v1`, a zero byte, `https://token.actions.githubusercontent.com`,
a zero byte, and `922514152`, and the fingerprint is
`ps1_snirenkjwr7m5ozgcufameodnm`.

Twenty-six base32 characters hold 130 bits, so the last character carries
two padding bits, which are zero. Text whose last character sets either
bit is not a fingerprint, so each fingerprint has exactly one spelling.

#### What the fingerprint pins

A fingerprint commits to the issuer and repository ID. It excludes values
that can change while the repository stays the same, and subpaths that
distinguish tools within that repository:

| Excluded value | Consequence |
| --- | --- |
| Owner | A transfer preserves the repository ID and fingerprint. Consumers do not remember or compare owners, as [Repository identity and renames](#repository-identity-and-renames) defines. |
| Repository name | A rename preserves the ID, so the fingerprint remains valid. |
| Workflow and ref | A replaced workflow or a new release tag does not change the fingerprint. The separate signer-continuity check in [Consumer rules](#consumer-rules) still applies. |
| Monorepo subpath | One fingerprint covers every tool in the repository. The statement's project name identifies the tool. |

When a repository is deleted and someone else creates one under its
name, the new repository has a new ID and so a different fingerprint.
That is what makes the fingerprint a pin.

A release has no fingerprint when it is signed with a key, when its
certificate records no repository ID (because it predates the extension
or the issuer does not give one), or when the recorded ID is not a string
of decimal digits. A project whose releases have no fingerprint is pinned
with an identity and issuer, or a public key, as
[Consumer rules](#consumer-rules) says.

The fingerprint is a public commitment, not a secret. Its 128 bits make
finding another repository with the same fingerprint infeasible, while its
text form fits in a README. It carries no authority by itself: the
consumer trusts the project that publishes it and the place it is
published. A fingerprint does not belong in a packslip, which cannot vouch
for its own signer.

#### Checking a fingerprint

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
3. Apply [Repository identity and renames](#repository-identity-and-renames)
   as for any release: the certificate's Source Repository URI must be the
   repository the statement names, and the signer a workflow of it. The
   fingerprint says which repository; these say the statement is about it.

The consumer refuses a malformed pin rather than ignoring it or treating
it as "no pin". A valid pin must start with `ps1_`, have exactly 26
lowercase base32 characters after the prefix, and have zero padding bits
in the last character. An unknown prefix, such as `ps2_`, is refused; the
consumer does not guess what it commits to.

The [conformance vectors](https://github.com/jdx/packslip/tree/main/tests/conformance)
for the fingerprint give inputs, their fingerprints, texts that are not
fingerprints, and matches and mismatches. The reference implementation
computes a fingerprint with `Fingerprint::of_signer` and checks it with
`Fingerprint::verify`. The CLI prints a release's fingerprint with
`packslip pin` and checks a release against a supplied fingerprint with
`packslip verify --pin`.

## What a verified packslip proves

A verified packslip authenticates the named signer's statement about the
release. A downloaded file whose digest matches the statement is the file
the signer described. A logged signature also has a verified integration
time. An explicitly accepted unlogged signature has no such log evidence.

Two kinds of assurance require separate checks:

- **Build provenance and safety.** The manifest does not establish how an
  artifact was built or whether it is safe. Linked SLSA provenance must be
  fetched and verified separately against the builder's identity and the
  consumer's policy. A provenance URL alone establishes no build level.
- **Freshness.** A single release manifest does not establish freshness,
  detect withdrawal, or prevent rollback. Those checks require discovery
  metadata and remembered consumer state, as [Discovery](#discovery) and
  [Consumer rules](#consumer-rules) define.

### Resource coverage

The source of a resource determines what authenticates it:

| Source | Authentication |
| --- | --- |
| `archive` | Covered by the archive's signed digest. |
| `asset` | Covered by the asset's signed digest. |
| `repo` | Pinned by the source commit. |
| `exec` | The consumer verifies the executable and controls when it runs. Its output is not separately signed. |

### Reference CLI behavior

`packslip verify` checks the supplied bundle and local files passed with
`--artifact`. It reports signing information, provenance links, resources,
and host requirements. It does not fetch provenance, install resources, or
maintain trust history across invocations. For a release list, it also
does not enforce expiry or compare its sequence with a previously accepted
list.

Without `--pubkey`, `--identity`, `--identity-prefix`, or `--issuer`, the
command derives its signer policy from the GitHub or GitLab project the
statement claims. As [Project names](#project-names) explains, that default
does not check the user's intended identity. Supply the expected public
key with `--pubkey`, or the expected identity or identity prefix together
with `--issuer`. Only supplied identity flags are checked: `--issuer`
alone accepts any signer from that issuer. A supplied `--pin` adds the
repository-fingerprint check to the identity policy.

With any policy, compare the reported project and version with the request.
A signer can sign other projects and older versions; `packslip verify`
does not know which one the user asked for. See
[Verify a release](https://packslip.dev/docs/verifying/) for CLI examples.

## The release statement

A release statement is an in-toto statement with `predicateType`
`https://packslip.dev/release/v1`. Its `subject` binds file names to
digests; its `predicate` describes the project, version, artifacts,
resources, and signer.

This example describes a mise release with one Linux x86_64 archive and
a separate SBOM asset. Digest and commit values are abbreviated as `...`;
an actual statement must supply their full values.

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

### Subjects and digests

`subject` lists every artifact by file name and digest, plus every
separate file used as a resource's `asset`. `artifacts` describes the
installable files with those same names. The two lists have these rules:

- Every artifact is a subject.
- Every subject is either an artifact or a resource's `asset`.
- Neither list contains duplicate names.
- At least one artifact is required.

Each subject requires a `sha256` digest of 64 lowercase hexadecimal
characters. A `sha512` digest is optional and has 128 lowercase
hexadecimal characters. An artifact's required `size` is its length in
bytes and is checked alongside its digest. Its optional `url` gives the
download location.

### Project, version, and source

`project` follows the [project-name rules](#project-names). `version` is
[semver 2.0.0](https://semver.org/spec/v2.0.0.html); its prerelease part,
if present, marks a prerelease and names its channel. See
[Versions](#versions). `published_at` is a UTC timestamp in
[RFC 3339](https://www.rfc-editor.org/rfc/rfc3339) format.

The optional `source` object identifies where the release was built
from. When present, it requires a repository URL in `repo`; `commit` and
the vendor's spelling of `tag` are optional. A resource with a `repo`
source requires `source.commit` so its content is pinned. A consumer
matches a requested tag against `source.tag` or the release-list entry's
`tag`, as [Matching a request](#matching-a-request) defines.

### Platform, format, and variant

`os`, `arch`, `libc`, `format`, and `variant` are lowercase tokens made
of letters, digits, `_`, `-`, and `.`, starting with a letter or digit.
[Vocabularies](#vocabularies) lists the documented values. A value outside
those vocabularies is well-formed but does not match a host or unpack
with a consumer that does not know it. A vendor uses such a value only
for a platform or format this document has not named yet.

An absent `os`, `arch`, or `libc` means the artifact does not depend on
that host property. For example:

- A universal macOS binary has `os: "darwin"` and no `arch`.
- A statically linked Linux executable that loads no host C library has
  no `libc`.
- A script or jar has none of the three.

Every artifact requires `format`: an archive or installer type, or `raw`
for a bare executable. Artifacts that differ only in format carry the
same build, and a consumer chooses its preferred format. A vendor must
not publish two artifacts with the same `os`, `arch`, `libc`, `variant`,
and `format`. `packslip create` rejects such a pair; a consumer that
receives one refuses to choose between them.

`variant` distinguishes builds sharing `os`, `arch`, and `libc`, such as
`fips`, `baseline`, `debug`, `installer`, or `source`. Unless asked for a
variant, a consumer selects only artifacts without one.

### Executable paths and command names

An artifact's optional `bin` list names its executables. A string entry
is a path from the true archive root, with no top-level directory
stripped. For a bare executable, it is the artifact's own name without
its compression suffix.

When the command name on PATH differs from the file name, use an object:
`{ "path": "bin/oxlint-x86_64", "name": "oxlint" }`. Two entries may
share a path under different names, allowing aliases such as `pnpx` for
`pnpm`; a consumer may create links or copies.

A command name is the name as typed, without `.exe`. A Windows path
keeps its extension, such as `bin/tool.exe`, and the consumer places it
on PATH as `name.exe`. Names in `requires.bin` and a resource's `bin`
follow the same convention, so consumers compare them without adding or
stripping an extension. For compatibility, the reference implementation
reads an older document whose `name` includes `.exe` as if it did not.

### Requirements, resources, and provenance

An artifact's optional `requires` object describes what the host must
provide: the minimum OS version (`os_min`), minimum glibc version for a
`gnu` Linux build (`glibc_min`), shared libraries (`libs`), and external
commands (`bin`). OS versions use the OS's own terms, such as `12` for
macOS Monterey and `10.0.17763` for Windows. See
[Host requirements](#host-requirements).

The optional release-level `resources` list describes what ships beside
the executables. Each entry has a `kind` and exactly one source. See
[Resources](#resources).

An artifact's optional `provenance` list contains URLs of
[SLSA build provenance](https://slsa.dev/spec/v1.0/provenance) statements.
The packslip authenticates the release statement; separately verified
provenance establishes build claims at the
[SLSA build level](https://slsa.dev/spec/v1.0/levels) its builder supports.

### Signer and additional metadata

`identity` describes how the statement is signed and by whom, allowing a
consumer to compare its pin with the received signature. For
`sigstore-oidc`, `key_id` is the certificate's subject identity: a
workflow URI for CI or an email address for a person. `issuer` is the
OIDC issuer. For `sigstore-key`, `key_id` is the key ID in uppercase
hexadecimal. See [Signing](#signing).

`attested_by` is `vendor` by default, or `repackager`. A repackager
describes what it checked in `evidence`. See
[Repackager attestation](#repackager-attestation).

The optional `notes_url` points to release notes. `extensions` carries
additional metadata under keys naming the party that defines it. See
[Extensions](#extensions).

### Schema and validation

`packslip schema` prints the JSON schema, also published as
[release-v1.json](https://packslip.dev/schema/release-v1.json). The schema
checks types, required fields, token patterns, semver grammar, and digest
lengths. The reference implementation's validator checks the remaining
rules, including project-name grammar, the fixed `_type` and
`predicateType`, at least one artifact, and the cross-references between
subjects, artifacts, and resources. Passing schema validation alone does
not establish those relationships.

### Vocabularies

The documented platform tokens follow components of Rust target triples,
so a vendor can map its build matrix directly onto them:

| Field | Documented values |
|---|---|
| `os` | `linux`, `darwin`, `windows`, `freebsd`, `netbsd`, `openbsd`, `illumos`, `android`, `ios` |
| `arch` | `x86_64`, `aarch64`, `armv7`, `armv6`, `riscv64`, `i686`, `powerpc64le`, `s390x`, `loongarch64` |
| `libc` | `gnu`, `musl` for Linux builds |

The documented `format` values describe how the file is installed:

| File type | Formats | Consumer behavior |
|---|---|---|
| Archive | `tar.xz`, `tar.gz`, `tar.zst`, `tar.bz2`, `tgz`, `tar`, `zip`, `7z` | Unpack the archive. |
| Single compressed executable | `gz`, `xz`, `zst`, `bz2` | Decompress to the file named by `bin`. |
| Installer | `deb`, `rpm`, `dmg`, `pkg`, `msi`, `msix`, `exe`, `appimage` | Hand to the platform's installer or a launcher. |
| Bare executable | `raw` | Install the executable directly. |

A package manager installing into its own directory does not unpack
installer formats. A `.exe` that is the program itself uses `raw`; the
`exe` format means a Windows installer.

Paths in `bin` and `archive` sources use `/` between directories, as ZIP
entry names require. Windows PowerShell 5.1's `Compress-Archive` instead
writes `\` in ZIP entry names. Consumers interpret those backslashes as
slashes: `tool-1.0\bin\tool.exe` refers to
`tool-1.0/bin/tool.exe`.

`format` is required even when the file name has no format suffix.
Consumers select only formats they handle; an absent format cannot be
selected. Files that are not installable artifacts, such as a shared
library, header, or checksum sidecar, stay outside `artifacts`.

These vocabularies are open. Vendors may use well-formed tokens for types
not yet named here, following
[the token rules](#platform-format-and-variant); consumers skip formats
they cannot open.

### Field reference

Required fields inside an optional object are required when that object
is present. For example, `source` is optional, but a supplied `source`
must include `repo`. A resource must choose exactly one of `archive`,
`asset`, `repo`, and `exec`.

Paths beginning with `artifacts[]` or `resources[]` below are relative to
`predicate`; paths beginning with `requires` are relative to an artifact.

#### Statement fields

| Field | Type | Presence | Meaning |
|---|---|---|---|
| `_type` | string | required | Always `https://in-toto.io/Statement/v1`. |
| `subject[]` | array | required | One entry per artifact and per resource asset. |
| `subject[].name` | string | required | The file name. |
| `subject[].digest.sha256` | string | required | SHA-256 of the file, lowercase hex. |
| `subject[].digest.sha512` | string | optional | SHA-512 of the file, lowercase hex. |
| `predicateType` | string | required | Always `https://packslip.dev/release/v1`. |

#### Release fields

| Field | Type | Presence | Meaning |
|---|---|---|---|
| `predicate.project` | string | required | Host path naming the project, such as `github.com/jdx/mise` or `github.com/oxc-project/oxc/oxlint`. |
| `predicate.version` | string | required | Semver 2.0.0. Its prerelease part marks a prerelease and names the channel. |
| `predicate.published_at` | string | required | RFC 3339 UTC publish time. |
| `predicate.notes_url` | string | optional | URL of the release notes. |
| `predicate.source` | object | optional | Where the release was built from. |
| `predicate.source.repo` | string | required | Source repository URL. |
| `predicate.source.commit` | string | conditional | Commit the release was built from. Required when any resource uses `repo`. |
| `predicate.source.tag` | string | optional | Tag the release was built from, as the vendor spells it. |
| `predicate.artifacts[]` | array | required | One entry per artifact; at least one. |
| `predicate.resources[]` | array of object | optional | What ships besides the executables. See [Resources](#resources). |

#### Artifact fields

| Field | Type | Presence | Meaning |
|---|---|---|---|
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
| `artifacts[].provenance[]` | array of string | optional | URLs of SLSA build provenance statements for this artifact. |

#### Requirement fields

| Field | Type | Presence | Meaning |
|---|---|---|---|
| `requires.os_min` | string | optional | Minimum OS version in the OS's own terms. |
| `requires.glibc_min` | string | optional | Minimum glibc for a `gnu` Linux build. |
| `requires.libs[]` | array of string | optional | Shared libraries loaded from the host, by loader name (`libssl.so.3`, `vcruntime140.dll`). Read from the executables by `packslip create`; empty means none needed, absent means unchecked. |
| `requires.bin[]` | array of object | optional | Commands the executables need on PATH: `{ name, min? }`, a bare name and the lowest version that works. |

#### Resource fields

| Field | Type | Presence | Meaning |
|---|---|---|---|
| `resources[].kind` | string | required | `completion`, `man`, `cli-spec`, `skill`, `sbom`, `desktop`, `icon`, `app`, or a kind consumers may not know yet. |
| `resources[].artifact` | string | optional | Exact artifact file name. Limits this resource to that artifact and outranks platform-only scope. |
| `resources[].os`, `arch`, `libc` | string | optional | Limit the entry to artifacts of that platform, when layouts differ. |
| `resources[].archive` | string | one source | Path inside the selected artifact, from the archive root. |
| `resources[].asset` | string | one source | Name of a separate release file, listed in `subject` with its digest. |
| `resources[].url` | string | optional | Download URL of the asset. Only with `asset`. |
| `resources[].repo` | string | one source | Path in the source repository at `source.commit`, which is then required. |
| `resources[].exec[]` | array of string | one source | An argv whose first element is a `bin` name and whose stdout is the file. See [Running an exec entry](#running-an-exec-entry). |
| `resources[].env` | object of string | optional | Environment variables for the command, with `{shell}` substituted in values as in the argv. Only with `exec`. |
| `resources[].shell` | string | conditional | Required for a static `completion`: the shell it completes. |
| `resources[].shells[]` | array of string | conditional | Required for an `exec` completion: every shell it generates, substituted for `{shell}` in argv and `env` values. |
| `resources[].format` | string | conditional | Required for `cli-spec` (`usage`) and `sbom` (`cyclonedx`, `spdx`). |
| `resources[].bin` | string | conditional | The executable the entry is for, by its `bin` name. Required for `cli-spec`; for `completion` or `man`, required when the release has several executables, and identifying the only executable when omitted from a release with one. |
| `resources[].name` | string | conditional | Required for `skill`: its name. |

#### Signer and extension fields

| Field | Type | Presence | Meaning |
|---|---|---|---|
| `predicate.identity` | object | required | How the document is signed and by whom. See [Signing](#signing). |
| `predicate.identity.scheme` | string | required | `sigstore-oidc` or `sigstore-key`. |
| `predicate.identity.key_id` | string | required | The certificate identity, or the key id in uppercase hex. |
| `predicate.identity.issuer` | string | optional | The OIDC issuer, for `sigstore-oidc`. |
| `predicate.identity.pin_workflow` | boolean | optional | For `sigstore-oidc`: `false` asks consumers to hold later releases to the signing repository, not to the workflow that signed this one. `true` when absent. See [Workflow pinning](#workflow-pinning). |
| `predicate.attested_by` | string | optional | `vendor` (default) or `repackager`. See [Repackager attestation](#repackager-attestation). |
| `predicate.evidence[]` | array of object | optional | What a repackager checked: `{ kind, detail? }`. |
| `extensions` | object | optional | Vendor- or consumer-defined data, keyed by who defines it, on the release, each artifact, each resource, the release list, and each list entry. See [Extensions](#extensions). |

## Resources

The optional `resources` list describes content associated with the
release: completions, man pages, CLI specifications, agent skills, SBOMs,
and desktop integration files. Each entry has a `kind` and exactly one
source. Several entries can offer alternative sources for the same
resource; scope, specificity, and source priority determine which one a
consumer uses.

The entries also describe how consumers can install the software. A
command-line package manager installs executables listed in `bin`; a
desktop launcher uses `desktop` or `app` resources. A release can provide
both without assigning the application a single category.

### Source types

A resource comes from one of four sources. After
[resource selection](#selecting-resources) narrows the entries, consumers
prefer these sources in the order shown:

| Source | Content | Verification |
|---|---|---|
| `archive` | A path inside the selected artifact, from its true archive root. Available only when the artifact contains paths. | Covered by the artifact's digest. |
| `asset` | A separate release file, named in `subject`; the entry's `url` gives its download location. A skill directory can ship as its own archive this way. | Checked against the subject's digest, just like an artifact. |
| `repo` | A path in the source repository at `source.commit`. | Pinned by `source.commit`, which is required. |
| `exec` | The stdout of an argv whose first element is a `bin` name; `env` supplies command-specific environment variables. | The executable is verified; its generated output has no separate verification. |

An `exec` source is common for completions: cobra, clap, oclif, and usage
generate them from the binary without shipping a static file. If a vendor
also provides a static file, it lists that source first.
[Running an exec entry](#running-an-exec-entry) defines when a consumer
may run the command.

### Resource scope

An entry's optional `os`, `arch`, and `libc` fields limit it to artifacts
with matching field values. Each supplied field must equal the selected
artifact's field. Omitting all three imposes no platform restriction.

The optional `artifact` field names an exact file in `artifacts`. Use it
when artifacts for the same platform have different layouts or when a
resource belongs to one variant. The named artifact must exist and must
match any platform scope on the entry. For example, if a `fips` build
places its man page at a different path from `tool-linux-x64.tar.xz`,
name each artifact in its corresponding resource entry. The
[TOML manifest](https://packslip.dev/docs/describing-releases/#use-a-toml-manifest)
that `packslip create` reads spells this as
`artifact = "tool-linux-x64.tar.xz"` inside `[[resource]]`.

An `archive` entry never applies to an artifact whose `format` is bare:
`raw`, `gz`, `xz`, `zst`, or `bz2`. Such an artifact is the executable
itself, so it holds no path for the entry to name. A vendor that publishes
`tool-linux-x64.tar.xz` beside a bare `tool-linux-x64` therefore needs no
scope to keep an archive entry off the bare one; the source itself limits
its applicability. The same man page can still be supplied for a bare
artifact through an `asset` or `repo` entry.

### Resource identity

Entries compete only when they describe the same resource. Identity
consists of the `kind` and the following fields:

| Kind | Identity within that kind |
|---|---|
| `completion` | `bin` and `shell`. An `exec` entry offering several shells has one identity per shell. |
| `cli-spec` | `bin` and `format`. |
| `skill` | `name`. |
| `sbom` | `format`. |
| Every other kind | The source's file name, or the kind alone for an `exec` source. |

Entries with different identities never hide one another. A Linux-scoped
skill does not replace an unscoped skill with a different name, and a
platform-specific zsh completion does not affect a bash completion.

### Selecting resources

For each resource identity, a consumer applies these steps. The reference
implementation exposes this selection as `select_resources`.

1. Keep the entries that apply to the selected artifact.
2. Of those, keep the most specific. An entry that names `artifact`
   outranks every entry scoped only by platform; among entries equally
   scoped by `artifact`, the entries naming the most of `os`, `arch`, and
   `libc` win.
3. Try the remaining entries by source priority, as
   [Fallback and verification](#fallback-and-verification) defines.
4. Try entries with the same source type in their order in `resources`.
   This final tie-breaker is an ordered list of alternatives: the first
   usable entry wins, and a consumer stops fetching or running
   lower-priority entries once it has that thing.

For example, two equally scoped `repo` entries for one skill try the
first directory, then the second only if the first is unavailable. A
verification failure refuses the release; it is never a reason to try the
next entry.

### Resource kinds

#### Shell completions and man pages

`completion` supplies a shell completion script. A static source uses
`shell` to name its shell: `bash`, `zsh`, `fish`, `powershell`, `nushell`,
or `elvish`. An `exec` source uses `shells` to list every shell the
command generates. The consumer substitutes each shell for `{shell}`
in argv elements and `env` values. Common patterns are:

| Generator | `exec` | `env` |
|---|---|---|
| cobra or clap | `["tool", "completion", "{shell}"]` | None needed. |
| clap dynamic completions | `["tool"]` | `{ "COMPLETE": "{shell}" }` |
| click | `["tool"]` | `{ "_TOOL_COMPLETE": "{shell}_source" }` |

`bin` names the executable the completion is for. When a release has
one executable, the field may be omitted: the entry then identifies that
executable and competes with an entry that names it explicitly. When a
release has several executables, `bin` is required.

`man` supplies a man page whose section is its file suffix, as in
`mise.1`. Its `bin` identifies the documented executable using the same
rules as a completion.

#### CLI specifications

`cli-spec` is a machine-readable description of the executable named by
its required `bin`, in the specified `format`. The documented format is
`usage`, a [usage](https://usage.jdx.dev) spec. The consumer's own copy of
`usage` can generate completions for every shell it supports, a man page,
and Markdown documentation from that spec, without running vendor code
at install time.

Generated completions call `usage complete-word` when the shell requests
completion, so a consumer that generates them installs `usage` beside
the tool. Generated man pages and documentation have no runtime
dependency on `usage`. A vendor can also ship static completions for
consumers that want to avoid that dependency.

#### Agent skills

`skill` describes an agent skill in the
[Agent Skills](https://agentskills.io) format, identified by `name`. A
skill is a directory containing `SKILL.md` and the files it references.
Its source determines the layout:

- `archive` or `repo`: the path names the skill directory.
- `asset`: the file is an archive of the directory's contents.
  `SKILL.md` is either at the archive root or inside one top-level
  directory that the consumer strips. Nothing else is at the root.
- `exec`: the command prints a single `SKILL.md`.

A consumer counts a skill as present only after `SKILL.md` is in place.
A partially fetched directory does not satisfy the resource.

#### Software bills of materials

`sbom` supplies a software bill of materials with `format` set to
`cyclonedx` or `spdx`. Its source must be `archive`, `asset`, or `repo`,
so the content is covered by a digest or source commit; `exec` is never
allowed. A release with one SBOM per platform lists an entry per `os` or
per artifact archive.

#### Desktop integration

- `desktop` supplies a freedesktop
  [desktop entry](https://specifications.freedesktop.org/desktop-entry-spec/latest/)
  for a Linux launcher. AppImage, deb, rpm, and the Windows installer
  formats carry their own desktop integration.
- `icon` supplies an icon file. A hicolor path such as
  `share/icons/hicolor/512x512/...` identifies its size.
- `app` names a macOS application bundle inside a `dmg` or `zip`, by its
  path in the archive. A consumer copies the bundle to Applications.
  Only an `archive` source is allowed.

### Fallback and verification

After scope and specificity have narrowed the alternatives, a consumer
takes the first usable source for each resource in this order:

1. `archive`.
2. `asset`.
3. `repo`.
4. Content generated from a `cli-spec`.
5. `exec`.

Static `cli-spec` sources follow the same `archive`, `asset`, `repo`
priority. Within each source type, entries are tried in document order.
A consumer ignores kinds and sources it does not know, so a vendor may
publish a `font` or its own kind before the specification names it.

Resources are optional to installation. If an asset or repository file is
unavailable, or a network request fails, the consumer may try the next
eligible source. If no source is usable, it reports the missing resource
and completes installation of the executables; a later attempt may fetch
the resource.

Verification failure has a different consequence. A resource whose
digest differs from `subject`, or whose repository content differs from
`source.commit`, causes the consumer to refuse the release. It does not
try another source, just as it would not accept an artifact with the
wrong digest.

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

Whenever it runs an `exec` entry, the consumer:

- Places the release's executables on PATH.
- Uses a directory of its own outside the user's project.
- Supplies no standard input and discards standard error.
- Applies a timeout it chooses; a few seconds suits a completion.
- Substitutes the requested shell for `{shell}` in argv and `env` values.
- Adds `env` to an environment that is otherwise the consumer's own.

A non-zero exit, timeout, or empty output makes the entry unavailable for
this attempt. The consumer may try it again later.

## Host requirements

`requires` describes what the host must provide before the software can
run. Its four optional fields are `os_min`, `glibc_min`, `libs`, and
`bin`. They use OS versions, loader names, and command names, allowing
consumers to check requirements without a shared package registry.

### Minimum OS and glibc versions

`os_min` is the lowest supported OS version, expressed in the OS's own
terms. `glibc_min` is the lowest supported glibc version for a `gnu`
Linux build.

For `os: "linux"`, `os_min` names the Linux kernel version, excluding local
build suffixes such as `-18-amd64` or `+`. It does not name a distribution's
release: the OS selector identifies Linux, with no distribution identifier.
For `darwin`, `os_min` names the macOS product version. For `windows`, it
names the Windows major, minor, and build numbers.

### Shared libraries

`libs` lists shared libraries the executables load from the host, using
the names resolved by the loader:

| Platform | Name | Examples |
|---|---|---|
| Linux and FreeBSD | soname | `libssl.so.3`, `libstdc++.so.6` |
| Windows | DLL name | `vcruntime140.dll` |
| macOS | dylib file name | `libssl.3.dylib` |

The list excludes the C runtime and loader already described by `libc`
and `glibc_min`: libc, libm, libdl, libpthread, librt, libgcc_s, and the
dynamic loader; `/usr/lib` and `/System/Library` on macOS; and DLLs
shipped by Windows. It also excludes libraries bundled in the artifact
and found through its own rpath.

`packslip create` reads the list from the executables named in the
artifact's `bin`. A consumer with the artifact can inspect the same
bytes; the signed release manifest lets it check before downloading.
Presence of the field records whether that inspection happened:

- `"libs": []` means the executables were inspected and need no host
  libraries beyond the excluded ones.
- An absent `libs` means libraries were not checked, as with an
  installer that `create` does not open or an executable that is a
  script.

### Required commands

`requires.bin` lists commands the release's executables invoke and
cannot work without. Each entry has a bare `name`, as invoked by the
executable (`java`, `python3`, or `git`), with no directory or `.exe`
suffix. Its optional `min` gives the lowest supported version.

The vendor declares these requirements; binary inspection does not
establish that a program invokes `java`. Only required commands belong
here. Optional integrations go under `extensions`, and a required
command must not be one the release itself provides.

### Comparing versions

`min`, `glibc_min`, and `os_min` are numeric lower bounds, so a `min` of
`17` means 17.0.0 and later, including 21. A consumer compares
dot-separated nonnegative integer components numerically, padding missing
components with zero; `2.10` exceeds `2.9`. If either spelling cannot be
compared this way, the check is unknown and the consumer warns rather
than guessing.

### Checking requirements before installation

Select the artifact first, as
[Selecting an artifact](#selecting-an-artifact) defines, then check its
requirements before installing. Requirements do not break selection ties
or silently redirect installation to another build.

The consumer's response depends on which requirement failed:

| Check result | Required response |
|---|---|
| A library in `libs` is missing, or the host is below `glibc_min` or `os_min`. | Refuse installation and explain what is missing in the consumer's own terms, such as a distribution package providing the soname or an OS upgrade. The user may override the refusal for that tool. |
| A command in `bin` is missing or below `min`. | Install with a warning naming the command and required version. If the consumer can install that command, present it as an available installation. |
| A check is unknown. | Warn and continue, including when versions are unreadable or the loader cannot be queried. |

Missing libraries or unsupported OS and glibc versions prevent the
executables from starting. A missing command affects the code paths that
invoke it, and the user may intend to install that command next.

Check libraries using the loader's own search locations: the dynamic
linker's cache and search path on Linux (`ldconfig -p`,
`LD_LIBRARY_PATH`), system library and framework directories on macOS,
and PATH with the system directory on Windows. Find commands on PATH,
adding `.exe` on Windows, and run them with `--version` to read their
versions. Compare `min`, `glibc_min`, and `os_min` against the command,
glibc, and OS versions respectively. A failed version probe or an
unrecognized version spelling leaves the check unknown.

### Limits of host requirements

`requires` uses names the OS resolves. It does not name other projects or
their versions, or say where to get them. A package manager's installation
hints belong under `extensions`.

Version 1 does not express alternative commands satisfying one
requirement, such as `terraform` or `tofu`, or required library symbol
versions, such as `libstdc++.so.6` at `GLIBCXX_3.4.29`. `glibc_min`
expresses a version requirement for libc alone.

## Extensions

Use `extensions` for metadata this specification does not define, such
as installation hints, an end-of-life date, or a build ID. An extensions
object is allowed on the release predicate, each artifact, each resource,
the release-list predicate, and each release-list entry:

```json
"extensions": {
  "example.com": { "build_id": "20260901.3" }
}
```

Each key identifies the party that defines the value. Consumers use
their names, such as `mise` or `pacvamp`; vendors use domains they
control, such as `example.com`. That party documents the value's meaning.
The signature covers extension data, and `packslip show` prints it
unchanged. A consumer reads the keys it defines and ignores the rest.

packslip does not and will not assign meaning to data under `extensions`
or claim its keys for future standard fields. This keeps namespaced data
from colliding with later additions to the format.

Outside `extensions`, consumers also ignore unknown fields so revisions
of version 1 can add fields without breaking older consumers. Vendors
must put their own data in `extensions` rather than inventing fields
elsewhere: a future revision could assign an invented field a different
meaning. If an extension becomes useful across many vendors, the format
may add a standard field for it; the extension key continues working
alongside that field.

## Repackager attestation

When a vendor publishes no packslip, a package repository (such as apt or
AUR), registry, or mirror may describe and sign the vendor's artifacts
with `"attested_by": "repackager"`. Mirrors can also attest to files
already covered by a vendor's packslip, as described below.

The `project` still identifies the vendor's project, and the artifacts
remain the vendor's files. `identity` identifies the repackager that
signs this statement. `evidence` records what it checked before signing:

```json
"attested_by": "repackager",
"evidence": [
  { "kind": "apt-release-gpg", "detail": "3FEF9748469ADBE15DA7CA80AC2D62742012EA22" },
  { "kind": "pkgbuild-checksums" }
]
```

Each evidence entry has a `kind` and optional `detail`. The documented
kinds are:

| Kind | Evidence checked |
|---|---|
| `pkgbuild-checksums` | Artifact digests matched the packaging maintained by the repackager. |
| `checksum-file-over-tls` | The vendor's unsigned checksum file, retrieved over TLS. |
| `apt-release-gpg` | An apt index signed with the key identified in `detail`. |
| `vendor-signature` | A detached signature published by the vendor. |
| `vendor-packslip` | The vendor's packslip was verified; `detail` is the SHA-256 of its bundle file. |
| `github-attestation` | GitHub artifact attestations were verified. |
| `provenance-verified` | SLSA provenance was verified against the builder. |
| `scan` | The artifacts were scanned; `detail` points to the report. |
| `none` | Nothing was checked; the document rests on the repackager's signature alone. |

A verified repackager document authenticates the repackager's publication
of these digests and its statement that it checked the listed evidence.
It does not establish vendor claims the vendor did not sign. Consumers
rank it below a vendor document. A consumer that already holds a vendor
document for a project refuses to replace it with a repackager document
without a person's approval.

A mirror is a repackager whose document keeps the vendor's digests,
substitutes the mirror's download URLs, and includes `vendor-packslip`
evidence. A consumer using the mirror obtains the same bytes under the
mirror's pin. It can also fetch and verify the vendor's document to
retain both attestations.

## Discovery

Publish each release's bundle alongside its artifacts, either as a
release asset or in the release's directory on a download site.

Discovery tells a consumer which releases exist and where to fetch
their bundles. Projects on their own domains publish a
[signed release list](#the-signed-list). GitHub projects use the
[repository's releases endpoint](#github) and may add a signed list to
withdraw releases, mark security fixes, map tags to versions, or recommend
a default. Discovery does not replace verification of the selected
release.

### The signed list

A release list uses the same signed bundle structure as a release
packslip, but its statement has the `releases/v1` predicate. Its subjects
pin release bundles rather than artifacts:

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

Each entry copies `version` and `published_at` from the release's
packslip. If the packslip has a `source.tag`, the entry also copies it as
`tag`, preserving the vendor's spelling. Entries may additionally carry:

- `status: "yanked"` for a withdrawn release, optionally with a
  `status_reason`.
  Consumers never select a yanked release and warn when they hold one.
- `security: true` for a release that fixes a vulnerability. A consumer
  may shorten its minimum release age for that release.
- `evidence` on a list from someone other than the vendor, describing
  what that publisher checked.

The optional list-level `latest` recommends the vendor's default
release. It must match an entry's exact semver `version`, including any
build metadata; it is neither a tag, a range, nor a per-release flag. A
version absent from the list makes the list invalid. A listed target that
is yanked or otherwise ineligible leaves the list valid but triggers the
[Latest](#latest) fallback rules.

#### List fields

| Field | Type | Presence | Meaning |
|---|---|---|---|
| `_type` | string | required | Always `https://in-toto.io/Statement/v1`. |
| `subject[]` | array | required | One entry per listed packslip. |
| `subject[].name` | string | required | The packslip's URL, as its entry's `packslip` gives it. |
| `subject[].digest.sha256` | string | required | SHA-256 of the bundle file, lowercase hex. |
| `subject[].digest.sha512` | string | optional | SHA-512 of the bundle file, lowercase hex. |
| `predicateType` | string | required | Always `https://packslip.dev/releases/v1`. |
| `predicate.project` | string | required | The project the list is for. See [Project names](#project-names). |
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

#### Freshness and rollback protection

Each subject names a listed packslip's URL and carries the digest of its
bundle file. The consumer therefore checks the exact document the list
selected, even when a URL's contents change.

The list also carries two freshness controls, borrowed from TUF's
timestamp role:

- `expires_at` limits how long the list can be used. A consumer refuses
  an expired list, so a mirror cannot serve an old view indefinitely.
- `sequence` prevents rollback. A consumer remembers the highest value
  it has accepted and refuses a lower one.

These controls apply to the list, separately from verification of its
release bundles.

#### Publishing a list

For a project on its own domain, publish the list at:

```text
https://<host>/.well-known/packslip/<path>.json
```

Here `<host>` is the project name's host and `<path>` is the remainder
of its name. A bare host instead uses
`https://<host>/.well-known/packslip.json`.

| Project | List URL |
| --- | --- |
| `mise.jdx.dev` | `https://mise.jdx.dev/.well-known/packslip.json` |
| `jdx.dev/mise` | `https://jdx.dev/.well-known/packslip/mise.json` |
| `github.com/jdx/mise` | `https://github.com/.well-known/packslip/jdx/mise.json` |

GitHub does not serve the last URL; [GitHub](#github) defines its discovery
path instead. A forge that serves its well-known list needs no additional
discovery mechanism. A project on its own domain must publish the list:
if none exists, the consumer refuses the project rather than guessing
download URLs.

The project's domain anchors its identity, while the list and release
bundles point to wherever the bytes are hosted. A vendor with only a git
repository can keep the list and bundles at paths served as raw files
and point artifact URLs at another host, including an LFS store. A
forge's release API is not required.

The reference library exposes `list_url` for both a project's own list
and a list kept by [another publisher](#lists-from-other-publishers).
`github_list_path` returns the repository-relative path for a GitHub
supplementary list. The CLI's `packslip releases` command builds a list
from local release bundles; `--latest` sets the list-level recommendation,
and omitting that option leaves the list without one. The list's JSON
schema is at `https://packslip.dev/schema/releases-v1.json`.

### GitHub

For `github.com/<owner>/<repo>[/<subpath>]`, the repository's releases
endpoint supplies discovery without requiring a signed list:

- Include only non-draft releases carrying a packslip for the requested
  `project`. A packslip may name the same repository under a new name,
  subject to [Repository identity and renames](#repository-identity-and-renames). GitHub redirects the old
  API and release URLs to the renamed repository, so requests using the
  old name still reach its releases.
- Derive each version from its tag using [Tags](#tags). This lets a
  consumer list versions without fetching every bundle. On installation,
  verify the packslip and refuse the release if its signed `version`
  differs from the version derived from the tag. A tag that names no
  version is omitted from tag-based discovery.
- Determine ordering and prerelease status from the version, ignoring
  the endpoint's order and prerelease flag. GitHub's latest-release
  pointer is an unsigned recommendation governed by [Latest](#latest).

#### Supplementary signed lists

The repository may also carry a signed list on its default branch:
`.well-known/packslip.json`, or `.well-known/packslip/<subpath>.json` for
a monorepo tool (`.well-known/packslip/crates/cli.json` for
`github.com/biomejs/biome/crates/cli`). GitHub serves the file at
`https://raw.githubusercontent.com/<owner>/<repo>/HEAD/<path>`.

The list is signed by the same identity as the release packslips. It
supplements the endpoint rather than replacing it:

- For a listed version, the entry's `tag`, `status`, and `security` apply,
  and the consumer accepts only the bundle whose digest the list pins.
- Versions omitted from the list remain discoverable through the
  endpoint.

Publish a supplementary list only when the project needs to withdraw a
release, mark a security fix, recommend a default with `latest`, or map a
tag that names no version. A project needing none of those omits the
list. A signed withdrawal also works with immutable GitHub releases,
where the release and its bundle cannot be deleted.

#### Keeping a supplementary list available

After a consumer accepts a supplementary list, it remembers that the
project uses one. A later missing list is an error: returning to
endpoint-only discovery would silently undo signed withdrawals.

The vendor must re-sign the list before every `expires_at`, even if no
new release has shipped, because consumers refuse expired lists. To
retire its entries, publish a fresh, unexpired list with a nondecreasing
sequence. A user may explicitly clear the remembered list policy for
that project.

### Lists from other publishers

A registry, mirror, or scanning service can publish a list under its own
host. It publishes one list per vendor project, using the vendor's full
project name in the well-known path:

```text
https://registry.example/.well-known/packslip/github.com/jdx/mise.json
```

The library's `list_url` returns this URL for a publisher other than the
project's host. A consumer trusts a publisher only through configuration
and keeps a pin for each trusted host.

Each entry pins a release bundle's digest. That bundle may be the
vendor's packslip or a repackager document signed by the list's publisher.
Verify a vendor document against the vendor's pin implied by `project`.
Verify a repackager document against the pin implied by its own `project`
and `attested_by`. The list's signature establishes who selected the
releases and what their `evidence` says the publisher checked. It does
not establish who built the artifacts.

This selection is a **stamp**, and the publisher is a **stamping host**.
Stamping lets a registry require a scan or an organization curate
versions without maintaining a per-tool installation recipe. A consumer
configured to trust stamping hosts applies these admission rules:

- Offer and install only versions listed by at least one trusted host.
  A valid vendor document alone does not admit an unlisted version.
- Any one trusted host's non-yanked stamp suffices. A host withdrawing its
  own stamp does not veto another trusted host's approval; an operator
  that needs one host to control admission configures that host alone.
- A vendor withdrawal still excludes the version regardless of stamps.
- A user may choose to trust the vendor alone for a particular project.
  For that project, accept the vendor's document under the vendor's pin
  without requiring a stamp.

A stamping host can use any signing scheme that fits its operation. For
example, a continuous scanning service can hold a key, while a registry
publishing from a repository can sign keylessly. In either case, the
consumer pins the host.

## Versions

The signed `version` must use semver 2.0.0: `MAJOR.MINOR.PATCH`, with an
optional prerelease part after `-` and optional build metadata after `+`.
Calendar versions such as `2026.9.1` qualify. Both `packslip create` and
consumers refuse other spellings. `source.tag` preserves the vendor's
tag, which may have a different spelling such as `v2026.9.1` or
`oxlint_v1.0.0`.

### Ordering, prereleases, and channels

Derive all three properties from the signed version string:

- **Ordering:** use semver precedence, which gives ranges such as `^1.2`
  meaning. Build metadata does not affect order. A backport
  `20.19.1` still ranks below `22.0.0` even when published later; neither
  GitHub endpoint order nor release-list order changes that. A vendor's
  separate recommendation affects only [Latest](#latest).
- **Prerelease status:** a prerelease part makes the version a
  prerelease. `1.2.0-rc.1` is a prerelease; `1.2.0` is not. Skip
  prereleases unless requested. Promoting a release candidate requires
  publishing the final version, rather than changing a flag.
- **Channel:** the first prerelease identifier names the channel if it
  is not numeric. A channel request selects only releases on that
  channel, ordered by semver precedence. Vendors choose channel names;
  packslip defines none.

| Version | Channel |
| --- | --- |
| `1.3.0-nightly.20260904` | `nightly` |
| `1.2.0-beta.2` | `beta` |
| `1.2.0-rc.1` | `rc` |
| `1.2.0-1` | None: the first prerelease identifier is numeric. |
| `1.2.0` | None: the version has no prerelease part. |

Separate prerelease and channel fields could contradict the signed
version or mutable forge metadata, so the format does not store them.
Withdrawals and default recommendations instead belong in discovery
metadata, where they can change without replacing a signed release
manifest.

### Spelling a version

A vendor using another version spelling must choose a semver spelling
once and keep it. The tag retains the original spelling.

- A prefix belongs to the tag, not the version: `jq-1.7.1` is version
  `1.7.1`. [Tags](#tags) lists the prefixes a consumer strips on its own;
  a tag with any other prefix, such as `release-1.2.3`, needs a signed
  list entry to map it.
- A missing patch component is `0`: `4.1` is `4.1.0`.
- Remove leading zeros: `25.07.1` becomes `25.7.1`, and the date
  `2026.08.31` becomes `2026.8.31`. The dashed date `2026-08-31` uses
  that same semver spelling.
- A fourth ordered component has no semver representation. The format
  cannot represent a scheme that needs one: build metadata does not
  affect ordering, and a prerelease part has a different meaning.

Two releases must not share a version. If the vendor's spelling would
collide, as with two releases on the same day, the semver spelling must
distinguish them with an ordered component rather than build metadata.

### Tags

To derive a forge release's version from its tag, remove an optional
project prefix and an optional `v`, then apply
[Spelling a version](#spelling-a-version) to the remainder. An allowed
prefix is the tool's full subpath, the last segment of that subpath, or
the repository name, followed by `/`, `-`, `_`, or `@`.

| Project | Tag | Version |
| --- | --- | --- |
| Any project | `v1.2.3` | `1.2.3` |
| `github.com/oxc-project/oxc/oxlint` | `oxlint_v1.0.0` | `1.0.0` |
| `github.com/biomejs/biome/crates/cli` | `cli/v1.9.4` | `1.9.4` |
| `github.com/jqlang/jq` | `jq-1.7.1` | `1.7.1` |
| Any project | `v4.1` | `4.1.0` |

Deriving a version from a tag avoids fetching every bundle during
listing. The signed packslip remains authoritative: if its `version`
differs from the derived version, refuse the release. Tags such as
`nightly` and `release-1.2.3` do not name versions under these rules and
are omitted from tag-based listing. To expose such a release, publish a
signed list entry mapping its tag to its version.

### Matching a request

A version request matches a prefix of dot-separated components:

| Request | Matches |
| --- | --- |
| `20` | Every `20.x.y` release. |
| `3.12` | Every `3.12.x` release. |
| `1.2.0-beta` | The betas of `1.2.0`. |

A request may also use the vendor's tag, with or without its leading
`v`. Match that spelling against `source.tag` or the release-list entry's
`tag`, so a user can request a release by the name the vendor published.

A matching release is eligible only if it is not yanked, satisfies the
prerelease policy, and belongs to the requested channel when one was
given. Prereleases require an explicit request. The release list's
`sequence` supplies rollback protection; a single release manifest does
not.

### Latest

An unconstrained `latest` request, including a consumer's default install
request, asks for the vendor's recommended eligible release. For example,
a vendor may publish `3.0.0` while recommending `2.8.4` as the default.
Exact versions, version prefixes, ranges, and channel requests still use
their own [matching rules](#matching-a-request) and semver precedence;
the recommendation does not change their ordering.

#### Finding the recommendation

Use the first applicable source:

1. **Vendor's signed list.** If the accepted list has `latest`, use it.
   For GitHub projects, this is the supplementary list. Verify the list's
   signature, identity, expiry, and sequence before using the pointer.
2. **GitHub's latest release endpoint.** For a GitHub project without a
   signed `latest`, use the release returned by the
   [latest release endpoint](https://docs.github.com/en/rest/releases/releases#get-the-latest-release).
   Resolve its tag through normal discovery, including any signed list
   entry that maps the tag. The release must belong to the requested
   project: a repository-wide pointer for another tool supplies no
   recommendation for this one. The endpoint is an unsigned discovery
   hint; it supplies neither authenticity nor sequence protection.
3. **No recommendation.** Outside GitHub, a project without a signed
   pointer has no recommendation.

Pointers in third-party stamping lists do not replace the vendor's
recommendation. Those lists control which candidates are admitted.

#### Checking the recommended release

Verify a recommended candidate exactly as any other release: check its
signature and identity, then the manifest's version and digest
consistency. A verification failure is an error.

Select a verified recommendation only if it also passes every
eligibility check: vendor withdrawals, prerelease policy, minimum release
age, configured stamping policy, and artifact and host eligibility. A
recommendation does not waive any of those checks.

#### Falling back to version precedence

If there is no recommendation, or its target is absent from discovery or
excluded by eligibility policy, select the highest-precedence eligible
release from the normal candidate set. An ineligible signed pointer
falls directly back to semver selection; do not try GitHub's pointer
instead. Report any skipped recommendation and the reason. Fail if no
eligible release exists.

For example, when the recommended `2.8.4` is yanked or too young, the
highest eligible release may be `3.0.0`. To exclude `3.0.0`, the vendor
must withdraw it or use an
[admission policy](#lists-from-other-publishers). A `latest` pointer
recommends a release; it does not exclude other versions.

Fallback applies to an absent or ineligible recommendation, never to a
verification or fetch failure:

- An invalid, expired, rolled-back, or unexpectedly missing signed list
  is an error.
- A candidate with a bad signature or inconsistent digest or version is
  an error.
- A GitHub latest-endpoint response indicating that no latest release
  exists supplies no pointer. Other fetch errors remain errors.

Change or remove a signed recommendation by publishing a new list with
an increased sequence. No release manifest needs to be replaced.

## Selecting an artifact

The first three rules filter candidates; the remaining rules choose one
of those candidates. The reference library exposes this process as
`select_artifact`.

1. **Match the host.** Each of `os`, `arch`, and `libc` must either be
   absent or match the host. If the host's libc is unknown, accept only
   artifacts without a `libc`. A consumer may support additional
   fallbacks, such as a `gnu` host using a `musl` build or an `aarch64`
   macOS host using `x86_64` under Rosetta. Rank exact matches first and
   report any fallback used.
2. **Match the variant.** If a variant was requested, keep only artifacts
   carrying it. Without a requested variant, keep only artifacts that
   have none.
3. **Check format support.** Keep only formats the consumer can handle.
4. **Prefer specificity.** Choose the artifact naming the most of `os`,
   `arch`, and `libc`. A host-specific build therefore beats a portable
   build.
5. **Apply format preference.** For equally specific artifacts, use the
   consumer's format preference. A typical order is `tar.xz`, `tar.zst`,
   `tar.gz`, `tgz`, `tar.bz2`, `tar`, `zip`, `7z`, then single compressed
   executables, then `raw`.
6. **Refuse unresolved ties.** Two remaining artifacts with equal rank
   are a vendor error. The consumer refuses to guess between them.

## Consumer rules

The rules cover discovery, verification, selection, and installation.
Consumers must keep state across installs to enforce signer continuity,
no-downgrade policy, and release-list rollback protection. The rules and
their linked sections use this remembered state:

- **Pins:** the signer and its repository ID, any signer fingerprint or
  lockfile commitment, and the `pin_workflow` value.
- **Accepted trust properties:** the last accepted release's
  `identity.scheme`, `attested_by`, and provenance.
- **List history:** the highest accepted `sequence` and whether a signed
  list has ever been accepted.
- **User choices:** approved signer changes, permission to accept
  unlogged bundles, and decisions to trust a vendor without a stamp.

1. **Pin the identity once.**
   - For a forge project, use the project name to establish the first
     pin: accept only the forge's issuer and an identity under that
     repository. Remember the repository ID in the certificate. That ID
     then pins the project as [Repository identity and renames](#repository-identity-and-renames) defines:
     renames and transfers preserve the pin, while a repository recreated
     under the old name does not inherit it.
   - If the consumer has independently read the project's published
     [signer fingerprint](#signer-fingerprints), include it in the first
     pin and check releases against it. This avoids trusting a release
     on first use.
   - For other projects, pin a public key or identity from the consumer's
     maintained pins, or from the well-known list on first use.
   - Trust another publisher's list per host, through configuration.
   - Never take a key from the document itself or trust a bundle's key
     hint.
2. **Verify the bundle and downloaded files.** Verify the signature,
   certificate chain, and log entry as Sigstore defines them, then the
   statement's structure. For a forge project, check that the statement
   names the requested project or the same repository renamed, under
   [Project names](#project-names) and [Repository identity and renames](#repository-identity-and-renames). Then verify the
   subject digest of every downloaded artifact or asset and the size of
   every artifact. Refuse a bundle without a log entry unless unlogged
   bundles were explicitly allowed under [Signing](#signing).
3. **Enforce no-downgrade.** Compared with the last release accepted for
   the project, refuse a release:
   - whose `identity.scheme` is weaker;
   - whose signer changed without a person's approval;
   - whose `attested_by` went from vendor to repackager;
   - that dropped per-artifact provenance the last release carried.

   Compare keyless signers by workflow path, without the ref: a new tag
   from the same workflow keeps the same signer. Across a repository
   rename or transfer, compare the path inside the repository only after
   the repository IDs establish that it is the same repository. Use the
   ID from the release's certificate, or, when comparing two recorded
   signers such as lockfile entries, the ID recorded with each. A changed
   repository ID means a changed signer even if the identity string
   matches.

   With `identity.pin_workflow: false`, compare by repository alone. If
   the remembered value is `true`, changing it to `false` weakens the pin
   and requires a person's approval under
   [Workflow pinning](#workflow-pinning).
4. **Apply the minimum release age.** Measure any configured age from the
   log's integration time. Use `published_at` only for an unlogged bundle
   the consumer chose to accept.
5. **Use the project's discovery metadata.**
   - For GitHub projects, read the releases endpoint and any supplementary
     signed list. For other projects, read the signed list at the
     well-known URL. Refuse a project with neither discovery source, and
     refuse when a previously accepted signed list is now missing, as
     [GitHub](#github) describes.
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
6. **Select one artifact.** Apply
   [Selecting an artifact](#selecting-an-artifact) and refuse unresolved
   ties.
7. **Check host requirements before installing.** Check `requires` for
   the artifact selected by rule 6, using
   [Host requirements](#host-requirements). Refuse an install when a
   library, glibc, or OS version prevents the executables from starting.
   Warn when a required command is missing or too old, and report either
   outcome in the consumer's own terms. A requirement the consumer cannot
   check produces a warning, never a refusal. Requirements do not resolve
   an ambiguous artifact selection.
8. **Install resources.** Apply [Resources](#resources):
   - For each resource identity, filter to entries whose scope fits the
     selected artifact, then keep the most specific entries under
     [Selecting resources](#selecting-resources). Choose the most verifiable
     source in the order given by [Source types](#source-types) and
     [Fallback and verification](#fallback-and-verification).
   - Run an `exec` completion when a shell first requests it and cache
     the result. Run other `exec` entries only if the user chose to run
     vendor code at install time, under
     [Running an exec entry](#running-an-exec-entry).
   - Ignore kinds the consumer does not know.
   - If a resource cannot be fetched, report it and finish the install
     without it. If its digest differs from the signed digest, fail the
     install.

## Stability

Version 1 is final. The predicate types
`https://packslip.dev/release/v1` and `https://packslip.dev/releases/v1`
identify this format contract: a document that verifies under them today
continues to verify for as long as its signing material holds.

An unchanged verifier may still need updates to support new Sigstore
bundle or log formats. The reference implementation's
[compatibility matrix](https://packslip.dev/docs/compatibility/) records
the release baselines, historical verification material, and
authenticated root rotations it has tested. Security policy and
freshness checks continue to apply; format stability does not require
accepting expired metadata or overriding a security rejection.

### What version 1 holds fixed

- Every defined field's meaning and default. A field cannot be redefined,
  narrowed, or given a different default.
- Field names and types, and the vocabularies that constrain them.
- The rules that decide which release and which artifact a consumer
  installs: [Versions](#versions), [Discovery](#discovery),
  [Selecting an artifact](#selecting-an-artifact), [Resources](#resources),
  [Host requirements](#host-requirements), and
  [Consumer rules](#consumer-rules).
- The absence of specification-defined meaning under
  [`extensions`](#extensions). This specification will never assign
  meaning to their contents.

### What version 1 may add

- Add an optional field at any level. Consumers ignore fields they do not
  know, so an older consumer reads a newer document correctly and a newer
  consumer reads an older one.
- Add a vocabulary member, such as a resource kind, evidence kind, format,
  OS token, or architecture token. Consumers ignore unfamiliar members
  except where a rule here says otherwise.
- Clarify ambiguous wording where implementations already agreed on its
  meaning, or add examples and guidance.

Removing a field, making an optional field required, changing a selection
rule's result, or making any change outside these bounds requires version
2. That version gets its own predicate type, published alongside version
1, and consumers may accept both. No revision of this specification
invalidates a release already signed under `release/v1`, and vendors are
not required to migrate.

### Format and implementation versions

The predicate types version the format. Separately, the reference Rust
crate, CLI, and two GitHub Actions share an implementation semver
version. The implementation may reach version 2 while the format remains
at version 1.

### Conformance coverage

The
[conformance vectors](https://github.com/jdx/packslip/tree/main/tests/conformance)
express the covered rules as executable examples: artifact and resource
selection, tag versions, statement validity, forge identity, and signer
fingerprints. An implementation that disagrees with a vector disagrees
with this specification.

The forge identity vectors include a remembered pin in each case. The
other vectors do not cover rules that depend on state retained between
installs, such as no-downgrade and release-list sequences. Consumers must
implement those [stateful rules](#consumer-rules) as well.

## Tooling

The [packslip repository](https://github.com/jdx/packslip) contains the
reference Rust library, CLI, and two composite GitHub Actions. The
[release action](https://github.com/jdx/packslip/blob/main/action.yml)
publishes a release's packslip; the
[release-list action](https://github.com/jdx/packslip/blob/main/releases/action.yml)
builds and signs discovery metadata.

The CLI creates and verifies release and list bundles, prints signer
fingerprints, and installs tools. Its commands serve different parts of
the consumer workflow:

- `packslip install` discovers releases, selects and installs an artifact,
  and remembers trust state across installs.
- `packslip verify` checks a supplied bundle and local files without
  maintaining that installation state.

Other consumers build their discovery, installation, and trust policies
around the library's verification and selection operations. Use the
guides for task-oriented examples and the command reference for flags
and arguments:

| Task | Guide | Command reference |
| --- | --- | --- |
| Install the CLI | [Installation methods](https://packslip.dev/docs/getting-started/#install-packslip) | [CLI overview](https://packslip.dev/cli/) |
| Install an upstream tool | [Install a tool with packslip](https://packslip.dev/docs/bootstrap/) | [install](https://packslip.dev/cli/install/) |
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
| Export JSON schemas | [JSON schemas](#json-schemas) | [schema](https://packslip.dev/cli/schema/) |
| Check another implementation | [Conformance vectors](https://github.com/jdx/packslip/tree/main/tests/conformance) | — |

The CLI reference is generated from command help, and the specification
page is generated from this source. See
[Contributing](https://github.com/jdx/packslip/blob/main/CONTRIBUTING.md)
for the documentation workflow.
