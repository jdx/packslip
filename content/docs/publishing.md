---
title: Publish with GitHub Actions
weight: 20
description: Add a signed packslip to an existing GitHub release workflow.
---
# Publish with GitHub Actions

The `jdx/packslip` action signs a manifest with your workflow's identity
and uploads the bundle to an existing GitHub release. No long-lived
signing key is needed.

## Add the release step

Use this fragment in your release job. The artifacts must already exist
on the runner, and the GitHub release must exist before the upload step.
Upload your binaries through your existing release workflow; this action
uploads the packslip bundle only.

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
| `token` | Download/upload token; defaults to `github.token`. |

The provenance step covers files matched by `artifacts`. Files supplied
only through the manifest or a resource declaration are not automatically
included in that step. Include them in `artifacts` if you want the action
to attest them too. A file also declared as a resource asset is recorded
as an asset rather than an installable artifact.

The action passes project, version, source, and URL metadata as CLI flags,
which take precedence over the corresponding manifest values. Change
those values through action inputs where available.

## Check the result

Download the bundle and one release artifact, then follow
[Verify a release](/docs/verifying/). Verification of the packslip does
not verify the linked provenance; consumers must check that separately.
