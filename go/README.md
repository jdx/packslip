# packslip Go module

This module is the publisher-facing Go library for [packslip release-v1](https://packslip.dev/release/v1/) statements. It constructs release statements from explicit artifact metadata, streams artifact digests, validates the statement, and signs it keylessly with Sigstore.

It is intentionally not a GoReleaser adapter or a release CLI. Mapping build platforms, archive names, upload destinations, and release-tool configuration stays in an adapter owned by that tool or its users.

```go
statement, err := packslip.Build(packslip.Predicate{
    Project: "github.com/acme/tool",
    Version: "0.1.0",
    PublishedAt: time.Now().UTC(),
    Source: &packslip.Source{Repo: "github.com/acme/tool", Tag: "v0.1.0"},
    Identity: packslip.Identity{
        Scheme: "sigstore-oidc",
        KeyID: "https://github.com/acme/tool/.github/workflows/release.yml@refs/tags/v0.1.0",
        Issuer: "https://token.actions.githubusercontent.com",
    },
}, []packslip.ArtifactInput{{
    Path: "dist/tool-linux-x64.tar.xz",
    Artifact: packslip.Artifact{Name: "tool-linux-x64.tar.xz", OS: "linux", Arch: "x86_64", Format: "tar.xz", Bin: []packslip.Bin{packslip.NewBin("tool")}},
}})
if err != nil { /* handle malformed metadata or an unreadable artifact */ }

options, err := packslip.ProductionKeylessOptions(ctx, oidcToken)
if err != nil { /* handle public-good TUF discovery */ }
bundle, err := packslip.SignKeyless(statement, options)
```

When a resource references a separately uploaded release asset, pass it as
`packslip.BuildOptions{Assets: []packslip.AssetInput{{Name: "...", Path: "..."}}}`.
Assets are hashed into subjects but are not artifacts.

`Build` and `Statement.Validate` only establish structural validity. `SignKeyless` returns a Sigstore bundle after `sigstore-go` has verified it against the injected trusted root. Never treat parsed JSON as cryptographically verified.

## Releases

This is an independent Go module. Its versions use tags such as `go/v0.1.0`; root `vX.Y.Z` tags remain the Rust CLI/action release stream. The repository's release-plz configuration publishes the root Cargo crate only, so it does not publish this module. Go module distribution follows the normal Go module proxy behavior—there is no registry upload step.

After review, a maintainer validates the checked-out `main` revision, then creates and pushes an annotated module tag:

```sh
cd go
go mod tidy
git diff --exit-code -- go.mod go.sum
gofmt -w *.go cmd/conformance/*.go
go test ./...
git -C .. tag -a go/v0.1.0 -m 'packslip Go v0.1.0'
git -C .. push origin go/v0.1.0
```
