import { StatementError } from './errors.ts';
import { checkStatement, RELEASE_PREDICATE_TYPE, type Statement } from './model.ts';
import { sameProject } from './project.ts';

export { RELEASE_PREDICATE_TYPE };

export interface Subject {
  name: string;
  digest: { sha256: string; sha512?: string };
}

export interface ReleaseArtifactInfo {
  name: string;
  size?: number;
  [key: string]: unknown;
}

export interface ReleasePredicate {
  project: string;
  version: string;
  source?: { repo?: string; commit?: string; tag?: string };
  artifacts: ReleaseArtifactInfo[];
  [key: string]: unknown;
}

/**
 * A statement whose signature, signer, and structure have been checked: it
 * follows the release/v1 rules, and its `predicate.identity` describes the
 * certificate that signed it.
 */
export interface VerifiedStatement {
  _type: string;
  subject: Subject[];
  predicateType: typeof RELEASE_PREDICATE_TYPE;
  predicate: ReleasePredicate;
  /** What the signing certificate says about the signer. */
  signer: { identity: string; issuer: string };
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function parsePayload(payload: string): Record<string, unknown> {
  let statement: unknown;
  try {
    statement = JSON.parse(Buffer.from(payload, 'base64').toString('utf8'));
  } catch {
    throw new StatementError('the signed payload is not valid JSON');
  }
  if (!isObject(statement)) throw new StatementError('the signed payload is not an object');
  return statement;
}

/**
 * Decode a DSSE payload and check it against the release/v1 rules. Returns the
 * statement as this package exposes it, with the predicate as it was signed,
 * and the parsed model the signer checks read.
 */
export function readRelease(payload: string): { statement: Omit<VerifiedStatement, 'signer'>; model: Statement } {
  const raw = parsePayload(payload);
  const model = checkStatement(raw);
  const subject = model.subject.map(({ name, digest: { sha256, sha512 } }) => ({
    name,
    digest: sha512 === undefined ? { sha256 } : { sha256, sha512 },
  }));
  return {
    statement: {
      _type: model._type,
      subject,
      predicateType: RELEASE_PREDICATE_TYPE,
      predicate: raw['predicate'] as ReleasePredicate,
    },
    model,
  };
}

/**
 * Require the statement to name the project and source tag the caller asked
 * for. A GitHub or GitLab repository's name matches in any case, as the
 * forge resolves it.
 */
export function checkExpected(model: Statement, expect: { project?: string | undefined; tag?: string | undefined }): void {
  if (expect.project !== undefined && !sameProject(model.predicate.project, expect.project)) {
    throw new StatementError(`the statement is for ${model.predicate.project}, not ${expect.project}`, 'project');
  }
  if (expect.tag !== undefined) {
    const tag = model.predicate.source?.tag;
    if (tag !== expect.tag) {
      throw new StatementError(
        `the statement's source tag is ${JSON.stringify(tag)}, not ${JSON.stringify(expect.tag)}`,
        'tag',
      );
    }
  }
}

/** {@link readRelease} and {@link checkExpected}, without any signer checks. */
export function readStatement(
  payload: string,
  expect: { project?: string | undefined; tag?: string | undefined },
): Omit<VerifiedStatement, 'signer'> {
  const { statement, model } = readRelease(payload);
  checkExpected(model, expect);
  return statement;
}

export interface ArtifactDigest {
  name: string;
  sha256: string;
  sha512?: string;
  /** The artifact's size in bytes, when the statement lists the asset as an artifact. */
  size?: number;
}
