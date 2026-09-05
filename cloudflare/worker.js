// packslip.dev: the documentation is served as static assets, which
// answer before this script runs, so the script only sees what the site
// has no file for. Two shapes of those are release data in R2, laid out
// as <tool>/<tag>/<file> and <tool>/.well-known/packslip.json; everything
// else falls through to the site's 404 page.
//
// Each release request is counted in Analytics Engine: the tool, tag,
// file, and what kind of document it was, so installs (an artifact) can be
// told from index refreshes (the list) and manifest checks (the bundle).

const IMMUTABLE = "public, max-age=31536000, immutable";
const LIST = "public, max-age=300";

export default {
  async fetch(request, env, ctx) {
    const path = new URL(request.url).pathname;
    const isList = path === "/.well-known/packslip.json";
    const release = isList ? null : path.match(/^\/(v[^/]+)\/([^/]+)$/);
    if (!isList && !release) {
      return env.ASSETS.fetch(request);
    }
    if (request.method !== "GET" && request.method !== "HEAD") {
      return new Response(null, { status: 405, headers: { allow: "GET, HEAD" } });
    }

    // A tag's files never change, so the edge keeps them; the list does,
    // and is small, so every request for it reads R2.
    const cache = caches.default;
    let response = isList ? undefined : await cache.match(request);
    if (!response) {
      const object = await env.RELEASES.get(`${env.TOOL}${path}`, {
        range: request.headers,
        onlyIf: request.headers,
      });
      if (!object) {
        return env.ASSETS.fetch(request);
      }
      const headers = new Headers();
      object.writeHttpMetadata(headers);
      headers.set("etag", object.httpEtag);
      headers.set("accept-ranges", "bytes");
      headers.set("cache-control", isList ? LIST : IMMUTABLE);
      if (isList) {
        headers.set("content-type", "application/json");
      }
      const status = object.body === undefined ? 304 : object.range ? 206 : 200;
      response = new Response(
        request.method === "HEAD" || status === 304 ? null : object.body,
        { status, headers },
      );
      if (!isList && status === 200) {
        ctx.waitUntil(cache.put(request, response.clone()));
      }
    }

    const file = isList ? "" : release[2];
    env.DOWNLOADS.writeDataPoint({
      indexes: [env.TOOL],
      blobs: [
        env.TOOL,
        isList ? "" : release[1],
        file,
        kind(isList, file),
        client(request.headers.get("user-agent") || ""),
        request.cf?.country || "",
      ],
      doubles: [1],
    });
    return response;
  },
};

function kind(isList, file) {
  if (isList) return "list";
  if (file === "packslip.sigstore.json" || file.match(/^packslip\..*\.sigstore\.json$/)) return "bundle";
  if (file.endsWith(".usage.kdl")) return "resource";
  return "artifact";
}

function client(userAgent) {
  if (/^mise\b/.test(userAgent)) return "mise";
  if (/^(curl|wget)\b/i.test(userAgent)) return "shell";
  if (/mozilla/i.test(userAgent)) return "browser";
  return "other";
}
