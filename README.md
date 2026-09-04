<p>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="static/logo-dark.svg">
    <img src="static/logo.svg" alt="packslip" width="300" height="78">
  </picture>
</p>

> **Work in progress.** packslip is a draft, not a standard. mise ships
> experimental support for it, but the format and the tooling still change
> between releases, so use it for testing and nothing you depend on.
> Expect breaking changes until it is declared stable. Feedback is welcome
> in [issues](https://github.com/jdx/packslip/issues).

A signed release manifest for vendor binaries. One file per release,
`packslip.sigstore.json`, says what shipped and how to verify it; any
consumer checks it against one pinned identity or key and gets checksums,
platforms, executables, provenance links, and whatever else ships with
them (completions, man pages, a CLI spec, a skill, a desktop entry), with
no per-vendor logic and no registry entry.

Site and specification: [packslip.dev](https://packslip.dev) ·
[release/v1](https://packslip.dev/release/v1/)

Created by [Jeff Dickey (@jdx)](https://github.com/jdx), the author of
[mise](https://mise.jdx.dev) and [usage](https://usage.jdx.dev).

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
      resources: |
        completion/zsh=archive:share/zsh/site-functions/_mytool
        man=archive:share/man/man1/mytool.1
        cli-spec/usage=exec:mytool usage
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

Names are host paths. A tool in a monorepo adds a subpath
(`github.com/oxc-project/oxc/oxlint`) and gets its own packslip per
release, named `packslip.oxlint.sigstore.json`; the identity pin stays
the repository. Two builds for one platform take a variant
(`tool-fips-linux-x64.tar.gz@fips`), an executable whose PATH name differs
from its file is `--bin oxlint=bin/oxlint-x86_64`, and a repository that
redistributes a vendor's artifacts without a vendor packslip signs one of
its own with `--attested-by repackager --evidence apt-release-gpg=<key>`.

What ships besides the executables is a `resources` entry with a kind and
one source: `--resource completion/zsh=archive:share/zsh/site-functions/_mytool`
for a file inside every archive, `--resource skill/mytool=repo:skills/mytool`
for a path at the release commit, `--resource skill/mytool=asset:dist/skill.tar.gz`
for a separate release file digested alongside the artifacts,
`--resource sbom/cyclonedx=asset:dist/mytool.cdx.json` for a bill of
materials that verifies like an artifact, and
`--resource 'completion/bash,zsh,fish=exec:mytool completion {shell}'` for
a command consumers may run. A `cli-spec/usage` entry points at a
[usage](https://usage.jdx.dev) spec, from which a consumer derives
completions, a man page, and docs with its own tooling. Desktop
applications add `desktop`, `icon`, and `app` entries; there is no CLI or
GUI type, since the entries say which a release is.

What the host must already have is `requires`, in names the operating
system resolves rather than package names. `packslip create` opens each
archive and records the shared libraries its executables load
(`libssl.so.3`, `vcruntime140.dll`) as `libs`, leaving out the C runtime
and anything the archive ships itself. A command the executables run and
cannot work without is declared: `--require bin:java@17`. Neither says
where to get anything; that is the consumer's call, and a distribution
resolves a soname or a command name to a package on its own.

Anything the spec has no field for goes under `extensions`, keyed by who
defines it: `--extension 'example.com={"build_id":"20260901.3"}'` for the
vendor's own data, or a consumer's name such as `mise` for hints that
consumer documents. packslip never assigns meaning to a key there, so it
cannot collide with a field a later revision adds.

`--bin mytool` is looked up inside each archive, so the packslip records
the true path (`mytool-1.2.3-linux-x64/mytool`), and a bare executable
such as `mytool-linux-arm64` or `mytool.exe` gets that name on PATH. What
the flags cannot say per artifact, such as executables at different paths
in different archives, a `.exe` that is the program rather than an
installer, host requirements, or an artifact that runs anywhere, goes in
a TOML manifest passed as `--manifest release.toml`:

```toml
bin = ["mytool"]
requires = { glibc_min = "2.31" }

[[artifact]]
path = "dist/mytool-1.2.3-windows-x64.exe"
format = "raw"

[[resource]]
kind = "man"
os = "linux"
archive = "mytool-1.2.3-linux-x64/share/man/man1/mytool.1"
```

Consumers list a GitHub project's versions from its tags, so a tag is the
version, optionally after a `v` and the tool's or repository's name
(`v1.2.3`, `oxlint_v1.0.0`, `jq-1.7.1`). A vendor that needs to withdraw
a release, or whose tags name no version, commits a signed release list
at `.well-known/packslip.json` on the default branch.

## Layout

- `src/`, `tests/` — the `packslip` crate: schema, generator, verifier.
- `action.yml` — the composite GitHub Action.
- `docs/spec/packslip.md` — the specification's canonical text.
- `packslip.usage.kdl`, `content/cli/`, `content/spec.md`, `static/schema/` — the generated usage spec, CLI reference, spec page, and JSON schemas.
- `content/`, `layouts/`, `static/` — the Hugo source for packslip.dev, served by GitHub Pages.

Run `mise run render` after changing CLI arguments, help text, the schema, or
the specification, and commit the updated spec and pages. Run `mise run docs` to regenerate them and preview the
site locally; `mise run docs:build` performs the production build. The Pages
workflow fetches the current GitHub star count into `data/github.json` before
building, so visitors receive it in the rendered HTML.

MIT licensed. Copyright (c) 2026 Jeff Dickey (@jdx).
