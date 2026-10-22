# packslip

Verify a packslip release bundle (`packslip.sigstore.json`), its signed
statement, and the files it lists, from Node.js tools that cannot run the
`packslip` CLI, such as a GitHub Action. To sign releases, see
[Publish with GitHub Actions](https://packslip.dev/docs/publishing/); to verify
from a shell, see [Verify a release](https://packslip.dev/docs/verifying/).

## Install

```sh
npm install packslip
```

It needs Node.js `^22.22.2 || ^24.15.0 || >=26`, is an ES module with
TypeScript types, and depends only on
[`sigstore`](https://www.npmjs.com/package/sigstore).

## Verify a download

The package downloads nothing: your code fetches the files, as `download` does
below. Its only network access is `sigstore` fetching the public-good trust
root. Save this as `verify.mjs` and run `node verify.mjs` to verify hk 2.8.0's
Linux x86_64 archive.

```js
import { writeFile } from 'node:fs/promises';
import { findArtifact, verifyFileDigest, verifyRelease } from 'packslip';

const release = 'https://github.com/jdx/hk/releases/download/v2.8.0';
const asset = 'hk-x86_64-unknown-linux-gnu.tar.gz';

async function download(name) {
  const response = await fetch(`${release}/${name}`);
  if (!response.ok) throw new Error(`${name}: HTTP ${response.status}`);
  return new Uint8Array(await response.arrayBuffer());
}

const statement = await verifyRelease(await download('packslip.sigstore.json'), {
  project: 'github.com/jdx/hk',
  identity: 'https://github.com/jdx/hk/.github/workflows/release.yml@refs/tags/v2.8.0',
  issuer: 'https://token.actions.githubusercontent.com',
  tag: 'v2.8.0',
});

await writeFile(asset, await download(asset));
await verifyFileDigest(asset, findArtifact(statement, asset));
console.log(`verified ${asset} from ${statement.predicate.project} ${statement.predicate.version}`);
```

It prints:

```text
verified hk-x86_64-unknown-linux-gnu.tar.gz from github.com/jdx/hk 2.8.0
```

`verifyRelease` takes the bundle as a string, `Uint8Array`, or parsed object;
wrap an `ArrayBuffer` in `new Uint8Array()`, as `download` does. It returns the
statement, with the certificate's identity and issuer in `statement.signer`.
Its options:

- `identity` (required): the exact certificate identity, or a function
  returning `true` for an acceptable one.
- `issuer` (required): the exact OIDC issuer, such as
  `https://token.actions.githubusercontent.com` or `https://gitlab.com`.
- `project` and `tag`: the project and `source.tag` the statement must name.
- `allowOtherRepository`: accept a
  [repackager's release](#accept-a-repackagers-release).
- `sigstore`: TUF, retry, and timeout settings for `sigstore`.

## Choose the signer policy

The policy comes from you, never from the bundle. For a forge project
(`github.com/owner/repo` or `gitlab.com/group/project`), `project` also
requires a certificate from that repository, or a reusable workflow it calls,
through the forge's issuer. `identity` narrows this. A domain project, such as
`mytool.example.com`, has no repository, so `identity` and `issuer` are the
whole policy.

A string `identity` matches exactly, so the example's identity accepts only
`v2.8.0`. To accept any tag of hk's release workflow, pass a function:

```js
identity: (san) => san.startsWith('https://github.com/jdx/hk/.github/workflows/release.yml@refs/tags/'),
```

Only a forge repository name, in `project` or a string `identity`, matches in
any letter case; a function sees the certificate's spelling.

**Warning:** Always pass `project`, and name the signing workflow in
`identity`. Without `project`, a statement for another project passes whenever
`identity` accepts its signer. An `identity` that accepts anything passes any
workflow run of the repository on any ref, even a pull request. For a domain
project or with `allowOtherRepository`, such an `identity` passes any signer
from the issuer.

## Accept a repackager's release

A repackager re-signs another project's release from its own repository, so
the repository check refuses it. To accept it, set
`allowOtherRepository: true`, as `packslip verify --allow-other-repository`
does, and name the repackager's workflow in `identity`. The statement must
still name `project`, which the option requires.

## Handle errors

Every failed check throws a `PackslipError` subclass with a `code`. Misused
options, such as `allowOtherRepository` without `project`, throw a `TypeError`
first; an unreadable file rejects with Node.js's own error.

| Error | `code` | Cause |
| --- | --- | --- |
| `BundleError` | `bundle` | Not JSON, or no DSSE envelope with an in-toto payload. |
| `SignatureError` | `signature` | `sigstore` rejected the signature, certificate, or log entry, or could not load the trust root. An altered statement fails here. |
| `IdentityError` | `identity` | The certificate lacks the identity or issuer you passed, or has a malformed Source Repository URI. |
| `StatementError` | `statement`, `predicate-type`, `project`, `tag` | Not a valid [release statement](https://packslip.dev/release/v1/#the-release-statement), or names another project or tag. |
| `DeclaredIdentityError` | `declared-identity` | The statement declares another scheme, signer, or issuer than the certificate's. |
| `ProjectSignerError` | `project-signer` | The certificate is not from the forge project's repository. |
| `ArtifactNotFoundError` | `artifact-not-found` | `findArtifact`: the statement does not list the file. |
| `DigestMismatchError` | `digest-mismatch` | `verifyFileDigest`: the file's size or a digest differs. |

## Limits

- No discovery, artifact selection, release lists, or key-signed bundles;
  `packslip verify` checks the last two.
- No state, so it cannot compare a signer with the last one you accepted, as
  the [consumer rules](https://packslip.dev/release/v1/#consumer-rules) require.
- No signer fingerprint (`ps1_…`) check, so a repository recreated under the
  same name passes; `packslip verify` can
  [pin a signer with its fingerprint](https://packslip.dev/docs/verifying/#pin-a-signer-with-its-fingerprint).
- No equivalent of `packslip verify --source-repository`, and the result omits
  the certificate's source repository. When a GitHub run calls a reusable
  workflow, the certificate names that workflow, not the caller. So for a
  domain project or with `allowOtherRepository`, any repository calling the
  workflow in `identity` passes.
- `JSON.parse` reads the statement, so a field named twice keeps its last value
  and a `size` of `5.0`, `1e3`, or `-0` passes. `packslip verify` refuses
  these.

## Learn more

- [Verify from JavaScript](https://packslip.dev/docs/verify-js/): the full API,
  trust-root settings, and each error's fields.
- [How trust works](https://packslip.dev/docs/trust/): what a verified bundle
  establishes and does not.

MIT licensed. Source:
[packages/packslip](https://github.com/jdx/packslip/tree/main/packages/packslip).
