---
title: Release recipes
weight: 36
group: publish
description: Example packslip configurations for Rust and Go CLIs, monorepo tools, and desktop applications.
---
# Release recipes

These recipes start after your build has produced release files. Each
shows the expected layout and a complete `release.toml` for those files.
Replace the example project, version, URLs, and paths with your own.

Save one recipe as `release.toml`, then create the bundle in a CI job
that provides an OIDC identity (GitHub Actions with `id-token: write`, or
any job that sets `SIGSTORE_ID_TOKEN`, such as a GitLab CI job with an
`id_tokens` entry of that name and audience `sigstore`):

```sh
packslip create --manifest release.toml --out packslip
```

For local key signing, add `--key release.key`. `packslip create` writes
`packslip/packslip.sigstore.json`
(`packslip/packslip.lint.sigstore.json` for the monorepo recipe) and
uploads nothing, so publish it with the artifacts.

On GitHub, the action in [Publish with GitHub Actions](/docs/publishing/)
can run a recipe instead: pass `manifest: release.toml` and set
`artifacts` to the same files, at the paths the recipe lists. The
action's `download` input saves files in a temporary directory that an
`[[artifact]]` `path` cannot name, so it does not work for these recipes.
The action's inputs and their defaults replace the recipe's `project`,
`version`, `url_base`, `notes_url`, and `[source]`, so set its `project`,
`version`, and `tag` inputs wherever the defaults are wrong; see
[Action inputs](/docs/publishing/#action-inputs).

## Rust CLI with bundled documentation

This layout ships the executable, a static zsh completion, and a man page
in one archive. Generate the documentation in your existing build job.
All resource paths include the archive's top-level directory.

```text
dist/mytool-1.2.3-linux-x64.tar.gz
└── mytool-1.2.3/
    ├── bin/mytool
    └── share/
        ├── zsh/site-functions/_mytool
        └── man/man1/mytool.1
```

<!-- docs-test: recipe rust -->
```toml
project = "github.com/owner/mytool"
version = "1.2.3"
url_base = "https://github.com/owner/mytool/releases/download/v1.2.3"

[source]
repo = "https://github.com/owner/mytool"
tag = "v1.2.3"

[[artifact]]
path = "dist/mytool-1.2.3-linux-x64.tar.gz"
bin = ["mytool"]

[[resource]]
kind = "completion"
shell = "zsh"
bin = "mytool"
archive = "mytool-1.2.3/share/zsh/site-functions/_mytool"

[[resource]]
kind = "man"
bin = "mytool"
archive = "mytool-1.2.3/share/man/man1/mytool.1"
```

`bin = ["mytool"]` finds the executable and records its full archive
path. Resource paths are recorded as written and not checked against the
archive, so copy them from `tar -tzf` output. Unscoped resources apply to
every artifact, so when you add archives for other targets, each must
hold these files at the same paths. If the top-level directory differs
per target,
[scope each resource to its artifact](/docs/resources/#scope-resources-to-the-right-artifact).

## Go CLI with generated completions

This example assumes the tool implements `mytool completion SHELL`,
printing a completion script to stdout. Adapt `exec` to the interface
your program actually supports; packslip does not add that command.

```text
dist/mytool-1.2.3-linux-x64.tar.gz
└── mytool
```

<!-- docs-test: recipe go -->
```toml
project = "github.com/owner/mytool"
version = "1.2.3"
url_base = "https://github.com/owner/mytool/releases/download/v1.2.3"

[source]
repo = "https://github.com/owner/mytool"
tag = "v1.2.3"

[[artifact]]
path = "dist/mytool-1.2.3-linux-x64.tar.gz"
bin = ["mytool"]

[[resource]]
kind = "completion"
bin = "mytool"
shells = ["bash", "zsh", "fish"]
exec = ["mytool", "completion", "{shell}"]
```

The consumer substitutes the requested shell and caches successful output
for the installed version, executable, and shell. `packslip create`
records this command; it does not run it. To avoid executing the binary
to generate completions, ship static scripts or a [usage spec](/docs/resources/#completions-and-cli-specifications).

If you build with `CGO_ENABLED=0`, the binary is statically linked:
`packslip create` reads that from the executable and records no `libc`,
so the archive fits glibc and musl hosts alike.

## Monorepo tool with an executable alias

A repository can release tools independently or attach several tools to
one release. Give each tool its own project subpath and `release.toml`,
and include only that tool's artifacts. This example exposes `lint-x86_64`
as the command `lint`.

```text
dist/lint-1.2.3-linux-x64.tar.gz
└── bin/lint-x86_64
```

<!-- docs-test: recipe monorepo -->
```toml
project = "github.com/owner/toolkit/lint"
version = "1.2.3"
url_base = "https://github.com/owner/toolkit/releases/download/lint-v1.2.3"

[source]
repo = "https://github.com/owner/toolkit"
tag = "lint-v1.2.3"

[[artifact]]
path = "dist/lint-1.2.3-linux-x64.tar.gz"
bin = [{ name = "lint", path = "bin/lint-x86_64" }]
```

`packslip create` names the bundle `packslip.lint.sigstore.json` after the
project subpath. In CI it is still signed by a workflow of
`owner/toolkit`, so every tool in the repository is held to the same
signer, and consumers tell the tools apart by the signed `project`, not
by the bundle's file name. Run `packslip create` separately for each
other tool. When several tools share a release, use that release's tag
and download URL in each tool's `release.toml`. A supplementary release
list can map a shared tag to each tool's version when the tag itself does
not; see [Manage release lists](/docs/release-lists/).

With the action, set `project: github.com/owner/toolkit/lint` and
`version: 1.2.3`; the defaults would sign for the repository's project
and turn `lint-v1.2.3` into an invalid version. See
[Release several tools from one repository](/docs/publishing/#release-several-tools-from-one-repository).

## Desktop application for Linux and macOS

The Linux archive contains a runnable command and desktop integration
files. The macOS zip contains an application bundle without a PATH
command. Exact artifact scope prevents either platform from receiving
the other's resources.

```text
dist/myapp-1.2.3-linux-x64.tar.gz
├── bin/myapp
└── share/
    ├── applications/myapp.desktop
    └── icons/hicolor/256x256/apps/myapp.png

dist/myapp-1.2.3-darwin-arm64.zip
└── MyApp.app/
    └── Contents/…
```

<!-- docs-test: recipe desktop -->
```toml
project = "github.com/owner/myapp"
version = "1.2.3"
url_base = "https://github.com/owner/myapp/releases/download/v1.2.3"

[source]
repo = "https://github.com/owner/myapp"
tag = "v1.2.3"

[[artifact]]
path = "dist/myapp-1.2.3-linux-x64.tar.gz"
bin = ["bin/myapp"]

[[artifact]]
path = "dist/myapp-1.2.3-darwin-arm64.zip"
bin = []

[[resource]]
kind = "desktop"
artifact = "myapp-1.2.3-linux-x64.tar.gz"
archive = "share/applications/myapp.desktop"

[[resource]]
kind = "icon"
artifact = "myapp-1.2.3-linux-x64.tar.gz"
archive = "share/icons/hicolor/256x256/apps/myapp.png"

[[resource]]
kind = "app"
artifact = "myapp-1.2.3-darwin-arm64.zip"
archive = "MyApp.app"
```

Consumers choose which resource kinds they support. An app-aware consumer
can install the application bundle; a CLI-only consumer is not required
to do so. A packslip signature does not replace platform code signing or
notarization.

## Several platforms, from release to verification

A release with one archive per platform needs no manifest when the file
names say the platform. Pass the files and `packslip create` infers each
one's OS, architecture, and libc, as
[Select platforms and variants](/docs/describing-releases/#select-platforms-and-variants)
explains:

```text
dist/mytool-1.2.3-linux-x64.tar.gz
dist/mytool-1.2.3-linux-arm64.tar.gz
dist/mytool-1.2.3-darwin-arm64.tar.gz
dist/mytool-1.2.3-windows-x64.zip
```

```sh
packslip create --key release.key --no-log \
  --project example.com/mytool --version 1.2.3 \
  --url-base https://downloads.example.com/v1.2.3 \
  --out packslip dist/*
```

This example signs with `--key release.key` and `--no-log` so it runs
anywhere, so `verify` below needs `--allow-unlogged`. In CI, omit all three
to sign keylessly and keep the transparency-log entry. Check the inference with
`packslip show packslip/packslip.sigstore.json` before publishing. For this
release it records:

| File | `os` | `arch` | `libc` | `format` |
| --- | --- | --- | --- | --- |
| `mytool-1.2.3-linux-x64.tar.gz` | `linux` | `x86_64` | `gnu` | `tar.gz` |
| `mytool-1.2.3-linux-arm64.tar.gz` | `linux` | `aarch64` | `gnu` | `tar.gz` |
| `mytool-1.2.3-darwin-arm64.tar.gz` | `darwin` | `aarch64` | none | `tar.gz` |
| `mytool-1.2.3-windows-x64.zip` | `windows` | `x86_64` | none | `zip` |

Deployment tooling can then check all four files in one call. Copy the
bundle and files to the build host, keeping their names, and pass each
file:

```sh
packslip verify packslip.sigstore.json --pubkey release.pub --allow-unlogged \
  --artifact mytool-1.2.3-linux-x64.tar.gz \
  --artifact mytool-1.2.3-linux-arm64.tar.gz \
  --artifact mytool-1.2.3-darwin-arm64.tar.gz \
  --artifact mytool-1.2.3-windows-x64.zip
```

```text
ok: example.com/mytool 1.2.3 published 2026-10-06T15:05:23Z signed by BEA040B74F3FAC9E (sigstore-key) unlogged (4 of 4 artifact(s) checked)
```

`4 of 4` shows every platform's file was supplied and matched. A deploy
that stages only one platform passes just that file and sees `1 of 4`.
For a monorepo, run the same `create` and `verify` once per tool: each
tool has its own bundle and `project`, and a `--pin` or identity prefix for
the repository applies to every tool, so check the verified `project`
names the tool you meant. If you need the verified metadata instead of
a pass or fail, see
[Verify a release](/docs/verifying/#check-every-file-you-use).

## Add build provenance

`release.toml` describes what the release contains; it does not produce
build provenance. By default (`attest: true`), the action in
[Publish with GitHub Actions](/docs/publishing/#build-provenance) attests the files its
`artifacts` and `download` inputs match and links those statements.
Without the action, list the URLs of provenance statements your build
already produced in the artifact's `provenance` key:

```toml
[[artifact]]
path = "dist/mytool-1.2.3-linux-x64.tar.gz"
provenance = ["https://example.com/provenance/mytool-1.2.3-linux-x64.intoto.jsonl"]
```

Or pass one URL per artifact to `packslip create` with this flag, which
adds to the entry's own list:

```text
--provenance FILENAME=URL
```

Consumers verify provenance separately from the packslip signature.
