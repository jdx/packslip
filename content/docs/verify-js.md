---
title: Verify a release from JavaScript
weight: 62
group: consume
description: Check a release bundle and the files you downloaded from Node.js with the packslip npm package, for GitHub Actions and other JavaScript tools.
---
# Verify a release from JavaScript

The `packslip` npm package verifies a release bundle in Node.js, for a tool
that cannot run the `packslip` CLI, such as a JavaScript GitHub Action. It
applies the [consumer rules](/release/v1/#consumer-rules) for a bundle you
already trust a signer for: the signature, certificate and transparency log
entry through the official [`sigstore`](https://www.npmjs.com/package/sigstore)
package, then the statement, then each downloaded file's digest. It needs
Node.js 22.22.2 or later in 22, 24.15.0 or later in 24, or Node.js 26 or newer.

The package does no downloading. You fetch the bundle (`packslip.sigstore.json`
on a release) and the assets yourself; the only network access is sigstore's
fetch of the public-good trust root.

```ts
import { findArtifact, verifyFileDigest, verifyRelease } from 'packslip';

const statement = await verifyRelease(bundleJson, {
  identity: 'https://github.com/jdx/mise/.github/workflows/release.yml@refs/tags/v2026.10.11',
  issuer: 'https://token.actions.githubusercontent.com',
  project: 'github.com/jdx/mise',
  tag: 'v2026.10.11',
});

const artifact = findArtifact(statement, 'mise-v2026.10.11-linux-arm64');
await verifyFileDigest('/tmp/mise-v2026.10.11-linux-arm64', artifact);
```

`verifyFileDigest` streams the file and throws if its size, SHA-256, or
SHA-512 differs from the signed values.

## Choose the signer policy

`identity` and `issuer` are required, because the policy must come from you,
never from the bundle. `identity` is the exact certificate identity, or a
function that returns `true` for an acceptable one:

```ts
identity: (san) => san.startsWith('https://github.com/jdx/mise/.github/workflows/'),
```

For a GitHub project, the issuer is `https://token.actions.githubusercontent.com`.
The package keeps no state, so it cannot compare a signer with the last
release you accepted ([consumer rule 3](/release/v1/#consumer-rules)). Pin
the exact identity when you can, or keep that record yourself.

`project` and `tag` are optional checks of the signed statement. Pass them
when you know which release you asked for, so a validly signed bundle from
another project or release is refused.

## What it refuses

`verifyRelease` throws a subclass of `PackslipError`, each with a `code`:

| Error | `code` | Cause |
| --- | --- | --- |
| `BundleError` | `bundle` | Not JSON, or no DSSE in-toto envelope. |
| `SignatureError` | `signature` | Sigstore rejected the signature, certificate chain, or log entry, including a bundle with no log entry or an altered statement. |
| `IdentityError` | `identity` | The certificate identity or issuer is not the one you require. |
| `StatementError` | `statement`, `predicate-type`, `project`, `tag` | The statement is malformed, is not a `release/v1` statement (a `releases/v1` release list is refused), or names another project or tag. |

`findArtifact` throws `ArtifactNotFoundError` for an asset the statement does
not list, and `verifyFileDigest` throws `DigestMismatchError`.

Only keyless bundles signed through the public-good sigstore instance are
supported. Verify key-signed releases with `packslip verify`.
