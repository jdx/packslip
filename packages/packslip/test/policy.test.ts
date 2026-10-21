import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { describe, it } from 'node:test';
import { DeclaredIdentityError, IdentityError, ProjectSignerError, StatementError, verifyRelease } from '../src/index.ts';
import { checkOptions, checkSigned } from '../src/policy.ts';
import { GITHUB_ISSUER, type CertificateSigner } from '../src/signer.ts';

// The bundle jdx/mise v2026.10.11 published as packslip.sigstore.json. Its
// statement is re-encoded below with another declared signer, which only
// checkSigned can be given: sigstore would refuse the altered envelope.
const bundleText = readFileSync(new URL('./mise-2026.10.11.sigstore.json', import.meta.url), 'utf8');
const MISE_IDENTITY = 'https://github.com/jdx/mise/.github/workflows/release.yml@refs/tags/v2026.10.11';
const HK_IDENTITY = 'https://github.com/jdx/hk/.github/workflows/release.yml@refs/tags/v2.3.0';

/** A certificate from jdx/hk's release workflow, which a repackager of mise might use. */
const HK: CertificateSigner = { identity: HK_IDENTITY, issuer: GITHUB_ISSUER, sourceRepository: 'https://github.com/jdx/hk' };

/** The mise statement as a payload, declaring `signer` as its identity. */
function payloadDeclaring(signer: string): string {
  const statement = JSON.parse(Buffer.from(JSON.parse(bundleText).dsseEnvelope.payload, 'base64').toString('utf8'));
  statement.predicate.identity.key_id = signer;
  return Buffer.from(JSON.stringify(statement)).toString('base64');
}

describe('allowOtherRepository', () => {
  const repackaged = payloadDeclaring(HK_IDENTITY);
  const policy = { identity: (san: string) => san.startsWith('https://github.com/jdx/hk/'), issuer: GITHUB_ISSUER, project: 'github.com/jdx/mise' };

  it("refuses another repository's signer for the project without it, though identity accepts the signer", () => {
    assert.throws(
      () => checkSigned(HK, repackaged, policy),
      (error: unknown) =>
        error instanceof ProjectSignerError && error.reason === 'source-repository' && error.sourceRepository === 'https://github.com/jdx/hk',
    );
  });

  it('accepts that signer with it, and still checks the project the statement names', () => {
    const statement = checkSigned(HK, repackaged, { ...policy, allowOtherRepository: true });
    assert.equal(statement.predicate.project, 'github.com/jdx/mise');
    assert.deepEqual(statement.signer, { identity: HK_IDENTITY, issuer: GITHUB_ISSUER });
    assert.throws(
      () => checkSigned(HK, repackaged, { ...policy, project: 'github.com/jdx/hk', allowOtherRepository: true }),
      (error: unknown) => error instanceof StatementError && error.code === 'project',
    );
  });

  it('leaves identity, issuer, and the declared signer in force', () => {
    const allowed = { ...policy, allowOtherRepository: true };
    assert.throws(() => checkSigned(HK, repackaged, { ...allowed, identity: MISE_IDENTITY }), IdentityError);
    assert.throws(() => checkSigned(HK, repackaged, { ...allowed, issuer: 'https://gitlab.com' }), IdentityError);
    assert.throws(() => checkSigned(HK, payloadDeclaring(MISE_IDENTITY), allowed), DeclaredIdentityError);
  });

  it('needs a project and an identity that names the signer', () => {
    assert.throws(() => checkOptions({ ...policy, project: undefined, allowOtherRepository: true } as never), {
      name: 'TypeError',
      message: /needs a project$/,
    });
    assert.throws(() => checkOptions({ issuer: GITHUB_ISSUER, project: 'github.com/jdx/mise', allowOtherRepository: true } as never), {
      name: 'TypeError',
      message: /needs an identity that names the signer it accepts$/,
    });
    checkOptions({ ...policy, allowOtherRepository: true });
    checkOptions({ ...policy, project: undefined, allowOtherRepository: false } as never);
  });

  it('refuses a misused option before reading the bundle', async () => {
    await assert.rejects(verifyRelease('not json', { identity: MISE_IDENTITY, issuer: GITHUB_ISSUER, allowOtherRepository: true }), {
      name: 'TypeError',
      message: /needs a project$/,
    });
  });
});

describe('checkSigned', () => {
  const MISE: CertificateSigner = { identity: MISE_IDENTITY, issuer: GITHUB_ISSUER, sourceRepository: 'https://github.com/jdx/mise' };
  const payload = payloadDeclaring(MISE_IDENTITY);

  it("returns the statement a project's own repository signed, with or without allowOtherRepository", () => {
    for (const allowOtherRepository of [false, true]) {
      const statement = checkSigned(MISE, payload, {
        identity: MISE_IDENTITY,
        issuer: GITHUB_ISSUER,
        project: 'github.com/jdx/mise',
        tag: 'v2026.10.11',
        allowOtherRepository,
      });
      assert.equal(statement.predicate.version, '2026.10.11');
    }
  });

  it('matches a GitHub repository name in a string identity and the project in any case', () => {
    const statement = checkSigned(MISE, payload, {
      identity: MISE_IDENTITY.replace('jdx/mise', 'JDX/Mise'),
      issuer: GITHUB_ISSUER,
      project: 'github.com/JDX/MISE',
    });
    assert.deepEqual(statement.signer, { identity: MISE_IDENTITY, issuer: GITHUB_ISSUER });
    assert.throws(
      () => checkSigned(MISE, payload, { identity: MISE_IDENTITY.replace('release.yml', 'Release.yml'), issuer: GITHUB_ISSUER }),
      IdentityError,
    );
  });
});
