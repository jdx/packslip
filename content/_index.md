---
title: A signed manifest for every release
description: Publish the checksums, platforms, executables, and resources for your software in one signed release manifest.
---
## What a packslip does

Publish `packslip.sigstore.json` beside your release artifacts. It contains
the files' digests, platform metadata, executable paths, and links to
resources and build provenance. Consumers verify the manifest against a
trusted identity or key, then check the files they download.

A GitHub release job can sign with its workflow identity. Publishers
outside supported CI can use an Ed25519 key. Both produce an in-toto
statement in a sigstore bundle.

[Follow the release workflow](/docs/release-workflow/) to see how local
configuration becomes a signed bundle and how consumers find it.

## Inside a packslip

Here is the release statement for a small portable tool, shortened to
show its core fields. The bundle wraps this metadata with a signature;
the digest below is abbreviated.

{{< release-example >}}

`subject` records the file's digest. `artifacts` describes where to get
it, how to unpack it, and which executable it contains. The signed
`project` and `version` identify the release. Platform-specific releases
also name their OS, architecture, and libc.

[Create this example and read the field-by-field explanation](/docs/getting-started/#read-the-manifest),
or explore the [full specification](/release/v1/#the-release-statement).

## Choose your next step

| If you… | Start here |
| --- | --- |
| Want to try the format | [Create and verify a local sample](/docs/getting-started/) |
| Publish software | [Add packslip to your GitHub release job](/docs/publishing/) |
| Need to describe a complex release | [Configure artifacts](/docs/describing-releases/) |
| Download software or build an installer | [Understand verification and trust](/docs/verifying/) |
| Want a consumer example | [Use packslip with mise](/docs/mise/) |

## Describe the release once

A consumer can read the platform and executable paths from the signed
manifest instead of maintaining filename guesses for each vendor.
Resources can include completions, man pages, CLI specifications, agent
skills, SBOMs, and desktop files. Host requirements describe libraries
and commands the software needs.

The metadata travels with the release, so a publisher can change an
archive layout and describe the new layout in the same release.
Consumers decide which formats and resource kinds they support.

## Know what verification proves

A verified packslip authenticates a signer's statement about the release.
Checking an artifact against it establishes that the downloaded bytes
match the signed digest. A checksum downloaded beside a binary cannot
provide that independent signer check on its own.

Build provenance is separate evidence and must be verified separately.
A single manifest does not detect withdrawn releases or prevent rollback:
those checks require discovery metadata and consumer state. Signed
[release lists](/docs/release-lists/) provide expiry, sequence numbers,
withdrawals, and an optional recommended version.

## Read the format

The [specification](/release/v1/) defines the release and release-list
predicates, signing schemes, and consumer rules. The
[CLI reference](/cli/) documents the generator and verifier.

packslip is developed by [Jeff Dickey](https://github.com/jdx), author of
[mise](https://mise.jdx.dev) and [usage](https://usage.jdx.dev).
The format is stable at [version 1](/release/v1/#stability).
[Feedback](https://github.com/jdx/packslip/issues) is welcome.
