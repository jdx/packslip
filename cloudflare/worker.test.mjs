import assert from "node:assert/strict";
import { test } from "node:test";
import worker from "./worker.js";

const hash = "a".repeat(64);
function fixture() {
  let reads = 0, writes = 0;
  const stored = new Map();
  globalThis.caches = { default: {
    async match(request) { return stored.get(request.url)?.clone(); },
    async put(request, response) {
      assert.equal(request.method, "GET");
      writes++;
      stored.set(request.url, response);
    },
  } };
  const env = {
    TOOL: "packslip",
    DOWNLOADS: { writeDataPoint() {} },
    ASSETS: { async fetch() { return new Response("asset", { status: 404 }); } },
    RELEASES: { async get(key, options) {
      reads++;
      assert.ok(key.startsWith("packslip/"));
      const conditional = options.onlyIf.has("if-none-match") || options.onlyIf.has("if-match");
      const ranged = options.range !== undefined;
      return {
        body: conditional ? undefined : `body${reads}`,
        httpEtag: '"test"', size: 20,
        ...(ranged ? { range: { offset: 2, length: 3 } } : {}),
        writeHttpMetadata(headers) { headers.set("content-type", "application/octet-stream"); },
      };
    } },
  };
  const waits = [];
  const ctx = { waitUntil(promise) { waits.push(promise); } };
  return { async get(path, options) {
    const response = await worker.fetch(new Request(`https://packslip.dev${path}`, options), env, ctx);
    await Promise.all(waits);
    return response;
  }, reads: () => reads, writes: () => writes };
}

test("mutable signed repository indices and keys always read current R2 data", async () => {
  for (const path of ["/gpg-key.pub", "/rpm/packslip.repo", "/apt/dists/stable/InRelease",
    "/apt/dists/stable/Release.gpg", "/apt/dists/stable/main/binary-arm64/Packages.gz",
    "/rpm/aarch64/repodata/repomd.xml.asc"]) {
    const f = fixture();
    assert.equal((await f.get(path)).headers.get("cache-control"), "public, max-age=300");
    assert.equal(await (await f.get(path)).text(), "body2");
    assert.equal(f.reads(), 2);
    assert.equal(f.writes(), 0);
  }
});

test("content-addressed packages, APT by-hash, and RPM metadata are immutable", async () => {
  for (const path of [`/apt/pool/main/packslip-${hash}_amd64.deb`,
    `/apt/dists/stable/main/binary-amd64/by-hash/SHA256/${hash}`,
    `/rpm/x86_64/packages/packslip-${hash}.rpm`,
    `/rpm/x86_64/repodata/${hash}-primary.xml.gz`, "/v1.4.0/packslip.tar.xz"]) {
    const f = fixture();
    assert.match((await f.get(path)).headers.get("cache-control"), /immutable/);
    assert.equal(await (await f.get(path)).text(), "body1");
    assert.equal(f.reads(), 1);
  }
});

test("HEAD, conditional, and range requests preserve download semantics without caching", async () => {
  const f = fixture(), path = `/apt/pool/main/packslip-${hash}_amd64.deb`;
  const head = await f.get(path, { method: "HEAD" });
  assert.equal(head.status, 200);
  assert.equal(await head.text(), "");
  assert.equal(f.writes(), 0);
  const unchanged = await f.get(path, { headers: { "if-none-match": '"test"' } });
  assert.equal(unchanged.status, 304);
  const mismatch = await f.get(path, { headers: { "if-match": '"wrong"' } });
  assert.equal(mismatch.status, 412);
  const range = await f.get(path, { headers: { range: "bytes=2-4" } });
  assert.equal(range.status, 206);
  assert.equal(range.headers.get("content-range"), "bytes 2-4/20");
  assert.equal(f.writes(), 0);
});

test("invalid repository paths never reach R2 and writes are rejected", async () => {
  const f = fixture();
  for (const path of ["/apt/secret", "/rpm/other/repodata/repomd.xml",
    "/apt/pool/main/not-addressed.deb", "/apt/%2Fsecret", "/rpm/../../secret"]) {
    assert.equal((await f.get(path)).status, 404);
  }
  assert.equal(f.reads(), 0);
  assert.equal((await f.get("/apt/dists/stable/InRelease", { method: "POST" })).status, 405);
  assert.equal(f.reads(), 0);
});
