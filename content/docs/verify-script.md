---
title: Commit a verifier into your project
weight: 63
group: consume
description: Check a keyless release bundle and the files you downloaded with one readable Node.js file you commit next to your build, with no packslip install and no dependencies.
---
# Commit a verifier into your project

`packslip-verify.mjs` is a single JavaScript file that verifies a keyless
release bundle and the files you downloaded from it. Commit it into your
repository, read it once, and run it in CI or a bootstrap script. It needs
only Node.js 20 or newer, uses only built-in modules, and makes no network
requests, so there is no binary to trust, no `npm install`, and no bundle to
diff. Use it where you cannot install packslip. Where you can, the
[`packslip verify`](/docs/verifying/) command checks more.

Copy the file from the repository at a release tag or commit you have
reviewed, and commit it:

```sh
curl -fsSLo scripts/packslip-verify.mjs \
  https://raw.githubusercontent.com/jdx/packslip/COMMIT/verifier/packslip-verify.mjs
```

## Verify a download

Pass the bundle, the signer policy you trust, and every downloaded file:

```sh
node scripts/packslip-verify.mjs packslip.sigstore.json \
  --issuer https://token.actions.githubusercontent.com \
  --identity-prefix https://github.com/jdx/mise/.github/workflows/ \
  --project github.com/jdx/mise \
  --tag v2026.10.11 \
  --artifact mise-v2026.10.11-linux-arm64
```

On success it prints what it verified and exits 0:

```text
verified github.com/jdx/mise 2026.10.11, signed by https://github.com/jdx/mise/.github/workflows/release.yml@refs/tags/v2026.10.11
  ok  mise-v2026.10.11-linux-arm64  sha256:370b4b4cfd740402e9a55aa1a8a01fbb1ac95c50af1207b2db2c5489dd4da0cd
```

On a failure it prints the reason and exits 1, for example
`verification failed: the signing identity … is not the expected one`.
Usage errors exit 2. `--json` prints one JSON object with `ok`, and on failure
a `code` such as `signature`, `identity`, `digest-mismatch`, or
`artifact-not-found`.

The policy always comes from you. `--issuer` is required together with one of:

| Flag | Accepts |
| --- | --- |
| `--identity` | One exact certificate identity, including the workflow's ref |
| `--identity-prefix` | Any identity starting with the prefix; keep the trailing slash |
| `--source-repository` | Any workflow whose certificate records that repository, for reusable workflows |

`--project` and `--tag` optionally require the signed statement to name that
project and source tag. As with `packslip verify`, keep the downloaded file
names: a renamed file is `not listed` in the statement.

## Use it as a module

```js
import { verifyFile, verifyRelease } from './packslip-verify.mjs';

const statement = verifyRelease(await readFile('packslip.sigstore.json'), {
  issuer: 'https://token.actions.githubusercontent.com',
  identity: (san) => san.startsWith('https://github.com/jdx/mise/.github/workflows/'),
  project: 'github.com/jdx/mise',
});
await verifyFile(statement, 'mise-v2026.10.11-linux-arm64');
```

`verifyRelease` throws a `VerifyError` whose `code` names the failed check,
and `verifyFile` rejects with one.

## What it checks

1. The Rekor log entry: its signed timestamp, that it logs this envelope and
   this certificate, and an inclusion proof under a checkpoint the log signed.
2. The Fulcio certificate, valid when the entry was logged, chained to the
   sigstore public-good root embedded in the file.
3. The signature over the statement.
4. The certificate's identity and OIDC issuer against your policy.
5. The statement's structure, then each file's signed size and digests.

It does **not** check the certificate's signed certificate timestamp or an
RFC 3161 timestamp, and it refuses key-signed bundles and ones from a Rekor
log that does not provide a signed entry timestamp: use `packslip verify` for
those. It cannot compare a signer with the last release you accepted or use
`--pin` signer fingerprints.

The embedded trust root is a snapshot of sigstore's `trusted_root.json`. A
sigstore key rotation makes a stale copy refuse new bundles, so replace the
file with a newer copy then. Because you commit the file, a change to it
shows up in review.
