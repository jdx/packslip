<p>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="static/logo-dark.svg">
    <img src="static/logo.svg" alt="packslip" width="300" height="78">
  </picture>
</p>

packslip is a signed release manifest for software distributed as archives,
installers, or executables. Publish `packslip.sigstore.json` beside your
release files so consumers can verify their digests, select a platform,
and find executables, resources, and build provenance.

> **Experimental.** The format and tooling are still changing. Use packslip
> for testing until the specification is declared stable.

## Start here

[How packslip fits a release](https://packslip.dev/docs/release-workflow/)
explains the path from local build files to signed metadata and installation,
including which parts belong to the publisher, CLI, and consumer.

- **Try it locally:** [Getting started](https://packslip.dev/docs/getting-started/)
  walks through creating and verifying a manifest without a CI account.
- **Publish a release:** [GitHub Actions](https://packslip.dev/docs/publishing/)
  covers the release workflow, inputs, and monorepos.
- **Describe your files:** [Artifact configuration](https://packslip.dev/docs/describing-releases/)
  covers platforms, executables, TOML configuration, and host requirements.
- **Consume a release:** [Verification](https://packslip.dev/docs/verifying/)
  explains trust pins and the checks an installer must perform.
- **Host your own releases:** [Release lists](https://packslip.dev/docs/release-lists/)
  covers discovery, withdrawals, and the recommended version.

For exact fields and rules, read the [specification](docs/spec/packslip.md),
[CLI reference](https://packslip.dev/cli/), or
[JSON schemas](https://packslip.dev/docs/#reference).

## Add it to a GitHub release

Add this step to a tag-triggered release job after building the artifacts
and creating the GitHub release:

```yaml
permissions:
  contents: write
  id-token: write
  attestations: write

steps:
  # Build the archives and create the release before this step.
  - uses: jdx/packslip@v0
    with:
      artifacts: dist/*.tar.xz dist/*.zip
      bin: mytool
```

The action attests the matched files, hashes the artifacts, signs the
manifest with the workflow's identity, verifies the bundle, and uploads
it to the existing release. It does not upload your binaries. Consumers
can pin the repository's identity without managing a signing key.

The action and CLI share a version: `@v0` follows CLI 0.x releases, and
`@v0.2.0` pins both to 0.2.0. By default, the action installs the CLI
version from its own commit's `Cargo.toml`. Set `packslip-version` to
explicitly override that selection.

### Bring your own CLI

`packslip-path` runs an executable that is already on the runner — a
path, or a name to look up on PATH — instead of downloading a release
archive. macOS releases are arm64 only, so an x64 macOS job builds the
CLI first and points the action at it:

```yaml
runs-on: macos-15-intel
steps:
  - uses: dtolnay/rust-toolchain@stable
  - run: cargo install packslip --version 0.2.0 --locked --root "$RUNNER_TEMP/packslip"
  - uses: jdx/packslip@v0.2.0
    with:
      packslip-path: ${{ runner.temp }}/packslip/bin/packslip
      artifacts: dist/*.tar.xz
      bin: mytool
```

Install the version the action ref pins; the two are released together.
The same applies to any runner a release archive does not suit: a
platform packslip does not ship, a self-hosted or network-restricted
machine, or a job that would rather build from source than download.
`packslip-path` takes precedence over `packslip-version`, and a binary
the action did not download is not verified, so the job vouches for it.

## Work on packslip

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, checks, and documentation
sources, and [RELEASING.md](RELEASING.md) for the maintainer release process.
Feedback on the draft belongs in [issues](https://github.com/jdx/packslip/issues).

Created by [Jeff Dickey (@jdx)](https://github.com/jdx), author of
[mise](https://mise.jdx.dev) and [usage](https://usage.jdx.dev).
MIT licensed; see [LICENSE](LICENSE).
