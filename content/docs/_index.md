---
title: Documentation
description: Install tools, publish signed releases, verify downloads, or build a consumer with packslip.
---
# Documentation

Choose a guide for the task you want to do. The CLI can install tools from
their signed releases, publishers can add a packslip to an existing release
job, and consumer developers can use the format and Rust crate in their own
installers or mirrors.

## Start here {#guides}

| You want to… | Start here |
| --- | --- |
| Get the command on your machine | [Install the CLI](/cli/#install-the-cli) |
| Install an upstream tool | [Install a tool with packslip](/docs/bootstrap/) |
| Try creating and verifying a manifest | [Getting started](/docs/getting-started/), an offline local walkthrough |
| Understand how the pieces connect | [How packslip fits a release](/docs/release-workflow/) |

## Publish software

Start by adding the packslip GitHub Action to your release job. The other
guides cover what a release needs beyond the action's defaults.

- [Publish with GitHub Actions](/docs/publishing/): sign and upload the
  bundle from your release job, with no signing key to manage.
- [Artifact configuration](/docs/describing-releases/): set the platforms,
  executable paths, and variants that file names do not say, with flags or
  `release.toml`, and look up every `release.toml` key.
- [Resources](/docs/resources/) and [Host requirements](/docs/host-requirements/):
  ship completions, man pages, skills, and SBOMs, and say what the host
  must provide.
- [Release recipes](/docs/recipes/): complete configurations for a Rust,
  Go, monorepo, or desktop release.
- [Manage release lists](/docs/release-lists/): withdraw versions, mark
  security fixes, and recommend a default release. A list is optional on
  GitHub and required for a project on its own domain.
- [Host releases on your own domain](/docs/self-hosting/): name a project
  after its download host and publish its releases and list there.

## Install software or build a consumer

- [Install a tool with packslip](/docs/bootstrap/): discover and verify a
  signed upstream release, install it for one user or the system, and keep
  trust across later installs. Package-manager bootstrapping is one use case.
- [packslip and mise](/docs/mise/): install mise using a pinned packslip
  bootstrapper, or use mise to manage packslip-backed tools and the resources
  for their active versions. Includes Docker examples.
- [Verify a release](/docs/verifying/): pin a signer (by repository, signer
  fingerprint, or public key), check downloaded files, and see what else a
  consumer must enforce.
- [Build an installer or mirror](/docs/installers/): find, verify, and
  select releases in your own tool, follow the consumer rules, and use the
  `packslip` crate as a library.
- [Distribution packaging](/docs/distributions/): install from the signed
  APT or RPM repository, or build the installer for a distribution package.
- [Compatibility and support](/docs/compatibility/): supported builds,
  verification evidence, and the limits of long-lived bootstrapper support.
- [Consumer rules](/release/v1/#consumer-rules): the complete contract a
  consumer implements. A successful `packslip verify` alone does not meet
  it.

## Reference {#reference}

- [CLI overview and reference](/cli/): installation, common tasks, and every
  command, argument, and flag.
- [Specification](/release/v1/): release statements, signing, discovery,
  version selection, and consumer requirements.
- [JSON schemas](/release/v1/#json-schemas): download schemas for release
  statements and release lists, and see what schema validation covers.
- [Contributing](https://github.com/jdx/packslip/blob/main/CONTRIBUTING.md):
  build the project and edit the documentation.

## Terms used in these docs

| Term | Meaning |
| --- | --- |
| Project | The name a consumer asks for: a host and optional path with no URL scheme, such as `github.com/owner/repo` or `mytool.example.com`. |
| Forge | A code host whose project names also say who is expected to sign: github.com or gitlab.com. A release of `github.com/owner/repo` is expected to be signed by a workflow of that repository. |
| Vendor | A project that builds and signs its own releases, as opposed to a repackager, such as a mirror, that signs a document for another vendor's files. |
| Consumer | Software that verifies or installs releases from their packslips, such as a package manager or a mirror. |
| Artifact | A release file the statement lists for a consumer to select and download, such as an archive, installer, bare executable, or source tarball. Files that ship for another purpose, such as an SBOM or a man page, are resources; a file that is neither, such as a checksum file, is left out of the statement. |
| Resource | An additional item, such as a completion script, man page, skill, or SBOM. |
| Statement | The JSON document containing digests and release metadata. |
| Bundle | A sigstore bundle: a signed statement and its verification material. A release's bundle is its packslip, or release manifest, published as `packslip.sigstore.json` (`packslip.<subpath>.sigstore.json` for one tool of a monorepo, with any `/` in the subpath written as `-`). A release list is a bundle too. |
| TOML manifest | The optional input to `packslip create --manifest`, usually `release.toml`. It is not signed or published. |
| Release list | A separate signed document that indexes releases and records mutable metadata such as withdrawals. |
| Stamping host | A third party, such as a registry or scanning service, that publishes signed release lists of the versions it has checked. A consumer that trusts stamping hosts selects only versions one of them lists, unless the user chose to trust the vendor alone for that project. |
| Keyless signing | Signing with a CI job's OIDC identity through sigstore. Fulcio issues a short-lived certificate naming the workflow, so there is no long-lived key to manage. |
| Transparency log | Rekor, the public log that records sigstore signatures. A key-signed release created with `--no-log` has no log entry, and consumers must accept that explicitly (`verify --allow-unlogged`). |
| Pin | What a consumer holds a project's releases to: an identity policy (issuer and identity or prefix), a public key, or a signer fingerprint. For a forge project, the consumer also remembers the repository ID of the releases it accepted. |
| Signer fingerprint | `ps1_` followed by 26 characters, derived from the forge's issuer and the repository ID, that names the repository a keyless project is signed from. `packslip pin` prints it, and `packslip verify --pin` checks a release against it. |
