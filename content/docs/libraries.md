---
title: Choose a library
weight: 66
group: build
description: Compare packslip's Rust crate, Go module, npm package, and committable verifier, and find the guide for the one that fits your code.
---
# Choose a library

Developers building packslip into their own code have four libraries to
choose from. By the end, you know which one fits your job and where its guide
is. To verify or install from a shell, use the CLI instead: see
[Verify a release](/docs/verifying/) or [Bootstrap a tool](/docs/bootstrap/).

## Compare the libraries

The libraries serve different jobs and deliberately do not expose the same
API.

| Library | Guide | Use it to | Main entry points | Needs |
| --- | --- | --- | --- | --- |
| Rust crate [`packslip`](https://crates.io/crates/packslip) | [Use the Rust crate](/docs/rust/) | Verify releases and release lists, keyless or key-signed, and hold them to stored pins and signer fingerprints. Select an artifact and its resources. Optional features create and sign statements. | `verify_forge`, `verify`, `verify_release_list`, `select_artifact`, `select_resources` | Rust 1.93 or newer |
| Go module `github.com/jdx/packslip/go` | [Publish from Go](/docs/go/) | Create, validate, and sign `release/v1` statements keylessly in a publisher written in Go. It does not verify bundles. | `Build`, `Statement.Validate`, `AmbientIDToken`, `IdentityFromToken`, `ProductionKeylessOptions`, `SignKeyless` | Go 1.25.8 or newer |
| npm package [`packslip`](https://www.npmjs.com/package/packslip) | [Verify from JavaScript](/docs/verify-js/) | Verify a keyless bundle and the files you downloaded from your own Node.js code, such as a JavaScript GitHub Action. It does not create or sign statements. | `verifyRelease`, `findArtifact`, `verifyFileDigest` | Node.js 22.22.2 or later in 22.x, 24.15.0 or later in 24.x, or 26 or newer |
| Committable verifier `packslip-verify.mjs` | [Commit a verifier into your project](/docs/verify-script/) | Verify a keyless bundle and the files you downloaded with one file you review and commit. Run it as a command, or [import it as a module](/docs/verify-script/#use-it-as-a-module). | `node packslip-verify.mjs`, `verifyRelease`, `verifyFile` | Node.js 20 or newer, no dependencies |

The two Node.js options get Sigstore's trust root differently. On each call,
the npm package checks Sigstore's TUF repository for the current one through
the `sigstore` package. The committable verifier makes no network requests: it checks
against a snapshot embedded in the file, which you update when Sigstore
rotates a key.

Neither Node.js option handles key-signed releases, release lists, or signer
fingerprints; the Rust crate and `packslip verify` do.
[What each consumer enforces](/docs/trust/#what-each-consumer-enforces)
compares them.

## Match versions across packages

The CLI, the GitHub Actions, the Rust crate, the npm package, and the Go module
share one release version, so `vX.Y.Z` names the same release in each:

- The npm package's version matches the release tag: release `vX.Y.Z` is
  `packslip@X.Y.Z` on npm. It is published with npm provenance, so npm shows
  the repository and workflow run that built each version.
- `go get github.com/jdx/packslip/go@vX.Y.Z` fetches the Go module of release
  `vX.Y.Z`. [Publish from Go](/docs/go/) explains its tags.
- The committable verifier has no package: copy it at a release tag or
  commit you reviewed. See
  [Copy the verifier](/docs/verify-script/#copy-the-verifier).
