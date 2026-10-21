import { createReadStream } from 'node:fs';
import { createHash } from 'node:crypto';
import { verify as sigstoreVerify, type Bundle, type VerifyOptions } from 'sigstore';
import { ArtifactNotFoundError, BundleError, DigestMismatchError, SignatureError } from './errors.ts';
import { checkOptions, checkSigned } from './policy.ts';
import { certificateSigner } from './signer.ts';
import type { ArtifactDigest, VerifiedStatement } from './statement.ts';

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
  /**
   * The certificate SAN URI, or a predicate that accepts it. A github.com or
   * gitlab.com repository's name in the URI matches in any case, as the forge
   * resolves it; the rest, such as the workflow's path and ref, matches exactly.
   */
  identity: string | ((identity: string) => boolean);
  /** The exact OIDC issuer of the certificate. */
  issuer: string;
  /**
   * Require the statement to name this project, such as `github.com/jdx/mise`.
   * For a `github.com` or `gitlab.com` project, also require the certificate to
   * belong to that repository: issued through the forge's OIDC issuer, and
   * recording the repository as its source repository (a reusable workflow of
   * another repository may sign) or, when it records none, signed by a workflow
   * of the repository. A github.com or gitlab.com repository's name matches in
   * any case, as the forge resolves it.
   */
  project?: string;
  /**
   * Accept a signer outside the repository a `github.com` or `gitlab.com`
   * `project` names, such as a repackager's, as `packslip verify
   * --allow-other-repository` does. The statement must still name `project`,
   * which this option needs. `identity` and `issuer` alone then decide which
   * signer is accepted, so name that signer there: a predicate that accepts
   * any identity accepts any signer from the issuer.
   */
  allowOtherRepository?: boolean;
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

/**
 * Verify a release bundle (the contents of `packslip.sigstore.json`) against a
 * signer the caller already trusts, and return its statement.
 *
 * The bundle must carry a Fulcio certificate and a transparency log entry that
 * sigstore's public-good instance accepts. Key-signed bundles are refused. Once
 * the signature and the caller's identity and issuer check out, the statement
 * must follow the release/v1 structural rules, and its `predicate.identity`
 * must name the certificate's identity and issuer. Then come the optional
 * `project` and `tag` checks, and, unless `allowOtherRepository` is set, the
 * check that a GitHub or GitLab `project` was signed from its repository.
 * Options that need another option throw a `TypeError` before verification.
 */
export async function verifyRelease(
  bundleJson: string | Uint8Array | object,
  options: VerifyReleaseOptions,
): Promise<VerifiedStatement> {
  checkOptions(options);
  const bundle = parseBundle(bundleJson);

  let signer;
  try {
    signer = await sigstoreVerify(bundle, { ...options.sigstore });
  } catch (cause) {
    const message = cause instanceof Error ? cause.message : String(cause);
    throw new SignatureError(`sigstore refused the bundle: ${message}`, { cause });
  }

  return checkSigned(certificateSigner(signer), bundle.dsseEnvelope.payload, options);
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
