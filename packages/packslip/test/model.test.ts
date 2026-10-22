import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { describe, it } from 'node:test';
import { StatementError } from '../src/index.ts';
import { checkStatement, validProject } from '../src/model.ts';
import { readStatement } from '../src/statement.ts';

// The repository's shared vectors. They are not part of the published package,
// so a test run outside a checkout has nothing to read.
const vectorsUrl = new URL('../../../tests/conformance/statement-validity.json', import.meta.url);
const vectors: { cases: { name: string; expect: string; statement: unknown }[] } | undefined = existsSync(vectorsUrl)
  ? JSON.parse(readFileSync(vectorsUrl, 'utf8'))
  : undefined;

const encode = (statement: unknown) => Buffer.from(JSON.stringify(statement)).toString('base64');

// Each case runs through readStatement, which is what verifyRelease runs on the
// payload once sigstore has verified the signature.
describe('statement-validity.json conformance vectors', () => {
  if (vectors === undefined) {
    it('needs the repository checkout', { skip: 'tests/conformance is unavailable outside a checkout' }, () => {});
    return;
  }
  it('has cases of both outcomes', () => {
    assert.ok(vectors.cases.some((c) => c.expect === 'accept'));
    assert.ok(vectors.cases.some((c) => c.expect === 'reject'));
  });
  for (const c of vectors.cases) {
    it(c.name, () => {
      const read = () => readStatement(encode(c.statement), {});
      if (c.expect === 'accept') {
        assert.doesNotThrow(read);
      } else if (c.expect === 'reject') {
        assert.throws(read, StatementError);
      } else {
        assert.fail(`unknown expectation ${JSON.stringify(c.expect)}`);
      }
    });
  }
});

// The cases below port the Rust crate's model tests (src/model.rs), so each
// rule of Statement::validate has a test here as well.
type Json = Record<string, any>;

const ARTIFACT = 'mise-v2026.9.1-linux-x64.tar.xz';
const OIDC = {
  scheme: 'sigstore-oidc',
  key_id: 'https://github.com/jdx/mise/.github/workflows/release.yml@refs/tags/v2026.9.1',
  issuer: 'https://token.actions.githubusercontent.com',
};

function sample(): Json {
  return {
    _type: 'https://in-toto.io/Statement/v1',
    subject: [{ name: ARTIFACT, digest: { sha256: 'a'.repeat(64) } }],
    predicateType: 'https://packslip.dev/release/v1',
    predicate: {
      project: 'github.com/jdx/mise',
      version: '2026.9.1',
      published_at: '2026-09-01T12:00:00Z',
      source: { repo: 'https://github.com/jdx/mise', commit: 'b'.repeat(40), tag: 'v2026.9.1' },
      artifacts: [
        {
          name: ARTIFACT,
          os: 'linux',
          arch: 'x86_64',
          libc: 'gnu',
          size: 12345678,
          url: `https://github.com/jdx/mise/releases/download/v2026.9.1/${ARTIFACT}`,
          format: 'tar.xz',
          bin: ['mise/bin/mise'],
        },
      ],
      identity: { scheme: 'sigstore-key', key_id: '5A0A0B8B9C6D7E1F' },
    },
  };
}

function edited(edit: (s: Json) => void): Json {
  const s = sample();
  edit(s);
  return s;
}

function addSubject(s: Json, name: string): void {
  s.subject.push({ name, digest: { sha256: 'd'.repeat(64) } });
}

function withResource(resource: Json): Json {
  return edited((s) => {
    s.predicate.resources = [resource];
  });
}

function rejects(statement: Json, message: RegExp, code = 'statement'): void {
  assert.throws(
    () => checkStatement(statement),
    (error: unknown) => {
      assert.ok(error instanceof StatementError, String(error));
      assert.equal(error.code, code);
      assert.match(error.message, message);
      return true;
    },
  );
}

describe('checkStatement accepts', () => {
  const accepted: [string, Json][] = [
    ['the sample', sample()],
    ['a sha512', edited((s) => (s.subject[0].digest.sha512 = 'f'.repeat(128)))],
    [
      'null for every optional scalar',
      edited((s) => {
        s.subject[0].digest.sha512 = null;
        s.predicate.source.tag = null;
        s.predicate.artifacts[0].variant = null;
        s.predicate.artifacts[0].requires = null;
        s.predicate.identity.issuer = null;
        s.predicate.identity.pin_workflow = null;
        s.predicate.notes_url = null;
      }),
    ],
    ['a null source', edited((s) => (s.predicate.source = null))],
    ['a repackager attestation', edited((s) => (s.predicate.attested_by = 'repackager'))],
    ['a workflow that is not pinned', edited((s) => (s.predicate.identity.pin_workflow = false))],
    ['a prerelease with build metadata', edited((s) => (s.predicate.version = '1.2.0-rc.1+build.5'))],
    ['an offset timestamp with a fraction', edited((s) => (s.predicate.published_at = '2026-09-01T12:00:00.123456789+02:00'))],
    ['a lowercase separator and zone', edited((s) => (s.predicate.published_at = '2026-09-01t12:00:00z'))],
    ['a space separator', edited((s) => (s.predicate.published_at = '2026-09-01 12:00:00Z'))],
    ['a leap day and a leap second', edited((s) => (s.predicate.published_at = '2024-02-29T23:59:60Z'))],
    // A consumer accepts any RFC 3339 offset and compares the instants.
    ['a negative offset', edited((s) => (s.predicate.published_at = '2026-09-01T04:30:00-07:30'))],
    ['an unknown local offset', edited((s) => (s.predicate.published_at = '2026-09-01T12:00:00-00:00'))],
    ['the largest offset', edited((s) => (s.predicate.published_at = '2026-09-02T11:59:00+23:59'))],
    ['a keyless identity', edited((s) => (s.predicate.identity = { ...OIDC }))],
    ['extensions', edited((s) => {
      s.predicate.extensions = { 'example.com': { build_id: '1' } };
      s.predicate.artifacts[0].extensions = { mise: { install_hint: 'brew' } };
    })],
    [
      'host requirements',
      edited((s) => {
        s.predicate.artifacts[0].requires = {
          os_min: '12',
          glibc_min: '2.31',
          libs: ['libssl.so.3', 'libz.so.1'],
          bin: [{ name: 'java', min: '17' }, { name: 'git' }],
        };
      }),
    ],
    ['an empty libs list', edited((s) => (s.predicate.artifacts[0].requires = { libs: [] }))],
    // A valid minimum that cannot be compared leaves the check unknown.
    ['a minimum that cannot be compared', edited((s) => (s.predicate.artifacts[0].requires = { bin: [{ name: 'java', min: '17-ea' }] }))],
    // A vendor must write these in the minimum grammar, but a consumer does
    // not refuse a document whose glibc_min or os_min breaks it.
    [
      'a glibc_min and an os_min outside the minimum grammar',
      edited((s) => (s.predicate.artifacts[0].requires = { os_min: 'v12', glibc_min: 'glibc-2.31' })),
    ],
    [
      'a Windows bin name that canonicalizes',
      edited((s) => (s.predicate.artifacts[0].bin = ['bin/tool.exe', { path: 'tool-x64.exe', name: 'tool.exe' }])),
    ],
    [
      'a bare gz executable named without its suffix',
      edited((s) => {
        s.subject[0].name = 'mise-linux-x64.gz';
        Object.assign(s.predicate.artifacts[0], {
          name: 'mise-linux-x64.gz',
          format: 'gz',
          bin: [{ path: 'mise-linux-x64', name: 'mise' }],
        });
      }),
    ],
    [
      'every documented resource kind from every fitting source',
      edited((s) => {
        addSubject(s, 'mise-skill.tar.gz');
        s.predicate.resources = [
          { kind: 'completion', shell: 'zsh', archive: 'mise/share/zsh/site-functions/_mise' },
          { kind: 'completion', shell: 'fish', repo: 'completions/mise.fish' },
          { kind: 'completion', shells: ['bash', 'zsh', 'fish'], exec: ['mise', 'completion', '{shell}'] },
          { kind: 'man', archive: 'mise/man/man1/mise.1' },
          { kind: 'cli-spec', format: 'usage', bin: 'mise', exec: ['mise', 'usage'] },
          { kind: 'skill', name: 'mise', asset: 'mise-skill.tar.gz', url: 'https://dl.example/mise-skill.tar.gz' },
          { kind: 'desktop', archive: 'share/applications/mise.desktop' },
          { kind: 'icon', archive: 'share/icons/hicolor/512x512/apps/mise.png' },
          { kind: 'app', archive: 'Mise.app' },
          { kind: 'font', archive: 'fonts/Mise.ttf' },
        ];
      }),
    ],
    [
      'a completion that names its shells in the environment',
      withResource({ kind: 'completion', shells: ['bash', 'zsh'], exec: ['mise'], env: { COMPLETE: '{shell}' } }),
    ],
    [
      'a cli-spec for a Windows bin by its bare name',
      edited((s) => {
        s.predicate.artifacts[0].bin = ['mise.exe'];
        s.predicate.resources = [{ kind: 'cli-spec', format: 'usage', bin: 'mise', exec: ['mise', 'usage'] }];
      }),
    ],
    [
      'an exec of a bin by its Windows form',
      withResource({ kind: 'man', exec: ['mise.exe', 'man'] }),
    ],
    ['a man page for a bin by its Windows form, in any case', withResource({ kind: 'man', bin: 'mise.EXE', archive: 'mise.1' })],
    [
      'a man page without bin in a release whose Windows and Unix artifacts ship one command',
      edited((s) => {
        addSubject(s, 'mise-windows-x64.zip');
        s.predicate.artifacts.push({
          name: 'mise-windows-x64.zip',
          os: 'windows',
          arch: 'x86_64',
          size: 1,
          format: 'zip',
          bin: ['mise/bin/mise.exe'],
        });
        s.predicate.resources = [{ kind: 'man', archive: 'mise/man/man1/mise.1' }];
      }),
    ],
    [
      'a completion that names one of several executables',
      edited((s) => {
        s.predicate.artifacts[0].bin.push('bin/other');
        s.predicate.resources = [{ kind: 'completion', shell: 'zsh', archive: 'share/_x', bin: 'mise.exe' }];
      }),
    ],
    [
      'an SBOM asset',
      edited((s) => {
        addSubject(s, 'mise.cdx.json');
        s.predicate.resources = [{ kind: 'sbom', format: 'cyclonedx', asset: 'mise.cdx.json' }];
      }),
    ],
    ['a repo resource pinned by source.commit', withResource({ kind: 'man', repo: 'man/mise.1' })],
    [
      'portable artifacts in two formats',
      edited((s) => {
        addSubject(s, 'mise.zip');
        delete s.predicate.artifacts[0].os;
        s.predicate.artifacts.push({ ...s.predicate.artifacts[0], name: 'mise.zip', format: 'zip' });
      }),
    ],
  ];
  for (const [name, statement] of accepted) {
    it(name, () => {
      checkStatement(statement);
    });
  }

  it('project names that are host paths', () => {
    for (const ok of [
      'github.com/jdx/mise',
      'mise.jdx.dev',
      'jdx.dev/mise',
      'gitlab.com/group/sub/proj',
      'example.org/tool_1.2~x',
      'github.com/oxc-project/oxc/oxlint',
    ]) {
      assert.ok(validProject(ok), ok);
    }
  });
});

describe('checkStatement rejects', () => {
  it('a release list or another statement type', () => {
    rejects(edited((s) => (s.predicateType = 'https://packslip.dev/releases/v1')), /predicateType must be/, 'predicate-type');
    rejects(edited((s) => (s._type = 'x')), /_type must be https:\/\/in-toto.io\/Statement\/v1, got "x"/);
  });

  it('project names that are not host paths', () => {
    for (const bad of [
      '',
      'mise',
      'localhost/x',
      'pkg:github/jdx/mise',
      'https://github.com/jdx/mise',
      'GitHub.com/jdx/mise',
      'github.com/',
      'github.com//mise',
      'github.com/jdx/mise/',
      'github.com/../mise',
      '.github.com/x',
      'git hub.com/x',
    ]) {
      assert.ok(!validProject(bad), bad);
      rejects(edited((s) => (s.predicate.project = bad)), /project must be a host path/);
    }
  });

  it('versions that are not semver', () => {
    rejects(edited((s) => (s.predicate.version = '')), /version must not be empty/);
    for (const bad of ['1', '1.2', 'v1.2.3', '1.2.3.4', '2026-09-04', 'nightly', '1.2.3-01']) {
      rejects(edited((s) => (s.predicate.version = bad)), /version must be semver 2\.0\.0/);
    }
  });

  it('timestamps that are not RFC 3339', () => {
    for (const bad of [
      'yesterday',
      'September 2026',
      '2026-09-01',
      '2026-09-01T12:00:00',
      '2026-02-29T12:00:00Z',
      '2026-13-01T12:00:00Z',
      '2026-09-01T24:00:00Z',
      // Spellings the Rust crate's parser also takes, which are not RFC 3339.
      '2026-09-01T12:00:00+24:00',
      '2026-09-01T12:00:00+0200',
      '2026-09-01T12:00:00+02',
      '2026-09-01T12:00Z',
      '2026-09-01T12Z',
      '20260901T120000Z',
      '2026-09-01T12:00:00Z[UTC]',
      '2026-09-01T12:00:00,5Z',
      '+002026-09-01T12:00:00Z',
    ]) {
      rejects(edited((s) => (s.predicate.published_at = bad)), /published_at must be RFC 3339/);
    }
  });

  it('timestamps beyond nanosecond precision, which the Rust crate also refuses', () => {
    // RFC 3339 allows any number of fractional digits. The Rust crate's parser
    // reads at most nine, and so does this one; nine are accepted above.
    rejects(
      edited((s) => (s.predicate.published_at = '2026-09-01T12:00:00.1234567890Z')),
      /published_at must be RFC 3339/,
    );
  });

  const rejected: [string, Json, RegExp][] = [
    ['an uppercase sha256', edited((s) => (s.subject[0].digest.sha256 = 'A'.repeat(64))), /sha256 for ".*" must be 64/],
    ['a short sha512', edited((s) => (s.subject[0].digest.sha512 = 'f'.repeat(127))), /sha512 for ".*" must be 128/],
    ['a subject with no artifact or asset', edited((s) => addSubject(s, 'other')), /subject "other" has no matching artifact/],
    [
      'a duplicate artifact',
      edited((s) => s.predicate.artifacts.push(structuredClone(s.predicate.artifacts[0]))),
      /artifact ".*" appears more than once/,
    ],
    ['a duplicate subject', edited((s) => s.subject.push(structuredClone(s.subject[0]))), /subject ".*" appears more than once/],
    [
      'no artifacts',
      edited((s) => {
        s.subject = [];
        s.predicate.artifacts = [];
      }),
      /at least one artifact is required/,
    ],
    ['an artifact no subject pins', edited((s) => (s.subject[0].name = 'other')), /artifact ".*" has no matching subject/],
    ['an uppercase os', edited((s) => (s.predicate.artifacts[0].os = 'Linux')), /has os "Linux"/],
    ['an empty variant', edited((s) => (s.predicate.artifacts[0].variant = '')), /has variant ""/],
    ['a resource with an uppercase arch', withResource({ kind: 'man', arch: 'X64', archive: 'm.1' }), /has arch "X64"/],
    [
      'a missing format',
      edited((s) => {
        delete s.predicate.artifacts[0].format;
        s.predicate.artifacts[0].bin = [];
      }),
      /has no format, so no consumer can select it/,
    ],
    ['a null format', edited((s) => (s.predicate.artifacts[0].format = null)), /has no format/],
    [
      'a bare executable with the wrong bin',
      edited((s) => {
        s.subject[0].name = 'mise-linux-x64.gz';
        Object.assign(s.predicate.artifacts[0], { name: 'mise-linux-x64.gz', format: 'gz', bin: ['mise'] });
      }),
      /is a bare executable, so its bin must be "mise-linux-x64", not "mise"/,
    ],
    [
      'an executable name with a slash',
      edited((s) => (s.predicate.artifacts[0].bin = [{ path: 'x', name: 'a/b' }])),
      /executable name containing a slash: "a\/b"/,
    ],
    [
      'an executable with an empty path',
      edited((s) => (s.predicate.artifacts[0].bin = [{ path: '', name: 'a' }])),
      /executable with an empty path/,
    ],
    [
      'an executable with an empty name',
      edited((s) => (s.predicate.artifacts[0].bin = [{ path: 'x', name: '' }])),
      /executable with an empty name/,
    ],
    [
      'an executable named only .exe',
      edited((s) => (s.predicate.artifacts[0].bin = [{ path: 'x', name: '.exe' }])),
      /executable with an empty name/,
    ],
    ['an executable path ending in a slash', edited((s) => (s.predicate.artifacts[0].bin = ['mise/bin/'])), /executable with an empty name/],
    [
      'an executable name that is still .exe once canonical',
      edited((s) => (s.predicate.artifacts[0].bin = [{ path: 'x', name: 'tool.EXE.exe' }])),
      /executable name ending in \.exe: "tool\.EXE"/,
    ],
    [
      'two artifacts a consumer could not choose between',
      edited((s) => {
        addSubject(s, 'twin.tar.xz');
        s.predicate.artifacts.push({ ...s.predicate.artifacts[0], name: 'twin.tar.xz' });
      }),
      /artifacts ".*" and "twin.tar.xz" describe the same os, arch, libc, variant, and format/,
    ],
    ['an empty extension key', edited((s) => (s.predicate.extensions = { '': 1 })), /the release has an extension with an empty key/],
    [
      'an empty artifact extension key',
      edited((s) => (s.predicate.artifacts[0].extensions = { '': 1 })),
      /artifact ".*" has an extension with an empty key/,
    ],
    [
      'an empty resource extension key',
      withResource({ kind: 'man', archive: 'm.1', extensions: { '': 1 } }),
      /resource "man" has an extension with an empty key/,
    ],
    // Host requirements.
    ['a library path', edited((s) => (s.predicate.artifacts[0].requires = { libs: ['lib/libz.so.1'] })), /requires library "lib\/libz.so.1"/],
    ['a library with a space', edited((s) => (s.predicate.artifacts[0].requires = { libs: ['lib z'] })), /requires library "lib z"/],
    ['a library twice', edited((s) => (s.predicate.artifacts[0].requires = { libs: ['libz.so.1', 'libz.so.1'] })), /requires library .* twice/],
    ['a command path', edited((s) => (s.predicate.artifacts[0].requires = { bin: [{ name: 'bin/java' }] })), /requires command "bin\/java"/],
    ['a command with .exe', edited((s) => (s.predicate.artifacts[0].requires = { bin: [{ name: 'java.exe' }] })), /requires command "java.exe"/],
    [
      'a minimum that does not start with a digit',
      edited((s) => (s.predicate.artifacts[0].requires = { bin: [{ name: 'java', min: 'v17' }] })),
      /with minimum "v17"/,
    ],
    ['an empty minimum', edited((s) => (s.predicate.artifacts[0].requires = { bin: [{ name: 'java', min: '' }] })), /with minimum ""/],
    [
      'a command twice',
      edited((s) => (s.predicate.artifacts[0].requires = { bin: [{ name: 'java' }, { name: 'java', min: '17' }] })),
      /requires command "java" twice/,
    ],
    [
      'a command the release provides',
      edited((s) => (s.predicate.artifacts[0].requires = { bin: [{ name: 'mise' }] })),
      /requires command "mise", which the release itself provides/,
    ],
    [
      'a command the release provides under its Windows form',
      edited((s) => {
        s.predicate.artifacts[0].bin = ['bin/mise.exe'];
        s.predicate.artifacts[0].requires = { bin: [{ name: 'mise' }] };
      }),
      /requires command "mise", which the release itself provides/,
    ],
    // Resources.
    ['an empty kind', withResource({ kind: '' }), /a resource has an empty kind/],
    ['no source', withResource({ kind: 'man' }), /must have exactly one of archive, asset, repo, exec/],
    ['two sources', withResource({ kind: 'man', archive: 'y', repo: 'x' }), /must have exactly one of/],
    ['an absolute archive path', withResource({ kind: 'man', archive: '/etc/passwd' }), /archive path must be relative/],
    ['an archive path with ..', withResource({ kind: 'man', archive: 'a/../b' }), /archive path must be relative/],
    ['an empty repo path', withResource({ kind: 'man', repo: '' }), /repo path must be relative/],
    [
      'a repo source without source.commit',
      edited((s) => {
        delete s.predicate.source.commit;
        s.predicate.resources = [{ kind: 'man', repo: 'man/mise.1' }];
      }),
      /comes from the repository, which needs source.commit/,
    ],
    ['an asset that is not a subject', withResource({ kind: 'man', asset: 'nope.tar.gz' }), /names asset "nope.tar.gz", which is not a subject/],
    ['a url on an archive source', withResource({ kind: 'man', archive: 'm.1', url: 'https://x' }), /has a url but its source is not an asset/],
    ['an exec of another program', withResource({ kind: 'man', exec: ['other'] }), /runs "other", which is not a bin name/],
    ['a completion with neither shell nor shells', withResource({ kind: 'completion', archive: '_mise' }), /needs exactly one of shell or shells/],
    [
      'a completion with both shell and shells',
      withResource({ kind: 'completion', shell: 'zsh', shells: ['bash'], archive: '_mise' }),
      /needs exactly one of shell or shells/,
    ],
    [
      'static shells',
      withResource({ kind: 'completion', shells: ['bash'], archive: '_mise' }),
      /lists shells, so its source must be exec with a \{shell\} placeholder/,
    ],
    ['env on an archive source', withResource({ kind: 'man', archive: 'm.1', env: { X: 'y' } }), /has env but its source is not exec/],
    [
      'an env name with =',
      withResource({ kind: 'man', exec: ['mise', 'man'], env: { 'A=B': 'y' } }),
      /has an env variable named "A=B"/,
    ],
    ['a shell with a space', withResource({ kind: 'completion', shell: ' zsh', archive: '_mise' }), /names shell " zsh"/],
    [
      'an empty shell',
      withResource({ kind: 'completion', shells: ['bash', ''], exec: ['mise', 'completion', '{shell}'] }),
      /names shell ""/,
    ],
    [
      'exec completion with a static shell',
      withResource({ kind: 'completion', shell: 'bash', exec: ['mise', 'completion', 'bash'] }),
      /lists shells, so its source must be exec/,
    ],
    [
      'exec completion without a placeholder',
      withResource({ kind: 'completion', shells: ['bash'], exec: ['mise', 'completion'] }),
      /lists shells, so its source must be exec/,
    ],
    ['a cli-spec without bin', withResource({ kind: 'cli-spec', format: 'usage', archive: 'mise.kdl' }), /cli-spec ".*" needs format and bin/],
    [
      'a cli-spec for another program',
      withResource({ kind: 'cli-spec', format: 'usage', bin: 'other', archive: 'mise.kdl' }),
      /describes "other", which is not a bin name/,
    ],
    ['a man page for another program', withResource({ kind: 'man', bin: 'other', archive: 'mise.1' }), /is for "other", which is not a bin name/],
    [
      'a man page in a release of several executables',
      edited((s) => {
        s.predicate.artifacts[0].bin.push('bin/other');
        s.predicate.resources = [{ kind: 'man', archive: 'share/man/other.1' }];
      }),
      /does not say which executable it is for, and the release has several: mise, other/,
    ],
    ['a skill without a name', withResource({ kind: 'skill', archive: 'skills/mise' }), /skill ".*" needs a name without a slash/],
    ['a skill name with a slash', withResource({ kind: 'skill', name: 'a/b', archive: 'skills/mise' }), /needs a name without a slash/],
    ['an app from the repository', withResource({ kind: 'app', repo: 'Mise.app' }), /app ".*" must come from an archive/],
    [
      'an SBOM without a format',
      edited((s) => {
        addSubject(s, 'mise.cdx.json');
        s.predicate.resources = [{ kind: 'sbom', asset: 'mise.cdx.json' }];
      }),
      /sbom ".*" needs a format/,
    ],
    ['an SBOM from exec', withResource({ kind: 'sbom', format: 'cyclonedx', exec: ['mise', 'sbom'] }), /sbom ".*" needs a format .* not exec/],
    [
      'a path inside a bare artifact',
      edited((s) => {
        s.subject[0].name = 'mise';
        Object.assign(s.predicate.artifacts[0], { name: 'mise', format: 'raw', bin: ['mise'] });
        s.predicate.resources = [{ kind: 'man', artifact: 'mise', archive: 'mise.1' }];
      }),
      /names a path inside artifact "mise", which is a bare raw executable/,
    ],
    [
      'an artifact outside the resource scope',
      withResource({ kind: 'man', artifact: ARTIFACT, os: 'darwin', archive: 'mise.1' }),
      /names artifact ".*", which is absent or outside its platform scope/,
    ],
    // Field types, which serde checks in the Rust crate.
    ['a missing predicate', edited((s) => delete s.predicate), /^predicate is required$/],
    ['a missing size', edited((s) => delete s.predicate.artifacts[0].size), /predicate\.artifacts\[0\]\.size is required/],
    ['a negative size', edited((s) => (s.predicate.artifacts[0].size = -1)), /size must be a non-negative integer/],
    ['a fractional size', edited((s) => (s.predicate.artifacts[0].size = 1.5)), /size must be a non-negative integer/],
    ['a string size', edited((s) => (s.predicate.artifacts[0].size = '5')), /size must be a non-negative integer/],
    ['a missing published_at', edited((s) => delete s.predicate.published_at), /published_at is required/],
    ['an unknown scheme', edited((s) => (s.predicate.identity.scheme = 'pgp')), /scheme must be sigstore-oidc or sigstore-key/],
    ['a missing key_id', edited((s) => delete s.predicate.identity.key_id), /identity\.key_id is required/],
    ['a keyless identity with an empty key_id', edited((s) => (s.predicate.identity = { ...OIDC, key_id: '' })), /sigstore-oidc identity needs a key_id/],
    [
      'a keyless identity without an issuer',
      edited((s) => (s.predicate.identity = { scheme: 'sigstore-oidc', key_id: OIDC.key_id })),
      /sigstore-oidc identity needs an issuer/,
    ],
    ['a keyless identity with a null issuer', edited((s) => (s.predicate.identity = { ...OIDC, issuer: null })), /needs an issuer/],
    ['a keyless identity with an empty issuer', edited((s) => (s.predicate.identity = { ...OIDC, issuer: '' })), /needs an issuer/],
    // serde also reads an object written as an array of its fields; the specification does not.
    ['requires as an array', edited((s) => (s.predicate.artifacts[0].requires = [])), /predicate\.artifacts\[0\]\.requires must be an object/],
    [
      'an identity as an array',
      edited((s) => (s.predicate.identity = [OIDC.scheme, OIDC.key_id, OIDC.issuer])),
      /predicate\.identity must be an object/,
    ],
    ['a source as an array', edited((s) => (s.predicate.source = ['https://github.com/jdx/mise'])), /predicate\.source must be an object/],
    ['a digest as an array', edited((s) => (s.subject[0].digest = ['a'.repeat(64)])), /subject\[0\]\.digest must be an object/],
    ['a bin as an array', edited((s) => (s.predicate.artifacts[0].bin = [['mise/bin/mise', 'mise']])), /bin\[0\] must be a path, or an object/],
    ['extensions as an array', edited((s) => (s.predicate.extensions = [])), /predicate\.extensions must be an object/],
    ['a string pin_workflow', edited((s) => (s.predicate.identity.pin_workflow = 'false')), /pin_workflow must be a boolean/],
    ['an unknown attestor', edited((s) => (s.predicate.attested_by = 'mirror')), /attested_by must be vendor or repackager/],
    ['a null attestor', edited((s) => (s.predicate.attested_by = null)), /attested_by must be vendor or repackager/],
    ['a source without repo', edited((s) => delete s.predicate.source.repo), /source\.repo is required/],
    ['null resources', edited((s) => (s.predicate.resources = null)), /predicate\.resources must be an array/],
    ['null extensions', edited((s) => (s.predicate.extensions = null)), /predicate\.extensions must be an object/],
    ['a non-string env value', withResource({ kind: 'man', exec: ['mise'], env: { X: 1 } }), /env\.X must be a string/],
    [
      'an object bin without a name',
      edited((s) => (s.predicate.artifacts[0].bin = [{ path: 'tool' }])),
      /bin\[0\] must be a path, or an object with a path and a name/,
    ],
    ['a subject without a digest', edited((s) => delete s.subject[0].digest), /subject\[0\]\.digest is required/],
    ['a non-string sha256', edited((s) => (s.subject[0].digest.sha256 = 1)), /subject\[0\]\.digest\.sha256 must be a string/],
  ];
  for (const [name, statement, message] of rejected) {
    it(name, () => rejects(statement, message));
  }

  it('anything but an object', () => {
    for (const value of [null, [], 'x', 1]) rejects(value as never, /the statement must be an object/);
  });
});
