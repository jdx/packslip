# packslip Go module

This module is the publisher-facing Go library for [packslip release-v1](https://packslip.dev/release/v1/) statements. It constructs release statements from explicit artifact metadata, streams artifact digests, validates the statement, and signs it keylessly with Sigstore.

Install it with `go get github.com/jdx/packslip/go@latest`; it requires Go
1.25.8 or newer. The first proxy-installable version is v1.9.1. See
[the library guide](https://packslip.dev/docs/libraries/) for the Go and npm
roles and release cadence.

It is intentionally not a GoReleaser adapter or a release CLI. Mapping build platforms, archive names, upload destinations, and release-tool configuration stays in an adapter owned by that tool or its users.

```go
// In CI: the OIDC token (SIGSTORE_ID_TOKEN, or a GitHub Actions token for the
// `sigstore` audience) and the identity Fulcio will certify for it.
oidcToken, err := packslip.AmbientIDToken(ctx) // packslip.ErrNoIDToken when there is none
if err != nil { /* handle a missing token */ }
identity, err := packslip.IdentityFromToken(oidcToken)
if err != nil { /* handle an unusable token */ }

statement, err := packslip.Build(packslip.Predicate{
    Project: "github.com/acme/tool",
    Version: "0.1.0",
    PublishedAt: time.Now().UTC(),
    Source: &packslip.Source{Repo: "https://github.com/acme/tool", Tag: "v0.1.0"},
    Identity: identity,
}, []packslip.ArtifactInput{{
    Path: "dist/tool-linux-x64.tar.xz",
    Artifact: packslip.Artifact{Name: "tool-linux-x64.tar.xz", OS: "linux", Arch: "x86_64", Format: "tar.xz", Bin: []packslip.Bin{packslip.NewBin("tool")}},
}})
if err != nil { /* handle malformed metadata or an unreadable artifact */ }

options, err := packslip.ProductionKeylessOptions(ctx, oidcToken)
if err != nil { /* handle public-good TUF discovery */ }
bundle, err := packslip.SignKeyless(statement, options)
```

`IdentityFromToken` derives the `Identity` Fulcio will certify for the token,
so the statement and the certificate agree.

These are the only functions that read the environment or make an OIDC
request; `SignKeyless` and `ProductionKeylessOptions` never do. The token is
not verified by `IdentityFromToken`: a mismatch fails in `SignKeyless` before
anything is logged.

Four helpers map a build matrix to the documented vocabularies:
`ArchFromGo(goarch, goarm)` translates a `GOARCH` (`amd64` to `x86_64`,
`arm` with `GOARM=7` to `armv7`; a universal macOS binary gets no arch),
`VariantFromGo(goarch, level)` returns the `variant` of a build above Go's
default CPU level (`goamd64: v3` is `v3`; soft-float ARM is `softfloat`; a
default-level build has none), `VariantFromRustABI(abi)` does the same for a
Rust target's ABI (`gnueabi` is `softfloat`, `gnueabihf` has none), and
`NewBareBin(artifactName, format, command)` names the executable inside a
`raw`, `gz`, `xz`, `zst`, or `bz2` artifact.

When a resource references a separately uploaded release asset, pass it as
`packslip.BuildOptions{Assets: []packslip.AssetInput{{Name: "...", Path: "..."}}}`.
Assets are hashed into subjects but are not artifacts.

`Build` and `Statement.Validate` only establish structural validity. `SignKeyless` returns a Sigstore bundle after `sigstore-go` has verified it against the injected trusted root. Never treat parsed JSON as cryptographically verified.

## Releases

This module shares the CLI/action/npm release version. When release-plz creates
the root `vX.Y.Z` tag, the release workflow creates `go/vX.Y.Z` at the same
commit; the `go/` prefix is required for a subdirectory module. The repository's
release-plz configuration publishes the root Cargo crate only. Go module
distribution follows the normal Go module proxy behavior—there is no registry
upload step or separate Go release dispatch.
