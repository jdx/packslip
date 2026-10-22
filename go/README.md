# Go module for packslip publishers

`github.com/jdx/packslip/go` lets a Go release tool publish a packslip without
the CLI: it builds the release statement from your artifacts and signs it
keylessly with Sigstore. To verify releases instead, see
[Choose a library](https://packslip.dev/docs/libraries/).

The module hashes each artifact, checks that the statement is well formed
under the [release/v1 specification](https://packslip.dev/release/v1/), and
returns the signed release bundle. It does not read GoReleaser
configuration, find or upload artifacts, or provide a release CLI. Mapping a
release tool's configuration to packslip belongs in an adapter owned by that
tool or its users.

## Install

```sh
go get github.com/jdx/packslip/go@latest
```

The module requires Go 1.25.8 or newer. Import it as `packslip`:

```go
import packslip "github.com/jdx/packslip/go"
```

## Sign a release

This program signs one archive of a GitHub project and writes the bundle to
`dist/packslip.sigstore.json`. Run it in a GitHub Actions job in the
project's repository with `id-token: write` permission. By default, consumers
accept a `github.com/` project's bundle only when a GitHub Actions job in that
repository signed it.
[How trust works](https://packslip.dev/docs/trust/#what-a-project-name-says-about-the-signer)
explains why.

To sign a domain project, such as `mytool.example.com`, or a `gitlab.com`
project from GitLab CI, change the program as
[Sign a domain or GitLab project](https://packslip.dev/docs/go/#sign-a-domain-or-gitlab-project)
describes. That section also says which CI systems cannot sign keylessly.

```go
package main

import (
	"context"
	"log"
	"os"
	"time"

	packslip "github.com/jdx/packslip/go"
)

func main() {
	ctx := context.Background()

	// SIGSTORE_ID_TOKEN, or a GitHub Actions token when the job has
	// `id-token: write`.
	token, err := packslip.AmbientIDToken(ctx)
	if err != nil {
		log.Fatal(err)
	}
	// The identity Fulcio will put in the signing certificate.
	identity, err := packslip.IdentityFromToken(token)
	if err != nil {
		log.Fatal(err)
	}

	statement, err := packslip.Build(packslip.Predicate{
		Project:     "github.com/owner/mytool",
		Version:     "1.2.3",
		PublishedAt: time.Now(),
		Source:      &packslip.Source{Repo: "https://github.com/owner/mytool", Tag: "v1.2.3"},
		Identity:    identity,
	}, []packslip.ArtifactInput{{
		Path: "dist/mytool-linux-x64.tar.gz",
		Artifact: packslip.Artifact{
			Name:   "mytool-linux-x64.tar.gz",
			OS:     "linux",
			Arch:   "x86_64",
			Format: "tar.gz",
			Bin:    []packslip.Bin{packslip.NewBin("mytool")},
		},
	}})
	if err != nil {
		log.Fatal(err)
	}

	options, err := packslip.ProductionKeylessOptions(ctx, token)
	if err != nil {
		log.Fatal(err)
	}
	bundle, err := packslip.SignKeyless(statement, options)
	if err != nil {
		log.Fatal(err)
	}
	if err := os.WriteFile("dist/packslip.sigstore.json", bundle, 0o644); err != nil {
		log.Fatal(err)
	}
}
```

- `AmbientIDToken` returns `SIGSTORE_ID_TOKEN` when it is set. Otherwise it
  requests a GitHub Actions token for the `sigstore` audience. It is the only
  function that reads an OIDC token from the environment or requests one.
- `IdentityFromToken` derives the identity Fulcio will certify for the token.
  It does not verify the token. `SignKeyless` refuses a certificate whose
  identity differs from the statement's `Identity`, before anything reaches
  the transparency log.
- `Build` streams each `Path` once to record its size, SHA-256, and SHA-512.
  `Name` is the artifact's file name in the release, the name you upload it
  under. The file must hold the exact bytes you upload.
- `ProductionKeylessOptions` selects the public-good Fulcio and Rekor services
  and loads the trusted root from Sigstore's TUF metadata. It downloads that
  metadata and caches it under `~/.sigstore/root`. For a private Sigstore
  deployment, fill in `KeylessOptions` yourself.
- `SignKeyless` returns the bundle after sigstore-go verifies it against the
  trusted root.

Upload the bundle to the release beside the artifacts, named
`packslip.sigstore.json`.

Passing `Build` or `Statement.Validate` establishes only that a statement is
well formed. It does not establish who signed a bundle or that its files
match. Verify bundles with `packslip verify` or a library from
[Choose a library](https://packslip.dev/docs/libraries/).

## Versions

The module shares packslip's release version with the CLI, the GitHub
Actions, the Rust crate, and the npm package. Each release `vX.Y.Z` also tags
the module as `go/vX.Y.Z` at the same commit. To require a specific release:

```sh
go get github.com/jdx/packslip/go@vX.Y.Z
```

The version tracks the implementation, not the statement format, which stays
`release/v1`.

## Documentation

- [Publish from Go](https://packslip.dev/docs/go/): the guide for this
  module, including `ArchFromGo` for mapping `GOARCH` and `GOARM`,
  `VariantFromGo` and `VariantFromRustABI` for CPU-level and float-ABI
  variants, `NewBareBin` for single-file artifacts, and `BuildOptions` for resource
  assets.
- [Go package reference](https://pkg.go.dev/github.com/jdx/packslip/go) on
  pkg.go.dev: every exported function and type.
- [release/v1 specification](https://packslip.dev/release/v1/): the rules a
  statement follows.
