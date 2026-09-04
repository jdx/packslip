---
title: Documentation
description: Guides and reference for publishing and verifying packslip release manifests.
---
# Documentation

Start with a working example, then add the metadata your release needs.
The guides explain the workflow; the specification defines the format
and the rules consumers must follow.

## Guides

| I want to… | Read |
| --- | --- |
| Create and verify my first packslip | [Getting started](/docs/getting-started/) |
| Add packslip to a GitHub release job | [Publish with GitHub Actions](/docs/publishing/) |
| Configure platforms and executable paths | [Artifact configuration](/docs/describing-releases/) |
| Ship completions, skills, or desktop files | [Resources](/docs/resources/) |
| Declare dependencies and minimum OS versions | [Host requirements](/docs/host-requirements/) |
| Adapt a common release layout | [Release recipes](/docs/recipes/) |
| Verify downloads or build a consumer | [Verify a release](/docs/verifying/) |
| Use packslip with mise | [Using packslip with mise](/docs/mise/) |
| Publish a release index or withdraw a version | [Manage release lists](/docs/release-lists/) |

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
