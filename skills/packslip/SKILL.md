---
name: packslip
description: "Publish, verify, and install signed packslip releases: add the jdx/packslip action or `packslip create` to a release job, declare resources and host requirements, verify bundles and downloaded artifacts, pin a `ps1_` signer fingerprint, and replace `curl | sh` with `packslip install`. Use when a repository has or needs packslip.sigstore.json (or packslip.<subpath>.sigstore.json), the jdx/packslip action, a packslip release.toml, or a packslip release list, or when a packslip command or the action fails."
---

# packslip

Work from the project's existing release workflow, artifact layout, and
trust policy. When a flag differs from these examples, check
`packslip --version` and the subcommand's `--help`.

## Rules for every task

- Take identities, issuers, fingerprints, keys, and trusted roots only from
  the vendor, a repackager, the user, or an administrator, never from a
  download, `packslip show`, or an error. Exception: a first install's
  fingerprint, once the user accepts trust on first use.
- Unless the user says to, never replace or drop a pin, widen an identity
  past the vendor's, pass `--issuer` alone for a domain project, or add
  `--allow-other-repository`, `--allow-unlogged`, `--accept-trust-change`,
  `--force`, `--allow-incompatible-host`, or `--trusted-root`, even when an
  error suggests one.
- Never sign a real release with `--no-log`.
- After `ok:`, check the verified project and version against the user's
  request (`verify --json` in scripts); `verify` does not, and
  `packslip show` verifies nothing.
- Find out why verification failed before retrying with other flags or
  files.

## Publish a release

1. On GitHub Actions, use `jdx/packslip@v1`
   ([Publish with GitHub Actions](https://packslip.dev/docs/publishing/)),
   pinned to a full commit SHA for hardening
   ([Pin the action to a commit](https://packslip.dev/docs/actions/#pin-the-action-to-a-commit)).
   Elsewhere, run `packslip create`
   ([Publish from other CI](https://packslip.dev/docs/other-ci/)). For a
   domain project, see
   [Host releases on your own domain](https://packslip.dev/docs/self-hosting/).
2. List only installable binaries or archives. `--bin mytool` finds the
   executable in each archive; `--bin mytool=mytool-X.Y.Z/bin/tool` maps
   the command to a path from the archive root, top-level directory
   included. Give each extra build of a platform (fips, baseline, debug) a
   variant, even in another format, and the main build none; `create`
   catches a missing variant only when two builds share a format.
   Put per-artifact settings in release.toml (`--manifest`, or the
   action's `manifest` input;
   [release.toml reference](https://packslip.dev/docs/release-toml/)).
3. Ship completions, man pages, CLI specs, skills, and SBOMs as resources
   ([Write a resource entry](https://packslip.dev/docs/resources/#write-a-resource-entry)).
   `create` does not check `archive:` or `repo:` paths; confirm them with
   `tar tf`, `unzip -l`, or `git cat-file -e COMMIT:PATH`.
4. Do not list shared libraries; `create` finds them.
   Declare commands needed on PATH with `--require bin:java@17` (the
   action's `require` input;
   [Declare host requirements](https://packslip.dev/docs/host-requirements/)).
5. Upload artifacts and `asset:` resource files first; the action uploads
   only the bundle. Sign in a separate job (`contents: read`,
   `id-token: write`, `attestations: write`) with `upload: false`, and pass
   the file at its `bundle` output through `actions/upload-artifact` to a
   [`contents: write` job that uploads it](https://packslip.dev/docs/publishing/#keep-the-action-away-from-release-write-access).
   For a draft release, stage files as `artifacts`; `download` cannot see
   drafts. Pass `version` when the tag minus a leading `v` is not semver
   (`mytool-v1.2.3`).
6. Sign every release, backfills included, from one workflow file, or
   consumers refuse it until a person approves
   ([Workflow pinning](https://packslip.dev/release/v1/#workflow-pinning)).
   For a run the release tag did not trigger, such as a backfill, pass
   `tag` and set `commit` to the tag's full commit SHA.
   For several files, see
   [Keep later releases acceptable](https://packslip.dev/docs/publishing/#keep-later-releases-acceptable).
7. After the first keyless release, publish the `ps1_…` signer fingerprint
   `packslip pin` prints for a trusted bundle (for a domain project, pass
   its `--issuer` and `--identity-prefix`).

## Verify and diagnose

Pass what the vendor publishes: `--pin ps1_…`, identity flags, or
`--pubkey`; for a github.com or gitlab.com project with none published,
pass no trust flags, since the name sets the policy.

```sh
packslip verify packslip.sigstore.json --artifact mytool-linux-x64.tar.gz --pin ps1_…
```

On a failure, stop and report it unless its row gives a step:

| Error text | Step |
| --- | --- |
| `must be signed from its repository` | In `verify` only, for a repackager the user accepts, add `--allow-other-repository`, its `--issuer`, and one of `--identity`, `--identity-prefix`, or `--source-repository`. |
| `expected an identity starting with` | If the vendor documents, or the user accepts, another repository's reusable workflow, replace `--identity-prefix` with `--source-repository https://github.com/owner/mytool`, keeping or adding `--issuer`. After a rename or transfer the user confirms, see [Pin a signer with its fingerprint](https://packslip.dev/docs/verifying/#pin-a-signer-with-its-fingerprint). |
| `but the pin is`, `different repository now`, `same repository` | The release is for, or signed by, another repository, possibly one that took a moved repository's name; a rename or transfer keeps the fingerprint and passes. Report it; never pin the printed value. |
| `identity mismatch: expected` | If only the tag differs and names the requested version, pass the vendor's identity for that tag or its `--identity-prefix`. |
| `no identity to verify against`, `pin the key instead` | Pass the vendor's `--pubkey`, or for a keyless domain project its `--issuer` with `--identity` or `--identity-prefix`. |
| `not listed in the document` | Check that the bundle is this file's release; restore the file's published name. |
| `sha256 is`, `size is` | Check the file name and version; download it again. |

If only a resource is missing, check its source and path before changing
signing or trust settings
([Troubleshoot a failure](https://packslip.dev/docs/verifying/#troubleshoot-a-failure)).

## Install a tool

1. Get packslip ([Install packslip](https://packslip.dev/docs/install/)).
2. Replace `curl | sh` with
   `packslip install github.com/owner/mytool --pin ps1_…`; a key-signed
   domain project takes the vendor's `--pubkey` instead.
3. With no published fingerprint, tell the user the first install trusts
   the signer the name or its host reports and holds later installs to it
   (for a keyless domain project, also to the repository that signed it).
   If they accept, omit `--pin` and record the printed
   `Signer fingerprint: ps1_…` line (`Signer pin:` in 1.10.1 and earlier;
   domain projects print none).
4. Pass `--version X.Y.Z` for repeatable builds.

`install` refuses gitlab.com and has no uninstall, upgrade, or list; run it
again for a later release
([Bootstrap a tool](https://packslip.dev/docs/bootstrap/)).

## Other tasks

[Remove a tool](https://packslip.dev/docs/bootstrap/#remove-a-tool),
[Use packslip with mise](https://packslip.dev/docs/mise/),
[Manage release lists](https://packslip.dev/docs/release-lists/),
[Publish from Go](https://packslip.dev/docs/go/),
[Verify from JavaScript](https://packslip.dev/docs/verify-js/),
[Use the Rust crate](https://packslip.dev/docs/rust/),
[Build an installer or mirror](https://packslip.dev/docs/installers/), and
[Commit a verifier into your project](https://packslip.dev/docs/verify-script/).
