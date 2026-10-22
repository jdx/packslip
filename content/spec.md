---
title: packslip release/v1 specification
url: /release/v1/
aliases:
  - /releases/v1/
toc_end_level: 4
---
# packslip: a signed release manifest

Version 1. Predicate types `https://packslip.dev/release/v1` and
`https://packslip.dev/releases/v1`.

Authors: Jeff Dickey ([@jdx](https://github.com/jdx)) and Shunsuke Suzuki
([@suzuki-shunsuke](https://github.com/suzuki-shunsuke)).

## Purpose and scope

A publisher describes a release once, in a signed release bundle that any
consumer can verify against a trusted identity or key. The bundle lists
artifacts and their digests, platforms, executable paths, resources, and
provenance links. Artifacts may be archives, installers, bare executables,
source tarballs, or other release files.

A release bundle lets consumers interpret the release without a per-vendor
file-name recipe or registry entry. It does not prescribe a package
manager, download host, or installation directory.

The format is an
[in-toto statement](https://github.com/in-toto/attestation/blob/main/spec/v1/statement.md)
inside a [Sigstore bundle](https://github.com/sigstore/protobuf-specs).
The statement's predicate carries the release metadata. The Sigstore
bundle carries the signature and verification material.

This specification defines two predicate types:

| Predicate | Describes | Defined in |
| --- | --- | --- |
| `https://packslip.dev/release/v1` | One release and its files. | [The release statement](#the-release-statement) |
| `https://packslip.dev/releases/v1` | A release list with mutable discovery metadata. | [The release list](#the-release-list) |

## Reading this specification

This specification defines the format and the rules for consumers that
verify or install releases. For working examples or CLI instructions,
start with the [guides](https://packslip.dev/docs/). Implementers should
read the [consumer rules](#consumer-rules) alongside the field
definitions: valid JSON alone is not enough to accept a release.

Start with the sections for your task:

- **Identify and authenticate a project:** [Bundle format](#bundle-format),
  [Signing](#signing), [Project names](#project-names),
  [Keyless signer identity](#keyless-signer-identity), and
  [What a verified packslip proves](#what-a-verified-packslip-proves).
- **Describe a release:** [The release statement](#the-release-statement),
  including its artifacts; [Resources](#resources);
  [Host requirements](#host-requirements); [Extensions](#extensions); and
  [Repackager attestation](#repackager-attestation).
- **Find a release and choose a file:** [Discovery](#discovery),
  [Versions](#versions), and
  [Selecting an artifact](#selecting-an-artifact).
- **Implement a consumer:** [Consumer rules](#consumer-rules) for
  verification, remembered trust, and installation;
  [JSON schemas](#json-schemas); the
  [Reference implementation](#reference-implementation); and the
  [conformance vectors](https://github.com/jdx/packslip/tree/main/tests/conformance).
- **Propose a format change:** [Stability](#stability), which defines what
  version 1 holds fixed and what a revision may add.

### Terms

This specification uses these terms for its documents, the parties that
use them, and the kinds of project:

| Term | Meaning |
| --- | --- |
| *packslip*, or *release bundle* | The Sigstore bundle published with a release. |
| *Statement* | The in-toto statement inside a bundle. |
| *Release list* | A bundle whose statement has the `releases/v1` predicate. |
| *Vendor* | The project whose releases a packslip describes. It normally builds and signs its own releases. |
| *Consumer* | Anything that verifies or installs releases, such as a package manager, install script, or verification tool. |
| *Repackager* | A third party that signs a packslip for a vendor's files; see [Repackager attestation](#repackager-attestation). |
| *Forge project* | A project named after a repository on `github.com` or `gitlab.com`: `github.com/<owner>/<repo>`, optionally followed by a [monorepo](#monorepos) subpath, or `gitlab.com/<path>`. See [Project names](#project-names). |
| *Domain project* | A project whose host is neither `github.com` nor `gitlab.com`, such as `mise.jdx.dev`. This includes a project on a forge this version does not name. See [Project names](#project-names). |
| *Signer fingerprint* | A value that identifies the repository and issuer used to sign a keyless project's releases. See [Signer fingerprints](#signer-fingerprints). |

### Conventions

The words "must" and "must not" state requirements, "may" states a
permission, and "should" states a recommendation. These words are
lowercase. Imperative steps, such as the consumer rules, also state
requirements.

Notes, examples, and the
[Reference implementation](#reference-implementation) appendix are
informative. A note starts with "Note:". An example is text introduced by
"for example" or presented as an example, including any JSON or list it
introduces.

JSON examples abbreviate digests and commits for readability. Those
placeholders are not valid release data.

## Bundle format

A release ships one
[Sigstore bundle](https://github.com/sigstore/protobuf-specs)
(v0.3) per project. Each bundle contains:

- A [DSSE](https://github.com/secure-systems-lab/dsse) envelope with payload
  type `application/vnd.in-toto+json`. The envelope carries
  [the release statement](#the-release-statement).
- Verification material: the signer's
  [Fulcio](https://docs.sigstore.dev/certificate_authority/overview/)
  certificate or a public-key hint, plus the
  [Rekor](https://docs.sigstore.dev/logging/overview/) transparency log entry
  for the signature.

The log entry may be omitted only for a key-signed release made without
network access. [Unlogged bundles](#unlogged-bundles) describes that case.

The signature covers the DSSE payload bytes. Consumers must not canonicalize
the JSON before verification.

General-purpose Sigstore tools can read the bundle, but consumers must also
validate the packslip predicate and apply this specification's rules.

To inspect the payload without verifying it, see
[Inspecting a bundle](#inspecting-a-bundle).

## Signing

A release bundle is signed under one of two schemes. The statement names
its scheme in [`identity.scheme`](#signer-and-extension-fields). Both
schemes produce the same [bundle format](#bundle-format). A signer should
prefer keyless signing when a supported CI identity is available.

| Scheme | Signing credential | Consumer's trust anchor |
| --- | --- | --- |
| `sigstore-oidc` | A CI job's OIDC identity. | The expected identity and issuer, optionally pinned by [signer fingerprint](#signer-fingerprints). |
| `sigstore-key` | A long-lived Ed25519 key. | The public key. |

### Keyless signing

Under `sigstore-oidc`, a CI job that has the id-token permission signs
under its own identity. Fulcio issues a short-lived certificate that names
the workflow that ran. Rekor logs the signature. The vendor has no key to
manage.

[Creating and signing](#creating-and-signing) describes how the reference
CLI obtains the OIDC token.

### Key signing

Under `sigstore-key`, the vendor signs with a long-lived Ed25519 key. This
scheme suits vendors who release outside a CI system with OIDC, or who
want consumers to pin a stable key.

A consumer must pin the signer's public key in minisign's public-key
format. A consumer must never pin the public-key hint that the bundle
carries. In the reference CLI, `packslip keygen` writes the secret key as
a hex seed and the public key in this format, and consumers pin that
public-key file. The bundle also carries the Rekor entry, and that entry's
verifier is the public key.

The key ID is the 8-byte number that follows the `Ed` algorithm bytes in a
minisign public key. `identity.key_id` spells it as minisign prints it:
those bytes read as a little-endian integer, written as 16 uppercase
hexadecimal digits with leading zeros, such as `5A0A0B8B9C6D7E1F`.

The tool that generates a key chooses its ID, and minisign chooses it at
random. A consumer therefore cannot, in general, recompute the ID from the
Ed25519 key. That is why a consumer pins the minisign public key: it
carries both the ID and the Ed25519 key. A consumer must compare `key_id`
with the pinned key's ID without regard to case.

A key-signed bundle also supports a dependency-free signature check. Its
DSSE signature is a raw Ed25519 signature over the pre-authentication
encoding of the payload, with payload type `application/vnd.in-toto+json`.
This check does not replace verification of the Rekor entry that
[consumer rule 2](#rule-2-verify-the-bundle-and-downloaded-files)
requires, unless the consumer chose to accept an
[unlogged bundle](#unlogged-bundles).

### Unlogged bundles

A key-signed bundle may be produced without a log entry
(`packslip create --no-log`) for an air-gapped release. A consumer must
refuse such a bundle unless it explicitly allows unlogged bundles
(`packslip verify --allow-unlogged` in the reference CLI). A repository
should record that choice per vendor.

## Project names

A project name is a host with an optional path, such as
`github.com/jdx/mise`, `gitlab.com/group/tool`, or `mise.jdx.dev`.

### Name syntax

A project name has this syntax:

- It has no URL scheme and no trailing slash.
- The host is DNS labels separated by dots. It contains at least one dot
  and no port.
- Each label is one or more lowercase ASCII letters, digits, and hyphens
  (`a`–`z`, `0`–`9`, `-`). A label neither starts nor ends with a hyphen.
- Optional path segments follow the host. Each segment is preceded by `/`.
- A segment is one or more ASCII letters, digits, `.`, `_`, `~`, and `-`
  (`A`–`Z`, `a`–`z`, `0`–`9`, `.`, `_`, `~`, `-`). A segment is neither
  `.` nor `..`.

A consumer may offer a short-name alias, such as `mise` for
`github.com/jdx/mise`. The alias is a convenience outside the format.

### Forge and domain projects

A project name is the project's location. On a forge, the name also says
who may sign. A name in one of the first two forms below names a forge
project: `github.com/<owner>/<repo>`, optionally followed by a
[monorepo](#monorepos) subpath, or `gitlab.com/<path>`. A name on any host
other than `github.com` and `gitlab.com` names a domain project.

- **GitHub:** `github.com/<owner>/<repo>`. Releases and their packslips
  are GitHub release assets. The packslip is expected to be signed by a
  workflow run of that repository through GitHub's OIDC issuer
  (`https://token.actions.githubusercontent.com`). The signing workflow
  may be a reusable workflow of another repository, as
  [Repository identity and renames](#repository-identity-and-renames)
  allows.
- **GitLab:** `gitlab.com/<path>`. A pipeline of that project likewise
  signs its packslips, through GitLab's OIDC issuer (`https://gitlab.com`).
  GitLab subgroups make paths arbitrary depth, so the whole path names the
  repository.
- **Other hosts, including forges this version does not name:** the vendor
  controls the domain. It publishes a release list at the well-known URL
  that [The release list](#the-release-list) defines. The consumer pins
  the key or identity that signs the list and, separately, the one that
  signs the releases, as
  [consumer rule 1](#rule-1-pin-the-identity-once) says.

For a forge project, a consumer derives its initial signer policy from the
project the user intends to install. Deriving it from an untrusted
statement would accept whichever repository the statement's author names,
without checking the user's intended identity.

After verification, the consumer must also check that the statement
names the requested project, or the same repository under another name
as [Repository identity and renames](#repository-identity-and-renames)
allows. A forge name can change hands. The name locates the project, while
the forge's repository ID pins its identity.

### Letter case on forges

GitHub and GitLab resolve owner, group, and repository names without
regard to case. Neither lets two repositories' names differ only in case.

On `github.com` and `gitlab.com`, a consumer must therefore compare the
repository part of a name without regard to ASCII case, wherever it
compares one. The repository part is `<owner>/<repo>` on GitHub and the
whole path on GitLab. These comparisons include:

- the `project` of a statement or release list with the requested project;
- the `project` of a statement or release list with the certificate's
  Source Repository URI (see
  [Repository identity and renames](#repository-identity-and-renames));
- the `project` of a statement or release list with the signing
  workflow's repository;
- two signing workflows' repositories with each other;
- a certificate identity or identity prefix the user supplies with the
  certificate's identity;
- a source repository the user supplies with the certificate's Source
  Repository URI;
- a project named in the consumer's pins or lockfile with the requested
  project.

`github.com/jdx/mise` and `github.com/JDX/mise` name the same project. A
statement for one is therefore a statement for the other, not a rename.

A monorepo subpath, a workflow's path and ref, and a name on any other
host compare exactly.

### Monorepos

On `github.com`, a repository that releases several tools names each tool
with a subpath: `github.com/oxc-project/oxc/oxlint`,
`github.com/bazelbuild/buildtools/buildifier`, and
`github.com/biomejs/biome/crates/cli`. On `gitlab.com`, the whole path
names the repository, as
[Forge and domain projects](#forge-and-domain-projects) says. A GitLab
project therefore has no monorepo subpath.

Each tool gets its own packslip per release, with its own `version`. The
statement's `source.tag` carries the real tag, such as `oxlint_v1.0.0` or
`cli/v1.9.4`, so consumers do not have to infer the tag from the version.

The identity pin is still the repository. Any workflow run of
`oxc-project/oxc` may sign a packslip for `oxc-project/oxc/oxlint`.

When several tools share one GitHub release, each tool ships its own
bundle. The bundle is named `packslip.<subpath>.sigstore.json`, with each
`/` in the subpath replaced by `-`:

| Project within the repository | Bundle name |
| --- | --- |
| The repository itself | `packslip.sigstore.json` |
| `oxlint` | `packslip.oxlint.sigstore.json` |
| `crates/cli` | `packslip.crates-cli.sigstore.json` |

Consumers select a tool's bundle by its signed statement, not by trusting
the file name. They read the release's `packslip*.sigstore.json` assets.
They keep the one whose `predicateType` is `release/v1` and whose
`project` matches the requested name or names the same tool in the
renamed repository.

A tool keeps its subpath across a rename. `github.com/old/repo/tool`
becomes `github.com/new/repo/tool`, never another tool in the repository.

## Keyless signer identity

The subsections below define which repository a keyless
(`sigstore-oidc`) certificate identifies, how a consumer compares signing
workflows, and the signer fingerprint, which lets a consumer pin a
repository before its first install.

### Repository identity and renames

A forge repository can be renamed or transferred to another owner, and
the forge then redirects the old name to the new one. For example,
`github.com/jdx/rtx` became `github.com/jdx/mise`, and GitHub answers for
the first with the second. Once the old name is free, anyone can create
another repository under it, including a workflow at the same path. The
name alone cannot distinguish the original repository from its
replacement. The forge's repository ID can: it survives renames and
transfers, never changes, and is never reused.

Fulcio records the IDs in every certificate it issues to a GitHub Actions
or GitLab CI job. It takes them from the job's OIDC token:

| Extension | OID | GitHub claim | GitLab claim |
| --- | --- | --- | --- |
| Source Repository URI | `1.3.6.1.4.1.57264.1.12` | server URL + `repository` | server URL + `project_path` |
| Source Repository Identifier | `1.3.6.1.4.1.57264.1.15` | `repository_id` | `project_id` |
| Source Repository Owner URI | `1.3.6.1.4.1.57264.1.16` | server URL + `repository_owner` | server URL + `namespace_path` |
| Source Repository Owner Identifier | `1.3.6.1.4.1.57264.1.17` | `repository_owner_id` | `namespace_id` |

Each extension value is a DER UTF8String. An empty value means the forge
supplied none.

For these IDs to authenticate a packslip, the Source Repository URI must
identify the repository named by `project`. The repository's name is
compared without regard to case, as
[Letter case on forges](#letter-case-on-forges) defines. The Source
Repository URI names the repository whose run requested the certificate,
whichever workflow signed. The signer may therefore be a reusable
workflow of another repository. Only a certificate that records no Source
Repository URI falls back to requiring the signer to be a workflow of the
named repository.

The two owner extensions record who owned the repository when the
certificate was issued. They are not part of the project's identity:
consumers must neither remember nor compare them.

**Note:** Only the current owner can transfer a repository, and that
owner already controls what it signs.

A consumer must remember the repository ID of the first release it
accepts, with its signer (see [Consumer rules](#consumer-rules)), and
compare each later release with it:

| Case | Statement and certificate | Consumer behavior |
| --- | --- | --- |
| Same project | The statement names the requested project, and the certificate carries the pinned repository ID. | Accept it. |
| Renamed or transferred | The statement names another repository on the same forge, with the same monorepo subpath, and the certificate carries the pinned repository ID. | Accept it and report the name it was signed under. |
| Different repository | The certificate carries a repository ID other than the pinned one. | Refuse it, even if the statement names the requested project. |
| No ID to compare | The certificate or remembered pin lacks a repository ID, as with certificates issued before Fulcio recorded them. | Check by name alone. Refuse a statement for another name when either side has no ID to compare, as with no pin. |

A rename or transfer can be checked in either direction. The user may ask
for the old name while verifying a release signed after the move. The
user may also ask for the new name while verifying a release signed
before it. A replacement repository under the old name has a different ID
and is refused.

Signer continuity compares the workflow's path inside the repository. For
example, `.github/workflows/release.yml` in `jdx/rtx` continues as the
same workflow in `jdx/mise`. The same path in `jdx/hk` continues in
`acme/hk` after a transfer.

A reusable workflow of another repository is compared as a workflow of
that repository, by that repository's name. It continues only the same
path in the same repository. It does not continue the project's own
workflow at that path, and the project's own workflow does not continue
it.

A pin taken from an accepted release records the release's signed project
name and repository ID. An earlier revision of this section also required
a pin to store an owner ID. A consumer that reads a pin storing an owner
ID must ignore that owner ID.

A consumer may hold several pins for a project, such as its own record
and a lockfile's commitment. Each release must match every pin. A refusal
must say which pin the release disagreed with.

With no pin, the consumer has only the forge's word, and first use is
trust on first use. GitHub's `GET /repos/<owner>/<repo>` redirects a
renamed or transferred repository's old name to the repository and
returns its `id`. A consumer may take that `id` as the expected
repository ID. If the name was recreated before the consumer first saw
it, the forge answers for the new repository. The release then cannot
reveal that the name previously belonged to another repository.

Two mechanisms limit that first-use exposure:

- A lockfile that records the repository ID carries that pin to every
  machine that reads it.
- A [signer fingerprint](#signer-fingerprints) that the project publishes
  lets a consumer pin the repository before its first install.

### Workflow pinning

By default, a consumer of a keylessly signed project must compare signing
workflows:

- a release's with the workflow of the last release it accepted;
- a release list's with the workflow of the last list it accepted.

A different workflow in the same repository is a changed signer. The
consumer must refuse the release or list until a person approves it
([rule 3](#rule-3-enforce-no-downgrade)).

This default requires approval each time a vendor switches between
release workflows, including when several jobs share a reusable workflow.
A vendor may instead declare, in its signed `identity`, that consumers
pin the repository while allowing its signing workflow to change:

```json
"identity": {
  "scheme": "sigstore-oidc",
  "key_id": "https://github.com/example/tool/.github/workflows/release.yml@refs/tags/v1.2.0",
  "issuer": "https://token.actions.githubusercontent.com",
  "pin_workflow": false
}
```

`pin_workflow` defaults to `true`, so omitting it pins the signing
workflow. The vendor declares the field in a signed release or release
list. The field has no meaning for `sigstore-key`, which has no workflow.
In the reference CLI, `packslip create --no-pin-workflow` and
`packslip releases --no-pin-workflow` write `false`. Both refuse the flag
when `--key` is also given.

With `pin_workflow: false`, a consumer must still require everything else
of the signer: the issuer, and a certificate whose repository is the
pinned one. Where the forge records a repository ID, the certificate's
repository is compared with the pinned one by that ID, as
[Repository identity and renames](#repository-identity-and-renames)
describes. The consumer stops comparing the following with the last
accepted signer:

- the signing workflow's path;
- where repository IDs establish the repository, the repository that the
  workflow file belongs to.

The consumer must remember `pin_workflow` beside the signer. Disabling a
previously pinned workflow reduces remembered trust and requires
approval, as [rule 3](#rule-3-enforce-no-downgrade) defines:

| Remembered value | Incoming value | Consumer behavior |
| --- | --- | --- |
| No remembered signer | `true` or omitted | Pin the signing workflow; there is no remembered trust to reduce. |
| No remembered signer | `false` | Pin the repository without pinning the workflow; there is no remembered trust to reduce. |
| `true` | `true` or omitted | Compare the workflow with the last accepted signer; a change requires approval. |
| `true` | `false` | Refuse until a person approves the reduction. Then remember `false`, without asking again for that reduction. |
| `false` | `false` | Continue without comparing workflow paths. |
| `false` | `true` or omitted | Resume comparison with the last accepted signer's workflow. This is not a reduction, but a different workflow is a changed signer and requires approval. |

**Note:** A consumer written before this field existed ignores it, as it
ignores any field it does not know, and keeps holding the vendor to its
workflow. Such a consumer therefore still refuses a release from a
changed workflow until a person approves it, even when the vendor sets
the field.

### Signer fingerprints

A signer fingerprint identifies the repository and issuer used to sign a
keyless project's releases. It lets a consumer pin that repository before
accepting its first release, rather than trusting the first release it
finds (see [rule 1](#rule-1-pin-the-identity-once)).

A project can publish its fingerprint independently of its releases, for
example in its README or install instructions. Users can then record it
in a Dockerfile or a consumer's lockfile. A fingerprint looks like this:

```text
ps1_snirenkjwr7m5ozgcufameodnm
```

#### Computing a fingerprint

A fingerprint is `ps1_` followed by the first 128 bits of this hash:

```text
SHA-256( "packslip-signer-v1" 0x00 issuer 0x00 repository_id )
```

Those 128 bits are encoded as 26 characters of RFC 4648 base32, in
lowercase and without padding (`a`–`z`, `2`–`7`). The hash input consists
of:

- `"packslip-signer-v1"`: the ASCII bytes of that string.
- `0x00`: a single zero byte.
- `issuer`: the certificate's OIDC issuer URL, such as
  `https://token.actions.githubusercontent.com` or `https://gitlab.com`.
- `repository_id`: the Source Repository Identifier (OID
  `1.3.6.1.4.1.57264.1.15`, see
  [Repository identity and renames](#repository-identity-and-renames)),
  as the string of decimal digits it is.

`issuer` and `repository_id` come from the certificate exactly as
recorded, with no case or slash normalization.

For GitHub repository `jdx/hk`, whose ID is `922514152`, the input is
`packslip-signer-v1`, a zero byte,
`https://token.actions.githubusercontent.com`,
a zero byte, and `922514152`, and the fingerprint is
`ps1_snirenkjwr7m5ozgcufameodnm`.

Twenty-six base32 characters hold 130 bits, so the last character carries
two padding bits, which are zero. Text whose last character sets either
bit is not a fingerprint. Each fingerprint therefore has exactly one
spelling.

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

When a repository is deleted and someone else creates one under its name,
the new repository has a new ID and so a different fingerprint. That is
what makes the fingerprint a pin.

A release has no fingerprint in any of these cases:

- It is signed with a key.
- Its certificate records no repository ID, because the certificate
  predates the extension or the issuer does not give one.
- The recorded ID is not a string of decimal digits.

A project whose releases have no fingerprint is pinned with an identity
and issuer, or with a public key, as [Consumer rules](#consumer-rules)
says.

A fingerprint is a public commitment, not a secret. Its 128 bits make
finding another repository with the same fingerprint infeasible, while
its text form fits in a README. It carries no authority by itself: the
consumer trusts the project that publishes it and the place where it is
published. A fingerprint does not belong in a packslip, which cannot
vouch for its own signer.

#### Checking a fingerprint

A consumer that holds a fingerprint must check it against a release after
verifying the bundle ([rule 2](#rule-2-verify-the-bundle-and-downloaded-files)).
The check uses only what the verified certificate records, never what the
statement says of itself:

1. Read the issuer and the Source Repository Identifier from the
   certificate. A certificate that records no repository ID matches no
   fingerprint: refuse the release.
2. Compute the fingerprint of the issuer and repository ID, and compare it
   with the one held. If they differ, the release is from another
   repository or through another issuer: refuse it.
3. Apply [Repository identity and renames](#repository-identity-and-renames)
   as for any release. The certificate's Source Repository URI must be
   the repository that the statement names. If the certificate records no
   Source Repository URI, the signer must be a workflow of that
   repository.

The fingerprint says which repository it is. Step 3 says that the
statement is about that repository.

A consumer must refuse a malformed fingerprint pin. It must not ignore
the pin or treat it as "no pin". A valid fingerprint pin:

- starts with `ps1_`;
- has exactly 26 lowercase base32 characters after the prefix;
- has zero padding bits in its last character.

A consumer must refuse an unknown prefix, such as `ps2_`. It must not
guess what such a value commits to.

## What a verified packslip proves

A verified packslip establishes the following:

- It authenticates the named signer's statement about the release.
- A downloaded file whose digest matches the statement is the file the
  signer described.
- A logged signature also has a verified integration time. An explicitly
  accepted unlogged signature has no such log evidence; see
  [Unlogged bundles](#unlogged-bundles).

What authenticates a resource depends on its source.
[Source types](#source-types) gives the verification for each source.

Two kinds of assurance require separate checks:

- **Build provenance and safety.** A packslip does not establish how an
  artifact was built or whether it is safe. Linked SLSA provenance must be
  fetched and verified separately against the builder's identity and the
  consumer's policy. A provenance URL alone establishes no build level.
- **Freshness.** A single release bundle does not establish freshness,
  detect withdrawal, or prevent rollback. Those checks require discovery
  metadata and remembered consumer state, as [Discovery](#discovery) and
  [Consumer rules](#consumer-rules) define.

[Reference CLI behavior](#reference-cli-behavior) describes what the
reference CLI's `packslip verify` checks and reports. As
[Project names](#project-names) explains, its default signer policy does
not check the user's intended identity. When using `packslip verify`,
supply the expected public key with `--pubkey`, or the expected identity
or identity prefix together with `--issuer`. Alternatively, supply
`--source-repository` with `--issuer`, as
[Verifying a bundle](#verifying-a-bundle) describes. With any signer
policy, compare the project and version that `packslip verify` reports
with the request. A signer can sign other projects and older versions, and
`packslip verify` does not know which one the user asked for.

## The release statement

A release statement is an in-toto statement whose `predicateType` is
`https://packslip.dev/release/v1`. Its `subject` binds file names to
digests. Its `predicate` describes the project, version, artifacts,
resources, and signer.

The following example describes a mise release with one Linux x86_64
archive and a separate SBOM asset. It abbreviates digest and commit values
as `...`; an actual statement must supply their full values.

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

`subject` lists, by file name and digest, every artifact and every
separate file used as a resource's `asset`. `artifacts` describes the
installable files, using the same names. The two lists have these
rules:

- Every artifact is a subject.
- Every subject is either an artifact or a resource's `asset`.
- Neither list contains duplicate names.
- At least one artifact is required.

Each subject must have a `sha256` digest of 64 lowercase hexadecimal
characters. A `sha512` digest is optional. When present, it must have 128
lowercase hexadecimal characters.

Each artifact must have a `size`, its length in bytes. The size is checked
alongside the artifact's digest. An artifact's optional `url` gives its
download location.

### Project, version, and source

`project` follows the [project-name rules](#project-names).

`version` is a [semver 2.0.0](https://semver.org/spec/v2.0.0.html)
version. Its prerelease part, if present, marks a prerelease and names its
channel. See [Versions](#versions).

`published_at` is a timestamp in
[RFC 3339](https://www.rfc-editor.org/rfc/rfc3339) format.

A publisher must write the following timestamps in UTC with the `Z`
suffix, such as `2026-09-01T12:00:00Z`:

- `published_at` in a release statement.
- `generated_at`, `expires_at`, and `releases[].published_at` in a
  [release list](#the-release-list).

A consumer must accept any RFC 3339 offset in these fields and must
compare them as instants. For example, `2026-09-01T14:00:00+02:00` and
`2026-09-01T12:00:00Z` are the same time.

The optional `source` object identifies where the release was built from.
When `source` is present, its fields are:

- `repo`: the repository URL. It is required.
- `commit`: optional.
- `tag`: optional. It holds the tag as the vendor spells it.

A resource with a `repo` source requires `source.commit`, so that the
resource's content is pinned.

[Matching a request](#matching-a-request) defines how a consumer matches a
requested tag against `source.tag` or the release-list entry's `tag`.

### Platform, format, and variant

Each value of `os`, `arch`, `libc`, `format`, and `variant` must be a
token. A token consists of lowercase letters, digits, `_`, `-`, and `.`,
and starts with a letter or digit.

[Vocabularies](#vocabularies) lists the documented values. A value outside
those vocabularies is well-formed, but it does not match a host or unpack
with a consumer that does not know it. The vocabularies are open: a vendor
may use such a value, but only for a platform or format this specification
has not named yet. Consumers skip formats they cannot open.

An absent `os`, `arch`, or `libc` means the artifact does not depend on
that host property. For example:

- A universal macOS binary has `os: "darwin"` and no `arch`.
- A statically linked Linux executable that loads no host C library has
  no `libc`.
- A script or jar has none of the three.

Every artifact must have a `format`: an archive or installer type, or
`raw` for a bare executable. Artifacts that differ only in format carry
the same build, and a consumer chooses its preferred format.

A vendor must not publish two artifacts with the same `os`, `arch`,
`libc`, `variant`, and `format`. A consumer that receives such a pair must
refuse to choose between them. The `packslip create` command rejects such
a pair.

`variant` distinguishes builds that share `os`, `arch`, and `libc`, such
as `fips`, `baseline`, `debug`, `installer`, or `source`. Unless asked for
a variant, a consumer must select only artifacts without one.

### Executable paths and command names

An artifact's optional `bin` list names its executables. Each entry is a
string or an object.

A string entry is a path from the true archive root, with no top-level
directory stripped. For a bare executable, the path is the artifact's own
name without its compression suffix.

Use an object entry when the command name on PATH differs from the file
name: `{ "path": "bin/oxlint-x86_64", "name": "oxlint" }`. Two entries may
share a path under different names, allowing aliases such as `pnpx` for
`pnpm`; a consumer may create links or copies.

A command name is the name as typed, without `.exe`. It must not be
empty, so a string entry's path ends in a file name rather than `/`. A
Windows path keeps its extension, such as `bin/tool.exe`, and the consumer
places it on PATH as `name.exe`.

Names in `requires.bin` and in a resource's `bin` follow the same
convention. Consumers therefore compare these names without adding or
stripping an extension.

For compatibility with older statements, every consumer must nonetheless
read each of the following values that ends in `.exe`, in any letter case,
as the name without it:

- an artifact `bin` entry's `name`;
- a resource's `bin`;
- the first element of a resource's `exec`.

For example, `{ "path": "tool.exe", "name": "tool.exe" }` names the
command `tool`, and so does an `exec` whose first element is `tool.EXE`.

### Requirements, resources, and provenance

An artifact's optional `requires` object describes what the host must
provide:

- `os_min`: the minimum OS version.
- `glibc_min`: the minimum glibc version, for a `gnu` Linux build.
- `libs`: shared libraries.
- `bin`: external commands.

OS versions use the OS's own terms, such as `12` for macOS Monterey and
`10.0.17763` for Windows. See [Host requirements](#host-requirements).

The optional release-level `resources` list describes what ships beside
the executables. Each entry must have a `kind` and exactly one source. See
[Resources](#resources).

An artifact's optional `provenance` list contains URLs of
[SLSA build provenance](https://slsa.dev/spec/v1.0/provenance) statements.
The release bundle authenticates the release statement. Provenance that is
verified separately establishes build claims at the
[SLSA build level](https://slsa.dev/spec/v1.0/levels) its builder
supports.

### Signer and additional metadata

`identity` describes how the statement is signed and by whom. It lets a
consumer compare its pin with the received signature. It must name the
actual signer, as
[consumer rule 2](#rule-2-verify-the-bundle-and-downloaded-files) requires. The two
schemes use its fields as follows:

- For `sigstore-oidc`, `key_id` is the certificate's subject identity: a
  workflow URI for CI, or an email address for a person. `issuer` is the
  OIDC issuer. Both fields are required for `sigstore-oidc`, and each
  must be non-empty.
- For `sigstore-key`, `key_id` is the signing key's minisign key ID in
  uppercase hexadecimal. See [Key signing](#key-signing).

`attested_by` is `vendor` by default, or `repackager`. A repackager
describes what it checked in `evidence`. See
[Repackager attestation](#repackager-attestation).

The optional `notes_url` points to release notes. `extensions` carries
additional metadata under keys that name the party defining it. See
[Extensions](#extensions).

### Vocabularies

The documented platform tokens follow components of Rust target triples:

| Field | Documented values |
|---|---|
| `os` | `linux`, `darwin`, `windows`, `freebsd`, `netbsd`, `openbsd`, `illumos`, `android`, `ios` |
| `arch` | `x86_64`, `aarch64`, `armv7`, `armv6`, `riscv64`, `i686`, `powerpc64le`, `s390x`, `loongarch64` |
| `libc` | `gnu`, `musl` for Linux builds |
| `variant` | `fips`, `baseline`, `debug`, `installer`, `source`, `softfloat`, `5-hardfloat`, and a CPU level such as `v3` |

A `softfloat` variant marks a 386, ARM, or MIPS build that uses software
floating point, so it stays distinct from the hard-float build of the same
`arch` and `libc`. An ARMv5 build is soft-float by default, so it has no
variant; an ARMv5 build that uses hardware floating point is `5-hardfloat`.
A CPU-level variant is the toolchain's own name for a level
above the platform default, such as `v3` for GOAMD64 or `v8.2` for
GOARM64. A build at the default level has no variant, so a consumer that
does not ask for one gets the build that runs everywhere. A comma in a level
becomes `-`, as in `v8.2-lse`.

**Note:** A vendor can therefore map its build matrix directly onto these
tokens.

The documented `format` values describe how the file is installed:

| File type | Formats | Consumer behavior |
|---|---|---|
| Archive | `tar.xz`, `tar.gz`, `tar.zst`, `tar.bz2`, `tgz`, `tar`, `zip`, `7z` | Unpack the archive. |
| Single compressed executable | `gz`, `xz`, `zst`, `bz2` | Decompress to the file named by `bin`. |
| Installer | `deb`, `rpm`, `dmg`, `pkg`, `msi`, `msix`, `exe`, `appimage` | Hand to the platform's installer or a launcher. |
| Bare executable | `raw` | Install the executable directly. |

A package manager installing into its own directory does not unpack
installer formats. A `.exe` that is the program itself uses `raw`. The
`exe` format means a Windows installer.

Paths in `bin` and `archive` sources use `/` between directories, as ZIP
entry names require. Windows PowerShell 5.1's `Compress-Archive` instead
writes `\` in ZIP entry names. Consumers interpret those backslashes as
slashes: `tool-1.0\bin\tool.exe` refers to `tool-1.0/bin/tool.exe`.

`format` is required even when the file name has no format suffix.
Consumers select only formats they handle; an absent format cannot be
selected.

Files that are not installable artifacts, such as a shared library,
header, or checksum sidecar, stay outside `artifacts`.

These vocabularies are open.
[Platform, format, and variant](#platform-format-and-variant) states when
a vendor may use a token the vocabularies do not list.

### Field reference

Required fields inside an optional object are required when that object
is present. For example, `source` is optional, but a supplied `source`
must include `repo`.

A resource must choose exactly one of `archive`, `asset`, `repo`, and
`exec`. The tables mark these four fields "one source".

Paths beginning with `artifacts[]` or `resources[]` are relative to
`predicate`. Paths beginning with `requires` are relative to an artifact.

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
| `predicate.published_at` | string | required | RFC 3339 publish time, written in UTC. See [Project, version, and source](#project-version-and-source). |
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
| `artifacts[].variant` | string | optional | Distinguishes builds sharing os, arch, and libc: `fips`, `baseline`, `debug`, `installer`, `source`, `softfloat`, `5-hardfloat`, or a CPU level such as `v3` ([Vocabularies](#vocabularies)). |
| `artifacts[].size` | integer | required | File size in bytes. Verified alongside the digest. |
| `artifacts[].url` | string | optional | Download URL. |
| `artifacts[].format` | string | required | Archive, compression, or installer type, or `raw` for a bare executable. |
| `artifacts[].bin[]` | array of string or object | optional | Executables inside the artifact: a path from the archive root, or `{ path, name }` when the PATH name differs. A name is the command as typed, without `.exe`, and is not empty. |
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
| `predicate.identity` | object | required | How the statement is signed and by whom. See [Signing](#signing). |
| `predicate.identity.scheme` | string | required | `sigstore-oidc` or `sigstore-key`. A closed vocabulary: any other value makes the statement invalid. |
| `predicate.identity.key_id` | string | required | The certificate identity, or the minisign key ID in uppercase hex. See [Key signing](#key-signing). |
| `predicate.identity.issuer` | string | conditional | The OIDC issuer, for `sigstore-oidc`, where it is required. |
| `predicate.identity.pin_workflow` | boolean | optional | For `sigstore-oidc`: `false` asks consumers to hold later releases to the signing repository, not to the workflow that signed this one. `true` when absent. See [Workflow pinning](#workflow-pinning). |
| `predicate.attested_by` | string | optional | `vendor` (default) or `repackager`. A closed vocabulary: any other value makes the statement invalid. See [Repackager attestation](#repackager-attestation). |
| `predicate.evidence[]` | array of object | optional | What a repackager checked: `{ kind, detail? }`. |
| `extensions` | object | optional | Vendor- or consumer-defined data, keyed by who defines it, on the release, each artifact, each resource, the release list, and each list entry. See [Extensions](#extensions). |

## Resources

The optional `resources` list describes content associated with the
release: completions, man pages, CLI specifications, agent skills, SBOMs,
and desktop integration files. Each entry must have a `kind` and exactly
one source. Several entries can offer alternative sources for the same
resource. Scope, specificity, and source priority determine which one a
consumer uses.

The entries also describe how consumers can install the software. A
command-line package manager installs the executables listed in
[`bin`](#executable-paths-and-command-names). A desktop launcher uses
`desktop` or `app` resources. A release can provide both without
assigning the application a single category.

### Source types

A resource comes from one of four sources. After
[resource selection](#selecting-resources) narrows the entries, consumers
prefer these sources in the order shown. The source also determines what
authenticates the content, as the Verification column shows.

| Source | Content | Verification |
|---|---|---|
| `archive` | A path inside the selected artifact, from its [true archive root](#executable-paths-and-command-names). Available only when the artifact contains paths. | Covered by the artifact's signed digest. |
| `asset` | A separate release file, named in `subject`. The entry's `url` gives its download location. A skill directory can ship as its own archive this way. | Checked against the asset's signed digest in `subject`, just like an artifact. |
| `repo` | A path in the source repository at `source.commit`. | Pinned by `source.commit`, which is required when any resource uses `repo`. |
| `exec` | The stdout of an argv whose first element is a `bin` name. `env` supplies command-specific environment variables. | The consumer verifies the executable and controls when it runs. The generated output is not separately signed or verified. |

An `exec` source is common for completions: cobra, clap, oclif, and usage
generate them from the binary without shipping a static file. A vendor
that also provides a static file lists that source first.
[Running an exec entry](#running-an-exec-entry) defines when a consumer
may run the command.

### Resource scope

An entry's optional `os`, `arch`, and `libc` fields limit it to artifacts
with matching values. Each supplied field must equal the selected
artifact's field. An entry that omits all three has no platform
restriction.

The optional `artifact` field names an exact file in `artifacts`. Use it
when artifacts on the same platform have different layouts, or when a
resource belongs to one variant. The named artifact must exist. It must
also match any platform scope on the entry.

For example, if a `fips` build places its man page at a different path
from `tool-linux-x64.tar.xz`, name each artifact in its corresponding
resource entry. In
[release.toml](https://packslip.dev/docs/describing-releases/#use-a-toml-manifest),
which `packslip create` reads, this is spelled
`artifact = "tool-linux-x64.tar.xz"` inside `[[resource]]`.

An `archive` entry never applies to an artifact whose `format` is bare:
`raw`, `gz`, `xz`, `zst`, or `bz2`. Such an artifact is the executable
itself, so it holds no path for the entry to name. An `asset` or `repo`
entry can still supply the same man page for a bare artifact.

For example, a vendor that publishes `tool-linux-x64.tar.xz` beside a
bare `tool-linux-x64` needs no scope to keep an archive entry off the
bare one. The `archive` source itself limits where the entry applies.

### Resource identity

Entries compete only when they describe the same resource. A resource's
identity consists of its `kind` and the fields this table lists for that
kind:

| Kind | Identity within that kind |
|---|---|
| `completion` | `bin` and `shell`. An `exec` entry offering several shells has one identity per shell. |
| `cli-spec` | `bin` and `format`. |
| `skill` | `name`. |
| `sbom` | `format`. |
| `icon` | `bin`, if any, the source's hicolor size directory, if any, and the source's file name. An `exec` source has no file name. |
| Every other kind | `bin`, if any, and the source's file name. An `exec` source has no file name. |

When the last four `/`-separated segments of an icon's source path are
`hicolor`, SIZE, CONTEXT, and FILE, its hicolor size directory is SIZE.
Any other icon has no hicolor size directory. For example, SIZE is
`48x48` for `share/icons/hicolor/48x48/apps/tool.png`, and it can also be
a name such as `48x48@2` or `scalable`. An icon at
`share/icons/myhicolor/48x48/apps/tool.png` has none.

Where an identity includes `bin`, an entry that omits `bin` in a release
with one executable is for that executable.

Entries with different identities never hide one another. For example, a
Linux-scoped skill does not replace an unscoped skill with a different
name. A platform-specific zsh completion does not affect a bash
completion.

### Selecting resources

For each resource identity, a consumer must apply these steps in order:

1. Keep the entries that apply to the selected artifact, as
   [Resource scope](#resource-scope) defines.
2. Of those, keep the most specific. An entry that names `artifact`
   outranks every entry scoped only by platform. Among entries equally
   scoped by `artifact`, the entries naming the most of `os`, `arch`, and
   `libc` win.
3. Try the remaining entries by source priority, as
   [Fallback and verification](#fallback-and-verification) defines.
4. Try entries with the same source type in their order in `resources`.
   This final tie-breaker is an ordered list of alternatives. The first
   usable entry wins. Once the consumer has the resource, it stops
   fetching or running lower-priority entries.

A verification failure refuses the release. It is never a reason to try
the next entry.

For example, two equally scoped `repo` entries for one skill try the
first directory, then the second only if the first is unavailable.

### Resource kinds

#### Shell completions and man pages

`completion` supplies a shell completion script:

- A static source names its shell in `shell`: `bash`, `zsh`, `fish`,
  `powershell`, `nushell`, or `elvish`.
- An `exec` source lists every shell the command generates in `shells`.
  The consumer substitutes each shell for `{shell}` in argv elements and
  `env` values.

Common generators use these patterns:

| Generator | `exec` | `env` |
|---|---|---|
| cobra or clap | `["tool", "completion", "{shell}"]` | None needed. |
| clap dynamic completions | `["tool"]` | `{ "COMPLETE": "{shell}" }` |
| click | `["tool"]` | `{ "_TOOL_COMPLETE": "{shell}_source" }` |

`bin` names the executable the completion is for. When a release has one
executable, `bin` may be omitted. The entry then identifies that
executable and competes with an entry that names it explicitly. When a
release has several executables, `bin` is required.

`man` supplies a man page whose section is its file suffix, as in
`mise.1`. Its `bin` identifies the documented executable under the same
rules as a completion's `bin`.

#### CLI specifications

`cli-spec` is a machine-readable description of an executable. Its
required `bin` names the executable, and its required `format` names the
description's format. The documented format is `usage`, a
[usage](https://usage.jdx.dev) spec.

The consumer's own copy of `usage` can generate completions for every
shell it supports, a man page, and Markdown documentation from that spec.
It does so without running vendor code at install time.

Generated completions call `usage complete-word` when the shell requests
completion. A consumer that generates them therefore installs `usage`
beside the tool. Generated man pages and documentation have no runtime
dependency on `usage`. A vendor can also ship static completions for
consumers that want to avoid that dependency.

#### Agent skills

`skill` describes an agent skill in the
[Agent Skills](https://agentskills.io) format, identified by `name`. A
skill is a directory containing `SKILL.md` and the files it references.
The source determines the layout:

- `archive` or `repo`: the path names the skill directory.
- `asset`: the file is an archive of the directory's contents.
  `SKILL.md` is either at the archive root or inside one top-level
  directory that the consumer strips. Nothing else is at the root.
- `exec`: the command prints a single `SKILL.md`.

A consumer counts a skill as present only after `SKILL.md` is in place.
A partially fetched directory does not satisfy the resource.

#### Software bills of materials

`sbom` supplies a software bill of materials. Its `format` is `cyclonedx`
or `spdx`. Its source must be `archive`, `asset`, or `repo`, so a digest
or source commit covers the content. An `exec` source is never allowed.
A release with one SBOM per platform lists an entry per `os` or per
artifact archive.

#### Desktop integration

- `desktop` supplies a freedesktop
  [desktop entry](https://specifications.freedesktop.org/desktop-entry-spec/latest/)
  for a Linux launcher. AppImage, deb, rpm, and the Windows installer
  formats carry their own desktop integration.
- `icon` supplies an icon file. A hicolor path such as
  `share/icons/hicolor/512x512/...` identifies its size. The size is part
  of the icon's [identity](#resource-identity), so a release can list one
  icon at several sizes and a consumer installs each of them.
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
priority. Entries with the same source type are tried in their order in
`resources`, as step 4 of [Selecting resources](#selecting-resources)
defines.

A consumer must ignore kinds and sources it does not know. A vendor may
therefore publish a `font` or its own kind before this specification
names it.

Resources are optional to installation. If an asset or repository file is
unavailable, or a network request fails, the consumer may try the next
eligible source. If no source is usable, the consumer reports the missing
resource and completes installation of the executables. A later attempt
may fetch the resource.

A verification failure has a different consequence. The consumer must
refuse the release when a resource's digest differs from `subject`. It
must also refuse the release when a resource's repository content
differs from `source.commit`. In either case it must not try another
source, just as it would not accept an artifact with the wrong digest.

### Running an exec entry

An `exec` entry runs a release executable. Consumers run a completion
entry on demand, when the shell first requests completion. Running it on
demand needs no permission beyond installation. A consumer caches successful
output per release, executable, and shell, so repeated requests do not
rerun the command.

Other `exec` resources, such as a generated skill written to disk, run at
install time only if the user has allowed vendor code to run then.
Without that permission, the consumer treats the entry as absent instead
of failing the installation. Consumers may also generate completions at
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

An artifact's `requires` object describes what the host must provide
before the software can run. It has four optional fields: `os_min`,
`glibc_min`, `libs`, and `bin`. They use OS versions, loader names, and
command names, so consumers can check requirements without a shared
package registry. [Requirement fields](#requirement-fields) lists each
field's type.

For example, a `gnu` Linux artifact might declare:

```json
"requires": {
  "os_min": "4.18",
  "glibc_min": "2.31",
  "libs": ["libssl.so.3"],
  "bin": [{ "name": "git", "min": "2.30" }]
}
```

This artifact needs Linux kernel 4.18 or later, glibc 2.31 or later,
`libssl.so.3` from the host, and `git` 2.30 or later.

### Minimum OS and glibc versions

`os_min` is the lowest supported OS version, in the OS's own terms.
`glibc_min` is the lowest supported glibc version for a `gnu` Linux build.

What `os_min` names depends on the artifact's `os`:

| `os` | `os_min` names |
|---|---|
| `linux` | The Linux kernel version, excluding local build suffixes such as `-18-amd64` or `+`. |
| `darwin` | The macOS product version, such as `12` for macOS Monterey. |
| `windows` | The Windows major, minor, and build numbers, such as `10.0.17763`. |

For Linux, `os_min` does not name a distribution's release. The `os` field
identifies Linux and has no distribution identifier.

### Shared libraries

`libs` lists the shared libraries that the executables load from the host.
Each entry is the name the loader resolves:

| Platform | Name | Examples |
|---|---|---|
| Linux and FreeBSD | soname | `libssl.so.3`, `libstdc++.so.6` |
| Windows | DLL name | `vcruntime140.dll` |
| macOS | dylib file name | `libssl.3.dylib` |

`libs` excludes the C runtime and loader, which `libc` and `glibc_min`
already describe:

- libc, libm, libdl, libpthread, librt, libgcc_s, and the dynamic loader
- `/usr/lib` and `/System/Library` on macOS
- DLLs shipped by Windows

`libs` also excludes libraries bundled in the artifact and found through
its own rpath.

`packslip create` reads `libs` from the executables named in the
artifact's `bin`. A consumer that has the artifact can inspect the same
bytes. The release bundle lets the consumer check before downloading the
artifact.

Whether the field is present records whether that inspection happened:

- `"libs": []` means the executables were inspected and need no host
  libraries beyond the excluded ones.
- An absent `libs` means libraries were not checked, as with an installer
  that `create` does not open, or an executable that is a script.

### Required commands

`requires.bin` lists commands that the release's executables invoke and
cannot work without. Each entry has these fields:

- `name`: the bare command name, as the executable invokes it, such as
  `java`, `python3`, or `git`. It has no directory and no `.exe` suffix.
  [Executable paths and command names](#executable-paths-and-command-names)
  describes the `.exe` convention.
- `min` (optional): the lowest supported version.

The vendor declares these requirements. Binary inspection does not
establish that a program invokes `java`.

Only required commands belong in `requires.bin`. Optional integrations go
under [`extensions`](#extensions). A required command must not be one the
release itself provides.

### Comparing versions

A `min` is an ASCII digit followed by zero or more ASCII letters, digits,
`.`, `-`, and `+`, such as `17` or `3.12`. A statement with any other
`min` is invalid. A consumer must refuse such a statement when it checks
the statement's structure under
[consumer rule 2](#rule-2-verify-the-bundle-and-downloaded-files).

A vendor must write `glibc_min` and `os_min` in the same grammar, and
`packslip create` refuses other spellings. A consumer, however, must not
refuse a statement because its `glibc_min` or `os_min` breaks the grammar.

`min`, `glibc_min`, and `os_min` are numeric lower bounds. A `min` of `17`
means 17.0.0 and later, including 21.

A consumer must compare dot-separated nonnegative integer components
numerically, padding missing components with zero. For example, `2.10`
exceeds `2.9`.

If either spelling cannot be compared this way, the check is unknown, and
the consumer must warn rather than guess. This includes a valid `min` such
as `17-ea`.

### Checking requirements before installation

A consumer must first select the artifact, as
[Selecting an artifact](#selecting-an-artifact) defines. It must then
check that artifact's requirements before installing it. A consumer must
not use requirements to break selection ties or to silently redirect
installation to another build.

The consumer's response depends on which requirement failed:

| Check result | Required response |
|---|---|
| A library in `libs` is missing, or the host is below `glibc_min` or `os_min`. | Refuse installation and explain what is missing in the consumer's own terms, such as a distribution package providing the soname or an OS upgrade. The user may override the refusal for that tool. |
| A command in `bin` is missing or below `min`. | Install with a warning naming the command and required version. If the consumer can install that command, present it as an available installation. |
| A check is unknown. | Warn and continue, including when versions are unreadable or the loader cannot be queried. |

**Note:** The responses differ because of what each failure breaks.
Missing libraries or unsupported OS and glibc versions prevent the
executables from starting. A missing command affects the code paths that
invoke it, and the user might intend to install that command next.

A consumer must check libraries using the loader's own search locations:

| Platform | Search locations |
|---|---|
| Linux | The dynamic linker's cache and search path (`ldconfig -p`, `LD_LIBRARY_PATH`) |
| macOS | System library and framework directories |
| Windows | PATH with the system directory |

A consumer must find commands on PATH, adding `.exe` on Windows.

A consumer may run a command with `--version` to read its version. It may
instead leave the version unknown. Probing is optional because commands
share no version interface, and running one only because a release names
it executes a program the user did not choose to run.

**Note:** `packslip install` does not run these commands, so it warns that
it cannot check the `min` of a command it finds.

A consumer must compare `min`, `glibc_min`, and `os_min` against the
command, glibc, and OS versions respectively. A version left unknown, a
failed version probe, or a version spelling that cannot be compared leaves
the check unknown.

### Limits of host requirements

`requires` uses names the OS resolves. It does not name other projects or
their versions, and it does not say where to get them. A package manager's
installation hints belong under [`extensions`](#extensions).

Version 1 does not express:

- Alternative commands that satisfy one requirement, such as `terraform`
  or `tofu`.
- Required library symbol versions, such as `libstdc++.so.6` at
  `GLIBCXX_3.4.29`.

`glibc_min` expresses a version requirement for libc alone.

## Extensions

Use `extensions` for metadata that this specification does not define,
such as installation hints, an end-of-life date, or a build ID. An
`extensions` object is allowed in five places:

- the release predicate
- each artifact
- each resource
- the release-list predicate
- each release-list entry

Each key in an `extensions` object identifies the party that defines the
value. That party documents the value's meaning. Consumers use their own
names as keys, such as `mise` or `pacvamp`. Vendors use domains they
control, such as `example.com`.

In this example, the vendor that controls `example.com` records a build ID:

```json
"extensions": {
  "example.com": { "build_id": "20260901.3" }
}
```

The signature covers extension data. The reference CLI's `packslip show`
prints extension data unchanged.

Under `extensions`, packslip does not and will not assign meaning to data
or claim keys for future standard fields. This keeps namespaced data from
colliding with later additions to the format.

A consumer reads the keys it defines and ignores all other keys. Outside
`extensions`, consumers also ignore unknown fields. This lets revisions of
version 1 add fields without breaking older consumers.

Vendors must put their own data in `extensions` rather than inventing
fields elsewhere. A future revision could assign an invented field a
different meaning.

If an extension becomes useful across many vendors, the format may add a
standard field for it. The extension key keeps working alongside that
field.

## Repackager attestation

When a vendor publishes no packslip, a package repository (such as apt or
AUR), a registry, or a mirror may describe and sign the vendor's
artifacts. The statement it signs sets `"attested_by": "repackager"`. A
mirror can also attest to files that a vendor's packslip already covers,
as described below.

In a repackager's statement:

- `project` still identifies the vendor's project.
- The artifacts remain the vendor's files.
- `identity` identifies the repackager that signs the statement.
- `evidence` records what the repackager checked before signing.

For example:

```json
"attested_by": "repackager",
"evidence": [
  { "kind": "apt-release-gpg", "detail": "3FEF9748469ADBE15DA7CA80AC2D62742012EA22" },
  { "kind": "pkgbuild-checksums" }
]
```

Each `evidence` entry has a `kind` and an optional `detail`. The
documented kinds are:

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
| `none` | Nothing was checked; the statement carrying this evidence rests on the repackager's signature alone. |

A verified packslip from a repackager authenticates two things:

- the repackager's publication of the packslip's digests;
- the repackager's statement that it checked the listed evidence.

It does not establish vendor claims that the vendor did not sign.

A consumer must rank a repackager's packslip below a vendor's packslip. A
consumer that already holds a vendor's packslip for a project must refuse
to replace it with a repackager's packslip without a person's approval.

A mirror is a repackager whose statement keeps the vendor's digests,
substitutes the mirror's download URLs, and includes `vendor-packslip`
evidence. A consumer using the mirror obtains the same bytes under the
mirror's pin. That consumer can also fetch and verify the vendor's
packslip to retain both attestations.

## Discovery

Publish each release's bundle alongside its artifacts, either as a release
asset or in the release's directory on a download site.

Discovery tells a consumer which releases exist and where to fetch their
bundles. Where a consumer looks depends on the project:

- A [domain project](#forge-and-domain-projects) publishes a
  [release list](#the-release-list).
- A GitHub project uses the [repository's releases endpoint](#github). It
  may add a [release list](#supplementary-release-lists) to withdraw
  releases, mark security fixes, map tags to versions, or recommend a
  default.
- This version defines no discovery for `gitlab.com`. A consumer must
  find a GitLab project's packslips only through a
  [trusted publisher's list](#lists-from-other-publishers) or its own
  configuration, and must otherwise refuse the project.

Discovery does not replace verification of the selected release.

### The release list

A release list has the same signed bundle structure as a release bundle,
but its statement has the `releases/v1` predicate. The subjects for listed
packslips pin release bundles, not artifacts.

For example, this list for `mise.jdx.dev` recommends `2026.9.1`, marks it
as a security fix, and withdraws `2026.9.0`:

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
packslip, and writes `published_at` as the same instant in UTC. If the
packslip has a `source.tag`, the entry also copies it as `tag`, preserving
the vendor's spelling.

An entry may also carry:

- `status: "yanked"` for a withdrawn release, optionally with a
  `status_reason`. A consumer must never select a yanked release, and must
  warn when it holds one.
- `security: true` for a release that fixes a vulnerability. A consumer
  may shorten its minimum release age for that release.
- `evidence`, on a list from someone other than the vendor, describing
  what that publisher checked.

The optional list-level `latest` recommends the vendor's default release.
It must match an entry's exact semver `version`, including any build
metadata. It is not a tag, a range, or a per-release flag. If `latest`
names a version absent from the list, the list is invalid. If the entry it
names is yanked or otherwise ineligible, the list stays valid, and the
[Latest](#latest) fallback rules apply.

A list may have no releases, as when a GitHub vendor retires its last
supplementary entry or a domain project has no release to list. Its
statement must still carry a subject, with a `sha256` digest like any
other. The subject is needed because in-toto's statement validator
requires at least one subject, and Sigstore binds the signature to a
subject digest.

For example, the reference CLI writes one subject named `_`, in-toto's
name for a subject without a meaningful name, whose digest is the SHA-256
of empty input:

```json
"subject": [
  { "name": "_",
    "digest": { "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" } }
]
```

#### List fields

| Field | Type | Presence | Meaning |
|---|---|---|---|
| `_type` | string | required | Always `https://in-toto.io/Statement/v1`. |
| `subject[]` | array | required | At least one subject, including one for every listed packslip; a subject need not belong to a listed release. |
| `subject[].name` | string | required | For a listed packslip, its URL, as its entry's `packslip` gives it. |
| `subject[].digest.sha256` | string | required | SHA-256, lowercase hex; for a listed packslip, of its bundle file. |
| `subject[].digest.sha512` | string | optional | SHA-512, lowercase hex; for a listed packslip, of its bundle file. |
| `predicateType` | string | required | Always `https://packslip.dev/releases/v1`. |
| `predicate.project` | string | required | The project the list is for. See [Project names](#project-names). |
| `predicate.generated_at` | string | required | When the list was produced, RFC 3339, written in UTC. See [Project, version, and source](#project-version-and-source). |
| `predicate.expires_at` | string | required | When the list goes stale, RFC 3339, written in UTC. A consumer refuses an expired list. |
| `predicate.sequence` | integer | required | A nonnegative counter. A consumer refuses a list whose sequence is lower than one it has accepted. |
| `predicate.latest` | string | optional | The vendor's recommended default: the exact `version` of an entry. See [Latest](#latest). |
| `predicate.identity` | object | required | How the list is signed, with the fields a release statement's `identity` has. See [Field reference](#field-reference). |
| `predicate.releases[]` | array | required | The listed releases, in any order. It may be empty. |
| `releases[].version` | string | required | Semver 2.0.0, copied from the packslip. |
| `releases[].tag` | string | optional | The packslip's `source.tag`. |
| `releases[].published_at` | string | required | RFC 3339, copied from the packslip and written in UTC. |
| `releases[].packslip` | string | required | URL of the release's bundle. The subject of that name carries its digest. |
| `releases[].status` | string | optional | `yanked` when the release was withdrawn. A closed vocabulary: any other value makes the list invalid. |
| `releases[].status_reason` | string | optional | Why it was withdrawn. |
| `releases[].security` | boolean | optional | The release fixes a vulnerability. |
| `releases[].evidence[]` | array of object | optional | On a list from someone other than the vendor, what the publisher checked: `{ kind, detail? }`, with the kinds [Repackager attestation](#repackager-attestation) documents. |
| `extensions` | object | optional | On the list and on each entry. See [Extensions](#extensions). |

#### Freshness and rollback protection

Each listed packslip has a subject that names its URL and carries the
digest of its bundle file. The consumer therefore checks the exact release
bundle the list selected, even when a URL's contents change.

The list also carries two freshness controls, borrowed from TUF's
timestamp role:

- `expires_at` limits how long the list can be used. A consumer must
  refuse an expired list, so a mirror cannot serve an old view
  indefinitely.
- `sequence` prevents rollback. The publisher must give each new list a
  higher value than the last. A consumer must remember the highest value
  it has accepted, and must refuse a list with a lower value. An equal
  value is accepted, as when the same list is fetched again.

These controls apply to the list, separately from verification of its
release bundles.

#### Publishing a list

A domain project must publish its release list at this well-known URL:

```text
https://<host>/.well-known/packslip/<path>.json
```

Here `<host>` is the project name's host and `<path>` is the remainder of
its name. A bare host instead uses
`https://<host>/.well-known/packslip.json`.

If no list exists at the project's well-known URL, the consumer must
refuse the project rather than guess download URLs.

| Project | List URL |
| --- | --- |
| `mise.jdx.dev` | `https://mise.jdx.dev/.well-known/packslip.json` |
| `jdx.dev/mise` | `https://jdx.dev/.well-known/packslip/mise.json` |
| `github.com/jdx/mise` | `https://github.com/.well-known/packslip/jdx/mise.json` |

GitHub does not serve the last URL. The [GitHub](#github) section defines
its discovery path instead. A forge that this version does not name, and
that serves its well-known list, needs no additional discovery mechanism.

The project's domain anchors its identity, while the list and release
bundles point to wherever the bytes are hosted. A vendor with only a git
repository can keep the list and bundles at paths served as raw files, and
point artifact URLs at another host, including an LFS store. A forge's
release API is not required.

### GitHub

For `github.com/<owner>/<repo>[/<subpath>]`, the repository's releases
endpoint supplies discovery, and no release list is required:

- Include only non-draft releases that carry a packslip for the requested
  `project`. A packslip may name the same repository under a new name,
  subject to
  [Repository identity and renames](#repository-identity-and-renames).
  GitHub redirects the old API and release URLs to the renamed repository,
  so requests that use the old name still reach its releases.
- Derive each version from its tag using [Tags](#tags). This lets a
  consumer list versions without fetching every bundle. On installation,
  verify the packslip, and refuse the release if its signed `version`
  differs from the version derived from the tag. A tag that names no
  version is omitted from tag-based discovery.
- Determine ordering and prerelease status from the version, not from
  the endpoint's order or its prerelease flag. GitHub's latest-release
  pointer is an unsigned recommendation, governed by [Latest](#latest).

#### Supplementary release lists

The repository may also carry a release list on its default branch, called
a supplementary list. Its path is:

- `.well-known/packslip.json`; or
- `.well-known/packslip/<subpath>.json` for a monorepo tool. For example,
  `github.com/biomejs/biome/crates/cli` uses
  `.well-known/packslip/crates/cli.json`.

GitHub serves the file at
`https://raw.githubusercontent.com/<owner>/<repo>/HEAD/<path>`.

Like the release bundles, the list is signed from the repository through
the forge's OIDC issuer, as [Project names](#project-names) and
[Repository identity and renames](#repository-identity-and-renames)
require. It need not be signed by the workflow that signs the releases. A
consumer pins the list's signer separately, as
[consumer rule 1](#rule-1-pin-the-identity-once) says.

The list supplements the endpoint; it does not replace it:

- For a listed version, the entry's `tag`, `status`, and `security` apply.
  The consumer must accept only the bundle whose digest the list pins.
- Versions omitted from the list remain discoverable through the endpoint.

Publish a supplementary list only when the project needs to do one of these:

- withdraw a release;
- mark a security fix;
- recommend a default with `latest`;
- map a tag that names no version.

A project needing none of those omits the list. A signed withdrawal also
works with immutable GitHub releases, where the release and its bundle
cannot be deleted.

#### Keeping a supplementary list available

After a consumer accepts a supplementary list, it must remember that the
project uses one. A later missing list is an error, because returning to
endpoint-only discovery would silently undo signed withdrawals.

The vendor must re-sign the list before every `expires_at`, even if no new
release has shipped, because consumers refuse expired lists.

To retire its entries, publish a fresh, unexpired list with a higher
`sequence` that omits them, even if that leaves the list with no releases.

A user may explicitly clear the remembered list policy for that project.

### Lists from other publishers

A registry, mirror, or scanning service can publish a release list under
its own host. It publishes one list per vendor project, with the vendor's
full project name in the well-known path:

```text
https://registry.example/.well-known/packslip/github.com/jdx/mise.json
```

A consumer must trust another publisher only through configuration, and
must keep a pin for each trusted host.

Each entry pins a release bundle's digest. That bundle may be the vendor's
packslip, or a repackager's packslip signed by the list's publisher:

- Verify a vendor's packslip against the vendor's pin implied by `project`.
- Verify a repackager's packslip against the pin implied by its own
  `project` and `attested_by`.

The list's signature establishes who selected the releases and what their
`evidence` says the publisher checked. It does not establish who built the
artifacts.

This selection is a **stamp**, and the publisher is a **stamping host**.
Stamping lets a registry require a scan, or an organization curate
versions, without maintaining a per-tool installation recipe.

A consumer configured to trust stamping hosts must apply these admission rules:

- Offer and install only versions listed by at least one trusted host. A
  valid vendor packslip alone does not admit an unlisted version.
- Any one trusted host's non-yanked stamp suffices. A host withdrawing its
  own stamp does not veto another trusted host's approval. An operator
  that needs one host to control admission configures that host alone.
- A vendor withdrawal still excludes the version, regardless of stamps.
- A user may choose to trust the vendor alone for a particular project.
  For that project, accept the vendor's packslip under the vendor's pin
  without requiring a stamp.

A stamping host can use either [signing scheme](#signing), whichever fits
its operation. For example, a continuous scanning service might hold a
key, while a registry that publishes from a repository might sign
keylessly. In either case, the consumer pins the host.

## Versions

The signed `version` must use semver 2.0.0: `MAJOR.MINOR.PATCH`, with an
optional prerelease part after `-` and optional build metadata after `+`.
Calendar versions such as `2026.9.1` qualify. Consumers must refuse any
other spelling. The `packslip create` command refuses such spellings too.

The statement's `source.tag` preserves the vendor's tag. The tag may have
a different spelling, such as `v2026.9.1` or `oxlint_v1.0.0`.

### Ordering, prereleases, and channels

A consumer must derive all three of these properties from the signed
version string:

- **Ordering.** Use semver precedence, which gives ranges such as `^1.2`
  their meaning. Build metadata does not affect order. A backport
  `20.19.1` still ranks below `22.0.0`, even when it is published later.
  Neither GitHub endpoint order nor release-list order changes that. A
  vendor's separate recommendation affects only [Latest](#latest).
- **Prerelease status.** A prerelease part makes the version a
  prerelease: `1.2.0-rc.1` is a prerelease, and `1.2.0` is not. A
  consumer must skip prereleases unless they are requested. Promoting a
  release candidate requires publishing the final version. Changing a
  flag does not promote it.
- **Channel.** The first prerelease identifier names the channel if it
  is not numeric. For a channel request, a consumer must select only
  releases on that channel, ordered by semver precedence. Vendors choose
  channel names; the format defines none.

| Version | Channel |
| --- | --- |
| `1.3.0-nightly.20260904` | `nightly` |
| `1.2.0-beta.2` | `beta` |
| `1.2.0-rc.1` | `rc` |
| `1.2.0-1` | None: the first prerelease identifier is numeric. |
| `1.2.0` | None: the version has no prerelease part. |

**Note:** Separate prerelease and channel fields could contradict the
signed version or mutable forge metadata, so the format does not store
them.

Withdrawals and default recommendations belong in discovery metadata
instead. There they can change without replacing a signed release bundle.

### Spelling a version

A vendor whose versions use another spelling must choose a semver
spelling once and keep it. The tag retains the original spelling.

- A prefix belongs to the tag, not the version: `jq-1.7.1` is version
  `1.7.1`. [Tags](#tags) lists the prefixes a consumer strips on its own.
  A tag with any other prefix, such as `release-1.2.3`, needs a
  release-list entry to map it.
- A missing patch component is `0`: `4.1` is `4.1.0`.
- Remove leading zeros: `25.07.1` becomes `25.7.1`, and the date
  `2026.08.31` becomes `2026.8.31`. A vendor whose dates use dashes signs
  that same semver spelling: `2026-08-31` is version `2026.8.31`.
  Consumers do not respell dashed dates, so the tag `2026-08-31` names no
  version under [Tags](#tags). Map it with a release-list entry, or tag
  the release `v2026.08.31`.
- A fourth ordered component has no semver representation. The format
  cannot represent a scheme that needs one: build metadata does not
  affect ordering, and a prerelease part has a different meaning.

Two releases must not share a version. If the vendor's spelling would
collide, as with two releases on the same day, the semver spelling must
distinguish them with an ordered component, not build metadata.

### Tags

To derive a forge release's version from its tag:

1. Remove an optional project prefix.
2. Remove an optional `v`.
3. Apply [Spelling a version](#spelling-a-version) to the remainder.

An allowed project prefix is one of these, followed by `/`, `-`, `_`, or
`@`:

1. The tool's full subpath.
2. The last segment of that subpath.
3. The repository name.

Try the allowed prefixes in that order, then no prefix. Use the first
whose remainder yields a version. A prefix that begins the tag but leaves
no version does not end the search.

The repository name matches without regard to ASCII case, as
[Project names](#project-names) compares it. A subpath matches exactly.

| Project | Tag | Version |
| --- | --- | --- |
| Any project | `v1.2.3` | `1.2.3` |
| `github.com/oxc-project/oxc/oxlint` | `oxlint_v1.0.0` | `1.0.0` |
| `github.com/biomejs/biome/crates/cli` | `cli/v1.9.4` | `1.9.4` |
| `github.com/jqlang/jq` | `jq-1.7.1` | `1.7.1` |
| `github.com/acme/tool-suite/tool` | `tool-suite-1.2.0` | `1.2.0` |
| Any project | `v4.1` | `4.1.0` |

Deriving a version from a tag avoids fetching every bundle during
listing. The signed packslip remains authoritative. If its `version`
differs from the version derived from the tag, a consumer must refuse
the release.

Tags such as `nightly`, `release-1.2.3`, and `2026-08-31` name no version
under these rules. Tag-based listing omits them. To expose such a
release, publish a release-list entry that maps its tag to its version.

### Matching a request

A version request matches a prefix of dot-separated components:

| Request | Matches |
| --- | --- |
| `20` | Every `20.x.y` release. |
| `3.12` | Every `3.12.x` release. |
| `1.2.0-beta` | The betas of `1.2.0`. |

Build metadata is not a component, so a request without it matches a
release whose version adds it: `1.2.3` matches `1.2.3+build.5`.

A request may also use the vendor's tag, with or without its leading
`v`. A consumer must match that spelling against `source.tag` or the
release-list entry's `tag`. A user can therefore request a release by
the name the vendor published.

A matching release is eligible only if all of these hold:

- It is not yanked.
- It satisfies the prerelease policy.
- It belongs to the requested channel, when a channel was requested.

Prereleases require an explicit request. The release list's `sequence`
supplies rollback protection; a single release bundle does not.

### Latest

An unconstrained `latest` request asks for the vendor's recommended
eligible release. A consumer's default install request is also an
unconstrained `latest` request. For example, a vendor may publish
`3.0.0` while recommending `2.8.4` as the default.

Exact versions, version prefixes, ranges, and channel requests still use
their own [matching rules](#matching-a-request) and semver precedence.
The recommendation does not change their ordering.

#### Finding the recommendation

A consumer must take the recommendation from the first of these sources
that applies:

1. **The vendor's release list.** If the accepted list has `latest`, use
   it. For a GitHub project, this is the
   [supplementary list](#supplementary-release-lists). The consumer must
   verify the list's signature, identity, expiry, and sequence before
   using the pointer.
2. **GitHub's latest release endpoint.** For a GitHub project without a
   signed `latest`, use the release that the
   [latest release endpoint](https://docs.github.com/en/rest/releases/releases#get-the-latest-release)
   returns.
   - Resolve its tag through normal discovery, including any
     release-list entry that maps the tag.
   - The release must belong to the requested project. A pointer whose
     tag names no version of this project, under [Tags](#tags) or
     through a release-list entry, supplies no recommendation for this
     project. For example, `oxfmt_v1.0.0` supplies none for
     `github.com/oxc-project/oxc/oxlint`.
   - If the tag names a version of this project but the release carries
     no packslip for it, the recommendation's target is absent from
     discovery. This holds even if another release's tag names the same
     version. The consumer might learn that the release carries no
     packslip for the project only by fetching the release's bundles.
     Fall back and report the skipped recommendation as
     [Falling back to version precedence](#falling-back-to-version-precedence)
     says.
   - The endpoint is an unsigned discovery hint. It supplies neither
     authenticity nor sequence protection.
3. **No recommendation.** Outside GitHub, a project without a signed
   pointer has no recommendation.

Pointers in third-party stamping lists do not replace the vendor's
recommendation. Those lists control which candidates are admitted, as
[Lists from other publishers](#lists-from-other-publishers) describes.

#### Checking the recommended release

A consumer must verify a recommended candidate exactly as it verifies
any other release. It checks the candidate's signature and identity,
then the consistency of the bundle's version and digest. A verification
failure is an error.

A consumer must not select a verified recommendation unless it also
passes every eligibility check:

- Vendor withdrawals.
- Prerelease policy.
- Minimum release age.
- Configured stamping policy.
- Artifact and host eligibility.

A recommendation does not waive any of these checks.

#### Falling back to version precedence

In any of these cases, a consumer must select the highest-precedence
eligible release from the normal candidate set:

- There is no recommendation.
- The recommendation's target is absent from discovery.
- Eligibility policy excludes the recommendation's target.

The consumer must report any skipped recommendation and the reason. If no
eligible release exists, it must fail.

When a signed pointer is ineligible, the consumer falls directly back to
semver selection. It must not try GitHub's pointer instead.

A `latest` pointer recommends a release; it does not exclude other
versions.

For example, when the recommended `2.8.4` is yanked or too young, the
highest eligible release might be `3.0.0`. To exclude `3.0.0`, the vendor
must withdraw it or use an
[admission policy](#lists-from-other-publishers).

Fallback applies to an absent or ineligible recommendation, never to a
verification or fetch failure:

- An invalid, expired, rolled-back, or unexpectedly missing release list
  is an error.
- A candidate with a bad signature or inconsistent digest or version is
  an error.
- A GitHub latest-endpoint response indicating that no latest release
  exists supplies no pointer. Other fetch errors remain errors.

Change or remove a signed recommendation by publishing a new release
list with an increased `sequence`. No release bundle needs to be
replaced.

## Selecting an artifact

Rules 1 to 3 filter the candidate artifacts. Rules 4 to 6 choose one of
the artifacts that remain. The rules use the `os`, `arch`, `libc`,
`variant`, and `format` fields that
[Platform, format, and variant](#platform-format-and-variant) describes.

1. **Match the host.** Each of `os`, `arch`, and `libc` must be absent
   or match the host. If the host's libc is unknown, accept only
   artifacts without a `libc`.

   A consumer may support additional fallbacks, such as a `gnu` host
   using a `musl` build or an `aarch64` macOS host using `x86_64` under
   Rosetta. When the consumer supports fallbacks:

   - Use a fallback only when no artifact passes rules 1 to 3 without
     one.
   - When a fallback is used, apply rules 2 to 6 to the artifacts the
     fallback admits.
   - Report any fallback used.

   Any artifact that passes rules 1 to 3 without a fallback, even a
   portable one, therefore beats every fallback build.
2. **Match the variant.** If a variant was requested, keep only artifacts
   carrying it. If no variant was requested, keep only artifacts that
   have none.
3. **Check format support.** Keep only artifacts in formats the consumer
   can handle.
4. **Prefer specificity.** Choose the artifact that names the most of
   `os`, `arch`, and `libc`. A host-specific build therefore beats a
   portable build.
5. **Apply format preference.** For equally specific artifacts, use the
   consumer's format preference. A typical order is `tar.xz`, `tar.zst`,
   `tar.gz`, `tgz`, `tar.bz2`, `tar`, `zip`, `7z`, then single compressed
   executables, then `raw`.
6. **Refuse unresolved ties.** Two remaining artifacts with equal rank
   are a vendor error. Refuse to guess between them.

## Consumer rules

These rules cover discovery, verification, selection, and installation.
Consumers must keep state across installs to enforce signer continuity,
the no-downgrade policy, and release-list rollback protection.

### Remembered state

The rules, and the sections they link to, use this remembered state:

- **Pins:** the signer, its repository ID, and the `pin_workflow` value.
  The consumer keeps these for the releases and, separately, for a release
  list. Pins also include any signer fingerprint or lockfile commitment.
- **Accepted trust properties:** the `identity.scheme`, `attested_by`, and
  provenance of the last accepted release.
- **List history:** the highest accepted `sequence`, and whether a release
  list has ever been accepted.
- **User choices:** approved trust changes, permission to accept unlogged
  bundles, and decisions to trust a vendor without a stamp.

### Rule 1: Pin the identity once

Pin a project's release list and its releases separately. Their signers
may differ, as when a scheduled workflow re-signs the list. On a forge,
however, both are signed from the project's repository. Compare a list's
signer with the signer of the last accepted list. Compare a release's
signer with the signer of the last accepted release. Make both comparisons
as [rule 3](#rule-3-enforce-no-downgrade) compares signers.

For a forge project, use the project name to establish the first pin.
Accept only a certificate that meets both of these conditions:

- Its recorded OIDC issuer is the forge's issuer.
- Its Source Repository URI is the repository the project name
  identifies. When the certificate records no Source Repository URI, its
  identity must instead be under that repository.

Remember the repository ID the certificate records. That ID then pins the
project, as
[Repository identity and renames](#repository-identity-and-renames)
defines. Renames and transfers preserve the pin. A repository recreated
under the old name does not inherit it.

For any other project, including every domain project, pin one of these:

- A public key or identity from the consumer's maintained pins.
- On first use, a keyless identity from the release list at the
  [well-known URL](#publishing-a-list), fetched over HTTPS from the
  project's host. Take the list's signer from the list. Take the releases'
  signer from a release bundle whose digest that list pins.

For such a project, a public key comes only from the consumer's pins.

If the consumer has independently read the project's published
[signer fingerprint](#signer-fingerprints), include it in the first pin
and check releases against it. This avoids trusting a release on first
use. [Checking a fingerprint](#checking-a-fingerprint) describes the
check.

Trust another publisher's list per host, through configuration, as
[Lists from other publishers](#lists-from-other-publishers) describes.

Never take a key from the bundle or statement being verified. Never trust
a bundle's key hint.

### Rule 2: Verify the bundle and downloaded files

Apply these checks:

1. Verify the signature, certificate chain, and log entry as Sigstore
   defines them. Refuse a bundle without a log entry unless unlogged
   bundles were explicitly allowed, as
   [Unlogged bundles](#unlogged-bundles) describes.
2. Then verify the statement's structure.
3. Refuse the statement unless its `identity` names the actual signer,
   which requires all of the following:
   - `scheme` is `sigstore-oidc` for a bundle signed with a certificate,
     and `sigstore-key` for a bundle signed with a pinned key.
   - For `sigstore-oidc`, `key_id` equals the certificate's subject
     identity, and `issuer` equals the OIDC issuer the certificate
     records.
   - For `sigstore-key`, `key_id` is the pinned key's ID, compared as
     [Key signing](#key-signing) says.
4. For a forge project, check that the statement names the requested
   project or the same repository renamed. [Project names](#project-names)
   and [Repository identity and renames](#repository-identity-and-renames)
   define this check.
5. Then verify the subject digest of every downloaded artifact or asset,
   and the size of every artifact.

### Rule 3: Enforce no-downgrade

Compare each release with the last release accepted for the project.
Refuse the release until a person approves the change if any of these
hold:

- Its `identity.scheme` is weaker, which means it went from
  `sigstore-oidc` to `sigstore-key`.
- Its signer changed.
- Its `attested_by` went from `vendor` to `repackager`.
- It dropped provenance links from a build both releases offer. If the
  last release carried links, a release that offers none of the builds
  that carried them has dropped them too.

#### Comparing signers

Compare keyless signers by workflow path, without the ref. A new tag from
the same workflow therefore keeps the same signer.

Across a repository rename or transfer, compare the path inside the
repository only after the repository IDs establish that it is the same
repository. Use the repository ID from the release's certificate. When
comparing two recorded signers, such as lockfile entries, use the ID
recorded with each instead. A changed repository ID means a changed
signer, even if the identity string matches.

A reusable workflow of another repository is a different signer from the
project's own workflow at the same path, as
[Repository identity and renames](#repository-identity-and-renames)
describes.

When `identity.pin_workflow` is `false`, compare signers by repository
alone. If the remembered value is `true`, changing it to `false` weakens
the pin. That change requires a person's approval, as
[Workflow pinning](#workflow-pinning) describes.

#### Comparing provenance

Compare provenance build by build. A build is the set of artifacts that
share `os`, `arch`, `libc`, and `variant`, whatever their `format`, as
[Platform, format, and variant](#platform-format-and-variant) describes.
URLs change between releases, so compare counts of URLs:

1. Skip a build the new release no longer offers.
2. For each build in both releases, count the distinct `provenance` URLs
   of each of its artifacts.
3. Sort each release's counts for the build from largest to smallest.
4. Pair the two sorted lists in order until the shorter list ends.
5. The new release dropped provenance if any of its counts is lower than
   the count it is paired with.

For example, suppose the last release gave a build's `tar.gz` and `zip`
artifacts two URLs each. A release that offers only the `tar.gz`, with
two URLs, keeps provenance. A release whose `zip` has one URL drops it.

### Rule 4: Apply the minimum release age

Measure any configured minimum release age from the log's integration
time. Use `published_at` only for an unlogged bundle the consumer chose to
accept.

### Rule 5: Use the project's discovery metadata

Find the project's releases through its discovery source:

- For a GitHub project, read the releases endpoint and any
  [supplementary release list](#supplementary-release-lists).
- For a `gitlab.com` project, this version defines no discovery. Find a
  GitLab project's packslips only through a trusted publisher's list or
  the consumer's own configuration. Otherwise, refuse the project.
- For any other project, including every domain project, read the
  release list at the [well-known URL](#publishing-a-list).

Refuse a project with neither discovery source, and refuse when a
previously accepted release list is now missing, as [GitHub](#github)
describes.

Refuse a release list that has expired. Refuse a release list whose
`sequence` is below the last one accepted.

When selecting a release:

- Never select a yanked entry.
- Skip prereleases unless asked for them.
- Rank by semver precedence.
- For an unconstrained request, take the vendor's recommendation, as
  [Latest](#latest) says.

Refuse a release bundle whose digest is not the one the list pins. Refuse
a release bundle whose `version` is not the one its tag or list entry
named.

When the consumer trusts stamping hosts, select only versions one of them
lists, as [Lists from other publishers](#lists-from-other-publishers)
says. This restriction does not apply to a project for which the user
chose to trust the vendor alone.

### Rule 6: Select one artifact

Apply [Selecting an artifact](#selecting-an-artifact). Refuse unresolved
ties.

### Rule 7: Check host requirements before installing

Check `requires` for the artifact selected by
[rule 6](#rule-6-select-one-artifact), as
[Host requirements](#host-requirements) defines.

- Refuse the install when a library, glibc, or OS version prevents the
  executables from starting.
- Warn when a required command is missing or known to be too old.
- Report the refusal or the warning in the consumer's own terms.
- A requirement the consumer cannot check produces a warning, never a
  refusal. This includes a command's `min` when the consumer did not read
  the command's version.

Requirements do not resolve an ambiguous artifact selection.

### Rule 8: Install resources

Install resources as [Resources](#resources) defines. For each resource
identity:

1. Keep the entries whose scope fits the selected artifact.
2. Of those, keep the most specific entries, as
   [Selecting resources](#selecting-resources) defines.
3. Choose the most verifiable source, in the order that
   [Source types](#source-types) and
   [Fallback and verification](#fallback-and-verification) give.

Run an `exec` completion when a shell first requests it, and cache the
result. Run any other `exec` entry only if the user chose to run vendor
code at install time, as [Running an exec entry](#running-an-exec-entry)
describes.

Ignore kinds the consumer does not know.

If a resource cannot be fetched, report it and finish the install without
it. If a resource's digest differs from the signed digest, fail the
install.

## JSON schemas

The JSON schemas describe the decoded in-toto statements inside bundles:

| Document | Download | CLI |
| --- | --- | --- |
| One release (`release/v1`) | [release-v1.json](https://packslip.dev/schema/release-v1.json) | `packslip schema` |
| A release list (`releases/v1`) | [releases-v1.json](https://packslip.dev/schema/releases-v1.json) | `packslip schema --releases` |

The schemas check field shapes, required values, and value patterns.
They do not validate the enclosing Sigstore bundle or authenticate its
signer. They also do not apply rules that require comparing fields,
downloaded bytes, or remembered state. The
[reference implementation](#reference-implementation) validates those
relationships, and consumers also apply the
[consumer rules](#consumer-rules).

The release schema, which `packslip schema` prints, checks types, required
fields, token patterns, semver grammar, and digest lengths. The reference
implementation's validator checks the remaining rules. These include
[project-name grammar](#name-syntax), the fixed `_type` and
`predicateType`, at least one artifact, and the cross-references between
subjects, artifacts, and resources. Passing validation against the release
schema alone does not establish those relationships.

To decode the statement inside a bundle, see
[Inspecting a bundle](#inspecting-a-bundle).

## Stability

Version 1 is final. The predicate types `https://packslip.dev/release/v1`
and `https://packslip.dev/releases/v1` identify this format contract. A
bundle that verifies under them today continues to verify for as long as
its signing material holds.

Security policy and freshness checks still apply. Format stability does
not require accepting expired metadata or overriding a security
rejection. Even though version 1 is final, a verifier can still need
updates to support new Sigstore bundle or log formats.

**Note:** The reference implementation's
[compatibility matrix](https://packslip.dev/docs/compatibility/) records
the release baselines, historical verification material, and
authenticated root rotations it has tested.

### What version 1 holds fixed

- The meaning and default of every defined field. A field cannot be
  redefined, narrowed, or given a different default.
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

A revision of version 1 may:

- Add an optional field at any level. Consumers ignore fields they do not
  know. An older consumer therefore reads a newer statement correctly, and
  a newer consumer reads an older one.
- Add a vocabulary member, such as a resource kind, evidence kind,
  format, OS token, or architecture token. Consumers ignore unfamiliar
  members except where a rule in this specification says otherwise.
  Three vocabularies are closed in version 1 and gain no members:
  `identity.scheme`, `attested_by`, and `releases[].status`. A value this
  specification does not define for one of them makes the statement
  invalid.
- Clarify ambiguous wording where implementations already agreed on its
  meaning.
- Add examples and guidance.

Removing a field, making an optional field required, changing a selection
rule's result, or making any other change outside these bounds requires
version 2. Version 2 gets its own predicate type, published
alongside version 1. Consumers may accept both.

No revision of this specification invalidates a release already signed
under `release/v1`. Vendors are not required to migrate.

### Format and implementation versions

The predicate types version the format. Separately, the reference Rust
crate, CLI, and two GitHub Actions share an implementation version that
follows semver. The `packslip` npm package and the Go module
`github.com/jdx/packslip/go` (tagged `go/vX.Y.Z`) are released at that
same version. The implementation can reach version 2 while the format
stays at version 1.

### Conformance coverage

The
[conformance vectors](https://github.com/jdx/packslip/tree/main/tests/conformance)
express the rules they cover as executable examples. They cover artifact
and resource selection, tag versions, statement validity, forge identity,
and signer fingerprints.

The text of this specification governs. A vector that contradicts the
text is a bug to fix. Otherwise, an implementation that disagrees with a
vector disagrees with this specification.

The forge identity vectors include a remembered pin in each case. The
other vectors do not cover rules that depend on state retained between
installs, such as no-downgrade and release-list sequences. Consumers must
implement those [stateful rules](#consumer-rules) as well.

## Reference implementation

This section is informative: it describes the reference implementation
and adds no format rule.

The [packslip repository](https://github.com/jdx/packslip) holds the
reference Rust library, the CLI, and two composite GitHub Actions:

- The [release action](https://github.com/jdx/packslip/blob/main/action.yml)
  publishes a release's packslip.
- The
  [release-list action](https://github.com/jdx/packslip/blob/main/releases/action.yml)
  builds and signs discovery metadata.

### Reference CLI behavior

The CLI creates and verifies release bundles and release lists, prints
signer fingerprints, and installs tools. Its commands serve different
parts of the consumer workflow:

- `packslip install` discovers releases, selects and installs an
  artifact, and remembers trust state across installs.
- `packslip verify` checks a supplied bundle and local files without
  keeping that installation state.

#### Creating and signing

`packslip create` creates and signs a release bundle. It uses
[keyless signing](#keyless-signing) (`sigstore-oidc`) unless given
`--key`. It takes the OIDC token from `SIGSTORE_ID_TOKEN` when that
variable is set, and otherwise from the CI job's ambient credential.
Supported ambient credentials include GitHub Actions, GitLab CI, and the
other providers Sigstore's clients detect. The command fails if neither
token source is available.

`packslip releases` builds a [release list](#the-release-list) from local
release bundles. `--latest` sets the list-level recommendation. Without
`--latest`, the list has no recommendation.

#### Verifying a bundle

`packslip verify` checks the supplied bundle and the local files passed
with `--artifact`. It reports signing information, provenance links,
resources, and host requirements. It does not fetch provenance, install
resources, or keep trust history across invocations. For a release list,
it also does not enforce expiry or compare the list's sequence with a
previously accepted list.

Without `--pubkey`, `--identity`, `--identity-prefix`, `--issuer`, or
`--source-repository`, the command derives its signer policy from the
GitHub or GitLab project the statement claims. That policy requires the
forge's issuer and a certificate bound to that repository, as
[Repository identity and renames](#repository-identity-and-renames)
describes. A release signed through another repository's reusable
workflow therefore verifies. As [Project names](#project-names) explains,
this default does not check the user's intended identity.

Supply the expected public key with `--pubkey`, or the expected identity
(`--identity`) or identity prefix (`--identity-prefix`) together with
`--issuer`. `--source-repository` with `--issuer` instead requires the
certificate to record the expected repository as its source, whichever
workflow signed.

What the identity flags check depends on the project:

- **GitHub or GitLab project.** Identity flags narrow the policy rather
  than replace it. The signer must match them and still belong to the
  project's repository, as
  [Repository identity and renames](#repository-identity-and-renames)
  describes. So `--issuer` alone accepts only that repository's signers.
- **Any other project.** Only the supplied identity flags are checked, so
  `--issuer` alone accepts any signer from that issuer.

`--allow-other-repository` lifts the requirement that a GitHub or GitLab
project's signer belong to the project's repository, for a bundle signed
outside it, such as a repackager's. It needs `--issuer` with
`--identity`, `--identity-prefix`, or `--source-repository` to name that
signer.

A supplied `--pin` adds the signer-fingerprint check to the identity
policy: `packslip verify --pin` checks a release against the supplied
signer fingerprint. `packslip pin` prints a release's signer fingerprint.

With any policy, compare the reported project and version with the
request. A signer can sign other projects and older versions, and
`packslip verify` does not know which one the user asked for. See
[Verify a release](https://packslip.dev/docs/verifying/) for CLI
examples.

#### Inspecting a bundle

`packslip show` decodes a bundle's payload. Neither form verifies it:

```sh
packslip show packslip.sigstore.json       # Decode and pretty-print the statement.
packslip show --raw packslip.sigstore.json # Print the signed payload, then a newline.
```

The payload can also be decoded with
`jq -r .dsseEnvelope.payload BUNDLE | base64 -d`.

For an authenticated check, see
[Verify a release](https://packslip.dev/docs/verifying/). To install
packslip, see the [CLI overview](https://packslip.dev/cli/).

### Library operations

Other consumers build their discovery, installation, and trust policies
around the library's verification and selection operations. The
reference library exposes these operations:

| Operation | What it does | Section |
| --- | --- | --- |
| `sigstore::source_repository` | Reads the Source Repository extensions from a bundle's signing certificate. | [Repository identity and renames](#repository-identity-and-renames) |
| `forge::check_source` | Classifies a release into the repository identity cases. | [Repository identity and renames](#repository-identity-and-renames) |
| `verify_forge` | Verifies a bundle and classifies it into those cases in one call. | [Repository identity and renames](#repository-identity-and-renames) |
| `forge::same_workflow` | Compares two signers a consumer recorded, with the repository ID recorded alongside each, for consumer rule 3 when no release is at hand. | [Rule 3: Enforce no-downgrade](#rule-3-enforce-no-downgrade) |
| `Fingerprint::of_signer` | Computes a signer fingerprint. | [Computing a fingerprint](#computing-a-fingerprint) |
| `Fingerprint::verify` | Checks a signer fingerprint. | [Checking a fingerprint](#checking-a-fingerprint) |
| `select_resources` | Selects the resources to use with an artifact. | [Selecting resources](#selecting-resources) |
| `list_url` | Returns the URL of a project's own release list, and, for a publisher other than the project's host, the URL of that publisher's list. | [Publishing a list](#publishing-a-list), [Lists from other publishers](#lists-from-other-publishers) |
| `github_list_path` | Returns the repository-relative path of a GitHub supplementary list. | [Supplementary release lists](#supplementary-release-lists) |
| `select_artifact` | Selects the one artifact for a host. | [Selecting an artifact](#selecting-an-artifact) |

The
[conformance vectors](https://github.com/jdx/packslip/tree/main/tests/conformance)
for signer fingerprints give inputs, their fingerprints, texts that are
not fingerprints, and matches and mismatches.

### Guides and command reference

Use the guides for task-oriented examples and the command reference for
flags and arguments:

| Task | Guide | Command reference |
| --- | --- | --- |
| Install the CLI | [Install packslip](https://packslip.dev/docs/getting-started/#install-packslip) | [CLI overview](https://packslip.dev/cli/) |
| Install an upstream tool | [Bootstrap a tool](https://packslip.dev/docs/bootstrap/) | [install](https://packslip.dev/cli/install/) |
| Create a first packslip | [Try packslip locally](https://packslip.dev/docs/getting-started/) | [create](https://packslip.dev/cli/create/) |
| Generate an Ed25519 key | [Try packslip locally](https://packslip.dev/docs/getting-started/#create-a-sample-release) | [keygen](https://packslip.dev/cli/keygen/) |
| Publish from GitHub | [Publish with GitHub Actions](https://packslip.dev/docs/publishing/) | [action.yml](https://github.com/jdx/packslip/blob/main/action.yml) |
| Describe release files | [Describe your artifacts](https://packslip.dev/docs/describing-releases/), [Ship resources](https://packslip.dev/docs/resources/), [Declare host requirements](https://packslip.dev/docs/host-requirements/), [Release recipes](https://packslip.dev/docs/recipes/) | [create](https://packslip.dev/cli/create/) |
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
