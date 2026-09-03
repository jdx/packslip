# packslip

A signed release manifest for vendor binaries. One file per release,
`packslip.sigstore.json`, says what shipped and how to verify it; any
consumer checks it against one pinned identity or key and gets checksums,
platforms, executables, and provenance links, with no per-vendor logic and
no registry entry.

Site and specification: [packslip.dev](https://packslip.dev) ·
[release/v1](https://packslip.dev/release/v1/)

## In a GitHub release job

```yaml
permissions:
  contents: write       # upload the release asset
  id-token: write       # sign with this job's identity
  attestations: write   # attest build provenance for the artifacts

steps:
  - uses: jdx/packslip@v1
    with:
      artifacts: dist/*
      bin: mytool
```

That attests SLSA build provenance for the artifacts, digests them, signs
the manifest keylessly through sigstore with the workflow's own identity,
links the provenance from it, and uploads `packslip.sigstore.json` to the
release. There is no key to create or store. Consumers verify against the
repository name: a packslip for `github.com/owner/repo` must be signed by
a workflow of that repository.

## The binary

```sh
cargo install packslip          # or download a release archive
packslip create --project github.com/owner/repo --version 1.2.3 \
  --out dist --url-base https://github.com/owner/repo/releases/download/v1.2.3 \
  --bin mytool dist/*.tar.xz
packslip verify dist/packslip.sigstore.json --artifact mytool-1.2.3-linux-x64.tar.xz
packslip show dist/packslip.sigstore.json
```

Inside a CI job `create` signs keylessly. Elsewhere, `packslip keygen`
makes an Ed25519 key and `create --key release.key` signs with it, still
logging to Rekor; consumers then verify with `--pubkey release.pub`. A
project on its own domain publishes a signed release list with
`packslip releases`.

## Layout

- `src/`, `tests/` — the `packslip` crate: schema, generator, verifier.
- `action.yml` — the composite GitHub Action.
- `docs/spec/packslip.md` — the specification's canonical text.
- `site/` — packslip.dev, served by GitHub Pages.

MIT licensed.
