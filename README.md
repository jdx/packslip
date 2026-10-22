<p>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/jdx/packslip/HEAD/static/logo-dark.svg">
    <img src="https://raw.githubusercontent.com/jdx/packslip/HEAD/static/logo.svg" alt="packslip" width="300" height="78">
  </picture>
</p>

A packslip is a signed release manifest for software distributed as archives,
installers, or executables. A release publishes one file,
`packslip.sigstore.json` (the release bundle), beside its artifacts. An
installer, package manager, or mirror verifies the bundle against the
project's signer. It then reads which artifact suits the host, where the
executables are inside that artifact, and the digest the download must match.
It need not guess from file names or trust a checksum file served next to the
binary. The format is [stable at version 1](https://packslip.dev/release/v1/#stability).

## Publish a release

Add these permissions and this step to a tag-triggered release job, after the
workflow builds the artifacts and uploads them to the GitHub release:

```yaml
permissions:
  contents: write     # Upload the bundle to the release.
  id-token: write     # Sign with the workflow's identity.
  attestations: write # Attest the matched files.

steps:
  - uses: jdx/packslip@v1
    with:
      artifacts: dist/*.tar.xz dist/*.zip
      bin: mytool
```

The action attests the matched files and signs a statement of their digests,
platforms, and executable paths with the workflow's identity. It verifies the
bundle and uploads it to the release. It uploads no artifacts and needs no
long-lived signing key. Users can then install mytool with
`packslip install github.com/owner/mytool` instead of `curl | sh`.

**Warning:** this one-job form gives the action `contents: write`. To
[keep the action away from release write access](https://packslip.dev/docs/publishing/#keep-the-action-away-from-release-write-access),
run it in a job with `contents: read` and upload the bundle from a separate job.

For CI other than GitHub Actions, see
[Publish from other CI](https://packslip.dev/docs/other-ci/).

## Install a tool

`packslip install` replaces `curl | sh` with a verified install. It finds a
tool's signed release and checks it against the name you asked for. It
installs the complete archive and puts the declared commands in a bin
directory. It runs no downloaded code. After you
[install packslip](https://packslip.dev/docs/install/):

```sh
packslip install github.com/jdx/hk
```

It supports GitHub projects and domain projects, which are named after a host,
such as `mytool.example.com`. It has no upgrade, list, or uninstall command.
See [Bootstrap a tool](https://packslip.dev/docs/bootstrap/).

## Verify a release

When the bundle names a GitHub or GitLab project, `packslip verify` requires a
signer from that repository. It checks each `--artifact` against its digest in
the bundle:

```sh
curl -fLO https://github.com/jdx/hk/releases/download/v2.3.0/packslip.sigstore.json
curl -fLO https://github.com/jdx/hk/releases/download/v2.3.0/hk-x86_64-unknown-linux-gnu.tar.gz
packslip verify packslip.sigstore.json --artifact hk-x86_64-unknown-linux-gnu.tar.gz
```

The command trusts the project name the bundle claims, so check that the first
line of output begins `ok: github.com/jdx/hk 2.3.0`. To verify a domain
project, or to name the repository, signer fingerprint, or public key you
trust, see [Verify a release](https://packslip.dev/docs/verifying/).

## Documentation

The [documentation index](https://packslip.dev/docs/) lists every guide. Start
with one of these:

- [How packslip works](https://packslip.dev/docs/release-workflow/) and
  [How trust works](https://packslip.dev/docs/trust/)
- [Try packslip locally](https://packslip.dev/docs/getting-started/) with a
  local key and no CI account
- [Publish with GitHub Actions](https://packslip.dev/docs/publishing/),
  [Release recipes](https://packslip.dev/docs/recipes/), and
  [Manage release lists](https://packslip.dev/docs/release-lists/)
- [Verify a release](https://packslip.dev/docs/verifying/) and
  [Commit a verifier into your project](https://packslip.dev/docs/verify-script/)
- [Use packslip with mise](https://packslip.dev/docs/mise/)
- [Build an installer or mirror](https://packslip.dev/docs/installers/) and
  [Choose a library](https://packslip.dev/docs/libraries/)
- The [specification](https://packslip.dev/release/v1/),
  [CLI reference](https://packslip.dev/cli/),
  [GitHub Actions reference](https://packslip.dev/docs/actions/), and
  [JSON schemas](https://packslip.dev/release/v1/#json-schemas)
- The [agent skill](https://github.com/jdx/packslip/blob/main/skills/packslip/SKILL.md),
  which helps a coding agent configure a release, declare resources, and
  diagnose verification failures

## Contributing

This repository holds the
[specification source](https://github.com/jdx/packslip/blob/main/docs/spec/packslip.md),
the CLI that publishes, verifies, and installs releases, and the Rust crate for
building consumers. It also holds two GitHub Actions: `jdx/packslip` publishes
a packslip, and `jdx/packslip/releases` builds a signed release list.
[CONTRIBUTING.md](https://github.com/jdx/packslip/blob/main/CONTRIBUTING.md)
covers setup, checks, and documentation sources.
[RELEASING.md](https://github.com/jdx/packslip/blob/main/RELEASING.md) covers
the maintainer release process. Report bugs and format feedback in
[GitHub issues](https://github.com/jdx/packslip/issues). Report a vulnerability
privately as [SECURITY.md](https://github.com/jdx/packslip/blob/main/SECURITY.md)
describes.

## Credits

Developed by [Jeff Dickey (@jdx)](https://github.com/jdx), author of
[mise](https://mise.jdx.dev), and
[Shunsuke Suzuki (@suzuki-shunsuke)](https://github.com/suzuki-shunsuke),
author of [aqua](https://aquaproj.github.io/).

## License

MIT; see [LICENSE](https://github.com/jdx/packslip/blob/main/LICENSE).
