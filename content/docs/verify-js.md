---
title: Verify from JavaScript
weight: 64
group: build
description: Verify a keyless release bundle and the files you downloaded from Node.js code, such as a JavaScript GitHub Action, with the packslip npm package.
---
# Verify from JavaScript

Authors of Node.js tools that cannot run the packslip CLI, such as a
JavaScript GitHub Action, can verify releases with the `packslip` npm package.
By the end, your code checks a release bundle and every file it downloaded.
From a shell, see [Verify a release](/docs/verifying/); in CI with no
dependencies, see [Commit a verifier into your project](/docs/verify-script/).

## Install the package

```sh
npm install packslip
```

It needs Node.js 22.22.2 or later in 22.x, 24.15.0 or later in 24.x, or
Node.js 26 or newer. It is an ES module and ships TypeScript types.

## Verify a bundle and a download

The package downloads nothing from the release. Fetch the bundle (`packslip.sigstore.json` on
the release) and the assets you need:

```sh
curl -fsSLO https://github.com/jdx/hk/releases/download/v2.3.0/packslip.sigstore.json
curl -fsSLO https://github.com/jdx/hk/releases/download/v2.3.0/hk-x86_64-unknown-linux-gnu.tar.gz
```

Save this as `verify.mjs`:

```js
import { readFile } from 'node:fs/promises';
import { findArtifact, verifyFileDigest, verifyRelease } from 'packslip';

const bundleJson = await readFile('packslip.sigstore.json', 'utf8');
const statement = await verifyRelease(bundleJson, {
  identity: 'https://github.com/jdx/hk/.github/workflows/release.yml@refs/tags/v2.3.0',
  issuer: 'https://token.actions.githubusercontent.com',
  project: 'github.com/jdx/hk',
  tag: 'v2.3.0',
});

const artifact = findArtifact(statement, 'hk-x86_64-unknown-linux-gnu.tar.gz');
await verifyFileDigest('hk-x86_64-unknown-linux-gnu.tar.gz', artifact);
console.log(`verified ${statement.predicate.project} ${statement.predicate.version}, signed by ${statement.signer.identity}`);
```

Run it:

```sh
node verify.mjs
```

```text
verified github.com/jdx/hk 2.3.0, signed by https://github.com/jdx/hk/.github/workflows/release.yml@refs/tags/v2.3.0
```

- `verifyRelease(bundleJson, options)` takes the bundle as a string, a
  `Uint8Array`, or a parsed object. It returns the signed statement plus
  `signer`, the identity and issuer the certificate records.
- `findArtifact(statement, name)` returns the `name` and signed `sha256` of
  any asset in the statement, such as a completion script, plus `sha512` and
  `size` when the statement records them.
- `verifyFileDigest(path, artifact)` streams the file and throws if its size,
  SHA-256, or SHA-512 differs from the signed values.

`verifyRelease` checks a bundle against a signer you already trust, in this
order:

1. The signature, certificate, and transparency log entry, through the official
   [`sigstore`](https://www.npmjs.com/package/sigstore) package
   ([Configure the trust root](#configure-the-trust-root)).
2. The certificate's signer, against the required `identity` and `issuer`
   ([Choose the signer policy](#choose-the-signer-policy)).
3. The statement: the [release statement](/release/v1/#the-release-statement)
   rules, and that its `identity` block names the certificate's signer.
4. The optional `project` and `tag`
   ([Check the project and tag](#check-the-project-and-tag)).
5. For a forge `project`, that the certificate comes from its repository,
   unless you set `allowOtherRepository`
   ([Accept a repackager's release](#accept-a-repackagers-release)).

## Choose the signer policy

`identity` and `issuer` are required because the signer policy must come from
you, never from the bundle. `identity` is the exact certificate identity, as
above, or a function that returns `true` to accept one, such as this one for
any tag of the release workflow:

```js
identity: (san) => san.startsWith('https://github.com/jdx/hk/.github/workflows/release.yml@'),
```

In a string `identity` and in `project`, a GitHub or GitLab repository's name
matches in any letter case. The workflow's path and ref match exactly. A
function receives the identity as the certificate spells it.

`issuer` is the exact OIDC issuer: `https://token.actions.githubusercontent.com`
for a GitHub project and `https://gitlab.com` for a GitLab project.

The package keeps no record of accepted releases, so it cannot compare a
signer with the last one ([consumer rule 3](/release/v1/#consumer-rules)).
Accept only the exact identity, built from the tag you pass, or at least the
workflow prefix shown above. Or keep that record yourself, as
[Signer continuity and workflow pinning](/docs/trust/#signer-continuity-and-workflow-pinning)
describes.

## Check the project and tag

`project` and `tag` (the statement's `source.tag`) are optional checks of the
signed statement. Pass them when you know which release you asked for, so a
validly signed bundle from another project or release is refused.

For a forge project, `project` also requires a certificate from that
repository, issued through the forge's issuer; otherwise `verifyRelease`
throws `ProjectSignerError`. The signer may be a reusable workflow of another
repository.
[What a project name says about the signer](/docs/trust/#what-a-project-name-says-about-the-signer)
and [Repository identity and renames](/release/v1/#repository-identity-and-renames)
explain the binding.

`identity` narrows this check and never replaces it. For a domain project,
`identity` and `issuer` are the whole signer policy.

## Accept a repackager's release

A [repackager](/release/v1/#repackager-attestation)'s release names the
vendor's project but is signed from the repackager's repository, so `project`
refuses it for a forge project. To accept it, set `allowOtherRepository: true`,
as `packslip verify --allow-other-repository` does; the statement must still
name `project`. The option needs `project` and `identity`; without either,
`verifyRelease` throws a `TypeError` before reading the bundle.

**Warning:** With `allowOtherRepository`, `identity` and `issuer` alone decide
which signer is accepted. Name the repackager's signer there; a function that
accepts any identity accepts any signer from the issuer.

```js
const statement = await verifyRelease(bundleJson, {
  identity: (san) => san.startsWith('https://github.com/repackager/builds/.github/workflows/'),
  issuer: 'https://token.actions.githubusercontent.com',
  project: 'github.com/jdx/hk',
  allowOtherRepository: true,
});
```

## Handle a failure

Each function throws a subclass of `PackslipError`, with a `code`, when it
refuses a release or a file. Catch the base class to report any refusal:

```js
import { readFile } from 'node:fs/promises';
import { PackslipError, verifyRelease } from 'packslip';

const bundleJson = await readFile('packslip.sigstore.json', 'utf8');
try {
  await verifyRelease(bundleJson, {
    identity: (san) => san.startsWith('https://github.com/jdx/hk/.github/workflows/release.yml@'),
    issuer: 'https://token.actions.githubusercontent.com',
    project: 'github.com/jdx/hk',
    tag: 'v2.4.0',
  });
} catch (error) {
  if (!(error instanceof PackslipError)) throw error;
  console.error(`packslip refused the release (${error.code}): ${error.message}`);
  process.exitCode = 1;
}
```

Run on the hk v2.3.0 bundle, it prints:

```text
packslip refused the release (tag): the statement's source tag is "v2.3.0", not "v2.4.0"
```

- `BundleError` (`bundle`): the input is not JSON, or has no DSSE envelope
  with an in-toto payload.
- `SignatureError` (`signature`): `sigstore` rejected the signature,
  certificate chain, or log entry. Causes include no log entry, an altered
  statement, a key-signed bundle, and an unfetchable trust root.
- `IdentityError` (`identity`): the certificate identity or issuer differs
  from what you require. `actualIdentity` and `actualIssuer` hold the
  certificate's.
- `StatementError` (`statement`, `predicate-type`, `project`, `tag`): the
  statement breaks a [release statement](/release/v1/#the-release-statement)
  rule, is not `release/v1` (a `releases/v1` release list is refused), or
  names another project or tag.
- `DeclaredIdentityError` (`declared-identity`): the statement's `identity`
  declares another scheme, signer, or issuer than the certificate's
  (properties `field`, `declared`, `actual`).
- `ProjectSignerError` (`project-signer`): the certificate is not from the
  forge repository `project` names. `reason` is `issuer`, `source-repository`,
  or `signer` (no source repository recorded, and not a workflow of the
  repository).
- `ArtifactNotFoundError` (`artifact-not-found`): `findArtifact` got an asset
  the statement does not list.
- `DigestMismatchError` (`digest-mismatch`): `verifyFileDigest` found a
  different size, SHA-256, or SHA-512 (properties `field`, `expected`,
  `actual`).

A file `verifyFileDigest` cannot read rejects with Node.js's own error, such
as `ENOENT`, not a `PackslipError`.

## Configure the trust root

The package's only network access is the `sigstore` package's TUF fetch of
the public-good trust root. By default it checks for an update on each call
and caches the result on disk. `verifyRelease`'s `sigstore` option, such as
`sigstore: { tufCachePath: '.cache/sigstore', timeout: 10000 }`, changes that:

- `tufMirrorURL`: the TUF repository. Default:
  `https://tuf-repo-cdn.sigstore.dev`.
- `tufRootPath`: a `root.json` to start from when the cache is empty. A
  non-default mirror needs one.
- `tufCachePath`: the cache directory. Default: `sigstore-js` in the user's
  data directory.
- `tufForceCache`: use the cache while it is valid; fetch only when it is
  missing or expired.
- `retry`: retries after a failed fetch. Default: 2.
- `timeout`: milliseconds per fetch. Default: 5000.

## Know the limits

- It does not discover releases or select a platform's artifact; your
  application does. It does not create or sign statements.
- It supports only keyless releases with a log entry from the public-good
  sigstore instance. Verify key-signed releases and release lists with
  `packslip verify`; see [Verify a release](/docs/verifying/).
- It reads no repository ID and takes no signer fingerprint (`ps1_…`), so
  `project` and `identity` compare names only. A repository recreated under
  the project's name passes
  ([First use and signer fingerprints](/docs/trust/#first-use-and-signer-fingerprints)).
  A release signed before a rename names the old project, so a `project`
  with the new name refuses it. To hold a project to its repository, use
  `packslip verify --pin`
  ([Pin a signer with its fingerprint](/docs/verifying/#pin-a-signer-with-its-fingerprint))
  or the Rust crate's stored pin
  ([Hold a project to its signer](/docs/rust/#hold-a-project-to-its-signer)).
- It accepts two spellings `packslip verify` refuses: a field the format
  defines repeated in one object, such as two `version` keys (it keeps the
  last), and a whole-number `size` written as `5.0`, `1e3`, or `-0`.
