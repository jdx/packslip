// packslip.dev: the documentation is served as static assets, and this
// script runs first only for the paths run_worker_first lists in
// wrangler.jsonc: release files, the signed list, and package repositories. Two shapes of those
// are release data in R2, laid out as <tool>/<tag>/<file> and
// <tool>/.well-known/packslip.json; everything else, including a release
// path R2 has no object for, is handed to the assets, which answer with
// the site's page or its 404 page.
//
// Each release request is counted in Analytics Engine: the tool, tag,
// file, and what kind of document it was, so installs (an artifact) can be
// told from index refreshes (the list) and manifest checks (the bundle).

const IMMUTABLE = "public, max-age=31536000, immutable";
const LIST = "public, max-age=300";

function repository(path) {
  if (path === "/gpg-key.pub" || path === "/rpm/packslip.repo") return { immutable: false };
  if (/^\/apt\/pool\/main\/packslip-[a-f0-9]{64}_(amd64|arm64)\.deb$/.test(path)) return { immutable: true };
  if (/^\/apt\/dists\/stable\/main\/binary-(amd64|arm64)\/by-hash\/SHA256\/[a-f0-9]{64}$/.test(path)) return { immutable: true };
  if (/^\/apt\/dists\/stable\/(InRelease|Release|Release\.gpg)$/.test(path)) return { immutable: false };
  if (/^\/apt\/dists\/stable\/main\/binary-(amd64|arm64)\/Packages(\.gz)?$/.test(path)) return { immutable: false };
  if (/^\/rpm\/(x86_64|aarch64)\/packages\/packslip-[a-f0-9]{64}\.rpm$/.test(path)) return { immutable: true };
  if (/^\/rpm\/(x86_64|aarch64)\/repodata\/repomd\.xml(\.asc)?$/.test(path)) return { immutable: false };
  if (/^\/rpm\/(x86_64|aarch64)\/repodata\/[a-f0-9]{64}-[a-zA-Z0-9._-]+$/.test(path)) return { immutable: true };
  return null;
}

export default {
  async fetch(request, env, ctx) {
    const path = new URL(request.url).pathname;
    const isList = path === "/.well-known/packslip.json";
    const release = isList ? null : path.match(/^\/(v[^/]+)\/([^/]+)$/);
    const repo = repository(path);
    if (!isList && !release && !repo) {
      return env.ASSETS.fetch(request);
    }
    if (request.method !== "GET" && request.method !== "HEAD") {
      return new Response(null, { status: 405, headers: { allow: "GET, HEAD" } });
    }

    // A tag's files never change, so the edge keeps them; the list does,
    // and is small, so every request for it reads R2.
    const cache = caches.default;
    const immutable = release !== null || repo?.immutable === true;
    // Let R2 evaluate conditional/range requests. HEAD responses cannot be
    // placed in Cache API, which accepts only full GET responses.
    const cacheable = immutable && request.method === "GET" &&
      !["range", "if-match", "if-none-match", "if-modified-since", "if-unmodified-since"]
        .some((header) => request.headers.has(header));
    let response = cacheable ? await cache.match(request) : undefined;
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
      headers.set("cache-control", immutable ? IMMUTABLE : LIST);
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
      const unmetPrecondition = request.headers.has("if-match") || request.headers.has("if-unmodified-since");
      const status = object.body === undefined ? (unmetPrecondition ? 412 : 304) : partial ? 206 : 200;
      response = new Response(
        request.method === "HEAD" || status === 304 || status === 412 ? null : object.body,
        { status, headers },
      );
      if (cacheable && status === 200) {
        ctx.waitUntil(cache.put(request, response.clone()));
      }
    }

    const file = isList ? "" : repo ? path.split("/").at(-1) : release[2];
    env.DOWNLOADS.writeDataPoint({
      indexes: [env.TOOL],
      blobs: [
        env.TOOL,
        isList ? "" : repo ? "repository" : release[1],
        file,
        repo ? "repository" : kind(isList, file),
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
