// What the signing certificate says, checked against the statement's own
// `identity` block (`check_declared` in the Rust crate's src/verify.rs) and
// against the forge project the caller asked for (`forge::check_source` in
// src/forge.rs, as `packslip verify` applies it to the statement's project).

import { DeclaredIdentityError, IdentityError, ProjectSignerError } from './errors.ts';
import type { Identity } from './model.ts';
import { identityHasPrefix, sameIdentity } from './project.ts';

export const GITHUB_ISSUER = 'https://token.actions.githubusercontent.com';
export const GITLAB_ISSUER = 'https://gitlab.com';

// Fulcio's Source Repository URI extension: the repository whose CI run
// requested the certificate, whichever workflow signed.
const OID_SOURCE_REPOSITORY_URI = '1.3.6.1.4.1.57264.1.12';

/** The parts of sigstore's `Signer` this package reads. */
export interface SigstoreSigner {
  identity?: {
    subjectAlternativeName?: string;
    extensions?: { issuer?: string };
    oids?: { oid?: { id: number[] } | undefined; value: Uint8Array }[];
  };
}

export interface CertificateSigner {
  /** The certificate's SAN: a workflow URI for a CI identity, else an email. */
  identity: string;
  issuer: string;
  /** The Source Repository URI, when the certificate records a non-empty one. */
  sourceRepository: string | undefined;
}

/** What a verified sigstore certificate says about its signer. */
export function certificateSigner(signer: SigstoreSigner): CertificateSigner {
  const identity = signer.identity?.subjectAlternativeName;
  const issuer = signer.identity?.extensions?.issuer;
  if (identity === undefined || issuer === undefined) {
    throw new IdentityError('the bundle is not signed with a certificate that names an identity and issuer');
  }
  const extension = signer.identity?.oids?.find((entry) => entry.oid?.id.join('.') === OID_SOURCE_REPOSITORY_URI);
  let sourceRepository: string | undefined;
  if (extension !== undefined) {
    const text = derUtf8String(extension.value);
    if (text === undefined) {
      throw new IdentityError(`the certificate's Source Repository URI extension is malformed`, { identity, issuer });
    }
    // An empty value means the forge had none to give.
    sourceRepository = text === '' ? undefined : text;
  }
  return { identity, issuer, sourceRepository };
}

/** The text of a DER UTF8String, or undefined when the bytes are not exactly one. */
function derUtf8String(der: Uint8Array): string | undefined {
  if (der[0] !== 0x0c || der.length < 2) return undefined;
  let length = der[1] ?? 0;
  let offset = 2;
  if (length & 0x80) {
    const octets = length & 0x7f;
    if (octets === 0 || octets > 3 || der.length < 2 + octets) return undefined;
    length = 0;
    for (let i = 0; i < octets; i++) length = length * 256 + (der[2 + i] ?? 0);
    offset += octets;
  }
  if (offset + length !== der.length) return undefined;
  try {
    return new TextDecoder('utf-8', { fatal: true }).decode(der.subarray(offset));
  } catch {
    return undefined;
  }
}

/**
 * Check the statement's `identity` block against the certificate that signed
 * it. This package verifies keyless bundles only, so the scheme must be
 * `sigstore-oidc`, `key_id` the certificate's identity, and `issuer` the
 * certificate's issuer. `checkStatement` has already refused a sigstore-oidc
 * identity without an issuer.
 */
export function checkDeclaredIdentity(declared: Identity, signer: { identity: string; issuer: string }): void {
  if (declared.scheme !== 'sigstore-oidc') {
    throw new DeclaredIdentityError('scheme', declared.scheme, 'sigstore-oidc');
  }
  if (declared.key_id !== signer.identity) {
    throw new DeclaredIdentityError('key_id', declared.key_id, signer.identity);
  }
  if (declared.issuer !== signer.issuer) {
    throw new DeclaredIdentityError('issuer', declared.issuer ?? '', signer.issuer);
  }
}

/** A GitHub or GitLab project name split along its forge's layout. */
function forgeRepository(project: string): { host: 'github.com' | 'gitlab.com'; repository: string } | undefined {
  const [host, ...path] = project.split('/');
  if (host === 'github.com') {
    // `github.com/owner/repo`, with an optional monorepo subpath after it that
    // names a tool of the repository.
    const [owner, repo] = path;
    return owner && repo ? { host, repository: `${owner}/${repo}` } : undefined;
  }
  // GitLab subgroups make paths arbitrary depth, so the whole path is the repository.
  if (host === 'gitlab.com' && path.length > 0 && path.every((segment) => segment !== '')) {
    return { host, repository: path.join('/') };
  }
  return undefined;
}

/**
 * Check that the certificate belongs to the repository a GitHub or GitLab
 * `project` names, as the specification's Project names and Repository
 * identity sections require: the forge's issuer, and a Source Repository URI
 * that is the project's repository. The signer may then be a reusable workflow
 * of another repository. A certificate that records no Source Repository URI
 * must instead be signed by a workflow of the repository. The repository's name
 * compares without regard to ASCII case, as the forge resolves it. Any other
 * project has no repository to compare, so the caller's identity policy is all
 * there is.
 */
export function checkProjectSigner(project: string, signer: CertificateSigner): void {
  const forge = forgeRepository(project);
  if (forge === undefined) return;
  const details = { project, identity: signer.identity, issuer: signer.issuer, sourceRepository: signer.sourceRepository };
  const issuer = forge.host === 'gitlab.com' ? GITLAB_ISSUER : GITHUB_ISSUER;
  if (signer.issuer !== issuer) {
    throw new ProjectSignerError(`${project} is signed through issuer ${JSON.stringify(signer.issuer)}, expected ${issuer}`, {
      ...details,
      reason: 'issuer',
    });
  }
  const repository = `https://${forge.host}/${forge.repository}`;
  if (signer.sourceRepository === undefined) {
    const prefix = forge.host === 'gitlab.com' ? `${repository}//` : `${repository}/`;
    if (!identityHasPrefix(signer.identity, prefix)) {
      throw new ProjectSignerError(`signed by ${signer.identity}, which is not a workflow of ${repository}`, {
        ...details,
        reason: 'signer',
      });
    }
  } else if (!sameIdentity(signer.sourceRepository, repository)) {
    throw new ProjectSignerError(`the certificate's source repository is ${signer.sourceRepository}, not ${repository}`, {
      ...details,
      reason: 'source-repository',
    });
  }
}
