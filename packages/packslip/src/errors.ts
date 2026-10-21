export type PackslipErrorCode =
  | 'bundle'
  | 'signature'
  | 'identity'
  | 'statement'
  | 'declared-identity'
  | 'project-signer'
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

/**
 * The signed statement is not a packslip release statement (`predicate-type`),
 * breaks the release/v1 structural rules (`statement`), or names another project
 * or tag than the caller requires (`project`, `tag`).
 */
export class StatementError extends PackslipError {
  constructor(message: string, code: 'statement' | 'predicate-type' | 'project' | 'tag' = 'statement') {
    super(code, message);
  }
}

/**
 * The statement's `identity` block does not describe the certificate that
 * signed it: it declares a key-signed release (`sigstore-key`), another signer,
 * or another issuer.
 */
export class DeclaredIdentityError extends PackslipError {
  /** The field of the statement's `predicate.identity` that differs. */
  readonly field: 'scheme' | 'key_id' | 'issuer';
  /** What the statement declares. */
  readonly declared: string;
  /** What the signing certificate says. */
  readonly actual: string;

  constructor(field: 'scheme' | 'key_id' | 'issuer', declared: string, actual: string) {
    super(
      'declared-identity',
      field === 'scheme'
        ? `the statement declares scheme ${declared}, but the bundle was signed by an OIDC identity`
        : field === 'key_id'
          ? `the statement declares signer ${JSON.stringify(declared)}, but the bundle was signed by ${JSON.stringify(actual)}`
          : `the statement declares issuer ${JSON.stringify(declared)}, but the certificate says ${JSON.stringify(actual)}`,
    );
    this.field = field;
    this.declared = declared;
    this.actual = actual;
  }
}

/**
 * The certificate does not belong to the GitHub or GitLab repository that the
 * requested `project` names. It came through another issuer (`issuer`), records
 * another source repository (`source-repository`), or records none and was
 * signed outside the repository (`signer`).
 */
export class ProjectSignerError extends PackslipError {
  readonly project: string;
  readonly reason: 'issuer' | 'source-repository' | 'signer';
  readonly actualIdentity: string;
  readonly actualIssuer: string;
  /** The Source Repository URI the certificate records, if any. */
  readonly sourceRepository: string | undefined;

  constructor(
    message: string,
    details: {
      project: string;
      reason: 'issuer' | 'source-repository' | 'signer';
      identity: string;
      issuer: string;
      sourceRepository: string | undefined;
    },
  ) {
    super('project-signer', message);
    this.project = details.project;
    this.reason = details.reason;
    this.actualIdentity = details.identity;
    this.actualIssuer = details.issuer;
    this.sourceRepository = details.sourceRepository;
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
