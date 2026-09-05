---
title: Publish with GitHub Actions
weight: 20
description: Add a signed packslip to an existing GitHub release workflow.
---
# Publish with GitHub Actions

The `jdx/packslip` action signs a manifest with your workflow's identity
and uploads the bundle to an existing GitHub release. No long-lived
signing key is needed.

## Prepare the files and release

The action consumes final local files. In a build matrix, collect the artifacts
into the signing job before running it. Complete any archive rewriting,
platform signing, or notarization that changes the bytes first.

Create the GitHub release and upload the artifacts through your existing
workflow. Upload separate resource assets, such as SBOMs, too. The action
uploads only the packslip bundle; a signed URL does not upload its target.

## Add the release step

Use this fragment after those preparation steps in your release job.

```yaml
permissions:
  contents: write       # Upload the bundle.
  id-token: write       # Sign with the workflow's identity.
  attestations: write  # Publish provenance for the matched files.

steps:
  # Your existing build and release steps go here.
  - uses: jdx/packslip@v0
    id: packslip
    with:
      artifacts: dist/*.tar.xz dist/*.zip
      bin: mytool
```

Run on a release tag, or pass `tag` explicitly. By default, `project` is
`github.com/<owner>/<repo>`, and `version` is the tag with a leading `v`
removed. For tags such as `mytool-v1.2.3`, pass `version: 1.2.3` explicitly.
The action does not normalize arbitrary tag formats.

The action installs its matching packslip version, attests the files
matched by `artifacts`, creates and signs the manifest, verifies the
bundle, and uploads it. The output `${{ steps.packslip.outputs.bundle }}`
is the local bundle path.

The action and CLI share a version: `@v0` follows CLI 0.x releases, and
`@v0.2.0` pins both to 0.2.0. By default, the action installs the CLI
version from its own commit's `Cargo.toml`. Set `packslip-version` to
explicitly override that selection.
`packslip-path` runs a CLI the job already has instead of downloading
one; see [Build the CLI on the runner](#build-the-cli-on-the-runner).

## Add resources and requirements

```yaml
- uses: jdx/packslip@v0
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
Use a [TOML manifest](/docs/describing-releases/#use-a-toml-manifest) when
paths or requirements differ between artifacts.

## Release several tools from one repository

Run the action once per tool, selecting only that tool's artifacts:

```yaml
- uses: jdx/packslip@v0
  with:
    project: github.com/owner/repo/mytool
    version: 1.2.3
    tag: mytool-v1.2.3
    artifacts: dist/mytool-*.tar.xz
    bin: mytool
```

This writes `packslip.mytool.sigstore.json`. Nested subpaths use hyphens
in the filename: `tools/mytool` becomes `packslip.tools-mytool.sigstore.json`.
The signer is still pinned to the repository. Consumers match the signed
`project` field, not the bundle filename.

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
      contents: write
      id-token: write
      attestations: write
    steps:
      # Build the archives and create the release before these steps.
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo install packslip --version 0.2.0 --locked --root "$RUNNER_TEMP/packslip"
      - uses: jdx/packslip@v0.2.0
        with:
          packslip-path: ${{ runner.temp }}/packslip/bin/packslip
          artifacts: dist/*.tar.xz
          bin: mytool
```

Build on the runner that will run the binary; the action executes it
rather than cross-compiling for anything. Install the version the action
ref pins, since the action and CLI are released together, and prefer
`--locked` so the build uses the dependency versions that release was
tested with. The same approach covers a platform packslip does not ship,
a self-hosted or network-restricted runner, and a job that would rather
build from source than download.

A matrix that needs this on only some runners can leave the input empty
elsewhere; an empty `packslip-path` downloads as usual:

```yaml
packslip-path: ${{ runner.os == 'macOS' && runner.arch == 'X64' && format('{0}/packslip/bin/packslip', runner.temp) || '' }}
```

`packslip-path` takes precedence over `packslip-version`, which the action
warns about when both are set. A downloaded archive is checked against
jdx/packslip's build provenance before it runs; a binary supplied this way
is not checked at all, so the job vouches for where it came from.

## Action inputs

| Input | Purpose and default |
| --- | --- |
| `artifacts` | Required. Whitespace-separated local files or globs; at least one file must match. |
| `bin` | Whitespace-separated executable names or `NAME=PATH` entries. |
| `project` | Project name; defaults to `github.com/<owner>/<repo>`. |
| `version` | Semver version; defaults to the tag without its leading `v`. |
| `tag` | Existing release tag; defaults to the triggering tag. |
| `manifest` | Path to a TOML manifest. Its artifact entries join the matched files. |
| `variants` | Whitespace-separated `FILENAME=VARIANT` entries. |
| `resources` | One resource declaration per line. |
| `require` | One `bin:NAME[@MIN]` requirement per line. |
| `extensions` | One `NAME=JSON` extension per line. |
| `url-base` | Artifact download prefix; defaults to the release's download URL. |
| `notes-url` | Defaults to the release page. |
| `attest` | Defaults to `true`. Set to `false` to skip generating and linking provenance. |
| `out` | Bundle output directory; defaults to `packslip`. |
| `upload` | Defaults to `true`. Set to `false` to keep the bundle local. |
| `packslip-version` | CLI version; defaults to the version in the action's `Cargo.toml`. |
| `packslip-path` | An existing packslip executable to run instead of downloading a release: a path, or a name on PATH. Takes precedence over `packslip-version`. |
| `token` | Download/upload token; defaults to `github.token`. |

The provenance step covers files matched by `artifacts`. Files supplied
only through the manifest or a resource declaration are not automatically
included in that step. Include them in `artifacts` if you want the action
to attest them too. A file also declared as a resource asset is recorded
as an asset rather than an installable artifact.

The action passes project, version, source, and URL metadata as CLI flags,
which take precedence over the corresponding manifest values. Change
those values through action inputs where available.

## Check the published result {#check-the-result}

The action verifies the local bundle before uploading it. Check the published
release separately: download the bundle and an artifact from the URLs users
will use, then [verify both](/docs/verifying/#verify-against-the-expected-repository)
against your repository identity. This also catches a wrong upload, stale file,
or URL that points at a different build.

Inspect the statement with `packslip show` to confirm the project, normalized
version, source tag, platforms, and executable paths. Inspection does not
replace verification. Linked build provenance also needs its own verification;
the packslip verification command does not fetch it.

For a draft workflow trial, set `upload: false` to retain the bundle locally.
This still signs the statement and, with the default `attest: true`, publishes
provenance. Use the [local walkthrough](/docs/getting-started/) for an offline
trial that does not contact signing services.

## Troubleshoot publication

| Symptom | What to check |
| --- | --- |
| No artifacts matched | Files must be present in this job's working directory. Download matrix outputs before the action, and check the glob. |
| Version rejected | Pass a semver `version` explicitly for tags such as `mytool-v1.2.3` or `v4.1`. |
| Executable missing or ambiguous | Check the archive contents and use an explicit path or `NAME=PATH` mapping. |
| Bundle upload fails | The release named by `tag` must exist and the token must have `contents: write`. |
| Signing cannot obtain a CI identity | Give the signing job `id-token: write`. |
| A resource URL returns a missing file | Upload that separate asset; resource declarations only describe it. |
| A download fails its digest check | Compare the published file with the final local file that was signed; do not disable verification. |
