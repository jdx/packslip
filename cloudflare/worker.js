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
      // Only hand R2 the headers as a range when one was actually asked
      // for: given a plain GET it still reports a range covering the whole
      // object, which turned every download into a 206.
      const wantsRange = request.headers.has("range");
      const object = await env.RELEASES.get(`${env.TOOL}${path}`, {
        ...(wantsRange ? { range: request.headers } : {}),
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
      const partial = wantsRange && object.range !== undefined;
      if (partial) {
        // A 206 without content-range is malformed, and R2 gives the range
        // back either as an offset and length or as a suffix length.
        const size = object.size;
        const suffix = "suffix" in object.range;
        const offset = suffix ? size - object.range.suffix : object.range.offset ?? 0;
        const length = suffix ? object.range.suffix : object.range.length ?? size - offset;
        headers.set("content-range", `bytes ${offset}-${offset + length - 1}/${size}`);
      }
      const status = object.body === undefined ? 304 : partial ? 206 : 200;
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
