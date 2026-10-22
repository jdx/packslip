---
title: Signed manifests for software releases
description: Publish signed release bundles, verify downloads, and install upstream tools with packslip.
lede: Describe your release once with signed digests, platforms, and executable paths. Installers can then select and verify the right download without guessing its layout. Your install instructions no longer need a curl | sh script.
---
## Add a packslip to your release job

Add the `jdx/packslip@v1` step to your release workflow after the steps that
build and upload your files. It publishes one signed file,
`packslip.sigstore.json`, beside your release artifacts. This release bundle,
or packslip, records each file's digest, platform, and executable paths, and
links to resources and build provenance. The action signs it with the
workflow's identity, so there is no signing key to create or store.

[Publish with GitHub Actions](/docs/publishing/) shows the job's permissions
and how to keep release write access away from the action.
[Publish from other CI](/docs/other-ci/) covers other CI systems and Ed25519
keys. [How packslip works](/docs/release-workflow/) follows a release from
local configuration to a signed bundle and shows how consumers find it.

## Replace your install script

An installer, package manager, or mirror verifies the packslip against a
signer it trusts, then checks each download against the signed digest. It
reads platforms and executable paths from the packslip instead of guessing
them from your file names. Because the metadata travels with the release, you
can change an archive's layout and describe it in the same release.

A checksum file beside the binary cannot prove the download is the one you
released: whoever replaces the binary can replace the checksum.
[How trust works](/docs/trust/) explains what verification establishes and
what it does not.

Set the action's `bin` input to your tool's commands. Users with packslip can
then replace your `curl | sh` line with:

```sh
packslip install github.com/owner/mytool
```

The command picks the build for the user's platform, checks that the release
was signed in the repository the name points to, and verifies the download.
It then exports only the commands your packslip declares into a command
directory, such as `~/.local/bin`. After that, the tool can update itself, or
the user runs `packslip install` again. It replaces an install script, not a
package manager.

[Publish your signer fingerprint](/docs/publishing/#publish-your-signer-fingerprint)
next to the install command. With `--pin`, as in the hk example below, users
accept only a release signed in your repository, even on a first install.
Without it, a first install trusts the repository GitHub reports for the name.

## Install a tool from its signed release

The packslip CLI installs a tool from its GitHub releases, or from its own
domain when that domain serves a signed release list. The tool's packslip
must list a build for your platform that declares its commands. Install the
CLI with [Install packslip](/docs/install/), then pass the tool's project name
to `packslip install`. For example, [hk](https://hk.jdx.dev) publishes signed
GitHub releases; `--pin` takes its signer fingerprint:

```sh
packslip install github.com/jdx/hk --pin ps1_snirenkjwr7m5ozgcufameodnm
```

[Bootstrap a tool](/docs/bootstrap/) covers install destinations,
replacement, trust, and the projects install accepts. To install mise this
way or let mise manage tools that publish a packslip, see
[Use packslip with mise](/docs/mise/).

## Inside a packslip

Here is a small portable tool's release statement, shortened to its core
fields with the digest abbreviated. The bundle wraps it with a signature.

{{< release-example >}}

`subject` records the file's digest. `artifacts` describes where to get the
file, how to unpack it, and which executable it contains. The signed
`project` and `version` identify the release. A platform-specific artifact
also carries whichever of `os`, `arch`, and `libc` it depends on; a
statically linked Linux executable has no `libc`.

[Try packslip locally](/docs/getting-started/) creates this example, and
[Read the signed statement](/docs/getting-started/#read-the-signed-statement)
explains each field. The specification defines
[the release statement](/release/v1/#the-release-statement).

## Who is using packslip

These upstream releases carry a packslip; each link downloads it:
[Helmfile 1.8.1](https://github.com/helmfile/helmfile/releases/download/v1.8.1/packslip.sigstore.json),
[Dagu 2.19.2](https://github.com/dagucloud/dagu/releases/download/v2.19.2/packslip.sigstore.json),
[Worktrunk 0.80.0](https://github.com/max-sixty/worktrunk/releases/download/v0.80.0/packslip.sigstore.json),
[Timoni 0.35.0](https://github.com/stefanprodan/timoni/releases/download/v0.35.0/packslip.sigstore.json),
[Flint 0.22.14](https://github.com/grafana/flint/releases/download/v0.22.14/packslip.sigstore.json), and
[mise 2026.10.9](https://github.com/jdx/mise/releases/download/v2026.10.9/packslip.sigstore.json).
[Who is using packslip](/docs/who-is-using-packslip/) gives a dated list
that separates upstream projects from repackagers and third-party builds.

## Choose your next step

| If you… | Start here |
| --- | --- |
| Publish from a GitHub release job | [Publish with GitHub Actions](/docs/publishing/) |
| Need to describe a complex release | [Describe your artifacts](/docs/describing-releases/) |
| Withdraw or recommend versions | [Manage release lists](/docs/release-lists/) |
| Serve releases from your own domain | [Host releases on your own domain](/docs/self-hosting/) |
| Want to try the format | [Try packslip locally](/docs/getting-started/) |
| Need the packslip command | [Install packslip](/docs/install/) |
| Want to install an upstream tool | [Bootstrap a tool](/docs/bootstrap/) |
| Use mise or want to bootstrap it | [Use packslip with mise](/docs/mise/) |
| Download software and check it | [Verify a release](/docs/verifying/) |
| Build an installer or use the Rust crate | [Build an installer or mirror](/docs/installers/) or [Use the Rust crate](/docs/rust/) |

[Documentation](/docs/) lists every guide.

## Reference and background

The [specification](/release/v1/) defines the release and release-list
predicates, signing schemes, and consumer rules. The [CLI reference](/cli/)
covers common tasks and every command, argument, and flag. The
[JSON schemas](/release/v1/#json-schemas) describe the decoded release and
release-list statements. The
[Introducing packslip](https://jdx.dev/posts/2026-09-05-introducing-packslip/)
announcement explains why the format exists and shows it in use with mise.

[Jeff Dickey](https://github.com/jdx), author of [mise](https://mise.jdx.dev),
and [Shunsuke Suzuki](https://github.com/suzuki-shunsuke), author of
[aqua](https://aquaproj.github.io/), develop packslip. The format is stable
at [version 1](/release/v1/#stability).
[Feedback](https://github.com/jdx/packslip/issues) is welcome in GitHub
issues.
