---
title: How packslip fits a release
weight: 5
description: Follow release files from local configuration to a signed bundle, discovery, and verified installation.
---
# How packslip fits a release

packslip adds a signed description to the release files you already build.
Your build produces the software; packslip records what shipped; a consumer
uses that record to choose and verify a download.

## From build to installation

1. **Build and package.** Produce the final archives, executables, installers,
   and separate resource files. Finish any platform signing or notarization
   that changes those files before creating the packslip.
2. **Describe and sign.** Give `packslip create` the local files and release
   metadata. It hashes the files, records their platform and layout, and
   signs the resulting statement with a CI identity or an Ed25519 key.
3. **Publish.** Upload the bundle, artifacts, and separate resource assets.
   The URLs in the statement must point to the exact bytes that were hashed.
4. **Make the release discoverable.** On GitHub, attach the bundle to the
   release. For a project on its own domain, publish a signed release list
   at the project's well-known URL.
5. **Verify and install.** The consumer finds an eligible release, verifies
   its signer and metadata against the requested project, chooses an artifact,
   and checks the downloaded bytes before unpacking or running them.

The CLI covers creation and verification of individual documents and local
files. An installer adds discovery, artifact selection, installation, and
remembered trust policy. See [verification](/docs/verifying/) for that boundary
and [mise](/docs/mise/) for a consumer example.

## Three documents with different jobs

The word *manifest* can mean the publisher's input or the signed release
record. They are different files:

| Document | Who creates it | What it contains | Where it belongs |
| --- | --- | --- | --- |
| `release.toml` | The publisher | Local paths, metadata overrides, and resource declarations for `create --manifest`. | In the source repository or build workspace; it is optional. |
| `packslip.sigstore.json` | `packslip create` | One release statement, its signature, and verification material. | Beside the published release files. |
| `packslip.json` | `packslip releases` | A signed list of release bundles, their digests, and discovery policy such as withdrawals. | At the project's discovery location. |

`release.toml` is not the document consumers verify. Creation resolves its
local paths into subject names, digests, download URLs, and artifact metadata.
Uploading the TOML alone does not publish a packslip.

A release bundle contains a **statement** inside a **sigstore bundle**. The
statement's `subject` lists file digests; its `predicate` holds the project,
version, artifacts, and resources. `packslip show` displays that inner statement
without verifying it. The JSON schemas describe the inner statements, not the
TOML input or the outer bundle.

## Identity, version, and location

Keep these values distinct when configuring a release:

| Value | Example | Purpose |
| --- | --- | --- |
| Project | `github.com/owner/repo/mytool` | Names the tool the consumer requested. It has no URL scheme. |
| Version | `1.2.3` | The release's semver version, used for selection. |
| Source tag | `mytool-v1.2.3` | Preserves the publisher's tag spelling. |
| Artifact URL | `https://downloads.example.com/mytool/1.2.3/mytool-linux-x64.tar.gz` | Locates bytes whose digest is in the signed statement. |
| Signer | A workflow identity and issuer, or a trusted public key | Authenticates the statement. |

A monorepo subpath names a tool; its GitHub signer is still pinned to the
repository. A download host locates files; it does not replace the signer pin.
Consumers check both the expected signer and the signed project and version.

The format requires semver even when a publisher's tag uses another spelling.
For example, a tag `v4.1` can describe version `4.1.0`. Pass the normalized
version explicitly when creating such a release; do not assume the action
normalizes arbitrary tags. See [version spelling](/release/v1/#spelling-a-version)
for supported mappings and their limits.

## What changes after publication

Treat an individual release bundle and the files it describes as the record
of that release. Changing an archive after signing breaks its digest match.
Changing the bundle also changes the digest recorded in any signed release list.

Use a **new release** for changed software. Use a **new signed release list**
to withdraw a release, mark a security fix, recommend a default version, or
refresh discovery metadata. List updates need an increased sequence and a
current expiry; they do not re-sign or replace the individual release bundles.

A GitHub list is supplementary: versions omitted from it can still be found
through GitHub releases. Keep an explicit yanked entry to withdraw a version;
omitting the entry does not withdraw it. See [release lists](/docs/release-lists/).

## Put it into practice

- [Getting started](/docs/getting-started/) creates and verifies a local sample.
- [Artifact configuration](/docs/describing-releases/) maps your release layout
  to executable paths, platforms, and variants.
- [Publish with GitHub Actions](/docs/publishing/) adds signing to a release job.
- [Manage release lists](/docs/release-lists/) covers discovery and ongoing updates.
