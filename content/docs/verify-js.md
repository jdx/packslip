---
title: Verify a release from JavaScript
weight: 62
group: consume
description: Check a release bundle and the files you downloaded from Node.js with the packslip npm package, for GitHub Actions and other JavaScript tools.
---
# Verify a release from JavaScript

The `packslip` npm package verifies a release bundle in Node.js, for a tool
that cannot run the `packslip` CLI, such as a JavaScript GitHub Action. It
applies [consumer rule 2](/release/v1/#consumer-rules) to a bundle whose
signer you already trust. It checks the signature, certificate and
transparency log entry through the official
[`sigstore`](https://www.npmjs.com/package/sigstore) package. Then it checks
the statement against the [release statement](/release/v1/#the-release-statement)
rules and against the signer the statement declares, and finally each
downloaded file's digest and size. It needs Node.js 22.22.2 or later in 22,
24.15.0 or later in 24, or Node.js 26 or newer.

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
another project or release is refused. For a `github.com` or `gitlab.com`
project, `project` also requires the certificate to come from that
repository's CI, through the forge's issuer. When the certificate records the
repository as its source repository, a reusable workflow of another
repository may sign; when it records none, a workflow of the repository must.
`identity` narrows this requirement and never replaces it.

A repackager's release names the vendor's project but is signed from the
repackager's repository, so `project` refuses it. To accept one, set
`allowOtherRepository: true`, as `packslip verify --allow-other-repository`
does: the statement must still name `project`, but the certificate may come
from another repository. `identity` and `issuer` then alone decide which
signer is accepted, so name the repackager's signer there; a predicate that
accepts any identity would accept any signer from the issuer.

```ts
const statement = await verifyRelease(bundleJson, {
  identity: (san) => san.startsWith('https://github.com/repackager/builds/.github/workflows/'),
  issuer: 'https://token.actions.githubusercontent.com',
  project: 'github.com/jdx/mise',
  allowOtherRepository: true,
});
```

`allowOtherRepository` needs `project`; without one, `verifyRelease` throws a
`TypeError` before it reads the bundle.

## What it refuses

`verifyRelease` throws a subclass of `PackslipError`, each with a `code`:

| Error | `code` | Cause |
| --- | --- | --- |
| `BundleError` | `bundle` | Not JSON, or no DSSE in-toto envelope. |
| `SignatureError` | `signature` | Sigstore rejected the signature, certificate chain, or log entry, including a bundle with no log entry or an altered statement. |
| `IdentityError` | `identity` | The certificate identity or issuer is not the one you require. |
| `StatementError` | `statement`, `predicate-type`, `project`, `tag` | The statement breaks a [release statement](/release/v1/#the-release-statement) rule, is not a `release/v1` statement (a `releases/v1` release list is refused), or names another project or tag. |
| `DeclaredIdentityError` | `declared-identity` | The statement's `identity` declares another scheme, signer, or issuer than the certificate's. |
| `ProjectSignerError` | `project-signer` | The certificate does not come from the GitHub or GitLab repository that `project` names. |

`findArtifact` throws `ArtifactNotFoundError` for an asset the statement does
not list, and `verifyFileDigest` throws `DigestMismatchError`.

The package reads the statement with `JSON.parse`, which keeps each value but
not how it was written. So two spellings that `packslip verify` refuses get
through: a field named twice in one object, which keeps its last value, and a
whole-number `size` written as a decimal or with an exponent, such as `5.0`.
A `size` that is not a whole number, such as `5.5`, is refused.

Only keyless bundles signed through the public-good sigstore instance are
supported. Verify key-signed releases with `packslip verify`.
