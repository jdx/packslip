---
title: Verify a release
description: Check a release bundle and downloaded files against a project's repository, a signer fingerprint, or a public key, and see what that proves.
group: consume
weight: 52
---
# Verify a release

Check downloaded release files before you unpack or run them, and learn
what a passing result proves. To find, verify, and install a tool in one
command, see [Bootstrap a tool](/docs/bootstrap/); to verify keyless
releases without packslip, see
[Commit a verifier into your project](/docs/verify-script/).

You need packslip ([Install packslip](/docs/install/)), the release bundle
(`packslip.sigstore.json`), the downloaded files, and a signer policy from
the publisher's documentation or an earlier release you trust. What you
know about the signer picks the section:

- Only a forge project's name (`github.com/…` or `gitlab.com/…`):
  [no trust flags](#verify-against-the-expected-repository). A domain
  project such as `mytool.example.com` needs identity flags or `--pubkey`.
- The workflow or repository that signs: [identity flags](#narrow-the-policy-with-identity-flags).
- A `ps1_…` signer fingerprint: [`--pin`](#pin-a-signer-with-its-fingerprint).
- An Ed25519 public key: [`--pubkey`](#verify-against-a-public-key).
- A repackager that re-signed the vendor's files: [`--allow-other-repository`](#verify-a-repackagers-release).

## Verify against the expected repository

For a forge project, run `verify` without trust flags. This downloads and
checks hk 2.3.0's bundle and Linux x86_64 archive:

```sh
curl -fsSLO https://github.com/jdx/hk/releases/download/v2.3.0/packslip.sigstore.json
curl -fsSLO https://github.com/jdx/hk/releases/download/v2.3.0/hk-x86_64-unknown-linux-gnu.tar.gz
packslip verify packslip.sigstore.json --artifact hk-x86_64-unknown-linux-gnu.tar.gz
```

`verify` derives the policy from the statement's project: here, the release
must be signed by a GitHub Actions run whose certificate records
`https://github.com/jdx/hk` as its source repository, whichever workflow ran
([What a project name says about the signer](/docs/trust/#what-a-project-name-says-about-the-signer)).
On success, `verify` prints an `ok:` line
([Understand the result](#understand-the-result)) and exits 0.

**Warning:** This shows only that the signer belongs to the project the
statement names, not that it is the project or version you meant. Compare
both with your request. In a monorepo, the derived policy, an identity
prefix, and a `--pin` accept every tool in the repository.

## Narrow the policy with identity flags

Identity flags name the issuer and workflow that must sign. What they do
depends on the project:

| Project | Identity flags | `--issuer` alone accepts |
| --- | --- | --- |
| Forge project | Narrow the derived policy: the signer must still belong to the project's repository. | The repository's workflows and the reusable workflows they call. |
| Domain project | Are the whole policy, required unless you pass `--pubkey`. Each checks only what it names. | Any repository on that forge, so add `--identity-prefix`, `--identity`, or `--pin`. |

This accepts any GitHub Actions workflow of `owner/mytool`:

```sh
packslip verify packslip.sigstore.json \
  --identity-prefix https://github.com/owner/mytool/ \
  --issuer https://token.actions.githubusercontent.com \
  --artifact mytool-X.Y.Z-linux-x64.tar.gz
```

- `--identity-prefix` is a prefix the certificate identity must start with.
  Keep the trailing `/`: `https://github.com/owner/mytool` also matches
  `https://github.com/owner/mytool-fork`. To accept one workflow file,
  extend the prefix through its `@`:
  `https://github.com/owner/mytool/.github/workflows/release.yml@`.
- `--identity` is one exact identity, including the workflow's ref
  (`…/release.yml@refs/tags/vX.Y.Z`), so it accepts only releases signed
  from that tag. Use it to check one release, not as a standing policy.
- `--issuer` is `https://token.actions.githubusercontent.com` for GitHub
  Actions, or `https://gitlab.com` for GitLab CI.

Repository names match in any letter case; workflow paths and refs match
exactly. For GitLab, the prefix ends in `//`, which separates the project
path from the pipeline configuration file:

```sh
packslip verify packslip.sigstore.json \
  --identity-prefix https://gitlab.com/owner/mytool// \
  --issuer https://gitlab.com \
  --artifact mytool-X.Y.Z-linux-x64.tar.gz
```

The [`packslip verify`](/cli/verify/) and [`packslip pin`](/cli/pin/)
references list every flag.

### Require the source repository

An identity prefix checks where the signing workflow lives, not which
repository's run called it. To require the repository the run came from,
whichever workflow signed:

```sh
packslip verify packslip.sigstore.json \
  --source-repository https://github.com/owner/mytool \
  --issuer https://token.actions.githubusercontent.com \
  --artifact mytool-X.Y.Z-linux-x64.tar.gz
```

- `--source-repository` requires the certificate to record that repository
  as its source. It replaces, and refuses, `--identity` and
  `--identity-prefix`.
- `--issuer` is required with it, because the repository is only as good as
  the issuer that vouches for it.

For a domain project, only `--source-repository` or `--pin` checks the
certificate's source repository. With an identity prefix alone, a run of
any repository that calls a workflow under the prefix passes. A forge
project's derived policy already requires its repository. When another
repository's reusable workflow signs its releases, a prefix of the project
refuses them, so use `--source-repository` or no trust flags.

## Pin a signer with its fingerprint

A signer fingerprint (`ps1_` and 26 characters) names the signing
repository by its forge ID, which an identity prefix cannot; see
[First use and signer fingerprints](/docs/trust/#first-use-and-signer-fingerprints).
Vendors publish it on their website or README
([Publish your signer fingerprint](/docs/publishing/#publish-your-signer-fingerprint));
record it in your Dockerfile, CI workflow, or lockfile. To compute it:

```sh
packslip pin packslip.sigstore.json
```

For `github.com/jdx/hk` 2.3.0, this prints `ps1_snirenkjwr7m5ozgcufameodnm`.
`packslip pin` verifies as `verify` does without `--allow-other-repository`,
taking the same identity flags (a domain project needs them), then prints
the signing repository's fingerprint.

**Warning:** `packslip pin` cannot tell the genuine repository from one
that took its name. Run it only on a release you already trust, and obtain
the fingerprint before the name changes hands: one copied or computed from
the new repository identifies the new owner's.

Then pass the fingerprint to `verify`:

```sh
packslip verify packslip.sigstore.json \
  --pin ps1_snirenkjwr7m5ozgcufameodnm \
  --artifact hk-x86_64-unknown-linux-gnu.tar.gz
```

- `--pin` adds a check on top of the policy, here the one hk's GitHub name
  implies: the certificate must record a repository ID that produces the
  fingerprint. A domain project needs `--issuer` with it, and optionally an
  identity prefix.
- A certificate without a repository ID, such as an older one, cannot pass
  `--pin`; use an identity prefix and issuer.
- A key-signed release has no fingerprint; pin its key with
  [`--pubkey`](#verify-against-a-public-key).

A release from any other repository, including one that took over the old
name, is refused. Checking hk 2.3.0 against another repository's
fingerprint names the signer, then the pin:

```text
verification failed: the release is signed by ps1_snirenkjwr7m5ozgcufameodnm, but the pin is ps1_kwhjac5qpc45qetfh6ppwisi6a
```

After a rename or transfer, `--pin` keeps passing while an
`--identity-prefix` of the old name fails with
`expected an identity starting with`. Before updating the prefix, confirm
that `packslip pin` prints the same fingerprint for a new release as for
one from before the move, or ask the project. See
[Repository identity, renames, and transfers](/docs/trust/#repository-identity-renames-and-transfers);
[Signer fingerprints](/release/v1/#signer-fingerprints) defines the value.

## Verify against a public key

For a key-signed release, pass the vendor's Ed25519 public key: the
minisign `.pub` file its `packslip keygen` wrote, or its base64 line. Get
it independently of the release, such as from the vendor's website, or
reuse one you pinned earlier:

```sh
packslip verify packslip.sigstore.json \
  --pubkey release.pub \
  --artifact mytool-X.Y.Z-linux-x64.tar.gz
```

The key is the whole policy and names no repository, so `verify` refuses
`--pubkey` with the identity flags, `--source-repository`, `--pin`, or
`--allow-other-repository`.

**Warning:** Never take a new key from the same download to make
verification pass. The bundle's key hint is an unverified label: packslip
never trusts it, and your tooling should not either.

## Verify a repackager's release

A repackager re-signs another vendor's files: its bundle names the vendor's
project but is signed from the repackager's repository. For a forge
project, the default policy refuses it with
`must be signed from its repository`, even with identity flags. To accept
a repackager you trust:

```sh
packslip verify packslip.sigstore.json \
  --identity-prefix https://github.com/repackager/builds/ \
  --issuer https://token.actions.githubusercontent.com \
  --allow-other-repository \
  --artifact mytool-X.Y.Z-linux-x64.tar.gz
```

- `--allow-other-repository` lifts the requirement that the signer belong
  to the project's repository. It needs `--issuer`, plus `--identity`,
  `--identity-prefix`, or `--source-repository`.
- `--pin` still applies, with the repackager's fingerprint.

`verify` takes no expected project, so this accepts every project the
repackager signs. [Repackager attestation](/release/v1/#repackager-attestation)
defines the `"attested_by": "repackager"` claim behind `repackager-attested`
in the `ok:` line.

## Check every file you use

Repeat `--artifact` (`-a`) for every file you downloaded, including
resource assets such as completion scripts. Deployment tooling can copy
the bundle and files to the host and check a whole release in one call.
Files are matched by name, so keep the published names: a renamed file
fails with `not listed in the document`. Each file's SHA-256 must match
the signed digest, and an artifact's size too.

This adds hk's Linux arm64 archive and bash completion script to the first
download:

```sh
curl -fsSLO https://github.com/jdx/hk/releases/download/v2.3.0/hk-aarch64-unknown-linux-gnu.tar.gz
curl -fsSLO https://github.com/jdx/hk/releases/download/v2.3.0/hk.bash
packslip verify packslip.sigstore.json \
  --artifact hk-x86_64-unknown-linux-gnu.tar.gz \
  --artifact hk-aarch64-unknown-linux-gnu.tar.gz \
  --artifact hk.bash
```

The `ok:` line ends with
`(2 of 7 artifact(s) and 1 of 5 asset(s) checked, provenance linked, 7 resource(s))`.
Each pair counts the files you passed against those the release lists.
Equal numbers mean every file of that kind was supplied and matched, if you
passed each file once. A release with no assets omits their count:
`(2 of 3 artifact(s) checked)`. In `--json`, `checked_artifacts` and
`checked_assets` list the names of the files checked, and `artifact_count`
and `asset_count` give how many the release lists. An asset you pass
appears only in `checked_assets`.

Without `--artifact`, success covers the bundle alone. `verify` downloads
nothing, so it never hashes or installs remote artifacts.

## Read the verified artifacts from a script

For tooling that needs verified metadata rather than an install (which
file to fetch, its digest, the commands it provides), `--json` prints a
report on standard output. On failure, standard output stays empty and the
reason goes to standard error;
[`packslip verify`](/cli/verify/#exit-status) lists the exit codes.

The report lists every artifact in the statement, whether or not you passed
`--artifact`. To choose a file before downloading it, read this list, not
`packslip show`, which prints the statement without verifying it:

```sh
packslip verify packslip.sigstore.json --pin ps1_snirenkjwr7m5ozgcufameodnm --json \
  | jq -r '.artifacts[] | select(.os == "linux" and .arch == "x86_64") | "\(.name) \(.sha256)"'
```

For hk 2.3.0, this prints each match's name and signed SHA-256:

```text
hk-x86_64-unknown-linux-gnu.tar.gz 79cabdccba39e1eef4e443e11713c923381f697b03c00ef4a12dfcd1ca5380b0
hk-x86_64-unknown-linux-musl.tar.gz 307f760266af8a393c47a8039e2c966c78361eae33949865e9ab4cc163f517b4
```

Each `artifacts` entry has the signed `sha256` and the statement's fields
(`name`, `size`, `url`, `format`, `os`, `arch`, `libc`, `variant`, `bin`,
`requires`, `provenance`, `extensions`), omitting absent ones. When several
builds fit a platform, like gnu and musl here, the
[artifact selection rules](/release/v1/#selecting-an-artifact) pick one;
the Rust crate implements them as `packslip::select_artifact`
([Select an artifact and its resources](/docs/rust/#select-an-artifact-and-its-resources)).
Download the file from its signed `url`, or from a mirror that keeps the
file name. Then confirm it with `verify --artifact` under the same policy,
or compare its SHA-256 with the signed digest.

The report has `"pin_workflow": false` when the release opts out of
workflow pinning, and omits the field otherwise
([Workflow pinning](/release/v1/#workflow-pinning)).
It follows [`verify-report-v1.json`](/schema/verify-report-v1.json), which
`packslip schema --report` prints. Within version 1, fields are added but
never removed, renamed, or given a new meaning, so ignore unknown fields.

## Verify a release list

`verify` also accepts a signed release list. The one packslip.dev serves
at `.well-known/packslip.json` belongs to a domain project and is signed
keylessly by a `jdx/packslip` workflow, so trust flags are the whole
policy. `--pin` holds it to that repository, which a prefix alone does not
([Require the source repository](#require-the-source-repository)):

```sh
curl -fsSLO https://packslip.dev/.well-known/packslip.json
packslip verify packslip.json \
  --identity-prefix https://github.com/jdx/packslip/ \
  --issuer https://token.actions.githubusercontent.com \
  --pin ps1_mcx64bcghek2t4vljgb3lho3ti
```

For the list generated on 2026-10-21, it prints:

```text
ok: release list for packslip.dev sequence 1792590322 expires 2026-11-20T13:45:22.437645283Z signed by https://github.com/jdx/packslip/.github/workflows/packslip-releases.yml@refs/tags/v1.10.1 (sigstore-oidc) listing 14 release(s)
  repository https://github.com/jdx/packslip (id 1356006409), owner https://github.com/jdx (id 216188)
```

This checks the list's signature and structure. Trust flags work as for a
release, so a forge project's list needs none and a key-signed list takes
`--pubkey release.pub`. The `ok:` line ends with `, N yanked` when the
list withdraws releases. `--json` prints the verified list statement.

`verify` reports `expires_at` and `sequence` but does not compare them
with the current time or a list you accepted before; a consumer must
([consumer rule 5](/release/v1/#consumer-rules)). It also fetches neither
the list nor the release bundles it references, and refuses `--artifact`
for a list. To publish a list, see
[Manage release lists](/docs/release-lists/).

## Understand the result

The hk 2.3.0 check from
[Verify against the expected repository](#verify-against-the-expected-repository)
prints this, trimmed to one of its seven `requires` lines (one per artifact
that declares requirements):

```text
ok: github.com/jdx/hk 2.3.0 published 2026-09-26T19:59:21.295096144Z signed by https://github.com/jdx/hk/.github/workflows/release.yml@refs/tags/v2.3.0 (sigstore-oidc) logged 2026-09-26T19:59:21Z (1 of 7 artifact(s) and 0 of 5 asset(s) checked, provenance linked, 7 resource(s))
  repository https://github.com/jdx/hk (id 922514152), owner https://github.com/jdx (id 216188)
  requires hk-x86_64-pc-windows-msvc.zip: libs combase.dll vcruntime140.dll
```

The summary line's parts:

| Part | Meaning |
| --- | --- |
| `github.com/jdx/hk 2.3.0` | The statement's project and version. A prerelease adds `(prerelease)`, or `(beta prerelease)` when its prerelease part names a channel. |
| `published …` | The publish time. |
| `signed by … (sigstore-oidc)` | The certificate identity, or a key ID and `(sigstore-key)`, then `repackager-attested` if the statement declares a repackager made it. |
| `logged …` or `unlogged` | When the transparency log recorded the signature, or `unlogged` for a bundle accepted with `--allow-unlogged`. |
| `1 of 7 artifact(s) and 0 of 5 asset(s) checked` | Files passed with `--artifact`, of those listed; see [Check every file you use](#check-every-file-you-use). |
| `provenance linked` | Every artifact links build provenance. |
| `7 resource(s)` | Resources declared, such as completions and skills. |

The `repository` line appears when the certificate records a source
repository, as Fulcio's do for GitHub Actions and GitLab CI. It names the
repository the signing run was based on, with the forge's immutable IDs
for it and its owner (`source_repository` in `--json`).

`verify` checks the signature, a keyless bundle's certificate chain, the
transparency-log entry, and the statement's structure, including that its
`identity` block names the actual signer. `--trusted-root` replaces the
embedded Sigstore trust root behind the chain and log checks, for another
deployment or a newer root; see
[Trust-root lifetime and freshness](/docs/compatibility/#trust-root-lifetime-and-freshness).
A bundle without a log entry fails unless you pass `--allow-unlogged`; see
[Transparency log and unlogged bundles](/docs/trust/#transparency-log-and-unlogged-bundles).

Success establishes what
[What a verified release proves](/docs/trust/#what-a-verified-release-proves)
lists. It does not establish that linked provenance was fetched or
verified, or that this host meets the reported requirements. The
specification's full account is
[What a verified packslip proves](/release/v1/#what-a-verified-packslip-proves).

`packslip verify` checks one bundle or list per run and keeps no state.
An installer adds discovery, artifact selection, and remembered trust
([Build an installer or mirror](/docs/installers/),
[consumer rules](/release/v1/#consumer-rules)).
The Rust crate offers the same verifier and selection rules
([Use the Rust crate](/docs/rust/)).

## Troubleshoot a failure

A failed verification prints `verification failed:` and the reason to
standard error; a trust-flag conflict or unusable input prints `Error:`
and the reason. Both exit 1; an invalid command line exits 2.
`packslip pin` prints only the reason when it cannot fingerprint a
verified release. Compare the expected identity, repository,
file name, and digest with what the command reports, and never loosen your
flags just to pass.

| Error | What to check |
| --- | --- |
| `expected an identity starting with` or `identity mismatch: expected` | The signer does not match the prefix or identity you named. A standing policy needs `--identity-prefix`, since `--identity` includes the tag ref. If another repository's reusable workflow signed, use no trust flags or `--source-repository` ([Require the source repository](#require-the-source-repository)). After a rename, see [Pin a signer with its fingerprint](#pin-a-signer-with-its-fingerprint). |
| `must be signed from its repository`; from `packslip pin`, `the certificate's source repository is …, not`, `which is not a workflow of`, or `is signed through issuer` | Another repository or another CI issuer signed this forge project's release. Accept it only from a [repackager you trust](#verify-a-repackagers-release). |
| `issuer mismatch: expected` | A different OIDC issuer signed. Pass the issuer of the CI system that ran the release. With no trust flags, the release came from outside its forge's CI; do not use it. |
| `the certificate's source repository is …, expected` or `the certificate records no source repository` | Another repository signed, or the certificate does not say which. Check your `--source-repository` URL. |
| `the release is signed by ps1_…, but the pin is ps1_…` | Another repository signed, often one that took a renamed or deleted repository's name. Find where the project went; never replace your pin with the printed fingerprint. |
| `records no repository ID` | The certificate predates Fulcio's repository extensions or comes from another CI system. Use `--identity-prefix` and `--issuer`, not `--pin`. |
| `no identity to verify against` | A domain project: pass identity flags, or `--pubkey` for a key-signed release, which `packslip pin` cannot fingerprint. |
| `an identity was pinned; pin the key instead` or `a key was pinned; pin an identity instead` | `an identity was pinned`: the release is key-signed; pass `--pubkey` (`packslip pin` cannot fingerprint it). `a key was pinned`: it is keyless; use identity flags or `--pin`. |
| `signature does not verify with the pinned key` | Another key signed, or the bundle changed after signing. Get the vendor's current key from its own channel. |
| `no transparency log entry` | Pass `--allow-unlogged` only for a vendor that confirms it skipped logging on purpose. |
| `statement declares` | The statement's `identity` block names a signer other than the actual one. Do not use the release. |
| `not listed in the document` | Restore the file's original name. |
| `sha256 is` or `size is`, then `document says` | Check the file name and release version, then download again from the vendor's URL. |
| `needs --issuer`, `pins the repository, not a workflow`, `accepts a signer outside the project's repository`, `pins a key`, or `a key has none` | The trust flags conflict or are incomplete; each flag's section above says what it needs and refuses. |
| `a release list has no artifacts to check` or `a release list is not a release` | `verify --artifact` and `packslip pin` need a release's bundle. Verify the list without `--artifact`, then each release's bundle. |
