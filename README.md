<p>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="static/logo-dark.svg">
    <img src="static/logo.svg" alt="packslip" width="300" height="78">
  </picture>
</p>

packslip is a signed release manifest for software distributed as archives,
installers, or executables. A release publishes one file,
`packslip.sigstore.json`, beside its artifacts. A consumer, such as an
installer, package manager, or mirror, verifies that file against the project's signer
and reads three things from it: which artifact suits the host, where the
executables are inside that artifact, and the digest the download must
match. The consumer does not have to guess from file names or trust a
checksum file served next to the binary.

This repository holds the [specification](docs/spec/packslip.md), the
`packslip` CLI that publishes, verifies, and installs releases, the Rust crate
for building consumers, and two
GitHub Actions: `jdx/packslip` publishes a packslip from a release job, and
`jdx/packslip/releases` builds a signed release list. The format is stable
at version 1; [Stability](https://packslip.dev/release/v1/#stability) says
what that fixes and what a revision may add.

## Start here

- **Install a tool:** [Install a tool with packslip](https://packslip.dev/docs/bootstrap/)
  discovers and verifies a signed release, installs its complete archive, and
  links the declared commands. It works for any supported tool that publishes
  a packslip, including mise.
- **Try it locally:** [Getting started](https://packslip.dev/docs/getting-started/)
  creates and verifies a sample release with a local key. It needs no CI
  account.
- **Publish releases:** [Publish with GitHub Actions](https://packslip.dev/docs/publishing/)
  adds signing to a release job.
  [Artifact configuration](https://packslip.dev/docs/describing-releases/)
  maps your files to platforms and executables.
  [Resources](https://packslip.dev/docs/resources/) covers completions, man
  pages, skills, and SBOMs, and
  [Host requirements](https://packslip.dev/docs/host-requirements/) covers
  what the host must provide.
  [Release recipes](https://packslip.dev/docs/recipes/) gives complete
  configurations for Rust, Go, monorepo, and desktop releases.
- **Withdraw, recommend, or self-host releases:**
  [Manage release lists](https://packslip.dev/docs/release-lists/) and
  [Host releases on your own domain](https://packslip.dev/docs/self-hosting/).
- **Verify or install releases:** [Verify a release](https://packslip.dev/docs/verifying/)
  checks a downloaded release with the CLI against a repository, signer
  fingerprint, or public key you trust.
  [Build an installer or mirror](https://packslip.dev/docs/installers/)
  walks through finding, verifying, and selecting a release in your own
  tool, and shows the Rust crate that implements the verification and
  selection steps.
  [packslip and mise](https://packslip.dev/docs/mise/) covers installing mise
  with packslip and using mise to manage tools with packslip releases, including
  their completions, man pages, and skills.

[How packslip fits a release](https://packslip.dev/docs/release-workflow/)
shows how the pieces connect, and the
[documentation index](https://packslip.dev/docs/) lists every guide. For
exact fields and rules, read the [specification](docs/spec/packslip.md),
[CLI reference](https://packslip.dev/cli/), or
[JSON schemas](https://packslip.dev/release/v1/#json-schemas).

## Install the CLI

With mise installed and activated:

```sh
mise use -g packslip
packslip version
```

For a standalone install on Linux or macOS, run
`curl -fsSL https://packslip.sh | sh`. On Windows, run
`irm https://packslip.sh/install.ps1 | iex` in PowerShell.
[Installation methods](https://packslip.dev/docs/getting-started/#install-packslip)
also cover distribution packages, pinned scripts, the container image, direct
downloads, and source builds. Release executables cover Linux and Windows on
x64 and ARM64, and macOS on ARM64; Intel Macs need a source build.

With packslip 1.5.1 or newer, install mise's latest stable release:

```sh
packslip install github.com/jdx/mise --pin ps1_nlhmwtfeufglxv5myvwvronk7a
```

The pin identifies mise's signing repository. packslip verifies the release
and download before installing; it runs no downloaded code. It reports the
command path and any PATH setup needed. See
[packslip and mise](https://packslip.dev/docs/mise/) for pinned-bootstrapper
and Docker examples, and the [CLI overview](https://packslip.dev/cli/) for
other tasks.

## Add it to a GitHub release

In a tag-triggered release job, after your workflow builds the artifacts
and uploads them to the GitHub release, add:

```yaml
permissions:
  contents: write     # Upload the bundle to the release.
  id-token: write     # Sign with the workflow's identity.
  attestations: write # Attest the matched files.

steps:
  # Build the archives and upload them to the release before this step.
  - uses: jdx/packslip@v1
    with:
      artifacts: dist/*.tar.xz dist/*.zip
      bin: mytool
```

The action attests the matched files, signs a release manifest of their
digests, platforms, and executable paths with the workflow's identity,
verifies it, and uploads
`packslip.sigstore.json` to the release. It does not upload the artifacts.
No long-lived signing key is needed.

This one-job form gives the action `contents: write`. To keep it away from
release write access,
[run it in a separate read-only job](https://packslip.dev/docs/publishing/#keep-the-action-away-from-release-write-access).
[Publish with GitHub Actions](https://packslip.dev/docs/publishing/) covers
every input, how `@v1` and `packslip-version` select the CLI, and how to
[build the CLI on the runner](https://packslip.dev/docs/publishing/#build-the-cli-on-the-runner)
when packslip publishes no release for it, such as x64 macOS. A project
hosted on its own domain also publishes a signed release list with
`jdx/packslip/releases`; see
[Host releases on your own domain](https://packslip.dev/docs/self-hosting/).

## Verify a release

packslip signs its own releases. To check one with the
[packslip CLI](https://packslip.dev/docs/getting-started/#install-packslip),
download a release's bundle and one of its archives. `packslip verify`
checks the bundle against the workflow that signs packslip releases, and
the archive against the digest in the bundle. This example checks the
Linux x64 archive of 1.5.1; `packslip show packslip.sigstore.json` lists
the others:

```sh
curl -fLO https://packslip.dev/v1.5.1/packslip.sigstore.json
curl -fLO https://packslip.dev/v1.5.1/packslip-v1.5.1-linux-x64.tar.xz
packslip verify packslip.sigstore.json \
  --identity-prefix https://github.com/jdx/packslip/.github/workflows/release.yml@ \
  --issuer https://token.actions.githubusercontent.com \
  --pin ps1_mcx64bcghek2t4vljgb3lho3ti \
  --artifact packslip-v1.5.1-linux-x64.tar.xz
```

The bundle's project is `packslip.dev`, a domain rather than a GitHub
repository name, so `verify` cannot work out the expected signer from it,
and the identity flags are required. `--identity-prefix` names the
repository that signs packslip releases by its current name. `--pin` adds
a check on top of the identity flags: its value is that repository's
signer fingerprint, derived from the repository's ID, so a release signed
from a different repository that later takes the name `jdx/packslip` is
refused. The fingerprint stays the same if the repository is renamed or
moves to another owner, but `--identity-prefix` would then need the new
URL. See
[Pin a signer with its fingerprint](https://packslip.dev/docs/verifying/#pin-a-signer-with-its-fingerprint).

## Agent skill

The packslip [skill](skills/packslip/SKILL.md) helps a coding agent
configure a release, declare resources, and diagnose verification failures.
packslip releases after 1.4.0 declare the skill for their own version, so
mise can link the copy that matches your installed CLI into your agent's
skills directory:

```sh
mise use packslip
mise skills sync --dir .agents/skills
```

`mise use packslip` resolves through the mise registry, which supplies the
signer pin for packslip's own releases, published as project
`packslip.dev`. The synced links point into your mise installs, so keep
them out of version control. Run the same
`mise skills sync --dir .agents/skills` command again after changing the
packslip version. To have mise do that after every `mise install` and
`mise use`, set
[`skills.dir`](https://mise.jdx.dev/configuration/settings.html#skills.dir)
to `.agents/skills` and turn on
[`skills.auto_sync`](https://mise.jdx.dev/configuration/settings.html#skills.auto_sync).
Without `skills.dir`, mise links into `.claude/skills`.

## Work on packslip

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, checks, and documentation
sources, and [RELEASING.md](RELEASING.md) for the maintainer release process.
Bug reports and format feedback belong in
[issues](https://github.com/jdx/packslip/issues).

Developed by [Jeff Dickey (@jdx)](https://github.com/jdx), author of
[mise](https://mise.jdx.dev), and
[Shunsuke Suzuki (@suzuki-shunsuke)](https://github.com/suzuki-shunsuke),
author of [aqua](https://aquaproj.github.io/).
MIT licensed; see [LICENSE](LICENSE).
