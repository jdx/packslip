import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, it } from 'node:test';
import {
  ArtifactNotFoundError,
  BundleError,
  DigestMismatchError,
  IdentityError,
  SignatureError,
  StatementError,
  findArtifact,
  verifyFileDigest,
  verifyRelease,
  type VerifiedStatement,
} from '../src/index.ts';
import { sameIdentity } from '../src/project.ts';
import { readStatement } from '../src/statement.ts';

// The bundle jdx/mise v2026.10.11 published as packslip.sigstore.json.
const bundleText = readFileSync(new URL('./mise-2026.10.11.sigstore.json', import.meta.url), 'utf8');
const IDENTITY = 'https://github.com/jdx/mise/.github/workflows/release.yml@refs/tags/v2026.10.11';
const ISSUER = 'https://token.actions.githubusercontent.com';
const ASSET = 'mise-v2026.10.11-linux-arm64';

function payloadOf(bundle: string): Record<string, any> {
  return JSON.parse(Buffer.from(JSON.parse(bundle).dsseEnvelope.payload, 'base64').toString('utf8'));
}

function withPayload(bundle: string, edit: (statement: Record<string, any>) => void): string {
  const parsed = JSON.parse(bundle);
  const statement = payloadOf(bundle);
  edit(statement);
  parsed.dsseEnvelope.payload = Buffer.from(JSON.stringify(statement)).toString('base64');
  return JSON.stringify(parsed);
}

// Verification fetches sigstore's trust root over TUF, so these need network access.
describe('verifyRelease (public-good sigstore)', () => {
  it('verifies the real mise release bundle', async () => {
    const statement = await verifyRelease(bundleText, {
      identity: IDENTITY,
      issuer: ISSUER,
      project: 'github.com/jdx/mise',
      tag: 'v2026.10.11',
    });
    assert.equal(statement.predicate.version, '2026.10.11');
    assert.deepEqual(statement.signer, { identity: IDENTITY, issuer: ISSUER });
    const artifact = findArtifact(statement, ASSET);
    assert.equal(artifact.sha256, '370b4b4cfd740402e9a55aa1a8a01fbb1ac95c50af1207b2db2c5489dd4da0cd');
    assert.match(artifact.sha512 ?? '', /^[0-9a-f]{128}$/);
  });

  it('still verifies a release from the project repository with allowOtherRepository', async () => {
    const statement = await verifyRelease(bundleText, {
      identity: IDENTITY,
      issuer: ISSUER,
      project: 'github.com/jdx/mise',
      allowOtherRepository: true,
    });
    assert.deepEqual(statement.signer, { identity: IDENTITY, issuer: ISSUER });
  });

  it('accepts an identity predicate', async () => {
    const statement = await verifyRelease(bundleText, {
      identity: (san) => san.startsWith('https://github.com/jdx/mise/.github/workflows/'),
      issuer: ISSUER,
    });
    assert.equal(statement.predicate.project, 'github.com/jdx/mise');
  });

  it('rejects a different identity', async () => {
    await assert.rejects(
      verifyRelease(bundleText, { identity: 'https://github.com/evil/mise/.github/workflows/release.yml@refs/tags/v1', issuer: ISSUER }),
      (error: unknown) => error instanceof IdentityError && error.actualIdentity === IDENTITY,
    );
  });

  it('matches a GitHub repository name in a string identity in any case', async () => {
    const statement = await verifyRelease(bundleText, { identity: IDENTITY.replace('jdx/mise', 'JDX/Mise'), issuer: ISSUER });
    assert.equal(statement.signer.identity, IDENTITY);
    await assert.rejects(
      verifyRelease(bundleText, { identity: IDENTITY.replace('release.yml', 'Release.yml'), issuer: ISSUER }),
      IdentityError,
    );
  });

  it('rejects a different issuer', async () => {
    await assert.rejects(
      verifyRelease(bundleText, { identity: IDENTITY, issuer: 'https://gitlab.com' }),
      IdentityError,
    );
  });

  it('rejects a project or tag that does not match', async () => {
    await assert.rejects(
      verifyRelease(bundleText, { identity: IDENTITY, issuer: ISSUER, project: 'github.com/jdx/usage' }),
      (error: unknown) => error instanceof StatementError && error.code === 'project',
    );
    await assert.rejects(
      verifyRelease(bundleText, { identity: IDENTITY, issuer: ISSUER, tag: 'v2026.10.10' }),
      (error: unknown) => error instanceof StatementError && error.code === 'tag',
    );
  });

  it('rejects a tampered subject digest', async () => {
    const tampered = withPayload(bundleText, (statement) => {
      statement.subject[0].digest.sha256 = '0'.repeat(64);
    });
    await assert.rejects(verifyRelease(tampered, { identity: IDENTITY, issuer: ISSUER }), SignatureError);
  });

  it('verifies a snapshot of an object bundle, not the caller\'s reference', async () => {
    const bundle = JSON.parse(bundleText);
    const statement = await verifyRelease(bundle, {
      identity: (san) => {
        // Swapping the payload during the identity check must not change the result.
        bundle.dsseEnvelope.payload = Buffer.from('{}').toString('base64');
        return san === IDENTITY;
      },
      issuer: ISSUER,
    });
    assert.equal(statement.predicate.project, 'github.com/jdx/mise');
  });

  it('rejects input that is not a DSSE bundle', async () => {
    await assert.rejects(verifyRelease('not json', { identity: IDENTITY, issuer: ISSUER }), BundleError);
    await assert.rejects(verifyRelease('{}', { identity: IDENTITY, issuer: ISSUER }), BundleError);
  });

  it('rejects a bundle with more log entries than a release carries', async () => {
    const bundle = JSON.parse(bundleText);
    const [entry] = bundle.verificationMaterial.tlogEntries;
    bundle.verificationMaterial.tlogEntries = Array.from({ length: 9 }, () => entry);
    await assert.rejects(verifyRelease(bundle, { identity: IDENTITY, issuer: ISSUER }), BundleError);
  });
});

describe('sameIdentity', () => {
  it('compares a GitHub or GitLab repository name without case, and the rest exactly', () => {
    assert.ok(sameIdentity(IDENTITY, IDENTITY.replace('jdx/mise', 'JDX/MISE')));
    assert.ok(sameIdentity('https://gitlab.com/Group/Tool//.gitlab-ci.yml@refs/tags/v1', 'https://gitlab.com/group/tool//.gitlab-ci.yml@refs/tags/v1'));
    assert.ok(sameIdentity('person@example.com', 'person@example.com'));
    for (const other of [
      IDENTITY.replace('.github', '.GITHUB'),
      IDENTITY.replace('refs/tags/v', 'refs/tags/V'),
      IDENTITY.replace('https://github.com', 'HTTPS://GITHUB.COM'),
      IDENTITY.replace('jdx/mise', 'jdx/mise-evil'),
    ]) {
      assert.ok(!sameIdentity(other, IDENTITY), other);
    }
    assert.ok(!sameIdentity('Person@example.com', 'person@example.com'));
  });
});

// A wrong predicate type cannot be re-signed, so the statement checks run on their own too.
describe('readStatement', () => {
  const payload = (edit: (statement: Record<string, any>) => void = () => {}) => {
    const statement = payloadOf(bundleText);
    edit(statement);
    return Buffer.from(JSON.stringify(statement)).toString('base64');
  };

  it('accepts the real statement', () => {
    const statement = readStatement(payload(), { project: 'github.com/jdx/mise' });
    assert.equal(statement.subject.length, 41);
  });

  it('matches a GitHub repository name in any case, but a subpath exactly', () => {
    assert.equal(readStatement(payload(), { project: 'github.com/JDX/Mise' }).predicate.project, 'github.com/jdx/mise');
    assert.throws(
      () => readStatement(payload((s) => (s.predicate.project = 'github.com/jdx/mise/tool')), { project: 'github.com/jdx/mise/Tool' }),
      (error: unknown) => error instanceof StatementError && error.code === 'project',
    );
    assert.throws(
      () => readStatement(payload((s) => (s.predicate.project = 'example.com/Tool')), { project: 'example.com/tool' }),
      (error: unknown) => error instanceof StatementError && error.code === 'project',
    );
  });

  it('rejects the release list predicate', () => {
    assert.throws(
      () => readStatement(payload((s) => (s.predicateType = 'https://packslip.dev/releases/v1')), {}),
      (error: unknown) => error instanceof StatementError && error.code === 'predicate-type',
    );
  });

  it('rejects another predicate type and statement type', () => {
    assert.throws(() => readStatement(payload((s) => (s.predicateType = 'https://slsa.dev/provenance/v1')), {}), StatementError);
    assert.throws(() => readStatement(payload((s) => (s._type = 'https://example.com/Statement')), {}), StatementError);
  });

  it('rejects a non-string _type and malformed artifacts', () => {
    assert.throws(() => readStatement(payload((s) => (s._type = { toString: null })), {}), StatementError);
    assert.throws(() => readStatement(payload((s) => (s.predicate.artifacts = [null])), {}), StatementError);
    assert.throws(() => readStatement(payload((s) => (s.predicate.artifacts[0].size = -1)), {}), StatementError);
  });

  it('rejects malformed digests and duplicate subjects', () => {
    assert.throws(() => readStatement(payload((s) => (s.subject[0].digest.sha256 = 'ABC')), {}), StatementError);
    assert.throws(() => readStatement(payload((s) => (s.subject[1].name = s.subject[0].name)), {}), StatementError);
    assert.throws(() => readStatement(payload((s) => (s.subject = [])), {}), StatementError);
  });
});

describe('findArtifact and verifyFileDigest', () => {
  const directory = mkdtempSync(join(tmpdir(), 'packslip-test-'));
  const file = join(directory, 'asset');
  writeFileSync(file, 'hello\n');
  const sha256 = '5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03';
  const sha512 =
    'e7c22b994c59d9cf2b48e549b1e24666636045930d3da7c1acb299d1c3b7f931f94aae41edda2c2b207a36e10f8bcb8d45223e54878f5b316e7ce3b6bc019629';
  const statement = {
    _type: 'https://in-toto.io/Statement/v1',
    subject: [{ name: 'asset', digest: { sha256, sha512 } }],
    predicateType: 'https://packslip.dev/release/v1',
    predicate: { project: 'example.com/tool', version: '1.0.0', artifacts: [{ name: 'asset', size: 6 }] },
    signer: { identity: 'x', issuer: 'y' },
  } as VerifiedStatement;

  it('finds an artifact with its size', () => {
    assert.deepEqual(findArtifact(statement, 'asset'), { name: 'asset', sha256, sha512, size: 6 });
  });

  it('throws for an unlisted asset', () => {
    assert.throws(() => findArtifact(statement, 'other'), ArtifactNotFoundError);
  });

  it('accepts a matching file', async () => {
    await verifyFileDigest(file, findArtifact(statement, 'asset'));
  });

  it('rejects a wrong sha256, sha512, or size', async () => {
    const artifact = findArtifact(statement, 'asset');
    await assert.rejects(verifyFileDigest(file, { ...artifact, sha256: '0'.repeat(64) }), (e: unknown) => e instanceof DigestMismatchError && e.field === 'sha256');
    await assert.rejects(verifyFileDigest(file, { ...artifact, sha512: '0'.repeat(128) }), (e: unknown) => e instanceof DigestMismatchError && e.field === 'sha512');
    await assert.rejects(verifyFileDigest(file, { ...artifact, size: 7 }), (e: unknown) => e instanceof DigestMismatchError && e.field === 'size');
  });
});
