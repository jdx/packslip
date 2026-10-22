---
title: How packslip compares
weight: 25
group: start
description: What install scripts, checksum files, Sigstore signatures, GitHub attestations, SLSA provenance, and registry recipes establish, and what packslip adds.
---
# How packslip compares

Publishers and installer authors can see what the release and install
tools they already use establish, and what a packslip adds. For what a
verified packslip itself establishes, read [How trust works](/docs/trust/).

## What a packslip adds

A packslip is the signed release bundle a vendor's release job makes. Its
statement records the project, the version, and each artifact's digest,
size, platform, format, variant, and executable paths. A consumer can
therefore select and install an artifact without a per-tool recipe. Two
more protections come from the consumer and from release lists:

- **Project binding.** An installer such as `packslip install` derives a
  GitHub project's expected signer from the requested name and remembers
  the repository's ID. See
  [What a project name says about the signer](/docs/trust/#what-a-project-name-says-about-the-signer)
  and [What each consumer enforces](/docs/trust/#what-each-consumer-enforces).
- **Release lists.** A signed release list adds withdrawals, security
  fixes, a recommended version, an expiry, and a sequence number that
  blocks rollback; see [Manage release lists](/docs/release-lists/).

## What each approach establishes

| Approach | Establishes | Does not establish |
| --- | --- | --- |
| Install script piped to a shell | The script came over TLS from whoever controls its host. | Who published the files it downloads, or what it does with them. |
| Checksum file | The download matches a digest served by the same host. | Who published the files. |
| Sigstore signature on each file | The identity the verifier names signed those bytes. | Which identity should sign this project, or which files make up a release. |
| GitHub artifact attestation | A run of the repository's workflow signed build provenance for this digest. | Which files make up the release, which suits the host, or whether it was withdrawn. |
| SLSA provenance | How an artifact was built, at the [SLSA build level](https://slsa.dev/spec/v1.0/levels) its builder meets, once verified against the builder's identity. | Which artifact to install, or which releases were withdrawn. |
| Registry recipe | What the registry's maintainers wrote. | That the vendor stands behind the entry. |
| packslip release bundle | The signer consumers expect for the project signed the version and each artifact's digest, size, and platform. | How an artifact was built, or whether it is safe; see [What a verified packslip proves](/release/v1/#what-a-verified-packslip-proves). |

## Install scripts

An install script (`curl -fsSL https://mytool.example.com/install.sh | sh`)
runs on arrival, and its own code decides what to download and check.
`packslip install` runs no downloaded code. It verifies the release
against the project name you give, checks the download's digest and size,
and puts only the declared commands on PATH; see
[Bootstrap a tool](/docs/bootstrap/). Like an install script, a keyless
domain project's first install trusts the project's host: it takes the
signer from the release list served there over HTTPS; see
[First use and signer fingerprints](/docs/trust/#first-use-and-signer-fingerprints).
The packslip CLI has to arrive first; see [Install packslip](/docs/install/).

## Checksum files

A checksum file such as `SHA256SUMS` catches a corrupted or truncated
download. It misses a binary replaced together with its checksum file on
the same host. Signing the checksum file adds a signer, but each project
picks its own scheme and key, which consumers must find and trust. A
packslip signs the digests under the signer consumers expect for the
project.

A checksum file is neither an artifact nor a resource, so it stays out of
the statement. A repackager relying on a vendor's checksum file records
`checksum-file-over-tls` evidence; see
[Repackager attestation](/release/v1/#repackager-attestation).

## Sigstore signatures

[cosign](https://github.com/sigstore/cosign) `sign-blob` signs one file,
and `cosign verify-blob` checks it against a certificate identity and OIDC
issuer the verifier supplies. A packslip uses the same Sigstore parts:
Fulcio certificates for keyless signing, the Rekor log, and the bundle
format, so general-purpose Sigstore tools can read it. Its one signature
covers a statement listing every file's digest, and consumers derive the
expected identity from a forge project's name.

## GitHub artifact attestations

`actions/attest-build-provenance` makes these attestations in the SLSA
build provenance format. `gh attestation verify` checks a file against
them:

```sh
gh attestation verify mytool-X.Y.Z-linux-x64.tar.gz --repo owner/mytool
```

The `jdx/packslip` action can attest the files it collects and link each
attestation from the artifact's `provenance` list. Files it downloaded are
attested under its own run, not the one that built them; see
[Build provenance](/docs/actions/#build-provenance).

`packslip verify` notes linked provenance without fetching it; its
`--json` report lists the links to check with `gh attestation verify`.
`packslip install` treats lost provenance as a trust change; see
[No-downgrade and approvals](/docs/trust/#no-downgrade-and-approvals).

## SLSA provenance

[SLSA build provenance](https://slsa.dev/spec/v1.0/provenance) is an
in-toto attestation of the builder, the source, and the build's
parameters. GitHub artifact attestations are one source of it. A
`provenance` link alone establishes no build level. Link another
builder's provenance with `packslip create --provenance`; see
[Set download URLs and provenance](/docs/describing-releases/#set-download-urls-and-provenance).

## Registry recipes

An [aqua registry](https://github.com/aquaproj/aqua-registry) entry tells
the installer how to build a tool's download URL per version and platform,
which format to expect, and where the executable is. It can also have the
installer check the vendor's checksum files and signatures. The
[mise registry](https://mise.jdx.dev/registry.html) maps short names to
backends such as aqua entries. Registry maintainers must update an entry
whenever the vendor renames files or changes an archive layout.

A packslip carries the same selection data, signed by the vendor in each
release. Registries use it in three ways:

- An entry shrinks to a name and its signers: the mise registry's entry
  for packslip names the domain project `packslip.dev` and the workflows
  that sign its releases and release list.
- A registry, mirror, or scanning service can act as a
  [stamping host](/docs/trust/#stamping-hosts) that signs release lists of
  the versions it has checked. mise can require its stamp; see
  [Require review by a third party](/docs/mise/#require-review-by-a-third-party).
- For a vendor that publishes no packslip, a registry can sign one as a
  repackager and record what it checked as evidence; see
  [Repackager attestation](/release/v1/#repackager-attestation).
