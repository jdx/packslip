---
title: Publish with GitHub Actions
weight: 20
group: publish
description: Add a signed packslip to an existing GitHub release workflow.
---
# Publish with GitHub Actions

Add `jdx/packslip` to the workflow that builds your release. The action
describes the finished files in a release manifest, signs it with the
workflow's identity, verifies the result, and uploads the bundle. You do
not need a long-lived signing key.

Choose the job layout that fits your workflow:

| Layout | When to use it |
| --- | --- |
| [One step in your release job](#add-the-release-step-to-an-existing-job) | The action may share the job's `contents: write` permission and upload the bundle itself. |
| [A separate signing job](#keep-the-action-away-from-release-write-access) | The action should have only `contents: read`; your own upload job attaches its output to the release. |

The action adds metadata to your release process. Keep your existing build
and artifact-upload steps. For publishing without GitHub Actions, start
with [Getting started](/docs/getting-started/) and
[Artifact configuration](/docs/describing-releases/).

## Prepare the files and release

Finish building, rewriting archives, platform signing, and notarization
before describing the files. The action records the digest of each final
file, so later changes to its bytes fail verification.

Create the GitHub release and upload the artifacts through your existing
workflow. Upload separate resource assets, such as SBOMs, too: the action
uploads only the packslip bundle. Listing a file's URL does not publish
that file.

If the repository uses immutable releases, create the release as a draft.
Publish it after uploading the bundle, because a published immutable
release's assets cannot change.

Once those files are on the release, either bring them into the signing
job yourself (a build matrix typically stages them with
`actions/download-artifact` before the packslip step) or let `download`
fetch them straight from the release; see
[Download from the release](#download-from-the-release). A job with only
`contents: read` cannot see a draft release, so a read-only job that signs
a draft must use the first way.

## Add the release step to an existing job

Use this single-job form when the action may hold `contents: write`, which
it needs to upload the bundle itself (`upload` defaults to `true`). Add it
to your release job after the steps that build the files and create the
release:

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

The default action inputs derive the project, version, download URLs, and
source commit from this workflow's repository and tag. This example finds
`mytool` in each archive, publishes provenance, and attaches
`packslip.sigstore.json` to the existing release.

To try the workflow without attaching a bundle, set `upload: false`. The
run still signs: its keyless signature is recorded in the public Rekor
log, and the default `attest: true` publishes provenance. For an offline
trial with a temporary key, use [Getting started](/docs/getting-started/).

## Keep the action away from release write access

Run packslip in its own job after the job that publishes your release
files. Give it `contents: read` to download assets, `id-token: write` to
sign, and `attestations: write` to publish provenance. Set `upload: false`
and pass its bundle to your own job with `contents: write`.

GitHub grants token permissions per job, not per step. An action can use
its job's token even when you do not pass it as an input, so the separate
job is what limits release write access. `upload: false` controls the
action's upload behavior.

```yaml
jobs:
  packslip:
    needs: release # Your existing job that publishes the release files.
    runs-on: ubuntu-latest
    permissions:
      contents: read       # Download the release assets.
      id-token: write      # Sign with the workflow's identity.
      attestations: write  # Attest the downloaded files.
    steps:
      - uses: jdx/packslip@v1
        id: packslip
        with:
          download: mytool-*.tar.xz mytool-*.zip
          bin: mytool
          upload: false
      - uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7
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
      - uses: actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c # v8
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

- It runs on a tag. If another event triggers the workflow, use its
  release tag in both the action's `tag` input and the upload command.
- It assumes your release job has already published the release. A
  draft, such as an immutable release before its bundle is attached, is
  invisible to this job's token; see
  [Download from the release](#download-from-the-release) for that layout.
- For a monorepo project, the bundle is named after the tool, such as
  `packslip/packslip.mytool.sigstore.json` (the action's `bundle` output).
  Use that path in both the `test -f` line and the upload command.
- If build jobs already attested every file, set `attest: link` and omit
  `attestations: write`; see [Build provenance](#build-provenance).
- The job does not check out the repository. Add `actions/checkout` if you
  pass a `manifest`.

GitHub has no token limited to one release asset, so the upload step that
holds `contents: write` belongs to your workflow, not to the packslip
action.

The packslip job can still sign as your workflow, because signing is what
it is for. To control which code can do that, pin the action to a release
commit (`jdx/packslip@<full SHA> # v1.4.0`), as the example does for the
artifact actions. The action then installs the CLI version from that
commit's `Cargo.toml`.

## Download from the release

When the archives are already on the GitHub release, because an earlier
job uploaded them or you are signing an existing release again, let the
action fetch them instead of staging them yourself:

```yaml
- uses: jdx/packslip@v1
  with:
    download: mytool-*.tar.xz mytool-*.zip
    bin: mytool
```

`download` replaces a `gh release download` step and the `artifacts` glob
that would repeat its patterns. It fetches the matching assets from the
release named by `tag` (the triggering tag by default) into a new
temporary directory and adds them to the files `artifacts` collects. Set
both inputs to combine files already on disk with files pulled from the
release. Keep the patterns from matching anything that is not an
artifact, such as a `packslip.sigstore.json` from an earlier run, because
every downloaded file is collected as an artifact.

Other inputs cannot refer to the downloaded files by path, because they
land in a temporary directory. An `asset:` resource needs a local file, so
fetch it to a known path first:

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

A TOML manifest given as `manifest` needs a checkout of the repository.
Its `[[artifact]]` paths are read from the working directory, so its
entries cannot describe files that `download` fetched. Fetch those files
to known paths yourself, or describe them with `bin`, `variants`, and
`formats`.

`token` must be able to read that release. GitHub lists draft releases
only to tokens with push access, so with `contents: read` the default
`github.token` can read a published release but not a draft. If your
release stays a draft until the bundle is attached, have the read-only job
collect the files with `actions/download-artifact` and `artifacts` instead
of `download`. Then upload the bundle and publish the release from the job
that has `contents: write`.

## Check the published result

The action verifies the local bundle before uploading it. Check the published
release separately: download the bundle and an artifact from the URLs users
will use, then verify both against the workflow identity of your repository,
as [Verify a release](/docs/verifying/#verify-against-the-expected-repository)
describes. This also catches a wrong upload, stale file, or URL that points
at a different build.

```sh
curl -fsSLO https://github.com/owner/repo/releases/download/v1.2.3/packslip.sigstore.json
curl -fsSLO https://github.com/owner/repo/releases/download/v1.2.3/mytool-1.2.3-linux-x64.tar.xz
packslip verify packslip.sigstore.json \
  --identity-prefix https://github.com/owner/repo/ \
  --issuer https://token.actions.githubusercontent.com \
  --artifact mytool-1.2.3-linux-x64.tar.xz
```

Inspect the statement with `packslip show` to confirm the project, normalized
version, source tag, platforms, and executable paths. Inspection does not
replace verification. `packslip verify` does not fetch linked build
provenance. If the release links provenance, check it separately:

```sh
gh attestation verify mytool-1.2.3-linux-x64.tar.xz --repo owner/repo
```

## Publish your signer fingerprint

Once you have verified the first published release of a project named
after its GitHub repository, such as `github.com/owner/repo`, print its
signer fingerprint from the bundle you downloaded:

```sh
packslip pin packslip.sigstore.json
```

For that name, `packslip pin` first verifies the bundle as
`packslip verify` does, under the identity policy the name implies (a
workflow of that repository, through GitHub's issuer), and then prints a
`ps1_…` value. Publish it where users can read it independently of your
releases, such as your README or install instructions. With it,
`packslip verify --pin` refuses a release signed from any other
repository, including one that later takes over your repository's name.

The fingerprint stays the same when the repository is renamed or
transferred and when the workflow changes, and one fingerprint covers every
tool of a monorepo, so you publish it once. A key-signed project has none;
publish its public key instead. See
[Pin a signer with its fingerprint](/docs/verifying/#pin-a-signer-with-its-fingerprint).

## Keep later releases acceptable

Consumers remember who signed the releases they accepted and hold the next
release to the same standard, so a workflow change that looks harmless can
make them refuse it.

- **Sign from one workflow file of the project's repository.** Consumers
  compare the path of the signing workflow, such as
  `.github/workflows/release.yml`, not its ref. A new tag, a manual
  dispatch of the same file, or a separate packslip job in that file is the
  same signer. A bundle signed from another file, such as a backfill
  workflow, looks like a changed signer, and consumers refuse it until a
  person approves it. A reusable workflow in your own repository is fine:
  every job that calls it signs as that one file. If the action runs inside
  a reusable workflow hosted in another repository, the certificate
  identity names that repository, so the action's own verification fails.
  Consumers, which accept only your repository's workflows, would refuse
  the bundle anyway.
- **Keep provenance.** Consumers refuse a release that drops the
  per-artifact provenance the previous release carried. Once you publish
  with `attest: true` or `link`, do not switch to `attest: false`, and
  collect every artifact through `artifacts` or `download`, not only
  through the TOML manifest, so each one gets a link.
- **Opt out of workflow pinning if you must sign from several files.**
  `packslip create --no-pin-workflow` asks consumers to hold your releases
  to the repository rather than to one workflow file. Neither the action's
  inputs nor a TOML manifest can set it, so a project that needs it runs
  [`packslip create`](/cli/create/) itself in the signing job, in place of
  the action. Consumers that already accepted your releases refuse the
  first release that declares it until a person approves it. Consumers
  written before the field existed ignore it, so they still refuse a
  release from a different workflow file until a person approves it.
  `packslip create` refuses the flag with `--key`. See
  [Workflow pinning](/release/v1/#workflow-pinning).

The [consumer rules](/release/v1/#consumer-rules) define these checks.

## Add resources and requirements

Declare what ships besides the executables with `resources`, and commands
the executables need with `require`. See [Resources](/docs/resources/) and
[Host requirements](/docs/host-requirements/) for the values.

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

Use paths from the actual archive root, including any top-level directory.
Only include resources and commands your release really provides or needs.
An `asset:` path names a local file in this job. A file that `artifacts`
also matches is recorded as the asset, not as an installable artifact.
When paths or requirements differ between artifacts, use a TOML manifest;
see [Artifact configuration](/docs/describing-releases/#use-a-toml-manifest).

## Release several tools from one repository

Run the action once per tool, selecting only that tool's artifacts:

```yaml
- uses: jdx/packslip@v1
  with:
    project: github.com/owner/repo/mytool
    version: 1.2.3
    tag: mytool-v1.2.3
    artifacts: dist/mytool-*.tar.xz
    bin: mytool
```

This writes `packslip.mytool.sigstore.json`. Nested subpaths use hyphens
in the file name: `tools/mytool` becomes `packslip.tools-mytool.sigstore.json`.
The signer is still pinned to the repository. Consumers match the signed
`project` field, not the bundle's file name.

## Publish a different source commit

`tag` selects the release, but does not change the default source commit:
`commit` defaults to the workflow's `github.sha`. If you dispatch from a branch
and check out a different release tag, pass that tag's full commit SHA explicitly:

```yaml
# After checking out the intended release tag and preparing its artifacts:
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

The example uses `attest: link` because provenance made in this run would
describe the dispatched run and its commit, not the build of the tag. It
links to the attestations your build jobs made for these files; see
[Build provenance](#build-provenance). If they made none, use
`attest: false`; consumers then refuse this release if the previous one
carried provenance, as
[Keep later releases acceptable](#keep-later-releases-acceptable) explains.

`commit` sets only `source.commit` in the release manifest. The action does
not check that it matches the tag or the files, so take it from the
checkout you built from, as above. Consumers fetch `repo:` resources, such
as a skill directory, from this commit, so a wrong value serves the wrong
files.

The signature still names the workflow and the ref you dispatched from.
Consumers compare the workflow's path, not its ref, so this run is the
same signer as a tag-triggered run of the same file, and a dispatch from
any branch is accepted too. If your workflow restricts which refs may
publish, keep that restriction, because consumers do not check the ref.
Workflows that already run on the release commit can omit `commit`.

## Publish outside GitHub releases

Set `url-base` when users download the files from your own host. The
release manifest gives each file's URL as `url-base` followed by `/` and
the file name. The action uploads nothing there, so copy the artifacts,
resource assets, and bundle to the host yourself. The action still
attaches the bundle to the GitHub release. Keep that for a project named
`github.com/owner/repo`, which consumers find through its GitHub releases.
A project named after the host instead publishes a signed release list
there and can set `upload: false`.
[Host releases on your own domain](/docs/self-hosting/) covers both.

## Build the CLI on the runner

The action downloads the release archive for the runner's platform. Where
that archive does not exist or cannot be used, `packslip-path` points the
action at an executable the job already has: a path, or a name to look up
on PATH. The action runs it as `packslip` for the rest of the steps and
skips the download.

macOS releases are arm64 only, so an x64 macOS job builds the CLI with
cargo first:

```yaml
jobs:
  release:
    runs-on: macos-15-intel
    permissions:
      contents: write      # Upload the bundle.
      id-token: write      # Sign with the workflow's identity.
      attestations: write  # Attest the collected files.
    steps:
      # Build the archives and create the release before these steps.
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo install packslip --version 1.4.0 --locked --root "$RUNNER_TEMP/packslip"
      - uses: jdx/packslip@v1.4.0
        with:
          packslip-path: ${{ runner.temp }}/packslip/bin/packslip
          artifacts: dist/*.tar.xz
          bin: mytool
```

Build the CLI for the runner's own platform, because the action runs the
binary on that runner. Install the version the action ref pins, since
the action and CLI are released together, and use `--locked` so the build
uses the dependency versions that release was tested with. `packslip-path`
also suits a platform packslip does not ship, a self-hosted runner that
already has packslip installed, and a job that prefers building from
source. It replaces only the CLI download: signing still contacts
Sigstore, and `download` and the upload still use the GitHub API.

A matrix that needs this on only some runners can leave the input empty
elsewhere; an empty `packslip-path` downloads as usual:

```yaml
packslip-path: ${{ runner.os == 'macOS' && runner.arch == 'X64' && format('{0}/packslip/bin/packslip', runner.temp) || '' }}
```

`packslip-path` takes precedence over `packslip-version`, which the action
warns about when both are set. A downloaded archive is checked against
jdx/packslip's build provenance before it runs; a binary supplied this way
is not checked at all, so the job vouches for where it came from.

The generated action commit pins the downloaded archive's SHA-256 internally.
Each Packslip release first publishes its final CLI archives, then creates an
immutable `action-vX.Y.Z` tag whose action source contains the five supported
platform archive digests. The moving `v1` tag advances to that action commit
only after the release assets are checked. Pin the generated action commit (or
`action-vX.Y.Z`) to keep that expected digest in the source you trust.

`packslip-sha256` is for a strict explicit `packslip-version` override. An
override otherwise retains the older build-provenance check and emits a
warning, because an action commit can internally lock only the release it was
prepared for:

```yaml
- uses: jdx/packslip@<full-action-lock-commit-sha> # action-v1.5.1
  with:
    packslip-version: 1.5.1
    packslip-sha256: d401822be0f4c0494dde170657499fc197709b530e18b547906dafe05c155d45
    artifacts: dist/*.tar.xz
    bin: mytool
```

The digest is checked after download and before provenance verification or
extraction. It applies only to the action-downloaded archive; when
`packslip-path` is set, the workflow supplies the executable and neither an
internal archive lock nor this input can verify it.

## What the action does

The action installs the packslip CLI released with it, fetches any
`download` assets, attests the collected files (see
[Build provenance](#build-provenance)), creates and signs the release
manifest, verifies the bundle, and uploads it to the release unless
`upload` is `false`. Its `bundle` output is the local bundle path.

Run it on a release tag, or pass the tag as `tag`. By default `project` is
`github.com/<owner>/<repo>` and `version` is the tag without a leading `v`.
The action does not normalize other tag spellings, so for a tag such as
`mytool-v1.2.3`, pass `version: 1.2.3`.

The action and CLI share a version: `@v1` follows CLI 1.x releases through a
post-build action-lock commit, while `action-vX.Y.Z` names the immutable action
commit that locks that CLI release's platform archives. By default, the action
installs the CLI version from its own commit's `Cargo.toml` and verifies the
matching internally pinned archive digest. Set `packslip-version` to override
that selection; set `packslip-sha256` too when the override needs an archive
digest check. `packslip-path` runs a CLI the job already has instead of downloading one; see
[Build the CLI on the runner](#build-the-cli-on-the-runner).

When a step passes an input that the pinned release does not define,
GitHub only warns, and the step runs without that input. Check that the
release you pin has every input you use: `download` needs 1.0.1 or later
and `commit` 1.3.0 or later.

GitHub-hosted Linux and Windows runners provide the required tools. For
macOS or self-hosted runners, check these before adding the action:

| Tool | Used for |
| --- | --- |
| bash 4 or later | Running the action's steps; macOS's own `/bin/bash` 3.2 is too old. |
| GitHub CLI (`gh`) with `gh attestation verify` | Installing and verifying packslip, downloading assets, and uploading the bundle. |
| `tar` with xz support, or `unzip` on Windows | Unpacking the packslip CLI. |
| `sha256sum` | Linking build provenance by digest. |

The `jdx/packslip/releases` action also needs `jq`; it publishes the lists
used for [Host releases on your own domain](/docs/self-hosting/).

## Action inputs

| Input | Purpose and default |
| --- | --- |
| `artifacts` | Whitespace-separated local files or globs. Between this and `download`, at least one file must match. |
| `download` | Whitespace-separated release asset name patterns to fetch before collecting artifacts; joins `artifacts`. See [Download from the release](#download-from-the-release). |
| `bin` | Whitespace-separated executables: a path inside the archive, a plain name to look up in each archive, or `NAME=PATH`. For a bare executable, the name it gets on PATH. |
| `project` | Project name; defaults to `github.com/<owner>/<repo>`. A host such as `mytool.example.com` names a project on its own domain; see [Host releases on your own domain](/docs/self-hosting/). |
| `version` | Semver version; defaults to the tag without its leading `v`. |
| `tag` | Existing release tag; defaults to the triggering tag and is required otherwise. |
| `commit` | Source commit SHA; defaults to `github.sha`. Override when the release source differs from the workflow commit. |
| `manifest` | Path to a TOML manifest for per-artifact paths, platforms, OS or glibc minimums, and artifact-scoped resources. Its artifacts join the collected files. Check out the repository so the file is present. |
| `variants` | Whitespace-separated `FILENAME=VARIANT` entries, for artifacts that share a platform. |
| `formats` | Whitespace-separated `FILENAME=FORMAT` entries, for an artifact whose name does not say what it is. |
| `resources` | One resource declaration per line. Add `@os[/arch[/libc]]` after the kind and any qualifier (`man@linux=…`) to scope one to a platform. |
| `require` | One `bin:NAME[@MIN]` requirement per line. |
| `extensions` | One `NAME=JSON` extension per line. |
| `url-base` | Artifact download prefix; defaults to the release's download URL. Set it when the files are served from elsewhere. |
| `notes-url` | Release-notes URL; defaults to the release page. |
| `attest` | Defaults to `true`. Use `link` when the build jobs already attested the files, or `false` for neither. See [Build provenance](#build-provenance). |
| `out` | Bundle output directory; defaults to `packslip`. |
| `upload` | Defaults to `true`, which needs `contents: write`. Set `false` to keep the bundle local, for example to upload it from a separate job. |
| `packslip-version` | CLI version, such as `1.4.0` without a `v`; defaults to the version in the action's `Cargo.toml`. |
| `packslip-path` | An existing packslip executable to run instead of downloading a release: a path, or a name on PATH. Takes precedence over `packslip-version`. |
| `packslip-sha256` | Optional lowercase SHA-256 for an explicit `packslip-version` override; checked before provenance verification and extraction. The default uses the pinned action commit's internal digest lock. Ignored with `packslip-path`. |
| `token` | Token for installing packslip, `download`, and the upload; defaults to `github.token`. |

Output: `bundle`, the path of the written bundle, such as
`packslip/packslip.sigstore.json` or `packslip/packslip.mytool.sigstore.json`.

The action always passes `--project`, `--version`, `--url-base`,
`--notes-url`, `--source-repo`, `--tag`, and `--commit` to
`packslip create`, and these flags take precedence over the TOML manifest
given as `manifest`. Its `project`, `version`, `url_base`, `notes_url`, and
`[source]` are therefore ignored; set the matching action inputs instead.
The source repository is always the workflow's repository.

### Build provenance

With the default `attest: true`, the action attests every file that
`artifacts` matched or `download` fetched, using
`actions/attest-build-provenance`, and links each artifact's attestation
from the packslip. This needs `attestations: write`. The action neither
attests nor links an artifact listed only in the TOML manifest; add the
file to `artifacts` to cover it. A resource asset carries no provenance
link, and the action attests one only when `artifacts` or `download` also
collected it.

Set `attest: link` when your build jobs already attest each final file.
GitHub serves provenance by the file's digest, whichever job attested it,
so the link is the same and the action makes no second attestation. `link`
needs no `attestations: write`, and it does not check that an attestation
exists: a file nobody attested gets a link that resolves to nothing.

`attest: false` neither attests nor links. Switching to it after a release
that carried provenance makes consumers refuse the next one; see
[Keep later releases acceptable](#keep-later-releases-acceptable). In a
private or internal repository, GitHub artifact attestations need GitHub
Enterprise Cloud; on other plans, set `attest: false` from the first
release.

## Troubleshoot publication

| Symptom | What to check |
| --- | --- |
| `Unexpected input(s)` warning | The step ran without that input. Either the input name is misspelled (compare it with [Action inputs](#action-inputs)), or the pinned action release predates the input; use a newer release. |
| `could not download packslip-v…` | An x64 macOS runner has no release archive; [build the CLI](#build-the-cli-on-the-runner). Elsewhere, check that `packslip-version` is a released version such as `1.4.0`, without a `v`. |
| `not running on a tag; pass the tag input` | The run's ref is not a tag, as on a branch push, a schedule, or a dispatch from a branch. Pass `tag`, and also `commit` if the checked-out source is not `github.sha`. |
| No artifacts matched | Files must be present in this job's working directory, or matched by `download` from the release named by `tag`. Download matrix outputs before the action (or set `download`), and check the glob. |
| The attest step fails with a permission error | Give the job `attestations: write` and `id-token: write`, or set `attest: link` or `false`. A private repository without GitHub Enterprise Cloud needs `attest: false`. |
| Version rejected | Pass a semver `version` explicitly for tags such as `mytool-v1.2.3` or `v4.1`. |
| `warning: tag "…" does not name version …` | Consumers that list the project's releases by tag will not see this release. Use a tag that names the version, such as `v1.2.3`, or name the release in a [supplementary list](/docs/release-lists/#publish-a-github-repositorys-list). |
| Executable missing or ambiguous | Check the archive contents and use an explicit path or `NAME=PATH` mapping. |
| Signing cannot obtain a CI identity | Give the signing job `id-token: write`. |
| Bundle upload fails | The release named by `tag` must exist, the token must have `contents: write`, and an immutable release must still be a draft. |
| A resource URL returns a missing file | Upload that separate asset; resource declarations only describe it. |
| A download fails its digest check | Compare the published file with the final local file that was signed; do not disable verification. |
