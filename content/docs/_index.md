---
title: Documentation
description: Publish signed releases, install tools, verify downloads, or build packslip into your own installer or mirror.
group: start
---
# Documentation

These guides are for publishers adding a packslip to an existing release job,
and for people installing tools from signed releases with the CLI. Developers
can also build packslip support into their own installers or mirrors with the
Rust crate or another library.

## Start

- [How packslip works](/docs/release-workflow/): see how the pieces connect,
  from local configuration to a signed bundle, discovery, and a verified
  installation.
- [Install packslip](/docs/install/): get the command on your machine,
  including from the signed APT or RPM repository.
- [Try packslip locally](/docs/getting-started/): verify a published release,
  then create, sign, and verify a sample release offline with a local key.
- [How trust works](/docs/trust/): what a verified release proves, which
  signer a project name implies, how consumers pin a signer and keep trusting
  it, and what each consumer enforces.
- [How packslip compares](/docs/comparison/): what install scripts, checksum
  files, Sigstore signatures, GitHub artifact attestations, SLSA provenance,
  and registry recipes establish, and what packslip adds to each.

## Publish

Start with the guide for what runs your release: GitHub Actions, another CI
system, or your own Go release tool. The other guides cover what a release
needs beyond the defaults.

- [Publish with GitHub Actions](/docs/publishing/): sign and upload the bundle
  from your release job, with no signing key to manage.
- [Publish from other CI](/docs/other-ci/): sign keylessly from GitLab CI, or
  with an Ed25519 key from any CI system, and publish the bundle beside your
  files.
- [Describe your artifacts](/docs/describing-releases/): set the platforms,
  executable paths, and variants that file names do not say, with flags or
  `release.toml`.
- [Ship resources](/docs/resources/): ship completions, man pages, CLI
  specifications, agent skills, SBOMs, and desktop files with a release.
- [Declare host requirements](/docs/host-requirements/): say which libraries,
  commands, and OS or glibc versions the host must provide.
- [Release recipes](/docs/recipes/): complete configurations for a Rust, Go,
  monorepo, or desktop release.
- [Manage release lists](/docs/release-lists/): withdraw releases, mark
  security fixes, recommend a default version, and list a domain project's
  releases.
- [Host releases on your own domain](/docs/self-hosting/): name a project
  after a domain you control, and publish its releases and signed release list
  from GitHub Actions.
- [Move a GitHub project to your domain](/docs/move-to-domain/): describe your
  releases again under a domain name, list them on your host, and move
  consumers from `github.com/owner/mytool` to `mytool.example.com`.
- [Publish from Go](/docs/go/): build and sign a release bundle from your own
  Go release tool with the `github.com/jdx/packslip/go` module.

## Install and verify

- [Bootstrap a tool](/docs/bootstrap/): replace `curl | sh` with
  `packslip install`, which discovers and verifies a signed upstream release,
  installs it for one user or the system, and keeps trust across later
  installs.
- [Verify a release](/docs/verifying/): check a release bundle and the files
  you downloaded against a project's repository, a signer fingerprint, or a
  public key, and see what that proves.
- [Commit a verifier into your project](/docs/verify-script/): check a keyless
  release bundle and the files you downloaded with one Node.js file you commit
  next to your build, with no dependencies.
- [Use packslip with mise](/docs/mise/): install mise with `packslip install`
  and mise's signer fingerprint, including in Docker, or use mise to manage
  tools that publish a packslip and the resources for their active versions.

## Build on packslip

- [Build an installer or mirror](/docs/installers/): find, verify, and select
  releases in your own tool, and follow the consumer rules.
- [Use the Rust crate](/docs/rust/): verify releases, select artifacts, hold
  projects to their signers, and create signed statements with the `packslip`
  crate.
- [Verify from JavaScript](/docs/verify-js/): verify a downloaded keyless
  release from Node.js with the `packslip` npm package.
- [Choose a library](/docs/libraries/): compare the Rust crate, the Go
  module, the npm package, and the committable verifier, and find the guide
  for the one that fits your code.
- [Package packslip for a distribution](/docs/distributions/): build a
  packslip package that installs and verifies tools without the publisher
  commands, from an offline source archive, for Debian, Ubuntu, Fedora, or
  another distribution.
- [Compatibility and support](/docs/compatibility/): supported platforms,
  verification evidence, what a pinned packslip binary keeps verifying, and
  what forces an update.
- [Consumer rules](/release/v1/#consumer-rules): the complete contract a
  consumer implements.

## Reference

- [GitHub Actions reference](/docs/actions/): every input and output of
  `jdx/packslip` and `jdx/packslip/releases`, with its default and the release
  that added it.
- [release.toml reference](/docs/release-toml/): every `release.toml` key, and
  how `packslip create` reads platforms and formats from file names.
- [Glossary](/docs/glossary/): the terms these docs use, such as statement,
  release bundle, and signer fingerprint.
- [Who is using packslip](/docs/who-is-using-packslip/): published signed
  release bundles from upstream projects, repackagers, and third-party builds.
- [CLI reference](/cli/): common tasks and every command, argument, and flag.
- [Specification](/release/v1/): release statements, signing, discovery,
  version selection, and consumer requirements.
- [JSON schemas](/release/v1/#json-schemas): download schemas for release
  statements and release lists, and see what schema validation covers.
- [Contributing](https://github.com/jdx/packslip/blob/main/CONTRIBUTING.md):
  build the project and edit the documentation.
