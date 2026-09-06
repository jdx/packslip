---
title: Documentation
description: Guides and reference for publishing and verifying packslip release manifests.
---
# Documentation

Choose a path based on whether you publish software, install it, or build
an integration. For the relationship between configuration, signing, and
discovery, read [How packslip fits a release](/docs/release-workflow/).

## Try packslip {#guides}

[Getting started](/docs/getting-started/) creates and verifies a small local
release. It needs no CI account or connection to the signing services once
the CLI is installed.

## Publish software

Follow these guides in the order your release needs them:

1. [Artifact configuration](/docs/describing-releases/): describe platforms,
   executable paths, and variants, using flags or a TOML manifest.
2. [Resources](/docs/resources/) and [host requirements](/docs/host-requirements/):
   describe additional files and what the host must provide.
3. [Release recipes](/docs/recipes/): adapt a Rust, Go, monorepo, or desktop layout.
4. [Publish with GitHub Actions](/docs/publishing/): sign and upload the bundle
   from your release job.
5. [Manage release lists](/docs/release-lists/): publish discovery metadata,
   withdraw versions, and recommend a default release.
6. [Host releases on your own domain](/docs/self-hosting/): name a project
   after its download host and publish its releases and list there.

## Install software or build a consumer

- [Using packslip with mise](/docs/mise/) shows artifact installation,
  completions, skills, and trust continuity in a consumer.
- [Verify a release](/docs/verifying/) explains trusted identities, file
  verification, and the additional policy an installer must enforce.
- [Consumer rules](/release/v1/#consumer-rules) define the complete contract.
  A successful CLI verification alone does not implement that contract.

## Reference

- [CLI reference](/cli/): every command, argument, and flag.
- [Specification](/release/v1/): release statements, signing, discovery,
  version selection, and consumer requirements.
- JSON schemas: [release statement](/schema/release-v1.json) and
  [release list](/schema/releases-v1.json). These describe the decoded
  in-toto statements, not the enclosing sigstore bundles.
- [Contributing](https://github.com/jdx/packslip/blob/main/CONTRIBUTING.md):
  build the project and edit the documentation.

## Terms used in these docs

| Term | Meaning |
| --- | --- |
| Artifact | A release file, such as an archive, installer, or executable. |
| Resource | An additional item, such as a completion script, man page, skill, or SBOM. |
| Statement | The JSON document containing digests and release metadata. |
| Bundle | The signed statement and its verification material, stored as `packslip.sigstore.json`. |
| Release list | A separate signed document that indexes releases and records mutable metadata such as withdrawals. |
| Consumer | An installer, package manager, mirror, or other tool that reads and verifies packslips. |
| Pin | The identity or public key a consumer has chosen to trust. |
