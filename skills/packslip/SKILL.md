---
name: packslip
description: "Configure signed release manifests with packslip: add the jdx/packslip action or `packslip create` to a release workflow, declare completions, man pages, CLI specs, skills, and SBOMs as resources, and verify bundles and downloaded artifacts. Use when a repository has or needs packslip.sigstore.json, a packslip release.toml, or the jdx/packslip action, or when packslip verification fails."
---

# packslip

Work from the project's existing release workflow, artifact layout, and trust
policy. Check `packslip --version` and the relevant subcommand's `--help` when
flags differ from these examples.

## Describe the release that was built

- On GitHub Actions, add `jdx/packslip@v1` after the steps that build the final
  files and create the release. By default it attests the matched files, signs
  the release manifest with the job's OIDC identity, verifies it, and uploads
  only the bundle to the release. Upload the artifacts and any `asset:` files
  yourself first. Without the action, `packslip create` writes the signed
  bundle and uploads nothing.
- Pin the action to the full commit of its `vX.Y.Z` release. For an archive
  digest check in addition to build provenance, set `packslip-sha256` to the
  lowercase SHA-256 of the Packslip archive for the runner's OS and architecture.
  The check runs before provenance verification, extraction, or execution.
  `packslip-path` supplies a local executable and ignores this archive digest.
- Prefer two jobs, so the signing job cannot change the release. Run the
  action with `upload: false` in a job with these permissions:

  ```yaml
  permissions:
    contents: read       # Download release assets; cannot change the release.
    id-token: write      # Sign with the workflow's OIDC identity.
    attestations: write  # Attest the files (the default attest: true).
  ```

  Upload the file at the action's `bundle` output with
  `actions/upload-artifact`, and attach it to the release from a second job
  that has `contents: write`. A `contents: read` token cannot see a draft
  release, so the action's `download` input cannot fetch its files. If the
  release stays a draft until the bundle is attached, download the build's
  files with `actions/download-artifact` and give their paths to the action's
  `artifacts` input instead. If the action instead runs as one step in an
  existing release job, it uploads the bundle itself, and that job needs
  `contents: write` in addition to the two write permissions above. See
  [Publish with GitHub Actions](https://packslip.dev/docs/publishing/) for
  both layouts.
- By default the action uses the triggering tag as `tag`, that tag without a
  leading `v` as `version`, and `github.sha` as `commit`. When the job does not
  run on the release tag, as in a manual dispatch from a branch, pass `tag` and
  the tag's full commit SHA as `commit`. Pass `version` explicitly when the tag
  without a leading `v` is not semver, such as `mytool-v1.2.3` or `v4.1`; the
  action does not normalize tags.
- Sign every release of a project from the same workflow file in the project's
  own repository. A consumer treats a release signed from a different workflow
  file as a new signer and refuses it until a person approves it, so run
  backfills from that file too. If releases must come from several workflow
  files of the repository, sign them with `packslip create --no-pin-workflow`.
  The action has no input for it (GitHub only warns about an input the action
  does not define and runs the step without it), so run `packslip create` in
  those jobs instead. Declare it from the first release: a consumer that
  accepted a release without it refuses the first one with it until a person
  approves it, and consumers that predate the field ignore it and still ask. See
  [Workflow pinning](https://packslip.dev/release/v1/#workflow-pinning).
- Select only installable binaries or archives as artifacts. Declare companion
  files as resources. Inspect archive contents before setting paths: `--bin
  mytool` discovers the executable within an archive, while `--bin
  mytool=bin/tool` uses an explicit path from the true archive root.
- Inspect inferred OS, architecture, libc, and format with `packslip show`.
  With `packslip create`, fix ambiguous metadata with an explicit artifact
  argument, such as `dist/mytool.tar.gz:linux/x86_64/gnu`, or `PATH:any` for a
  file that runs on every platform, or with a TOML manifest. The action's
  `artifacts` input takes only file paths and globs: set platforms in
  `release.toml` (the `manifest` input), and use the `variants` and `formats`
  inputs for those fields. Absent platform fields mean unrestricted, not
  unknown.
- Leave the main build of each platform (OS, architecture, and libc) without a
  variant, and give every other distinct build of that platform its own
  variant, whatever its format; consumers select only artifacts without a
  variant unless a user asks for one. Artifacts that differ only in format
  must contain the same build, because consumers pick among them by format
  preference.
- Do not list shared libraries; `packslip create` reads them from the
  executables it can open. Declare commands the program needs on PATH with
  `--require bin:NAME[@MIN]` (the action's `require` input), and put
  `glibc_min` or `os_min` on the artifact's `requires` in `release.toml`. See
  [Host requirements](https://packslip.dev/docs/host-requirements/).

Use CLI flags for shared metadata and a TOML manifest for per-artifact layouts,
requirements, or resource scope. See
[Artifact configuration](https://packslip.dev/docs/describing-releases/) for
`release.toml` examples, path rules, and
[every key](https://packslip.dev/docs/describing-releases/#keys-in-releasetoml).

## Declare completions, man pages, CLI specs, skills, and SBOMs

Each resource has exactly one source:

| Source | Use it for | What fixes the content to this release |
| --- | --- | --- |
| `archive` | Files already inside each applicable binary archive | Artifact digest |
| `asset` | A separately uploaded file or skill archive | Its signed digest |
| `repo` | A tracked directory such as `skills/mytool` | `source.repo` and `source.commit` |
| `exec` | Content that must be generated by an installed executable | The verified executable; consumer execution policy still applies |

For example, add these to the action's `resources` input:

```yaml
resources: |
  cli-spec/usage/mytool=asset:dist/mytool.usage.kdl
  skill/mytool=repo:skills/mytool
  sbom/cyclonedx=asset:dist/mytool.cdx.json
```

The `mytool` qualifier binds the CLI specification to that executable, including
in releases containing several commands. An SBOM names its format
(`sbom/cyclonedx` or `sbom/spdx`) and cannot come from `exec`.

At the recorded release commit, the skill directory must contain `SKILL.md` and
every file it links by relative path. A separate skill asset is an archive with
`SKILL.md` at its root or under one top-level directory. An `asset:` value is a
local path that `packslip create` reads. Upload the same file to the URL the
bundle records (`url-base` plus the file name), because the action uploads only
the bundle.

With `packslip create`, a `repo` resource also needs `--source-repo` and
`--commit`, or `repo` and `commit` in a `[source]` table in `release.toml`; the
action passes both flags. An `exec` command must start with one of the
release's `bin` names.

Scope resources when archives have different layouts. On the command line or in
the action's `resources` input, put `@` and the platform after the kind and any
qualifier: `man@linux=archive:share/man/man1/mytool.1` or
`completion/zsh@darwin=archive:share/zsh/site-functions/_mytool`. Scope to one
exact artifact with `artifact = "FILENAME"` in a TOML `[[resource]]`. Do not
declare a universal archive path that exists on only one target. Prefer static
files for skills so installation does not require running the tool.

`packslip create` checks executable paths inside readable archives, but not
`archive:` or `repo:` resource paths. List each archive (`tar -tzf`,
`unzip -l`) to confirm every `archive:` path on every target it applies to.
Confirm a `repo:` directory at the release commit with
`git cat-file -e COMMIT:skills/mytool/SKILL.md`.

See [Resources](https://packslip.dev/docs/resources/#write-a-resource-entry)
for the kind and qualifier grammar and for how consumers choose among
entries.

## Verify before publishing or consuming

`packslip show BUNDLE` decodes metadata; it does not verify trust. Verify the
expected signer and downloaded artifacts together, for example:

```sh
packslip verify packslip.sigstore.json \
  --identity-prefix 'https://github.com/owner/mytool/.github/workflows/release.yml@' \
  --issuer https://token.actions.githubusercontent.com \
  --artifact dist/mytool-linux-x64.tar.gz
```

Replace the identity with the project's actual trusted workflow. A repository
prefix such as `https://github.com/owner/mytool/` accepts any workflow of the
repository; the workflow-file prefix above is narrower. Repeat `--artifact` for
additional local files. Also compare the verified statement's project and
version with the requested release; `packslip verify` does not enforce that
match. For key-signed releases, use the vendor's trusted public key with
`--pubkey`; do not obtain a new trust anchor from the same untrusted download
just to make verification pass.

After the first keyless release, `packslip pin packslip.sigstore.json` prints
the repository's signer fingerprint (`ps1_` and 26 characters). Run it on a
release you trust, because it names whichever repository signed the bundle.
Publish the fingerprint in the install instructions so users can verify with
`--pin` instead of trusting the first release they see. A fingerprint mismatch
means another repository signed the release; never replace the pin to make
verification pass. A key-signed project publishes its public key instead.

For local packaging checks, create a temporary key with `packslip keygen`, sign
with `packslip create --key ... --no-log`, and verify with
`packslip verify --pubkey ... --allow-unlogged`. Keep this explicit offline
exception in the test; do not add it to production verification to work around
a failure.

When verification fails, compare the expected identity, source commit, subject
file name and digest with the actual release. Preserve the failure until its
cause is understood. When only a resource is missing, inspect its selected
source and path before changing signing or trust settings.

## References

- [Publish with GitHub Actions](https://packslip.dev/docs/publishing/):
  permissions, a signing job without release write access, publishing the
  signer fingerprint, monorepos, and troubleshooting.
- [Artifact configuration](https://packslip.dev/docs/describing-releases/):
  `release.toml` examples and path rules.
- [Resources](https://packslip.dev/docs/resources/): resource kinds,
  qualifiers, and source selection.
- [Host requirements](https://packslip.dev/docs/host-requirements/): commands
  and minimum OS or glibc versions.
- [Manage release lists](https://packslip.dev/docs/release-lists/) and
  [Host releases on your own domain](https://packslip.dev/docs/self-hosting/):
  projects named after a host, withdrawals, and recommended versions.
- [Verify a release](https://packslip.dev/docs/verifying/#troubleshoot-a-failure):
  what each verification failure means.
