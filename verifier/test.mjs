// Tests for packslip-verify.mjs. They run offline against the bundles two real
// releases published (jdx/mise 2026.10.11 and jdx/hk 2.3.0) and tamper with them.
//   node --test verifier/test.mjs
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, it } from 'node:test';
import { fileURLToPath } from 'node:url';
import { TRUSTED_ROOT as trustedRoot, readStatement, verifyFile, verifyRelease } from './packslip-verify.mjs';

const script = fileURLToPath(new URL('./packslip-verify.mjs', import.meta.url));
const miseBundlePath = fileURLToPath(new URL('../packages/packslip/test/mise-2026.10.11.sigstore.json', import.meta.url));
const hkBundlePath = fileURLToPath(new URL('../tests/fixtures/hk-v2.3.0.sigstore.json', import.meta.url));
const mise = readFileSync(miseBundlePath, 'utf8');
const hk = readFileSync(hkBundlePath, 'utf8');

const ISSUER = 'https://token.actions.githubusercontent.com';
const IDENTITY = 'https://github.com/jdx/mise/.github/workflows/release.yml@refs/tags/v2026.10.11';
const policy = { issuer: ISSUER, identity: IDENTITY, project: 'github.com/jdx/mise', tag: 'v2026.10.11' };

/** Parse a bundle, let `edit` change it, and serialize it again. */
function tamper(text, edit) {
  const bundle = JSON.parse(text);
  edit(bundle, bundle.verificationMaterial.tlogEntries[0]);
  return JSON.stringify(bundle);
}

const flip = (base64) => {
  const bytes = Buffer.from(base64, 'base64');
  bytes[bytes.length - 1] ^= 1;
  return bytes.toString('base64');
};

function refuses(bundle, options, code, pattern) {
  assert.throws(
    () => verifyRelease(bundle, options),
    (error) => error.code === code && (pattern === undefined || pattern.test(error.message)),
  );
}

describe('verifyRelease', () => {
  it('verifies the mise release bundle', () => {
    const statement = verifyRelease(mise, policy);
    assert.equal(statement.predicate.version, '2026.10.11');
    assert.deepEqual(statement.signer, { identity: IDENTITY, issuer: ISSUER });
    const subject = statement.subject.find((s) => s.name === 'mise-v2026.10.11-linux-arm64');
    assert.equal(subject.digest.sha256, '370b4b4cfd740402e9a55aa1a8a01fbb1ac95c50af1207b2db2c5489dd4da0cd');
  });

  it('verifies the hk release bundle with an identity predicate and a source repository', () => {
    const statement = verifyRelease(hk, {
      issuer: ISSUER,
      identity: (san) => san.startsWith('https://github.com/jdx/hk/'),
      sourceRepository: 'https://github.com/jdx/hk',
    });
    assert.equal(statement.predicate.project, 'github.com/jdx/hk');
  });

  it('accepts a Buffer', () => {
    assert.equal(verifyRelease(Buffer.from(mise), policy).predicate.project, 'github.com/jdx/mise');
  });

  describe('the signer policy', () => {
    it('is required', () => {
      refuses(mise, { issuer: ISSUER }, 'usage');
      refuses(mise, { identity: IDENTITY }, 'usage');
    });
    it('refuses another identity', () => {
      refuses(mise, { ...policy, identity: IDENTITY.replace('2026.10.11', '2026.10.10') }, 'identity', /not the expected one/);
      refuses(mise, { ...policy, identity: () => false }, 'identity');
    });
    it('refuses another issuer', () => {
      refuses(mise, { ...policy, issuer: 'https://gitlab.com' }, 'identity', /issuer/);
    });
    it('refuses another source repository', () => {
      refuses(mise, { ...policy, sourceRepository: 'https://github.com/jdx/mise-fork' }, 'identity', /source repository/);
    });
    it('refuses another project or tag', () => {
      refuses(mise, { ...policy, project: 'github.com/jdx/hk' }, 'project');
      refuses(mise, { ...policy, tag: 'v2026.10.10' }, 'tag');
    });
  });

  describe('a bundle that was changed', () => {
    it('is not JSON', () => refuses('{', policy, 'bundle'));
    it('has malformed certificate bytes', () => {
      refuses(tamper(mise, (b) => (b.verificationMaterial.certificate.rawBytes = 'AAAA')), policy, 'bundle', /malformed/);
    });
    it('has a log entry with the wrong shape', () => {
      refuses(tamper(mise, (_, e) => (e.canonicalizedBody = 'e30=')), policy, 'signature');
      refuses(tamper(mise, (b) => (b.dsseEnvelope.signatures[0].sig = 7)), policy, 'signature');
    });
    it('has no envelope', () => refuses(tamper(mise, (b) => delete b.dsseEnvelope), policy, 'bundle'));
    it('has another payload type', () => {
      refuses(tamper(mise, (b) => (b.dsseEnvelope.payloadType = 'text/plain')), policy, 'bundle');
    });
    it('has no certificate', () => {
      refuses(tamper(mise, (b) => delete b.verificationMaterial.certificate), policy, 'bundle');
    });
    it('has a changed statement', () => {
      const text = tamper(mise, (b) => {
        const statement = JSON.parse(Buffer.from(b.dsseEnvelope.payload, 'base64'));
        statement.subject[0].digest.sha256 = '0'.repeat(64);
        b.dsseEnvelope.payload = Buffer.from(JSON.stringify(statement)).toString('base64');
      });
      refuses(text, policy, 'signature', /different envelope/);
    });
    it('has a changed signature', () => {
      refuses(tamper(mise, (b) => (b.dsseEnvelope.signatures[0].sig = flip(b.dsseEnvelope.signatures[0].sig))), policy, 'signature');
    });
    it('has another release’s certificate', () => {
      const other = JSON.parse(hk).verificationMaterial.certificate;
      refuses(tamper(mise, (b) => (b.verificationMaterial.certificate = other)), policy, 'signature', /different certificate/);
    });
    it('has a certificate that does not chain to the trusted Fulcio root', () => {
      const root = structuredClone(trustedRoot);
      const authority = root.cas.at(-1);
      const intermediate = Buffer.from(authority.chain[0], 'base64');
      intermediate[intermediate.length - 3] ^= 1;
      authority.chain[0] = intermediate.toString('base64');
      refuses(mise, { ...policy, trustedRoot: root }, 'signature', /Fulcio root/);
    });
    it('was signed outside every Fulcio authority’s validity', () => {
      const root = structuredClone(trustedRoot);
      for (const authority of root.cas) authority.end = '2022-01-01T00:00:00Z';
      refuses(mise, { ...policy, trustedRoot: root }, 'signature', /Fulcio root/);
    });
    it('is from a log whose key is not the trusted one', () => {
      const root = structuredClone(trustedRoot);
      root.tlogs[0].publicKey = root.tlogs[1].publicKey;
      refuses(mise, { ...policy, trustedRoot: root }, 'signature', /entry timestamp/);
    });
    it('has no log entry', () => {
      refuses(tamper(mise, (b) => (b.verificationMaterial.tlogEntries = [])), policy, 'signature', /no transparency log entry/);
    });
    it('has a log entry from an unknown log', () => {
      refuses(tamper(mise, (_, e) => (e.logId.keyId = flip(e.logId.keyId))), policy, 'signature', /does not trust/);
    });
    it('has a changed integrated time', () => {
      refuses(tamper(mise, (_, e) => (e.integratedTime = String(Number(e.integratedTime) + 1))), policy, 'signature', /entry timestamp/);
    });
    it('has no signed entry timestamp', () => {
      refuses(tamper(mise, (_, e) => delete e.inclusionPromise), policy, 'signature', /no signed entry timestamp/);
    });
    it('has a changed signed entry timestamp', () => {
      const text = tamper(mise, (_, e) => (e.inclusionPromise.signedEntryTimestamp = flip(e.inclusionPromise.signedEntryTimestamp)));
      refuses(text, policy, 'signature');
    });
    it('has no inclusion proof', () => {
      refuses(tamper(mise, (_, e) => delete e.inclusionProof), policy, 'signature', /no inclusion proof/);
    });
    it('has a changed inclusion proof', () => {
      refuses(tamper(mise, (_, e) => (e.inclusionProof.hashes[0] = flip(e.inclusionProof.hashes[0]))), policy, 'signature', /inclusion proof/);
    });
    it('has a changed checkpoint', () => {
      const text = tamper(mise, (_, e) => {
        e.inclusionProof.checkpoint.envelope = e.inclusionProof.checkpoint.envelope.replace('rekor.sigstore.dev', 'rekor.evil.example');
      });
      refuses(text, policy, 'signature', /checkpoint/);
    });
    it('has a checkpoint for another tree', () => {
      const text = tamper(mise, (_, e) => (e.inclusionProof.treeSize = String(Number(e.inclusionProof.treeSize) + 1)));
      assert.throws(() => verifyRelease(text, policy), (error) => error.code === 'signature');
    });
  });
});

describe('readStatement', () => {
  const payload = JSON.parse(mise).dsseEnvelope.payload;
  const withArtifact = (edit) => {
    const statement = JSON.parse(Buffer.from(payload, 'base64'));
    edit(statement.predicate.artifacts[0]);
    return Buffer.from(JSON.stringify(statement)).toString('base64');
  };

  it('accepts the signed statement', () => {
    assert.equal(readStatement(payload, {}).predicate.project, 'github.com/jdx/mise');
  });

  it('requires a size on every artifact', () => {
    assert.throws(() => readStatement(withArtifact((a) => delete a.size), {}), { code: 'statement', message: /integer size/ });
    assert.throws(() => readStatement(withArtifact((a) => (a.size = -1)), {}), { code: 'statement' });
    assert.throws(() => readStatement(withArtifact((a) => (a.size = '12')), {}), { code: 'statement' });
  });
});

describe('verifyFile', () => {
  const dir = mkdtempSync(join(tmpdir(), 'packslip-verify-'));
  const content = 'a downloaded file\n';
  const file = join(dir, 'tool-1.0.0.tar.gz');
  writeFileSync(file, content);
  const statement = (overrides = {}) => ({
    subject: [
      {
        name: 'tool-1.0.0.tar.gz',
        digest: {
          sha256: createHash('sha256').update(content).digest('hex'),
          sha512: createHash('sha512').update(content).digest('hex'),
        },
      },
    ],
    predicate: { artifacts: [{ name: 'tool-1.0.0.tar.gz', size: content.length }] },
    ...overrides,
  });

  it('accepts a file that matches', async () => {
    const result = await verifyFile(statement(), file);
    assert.equal(result.name, 'tool-1.0.0.tar.gz');
    assert.equal(result.size, content.length);
  });

  it('refuses a file the statement does not list', async () => {
    const renamed = join(dir, 'renamed.tar.gz');
    writeFileSync(renamed, content);
    await assert.rejects(verifyFile(statement(), renamed), { code: 'artifact-not-found' });
  });

  it('refuses a file it cannot read', async () => {
    const missing = statement();
    await assert.rejects(verifyFile(missing, join(dir, 'missing', 'tool-1.0.0.tar.gz')), { code: 'file' });
  });

  it('refuses a changed file', async () => {
    const other = mkdtempSync(join(tmpdir(), 'packslip-verify-'));
    const changed = join(other, 'tool-1.0.0.tar.gz');
    writeFileSync(changed, 'a downloaded fire\n');
    await assert.rejects(verifyFile(statement(), changed), { code: 'digest-mismatch', message: /sha256 is/ });
  });

  it('refuses a file of another size', async () => {
    const sized = statement({ predicate: { artifacts: [{ name: 'tool-1.0.0.tar.gz', size: 1 }] } });
    await assert.rejects(verifyFile(sized, file), { code: 'digest-mismatch', message: /size is/ });
  });

  it('checks the SHA-512 too', async () => {
    const s = statement();
    s.subject[0].digest.sha512 = '0'.repeat(128);
    await assert.rejects(verifyFile(s, file), { code: 'digest-mismatch', message: /sha512 is/ });
  });
});

describe('command line', () => {
  const run = (...args) => spawnSync(process.execPath, [script, ...args], { encoding: 'utf8' });

  it('verifies a bundle', () => {
    const result = run(miseBundlePath, '--issuer', ISSUER, '--identity-prefix', 'https://github.com/jdx/mise/.github/workflows/', '--project', 'github.com/jdx/mise');
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /^verified github\.com\/jdx\/mise 2026\.10\.11, signed by /);
  });

  it('prints JSON', () => {
    const result = run(miseBundlePath, '--issuer', ISSUER, '--identity', IDENTITY, '--json');
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(JSON.parse(result.stdout).signer, { identity: IDENTITY, issuer: ISSUER });
  });

  it('exits 1 with the reason when verification fails', () => {
    const result = run(miseBundlePath, '--issuer', ISSUER, '--identity-prefix', 'https://github.com/jdx/mise-fork/');
    assert.equal(result.status, 1);
    assert.match(result.stderr, /^verification failed: the signing identity .* is not the expected one/);
  });

  it('exits 1 when a listed file is not in the statement', () => {
    const result = run(miseBundlePath, '--issuer', ISSUER, '--identity', IDENTITY, '--artifact', miseBundlePath, '--json');
    assert.equal(result.status, 1);
    assert.equal(JSON.parse(result.stdout).code, 'artifact-not-found');
  });

  it('reports an unreadable bundle or file as a failure', () => {
    const missing = run(join(tmpdir(), 'no-such-bundle.json'), '--issuer', ISSUER, '--identity', IDENTITY, '--json');
    assert.equal(missing.status, 1);
    assert.equal(JSON.parse(missing.stdout).code, 'bundle');
    const file = run(miseBundlePath, '--issuer', ISSUER, '--identity', IDENTITY, '--artifact', join(tmpdir(), 'mise-v2026.10.11-linux-arm64'), '--json');
    assert.equal(file.status, 1);
    assert.equal(JSON.parse(file.stdout).code, 'file');
  });

  it('exits 2 without a signer policy', () => {
    assert.equal(run(miseBundlePath).status, 2);
    assert.equal(run(miseBundlePath, '--issuer', ISSUER).status, 2);
    assert.equal(run(miseBundlePath, '--identity', IDENTITY).status, 2);
    assert.equal(run(miseBundlePath, '--issuer', ISSUER, '--identity', IDENTITY, '--identity-prefix', 'https://github.com/').status, 2);
  });
});
