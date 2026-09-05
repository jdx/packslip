---
title: Verify a release
weight: 40
description: Verify bundles and downloaded artifacts, choose a trust pin, and understand what a consumer must check.
---
# Verify a release

Verification checks a release against an identity or public key you
trust. Download the bundle and the artifacts you intend to use, then
pass the local files to `packslip verify` before unpacking or running them.

## Verify against the expected repository

For a GitHub release, explicitly pin the repository you meant to download:

```sh
packslip verify packslip.sigstore.json \
  --identity-prefix https://github.com/owner/repo/ \
  --issuer https://token.actions.githubusercontent.com \
  --artifact mytool-1.2.3-linux-x64.tar.gz
```

Keep the trailing slash on the repository prefix. For a narrower pin,
use `--identity` with the exact certificate identity instead.

Without identity flags, the CLI derives a policy from the bundle's
claimed project on GitHub or GitLab. That checks whether the signer
belongs to the project the document names. It does **not** establish that
this is the project or version you intended to install. Check those
values against your request as well.

## Verify against a public key

For a key-signed release, obtain the public key through a trusted channel:

```sh
packslip verify packslip.sigstore.json \
  --pubkey release.pub \
  --artifact mytool-1.2.3-linux-x64.tar.gz
```

`--pubkey` also accepts the public key's base64 line. Do not treat a key
hint inside the bundle as a trust pin. Use `--allow-unlogged` only when
you have chosen to accept unlogged signatures for this publisher.

## Check every file you use

Repeat `--artifact` to check multiple files, including separate resource assets:

```sh
packslip verify packslip.sigstore.json \
  --pubkey release.pub \
  --artifact mytool-1.2.3-linux-x64.tar.gz \
  --artifact mytool.cdx.json \
  --json
```

The command checks signatures and applicable certificate/log material,
validates the statement, and compares supplied files with the signed
subject digests. It also checks sizes for artifacts. Preserve the original
filenames so they match the statement's subjects.

Without `--artifact`, success verifies the bundle alone. It does not
fetch, hash, or install remote artifacts. `--json` returns a report for
scripts; verification failures exit with status 1.

## Verify a release list

The same command accepts a signed release list:

```sh
packslip verify packslip.json --pubkey release.pub
```

This checks its signature and structure. The CLI does not compare the
expiry with the current time or remember previously accepted sequences;
consumers must enforce those checks. It also does not fetch or verify
the release bundles the list references. `--artifact` is not accepted
for a release list.

## Understand the result

| A successful verification establishes… | It does not establish… |
| --- | --- |
| The statement was signed by an identity or key allowed by the policy. | The signer or its build environment was uncompromised. |
| Supplied files match the signed digests. | The software is safe or free of vulnerabilities. |
| A logged signature has verified transparency-log evidence. | This is the newest release or the vendor's recommended version. |
| The statement contains provenance links, if reported. | The linked provenance has been fetched or verified. |

`packslip show BUNDLE` prints the statement without verifying it. Use it
for inspection, not as evidence of authenticity.

## Build an installer or mirror

The CLI verifies individual documents. A consumer also needs discovery,
selection, and remembered policy. Implement the full
[consumer rules](/release/v1/#consumer-rules), including:

1. Match the verified project and version to the user's request and the
   accepted release-list entry or tag.
2. Verify signed lists, enforce expiry, and persist their highest accepted
   sequence. Once a signed list has been accepted, its disappearance is
   an error rather than permission to ignore withdrawals.
3. Remember the accepted signer and trust properties. Refuse unapproved
   signer changes, weaker signing, vendor-to-repackager changes, or lost
   provenance links. A workflow's tag ref can change without changing
   the workflow's identity for this comparison.
4. Apply any release-age policy to the verified log time, using the signed
   publication time only for an explicitly accepted unlogged bundle.
5. Select the artifact for the host and variant, then check host
   requirements. Ambiguous artifacts are an error.
6. Verify every downloaded artifact and resource asset before using it.
   Select resources for the chosen artifact and executable, and follow
   the execution rules for generated resources.

### Take the crate as a library

The `packslip` crate holds the schema, the verifier, and the selection
rules as well as the CLI and the generator. A consumer that only verifies
takes it without the parts it will not call:

```sh
cargo add packslip --no-default-features
```

That leaves the statement types, `verify`, `verify_release_list`,
`select_artifact`, and `select_resources`, and drops the archive readers,
the executable decoder that derives `requires.libs`, the signing path, the
JSON Schema generator, and the CLI: about seventy fewer crates in the
dependency graph. The features are additive and all on by default, so the
binary and any dependent that says nothing is unaffected:

| Feature | Adds |
| --- | --- |
| `cli` (default) | The `packslip` binary; implies the rest. |
| `create` | Build a statement from built artifacts; implies `archive`, `linkage`, and `sign`. |
| `archive` | Read tar and zip archives to resolve declared executable paths. |
| `linkage` | Derive `requires.libs` from ELF, Mach-O, and PE executables. |
| `sign` | Sign statements, keylessly through Fulcio or with a minisign key. |
| `manifest` | Read a `packslip.toml`. |
| `schema` | `Statement::schema()` and `ReleaseListStatement::schema()`. |

A lockfile can carry a project's signer commitment alongside artifact
URLs and digests so another machine can enforce it on its first install.
Local state can separately remember signer history and release-list
sequences. The format does not prescribe where a consumer stores either.

These distinctions are illustrated by the
[mise integration](/docs/mise/). For discovery and version policy, continue
with [Manage release lists](/docs/release-lists/).

## Troubleshoot a failure

| Failure | Check |
| --- | --- |
| Identity mismatch | Expected repository/workflow and issuer; do not change the pin just to make the command pass. |
| Missing public-key policy | Supply the trusted key for a key-signed release. |
| Unlogged bundle refused | Confirm whether the publisher intentionally omitted logging. |
| Digest or size mismatch | Confirm the original filename and release version, then obtain a fresh copy from the publisher. |
| Expired, rolled-back, or missing signed list | Obtain a current list; retain the existing trust state while investigating. |
| Provenance reported as linked | Verify those statements separately if your policy requires build provenance. |
