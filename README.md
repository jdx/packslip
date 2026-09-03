# packslip

A signed release manifest for vendor binaries. One document per release
says what shipped and how to verify it; any consumer checks it against one
pinned identity and gets checksums, platforms, executables, provenance
links, and an evidence level, with no per-vendor logic and no registry
entry.

Site and specification: [packslip.dev](https://packslip.dev) ·
[release/v1](https://packslip.dev/release/v1/)

## In a GitHub release job

```yaml
permissions:
  contents: write   # upload the release assets
  id-token: write   # sign with this job's identity

steps:
  - uses: jdx/packslip@v1
    with:
      artifacts: dist/*
      bin: mytool
```

That digests the artifacts, signs the manifest keylessly through sigstore
with the workflow's own identity, and uploads `packslip.json` and
`packslip.sigstore.json` to the release. There is no key to create or
store. Consumers verify against the repository name: a packslip for
`github.com/owner/repo` must be signed by a workflow of that repository.

## The binary

```sh
cargo install packslip          # or download a release archive
packslip create --project github.com/owner/repo --version 1.2.3 \
  --out dist --url-base https://github.com/owner/repo/releases/download/v1.2.3 \
  --bin mytool dist/*.tar.xz
packslip verify dist/packslip.json --artifact mytool-1.2.3-linux-x64.tar.xz
```

Inside a CI job `create` signs keylessly. Elsewhere, `packslip keygen`
makes a minisign key and `create --key release.key` signs with it;
consumers then verify with `--pubkey release.pub`.

## Layout

- `src/`, `tests/` — the `packslip` crate: schema, generator, verifier.
- `action.yml` — the composite GitHub Action.
- `docs/spec/packslip.md` — the specification's canonical text.
- `site/` — packslip.dev, served by GitHub Pages.

MIT licensed.
