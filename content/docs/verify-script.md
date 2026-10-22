---
title: Commit a verifier into your project
description: Verify keyless releases in CI or a bootstrap script with one dependency-free Node.js file that you review and commit.
group: consume
weight: 54
---
# Commit a verifier into your project

Where packslip cannot be installed, commit one reviewed JavaScript file that
verifies a keyless release bundle and its downloads in CI or a bootstrap
script. Where you can install packslip,
[Verify a release](/docs/verifying/) checks more; to verify from a Node.js
package, see [Verify from JavaScript](/docs/verify-js/).

## Copy the verifier

The verifier, `packslip-verify.mjs`, needs Node.js 20 or newer, imports only
Node.js built-in modules, and makes no network requests. There is no binary,
no `npm install`, and no bundled package to audit.

Copy it from the packslip repository at a release tag or a commit you have
reviewed, read it once, and commit it:

```sh
curl -fsSL --create-dirs -o scripts/packslip-verify.mjs \
  https://raw.githubusercontent.com/jdx/packslip/<full-commit-sha>/verifier/packslip-verify.mjs
```

Any later change to the file then shows up in code review.

## Verify a download

Pass the bundle, the signer policy you trust, and every file you downloaded.
This downloads hk 2.3.0's bundle and Linux x86_64 archive, then verifies them:

```sh
curl -fsSLO https://github.com/jdx/hk/releases/download/v2.3.0/packslip.sigstore.json
curl -fsSLO https://github.com/jdx/hk/releases/download/v2.3.0/hk-x86_64-unknown-linux-gnu.tar.gz
node scripts/packslip-verify.mjs packslip.sigstore.json \
  --issuer https://token.actions.githubusercontent.com \
  --identity-prefix https://github.com/jdx/hk/.github/workflows/ \
  --project github.com/jdx/hk \
  --tag v2.3.0 \
  --artifact hk-x86_64-unknown-linux-gnu.tar.gz
```

- `--issuer` and `--identity-prefix` are the
  [signer policy](#choose-the-signer-policy).
- `--project` requires the statement to name that project. For a forge
  project, it also [binds the signer to the project](#bind-the-signer-to-the-project).
- `--tag` requires the statement's source tag to be exactly that tag.
- `--artifact` is a downloaded artifact or resource asset, checked against
  its signed digests and, for an artifact, its size. Repeat it per file, and
  keep the published names: a renamed file is not listed in the statement.

On success, the verifier prints what it verified and exits 0:

```text
verified github.com/jdx/hk 2.3.0, signed by https://github.com/jdx/hk/.github/workflows/release.yml@refs/tags/v2.3.0
  ok  hk-x86_64-unknown-linux-gnu.tar.gz  sha256:79cabdccba39e1eef4e443e11713c923381f697b03c00ef4a12dfcd1ca5380b0
```

On a failure, it prints the reason to standard error and exits 1, for
example `verification failed: the signing identity … is not the expected one`.
A usage error, such as a missing policy flag, exits 2 with the usage text on
standard error.

With `--json`, it prints one JSON object on standard output instead. On
success it has `ok`, `project`, `version`, `signer`, and the checked `files`;
on failure, `ok`, `code`, and `error`. [What it checks](#what-it-checks)
gives each code. A usage error prints no JSON.

## Choose the signer policy

The policy always comes from you, never from the release. Pass `--issuer`,
the OIDC issuer the certificate must record, and exactly one of these:

| Flag | Accepts |
| --- | --- |
| `--identity` | One exact certificate identity, including the workflow's ref |
| `--identity-prefix` | Any identity that starts with the prefix; keep the trailing slash |
| `--source-repository` | Any workflow whose certificate records that repository URL, such as `https://github.com/owner/mytool`, as its source, including a reusable workflow of another repository that it calls |

GitHub and GitLab repository names in these flags and in `--project` match
in any letter case (`github.com/JDX/hk` names jdx/hk). Workflow paths, refs,
and monorepo subpaths match exactly.

## Bind the signer to the project

For a `github.com` or `gitlab.com` project, `--project` also requires the
certificate to come from that repository through the forge's OIDC issuer,
as `packslip verify` does. A reusable workflow of another repository may
sign; see
[What a project name says about the signer](/docs/trust/#what-a-project-name-says-about-the-signer).

The policy flags narrow this check and never replace it: with
`--project github.com/jdx/hk`, the prefix `https://github.com/jdx/` accepts
only bundles signed from jdx/hk. A jdx/hk bundle signed from another jdx
repository fails with `project-signer`. Without `--project`, or for a
domain project, the policy flags are the whole check, so pass `--project`
whenever you know it.

## Verify a repackager's release

A repackager's bundle names the vendor's project but is signed from the
repackager's repository, so `--project` refuses it for a forge project.
Name the repackager's signer with the policy flags and add
`--allow-other-repository`:

```sh
node scripts/packslip-verify.mjs packslip.sigstore.json \
  --issuer https://token.actions.githubusercontent.com \
  --identity-prefix https://github.com/repackager/builds/ \
  --project github.com/owner/mytool \
  --allow-other-repository \
  --artifact mytool-X.Y.Z-linux-x64.tar.gz
```

The statement must still name the project. `--allow-other-repository`
without `--project` is a usage error.

## Use it as a module

Import the verifier to act on the verified statement in your own script:

```js
import { readFile } from 'node:fs/promises';
import { identityHasPrefix, verifyFile, verifyRelease } from './packslip-verify.mjs';

const statement = verifyRelease(await readFile('packslip.sigstore.json'), {
  issuer: 'https://token.actions.githubusercontent.com',
  identity: (san) => identityHasPrefix(san, 'https://github.com/jdx/hk/.github/workflows/'),
  project: 'github.com/jdx/hk',
});
const file = await verifyFile(statement, 'hk-x86_64-unknown-linux-gnu.tar.gz');
console.log(statement.predicate.version, file.sha256);
```

`verifyRelease(bundle, options)` takes:

- `issuer` (required): the exact OIDC issuer.
- `identity`: an identity string, matched like `--identity`, or a function
  that receives the certificate's identity (its SAN) exactly as the
  certificate spells it and returns whether to accept it.
  `identityHasPrefix` matches like `--identity-prefix`.
- `sourceRepository`: as `--source-repository`. Pass it, `identity`, or
  both.
- `project`, `tag`, and `allowOtherRepository: true`: as the matching
  flags. `allowOtherRepository` needs `project`.
- `trustedRoot`: a trust root replacing the embedded one, shaped like the
  exported `TRUSTED_ROOT`.

It returns the signed statement plus a `signer` holding the certificate's
`identity` and `issuer`. It throws a `VerifyError` whose `code` names the
failed check, including `bundle` for a malformed bundle and `usage` for
missing options.
`verifyFile(statement, path)` resolves to the file's `name`, `size`, and
signed digests, and rejects with a `VerifyError` (code `file` when it cannot
read the download).

## What it checks

Each check's failure `code` is in parentheses. An unreadable, malformed, or
key-signed bundle fails with `bundle`. With `--json`, an unexpected failure
inside the verifier has the code `error`.

1. The Rekor log entry: its signed entry timestamp, that it logs this
   envelope and certificate, and an inclusion proof under a checkpoint the
   log signed (`signature`).
2. The Fulcio certificate: valid when the entry was logged, and chained to
   the embedded Sigstore public-good root (`signature`).
3. The signature over the statement (`signature`).
4. The certificate's identity, OIDC issuer, and source repository against
   your policy (`identity`).
5. The statement: its structure (`statement`), that it is a release, not a
   release list (`predicate-type`), its project and tag (`project`, `tag`),
   and that its `identity` block names the signing certificate's identity
   and issuer, as [consumer rule 2](/release/v1/#consumer-rules) requires
   (`declared-identity`).
6. For a forge `--project`, that the certificate comes from that
   repository, unless you pass `--allow-other-repository` (`project-signer`).
7. Each file: listed by name (`artifact-not-found`), readable (`file`), and
   matching its signed digests and, for an artifact, its size
   (`digest-mismatch`).

It does not check the certificate's signed certificate timestamp (SCT) or an
RFC 3161 timestamp. It cannot compare a signer with the last release you
accepted or use `--pin` signer fingerprints. It refuses bundles from a Rekor
log without signed entry timestamps; use `packslip verify` for those, for
key-signed bundles, and for release lists.

## Update the verifier

The embedded trust root is a snapshot of Sigstore's `trusted_root.json`
(Fulcio certificate chains and Rekor keys). After Sigstore rotates a key, a
stale copy fails new bundles with the code `signature` and a message such as
`the certificate does not chain to the sigstore Fulcio root` or
`the log entry is from a log this verifier does not trust`. Then replace the
file with a newer copy and review the change.
[Trust-root lifetime and freshness](/docs/compatibility/#trust-root-lifetime-and-freshness)
describes the same limit for `packslip verify`.
