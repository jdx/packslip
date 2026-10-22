import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { describe, it } from 'node:test';
import { verify } from 'sigstore';
import { DeclaredIdentityError, IdentityError, ProjectSignerError, StatementError } from '../src/index.ts';
import {
  certificateSigner,
  checkDeclaredIdentity,
  checkProjectSigner,
  GITHUB_ISSUER,
  GITLAB_ISSUER,
  type CertificateSigner,
} from '../src/signer.ts';
import { readRelease } from '../src/statement.ts';

// The bundle jdx/mise v2026.10.11 published as packslip.sigstore.json.
const bundleText = readFileSync(new URL('./mise-2026.10.11.sigstore.json', import.meta.url), 'utf8');
const IDENTITY = 'https://github.com/jdx/mise/.github/workflows/release.yml@refs/tags/v2026.10.11';
const MISE = { identity: IDENTITY, issuer: GITHUB_ISSUER };

// The mise statement as signed, with `edit` applied to its predicate's identity.
function declared(edit: (identity: Record<string, unknown>) => void = () => {}) {
  const bundle = JSON.parse(bundleText);
  const statement = JSON.parse(Buffer.from(bundle.dsseEnvelope.payload, 'base64').toString('utf8'));
  edit(statement.predicate.identity);
  return readRelease(Buffer.from(JSON.stringify(statement)).toString('base64')).model.predicate.identity;
}

function rejectsDeclared(identity: ReturnType<typeof declared>, field: string, actual: string): void {
  assert.throws(
    () => checkDeclaredIdentity(identity, MISE),
    (error: unknown) => {
      assert.ok(error instanceof DeclaredIdentityError, String(error));
      assert.equal(error.code, 'declared-identity');
      assert.equal(error.field, field);
      assert.equal(error.actual, actual);
      return true;
    },
  );
}

describe('checkDeclaredIdentity', () => {
  it('accepts the identity the real mise statement declares', () => {
    checkDeclaredIdentity(declared(), MISE);
  });

  it('never reaches a statement that leaves out the issuer, which the statement checks refuse', () => {
    assert.throws(
      () => declared((identity) => delete identity['issuer']),
      (error: unknown) => error instanceof StatementError && /sigstore-oidc identity needs an issuer/.test(error.message),
    );
    rejectsDeclared({ ...declared(), issuer: undefined }, 'issuer', GITHUB_ISSUER);
  });

  it('rejects a statement that declares a key', () => {
    rejectsDeclared(
      declared((identity) => {
        identity['scheme'] = 'sigstore-key';
        identity['key_id'] = '5A0A0B8B9C6D7E1F';
      }),
      'scheme',
      'sigstore-oidc',
    );
  });

  it('rejects another declared signer, including another ref of the same workflow', () => {
    for (const other of [
      'https://github.com/evil/mise/.github/workflows/release.yml@refs/tags/v2026.10.11',
      'https://github.com/jdx/mise/.github/workflows/release.yml@refs/tags/v2026.10.10',
      'https://github.com/jdx/mise/.github/workflows/release.yml',
    ]) {
      rejectsDeclared(
        declared((identity) => (identity['key_id'] = other)),
        'key_id',
        IDENTITY,
      );
    }
  });

  it('rejects another declared issuer', () => {
    rejectsDeclared(
      declared((identity) => (identity['issuer'] = GITLAB_ISSUER)),
      'issuer',
      GITHUB_ISSUER,
    );
  });

  it('names both sides in the message', () => {
    assert.throws(
      () => checkDeclaredIdentity(declared((identity) => (identity['key_id'] = 'https://example.com/other')), MISE),
      { message: `the statement declares signer "https://example.com/other", but the bundle was signed by "${IDENTITY}"` },
    );
  });
});

function signer(identity: string, issuer: string, sourceRepository?: string): CertificateSigner {
  return { identity, issuer, sourceRepository };
}

function rejectsProject(project: string, s: CertificateSigner, reason: string, message: RegExp): void {
  assert.throws(
    () => checkProjectSigner(project, s),
    (error: unknown) => {
      assert.ok(error instanceof ProjectSignerError, String(error));
      assert.equal(error.code, 'project-signer');
      assert.equal(error.reason, reason);
      assert.equal(error.project, project);
      assert.match(error.message, message);
      return true;
    },
  );
}

describe('checkProjectSigner', () => {
  const RELEASE = 'https://github.com/jdx/mise/.github/workflows/release.yml@refs/tags/v1';
  const SHARED = 'https://github.com/jdx/shared/.github/workflows/release.yml@refs/heads/main';

  it("accepts the project's own workflow, with or without a source repository", () => {
    checkProjectSigner('github.com/jdx/mise', signer(RELEASE, GITHUB_ISSUER, 'https://github.com/jdx/mise'));
    checkProjectSigner('github.com/jdx/mise', signer(RELEASE, GITHUB_ISSUER));
  });

  it("accepts a reusable workflow of another repository when the source repository is the project's", () => {
    checkProjectSigner('github.com/jdx/mise', signer(SHARED, GITHUB_ISSUER, 'https://github.com/jdx/mise'));
  });

  it('binds a monorepo tool to its repository', () => {
    checkProjectSigner(
      'github.com/oxc-project/oxc/oxlint',
      signer('https://github.com/oxc-project/oxc/.github/workflows/release.yml@refs/tags/oxlint_v1.0.0', GITHUB_ISSUER, 'https://github.com/oxc-project/oxc'),
    );
  });

  it('rejects a certificate from another source repository, whichever workflow signed', () => {
    rejectsProject(
      'github.com/jdx/mise',
      signer(RELEASE, GITHUB_ISSUER, 'https://github.com/evil/mise'),
      'source-repository',
      /the certificate's source repository is https:\/\/github.com\/evil\/mise, not https:\/\/github.com\/jdx\/mise/,
    );
    rejectsProject('github.com/jdx/mise', signer(SHARED, GITHUB_ISSUER, 'https://github.com/jdx/shared'), 'source-repository', /jdx\/shared/);
  });

  it('without a source repository, rejects a signer outside the repository', () => {
    rejectsProject('github.com/jdx/mise', signer(SHARED, GITHUB_ISSUER), 'signer', /which is not a workflow of https:\/\/github.com\/jdx\/mise/);
    // The prefix ends at the repository's slash, so a longer name does not pass.
    rejectsProject(
      'github.com/jdx/mise',
      signer('https://github.com/jdx/mise-fork/.github/workflows/release.yml@refs/tags/v1', GITHUB_ISSUER),
      'signer',
      /mise-fork/,
    );
  });

  it("rejects a certificate from another issuer than the forge's", () => {
    rejectsProject(
      'github.com/jdx/mise',
      signer('jdx@example.com', 'https://accounts.google.com'),
      'issuer',
      /github.com\/jdx\/mise is signed through issuer "https:\/\/accounts.google.com", expected https:\/\/token.actions.githubusercontent.com/,
    );
    rejectsProject('gitlab.com/group/tool', signer(RELEASE, GITHUB_ISSUER, 'https://github.com/jdx/mise'), 'issuer', /expected https:\/\/gitlab.com/);
  });

  it('binds a GitLab project by its whole path', () => {
    const pipeline = 'https://gitlab.com/group/sub/tool//.gitlab-ci.yml@refs/tags/v1';
    checkProjectSigner('gitlab.com/group/sub/tool', signer(pipeline, GITLAB_ISSUER));
    checkProjectSigner('gitlab.com/group/sub/tool', signer(pipeline, GITLAB_ISSUER, 'https://gitlab.com/group/sub/tool'));
    rejectsProject('gitlab.com/group/sub/tool', signer(pipeline, GITLAB_ISSUER, 'https://gitlab.com/group/other/tool'), 'source-repository', /group\/other\/tool/);
    rejectsProject(
      'gitlab.com/group/sub/tool',
      signer('https://gitlab.com/group/sub/tool-x//.gitlab-ci.yml@refs/tags/v1', GITLAB_ISSUER),
      'signer',
      /not a workflow of https:\/\/gitlab.com\/group\/sub\/tool/,
    );
  });

  it("leaves a project that is not on a forge to the caller's identity policy", () => {
    checkProjectSigner('mise.jdx.dev', signer('jdx@example.com', 'https://accounts.google.com'));
    checkProjectSigner('github.com/jdx', signer('jdx@example.com', 'https://accounts.google.com'));
  });
});

const SOURCE_REPOSITORY_OID = [1, 3, 6, 1, 4, 1, 57264, 1, 12];

function utf8String(text: string): Uint8Array {
  const bytes = Buffer.from(text, 'utf8');
  const length = bytes.length < 0x80 ? [bytes.length] : [0x81, bytes.length];
  return Buffer.concat([Buffer.from([0x0c, ...length]), bytes]);
}

function sigstoreSigner(value?: Uint8Array) {
  return {
    identity: {
      subjectAlternativeName: IDENTITY,
      extensions: { issuer: GITHUB_ISSUER },
      oids: value === undefined ? [] : [{ oid: { id: SOURCE_REPOSITORY_OID }, value }],
    },
  };
}

describe('certificateSigner', () => {
  it('reads the source repository from the real mise certificate', async () => {
    // Verification fetches sigstore's trust root over TUF, so this needs network access.
    const verified = await verify(JSON.parse(bundleText));
    assert.deepEqual(certificateSigner(verified), {
      identity: IDENTITY,
      issuer: GITHUB_ISSUER,
      sourceRepository: 'https://github.com/jdx/mise',
    });
  });

  it('reads short and long DER lengths', () => {
    assert.equal(certificateSigner(sigstoreSigner(utf8String('https://github.com/jdx/mise'))).sourceRepository, 'https://github.com/jdx/mise');
    const long = `https://github.com/jdx/${'x'.repeat(150)}`;
    assert.equal(certificateSigner(sigstoreSigner(utf8String(long))).sourceRepository, long);
  });

  it('treats an absent or empty source repository as none', () => {
    assert.equal(certificateSigner(sigstoreSigner()).sourceRepository, undefined);
    assert.equal(certificateSigner(sigstoreSigner(utf8String(''))).sourceRepository, undefined);
  });

  it('rejects a malformed source repository extension', () => {
    for (const value of [
      Buffer.from([0x04, 0x01, 0x41]),
      Buffer.from([0x0c, 0x05, 0x41]),
      Buffer.from([0x0c, 0x01, 0x41, 0x42]),
      Buffer.from([0x0c, 0x02, 0xc3, 0x28]),
    ]) {
      assert.throws(() => certificateSigner(sigstoreSigner(value)), IdentityError);
    }
  });

  it('rejects a certificate without an identity or issuer', () => {
    assert.throws(() => certificateSigner({ identity: { extensions: { issuer: GITHUB_ISSUER } } }), IdentityError);
    assert.throws(() => certificateSigner({ identity: { subjectAlternativeName: IDENTITY } }), IdentityError);
  });
});
