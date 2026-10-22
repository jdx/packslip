/** A github.com or gitlab.com project split into its repository's name and a monorepo subpath. */
function forgeName(project: string): { host: string; repository: string; subpath: string } | undefined {
  const [host, ...path] = project.split('/');
  if (host === 'github.com' && path.length >= 2 && path[0] !== '' && path[1] !== '') {
    return { host, repository: `${path[0]}/${path[1]}`, subpath: path.slice(2).join('/') };
  }
  if (host === 'gitlab.com' && path.length >= 1 && path.every((segment) => segment !== '')) {
    return { host, repository: path.join('/'), subpath: '' };
  }
  return undefined;
}

const foldAscii = (text: string): string => text.replace(/[A-Z]/g, (c) => c.toLowerCase());

/**
 * Whether two project names name the same project. On github.com and gitlab.com
 * the repository's name (`owner/repo`, or the GitLab project path) compares
 * without regard to ASCII case, as the forge resolves it; a monorepo subpath,
 * and a name on any other host, compare exactly.
 */
export function sameProject(a: string, b: string): boolean {
  const x = forgeName(a);
  const y = forgeName(b);
  if (x === undefined || y === undefined) return a === b;
  return x.host === y.host && foldAscii(x.repository) === foldAscii(y.repository) && x.subpath === y.subpath;
}

/**
 * Where a repository's name lies in a GitHub or GitLab URL or certificate
 * identity: `owner/repo` after `https://github.com/`, or the project path
 * before `//` after `https://gitlab.com/`. Undefined for any other URL.
 */
function repositorySpan(url: string): [number, number] | undefined {
  if (url.startsWith('https://github.com/')) {
    const start = 'https://github.com/'.length;
    const owner = url.indexOf('/', start);
    const repo = owner < 0 ? -1 : url.indexOf('/', owner + 1);
    return [start, repo < 0 ? url.length : repo];
  }
  if (url.startsWith('https://gitlab.com/')) {
    const start = 'https://gitlab.com/'.length;
    const config = url.indexOf('//', start);
    return [start, config < 0 ? url.length : config];
  }
  return undefined;
}

/**
 * Whether the certificate identity or URL `identity` starts with `prefix`. The
 * GitHub or GitLab repository name that `prefix` spells compares without
 * regard to ASCII case, as the forge resolves it; everything else, including a
 * workflow's path and ref, compares exactly.
 */
export function identityHasPrefix(identity: string, prefix: string): boolean {
  const span = repositorySpan(prefix);
  if (span === undefined) return identity.startsWith(prefix);
  const [start, end] = span;
  return (
    identity.length >= end &&
    identity.slice(0, start) === prefix.slice(0, start) &&
    foldAscii(identity.slice(start, end)) === foldAscii(prefix.slice(start, end)) &&
    identity.slice(end).startsWith(prefix.slice(end))
  );
}

/**
 * Whether two certificate identities are the same. A GitHub or GitLab
 * repository's name in them compares without regard to ASCII case, as the
 * forge resolves it; everything else, including a workflow's path and ref,
 * compares exactly.
 */
export function sameIdentity(actual: string, expected: string): boolean {
  const span = repositorySpan(expected);
  if (span === undefined || actual.length !== expected.length) return actual === expected;
  const [start, end] = span;
  return (
    actual.slice(0, start) === expected.slice(0, start) &&
    foldAscii(actual.slice(start, end)) === foldAscii(expected.slice(start, end)) &&
    actual.slice(end) === expected.slice(end)
  );
}
