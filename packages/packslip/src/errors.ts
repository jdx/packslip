export type PackslipErrorCode =
  | 'bundle'
  | 'signature'
  | 'identity'
  | 'statement'
  | 'predicate-type'
  | 'project'
  | 'tag'
  | 'artifact-not-found'
  | 'digest-mismatch';

/** Base class of every error this package throws on purpose. */
export class PackslipError extends Error {
  readonly code: PackslipErrorCode;

  constructor(code: PackslipErrorCode, message: string, options?: ErrorOptions) {
    super(message, options);
    this.name = new.target.name;
    this.code = code;
  }
}

/** The input is not a sigstore bundle carrying a DSSE in-toto statement. */
export class BundleError extends PackslipError {
  constructor(message: string, options?: ErrorOptions) {
    super('bundle', message, options);
  }
}

/** Sigstore refused the signature, certificate chain, or transparency log entry. */
export class SignatureError extends PackslipError {
  constructor(message: string, options?: ErrorOptions) {
    super('signature', message, options);
  }
}

/** The signing certificate's identity or issuer is not the one the caller expects. */
export class IdentityError extends PackslipError {
  readonly actualIdentity: string | undefined;
  readonly actualIssuer: string | undefined;

  constructor(message: string, actual: { identity?: string | undefined; issuer?: string | undefined } = {}) {
    super('identity', message);
    this.actualIdentity = actual.identity;
    this.actualIssuer = actual.issuer;
  }
}

/** The signed statement is malformed or is not a packslip release statement. */
export class StatementError extends PackslipError {
  constructor(message: string, code: 'statement' | 'predicate-type' | 'project' | 'tag' = 'statement') {
    super(code, message);
  }
}

/** The statement does not list the requested asset. */
export class ArtifactNotFoundError extends PackslipError {
  readonly assetName: string;

  constructor(assetName: string) {
    super('artifact-not-found', `the release statement does not list ${JSON.stringify(assetName)}`);
    this.assetName = assetName;
  }
}

/** A downloaded file differs from the signed digest or size. */
export class DigestMismatchError extends PackslipError {
  readonly assetName: string;
  readonly field: 'sha256' | 'sha512' | 'size';
  readonly expected: string;
  readonly actual: string;

  constructor(assetName: string, field: 'sha256' | 'sha512' | 'size', expected: string, actual: string) {
    super('digest-mismatch', `${assetName}: ${field} is ${actual}, but the signed statement says ${expected}`);
    this.assetName = assetName;
    this.field = field;
    this.expected = expected;
    this.actual = actual;
  }
}
