// What verifyRelease checks once sigstore has verified a bundle: the caller's
// signer policy, then the statement, the signer it declares, and the requested
// project and tag. It is kept apart from the sigstore call so the tests can run
// it on certificate fields that no published bundle carries, such as a
// repackager's signature for another repository's project.

import { IdentityError } from './errors.ts';
import type { VerifyReleaseOptions } from './index.ts';
import { checkDeclaredIdentity, checkProjectSigner, type CertificateSigner } from './signer.ts';
import { checkExpected, readRelease, type VerifiedStatement } from './statement.ts';

/**
 * Refuse options that cannot mean what the caller wants, before any network
 * access. `allowOtherRepository` lifts the repository requirement a GitHub or
 * GitLab `project` brings, as `packslip verify --allow-other-repository` does,
 * so it needs a `project`, and an `identity` (with the required `issuer`) that
 * names the signer it accepts instead.
 */
export function checkOptions(options: VerifyReleaseOptions): void {
  if (options.allowOtherRepository !== true) return;
  if (options.project === undefined) {
    throw new TypeError('allowOtherRepository lifts the repository requirement a project brings, so it needs a project');
  }
  if (typeof options.identity !== 'string' && typeof options.identity !== 'function') {
    throw new TypeError('allowOtherRepository needs an identity that names the signer it accepts');
  }
}

/**
 * Check a certificate sigstore has verified, and the statement it signed,
 * against the caller's options, and return the statement with its signer.
 */
export function checkSigned(
  certificate: CertificateSigner,
  payload: string,
  options: VerifyReleaseOptions,
): VerifiedStatement {
  const { identity, issuer } = certificate;
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

  const { statement, model } = readRelease(payload);
  checkDeclaredIdentity(model.predicate.identity, certificate);
  checkExpected(model, options);
  // A GitHub or GitLab project must be signed from its repository, whatever
  // `identity` accepts, unless the caller accepts a signer outside it.
  if (options.project !== undefined && options.allowOtherRepository !== true) {
    checkProjectSigner(options.project, certificate);
  }

  return { ...statement, signer: { identity, issuer } };
}
