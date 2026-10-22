---
title: How packslip works
weight: 5
group: start
description: Follow one release from build to installation, and see which files belong in your repository, in the release, and at the well-known URL.
---
# How packslip works

For publishers and installer authors new to packslip: follow one release
from build to installation, and see which files belong in your repository,
in the release, and at the well-known URL. To run commands now, use
[Try packslip locally](/docs/getting-started/) or
[Bootstrap a tool](/docs/bootstrap/).

A release's packslip connects the vendor's release job with the installer
that consumes its output. The vendor signs a description of the files it
built. The installer uses that description to select a download, check
who published it, and check that the downloaded bytes match. Adopting
packslip requires no change to your build or release hosting.

## From build to installation

1. **Build and package.** Produce the final archives, executables,
   installers, and separate resource files. Finish any platform signing or
   notarization that changes the files before you create the packslip,
   because the packslip records the final bytes.
2. **Describe and sign.** `packslip create` takes the local files and
   release metadata, hashes the files, and records their platform and
   layout. It signs the resulting statement with a CI identity (keyless)
   or an Ed25519 key. Either way, the result is an in-toto statement in a
   Sigstore bundle.
3. **Publish.** Upload the artifacts and any separate resource assets to
   the URLs the statement records, and put the release bundle beside
   them. Each recorded URL must serve exactly the bytes that were hashed.
   On GitHub, the `jdx/packslip` action signs the bundle and, by default,
   uploads it to the release; see
   [Publish with GitHub Actions](/docs/publishing/).
4. **Make the release discoverable.** A GitHub project whose tags name
   their versions needs nothing more: consumers find its releases through
   GitHub and read each version from its tag. A domain project publishes a
   signed release list at its well-known URL. Consumers refuse a domain
   project that has no release list there. A `gitlab.com` project has no
   [discovery](/release/v1/#discovery), so `packslip install` refuses it.
5. **Verify and install.** The consumer finds an eligible release and
   verifies its signer and metadata against the requested project. It then
   chooses an artifact and checks the downloaded bytes before unpacking or
   running them.

Two packslip commands verify releases:

- `packslip verify` checks a bundle and the files you downloaded against
  the signer a GitHub or GitLab project's name implies, or one you name
  with an identity or public key. `--pin` adds a signer-fingerprint check;
  see [Verify a release](/docs/verifying/).
- `packslip install` also discovers releases, selects an artifact, and
  installs it. Between runs it pins the releases' signer and the release
  list's signer separately, and remembers the list's highest sequence; see
  [What each consumer enforces](/docs/trust/#what-each-consumer-enforces).
  A package manager or other installer can play the same role; see
  [Build an installer or mirror](/docs/installers/).

## Three files with different jobs

Only the two signed files, the release bundle and the release list, are
published for consumers. release.toml is local input to the release job.

| File | Created by | Contains | Where it belongs |
| --- | --- | --- | --- |
| release.toml (optional) | The vendor | Local paths, metadata overrides, and resource declarations, passed with `packslip create --manifest`. | The source repository or build workspace. |
| The release bundle, `packslip.sigstore.json` | `packslip create`, or the `jdx/packslip` action | One release statement, its signature, and verification material. | Beside the published release files. |
| The release list | `packslip releases` or the `jdx/packslip/releases` action | A signed list of release bundles, their digests, and discovery policy such as withdrawals. | A domain project's well-known URL: `https://<host>/.well-known/packslip.json`, or `https://<host>/.well-known/packslip/<path>.json` for a name with a path. A GitHub repository's optional list goes under `.well-known/` on its default branch. |

`packslip create` resolves release.toml's local paths into subject names,
digests, download URLs, and artifact metadata, so uploading release.toml
alone does not publish a packslip. The
[release.toml reference](/docs/release-toml/) lists its keys.

Inside the release bundle, the statement's `subject` lists file digests,
and its `predicate` holds the project, version, artifacts, and resources.
`packslip show` prints the statement without verifying it. The
[JSON schemas](/release/v1/#json-schemas) describe the decoded statements,
not release.toml or the outer Sigstore bundle.

## Identity, version, and location

Keep these values distinct when you configure a release:

| Value | Example | Purpose |
| --- | --- | --- |
| Project | `github.com/owner/mytool` | Names the tool the consumer requested. It has no URL scheme. |
| Version | `1.2.3` | The release's semver version, used for selection. |
| Source tag | `mytool-v1.2.3` | Preserves the vendor's tag spelling. |
| Artifact URL | `https://github.com/owner/mytool/releases/download/mytool-v1.2.3/mytool-linux-x64.tar.gz` | Locates bytes whose digest is in the signed statement. |
| Signer | A workflow identity and issuer, or a trusted public key | Authenticates the statement. |

Artifact URLs can point at any download host.
[How trust works](/docs/trust/) explains which signer a consumer expects
for a project, and why the download host does not replace that check.
The version is semver whatever the tag looks like; see
[Spelling a version](/release/v1/#spelling-a-version).

## What changes after publication

**Warning:** Do not change a release's files or its bundle once they are
published. A changed archive no longer matches its signed digest, so
consumers refuse it. A re-signed bundle has a new digest, so it no longer
matches the digest a release list recorded for it. Publish changed
software as a new release.

Publish a new signed release list to withdraw a release, mark a security
fix, recommend a default version, or extend the list's expiry. The release
bundles stay unchanged. [Manage release lists](/docs/release-lists/)
explains sequence numbers, expiry, and what a GitHub project's list must
keep.

## Put it into practice

- To publish: [Try packslip locally](/docs/getting-started/), then
  [Publish with GitHub Actions](/docs/publishing/) or
  [Publish from other CI](/docs/other-ci/). To serve a domain project, see
  [Host releases on your own domain](/docs/self-hosting/).
- To install, verify, or build an installer: [Bootstrap a tool](/docs/bootstrap/),
  [Verify a release](/docs/verifying/),
  [Use packslip with mise](/docs/mise/), or
  [Build an installer or mirror](/docs/installers/).
- For background: [How trust works](/docs/trust/) and the
  [Glossary](/docs/glossary/).
