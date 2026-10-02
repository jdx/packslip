---
title: Signed manifests for software releases
description: Publish the checksums, platforms, executables, and resources for your software in one signed release manifest.
lede: Publish the information installers need to select, download, and verify your release artifacts, including supported platforms, executable paths, and file digests.
---
## What a packslip does

A packslip is one signed file, `packslip.sigstore.json`, published beside
your release artifacts. It records each file's digest, platform, and
executable paths, and links to resources and build provenance. A
consumer, such as a package manager or mirror, verifies the packslip
against a signer it trusts, then checks each file it downloads against the
signed digest.

A release job on a CI system with an OIDC identity, such as GitHub Actions
or GitLab CI, signs keylessly with that identity. A vendor whose CI has no
such identity, or that wants a stable key for consumers to pin, signs with
an Ed25519 key from `packslip keygen`. Both produce an in-toto statement in
a sigstore bundle.

[How packslip fits a release](/docs/release-workflow/) follows local
configuration to a signed bundle and shows how consumers find it.

## Inside a packslip

Here is the release statement for a small portable tool, shortened to
show its core fields. The bundle wraps this metadata with a signature;
the digest below is abbreviated.

{{< release-example >}}

`subject` records the file's digest. `artifacts` describes where to get
it, how to unpack it, and which executable it contains. The signed
`project` and `version` identify the release. A platform-specific
artifact also carries whichever of `os`, `arch`, and `libc` it depends
on; a statically linked Linux executable, for example, has no `libc`.

[Create this example and read the field-by-field explanation](/docs/getting-started/#read-the-manifest),
or explore the [full specification](/release/v1/#the-release-statement).

## Use release metadata in installers

A consumer can read the platform and executable paths from the packslip
instead of guessing them from each vendor's file names. Resources can
include completions, man pages, CLI specifications, agent skills, SBOMs,
and desktop files. Host requirements describe libraries and commands the
software needs.

The metadata travels with the release, so a vendor can change an archive
layout and describe the new layout in the same release. Consumers decide
which formats and resource kinds they support.

## What verification proves

A verified packslip proves that a signer you trust made this statement
about the release. Checking an artifact against it proves the downloaded
bytes are the ones the signer described. A checksum file served next to
the binary cannot do that, because whoever can replace the binary can
replace the checksum too.

Linked build provenance is separate evidence, and `packslip verify` does
not fetch or check it, so verify it separately. A single packslip does not
detect withdrawn releases or prevent rollback: those checks require
discovery metadata and consumer state. Signed
[release lists](/docs/release-lists/) provide expiry, sequence numbers,
withdrawals, and an optional recommended version. See
[What a verified packslip proves](/release/v1/#what-a-verified-packslip-proves)
for the full statement.

## Choose your next step

| If you… | Start here |
| --- | --- |
| Want to try the format | [Getting started](/docs/getting-started/) |
| Publish from a GitHub release job | [Publish with GitHub Actions](/docs/publishing/) |
| Need to describe a complex release | [Artifact configuration](/docs/describing-releases/) |
| Withdraw or recommend versions | [Manage release lists](/docs/release-lists/) |
| Serve releases from your own domain | [Host releases on your own domain](/docs/self-hosting/) |
| Download software and check it | [Verify a release](/docs/verifying/) |
| Build an installer or use the Rust crate | [Build an installer or mirror](/docs/installers/) |
| Want a consumer example | [Use packslip with mise](/docs/mise/) |

The [documentation index](/docs/) lists every guide.

## Reference and background

The [specification](/release/v1/) defines the release and release-list
predicates, signing schemes, and consumer rules. The
[CLI reference](/cli/) documents every command, including `create`,
`verify`, `pin`, `releases`, and `keygen`. The
[Introducing packslip](https://jdx.dev/posts/2026-09-05-introducing-packslip/)
announcement explains why the format exists and shows it in use with mise.

packslip is developed by [Jeff Dickey](https://github.com/jdx), author of
[mise](https://mise.jdx.dev), and
[Shunsuke Suzuki](https://github.com/suzuki-shunsuke), author of
[aqua](https://aquaproj.github.io/).
The format is stable at [version 1](/release/v1/#stability).
[Feedback](https://github.com/jdx/packslip/issues) is welcome.
