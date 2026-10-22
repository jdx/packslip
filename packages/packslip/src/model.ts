// The release/v1 statement and its structural rules, ported from the Rust
// crate's `Statement::validate` (src/model.rs) by way of the Go module's
// `Statement.Validate` (go/model.go). `checkStatement` reads a decoded payload
// the way serde reads it there, then applies the same rules, so a statement the
// reference implementation refuses is refused here too. The conformance
// vectors in tests/conformance/statement-validity.json run against it.
//
// Like the Go module, it also refuses four things that the Rust crate in
// packslip 1.10.1 accepts: an object written as an array of its fields, a
// timestamp that is not RFC 3339, an executable with an empty name, and a
// sigstore-oidc identity without a key_id or an issuer.

import { StatementError } from './errors.ts';

export const STATEMENT_TYPE = 'https://in-toto.io/Statement/v1';
/** The predicate type of a release statement. Release lists (`releases/v1`) are not releases. */
export const RELEASE_PREDICATE_TYPE = 'https://packslip.dev/release/v1';

// Semver 2.0.0, as its specification spells the grammar.
const SEMVER =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$/;
// An `os`, `arch`, `libc`, `format`, or `variant` value.
const TOKEN = /^[a-z0-9][a-z0-9_.-]*$/;
const SHA256 = /^[0-9a-f]{64}$/;
const SHA512 = /^[0-9a-f]{128}$/;
// RFC 3339 with any offset, the space separator its section 5.6 allows, and at
// most nanosecond precision. The Rust crate's parser (jiff) also takes some ISO
// 8601 spellings that are not RFC 3339, such as a `+0200` offset or no seconds;
// that is a leniency of that crate, so like the Go module this refuses them.
const RFC3339 = /^(\d{4})-(\d{2})-(\d{2})[Tt ](\d{2}):(\d{2}):(\d{2})(?:\.\d{1,9})?(?:[Zz]|[+-](\d{2}):(\d{2}))$/;
// Formats whose artifact is one executable rather than an archive.
const BARE_FORMATS = ['raw', 'gz', 'xz', 'zst', 'bz2'];

export type Extensions = Record<string, unknown>;
export interface Digest {
  sha256: string;
  sha512: string | undefined;
}
export interface Subject {
  name: string;
  digest: Digest;
}
export interface Bin {
  path: string;
  name: string;
}
export interface RequiredBin {
  name: string;
  min: string | undefined;
}
export interface Requires {
  os_min: string | undefined;
  glibc_min: string | undefined;
  libs: string[] | undefined;
  bin: RequiredBin[];
}
export interface Artifact {
  name: string;
  os: string | undefined;
  arch: string | undefined;
  libc: string | undefined;
  variant: string | undefined;
  size: number;
  url: string | undefined;
  format: string | undefined;
  bin: Bin[];
  requires: Requires | undefined;
  provenance: string[];
  extensions: Extensions;
}
export interface Resource {
  kind: string;
  artifact: string | undefined;
  os: string | undefined;
  arch: string | undefined;
  libc: string | undefined;
  shell: string | undefined;
  shells: string[];
  name: string | undefined;
  bin: string | undefined;
  format: string | undefined;
  archive: string | undefined;
  asset: string | undefined;
  url: string | undefined;
  repo: string | undefined;
  exec: string[];
  env: Record<string, string>;
  extensions: Extensions;
}
export type Scheme = 'sigstore-oidc' | 'sigstore-key';
export interface Identity {
  scheme: Scheme;
  key_id: string;
  issuer: string | undefined;
  pin_workflow: boolean | undefined;
}
export interface Source {
  repo: string;
  commit: string | undefined;
  tag: string | undefined;
}
export interface Evidence {
  kind: string;
  detail: string | undefined;
}
export interface Predicate {
  project: string;
  version: string;
  published_at: string;
  source: Source | undefined;
  artifacts: Artifact[];
  resources: Resource[];
  identity: Identity;
  attested_by: 'vendor' | 'repackager';
  evidence: Evidence[];
  notes_url: string | undefined;
  extensions: Extensions;
}
export interface Statement {
  _type: string;
  subject: Subject[];
  predicateType: string;
  predicate: Predicate;
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

const q = (value: unknown) => JSON.stringify(value);

// ---------------------------------------------------------------------------
// Reading: what serde enforces in the Rust crate. A required field must be
// present with its type; an optional scalar may also be null; a list or map
// that defaults to empty may be absent but not null. Unknown fields are
// ignored, so a later revision of version 1 can add optional ones.
//
// This reads what JSON.parse returns, which keeps each value but not how it was
// written, so two things serde_json sees in the text cannot be seen here. A
// field named twice in one object keeps its last value, where the Rust crate
// refuses a duplicate field (the Go module also keeps the last one). And a
// whole-number size written as a decimal or with an exponent, such as `5.0` or
// `1e3`, reads as that number, where both reference implementations refuse it.
// ---------------------------------------------------------------------------

class Fields {
  readonly #value: Record<string, unknown>;
  readonly #path: string;

  constructor(value: unknown, path: string) {
    if (!isObject(value)) throw new StatementError(`${path === '' ? 'the statement' : path} must be an object`);
    this.#value = value;
    this.#path = path;
  }

  at(key: string): string {
    return this.#path === '' ? key : `${this.#path}.${key}`;
  }

  get(key: string): unknown {
    return Object.hasOwn(this.#value, key) ? this.#value[key] : undefined;
  }

  #required(key: string): unknown {
    const value = this.get(key);
    if (value === undefined) throw new StatementError(`${this.at(key)} is required`);
    return value;
  }

  string(key: string): string {
    const value = this.#required(key);
    if (typeof value !== 'string') throw new StatementError(`${this.at(key)} must be a string`);
    return value;
  }

  optionalString(key: string): string | undefined {
    const value = this.get(key);
    if (value === undefined || value === null) return undefined;
    if (typeof value !== 'string') throw new StatementError(`${this.at(key)} must be a string`);
    return value;
  }

  optionalBoolean(key: string): boolean | undefined {
    const value = this.get(key);
    if (value === undefined || value === null) return undefined;
    if (typeof value !== 'boolean') throw new StatementError(`${this.at(key)} must be a boolean`);
    return value;
  }

  // A whole number of bytes. Number.isSafeInteger also refuses a size above
  // 2^53 - 1, which the Rust crate's u64 takes, because no JavaScript number
  // holds it exactly for comparison with a downloaded file's size.
  size(key: string): number {
    const value = this.#required(key);
    if (!Number.isSafeInteger(value) || (value as number) < 0) {
      throw new StatementError(`${this.at(key)} must be a non-negative integer`);
    }
    return value as number;
  }

  object(key: string): Fields {
    return new Fields(this.#required(key), this.at(key));
  }

  optionalObject(key: string): Fields | undefined {
    const value = this.get(key);
    return value === undefined || value === null ? undefined : new Fields(value, this.at(key));
  }

  list<T>(key: string, each: (item: unknown, path: string) => T, required = false): T[] {
    const value = required ? this.#required(key) : this.get(key);
    if (value === undefined) return [];
    if (!Array.isArray(value)) throw new StatementError(`${this.at(key)} must be an array`);
    return Array.from(value, (item: unknown, i) => each(item, `${this.at(key)}[${i}]`));
  }

  optionalList<T>(key: string, each: (item: unknown, path: string) => T): T[] | undefined {
    const value = this.get(key);
    return value === undefined || value === null ? undefined : this.list(key, each);
  }

  strings(key: string): string[] {
    return this.list(key, stringAt);
  }

  map(key: string): Record<string, unknown> {
    const value = this.get(key);
    if (value === undefined) return {};
    if (!isObject(value)) throw new StatementError(`${this.at(key)} must be an object`);
    return value;
  }

  stringMap(key: string): Record<string, string> {
    const value = this.map(key);
    for (const [name, entry] of Object.entries(value)) {
      if (typeof entry !== 'string') throw new StatementError(`${this.at(key)}.${name} must be a string`);
    }
    return value as Record<string, string>;
  }
}

function stringAt(value: unknown, path: string): string {
  if (typeof value !== 'string') throw new StatementError(`${path} must be a string`);
  return value;
}

function readSubject(value: unknown, path: string): Subject {
  const fields = new Fields(value, path);
  const digest = fields.object('digest');
  return {
    name: fields.string('name'),
    digest: { sha256: digest.string('sha256'), sha512: digest.optionalString('sha512') },
  };
}

// A string names the executable at that path under its own file name; an
// object gives both. Either way a name ending in `.exe` (in any case) is read as
// the name without it, as the Rust crate's `Bin` reads it: `bin/tool.exe` and
// `{ "path": "tool-x64.exe", "name": "tool.exe" }` both name the command `tool`.
function readBin(value: unknown, path: string): Bin {
  if (typeof value === 'string') return { path: value, name: commandName(fileName(value)) };
  if (isObject(value) && typeof value['path'] === 'string' && typeof value['name'] === 'string') {
    return { path: value['path'], name: commandName(value['name']) };
  }
  throw new StatementError(`${path} must be a path, or an object with a path and a name`);
}

function readRequires(fields: Fields): Requires {
  return {
    os_min: fields.optionalString('os_min'),
    glibc_min: fields.optionalString('glibc_min'),
    libs: fields.optionalList('libs', stringAt),
    bin: fields.list('bin', (item, path) => {
      const bin = new Fields(item, path);
      return { name: bin.string('name'), min: bin.optionalString('min') };
    }),
  };
}

function readArtifact(value: unknown, path: string): Artifact {
  const fields = new Fields(value, path);
  const requires = fields.optionalObject('requires');
  return {
    name: fields.string('name'),
    os: fields.optionalString('os'),
    arch: fields.optionalString('arch'),
    libc: fields.optionalString('libc'),
    variant: fields.optionalString('variant'),
    size: fields.size('size'),
    url: fields.optionalString('url'),
    // Optional here so that its absence is reported by name below.
    format: fields.optionalString('format'),
    bin: fields.list('bin', readBin),
    requires: requires === undefined ? undefined : readRequires(requires),
    provenance: fields.strings('provenance'),
    extensions: fields.map('extensions'),
  };
}

function readResource(value: unknown, path: string): Resource {
  const fields = new Fields(value, path);
  return {
    kind: fields.string('kind'),
    artifact: fields.optionalString('artifact'),
    os: fields.optionalString('os'),
    arch: fields.optionalString('arch'),
    libc: fields.optionalString('libc'),
    shell: fields.optionalString('shell'),
    shells: fields.strings('shells'),
    name: fields.optionalString('name'),
    bin: fields.optionalString('bin'),
    format: fields.optionalString('format'),
    archive: fields.optionalString('archive'),
    asset: fields.optionalString('asset'),
    url: fields.optionalString('url'),
    repo: fields.optionalString('repo'),
    exec: fields.strings('exec'),
    env: fields.stringMap('env'),
    extensions: fields.map('extensions'),
  };
}

function readIdentity(fields: Fields): Identity {
  const scheme = fields.string('scheme');
  if (scheme !== 'sigstore-oidc' && scheme !== 'sigstore-key') {
    throw new StatementError(`${fields.at('scheme')} must be sigstore-oidc or sigstore-key, got ${q(scheme)}`);
  }
  return {
    scheme,
    key_id: fields.string('key_id'),
    issuer: fields.optionalString('issuer'),
    pin_workflow: fields.optionalBoolean('pin_workflow'),
  };
}

function readPredicate(fields: Fields): Predicate {
  const source = fields.optionalObject('source');
  // Absent means the vendor; null is not a value the field takes.
  const attested = fields.get('attested_by') === undefined ? 'vendor' : fields.get('attested_by');
  if (attested !== 'vendor' && attested !== 'repackager') {
    throw new StatementError(`${fields.at('attested_by')} must be vendor or repackager`);
  }
  return {
    project: fields.string('project'),
    version: fields.string('version'),
    published_at: fields.string('published_at'),
    source:
      source === undefined
        ? undefined
        : { repo: source.string('repo'), commit: source.optionalString('commit'), tag: source.optionalString('tag') },
    artifacts: fields.list('artifacts', readArtifact, true),
    resources: fields.list('resources', readResource),
    identity: readIdentity(fields.object('identity')),
    attested_by: attested,
    evidence: fields.list('evidence', (item, path) => {
      const evidence = new Fields(item, path);
      return { kind: evidence.string('kind'), detail: evidence.optionalString('detail') };
    }),
    notes_url: fields.optionalString('notes_url'),
    extensions: fields.map('extensions'),
  };
}

function checkTypes(type: string, predicateType: string): void {
  if (type !== STATEMENT_TYPE) throw new StatementError(`_type must be ${STATEMENT_TYPE}, got ${q(type)}`);
  if (predicateType !== RELEASE_PREDICATE_TYPE) {
    throw new StatementError(`predicateType must be ${RELEASE_PREDICATE_TYPE}, got ${q(predicateType)}`, 'predicate-type');
  }
}

/**
 * Read a decoded statement and check it against the release/v1 rules. Throws
 * {@link StatementError}, with code `predicate-type` for a statement that is
 * not a release (a `releases/v1` release list among them).
 */
export function checkStatement(value: unknown): Statement {
  const fields = new Fields(value, '');
  // The types come first, so that a release list or another predicate is
  // refused as what it is rather than for lacking a release's fields.
  checkTypes(fields.string('_type'), fields.string('predicateType'));
  const statement: Statement = {
    _type: fields.string('_type'),
    subject: fields.list('subject', readSubject, true),
    predicateType: fields.string('predicateType'),
    predicate: readPredicate(fields.object('predicate')),
  };
  validateStatement(statement);
  return statement;
}

// ---------------------------------------------------------------------------
// Validation: the structural invariants beyond the field types. This proves
// nothing about the signature.
// ---------------------------------------------------------------------------

/** Check a statement's structural invariants. Throws {@link StatementError}. */
export function validateStatement(s: Statement): void {
  checkTypes(s._type, s.predicateType);
  const subjects = new Set<string>();
  for (const subject of s.subject) {
    if (subjects.has(subject.name)) throw new StatementError(`subject ${q(subject.name)} appears more than once`);
    subjects.add(subject.name);
    if (!SHA256.test(subject.digest.sha256)) {
      throw new StatementError(`sha256 for ${q(subject.name)} must be 64 lowercase hex characters`);
    }
    if (subject.digest.sha512 !== undefined && !SHA512.test(subject.digest.sha512)) {
      throw new StatementError(`sha512 for ${q(subject.name)} must be 128 lowercase hex characters`);
    }
  }

  const p = s.predicate;
  if (!validProject(p.project)) {
    throw new StatementError(
      `project must be a host path such as github.com/owner/repo or tool.example.com, got ${q(p.project)}`,
    );
  }
  if (p.version === '') throw new StatementError('version must not be empty');
  if (!SEMVER.test(p.version)) {
    throw new StatementError(
      `version must be semver 2.0.0 (MAJOR.MINOR.PATCH, with an optional prerelease part), got ${q(p.version)}`,
    );
  }
  if (!validTimestamp(p.published_at)) throw new StatementError(`published_at must be RFC 3339, got ${q(p.published_at)}`);
  // A keyless identity names the certificate's subject and the issuer that vouched for it.
  if (p.identity.scheme === 'sigstore-oidc') {
    if (p.identity.key_id === '') throw new StatementError('a sigstore-oidc identity needs a key_id, the certificate identity');
    if (p.identity.issuer === undefined || p.identity.issuer === '') {
      throw new StatementError('a sigstore-oidc identity needs an issuer');
    }
  }
  if (p.artifacts.length === 0) throw new StatementError('at least one artifact is required');
  checkExtensions('the release', p.extensions);

  const artifacts = new Set<string>();
  p.artifacts.forEach((artifact, i) => {
    const name = q(artifact.name);
    if (artifacts.has(artifact.name)) throw new StatementError(`artifact ${name} appears more than once`);
    artifacts.add(artifact.name);
    checkExtensions(`artifact ${name}`, artifact.extensions);
    if (!subjects.has(artifact.name)) throw new StatementError(`artifact ${name} has no matching subject`);
    // A consumer skips a format it does not handle, and none handles an absent one.
    if (artifact.format === undefined) {
      throw new StatementError(`artifact ${name} has no format, so no consumer can select it`);
    }
    checkTokens(`artifact ${name}`, {
      os: artifact.os,
      arch: artifact.arch,
      libc: artifact.libc,
      variant: artifact.variant,
      format: artifact.format,
    });
    const bare = isBareFormat(artifact.format) ? bareFileName(artifact.name, artifact.format) : undefined;
    for (const bin of artifact.bin) {
      if (bin.path === '') throw new StatementError(`artifact ${name} has an executable with an empty path`);
      // Read without `.exe`, so `.exe` and a path ending in `/` have no name either.
      if (bin.name === '') throw new StatementError(`artifact ${name} has an executable with an empty name`);
      if (bin.name.includes('/')) {
        throw new StatementError(`artifact ${name} has an executable name containing a slash: ${q(bin.name)}`);
      }
      if (commandName(bin.name) !== bin.name) {
        throw new StatementError(
          `artifact ${name} has an executable name ending in .exe: ${q(bin.name)}; a name is the command as typed, and the path carries the extension`,
        );
      }
      if (bare !== undefined && bin.path !== bare) {
        throw new StatementError(`artifact ${name} is a bare executable, so its bin must be ${q(bare)}, not ${q(bin.path)}`);
      }
    }
    // Two artifacts for one platform, variant, and format leave a consumer
    // nothing to choose by.
    const other = p.artifacts.slice(0, i).find((a) => selectorsEqual(a, artifact));
    if (other !== undefined) {
      throw new StatementError(
        `artifacts ${q(other.name)} and ${name} describe the same os, arch, libc, variant, and format; give one a variant`,
      );
    }
  });

  const binNames = new Set(p.artifacts.flatMap((a) => a.bin.map((b) => commandName(b.name))));
  const isBin = (name: string) => binNames.has(commandName(name));
  for (const artifact of p.artifacts) {
    if (artifact.requires !== undefined) validateRequires(q(artifact.name), artifact.requires, isBin);
  }

  const assets = new Set<string>();
  for (const resource of p.resources) {
    const asset = validateResource(resource, p, subjects, binNames);
    if (asset !== undefined) assets.add(asset);
  }
  for (const subject of s.subject) {
    if (!artifacts.has(subject.name) && !assets.has(subject.name)) {
      throw new StatementError(`subject ${q(subject.name)} has no matching artifact`);
    }
  }
}

function validateRequires(name: string, requires: Requires, isBin: (name: string) => boolean): void {
  const libs = new Set<string>();
  for (const lib of requires.libs ?? []) {
    if (!validLib(lib)) {
      throw new StatementError(
        `artifact ${name} requires library ${q(lib)}; a library is named as its loader resolves it, such as libssl.so.3`,
      );
    }
    if (libs.has(lib)) throw new StatementError(`artifact ${name} requires library ${q(lib)} twice`);
    libs.add(lib);
  }
  const commands = new Set<string>();
  for (const required of requires.bin) {
    if (!validRequiredBin(required.name)) {
      throw new StatementError(
        `artifact ${name} requires command ${q(required.name)}; a command is a bare name such as java`,
      );
    }
    if (required.min !== undefined && !validMin(required.min)) {
      throw new StatementError(
        `artifact ${name} requires command ${q(required.name)} with minimum ${q(required.min)}; a minimum starts with a digit, as in 17 or 3.12`,
      );
    }
    if (commands.has(required.name)) throw new StatementError(`artifact ${name} requires command ${q(required.name)} twice`);
    commands.add(required.name);
    if (isBin(required.name)) {
      throw new StatementError(`artifact ${name} requires command ${q(required.name)}, which the release itself provides`);
    }
  }
}

type ResourceSource = 'archive' | 'asset' | 'repo' | 'exec';

/** The one source a resource declares, or undefined when it declares none or several. */
function sourceOf(r: Resource): ResourceSource | undefined {
  const sources: ResourceSource[] = [];
  if (r.archive !== undefined) sources.push('archive');
  if (r.asset !== undefined) sources.push('asset');
  if (r.repo !== undefined) sources.push('repo');
  if (r.exec.length > 0) sources.push('exec');
  return sources.length === 1 ? sources[0] : undefined;
}

/** A short label for messages: `completion/zsh`, `skill/mise`, `cli-spec/usage/mise`. */
function label(r: Resource): string {
  let text = r.kind;
  for (const part of [r.format, r.bin, r.shell, r.name]) if (part !== undefined) text += `/${part}`;
  if (r.shells.length > 0) text += `/${r.shells.join(',')}`;
  return text;
}

/** Check one resource; returns the asset it names, if any. */
function validateResource(
  r: Resource,
  p: Predicate,
  subjects: Set<string>,
  binNames: Set<string>,
): string | undefined {
  const name = q(label(r));
  // A resource's `bin` and the program an `exec` runs are command names too, so
  // `mise.exe` there is the executable `mise`.
  const isBin = (bin: string) => binNames.has(commandName(bin));
  if (r.kind === '') throw new StatementError('a resource has an empty kind');
  checkExtensions(`resource ${name}`, r.extensions);
  // Said before the scope check below, which would report it as a platform mismatch.
  if (r.archive !== undefined && r.artifact !== undefined) {
    const format = p.artifacts.find((a) => a.name === r.artifact)?.format;
    if (format !== undefined && isBareFormat(format)) {
      throw new StatementError(
        `resource ${name} names a path inside artifact ${q(r.artifact)}, which is a bare ${format} executable and holds no paths`,
      );
    }
  }
  if (r.artifact !== undefined && !p.artifacts.some((a) => fits(r, a))) {
    throw new StatementError(
      `resource ${name} names artifact ${q(r.artifact)}, which is absent or outside its platform scope`,
    );
  }
  const source = sourceOf(r);
  if (source === undefined) throw new StatementError(`resource ${name} must have exactly one of archive, asset, repo, exec`);
  checkTokens(`resource ${name}`, { os: r.os, arch: r.arch, libc: r.libc });
  let asset: string | undefined;
  switch (source) {
    case 'archive':
      if (!validRelativePath(r.archive ?? '')) {
        throw new StatementError(`resource ${name}: archive path must be relative with no empty or .. segments`);
      }
      break;
    case 'asset':
      asset = r.asset ?? '';
      if (!subjects.has(asset)) throw new StatementError(`resource ${name} names asset ${q(asset)}, which is not a subject`);
      break;
    case 'repo':
      if (!validRelativePath(r.repo ?? '')) {
        throw new StatementError(`resource ${name}: repo path must be relative with no empty or .. segments`);
      }
      if (p.source?.commit === undefined) {
        throw new StatementError(`resource ${name} comes from the repository, which needs source.commit`);
      }
      break;
    case 'exec': {
      const program = r.exec[0] ?? '';
      if (!isBin(program)) {
        throw new StatementError(`resource ${name} runs ${q(program)}, which is not a bin name of any artifact`);
      }
      break;
    }
  }
  const env = Object.keys(r.env);
  if (env.length > 0 && source !== 'exec') throw new StatementError(`resource ${name} has env but its source is not exec`);
  const badName = env.find((key) => key === '' || key.includes('=') || key.includes('\0'));
  if (badName !== undefined) {
    throw new StatementError(
      `resource ${name} has an env variable named ${q(badName)}; a name is non-empty and holds no = or NUL`,
    );
  }
  if (r.url !== undefined && source !== 'asset') {
    throw new StatementError(`resource ${name} has a url but its source is not an asset`);
  }
  if (r.kind === 'completion' || r.kind === 'man') {
    if (r.bin !== undefined && !isBin(r.bin)) {
      throw new StatementError(`resource ${name} is for ${q(r.bin)}, which is not a bin name of any artifact`);
    }
    if (r.bin === undefined && binNames.size > 1) {
      throw new StatementError(
        `resource ${name} does not say which executable it is for, and the release has several: ${[...binNames].sort().join(', ')}`,
      );
    }
  }
  switch (r.kind) {
    case 'completion': {
      if ((r.shell !== undefined) !== (r.shells.length === 0)) {
        throw new StatementError(`completion ${name} needs exactly one of shell or shells`);
      }
      const badShell = [...(r.shell === undefined ? [] : [r.shell]), ...r.shells].find((shell) => !validShell(shell));
      if (badShell !== undefined) {
        throw new StatementError(`completion ${name} names shell ${q(badShell)}; a shell is a lowercase word such as zsh`);
      }
      const mentionsShell = [...r.exec, ...Object.values(r.env)].some((value) => value.includes('{shell}'));
      if ((source === 'exec' && r.shell !== undefined) || (r.shells.length > 0 && (source !== 'exec' || !mentionsShell))) {
        throw new StatementError(`completion ${name} lists shells, so its source must be exec with a {shell} placeholder`);
      }
      break;
    }
    case 'cli-spec':
      if (r.format === undefined || r.bin === undefined) throw new StatementError(`cli-spec ${name} needs format and bin`);
      if (!isBin(r.bin)) {
        throw new StatementError(`cli-spec ${name} describes ${q(r.bin)}, which is not a bin name of any artifact`);
      }
      break;
    case 'skill':
      if (r.name === undefined || r.name === '' || r.name.includes('/')) {
        throw new StatementError(`skill ${name} needs a name without a slash`);
      }
      break;
    case 'app':
      if (source !== 'archive') throw new StatementError(`app ${name} must come from an archive`);
      break;
    case 'sbom':
      if (r.format === undefined || source === 'exec') {
        throw new StatementError(`sbom ${name} needs a format (cyclonedx or spdx) and a static source, not exec`);
      }
      break;
  }
  return asset;
}

// Extension keys name who defines them, so an empty one is a mistake.
function checkExtensions(owner: string, extensions: Extensions): void {
  if (Object.hasOwn(extensions, '')) throw new StatementError(`${owner} has an extension with an empty key`);
}

function checkTokens(owner: string, values: Record<string, string | undefined>): void {
  for (const [field, value] of Object.entries(values)) {
    if (value !== undefined && !TOKEN.test(value)) {
      throw new StatementError(
        `${owner} has ${field} ${q(value)}; a value is a lowercase word of letters, digits, _, -, and .`,
      );
    }
  }
}

function selectorsEqual(a: Artifact, b: Artifact): boolean {
  return (
    a.os === b.os &&
    a.arch === b.arch &&
    a.libc === b.libc &&
    a.variant === b.variant &&
    a.format === b.format &&
    a.format !== undefined
  );
}

/** Whether a resource applies to an artifact, as `resource_fits` in the Rust crate says. */
function fits(r: Resource, a: Artifact): boolean {
  const insideBareArtifact = r.archive !== undefined && a.format !== undefined && isBareFormat(a.format);
  return (
    !insideBareArtifact &&
    (r.artifact === undefined || r.artifact === a.name) &&
    (r.os === undefined || r.os === a.os) &&
    (r.arch === undefined || r.arch === a.arch) &&
    (r.libc === undefined || r.libc === a.libc)
  );
}

function isBareFormat(format: string): boolean {
  return BARE_FORMATS.includes(format);
}

/** The file inside a bare-format artifact: its own name, minus any compression suffix. */
function bareFileName(name: string, format: string): string {
  if (format === 'raw' || !name.endsWith(`.${format}`)) return name;
  return name.slice(0, -(format.length + 1));
}

/**
 * A project name is a lowercase DNS host, optionally followed by path segments:
 * `github.com/jdx/mise`, `mise.jdx.dev`. No scheme, no empty or dot segments,
 * no trailing slash, and the host has at least one dot.
 */
export function validProject(project: string): boolean {
  const [host = '', ...segments] = project.split('/');
  const hostOk =
    host.includes('.') &&
    !host.startsWith('.') &&
    !host.endsWith('.') &&
    !host.includes('..') &&
    host
      .split('.')
      .every((label) => label !== '' && !label.startsWith('-') && !label.endsWith('-') && /^[a-z0-9-]+$/.test(label));
  return hostOk && segments.every((segment) => segment !== '.' && segment !== '..' && /^[A-Za-z0-9._~-]+$/.test(segment));
}

function validTimestamp(value: string): boolean {
  const match = RFC3339.exec(value);
  if (match === null) return false;
  // An absent offset is `Z`.
  const [year = 0, month = 0, day = 0, hour = 0, minute = 0, second = 0, offsetHour = 0, offsetMinute = 0] = match
    .slice(1)
    .map((part) => Number(part ?? 0));
  const leap = (year % 4 === 0 && year % 100 !== 0) || year % 400 === 0;
  const days = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][month - 1];
  return (
    days !== undefined &&
    day >= 1 &&
    day <= days &&
    hour <= 23 &&
    minute <= 59 &&
    // A leap second is accepted, as the Rust crate's parser accepts one.
    second <= 60 &&
    offsetHour <= 23 &&
    offsetMinute <= 59
  );
}

/** A path inside an archive or a repository: relative, with no empty or `..` segments. */
function validRelativePath(path: string): boolean {
  return path !== '' && !path.startsWith('/') && path.split('/').every((segment) => segment !== '' && segment !== '..');
}

/** A shared library name as a loader resolves it: one path segment, no whitespace or control characters. */
function validLib(name: string): boolean {
  return name !== '' && !/[/\\\x00-\x20\x7f]/.test(name);
}

/** A command name as typed at a shell: one segment, no `.exe`. */
function validRequiredBin(name: string): boolean {
  return validLib(name) && !/\.exe$/i.test(name);
}

/** A minimum version: dot-separated components starting with a digit. */
function validMin(min: string): boolean {
  return /^[0-9][A-Za-z0-9.+-]*$/.test(min);
}

/** A shell name is a plain lowercase word: `bash`, `zsh`, `powershell`. */
function validShell(shell: string): boolean {
  return /^[a-z0-9_-]+$/.test(shell);
}

function fileName(path: string): string {
  return path.slice(path.lastIndexOf('/') + 1);
}

/** A command's name as the specification writes it: as typed at a shell, without `.exe`. */
function commandName(name: string): string {
  return /\.exe$/i.test(name) ? name.slice(0, -4) : name;
}
