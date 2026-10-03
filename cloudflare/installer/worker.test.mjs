// Tests for the packslip.sh Worker against an in-memory bucket.
// Run with: node --test cloudflare/installer/worker.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";

import worker, { latest } from "./worker.mjs";

function list(predicate) {
  const statement = {
    _type: "https://in-toto.io/Statement/v1",
    subject: [],
    predicateType: "https://packslip.dev/releases/v1",
    predicate: { project: "packslip.dev", ...predicate },
  };
  return JSON.stringify({
    dsseEnvelope: {
      payloadType: "application/vnd.in-toto+json",
      payload: Buffer.from(JSON.stringify(statement)).toString("base64"),
      signatures: [],
    },
  });
}

const release = (version, extra = {}) => ({ version, tag: `v${version}`, ...extra });

// An environment whose bucket holds `files`, recording analytics points.
function env(files) {
  const points = [];
  return {
    TOOL: "packslip",
    points,
    DOWNLOADS: { writeDataPoint: (point) => points.push(point) },
    RELEASES: {
      async get(key) {
        if (!(key in files)) return null;
        const body = files[key];
        return {
          httpEtag: `"${key}"`,
          body: new Response(body).body,
          json: async () => JSON.parse(body),
        };
      },
    },
  };
}

const scripts = {
  "packslip/v1.4.0/install.sh": "#!/bin/sh\n# 1.4.0\n",
  "packslip/v1.4.0/install.ps1": "# 1.4.0 ps1\n",
  "packslip/v1.5.0/install.sh": "#!/bin/sh\n# 1.5.0\n",
  "packslip/v1.5.0/install.ps1": "# 1.5.0 ps1\n",
};

function bucket(predicate, files = scripts) {
  return env({ ...files, "packslip/.well-known/packslip.json": list(predicate) });
}

const releases = [
  release("1.3.0"),
  release("1.4.0"),
  release("1.5.0"),
  release("1.6.0", { status: "yanked", status_reason: "broken" }),
  release("2.0.0-rc.1"),
];

async function get(path, environment, init = {}) {
  return worker.fetch(new Request(`https://packslip.sh${path}`, init), environment);
}

test("/ serves the highest release that is neither yanked nor a prerelease", async () => {
  const response = await get("/", bucket({ releases }));
  assert.equal(response.status, 200);
  assert.equal(await response.text(), scripts["packslip/v1.5.0/install.sh"]);
  assert.equal(response.headers.get("content-type"), "text/plain; charset=utf-8");
  assert.equal(response.headers.get("cache-control"), "public, max-age=300");
  assert.equal(response.headers.get("content-location"), "/v1.5.0");
});

test("/install.sh is the same as /", async () => {
  const response = await get("/install.sh", bucket({ releases }));
  assert.equal(await response.text(), scripts["packslip/v1.5.0/install.sh"]);
});

test("/install.ps1 serves the latest release's PowerShell script", async () => {
  const response = await get("/install.ps1", bucket({ releases }));
  assert.equal(await response.text(), scripts["packslip/v1.5.0/install.ps1"]);
  assert.equal(response.headers.get("content-location"), "/v1.5.0/install.ps1");
});

test("the list's latest pointer wins over higher versions", async () => {
  const response = await get("/", bucket({ releases, latest: "1.4.0" }));
  assert.equal(await response.text(), scripts["packslip/v1.4.0/install.sh"]);
});

test("a latest pointer to a yanked release falls back to semver", () => {
  assert.equal(latest({ releases, latest: "1.6.0" }).version, "1.5.0");
});

test("versions compare numerically, not as text", () => {
  assert.equal(latest({ releases: [release("1.9.0"), release("1.10.0")] }).version, "1.10.0");
});

test("an entry without a tag is v plus its version", async () => {
  const environment = bucket({ releases: [{ version: "1.5.0" }] });
  assert.equal(await (await get("/", environment)).text(), scripts["packslip/v1.5.0/install.sh"]);
});

test("a versioned URL serves that release's script for good", async () => {
  for (const [path, key] of [
    ["/v1.4.0", "packslip/v1.4.0/install.sh"],
    ["/v1.4.0/install.sh", "packslip/v1.4.0/install.sh"],
    ["/v1.4.0/install.ps1", "packslip/v1.4.0/install.ps1"],
  ]) {
    const response = await get(path, bucket({ releases }));
    assert.equal(response.status, 200, path);
    assert.equal(await response.text(), scripts[key], path);
    assert.equal(response.headers.get("cache-control"), "public, max-age=31536000, immutable", path);
  }
});

test("every client gets the same bytes", async () => {
  const agents = [
    "curl/8.11.1",
    "Wget/1.24.5",
    "Mozilla/5.0 (Windows NT; Windows NT 10.0; en-US) WindowsPowerShell/5.1.26100.4202",
    "Mozilla/5.0 (Windows NT 10.0; Microsoft Windows 10.0.26100; en-US) PowerShell/7.5.2",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 Safari/605.1.15",
  ];
  for (const path of ["/", "/install.ps1", "/v1.4.0"]) {
    const bodies = new Set();
    for (const agent of agents) {
      const response = await get(path, bucket({ releases }), { headers: { "user-agent": agent } });
      bodies.add(await response.text());
    }
    assert.equal(bodies.size, 1, path);
  }
});

test("a release without the script is a 404 that says so", async () => {
  const response = await get("/v1.3.0", bucket({ releases }));
  assert.equal(response.status, 404);
  assert.match(await response.text(), /packslip v1\.3\.0 published no install\.sh/);
});

test("before any release has a script, / is a 404 too", async () => {
  const response = await get("/", bucket({ releases: [release("1.3.0")] }));
  assert.equal(response.status, 404);
});

test("unknown paths get the usage text", async () => {
  for (const path of ["/docs/", "/v1.4", "/v1.4.0/packslip-v1.4.0-linux-x64", "/install.bat"]) {
    const response = await get(path, bucket({ releases }));
    assert.equal(response.status, 404, path);
    assert.match(await response.text(), /curl -fsSL https:\/\/packslip\.sh \| sh/, path);
  }
});

test("no release list is a 503", async () => {
  const response = await get("/", env(scripts));
  assert.equal(response.status, 503);
});

test("HEAD has the headers and no body", async () => {
  const response = await get("/", bucket({ releases }), { method: "HEAD" });
  assert.equal(response.status, 200);
  assert.equal(response.headers.get("content-location"), "/v1.5.0");
  assert.equal(await response.text(), "");
});

test("other methods are refused", async () => {
  const response = await get("/", bucket({ releases }), { method: "POST", body: "x" });
  assert.equal(response.status, 405);
  assert.equal(response.headers.get("allow"), "GET, HEAD");
});

test("a served script is counted, with the client kind", async () => {
  const environment = bucket({ releases });
  await get("/install.ps1", environment, {
    headers: { "user-agent": "Mozilla/5.0 (Windows NT 10.0) PowerShell/7.5.2" },
  });
  await get("/nope", environment);
  assert.equal(environment.points.length, 1);
  assert.deepEqual(environment.points[0].blobs.slice(0, 5), [
    "packslip",
    "v1.5.0",
    "install.ps1",
    "installer",
    "powershell",
  ]);
});
