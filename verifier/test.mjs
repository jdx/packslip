// Tests for packslip-verify.mjs. They run offline against the bundles two real
// releases published (jdx/mise 2026.10.11 and jdx/hk 2.3.0) and tamper with them.
// A signer the real bundles cannot show, such as another repository's, gets a
// bundle from a throwaway CA and log that the test passes as the trust root.
//   node --test verifier/test.mjs
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash, generateKeyPairSync, sign } from 'node:crypto';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, it } from 'node:test';
import { fileURLToPath } from 'node:url';
import {
  TRUSTED_ROOT as trustedRoot,
  VerifyError,
  certificateSigner,
  checkDeclaredIdentity,
  checkProjectSigner,
  readStatement,
  verifyFile,
  verifyRelease,
} from './packslip-verify.mjs';

const script = fileURLToPath(new URL('./packslip-verify.mjs', import.meta.url));
const miseBundlePath = fileURLToPath(new URL('../packages/packslip/test/mise-2026.10.11.sigstore.json', import.meta.url));
const hkBundlePath = fileURLToPath(new URL('../tests/fixtures/hk-v2.3.0.sigstore.json', import.meta.url));
const mise = readFileSync(miseBundlePath, 'utf8');
const hk = readFileSync(hkBundlePath, 'utf8');

const ISSUER = 'https://token.actions.githubusercontent.com';
const GITLAB_ISSUER = 'https://gitlab.com';
const IDENTITY = 'https://github.com/jdx/mise/.github/workflows/release.yml@refs/tags/v2026.10.11';
const HK_IDENTITY = 'https://github.com/jdx/hk/.github/workflows/release.yml@refs/tags/v2.3.0';
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
    it('accepts a GitHub project signed from its repository under an identity policy that accepts more', () => {
      const anyJdxWorkflow = (san) => san.startsWith('https://github.com/jdx/');
      assert.equal(verifyRelease(mise, { issuer: ISSUER, identity: anyJdxWorkflow, project: 'github.com/jdx/mise' }).predicate.version, '2026.10.11');
      assert.equal(verifyRelease(hk, { issuer: ISSUER, identity: () => true, project: 'github.com/jdx/hk' }).predicate.version, '2.3.0');
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

/** The leaf certificate a bundle carries, as DER. */
const leafOf = (text) => Buffer.from(JSON.parse(text).verificationMaterial.certificate.rawBytes, 'base64');

function refusesProject(project, signer, pattern) {
  assert.throws(
    () => checkProjectSigner(project, signer),
    (error) => error instanceof VerifyError && error.code === 'project-signer' && pattern.test(error.message),
  );
}

describe('checkProjectSigner', () => {
  const RELEASE = 'https://github.com/jdx/mise/.github/workflows/release.yml@refs/tags/v1';
  const SHARED = 'https://github.com/jdx/shared/.github/workflows/release.yml@refs/heads/main';
  const signer = (identity, issuer, sourceRepository) => ({ identity, issuer, sourceRepository });

  it("accepts the project's own workflow, with or without a source repository", () => {
    checkProjectSigner('github.com/jdx/mise', signer(RELEASE, ISSUER, 'https://github.com/jdx/mise'));
    checkProjectSigner('github.com/jdx/mise', signer(RELEASE, ISSUER));
    checkProjectSigner('github.com/jdx/mise', certificateSigner(leafOf(mise)));
  });

  it("accepts a reusable workflow of another repository when the source repository is the project's", () => {
    checkProjectSigner('github.com/jdx/mise', signer(SHARED, ISSUER, 'https://github.com/jdx/mise'));
  });

  it('binds a monorepo tool to its repository', () => {
    const oxlint = 'https://github.com/oxc-project/oxc/.github/workflows/release.yml@refs/tags/oxlint_v1.0.0';
    checkProjectSigner('github.com/oxc-project/oxc/oxlint', signer(oxlint, ISSUER, 'https://github.com/oxc-project/oxc'));
    checkProjectSigner('github.com/oxc-project/oxc/oxlint', signer(oxlint, ISSUER));
  });

  it('refuses a certificate from another source repository, whichever workflow signed', () => {
    // hk's real certificate, put to a statement that names mise.
    refusesProject(
      'github.com/jdx/mise',
      certificateSigner(leafOf(hk)),
      /^github\.com\/jdx\/mise must be signed from its repository https:\/\/github\.com\/jdx\/mise, but https:\/\/github\.com\/jdx\/hk\/\S+ signed it from https:\/\/github\.com\/jdx\/hk$/,
    );
    refusesProject('github.com/jdx/mise', signer(RELEASE, ISSUER, 'https://github.com/evil/mise'), /signed it from https:\/\/github\.com\/evil\/mise/);
    refusesProject('github.com/jdx/mise', signer(SHARED, ISSUER, 'https://github.com/jdx/shared'), /signed it from https:\/\/github\.com\/jdx\/shared/);
  });

  it('without a source repository, refuses a signer outside the repository', () => {
    refusesProject('github.com/jdx/mise', signer(SHARED, ISSUER), /jdx\/shared\S+ is not one of its workflows$/);
    // The prefix ends at the repository's slash, so a longer name does not pass.
    refusesProject('github.com/jdx/mise', signer('https://github.com/jdx/mise-fork/.github/workflows/release.yml@refs/tags/v1', ISSUER), /mise-fork/);
  });

  it("refuses a certificate from another issuer than the forge's", () => {
    refusesProject(
      'github.com/jdx/mise',
      signer('jdx@example.com', 'https://accounts.google.com'),
      /^github\.com\/jdx\/mise must be signed from its repository through https:\/\/token\.actions\.githubusercontent\.com, but jdx@example\.com signed it through https:\/\/accounts\.google\.com$/,
    );
    refusesProject('gitlab.com/group/tool', signer(RELEASE, ISSUER, 'https://github.com/jdx/mise'), /through https:\/\/gitlab\.com,/);
  });

  it('binds a GitLab project by its whole path', () => {
    const pipeline = 'https://gitlab.com/group/sub/tool//.gitlab-ci.yml@refs/tags/v1';
    checkProjectSigner('gitlab.com/group/sub/tool', signer(pipeline, GITLAB_ISSUER));
    checkProjectSigner('gitlab.com/group/sub/tool', signer(pipeline, GITLAB_ISSUER, 'https://gitlab.com/group/sub/tool'));
    refusesProject('gitlab.com/group/sub/tool', signer(pipeline, GITLAB_ISSUER, 'https://gitlab.com/group/other/tool'), /group\/other\/tool/);
    refusesProject(
      'gitlab.com/group/sub/tool',
      signer('https://gitlab.com/group/sub/tool-x//.gitlab-ci.yml@refs/tags/v1', GITLAB_ISSUER),
      /tool-x\S+ is not one of its workflows$/,
    );
  });

  it("leaves a project that is not on a forge to the caller's identity policy", () => {
    checkProjectSigner('mise.jdx.dev', signer('jdx@example.com', 'https://accounts.google.com'));
    checkProjectSigner('github.com/jdx', signer('jdx@example.com', 'https://accounts.google.com'));
  });
});

/** Minimal DER: a tag, a definite length, and the concatenated parts. */
function der(tag, ...parts) {
  const body = Buffer.concat(parts.map((part) => Buffer.from(part)));
  const n = body.length;
  const length = n < 0x80 ? [n] : n < 0x100 ? [0x81, n] : [0x82, n >> 8, n & 0xff];
  return Buffer.concat([Buffer.from([tag, ...length]), body]);
}

function oid(text) {
  const [first, second, ...rest] = text.split('.').map(Number);
  const bytes = [40 * first + second];
  for (const n of rest) {
    const digits = [n & 0x7f];
    for (let v = n >> 7; v > 0; v >>= 7) digits.unshift((v & 0x7f) | 0x80);
    bytes.push(...digits);
  }
  return Buffer.from(bytes);
}

/** A non-critical X.509 extension with a DER `value`. */
const extension = (id, value) => der(0x30, der(0x06, oid(id)), der(0x04, value));

/**
 * Only the extensions `certificateSigner` reads, inside the TBS structure it
 * walks: a code signing EKU, the mise workflow as SAN, the GitHub issuer, and
 * `source` (DER bytes) as the Source Repository URI when given.
 */
function fakeCertificate(source) {
  const extensions = [
    extension('2.5.29.37', der(0x30, der(0x06, oid('1.3.6.1.5.5.7.3.3')))),
    extension('2.5.29.17', der(0x30, der(0x86, IDENTITY))),
    extension('1.3.6.1.4.1.57264.1.8', der(0x0c, ISSUER)),
  ];
  if (source !== undefined) extensions.push(extension('1.3.6.1.4.1.57264.1.12', source));
  return der(0x30, der(0x30, der(0xa3, der(0x30, ...extensions))));
}

describe('certificateSigner', () => {
  it('reads the real mise and hk certificates', () => {
    assert.deepEqual(certificateSigner(leafOf(mise)), { identity: IDENTITY, issuer: ISSUER, sourceRepository: 'https://github.com/jdx/mise' });
    assert.deepEqual(certificateSigner(leafOf(hk)), { identity: HK_IDENTITY, issuer: ISSUER, sourceRepository: 'https://github.com/jdx/hk' });
  });

  it('reads a long source repository', () => {
    const long = `https://github.com/jdx/${'x'.repeat(150)}`;
    assert.equal(certificateSigner(fakeCertificate(der(0x0c, long))).sourceRepository, long);
  });

  it('treats an absent or empty source repository as none', () => {
    assert.equal(certificateSigner(fakeCertificate()).sourceRepository, undefined);
    assert.equal(certificateSigner(fakeCertificate(der(0x0c, ''))).sourceRepository, undefined);
  });

  it('refuses a malformed source repository', () => {
    for (const value of [[0x04, 0x01, 0x41], [0x0c, 0x01, 0x41, 0x42], [0x0c, 0x02, 0xc3, 0x28]]) {
      assert.throws(() => certificateSigner(fakeCertificate(value)), { code: 'identity', message: /source repository/ });
    }
    assert.throws(() => certificateSigner(fakeCertificate([0x0c, 0x05, 0x41])), { code: 'bundle', message: /truncated/ });
  });
});

// A throwaway sigstore: a CA standing in for Fulcio and a key standing in for
// Rekor's, passed to verifyRelease as `trustedRoot`.
const now = Date.now();
const fakeCa = generateKeyPairSync('ec', { namedCurve: 'P-256' });
const fakeLog = generateKeyPairSync('ec', { namedCurve: 'P-256' });
const fakeLogKeyId = createHash('sha256').update(fakeLog.publicKey.export({ type: 'spki', format: 'der' })).digest('base64');
const ECDSA_SHA256 = der(0x30, der(0x06, oid('1.2.840.10045.4.3.2')));
const name = (cn) => der(0x30, der(0x31, der(0x30, der(0x06, oid('2.5.4.3')), der(0x0c, cn))));
const utcTime = (ms) => der(0x17, new Date(ms).toISOString().replace(/^\d\d|[-:T]|\.\d+/g, ''));

/** A certificate for `publicKey`, issued by the fake CA under `subject` with `extensions`. */
function issue(subject, publicKey, extensions) {
  const tbs = der(
    0x30,
    der(0xa0, der(0x02, [2])),
    der(0x02, [1]),
    ECDSA_SHA256,
    name('fake fulcio'),
    der(0x30, utcTime(now - 3600_000), utcTime(now + 3600_000)),
    name(subject),
    publicKey.export({ type: 'spki', format: 'der' }),
    der(0xa3, der(0x30, ...extensions)),
  );
  return der(0x30, tbs, ECDSA_SHA256, der(0x03, [0], sign('sha256', tbs, fakeCa.privateKey)));
}

const fakeRoot = issue('fake fulcio', fakeCa.publicKey, [
  extension('2.5.29.19', der(0x30, der(0x01, [0xff]))),
  extension('2.5.29.15', der(0x03, [1, 0x06])),
]);
const fakeTrustedRoot = {
  tlogs: [{ baseUrl: 'https://rekor.invalid', keyId: fakeLogKeyId, publicKey: fakeLog.publicKey.export({ type: 'spki', format: 'der' }).toString('base64'), start: new Date(now - 86400_000).toISOString() }],
  cas: [{ uri: 'https://fulcio.invalid', chain: [fakeRoot.toString('base64')], start: new Date(now - 86400_000).toISOString() }],
};

/**
 * A logged bundle whose statement names `project`, signed by a certificate
 * for the workflow `identity` that records `sourceRepository`, if given. The
 * statement declares that signer unless `declared` replaces its identity block
 * (`null` leaves it out).
 */
function fakeRelease({ project, identity, sourceRepository, declared = { scheme: 'sigstore-oidc', key_id: identity, issuer: ISSUER } }) {
  const signer = generateKeyPairSync('ec', { namedCurve: 'P-256' });
  const leaf = issue('', signer.publicKey, [
    extension('2.5.29.37', der(0x30, der(0x06, oid('1.3.6.1.5.5.7.3.3')))),
    extension('2.5.29.17', der(0x30, der(0x86, identity))),
    extension('1.3.6.1.4.1.57264.1.8', der(0x0c, ISSUER)),
    ...(sourceRepository === undefined ? [] : [extension('1.3.6.1.4.1.57264.1.12', der(0x0c, sourceRepository))]),
  ]);
  const statement = {
    _type: 'https://in-toto.io/Statement/v1',
    subject: [{ name: 'tool.tar.gz', digest: { sha256: '0'.repeat(64) } }],
    predicateType: 'https://packslip.dev/release/v1',
    predicate: { project, version: '1.0.0', artifacts: [{ name: 'tool.tar.gz', size: 1 }], ...(declared === null ? {} : { identity: declared }) },
  };
  const payload = Buffer.from(JSON.stringify(statement));
  const type = 'application/vnd.in-toto+json';
  const pae = Buffer.concat([Buffer.from(`DSSEv1 ${type.length} ${type} ${payload.length} `), payload]);
  const sig = sign('sha256', pae, signer.privateKey).toString('base64');
  const pem = `-----BEGIN CERTIFICATE-----\n${leaf.toString('base64')}\n-----END CERTIFICATE-----\n`;
  const body = Buffer.from(
    JSON.stringify({
      apiVersion: '0.0.1',
      kind: 'dsse',
      spec: {
        payloadHash: { algorithm: 'sha256', value: createHash('sha256').update(payload).digest('hex') },
        signatures: [{ signature: sig, verifier: Buffer.from(pem).toString('base64') }],
      },
    }),
  ).toString('base64');
  const integratedTime = Math.floor(now / 1000) - 60;
  const logID = Buffer.from(fakeLogKeyId, 'base64').toString('hex');
  const set = sign('sha256', Buffer.from(JSON.stringify({ body, integratedTime, logID, logIndex: 0 })), fakeLog.privateKey);
  // A tree of one entry: its root is the leaf hash.
  const root = createHash('sha256').update(Buffer.from([0])).update(Buffer.from(body, 'base64')).digest('base64');
  const note = `rekor.invalid\n1\n${root}\n`;
  const noteSig = Buffer.concat([Buffer.alloc(4), sign('sha256', Buffer.from(note), fakeLog.privateKey)]).toString('base64');
  return JSON.stringify({
    mediaType: 'application/vnd.dev.sigstore.bundle.v0.3+json',
    verificationMaterial: {
      certificate: { rawBytes: leaf.toString('base64') },
      tlogEntries: [
        {
          logIndex: '0',
          logId: { keyId: fakeLogKeyId },
          integratedTime: String(integratedTime),
          inclusionPromise: { signedEntryTimestamp: set.toString('base64') },
          inclusionProof: { logIndex: '0', rootHash: root, treeSize: '1', hashes: [], checkpoint: { envelope: `${note}\n— rekor.invalid ${noteSig}\n` } },
          canonicalizedBody: body,
        },
      ],
    },
    dsseEnvelope: { payload: payload.toString('base64'), payloadType: type, signatures: [{ sig, keyid: '' }] },
  });
}

describe('verifyRelease binds a GitHub project to its repository', () => {
  const anyJdxWorkflow = (san) => san.startsWith('https://github.com/jdx/');
  const options = { issuer: ISSUER, identity: anyJdxWorkflow, project: 'github.com/jdx/mise', trustedRoot: fakeTrustedRoot };
  const MISE_WORKFLOW = 'https://github.com/jdx/mise/.github/workflows/release.yml@refs/tags/v1.0.0';
  const HK_WORKFLOW = 'https://github.com/jdx/hk/.github/workflows/release.yml@refs/tags/v1.0.0';

  it("accepts the project's own workflow", () => {
    const bundle = fakeRelease({ project: 'github.com/jdx/mise', identity: MISE_WORKFLOW, sourceRepository: 'https://github.com/jdx/mise' });
    assert.equal(verifyRelease(bundle, options).signer.identity, MISE_WORKFLOW);
    assert.equal(verifyRelease(bundle, { ...options, identity: MISE_WORKFLOW, sourceRepository: 'https://github.com/jdx/mise' }).predicate.project, 'github.com/jdx/mise');
  });

  it('accepts a reusable workflow of another repository called from the project', () => {
    const shared = 'https://github.com/jdx/shared/.github/workflows/release.yml@refs/heads/main';
    const bundle = fakeRelease({ project: 'github.com/jdx/mise', identity: shared, sourceRepository: 'https://github.com/jdx/mise' });
    assert.equal(verifyRelease(bundle, options).signer.identity, shared);
  });

  it("refuses another repository's signature for the project, though the identity policy accepts its signer", () => {
    const bundle = fakeRelease({ project: 'github.com/jdx/mise', identity: HK_WORKFLOW, sourceRepository: 'https://github.com/jdx/hk' });
    refuses(bundle, options, 'project-signer', /^github\.com\/jdx\/mise must be signed from its repository https:\/\/github\.com\/jdx\/mise, but .* signed it from https:\/\/github\.com\/jdx\/hk$/);
    refuses(bundle, { ...options, identity: () => true }, 'project-signer');
    // Without the source repository, the signer must be a workflow of the project.
    refuses(fakeRelease({ project: 'github.com/jdx/mise', identity: HK_WORKFLOW }), options, 'project-signer', /is not one of its workflows$/);
  });

  it('leaves a bundle verified without a project to the identity policy alone', () => {
    const bundle = fakeRelease({ project: 'github.com/jdx/mise', identity: HK_WORKFLOW, sourceRepository: 'https://github.com/jdx/hk' });
    assert.equal(verifyRelease(bundle, { ...options, project: undefined }).signer.identity, HK_WORKFLOW);
  });

  it("with allowOtherRepository, accepts another repository's signer that the identity policy names, still checking the project", () => {
    const repackager = { ...options, identity: (san) => san.startsWith('https://github.com/jdx/hk/'), allowOtherRepository: true };
    const bundle = fakeRelease({ project: 'github.com/jdx/mise', identity: HK_WORKFLOW, sourceRepository: 'https://github.com/jdx/hk' });
    assert.equal(verifyRelease(bundle, repackager).signer.identity, HK_WORKFLOW);
    refuses(bundle, { ...repackager, project: 'github.com/jdx/hk' }, 'project');
    refuses(bundle, { ...repackager, identity: MISE_WORKFLOW }, 'identity');
  });

  it('refuses allowOtherRepository without a project, which it would have nothing to lift from', () => {
    const bundle = fakeRelease({ project: 'github.com/jdx/mise', identity: HK_WORKFLOW, sourceRepository: 'https://github.com/jdx/hk' });
    refuses(bundle, { ...options, project: undefined, allowOtherRepository: true }, 'usage', /needs a project/);
  });
});

describe('checkDeclaredIdentity', () => {
  const signer = { identity: IDENTITY, issuer: ISSUER };
  const refusesDeclared = (declared, pattern) =>
    assert.throws(
      () => checkDeclaredIdentity(declared, signer),
      (error) => error instanceof VerifyError && error.code === 'declared-identity' && pattern.test(error.message),
    );

  it('accepts a statement that declares the certificate that signed it', () => {
    checkDeclaredIdentity({ scheme: 'sigstore-oidc', key_id: IDENTITY, issuer: ISSUER }, signer);
    checkDeclaredIdentity({ scheme: 'sigstore-oidc', key_id: IDENTITY, issuer: ISSUER, pin_workflow: false }, signer);
  });

  it('refuses another scheme, signer, or issuer, and a missing issuer or identity block', () => {
    refusesDeclared({ scheme: 'sigstore-key', key_id: '5A0A0B8B9C6D7E1F' }, /declares scheme "sigstore-key", but the bundle was signed by an OIDC identity$/);
    refusesDeclared({ scheme: 'sigstore-oidc', key_id: HK_IDENTITY, issuer: ISSUER }, /declares signer ".*hk.*", but the bundle was signed by ".*mise.*"$/);
    refusesDeclared({ scheme: 'sigstore-oidc', key_id: IDENTITY, issuer: GITLAB_ISSUER }, /declares issuer "https:\/\/gitlab\.com", but the certificate says/);
    refusesDeclared({ scheme: 'sigstore-oidc', key_id: IDENTITY }, /declares no issuer, but the certificate says/);
    refusesDeclared(undefined, /declares no identity$/);
    refusesDeclared(['sigstore-oidc', IDENTITY, ISSUER], /declares no identity$/);
  });
});

describe('verifyRelease checks the declared identity', () => {
  const MISE_WORKFLOW = 'https://github.com/jdx/mise/.github/workflows/release.yml@refs/tags/v1.0.0';
  const options = { issuer: ISSUER, identity: MISE_WORKFLOW, project: 'github.com/jdx/mise', trustedRoot: fakeTrustedRoot };
  const release = (declared) => fakeRelease({ project: 'github.com/jdx/mise', identity: MISE_WORKFLOW, sourceRepository: 'https://github.com/jdx/mise', declared });

  it('accepts the real bundles, which declare their signers', () => {
    assert.equal(verifyRelease(mise, policy).predicate.identity.key_id, IDENTITY);
    assert.equal(verifyRelease(hk, { issuer: ISSUER, identity: HK_IDENTITY }).predicate.identity.key_id, HK_IDENTITY);
  });

  it('refuses a statement that names another signer, with or without a project', () => {
    const other = release({ scheme: 'sigstore-oidc', key_id: 'https://github.com/jdx/mise/.github/workflows/other.yml@refs/tags/v1.0.0', issuer: ISSUER });
    refuses(other, options, 'declared-identity', /declares signer/);
    refuses(other, { ...options, project: undefined }, 'declared-identity');
    refuses(release({ scheme: 'sigstore-oidc', key_id: MISE_WORKFLOW }), options, 'declared-identity', /declares no issuer/);
    refuses(release({ scheme: 'sigstore-key', key_id: '5A0A0B8B9C6D7E1F' }), options, 'declared-identity', /declares scheme/);
    refuses(release(null), options, 'declared-identity', /declares no identity/);
  });

  it('checks it even when allowOtherRepository accepts a signer outside the project', () => {
    const HK_WORKFLOW = 'https://github.com/jdx/hk/.github/workflows/release.yml@refs/tags/v1.0.0';
    const bundle = fakeRelease({ project: 'github.com/jdx/mise', identity: HK_WORKFLOW, sourceRepository: 'https://github.com/jdx/hk', declared: { scheme: 'sigstore-oidc', key_id: MISE_WORKFLOW, issuer: ISSUER } });
    refuses(bundle, { ...options, identity: HK_WORKFLOW, allowOtherRepository: true }, 'declared-identity');
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

  it("accepts a broad identity prefix with --project when the project's repository signed", () => {
    const result = run(miseBundlePath, '--issuer', ISSUER, '--identity-prefix', 'https://github.com/jdx/', '--project', 'github.com/jdx/mise');
    assert.equal(result.status, 0, result.stderr);
  });

  it('accepts --allow-other-repository and still checks the project the statement names', () => {
    const flags = ['--issuer', ISSUER, '--identity-prefix', 'https://github.com/jdx/', '--allow-other-repository', '--json'];
    const ok = run(miseBundlePath, ...flags, '--project', 'github.com/jdx/mise');
    assert.equal(ok.status, 0, ok.stderr);
    const other = run(miseBundlePath, ...flags, '--project', 'github.com/jdx/hk');
    assert.equal(other.status, 1);
    assert.equal(JSON.parse(other.stdout).code, 'project');
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

  it('exits 2 for --allow-other-repository without --project', () => {
    const result = run(miseBundlePath, '--issuer', ISSUER, '--identity-prefix', 'https://github.com/jdx/', '--allow-other-repository');
    assert.equal(result.status, 2);
    assert.match(result.stderr, /^--allow-other-repository lifts the repository requirement --project brings, so it needs --project\n/);
  });

  it('exits 2 without a signer policy', () => {
    assert.equal(run(miseBundlePath).status, 2);
    assert.equal(run(miseBundlePath, '--issuer', ISSUER).status, 2);
    assert.equal(run(miseBundlePath, '--identity', IDENTITY).status, 2);
    assert.equal(run(miseBundlePath, '--issuer', ISSUER, '--identity', IDENTITY, '--identity-prefix', 'https://github.com/').status, 2);
  });
});
