---
title: Publish with GitHub Actions
description: Add a signed packslip to an existing GitHub release workflow.
group: publish
weight: 30
---
# Publish with GitHub Actions

For maintainers who release from GitHub Actions: with the action in your
release workflow, every release carries a signed `packslip.sigstore.json`, and
your users get a signer fingerprint to pin. If you publish from another CI
system or sign with a key, use [Publish from other CI](/docs/other-ci/)
instead.

The `jdx/packslip` action describes the finished files in a statement, signs
it with the workflow's identity (no long-lived key), verifies it, and uploads
only the bundle; keep your build and upload steps. Run it in
[a separate signing job](#keep-the-action-away-from-release-write-access)
that cannot change the release (preferred), or as
[one step in your release job](#add-the-release-step-to-an-existing-job).

## Prepare the files and release

Before the packslip step runs, your workflow must have:

1. Built the final files, including rewritten archives, platform signing, and
   notarization. The action records each file's digest, so any later change
   to the bytes makes verification fail.
2. Created the GitHub release and uploaded the artifacts and any separate
   resource assets, such as an SBOM.
3. Kept the release a draft, if the repository uses immutable releases. A
   published immutable release's assets cannot change, so publish it after
   the bundle is uploaded.

The signing job gets the files from your own staging step, such as
`actions/download-artifact` after a build matrix, or with the `download` input
from the release; see [Download from the release](#download-from-the-release).

Some setups need more:

- On a macOS or self-hosted runner, check the
  [runner requirements](/docs/actions/#runner-requirements). On Intel macOS,
  also [build the CLI on the runner](#build-the-cli-on-the-runner).
- In a private or internal repository without GitHub Enterprise Cloud, set
  `attest: false` from the first release; artifact attestations there need
  that plan.

## Add the release step to an existing job

Add the step to your release job after the build and release steps. The job
needs `contents: write` because the action uploads the bundle (`upload`
defaults to `true`):

```yaml
permissions:
  contents: write      # Upload the bundle.
  id-token: write      # Sign with the workflow's identity.
  attestations: write  # Attest the collected files.

steps:
  # Your existing build and release steps go here.
  - uses: jdx/packslip@v1
    id: packslip
    with:
      artifacts: dist/*.tar.xz dist/*.zip
      bin: mytool
```

On a tag push, the defaults derive the project, version, download URLs, and
source commit from the repository and tag. This step finds `mytool` in each
archive, publishes build provenance, and attaches `packslip.sigstore.json` to
the release. If your tag is not a semver version, optionally after a `v`, or
the workflow does not run on it, [set the common inputs](#action-inputs).

To try the workflow without attaching a bundle, set `upload: false`. The run
still signs, recording its keyless signature in the public Rekor log, and the
default `attest: true` publishes provenance. For an offline trial with a
temporary key, use [Try packslip locally](/docs/getting-started/).

## Keep the action away from release write access

Run packslip in its own job after the job that publishes your release files,
with `contents: read` and `upload: false`; a second job with `contents: write`
attaches the bundle. GitHub grants token permissions per job, offers no token
limited to one release asset, and lets an action use its job's token even
when you do not pass it as an input. So only a separate job keeps write
access away from the action; `upload: false` only stops its own upload.

```yaml
jobs:
  packslip:
    needs: release # Your existing job that publishes the release files.
    runs-on: ubuntu-latest
    permissions:
      contents: read       # Download release assets; cannot change the release.
      id-token: write      # Sign with the workflow's identity.
      attestations: write  # Attest the downloaded files.
    steps:
      - uses: jdx/packslip@v1
        id: packslip
        with:
          download: mytool-*.tar.xz mytool-*.zip
          bin: mytool
          upload: false
      - uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1
        with:
          name: packslip-bundle
          path: ${{ steps.packslip.outputs.bundle }}
          if-no-files-found: error

  publish-packslip:
    needs: packslip
    runs-on: ubuntu-latest
    permissions:
      contents: write  # Upload the bundle to the release.
    steps:
      - uses: actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c # v8.0.1
        with:
          name: packslip-bundle
          path: packslip
      - name: Upload the signed bundle
        env:
          GH_TOKEN: ${{ github.token }}
        run: |
          set -euo pipefail
          test -f packslip/packslip.sigstore.json
          gh release upload "$GITHUB_REF_NAME" packslip/packslip.sigstore.json \
            --repo "$GITHUB_REPOSITORY" --clobber
```

Adapt the example to your workflow:

- It runs on a tag. For another trigger, pass the release tag as `tag` and
  use it in the upload command.
- It assumes the release is published. For a draft, stage the files with
  `actions/download-artifact` as `artifacts`, then upload the bundle and
  publish the release from the `contents: write` job; see
  [Download from the release](#download-from-the-release).
- For a monorepo tool, write the downloaded bundle's path, such as
  `packslip/packslip.mytool.sigstore.json`, in the `test -f` line and the
  upload command; the `bundle` output exists only in the `packslip` job.
- If build jobs already attested every file, set `attest: link` and drop
  `attestations: write`.
- To pass a release.toml as `manifest`, add `actions/checkout`.

**Warning:** The signing job can still sign as your workflow. To control
which code signs, pin the action to a release's full commit, as the example
pins the artifact actions; see
[Pin the action to a commit](/docs/actions/#pin-the-action-to-a-commit).

## Set the common inputs {#action-inputs}

The defaults fit one tool released on a tag such as `v1.2.3`. Set these as
needed:

| Input | Set it when |
| --- | --- |
| `artifacts` | The files are in the job's working directory. Give whitespace-separated paths or globs. |
| `download` | The files are on the GitHub release. Give asset name patterns. |
| `bin` | The artifacts contain commands. For each, give a name to find in each archive, a path from the archive root, or `NAME=PATH`. |
| `version` | The tag, without any leading `v`, is not semver. Give the version, such as `4.1.0` for `v4.1` or `1.2.3` for `mytool-v1.2.3`. |
| `tag` | The workflow does not run on the release tag. |
| `attest` | Build jobs already attested the files (`link`), or the repository cannot use attestations (`false`); see [Build provenance](/docs/actions/#build-provenance). |

The [version spelling rules](/release/v1/#spelling-a-version) say which
versions are accepted;
[Publish action inputs](/docs/actions/#publish-action-inputs) lists every
input and its default.

## Check the published result

The action verifies the bundle before uploading it. Also verify the published
files from the URLs users will use, to catch a wrong upload, a stale file, or
a URL pointing at a different build:

```sh
curl -fsSLO https://github.com/owner/mytool/releases/download/vX.Y.Z/packslip.sigstore.json
curl -fsSLO https://github.com/owner/mytool/releases/download/vX.Y.Z/mytool-X.Y.Z-linux-x64.tar.xz
packslip verify packslip.sigstore.json --artifact mytool-X.Y.Z-linux-x64.tar.xz
```

With no trust flags, `packslip verify` requires a workflow run of
`owner/mytool` to have signed the release; see
[Verify against the expected repository](/docs/verifying/#verify-against-the-expected-repository)
and [Understand the result](/docs/verifying/#understand-the-result).
`packslip show packslip.sigstore.json` prints the project, version, source
tag, platforms, and executable paths for review; it does not verify.

`packslip verify` does not fetch linked build provenance; check it
separately:

```sh
gh attestation verify mytool-X.Y.Z-linux-x64.tar.xz --repo owner/mytool
```

## Publish your signer fingerprint

After verifying the first published release, print the signer fingerprint
from the downloaded bundle:

```sh
packslip pin packslip.sigstore.json
```

For a forge project, `packslip pin` first verifies the bundle under the
policy the name implies, then prints `ps1_` and 26 characters; for hk,
`ps1_snirenkjwr7m5ozgcufameodnm`. Run it only on a release you trust;
[Pin a signer with its fingerprint](/docs/verifying/#pin-a-signer-with-its-fingerprint)
explains why.

Publish it once, for every release and every tool of a monorepo, where users
can read it independently of your releases, such as your README or install
instructions. Users then pass it with `--pin` instead of trusting the first
release they see:

```sh
packslip install github.com/jdx/hk --pin ps1_snirenkjwr7m5ozgcufameodnm
```

[First use and signer fingerprints](/docs/trust/#first-use-and-signer-fingerprints)
explains what it pins. A key-signed project publishes its public key
instead. For a domain project, see
[Publish the signer](/docs/self-hosting/#publish-the-signer).

## Keep later releases acceptable

Consumers hold each release to the signer they accepted before, so even a
harmless-looking workflow change can make them refuse it until a person
approves; see
[Signer continuity and workflow pinning](/docs/trust/#signer-continuity-and-workflow-pinning).
To avoid that:

- **Sign every release, backfills included, from one workflow file.** Moving
  the action to a separate job in the same file keeps the signer. A consumer
  that pins a ref, such as tags only, refuses a backfill dispatched from a
  branch; see
  [Publish a different source commit](#publish-a-different-source-commit).
- **Settle on a reusable workflow once.** A reusable workflow in your own
  repository signs as that one file for every workflow that calls it. Moving
  between your own workflow and a shared one changes the signer.
- **Keep provenance on every artifact once you start.** Do not switch to
  `attest: false`, and collect every artifact through `artifacts` or
  `download`; see
  [No-downgrade and approvals](/docs/trust/#no-downgrade-and-approvals).

If you must sign from several workflow files, such as stable and nightly,
declare `pin_workflow: false` from the first release with
`packslip create --no-pin-workflow`. Consumers treat the declaration
differently, and some still ask a person to approve a release from another
file; see
[Signer continuity and workflow pinning](/docs/trust/#signer-continuity-and-workflow-pinning).
No action input or release.toml key sets it: run
[`packslip create`](/cli/create/) in the signing job instead of the action,
with the flags in
[How inputs reach packslip create](/docs/actions/#how-inputs-reach-packslip-create),
and upload the bundle yourself.

[Workflow pinning](/release/v1/#workflow-pinning) and the
[consumer rules](/release/v1/#consumer-rules) define these checks.

## Download from the release

Set `download` to asset name patterns when the archives are already on the
GitHub release, as when an earlier job uploaded them or you re-sign a
release. It replaces a `gh release download` step and a matching `artifacts`
glob. Set both inputs to combine files on disk with files from the release.

- GitHub lists a draft only to tokens with push access, so a
  `contents: read` job stages a draft's files as `artifacts` instead.
- Every downloaded file is collected as an artifact, so keep the patterns
  from matching anything else, such as an earlier run's
  `packslip.sigstore.json`.

Downloaded files land in a temporary directory that no other input, and no
release.toml `[[artifact]]` path, can name. Describe them with the `bin`,
`variants`, and `formats` inputs. Fetch a file that another input or
release.toml must name, such as an `asset:` resource, to a known path first:

```yaml
- name: Fetch the SBOM
  env:
    GH_TOKEN: ${{ github.token }}
  run: gh release download "$GITHUB_REF_NAME" --repo "$GITHUB_REPOSITORY" --pattern mytool.cdx.json --dir dist
- uses: jdx/packslip@v1
  with:
    download: mytool-*.tar.xz mytool-*.zip
    bin: mytool
    resources: |
      sbom/cyclonedx=asset:dist/mytool.cdx.json
```

## Add resources and requirements

Declare what ships besides the executables with `resources`, and the commands
the executables need on PATH with `require`:

```yaml
- uses: jdx/packslip@v1
  with:
    artifacts: dist/*.tar.xz
    bin: mytool
    resources: |
      completion/zsh=archive:share/zsh/site-functions/_mytool
      man=archive:share/man/man1/mytool.1
      cli-spec/usage=exec:mytool usage
      sbom/cyclonedx=asset:dist/mytool.cdx.json
    require: |
      bin:java@17
```

[Ship resources](/docs/resources/) and
[Declare host requirements](/docs/host-requirements/) explain each value. The
action sets the source repository and commit, so a `repo:` resource needs no
extra input, and sets each file's URL from the release's download URL.

## Use release.toml with the action

When paths or requirements differ between artifacts, describe them in a
release.toml; see [Use release.toml](/docs/describing-releases/#use-a-toml-manifest).
Check out the repository, pass `manifest: release.toml`, and set `artifacts`
to the local files its `[[artifact]]` entries name:

```yaml
- uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7
- uses: jdx/packslip@v1
  with:
    manifest: release.toml
    artifacts: dist/*.tar.xz dist/*.zip
```

The action's inputs, defaults included, replace release.toml's `project`,
`version`, `url_base`, `notes_url`, and `[source]`
([How inputs reach packslip create](/docs/actions/#how-inputs-reach-packslip-create)),
so set the matching inputs (`project`, `version`, `tag`, `commit`,
`url-base`, `notes-url`) wherever the defaults are wrong.

## Release several tools from one repository

Run the action once per tool, with that tool's artifacts and project:

```yaml
- uses: jdx/packslip@v1
  with:
    project: github.com/owner/toolkit/mytool
    version: X.Y.Z
    tag: mytool-vX.Y.Z
    artifacts: dist/mytool-*.tar.xz
    bin: mytool
```

Without `project` and `version`, the defaults would sign for the repository's
project and turn `mytool-vX.Y.Z` into an invalid version. The step writes
`packslip.mytool.sigstore.json`, or `packslip.tools-mytool.sigstore.json` for
a nested subpath `tools/mytool`. Consumers match the signed `project` field,
not the file name, and a workflow of `owner/toolkit` still signs it.

## Publish a different source commit

`tag` selects the release, but `commit` still defaults to `github.sha`. If
you dispatch from a branch and check out a release tag, pass that tag's full
commit SHA:

```yaml
# After checking out the release tag and preparing its artifacts:
- name: Resolve the checked-out release commit
  id: source
  shell: bash
  run: echo "commit=$(git rev-parse HEAD)" >> "$GITHUB_OUTPUT"
- uses: jdx/packslip@v1
  with:
    tag: ${{ inputs.tag }}
    commit: ${{ steps.source.outputs.commit }}
    artifacts: dist/*.tar.xz dist/*.zip
    bin: mytool
    attest: link
```

- `attest: link` points at your build jobs' attestations; provenance made
  here would describe the dispatched run, not the build of the tag. If your
  build jobs made no attestations, use `attest: false`. Consumers refuse that
  release if earlier ones carried provenance; see
  [Keep later releases acceptable](#keep-later-releases-acceptable).
- The action does not check `commit` against the tag or the files, so take
  it from the checkout you built from. Consumers fetch `repo:` resources,
  such as a skill directory, from this commit.
- The signature names the workflow and the dispatch ref. Neither the policy
  the project name implies nor signer continuity checks the ref, so a
  dispatch from any branch passes. Keep any restriction your workflow places
  on which refs may publish. Consumers that pin the ref refuse the run:
  `packslip verify --identity` with a tag run's identity, and mise's
  `workflow` tool option, which accepts only tag runs unless it names
  another ref.

Workflows that already run on the release commit can omit `commit`.

## Publish outside GitHub releases

Set `url-base` when users download from your own host; each file's URL
becomes `url-base`, `/`, and the file name. Copy the artifacts, resource
assets, and bundle there yourself. The action still attaches the bundle to
the release named by `tag`. With no GitHub release, set `upload: false`, and
set `notes-url`, which otherwise points at the release page on GitHub.
[Host releases on your own domain](/docs/self-hosting/) covers forge and
domain projects and when to keep that upload.

## Build the CLI on the runner

The action downloads a packslip release archive for the runner's platform,
and Intel macOS has none. Build the CLI there with cargo and point
`packslip-path` at it (a path, or a name on PATH):

```yaml
# In the release job, after building the archives and creating the release:
- if: runner.os == 'macOS' && runner.arch == 'X64'
  run: cargo install packslip --version X.Y.Z --locked --root "$RUNNER_TEMP/packslip"
- uses: jdx/packslip@vX.Y.Z
  with:
    packslip-path: ${{ runner.os == 'macOS' && runner.arch == 'X64' && format('{0}/packslip/bin/packslip', runner.temp) || '' }}
    artifacts: dist/*.tar.xz
    bin: mytool
```

An empty `packslip-path` downloads the CLI as usual, so these steps also work
in a matrix where only some runners are Intel macOS (`macos-15-intel`).

- GitHub's macOS runner images include cargo. Elsewhere, install a Rust
  toolchain first.
- Build the version the action ref pins; the action and CLI are released
  together. `--locked` uses the dependency versions that release was tested
  with.
- `packslip-path` replaces only the CLI download. Signing still contacts
  Sigstore, and `download` and the upload still use the GitHub API.
- `packslip-path` also suits other platforms without a release archive,
  runners that already have packslip, and building from source.

**Warning:** The action does not check a `packslip-path` binary at all; the
job vouches for where it came from.

## Troubleshoot publication

| Error or symptom | What to do |
| --- | --- |
| `Unexpected input(s) '…'` | The step ignored that input. Fix its spelling, or pin an action release at or after its Since version in the [publish action inputs](/docs/actions/#publish-action-inputs). |
| `could not download packslip-vX.Y.Z-… from jdx/packslip vX.Y.Z` | If you set `packslip-version`, write a released version without a `v`, such as `1.10.1`. Otherwise check that the runner can reach github.com and `token` can read its public releases, then rerun. |
| `there is no macOS x64 release: build the CLI (…) and pass packslip-path`, or `Process completed with exit code 4.` from the Install packslip step on Intel macOS | [Build the CLI on the runner](#build-the-cli-on-the-runner). With the default CLI version, the step exits without a message. |
| `the action checkout has no release digest map; use a published action commit` | Pin the action to a release tag or a release's commit, or set `packslip-version` or `packslip-path`; see [CLI version and download checks](/docs/actions/#cli-version-and-download-checks). |
| `packslip-sha256 disagrees with the pinned action's release digest` | Remove `packslip-sha256`, or set it only with `packslip-version`. |
| `packslip archive SHA-256 mismatch: expected …, got …` | If you set `packslip-version`, give the digest of this runner's archive; each platform has its own. Otherwise the download does not match the action's digest: report it to jdx/packslip, and do not work around it with `packslip-version`. |
| `not running on a tag; pass the tag input` or `download is set but not running on a tag…` | The run is not on a tag, as on a branch push, a schedule, or a dispatch from a branch. Pass `tag`, and `commit` if the checked-out source is not `github.sha`. |
| `no artifacts matched: artifacts=[…] download=[…]` | Stage the files, such as matrix outputs, in this job's working directory, or match them with `download` on the release named by `tag`. Check the globs. |
| `version must be semver 2.0.0 (…), got "…"` | Pass `version` for tags such as `mytool-v1.2.3` or `v4.1`. |
| `warning: tag "…" does not name version …` | Consumers that list releases by tag will not see this one. Use a tag that names the version, such as `v1.2.3`, or name the release in a supplementary list; see [Publish a GitHub repository's list](/docs/release-lists/#publish-a-github-repositorys-list). |
| `…: no file named "mytool" inside the archive` or `…: several files named "mytool" inside the archive (…); give the path` | Check the archive contents, then give `bin` the file's path from the archive root, or a `NAME=PATH` mapping. |
| `missing "id-token" permission…` or `identity token: Token error: failed to detect ambient credentials…` | Give the job `id-token: write`; attesting and signing both need it. |
| `Failed to persist attestation: …` | Give the job `attestations: write`. Set `attest: link` if build jobs already attested the files, or `attest: false` in a private or internal repository without GitHub Enterprise Cloud. |
| `release not found` or `HTTP 403: Resource not accessible by integration (…)` | Check that the release named by `tag` exists. Uploading needs `contents: write`, and an immutable release must still be a draft. A `contents: read` job cannot download a draft's files; stage them as `artifacts`. With [no GitHub release](#publish-outside-github-releases), set `upload: false`. |
| `verification failed: artifact …: sha256 is …, document says …` | The published file is not the one the action signed. Compare it with the final local file and fix the upload; do not disable verification. |
| A resource URL returns a missing file | Upload that asset; a resource entry only describes it. |
