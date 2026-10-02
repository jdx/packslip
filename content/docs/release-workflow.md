---
title: How packslip fits a release
weight: 5
group: start
description: Follow release files from local configuration to a signed bundle, discovery, and verified installation.
---
# How packslip fits a release

packslip adds a signed description to the release files you already build.
Your build produces the software; packslip records what shipped; a consumer
uses that record to choose and verify a download. This overview is for
vendors and for anyone building a consumer. It follows one release through
those steps and names the files involved.

## From build to installation

1. **Build and package.** Produce the final archives, executables, installers,
   and separate resource files. Finish any platform signing or notarization
   that changes those files before creating the packslip.
2. **Describe and sign.** Give `packslip create` the local files and release
   metadata. It hashes the files, records their platform and layout, and
   signs the resulting statement with a CI identity or an Ed25519 key.
3. **Publish.** Upload the artifacts and any separate resource assets to the
   URLs the statement records, and the bundle beside them. Each URL must
   serve exactly the bytes that were hashed. On GitHub, the `jdx/packslip`
   action uploads only the bundle to the release (unless its `upload` input
   is `false`); it never uploads your artifacts or resource assets.
4. **Make the release discoverable.** A GitHub project whose tags name
   their versions needs nothing more: consumers find its releases through
   GitHub and read each version from its tag. A project named after its own
   domain publishes a signed release list at
   `https://<host>/.well-known/packslip.json`, or
   `https://<host>/.well-known/packslip/<path>.json` for a name with a path.
   Consumers refuse such a project without one.
5. **Verify and install.** The consumer finds an eligible release, verifies
   its signer and metadata against the requested project, chooses an artifact,
   and checks the downloaded bytes before unpacking or running them.

The CLI creates and verifies one document at a time and checks local files
against it. A consumer, such as a package manager, adds discovery, artifact
selection, installation, and the trust state it keeps for each project
across installs, such as its pin and the highest release-list sequence it
has accepted. See [Build an installer or mirror](/docs/installers/) for
what a consumer adds, and [Use packslip with mise](/docs/mise/) for one
consumer's choices.

## Three documents with different jobs

The word *manifest* can mean the vendor's TOML input or the signed release
record. They are different files:

| Document | Who creates it | What it contains | Where it belongs |
| --- | --- | --- | --- |
| `release.toml` | The vendor | Local paths, metadata overrides, and resource declarations for `create --manifest`. | In the source repository or build workspace; it is optional. |
| `packslip.sigstore.json` | `packslip create`, or the `jdx/packslip` action | One release statement, its signature, and verification material. | Beside the published release files. |
| `packslip.json` | `packslip releases`, or the `jdx/packslip/releases` action | A signed list of release bundles, their digests, and discovery policy such as withdrawals. | At `/.well-known/packslip.json` on the project's host (`/.well-known/packslip/<path>.json` for a name with a path), or in the `.well-known/` directory on a GitHub repository's default branch for its optional list. |

`release.toml` is not the document consumers verify. `packslip create`
resolves its local paths into subject names, digests, download URLs, and
artifact metadata. Uploading the TOML alone does not publish a packslip.
See [Use a TOML manifest](/docs/describing-releases/#use-a-toml-manifest)
for its keys.

`packslip.sigstore.json` is a **sigstore bundle** that wraps a signed in-toto
**statement**. The statement's `subject` lists file digests; its `predicate`
holds the project, version, artifacts, and resources. `packslip show`
displays that inner statement without verifying it. The JSON schemas
describe the inner statements, not the TOML input or the outer bundle.

`packslip releases` and the releases action write the list to
`packslip-releases.sigstore.json`, or to the file `--out` or the `out`
input names. Publish that file under the name and location in the table.

## Identity, version, and location

Keep these values distinct when configuring a release:

| Value | Example | Purpose |
| --- | --- | --- |
| Project | `github.com/owner/repo/mytool` | Names the tool the consumer requested. It has no URL scheme. |
| Version | `1.2.3` | The release's semver version, used for selection. |
| Source tag | `mytool-v1.2.3` | Preserves the vendor's tag spelling. |
| Artifact URL | `https://github.com/owner/repo/releases/download/mytool-v1.2.3/mytool-linux-x64.tar.gz` | Locates bytes whose digest is in the signed statement. |
| Signer | A workflow identity and issuer, or a trusted public key | Authenticates the statement. |

A monorepo subpath names a tool; its GitHub signer is still pinned to the
repository. Artifact URLs can point at any download host; the host locates
files and does not replace the signer pin. Consumers check both the
expected signer and the signed project and version.

A keyless project can also publish its signer fingerprint, the `ps1_…` value
that `packslip pin` prints, so a consumer does not have to trust the first
release it sees; see
[Publish your signer fingerprint](/docs/publishing/#publish-your-signer-fingerprint).

The version must be semver whatever the tag looks like: a tag `v4.1`
describes version `4.1.0`. The action's default version is the tag with a
leading `v` removed. For a tag such as `v4.1` or `mytool-v1.2.3`, set the
action's `version` input (`4.1.0`, `1.2.3`); otherwise `packslip create`
stops with `version must be semver 2.0.0`. On GitHub, consumers also list
versions from tags, so a release whose tag names no version is hidden
unless a signed release list in the repository maps it. See
[Spelling a version](/release/v1/#spelling-a-version).

## What changes after publication

Do not change a release's files or its bundle once they are published. A
changed archive no longer matches its signed digest, so consumers refuse it.
A re-signed bundle has a new digest, so it no longer matches the digest a
release list recorded for it. Publish changed software as a **new release**.

Use a **new signed release list** to withdraw a release, mark a security
fix, recommend a default version, or extend the list's expiry. Each new
list needs a higher `sequence` than the last and an expiry in the future;
the release bundles stay as they are. A list is an ongoing commitment:
consumers refuse an expired list, and a consumer that has accepted a list
treats a missing one as an error, so re-sign it before it expires even
when nothing else has changed.

On GitHub, a release list is optional and supplements what consumers find
through GitHub releases: a version the list leaves out is still offered.
To withdraw a version, list it as yanked (`packslip releases --yank`) and
keep that yanked entry in every later list. Leaving the version out does
not withdraw it, and a later list that drops the entry offers it again.
See [Manage release lists](/docs/release-lists/).

## Put it into practice

- [Getting started](/docs/getting-started/) creates and verifies a local sample.
- [Publish with GitHub Actions](/docs/publishing/) adds signing to a release job.
- [Verify a release](/docs/verifying/) shows what a consumer checks and how
  to pin a signer.
- [Documentation](/docs/) lists the guides for configuration, release
  lists, and building a consumer.
