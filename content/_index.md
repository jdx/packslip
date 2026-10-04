---
title: Signed manifests for software releases
description: Publish signed software release manifests, verify downloads, and install upstream tools with packslip.
lede: Describe your release once with signed digests, platforms, and executable paths. Installers can then select and verify the right download without guessing its layout.
---
## What a packslip does

A packslip is one signed file, `packslip.sigstore.json`, published beside
your release artifacts. It records each file's digest, platform, and
executable paths, and links to resources and build provenance. A
consumer, such as an installer, package manager, or mirror, verifies the packslip
against a signer it trusts, then checks each file it downloads against the
signed digest.

A release job on a CI system with an OIDC identity, such as GitHub Actions
or GitLab CI, signs keylessly with that identity. A vendor whose CI has no
such identity, or that wants a stable key for consumers to pin, signs with
an Ed25519 key from `packslip keygen`. Both produce an in-toto statement in
a sigstore bundle.

[How packslip fits a release](/docs/release-workflow/) follows local
configuration to a signed bundle and shows how consumers find it.

## Install a tool from its signed release

The packslip CLI can install any supported tool that publishes a packslip.
For example, with [packslip 1.5.1 or newer installed](/cli/#install-the-cli):

```sh
packslip install github.com/jdx/mise --pin ps1_nlhmwtfeufglxv5myvwvronk7a
```

This installs mise's latest stable release. packslip verifies the signing
repository and the artifact's digest and size, preserves the complete archive,
and exposes only the declared commands. It runs no downloaded code. Add
`--version 2026.10.1` to choose an exact mise version.

[Install a tool with packslip](/docs/bootstrap/) covers destinations,
replacement, and trust. [packslip and mise](/docs/mise/) shows how to pin the
bootstrapper while mise follows upstream releases, including in Docker.

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
| Need the packslip command | [Install the CLI](/cli/#install-the-cli) |
| Want to install an upstream tool | [Install a tool with packslip](/docs/bootstrap/) |
| Want to try the format | [Getting started](/docs/getting-started/) |
| Publish from a GitHub release job | [Publish with GitHub Actions](/docs/publishing/) |
| Need to describe a complex release | [Artifact configuration](/docs/describing-releases/) |
| Withdraw or recommend versions | [Manage release lists](/docs/release-lists/) |
| Serve releases from your own domain | [Host releases on your own domain](/docs/self-hosting/) |
| Download software and check it | [Verify a release](/docs/verifying/) |
| Build an installer or use the Rust crate | [Build an installer or mirror](/docs/installers/) |
| Use mise or want to bootstrap it | [packslip and mise](/docs/mise/) |

The [documentation index](/docs/) lists every guide.

## Reference and background

The [specification](/release/v1/) defines the release and release-list
predicates, signing schemes, and consumer rules. The
[CLI overview](/cli/) covers installation and common tasks, followed by the
command reference. The [JSON schemas](/release/v1/#json-schemas) describe the
decoded release and release-list statements. The
[Introducing packslip](https://jdx.dev/posts/2026-09-05-introducing-packslip/)
announcement explains why the format exists and shows it in use with mise.

packslip is developed by [Jeff Dickey](https://github.com/jdx), author of
[mise](https://mise.jdx.dev), and
[Shunsuke Suzuki](https://github.com/suzuki-shunsuke), author of
[aqua](https://aquaproj.github.io/).
The format is stable at [version 1](/release/v1/#stability).
[Feedback](https://github.com/jdx/packslip/issues) is welcome.
