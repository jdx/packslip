---
title: Release recipes
weight: 36
description: Example packslip configurations for Rust and Go CLIs, monorepo tools, and desktop applications.
---
# Release recipes

These recipes start after your build has produced release files. Each
shows the expected layout and a complete `release.toml` for those files.
Replace the example project, version, URLs, and paths with your own.

Save one recipe as `release.toml`, then create the bundle in a CI job
with a supported OIDC identity:

```sh
packslip create --manifest release.toml --out packslip
```

For local key signing, add `--key release.key`. Upload both the artifacts
and the bundle. The CLI does not upload them. On GitHub, you can instead
pass `manifest: release.toml` to the [action](/docs/publishing/), along
with an `artifacts` input matching the release files.

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
path. Resources use explicit paths. If you add archives with different
layouts, [scope the resources](/docs/resources/#scope-resources-to-the-right-artifact).

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
for the installed version, executable, and shell. Creation records this
command; it does not run it. To avoid executing the binary to generate
completions, ship static scripts or a [usage spec](/docs/resources/#completions-and-cli-specifications).

## Monorepo tool with an executable alias

A repository can release tools independently or attach several tools to
one release. Give each tool its own project subpath and manifest, and
include only that tool's artifacts. This example exposes `lint-x86_64`
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

The output is `packslip.lint.sigstore.json`; its signer still belongs to
`owner/toolkit`. Run creation separately for other tools. When several
tools share a release, use that release's tag and download URL for each
manifest. A supplementary [release list](/docs/release-lists/) can map a
shared tag to each tool's version when the tag itself does not do so.

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

## Add build provenance

These manifests describe release contents. They do not generate build
provenance. The [GitHub Action](/docs/publishing/) can attest its matched
files and link those statements. With the CLI, pass a provenance URL for
each artifact:

```text
--provenance FILENAME=URL
```

Consumers verify provenance separately from the packslip signature.
