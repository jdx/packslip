---
title: GitHub Actions reference
description: Every input and output of the jdx/packslip actions, how they check the packslip CLI, and what runners need.
group: reference
weight: 80
---
# GitHub Actions reference

Use this reference to write or review a workflow that runs `jdx/packslip`
or `jdx/packslip/releases`. It gives each input's default and the release
that added it, and how the actions choose and check the packslip CLI. To add
the action step by step, follow [Publish with GitHub Actions](/docs/publishing/).

## Versions and pinning

The actions and the packslip CLI share one version number: each tag `vX.Y.Z`
releases both, even when only an action changed. Both actions take these
references; for the release list action, write `jdx/packslip/releases` in
place of `jdx/packslip`:

| Reference | What the workflow runs |
| --- | --- |
| `jdx/packslip@v1` | The latest 1.x release; `v1` moves to each release's commit once it is published. |
| `jdx/packslip@vX.Y.Z` | Release X.Y.Z, which runs CLI X.Y.Z by default. |
| `jdx/packslip@<full-commit-sha> # vX.Y.Z` | The same release, fixed to its commit. |

A breaking change gets a new major tag, such as `v2`; after it, workflows on
`@v1` stop receiving updates until they move to `@v2`.

GitHub only warns `Unexpected input(s)` about an input the pinned release does
not define, then runs the step without it. Check each input's Since column
against the release you pin.

### Pin the action to a commit

The action's code decides what your workflow signs. As hardening, pin a
release's full commit with the version in a comment, as
[Publish with GitHub Actions](/docs/publishing/) does for
`actions/upload-artifact`:

```yaml
- uses: jdx/packslip@<full-commit-sha> # vX.Y.Z
```

Release tags are annotated, so ask for the commit a tag points at:

```sh
git ls-remote https://github.com/jdx/packslip 'refs/tags/vX.Y.Z^{}'
```

Pinning the commit also pins the CLI download on every platform, with no
hashes in your workflow; see
[CLI version and download checks](#cli-version-and-download-checks).

## Publish action

`jdx/packslip` signs keylessly. Its job needs these permissions:

- `id-token: write`, always.
- `attestations: write` while `attest` is `true`, the default.
- `contents: write` while `upload` is `true`, the default.

It runs these steps:

1. Installs the packslip CLI; see
   [CLI version and download checks](#cli-version-and-download-checks).
2. Fetches the assets `download` matches from the release named by `tag`
   into a new temporary directory; `token` must be able to read it.
3. Collects those and the files `artifacts` matches, or fails with
   `no artifacts matched`.
4. Attests the collected files, unless `attest` is `link` or `false`; see
   [Build provenance](#build-provenance).
5. Runs `packslip create` to create and sign the statement.
6. Verifies the bundle against the workflow's repository and GitHub's issuer.
7. Uploads the bundle to the release named by `tag` unless `upload` is
   `false`.

### Publish action inputs

<!-- inputs: action.yml -->

### Publish action outputs

<!-- outputs: action.yml -->

### How inputs reach packslip create

The action always passes `--project`, `--version`, `--tag`, `--commit`,
`--url-base`, `--notes-url`, and `--source-repo` to `packslip create`, from
its inputs or their defaults. They override a release.toml given as
`manifest`. `--source-repo` is always the workflow's repository,
`https://github.com/<owner>/<repo>`. `commit` sets only `source.commit` in
the statement, not the signing identity or the build provenance.

Besides those seven flags, the inputs map this way:

| Input | What the action passes to `packslip create` |
| --- | --- |
| `artifacts`, `download`, `variants` | Each collected file as a positional `ARTIFACTS` argument: `PATH`, or `PATH@VARIANT` when a `variants` entry names it |
| `bin`, `formats` | `--bin`, `--format` for each entry |
| `resources`, `require`, `extensions` | `--resource`, `--require`, `--extension` for each line |
| `manifest`, `out` | `--manifest`, `--out` |
| `attest` set to `true` or `link` | `--provenance NAME=URL` for each collected file |

`upload`, `token`, and the `packslip-` inputs have no flag. A project that
runs `packslip create` in place of the action, such as one
that needs `--no-pin-workflow`, passes the same flags;
[`packslip create`](/cli/create/) documents each one.

### Build provenance

`attest: true`, the default, attests the collected files and links each
attestation from the packslip. `link` only links, and `false` does neither;
use `false` in a private or internal repository without GitHub Enterprise
Cloud. Each link is
`https://api.github.com/repos/<owner>/<repo>/attestations/sha256:<digest>`.

- Use `link` when your build jobs already attest each final file; GitHub
  serves provenance by digest, whichever job attested it. `link` does not
  check that an attestation exists.
- To attest and link an artifact that only release.toml lists, add it to
  `artifacts` too.
- A resource asset carries no provenance link. The action attests one only
  when `artifacts` or `download` also collected it.

Before you switch to `false` after releases that carried provenance, read
[Keep later releases acceptable](/docs/publishing/#keep-later-releases-acceptable).

## Release list action

`jdx/packslip/releases` turns the bundles a project has published, laid out
as `<dir>/<tag>/packslip.sigstore.json`, into a signed release list. See
[Host releases on your own domain](/docs/self-hosting/) for the workflow and
[Manage release lists](/docs/release-lists/) for what a list does. It signs
keylessly, so its job needs `id-token: write`. It runs these steps:

1. Installs the packslip CLI as the publish action does.
2. Verifies every bundle under one identity policy (`issuer` with `identity`
   or `identity-prefix`), refusing one whose source tag differs from its
   directory's name.
3. Builds and signs the list with `packslip releases`, which refuses a bundle
   for another project. A tag in `yank` or `security` stands for
   `<url-base>/<tag>/<bundle>`; a URL is used as given.
4. Verifies the written list under the same identity policy.

With no bundles in `dir`, the action fails with
`no packslip.sigstore.json under <dir>/<tag>/; the action does not build an empty list`.
To publish a list with no releases, follow
[Retire every entry](/docs/release-lists/#retire-every-entry).

### Release list action inputs

<!-- inputs: releases/action.yml -->

### Release list action outputs

<!-- outputs: releases/action.yml -->

## CLI version and download checks

By default, an action runs the CLI release with its own version, read from
its commit's `Cargo.toml`. `packslip-version` picks another release. The
action downloads `packslip-vX.Y.Z-<os>-<arch>.tar.xz`, or `.zip` on Windows,
for `linux`, `darwin`, or `windows` on `x64` or `arm64`. There is no
`darwin-x64` archive; see
[Build the CLI on the runner](/docs/publishing/#build-the-cli-on-the-runner).

Each release's action commit includes a digest map: the SHA-256 of the CLI
archive for every platform packslip ships. With the default version, the
action checks the downloaded archive against that map. It then verifies the
archive's build provenance from jdx/packslip with `gh attestation verify`,
and only then extracts and runs it. A `packslip-sha256` set alongside the
default version must agree with the map. An action commit without the map
stops with
`the action checkout has no release digest map; use a published action commit`
unless `packslip-version` or `packslip-path` is set.

`packslip-path` runs an executable the job already has (a path, or a name on
PATH) as `packslip` for the remaining steps. It downloads nothing and skips
both checks, so the job vouches for the binary. It takes precedence over
`packslip-version` and `packslip-sha256`, and the action warns when either is
also set.

### Override the CLI version

With `packslip-version` set, the action skips the digest map, with a warning,
and still verifies the archive's build provenance from jdx/packslip. To pin
that archive too, set `packslip-sha256`:

```yaml
- uses: jdx/packslip@<full-commit-sha> # vX.Y.Z
  with:
    packslip-version: 1.10.1
    packslip-sha256: <64 lowercase hex digits>
    artifacts: dist/*.tar.xz
    bin: mytool
```

Take the digest from the asset's metadata on GitHub:

```sh
gh release view v1.10.1 --repo jdx/packslip --json assets \
  --jq '.assets[] | select(.name == "packslip-v1.10.1-linux-x64.tar.xz") | .digest | ltrimstr("sha256:")'
```

Each platform's archive has its own digest, so a matrix that spans platforms
gives each runner its own.

## Runner requirements

GitHub-hosted Linux and Windows runners have every tool the actions use. Check
macOS and self-hosted runners for these:

| Tool | Used for |
| --- | --- |
| `bash` 4.4 or later | Every step; macOS's own `/bin/bash` 3.2 is too old. |
| GitHub CLI (`gh`) 2.68.0 or later | Installing and verifying packslip, downloading assets, and uploading the bundle. An older `gh` fails the default CLI's provenance check. |
| `tar` with xz support, or `unzip` on Windows | Unpacking the CLI. |
| `sha256sum` | Linking provenance by digest and checking the CLI download; `shasum` also works for the download. |
| `jq` | Reading the digest map, and in the releases action each bundle's tag. |
