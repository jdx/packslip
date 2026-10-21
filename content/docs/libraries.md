---
title: Use the Go and npm libraries
weight: 64
group: consume
description: Build release publishers in Go or verify already-downloaded releases from Node.js.
---
# Use the Go and npm libraries

packslip has libraries for two different jobs. They deliberately do not expose
the same API:

| Library | Job | Main entry points |
| --- | --- | --- |
| Go module | Publishers creating and signing `release/v1` statements | `Build`, `Statement.Validate`, `AmbientIDToken`, `IdentityFromToken`, `ProductionKeylessOptions`, `SignKeyless` |
| [`packslip` npm package](https://www.npmjs.com/package/packslip) | Consumers verifying a bundle and downloaded files | `verifyRelease`, `findArtifact`, `verifyFileDigest` |

The Go module is installable from v1.9.1. The npm package is for verification
only; it does not generate release statements or sign bundles.

## Publish from Go

The Go module path is `github.com/jdx/packslip/go` and requires Go 1.25.8 or
newer. Install the release matching the repository version:

```sh
go get github.com/jdx/packslip/go@v1.9.1
```

Import it as `packslip`:

```go
import packslip "github.com/jdx/packslip/go"
```

In CI, `AmbientIDToken` returns `SIGSTORE_ID_TOKEN` or, in a GitHub Actions
job with `id-token: write`, a token for the `sigstore` audience. Other CI
systems export their token as `SIGSTORE_ID_TOKEN`. `IdentityFromToken` derives
the `Identity` that Fulcio will certify, which is the workflow URI for GitHub
Actions and GitLab CI, otherwise the verified email or subject. Use it for the
statement's `Predicate.Identity`; `SignKeyless` refuses a certificate that does
not match:

```go
oidcToken, err := packslip.AmbientIDToken(ctx)
if err != nil {
    return err
}
identity, err := packslip.IdentityFromToken(oidcToken)
if err != nil {
    return err
}
```

Build a statement from explicit metadata and the final, already-uploaded
artifact bytes. `Build` streams each file once to record its size, SHA-256, and
SHA-512 digest:

```go
statement, err := packslip.Build(packslip.Predicate{
    Project: "github.com/acme/tool",
    Version: "0.1.0",
    PublishedAt: time.Now().UTC(),
    Source: &packslip.Source{Repo: "https://github.com/acme/tool", Tag: "v0.1.0"},
    Identity: identity,
}, []packslip.ArtifactInput{{
    Path: "dist/tool-linux-x64.tar.xz",
    Artifact: packslip.Artifact{
        Name: "tool-linux-x64.tar.xz", OS: "linux", Arch: "x86_64",
        Format: "tar.xz", Bin: []packslip.Bin{packslip.NewBin("tool")},
    },
}})
if err != nil {
    return err
}
```

Pass the same token explicitly to production Sigstore configuration and
sign:

```go
options, err := packslip.ProductionKeylessOptions(ctx, oidcToken)
if err != nil {
    return err
}
bundle, err := packslip.SignKeyless(statement, options)
if err != nil {
    return err
}
```

`Build` and `Statement.Validate` establish only structural validity.
`SignKeyless` produces a Sigstore bundle and verifies it against the injected
trusted root; a parsed statement is not cryptographically verified merely
because it is valid JSON. The module intentionally does not map GoReleaser
configuration, upload artifacts, discover archives, or provide a release CLI.
It does translate common build vocabulary: `ArchFromGo` turns a `GOARCH` and
`GOARM` into packslip's `arch`, and `NewBareBin` names the executable of a
bare artifact (`raw`, `gz`, `xz`, `zst`, `bz2`).
For its complete publisher API, see the
[module README](https://github.com/jdx/packslip/tree/main/go).

## Verify from Node.js

Install the existing consumer package in a Node.js project:

```sh
npm install packslip
```

It supports Node.js 22.22.2 or later in 22, 24.15.0 or later in 24, and Node
26 or newer. Download the bundle and release asset yourself, then set an
explicit signer policy before accepting them:

```ts
import { findArtifact, verifyFileDigest, verifyRelease } from 'packslip';

const statement = await verifyRelease(bundleJson, {
  identity: 'https://github.com/jdx/mise/.github/workflows/release.yml@refs/tags/v2026.10.11',
  issuer: 'https://token.actions.githubusercontent.com',
  project: 'github.com/jdx/mise',
  tag: 'v2026.10.11',
});

const artifact = findArtifact(statement, 'mise-v2026.10.11-linux-arm64');
await verifyFileDigest('/tmp/mise-v2026.10.11-linux-arm64', artifact);
```

`verifyRelease` verifies the keyless Sigstore bundle and enforces the identity,
issuer, and optional project/tag policy. `verifyFileDigest` streams the asset
and checks its signed size and digests. The package does not download a
release, select a platform, create a statement, or sign; keep those concerns
in your application. See [Verify a release from JavaScript](/docs/verify-js/)
for its full API, errors, and trust-root configuration.

## Versions and releases

The CLI, GitHub Actions, npm package, and Go module share one release version.
For a root release tag `vX.Y.Z`, the release workflow creates a matching
`go/vX.Y.Z` tag at the same commit; Go requires this subdirectory prefix for
the module path. `release-plz` publishes the root Cargo crate only. The Go
module is distributed through the normal Go module proxy, so it needs no
registry-upload or separate release-dispatch step.
