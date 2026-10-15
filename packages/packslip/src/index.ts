import { createReadStream } from 'node:fs';
import { createHash } from 'node:crypto';
import { verify as sigstoreVerify, type Bundle, type VerifyOptions } from 'sigstore';
import {
  ArtifactNotFoundError,
  BundleError,
  DigestMismatchError,
  IdentityError,
  SignatureError,
} from './errors.ts';
import { readStatement, type ArtifactDigest, type VerifiedStatement } from './statement.ts';

export * from './errors.ts';
export {
  RELEASE_PREDICATE_TYPE,
  type ArtifactDigest,
  type ReleaseArtifactInfo,
  type ReleasePredicate,
  type Subject,
  type VerifiedStatement,
} from './statement.ts';

export interface VerifyReleaseOptions {
  /** The exact certificate SAN URI, or a predicate that accepts it. */
  identity: string | ((identity: string) => boolean);
  /** The exact OIDC issuer of the certificate. */
  issuer: string;
  /** Require the statement to name this project, such as `github.com/jdx/mise`. */
  project?: string;
  /** Require the statement's `source.tag`, such as `v2026.10.11`. */
  tag?: string;
  /** Passed to sigstore: a TUF mirror, root, or cache, and retry or timeout settings. */
  sigstore?: Pick<
    VerifyOptions,
    'tufMirrorURL' | 'tufRootPath' | 'tufCachePath' | 'tufForceCache' | 'retry' | 'timeout'
  >;
}

const DSSE_PAYLOAD_TYPE = 'application/vnd.in-toto+json';

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function parseBundle(bundleJson: string | Uint8Array | object): Bundle & {
  dsseEnvelope: { payload: string; payloadType: string };
} {
  let value: unknown = bundleJson;
  // Always work on a private copy: the caller's object must not be able to change
  // between sigstore verifying the bundle and this package reading its statement.
  try {
    if (typeof bundleJson === 'string' || bundleJson instanceof Uint8Array) {
      value = JSON.parse(typeof bundleJson === 'string' ? bundleJson : Buffer.from(bundleJson).toString('utf8'));
    } else {
      value = JSON.parse(JSON.stringify(bundleJson));
    }
  } catch (cause) {
    throw new BundleError('the bundle is not valid JSON', { cause });
  }
  if (!isObject(value) || !isObject(value['dsseEnvelope'])) {
    throw new BundleError('the bundle has no DSSE envelope');
  }
  const envelope = value['dsseEnvelope'];
  if (envelope['payloadType'] !== DSSE_PAYLOAD_TYPE || typeof envelope['payload'] !== 'string') {
    throw new BundleError(`the DSSE payload type must be ${DSSE_PAYLOAD_TYPE}`);
  }
  return value as never;
}

/** The certificate's SAN URI, from the leaf the bundle carries. */
function signerFromSigstore(signer: { identity?: { subjectAlternativeName?: string; extensions?: { issuer?: string } } }) {
  return { identity: signer.identity?.subjectAlternativeName, issuer: signer.identity?.extensions?.issuer };
}

/**
 * Verify a release bundle (the contents of `packslip.sigstore.json`) against a
 * signer the caller already trusts, and return its statement.
 *
 * The bundle must carry a Fulcio certificate and a transparency log entry that
 * sigstore's public-good instance accepts. Key-signed bundles are refused.
 */
export async function verifyRelease(
  bundleJson: string | Uint8Array | object,
  options: VerifyReleaseOptions,
): Promise<VerifiedStatement> {
  const bundle = parseBundle(bundleJson);

  let signer;
  try {
    signer = await sigstoreVerify(bundle, { ...options.sigstore });
  } catch (cause) {
    const message = cause instanceof Error ? cause.message : String(cause);
    throw new SignatureError(`sigstore refused the bundle: ${message}`, { cause });
  }

  const { identity, issuer } = signerFromSigstore(signer);
  if (identity === undefined || issuer === undefined) {
    throw new IdentityError('the bundle is not signed with a certificate that names an identity and issuer');
  }
  const identityOk =
    typeof options.identity === 'string' ? identity === options.identity : options.identity(identity);
  if (!identityOk) {
    throw new IdentityError(`the signing identity ${identity} is not the expected one`, { identity, issuer });
  }
  if (issuer !== options.issuer) {
    throw new IdentityError(`the certificate issuer ${issuer} is not the expected ${options.issuer}`, {
      identity,
      issuer,
    });
  }

  const statement = readStatement(bundle.dsseEnvelope.payload, options);

  return { ...statement, signer: { identity, issuer } };
}

/** Look up an asset in a verified statement by file name. */
export function findArtifact(statement: VerifiedStatement, assetName: string): ArtifactDigest {
  const subject = statement.subject.find((entry) => entry.name === assetName);
  if (subject === undefined) throw new ArtifactNotFoundError(assetName);
  const info = statement.predicate.artifacts.find((entry) => isObject(entry) && entry.name === assetName);
  const artifact: ArtifactDigest = { name: subject.name, sha256: subject.digest.sha256 };
  if (subject.digest.sha512 !== undefined) artifact.sha512 = subject.digest.sha512;
  if (info !== undefined && typeof info.size === 'number') artifact.size = info.size;
  return artifact;
}

/**
 * Stream a downloaded file and compare it with the signed digest (and the
 * signed size, when the statement has one). Throws {@link DigestMismatchError}.
 */
export async function verifyFileDigest(path: string, artifact: ArtifactDigest): Promise<void> {
  const sha256 = createHash('sha256');
  const sha512 = artifact.sha512 === undefined ? undefined : createHash('sha512');
  let size = 0;
  for await (const chunk of createReadStream(path)) {
    sha256.update(chunk as Buffer);
    sha512?.update(chunk as Buffer);
    size += (chunk as Buffer).length;
  }
  if (artifact.size !== undefined && size !== artifact.size) {
    throw new DigestMismatchError(artifact.name, 'size', String(artifact.size), String(size));
  }
  const actual256 = sha256.digest('hex');
  if (actual256 !== artifact.sha256) {
    throw new DigestMismatchError(artifact.name, 'sha256', artifact.sha256, actual256);
  }
  if (sha512 !== undefined && artifact.sha512 !== undefined) {
    const actual512 = sha512.digest('hex');
    if (actual512 !== artifact.sha512) {
      throw new DigestMismatchError(artifact.name, 'sha512', artifact.sha512, actual512);
    }
  }
}
