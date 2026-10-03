// packslip.sh: packslip's install scripts. Each release publishes
// install.sh and install.ps1 beside its other files in R2, filled in with
// the SHA-256 of every executable it ships (installer/render.sh). This
// Worker only chooses which release's script to serve; it never writes or
// alters one.
//
//   /                    install.sh of the latest release
//   /install.sh          the same
//   /install.ps1         install.ps1 of the latest release
//   /v1.2.3              install.sh of release 1.2.3, which never changes
//   /v1.2.3/install.sh   the same
//   /v1.2.3/install.ps1  install.ps1 of release 1.2.3
//
// A URL answers every client with the same bytes: nothing depends on the
// user agent, so the script a browser shows is the one a shell runs, and a
// SHA-256 taken of a versioned URL holds for every client that fetches it.
// The user agent is only counted.
//
// The latest release is the one packslip.dev's release list recommends
// with `latest`, or else its highest release that is neither withdrawn nor
// a prerelease, the order consumers follow (Latest in the specification).
// The list is read from the bucket the release job writes it to, without
// verifying it: this Worker trusts that bucket as it trusts the scripts in
// it.

const LATEST = "public, max-age=300";
const IMMUTABLE = "public, max-age=31536000, immutable";

const USAGE = `packslip.sh serves packslip's install scripts.

Linux and macOS:     curl -fsSL https://packslip.sh | sh
Windows PowerShell:  irm https://packslip.sh/install.ps1 | iex

A release's own copies, which never change:
  https://packslip.sh/vX.Y.Z
  https://packslip.sh/vX.Y.Z/install.ps1

https://packslip.dev/docs/getting-started/
`;

export default {
  async fetch(request, env) {
    if (request.method !== "GET" && request.method !== "HEAD") {
      return new Response(null, { status: 405, headers: { allow: "GET, HEAD" } });
    }
    const url = new URL(request.url);
    // Never a script over plain HTTP, even if the zone's Always Use HTTPS
    // is ever turned off: send the client to the same path over HTTPS.
    if (url.protocol !== "https:") {
      url.protocol = "https:";
      return Response.redirect(url.toString(), 301);
    }
    const route = parse(url.pathname);
    if (!route) {
      return text(404, USAGE);
    }
    const tag = route.tag ?? (await latestTag(env));
    if (!tag) {
      return text(503, "packslip.sh: packslip.dev's release list names no release to install\n");
    }
    const object = await env.RELEASES.get(`${env.TOOL}/${tag}/${route.file}`);
    if (!object) {
      return text(
        404,
        `packslip.sh: packslip ${tag} published no ${route.file}; see https://packslip.dev/docs/getting-started/#install-packslip\n`,
      );
    }

    env.DOWNLOADS.writeDataPoint({
      indexes: [env.TOOL],
      blobs: [
        env.TOOL,
        tag,
        route.file,
        "installer",
        client(request.headers.get("user-agent") || ""),
        request.cf?.country || "",
      ],
      doubles: [1],
    });
    const headers = new Headers({
      "content-type": "text/plain; charset=utf-8",
      "x-content-type-options": "nosniff",
      "cache-control": route.tag ? IMMUTABLE : LATEST,
      etag: object.httpEtag,
      // Where this release's copy lives for good, for anyone pinning it.
      "content-location": route.file === "install.sh" ? `/${tag}` : `/${tag}/${route.file}`,
    });
    return new Response(request.method === "HEAD" ? null : object.body, { headers });
  },
};

function parse(path) {
  if (path === "/" || path === "/install.sh") return { file: "install.sh" };
  if (path === "/install.ps1") return { file: "install.ps1" };
  const release = path.match(/^\/(v\d+\.\d+\.\d+)(?:\/(install\.sh|install\.ps1))?$/);
  return release ? { tag: release[1], file: release[2] ?? "install.sh" } : null;
}

async function latestTag(env) {
  const object = await env.RELEASES.get(`${env.TOOL}/.well-known/packslip.json`);
  if (!object) return null;
  const bundle = await object.json();
  const payload = Uint8Array.from(atob(bundle.dsseEnvelope.payload), (c) => c.charCodeAt(0));
  const statement = JSON.parse(new TextDecoder().decode(payload));
  const release = latest(statement.predicate);
  return release && (release.tag ?? `v${release.version}`);
}

// The release the list recommends, if it may be installed, and otherwise
// the highest that may: not withdrawn, and not a prerelease.
export function latest(predicate) {
  const eligible = (predicate.releases ?? []).filter(
    (r) => r.status !== "yanked" && precedence(r.version) !== null,
  );
  const recommended = eligible.find((r) => r.version === predicate.latest);
  if (recommended) return recommended;
  let best = null;
  for (const release of eligible) {
    if (best === null || compare(precedence(release.version), precedence(best.version)) > 0) {
      best = release;
    }
  }
  return best;
}

// Major, minor, and patch of a release version, ignoring build metadata;
// null for a prerelease or anything that is not semver.
function precedence(version) {
  const parts = /^(\d+)\.(\d+)\.(\d+)(?:\+[0-9A-Za-z.-]+)?$/.exec(version ?? "");
  return parts ? parts.slice(1, 4).map(BigInt) : null;
}

function compare(a, b) {
  for (let i = 0; i < 3; i++) {
    if (a[i] !== b[i]) return a[i] > b[i] ? 1 : -1;
  }
  return 0;
}

function text(status, body) {
  return new Response(body, {
    status,
    headers: { "content-type": "text/plain; charset=utf-8", "cache-control": "no-store" },
  });
}

function client(userAgent) {
  if (/^(curl|wget)\b/i.test(userAgent)) return "shell";
  if (/PowerShell\//.test(userAgent)) return "powershell";
  if (/mozilla/i.test(userAgent)) return "browser";
  return "other";
}
