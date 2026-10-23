---
title: Publish from Go
description: Build and sign a release bundle from your own Go release tool with the github.com/jdx/packslip/go module.
group: publish
weight: 48
---
# Publish from Go

Use the Go module when your Go release tool should sign releases itself.
At the end, your program writes a signed `packslip.sigstore.json` for the
files it built.

To sign from a shell step instead, see
[Publish with GitHub Actions](/docs/publishing/) or
[Publish from other CI](/docs/other-ci/). To verify releases from code, see
[Choose a library](/docs/libraries/).

## Know the limits

The module builds a statement from artifact metadata you pass, validates
it, and signs it keylessly with Sigstore only. To sign with a key, run
`packslip create --key` ([Sign with a key](/docs/other-ci/#sign-with-a-key)).
Unlike `packslip create`, `Build` does not open archives, check that a `Bin`
path exists, or derive host requirements.

The module is not a GoReleaser adapter or a release CLI. Mapping
GoReleaser configuration, discovering archives, and uploading files belong
in an adapter owned by the release tool or its users.

## Add the module

```sh
go get github.com/jdx/packslip/go@latest
```

The module requires Go 1.25.8 or newer. Import it as `packslip`:

```go
import packslip "github.com/jdx/packslip/go"
```

Module version `vX.Y.Z` is tagged `go/vX.Y.Z` (Go requires the
subdirectory prefix) on the commit of CLI release X.Y.Z, and the normal Go
module proxy serves it. To match the CLI version you use elsewhere, name it:

```sh
go get github.com/jdx/packslip/go@vX.Y.Z
```

## Sign a release

This program signs one archive of the GitHub project
`github.com/owner/mytool`. Run it in a GitHub Actions job of
`owner/mytool` with `id-token: write`; consumers accept the bundle only
when a workflow run in that repository signed it.

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
	if err := run(context.Background()); err != nil {
		log.Fatal(err)
	}
}

func run(ctx context.Context) error {
	oidcToken, err := packslip.AmbientIDToken(ctx)
	if err != nil {
		return err
	}
	identity, err := packslip.IdentityFromToken(oidcToken)
	if err != nil {
		return err
	}

	const base = "https://github.com/owner/mytool/releases/download/v1.2.3/"
	predicate := packslip.Predicate{
		Project:     "github.com/owner/mytool",
		Version:     "1.2.3",
		PublishedAt: time.Now().UTC(),
		Source:      &packslip.Source{Repo: "https://github.com/owner/mytool", Tag: "v1.2.3"},
		Identity:    identity,
	}
	inputs := []packslip.ArtifactInput{{
		Path: "dist/mytool-1.2.3-linux-x64.tar.gz",
		Artifact: packslip.Artifact{
			Name:   "mytool-1.2.3-linux-x64.tar.gz",
			URL:    base + "mytool-1.2.3-linux-x64.tar.gz",
			OS:     "linux",
			Arch:   packslip.ArchFromGo("amd64", ""),
			Format: "tar.gz",
			Bin:    []packslip.Bin{packslip.NewBin("mytool")},
		},
	}}
	statement, err := packslip.Build(predicate, inputs)
	if err != nil {
		return err
	}

	options, err := packslip.ProductionKeylessOptions(ctx, oidcToken)
	if err != nil {
		return err
	}
	bundle, err := packslip.SignKeyless(statement, options)
	if err != nil {
		return err
	}
	return os.WriteFile("dist/packslip.sigstore.json", bundle, 0o644)
}
```

- `AmbientIDToken` returns `SIGSTORE_ID_TOKEN` when it is set. Otherwise
  it requests a `sigstore`-audience token from GitHub Actions, or returns
  `packslip.ErrNoIDToken`.
- `IdentityFromToken` derives the identity Fulcio will certify; set it as
  `Predicate.Identity`. It is the workflow URI for GitHub Actions and
  GitLab CI, otherwise the verified email or subject. It returns an error
  for an unusable token but does not verify it; Fulcio does. If the
  certificate's identity differs from the statement's, `SignKeyless` fails
  before anything is logged.
- `Version` must be the version the tag names ([Tags](/release/v1/#tags)).
- `Build` streams each local file at `Path` once to record its size,
  SHA-256, and SHA-512, so pass the final files, byte for byte as you
  upload them. `Name` is the published name. `Build` returns an error for
  malformed metadata or an unreadable artifact.
- `NewBin("mytool")` creates the `Bin` entry for a command at that path in
  the archive, so match the archive's layout. `NamedBin(path, name)` gives
  the command a name other than its file name.
- `ProductionKeylessOptions` takes the same token. It fetches Sigstore's
  public-good trust root and signing configuration (which names the Fulcio
  and Rekor servers) over TUF, caches them under `~/.sigstore/root`, and
  returns an error when a fetch fails.
- `SignKeyless` returns the Sigstore bundle. Neither it nor
  `ProductionKeylessOptions` reads a token from the environment or
  requests one.

Before you publish, check the bundle. For a GitHub project,
`packslip verify` derives the signer from the project's name:

```sh
packslip verify dist/packslip.sigstore.json --artifact dist/mytool-1.2.3-linux-x64.tar.gz
```

Upload the bundle beside the artifacts as `packslip.sigstore.json`. When
several tools of a monorepo share one GitHub release, name each bundle
after its tool's subpath, such as `packslip.mytool.sigstore.json` for
`github.com/owner/repo/mytool` ([Monorepos](/release/v1/#monorepos)).
`BundleName(project, repo)` gives the name. With `repo` set to
`github.com/owner/repo`, the project `github.com/owner/repo/mytool` is
`packslip.mytool.sigstore.json`. The repository itself, or a project outside
it, gets `packslip.sigstore.json`.

To catch a wrong `Project` before anything is signed, call
`CheckProjectSigner(project, oidcToken)` after `AmbientIDToken`. A
`github.com` project must belong to the GitHub Actions repository that runs
the job, and a `gitlab.com` project must be the GitLab CI project. A reusable
workflow of another repository is fine. Otherwise consumers refuse the bundle
after it is in the transparency log. A domain project always passes, because
its vendor pins the signer. The check reads the token without verifying it, and
it also fails for a project that names a repository's old name.

## Map a Go build matrix

`ArchFromGo(goarch, goarm)` maps a `GOARCH` to the documented architecture
vocabulary ([Vocabularies](/release/v1/#vocabularies)): `amd64` to
`x86_64`, `arm64` to `aarch64`, `386` to `i686`, `ppc64le` to
`powerpc64le`, and `loong64` to `loongarch64`. Other architectures take
Rust's spelling (`mipsle` to `mipsel`, `mips64le` to `mips64el`, `ppc64` to
`powerpc64`) or pass through unchanged. `all`, GoReleaser's universal macOS
binary, gives no `arch`, so consumers treat the artifact as running on
every architecture.

For `arm`, it appends `GOARM` to `armv`, as in `armv7`. An empty `GOARM`
means 6, GoReleaser's default. A plain Go cross-build with `GOARM` unset
builds for 7, so pass the value your build used. A floating-point option
is ignored, so `7,softfloat` counts as 7. Values other than 6 and 7 fall
outside the vocabulary.

A `GoTarget` holds one build's settings (`GOARCH`, `GOARM`, `GOAMD64`,
`GO386`, `GOARM64`, `GOMIPS`, `GOPPC64`, `GORISCV64`, and `FloatABI`), so a
tool that stores them apart does not pick the right one itself. `Arch()` calls
`ArchFromGo`. `Variant()` calls `VariantFromGo` with the setting for `GOARCH`.
For `arm` it reads `FloatABI`, either Go's `softfloat` or `hardfloat` or a
Rust or Zig target ABI such as `gnueabi`, through `VariantFromRustABI`:

```go
target := packslip.GoTarget{GOARCH: "amd64", GOAMD64: "v3"}
artifact.Arch = target.Arch()       // "x86_64"
artifact.Variant = target.Variant() // "v3"
```

`VariantFromGo(goarch, level)` returns the `variant` that distinguishes
non-default CPU-level and floating-point builds of one platform. Pass the Go
setting for the architecture (`GOAMD64`, `GO386`, `GOARM64`, `GOMIPS`,
`GOPPC64`, `GORISCV64`, or `GOARM` with its float option for `arm`):
`VariantFromGo("amd64", "v3")` is `v3`, `VariantFromGo("arm", "7,softfloat")`
is `softfloat`, `("arm", "5,hardfloat")` is `5-hardfloat`, and a default-level
build such as `("amd64", "v1")` has no variant. A comma in a level becomes `-`, so `v8.2,lse` is `v8.2-lse`.
`VariantFromRustABI(abi)` does the same for a Rust target's ABI: `gnueabi`
and `musleabi` are `softfloat`, and `gnueabihf` has none
([Vocabularies](/release/v1/#vocabularies)).

`NewBareBin(artifactName, format, command)` names the executable inside a
bare artifact (format `raw`, `gz`, `xz`, `zst`, or `bz2`): the artifact's
name without the compression suffix, installed as `command` with any
trailing `.exe` dropped. This entry installs `mytool-1.2.3-darwin-arm64` as
`mytool`:

```go
name := "mytool-1.2.3-darwin-arm64.gz"
artifact := packslip.Artifact{
	Name:   name,
	OS:     "darwin",
	Arch:   packslip.ArchFromGo("arm64", ""),
	Format: "gz",
	Bin:    []packslip.Bin{packslip.NewBareBin(name, "gz", "mytool")},
}
```

## Add separately uploaded assets

Pass each separately uploaded release asset that a resource references
in `packslip.BuildOptions`. For a man page shipped as its own release file,
replace the `Build` call in the program above with:

```go
predicate.Resources = []packslip.Resource{{Kind: "man", Asset: "mytool.1"}}
statement, err := packslip.Build(predicate, inputs, packslip.BuildOptions{
	Assets: []packslip.AssetInput{{Name: "mytool.1", Path: "dist/mytool.1"}},
})
```

Assets are hashed into the statement's subjects but are not artifacts.
`Build` refuses an asset that no resource names.
[Ship resources](/docs/resources/) lists the resource kinds.

## Sign a domain or GitLab project

To sign a domain project, or a `gitlab.com` project from GitLab CI:

1. Set `Project` to the project's name, `Source.Repo` to the repository
   you build from, and each artifact's `URL` to where you upload it
   ([Choose a project name](/docs/other-ci/#choose-a-project-name)).
2. Outside GitHub Actions, put an OIDC token for the `sigstore` audience
   in `SIGSTORE_ID_TOKEN`. On GitLab CI, the `id_tokens` entry in
   [Sign from GitLab CI](/docs/other-ci/#sign-from-gitlab-ci) does that.
3. Check the bundle with `packslip verify` and the identity flags consumers
   will use. A `gitlab.com` project needs none.
4. For a domain project, add each release to its signed release list with
   `packslip releases`; consumers refuse a domain project without one. The
   module does not build lists
   ([Create the list](/docs/release-lists/#create-the-list)).

The module cannot sign on Buildkite or CircleCI: Fulcio certifies a
pipeline URL there, which never matches the derived identity, so
`SignKeyless` fails. Sign those releases with `packslip create --key`
([Sign with a key](/docs/other-ci/#sign-with-a-key)).

## Prepare for later releases

Consumers hold each release to the ones they accepted before
([Keep later releases acceptable](/docs/publishing/#keep-later-releases-acceptable)).
From the first release:

- Publish the signer fingerprint that `packslip pin` prints for a bundle
  you verified
  ([Publish your signer fingerprint](/docs/publishing/#publish-your-signer-fingerprint)).
- Sign every release from the same workflow or pipeline file. Consumers
  refuse one from another file until a person approves it.
- Once you link build provenance, keep setting `Provenance` on each
  artifact.
- To sign from several workflow files, turn off workflow pinning before
  `Build` with `f := false; predicate.Identity.PinWorkflow = &f`, as
  `packslip create --no-pin-workflow` does.

## Check what signing establishes

| Call | Establishes | Does not establish |
| --- | --- | --- |
| `Build`, `Statement.Validate` | Only structural validity | Who made the statement |
| `SignKeyless` | sigstore-go verified the returned bundle against the trust root in the options, and its certificate names the statement's identity | That the files are safe, or how they were built |

**Warning:** Never treat parsed statement JSON as cryptographically
verified, even when `Statement.Validate` accepts it. Verify the bundle
with `packslip verify` or a library that verifies
([Choose a library](/docs/libraries/)).

The [Go package reference](https://pkg.go.dev/github.com/jdx/packslip/go)
documents every exported name; the
[module README](https://github.com/jdx/packslip/tree/main/go) summarizes
the API.
