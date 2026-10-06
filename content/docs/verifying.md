---
title: Verify a release
weight: 50
group: consume
description: Check a release bundle and the files you downloaded against a repository, signer fingerprint, or public key you trust, and see what a successful result proves.
---
# Verify a release

This guide is for anyone who downloads a release and wants to check it
with the packslip CLI before unpacking or running it. `packslip verify`
checks the bundle against a pin you trust: a repository identity, a
signer fingerprint, or a public key. Pass each file you downloaded with
`--artifact`, and the same call checks it against the bundle. If you
have not chosen a file yet, verify the bundle alone first, choose the
artifact for your platform (`packslip show` lists them), and then verify
the downloaded file under the same pin.

## Verify against the expected repository

For a release signed in GitHub Actions, pin the repository you meant to
download:

```sh
packslip verify packslip.sigstore.json \
  --identity-prefix https://github.com/owner/repo/ \
  --issuer https://token.actions.githubusercontent.com \
  --artifact mytool-1.2.3-linux-x64.tar.gz
```

Keep the trailing slash: without it, the prefix
`https://github.com/owner/repo` also matches workflows of
`https://github.com/owner/repo-fork`.

Without identity flags, `verify` derives the policy from the project the
bundle claims: for `github.com/owner/repo`, a workflow under
`https://github.com/owner/repo/` through GitHub's issuer, and the
equivalent for a `gitlab.com` project. That shows only that the signer
belongs to the project the document names. It does **not** show that this
is the project or version you intended to install, so compare both with
your request. For a project on any other host there is nothing to derive,
and `verify` stops with `no identity to verify against`; pass `--pubkey`
or identity flags.

The identity flags replace that derived policy, and each one checks only
what it names. Pass `--issuer` together with `--identity-prefix` or
`--identity`: `--issuer` alone accepts a workflow of any repository on
that forge.

To pin one workflow file, extend the prefix through its `@`:
`--identity-prefix https://github.com/owner/repo/.github/workflows/release.yml@`.
`--identity` takes one exact certificate identity, which includes the
workflow's ref, such as
`https://github.com/owner/repo/.github/workflows/release.yml@refs/tags/v1.2.3`,
so it accepts only releases signed from that tag. Use it to check a single
release, not as a standing pin.

For a GitLab project, the prefix ends in `//`, which separates the project
path from the pipeline's configuration file, and the issuer is
`https://gitlab.com`:

```sh
packslip verify packslip.sigstore.json \
  --identity-prefix https://gitlab.com/group/tool// \
  --issuer https://gitlab.com \
  --artifact mytool-1.2.3-linux-x64.tar.gz
```

The [`packslip verify`](/cli/verify/) and [`packslip pin`](/cli/pin/)
references list every flag, including `--trusted-root` for a sigstore
deployment other than the public one.

## Pin a signer with its fingerprint

An identity prefix names a repository by its current name. Once a
repository is renamed or deleted, someone else can create a new one under
the old name, and its workflows match the same prefix. `packslip verify`
remembers nothing between runs, so it accepts the newcomer. An installer
that remembers the repository ID of the first release it accepts refuses
the newcomer later, but it still has to trust that first release. A
signer fingerprint closes the gap. It is `ps1_` followed by 26 characters
derived from the forge's issuer and the repository's numeric ID, so it
names one repository whatever that repository is called.

A vendor publishes its fingerprint where consumers can read it without
trusting a release, such as its website or README. A consumer records it
once in its own configuration, such as a Dockerfile, a CI workflow, or a
lockfile. The fingerprint protects you from the moment you record it: one
copied from the repository, or printed by `packslip pin` from a release,
after the name changed hands is the newcomer's.

A vendor gets its fingerprint by running `packslip pin` on a release its
own workflow just published, as
[Publish with GitHub Actions](/docs/publishing/#publish-your-signer-fingerprint)
shows. A consumer can do the same with a release it already trusts:

```sh
packslip pin packslip.sigstore.json
```

For the `github.com/jdx/hk` 2.3.0 release, it prints:

```text
ps1_snirenkjwr7m5ozgcufameodnm
```

`packslip pin` verifies the bundle as `packslip verify` does, under the
policy the project name implies or the identity flags you pass, then
prints the fingerprint of the repository that signed it. It cannot tell
the genuine repository from one that took its name, so run it only on a
release you already trust.

A consumer then passes the fingerprint it recorded to `verify`:

```sh
packslip verify packslip.sigstore.json \
  --pin ps1_snirenkjwr7m5ozgcufameodnm \
  --artifact hk-x86_64-unknown-linux-gnu.tar.gz
```

A release signed from any other repository, including one that took over
the old name, is refused, and the message names both fingerprints.
Checking the same hk release against another repository's fingerprint
prints:

```text
verification failed: the release is signed by ps1_snirenkjwr7m5ozgcufameodnm, but the pin is ps1_kwhjac5qpc45qetfh6ppwisi6a
```

`--pin` adds a check to the identity policy rather than replacing it. The
example passes no identity flags, so `verify` derives the policy from the
project the bundle names, which it can do only for a GitHub or GitLab
project. A release then passes only when:

- it is keyless and verifies under the policy;
- its certificate records a repository ID, as GitHub Actions and GitLab
  CI certificates have since Fulcio added the extension;
- that repository's fingerprint equals the pin; and
- for a GitHub or GitLab project, the certificate's repository is the one
  the statement names, and the signer is a workflow of it.

The fingerprint stays the same when the repository is renamed, moves to
another owner, or changes its release workflow. Every tool in a monorepo
shares it, so still check that the verified project is the tool you asked
for. A key-signed project has no fingerprint; pin its key with `--pubkey`.
See [Signer fingerprint](/release/v1/#signer-fingerprint) for how it is
derived.

## Verify against a public key

For a key-signed release, pass the vendor's public key: the `.pub` file
its `packslip keygen` wrote. Get it independently of the release, such as
from the vendor's website, or reuse a key you pinned earlier:

```sh
packslip verify packslip.sigstore.json \
  --pubkey release.pub \
  --artifact mytool-1.2.3-linux-x64.tar.gz
```

`--pubkey` also accepts the public key's base64 line. The bundle's key
hint is an unverified label for the signing key: packslip never trusts
it, and your own tooling should not either. Use `--allow-unlogged` only
when you have chosen to accept unlogged signatures for this vendor.

## Check every file you use

Repeat `--artifact` to check multiple files, including separate resource
assets:

```sh
packslip verify packslip.sigstore.json \
  --pubkey release.pub \
  --artifact mytool-1.2.3-linux-x64.tar.gz \
  --artifact mytool.cdx.json \
  --json
```

The command checks the signature, the certificate chain of a keyless
bundle, the transparency-log entry (a bundle without one fails unless you
pass `--allow-unlogged`), and the statement's structure. It then looks up
each supplied file in the statement by file name and compares its SHA-256
with the signed digest, and for an artifact, also its size. Keep the
original file names: a renamed file fails with
`not listed in the document`.

The success line counts artifacts and resource assets separately, so you
can see whether every file you meant to check was supplied:

```text
ok: example.com/mytool 1.2.3 published 2026-10-02T19:32:44Z signed by 5CA6E9FAFB1F4098 (sigstore-key) unlogged (1 of 1 artifact(s) and 1 of 1 asset(s) checked, 1 resource(s))
```

A release that lists no assets omits the asset count: `(2 of 3
artifact(s) checked)`. With `--json`, `checked_artifacts` and
`artifact_count` cover artifacts, and `checked_assets` and `asset_count`
cover assets. An asset you supply is reported only in `checked_assets`.

Without `--artifact`, success verifies the bundle alone. It does not
fetch, hash, or install remote artifacts. `--json` prints the report to
standard output for scripts. On failure nothing is printed there: the
reason goes to standard error, and the command exits 1 for a failed
verification or unusable input, or 2 for a usage error.

## Read the verified artifacts from a script

Build and deployment tooling often needs the verified metadata, not an
install: which file to fetch for a platform, its digest, and the commands
it provides. `--json` includes every artifact the signed statement lists,
whether or not you passed `--artifact`, so one command verifies the bundle
and returns what to stage:

```sh
packslip verify packslip.sigstore.json --pin ps1_snirenkjwr7m5ozgcufameodnm --json \
  | jq -r '.artifacts[] | select(.os == "linux" and .arch == "x86_64") | "\(.name) \(.sha256)"'
```

For the hk 2.3.0 release, this prints each matching artifact's name and
signed SHA-256. Each entry in `artifacts` has the statement's fields for
that artifact (`name`, `size`, `url`, `format`, `os`, `arch`, `libc`,
`variant`, `bin`, `requires`, `provenance`, `extensions`; absent ones are
omitted) and the signed `sha256`. Download the file yourself, from the
signed `url` or a mirror that keeps the file name, then confirm it by
running `verify` again with `--artifact`, or compare its SHA-256 with the
signed digest. Where the release lists several builds for a platform, the
[artifact selection rules](/release/v1/#selecting-an-artifact) decide
which one a consumer takes; the Rust library's `packslip::select_artifact`
implements them.

The report follows [`verify-report-v1.json`](/schema/verify-report-v1.json),
which `packslip schema --report` prints. Within version 1, packslip adds
report fields but never removes, renames, or changes the meaning of one, so
ignore fields you do not know.

## Verify a release list

The same command accepts a signed release list:

```sh
packslip verify packslip.json --pubkey release.pub
```

This checks the list's signature and structure; `--pin` and the identity
flags apply as they do to a release. The CLI does not compare the expiry
with the current time or the sequence with lists you accepted before. The
`ok:` line and `--json` report the list's `expires_at` and `sequence`,
and a consumer must enforce both, as
[consumer rule 5](/release/v1/#consumer-rules) says. The CLI also does
not fetch or verify the release bundles the list references, and it does
not accept `--artifact` for a release list.

## Understand the result

This example verifies the `github.com/jdx/hk` 2.3.0 bundle without
`--artifact`:

```sh
packslip verify packslip.sigstore.json
```

The command prints a summary line, the signing repository, and one
`requires` line for each artifact that declares requirements. The output
below is trimmed to one of its seven `requires` lines:

```text
ok: github.com/jdx/hk 2.3.0 published 2026-09-26T19:59:21.295096144Z signed by https://github.com/jdx/hk/.github/workflows/release.yml@refs/tags/v2.3.0 (sigstore-oidc) logged 2026-09-26T19:59:21Z (0 of 7 artifact(s) and 0 of 5 asset(s) checked, provenance linked, 7 resource(s))
  repository https://github.com/jdx/hk (id 922514152), owner https://github.com/jdx (id 216188)
  requires hk-x86_64-pc-windows-msvc.zip: libs combase.dll vcruntime140.dll
```

- In `0 of 7 artifact(s) and 0 of 5 asset(s) checked`, each first number
  counts the files you passed with `--artifact` and each second number how
  many the release lists. Here nothing was hashed; see
  [Check every file you use](#check-every-file-you-use).
- The `repository` line appears for a keyless bundle from GitHub Actions
  or GitLab CI. It names the repository the signing run was based on,
  with the forge's immutable IDs for it and its owner, and `--json`
  reports them as `source_repository`. A repository keeps its ID when it
  is renamed or transferred to another owner, so a consumer pins the ID
  rather than the name or the owner; see
  [Forge identity](/release/v1/#forge-identity).
- Each `requires` line repeats what one artifact declares. `verify` does
  not check it against this host.

| A successful verification establishes… | It does not establish… |
| --- | --- |
| The statement was signed by an identity or key allowed by the policy. | The signer or its build environment was uncompromised. |
| The signer belongs to the project the statement names, or matches your flags. | The project and version are the ones you asked for. |
| Supplied files match the signed digests. | The software is safe or free of vulnerabilities. |
| A logged signature has verified transparency-log evidence. | This is the newest release or the vendor's recommended version. |
| The statement contains provenance links, if reported. | The linked provenance has been fetched or verified. |
| Each artifact's declared host requirements are reported. | This host meets them. |

For the full account, see
[What a verified packslip proves](/release/v1/#what-a-verified-packslip-proves).

`packslip show BUNDLE` prints the statement without verifying it. Use it
for inspection, not as evidence of authenticity.

## Troubleshoot a failure

| Symptom | What to check |
| --- | --- |
| `expected an identity starting with` or `identity mismatch: expected` | The release was signed outside the repository or identity you pinned. `--identity` is exact, including the tag ref, so a standing pin needs `--identity-prefix`. Do not loosen the pin just to make the command pass. If the repository was renamed or moved, see the next row. |
| The repository was renamed or moved to another owner | With `--pin`, or a pin the library stored, nothing changes: the repository ID is the same, so releases from before and after the move verify, and the `ok:` and `repository` lines show the name each release was signed under. An `--identity-prefix` pin names the old location, so releases signed after the move fail with `expected an identity starting with`. Before you update the prefix, confirm the move: check that `packslip pin` prints the same fingerprint for a new release as for one you accepted before the move, or ask the project. |
| `issuer mismatch: expected` | `--issuer` names a different OIDC issuer from the one that signed the release. Pass the issuer of the CI system that ran the release: `https://token.actions.githubusercontent.com` for GitHub Actions, `https://gitlab.com` for GitLab CI. |
| `the release is signed by ps1_…, but the pin is ps1_…` | A different repository signed the release, often one that took a renamed or deleted repository's name. Find where the project went, and do not replace your pin with the fingerprint the message prints. |
| `records no repository ID` | The certificate predates Fulcio's repository extensions or comes from another CI system. Pin the release with `--identity-prefix` and `--issuer` instead of `--pin`. |
| `no identity to verify against` | The project is not named on GitHub or GitLab, so `verify` has no policy to derive. Pass `--pubkey` for a key-signed release, or identity flags for a keyless one. |
| `an identity was pinned; pin the key instead` | The release is key-signed. Pass the vendor's public key with `--pubkey`. |
| `a key was pinned; pin an identity instead` | The release is keyless. Pin its identity or its signer fingerprint instead of a key. |
| `no transparency log entry` | Confirm with the vendor that it omitted logging on purpose before you pass `--allow-unlogged`. |
| `not listed in the document` | Restore the file's original name. |
| `sha256 is` or `size is`, followed by `document says` | Confirm the original file name and release version, then download it again from the vendor's URL. |
| Expired, rolled-back, or missing signed list | Obtain a current list; retain the existing trust state while investigating. |

## Build an installer or mirror

The CLI verifies one document at a time. An installer also needs
discovery, artifact selection, and remembered trust; see
[Build an installer or mirror](/docs/installers/) and the
[consumer rules](/release/v1/#consumer-rules).

### Take the crate as a library

The `packslip` crate's verifier, selection rules, and feature flags are
covered in [Use the Rust library](/docs/installers/#use-the-rust-library).
