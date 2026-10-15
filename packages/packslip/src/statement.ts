import { StatementError } from './errors.ts';

/** The predicate type of a release statement. Release lists (`releases/v1`) are not releases. */
export const RELEASE_PREDICATE_TYPE = 'https://packslip.dev/release/v1';

const STATEMENT_TYPES = ['https://in-toto.io/Statement/v1'];

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

/** A statement whose signature, signer, and structure have been checked. */
export interface VerifiedStatement {
  _type: string;
  subject: Subject[];
  predicateType: typeof RELEASE_PREDICATE_TYPE;
  predicate: ReleasePredicate;
  /** What the signing certificate says about the signer. */
  signer: { identity: string; issuer: string };
}

const SHA256 = /^[0-9a-f]{64}$/;
const SHA512 = /^[0-9a-f]{128}$/;

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}


function parseStatement(payload: string): Record<string, unknown> {
  let statement: unknown;
  try {
    statement = JSON.parse(Buffer.from(payload, 'base64').toString('utf8'));
  } catch (cause) {
    throw new StatementError('the signed payload is not valid JSON');
  }
  if (!isObject(statement)) throw new StatementError('the signed payload is not an object');
  return statement;
}

function parseSubjects(value: unknown): Subject[] {
  if (!Array.isArray(value) || value.length === 0) throw new StatementError('the statement has no subjects');
  const seen = new Set<string>();
  return value.map((entry) => {
    const digest = isObject(entry) ? entry['digest'] : undefined;
    if (!isObject(entry) || typeof entry['name'] !== 'string' || !isObject(digest)) {
      throw new StatementError('a subject needs a name and a digest');
    }
    const name = entry['name'];
    if (seen.has(name)) throw new StatementError(`the statement lists ${JSON.stringify(name)} twice`);
    seen.add(name);
    const sha256 = digest['sha256'];
    const sha512 = digest['sha512'];
    if (typeof sha256 !== 'string' || !SHA256.test(sha256)) {
      throw new StatementError(`${name}: sha256 must be 64 lowercase hexadecimal characters`);
    }
    if (sha512 !== undefined && (typeof sha512 !== 'string' || !SHA512.test(sha512))) {
      throw new StatementError(`${name}: sha512 must be 128 lowercase hexadecimal characters`);
    }
    return { name, digest: sha512 === undefined ? { sha256 } : { sha256, sha512 } };
  });
}

export function readStatement(
  payload: string,
  expect: { project?: string | undefined; tag?: string | undefined },
): Omit<VerifiedStatement, 'signer'> {
  const statement = parseStatement(payload);
  const type = statement['_type'];
  if (typeof type !== 'string' || !STATEMENT_TYPES.includes(type)) {
    throw new StatementError('the signed payload is not an in-toto statement');
  }
  if (statement['predicateType'] !== RELEASE_PREDICATE_TYPE) {
    throw new StatementError(
      `the predicate type is ${JSON.stringify(statement['predicateType'])}, not ${RELEASE_PREDICATE_TYPE}`,
      'predicate-type',
    );
  }
  const predicate = statement['predicate'];
  if (
    !isObject(predicate) ||
    typeof predicate['project'] !== 'string' ||
    typeof predicate['version'] !== 'string' ||
    !Array.isArray(predicate['artifacts'])
  ) {
    throw new StatementError('the predicate needs a project, a version, and artifacts');
  }
  for (const artifact of predicate['artifacts']) {
    if (
      !isObject(artifact) ||
      typeof artifact['name'] !== 'string' ||
      (artifact['size'] !== undefined && !(Number.isSafeInteger(artifact['size']) && (artifact['size'] as number) >= 0))
    ) {
      throw new StatementError('each artifact needs a name and, if present, a non-negative integer size');
    }
  }
  const subject = parseSubjects(statement['subject']);

  if (expect.project !== undefined && predicate['project'] !== expect.project) {
    throw new StatementError(
      `the statement is for ${predicate['project']}, not ${expect.project}`,
      'project',
    );
  }
  if (expect.tag !== undefined) {
    const source = predicate['source'];
    const tag = isObject(source) ? source['tag'] : undefined;
    if (tag !== expect.tag) {
      throw new StatementError(
        `the statement's source tag is ${JSON.stringify(tag)}, not ${JSON.stringify(expect.tag)}`,
        'tag',
      );
    }
  }

  return { _type: type, subject, predicateType: RELEASE_PREDICATE_TYPE, predicate: predicate as ReleasePredicate };
}

export interface ArtifactDigest {
  name: string;
  sha256: string;
  sha512?: string;
  /** The artifact's size in bytes, when the statement lists the asset as an artifact. */
  size?: number;
}
