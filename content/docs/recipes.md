---
title: Release recipes
description: Complete release.toml files for a Rust CLI, a Go CLI, a monorepo tool, and a desktop app, plus a multi-platform release that needs none.
group: publish
weight: 40
---
# Release recipes

Copy the recipe that matches your release layout to get a signed release
bundle to publish beside your files. To look up a single key, see the [release.toml reference](/docs/release-toml/).

Each recipe starts after your build has produced the files and gives the
complete `release.toml`. Replace the example project, version, URLs, and
paths with your own.

| Recipe | Use it when |
| --- | --- |
| [Rust CLI](#rust-cli-with-bundled-documentation) | An archive includes a command, a static completion script, and a man page. |
| [Go CLI](#go-cli-with-generated-completions) | The installed command generates completions for several shells. |
| [Monorepo tool](#monorepo-tool-with-an-executable-alias) | A tool needs its own project name or a command alias. |
| [Desktop application](#desktop-application-for-linux-and-macos) | Linux and macOS releases provide different application resources. |
| [Several platforms](#several-platforms-from-release-to-verification) | Each platform has one archive whose name says the platform. No release.toml needed. |

The metadata choices work with any build language: a Rust CLI can use
generated completions, and a Go CLI can ship static documentation.

The APT and RPM packages leave out the publishing commands;
[install packslip another way](/docs/install/#choose-a-method).

## Create a bundle from a recipe

The first four recipes name github.com projects. Consumers accept those
by default only from a GitHub Actions run of the repository, as
[What a project name says about the signer](/docs/trust/#what-a-project-name-says-about-the-signer)
explains.

| Where you run it | What to do |
| --- | --- |
| GitHub Actions in the project's repository | Pass `release.toml` to the action's `manifest` input, and list the same files in `artifacts` at the recipe's paths. [Use release.toml with the action](/docs/publishing/#use-releasetoml-with-the-action) says which keys the action's inputs replace. |
| Another CI system | Give the job an OIDC token for Sigstore, as [Sign keylessly](/docs/other-ci/#sign-keylessly) shows. Change `project` to a domain project such as `mytool.example.com`, as [Choose a project name](/docs/other-ci/#choose-a-project-name) explains. Then follow the steps below. |
| Your own machine, to try a recipe | Run [Try a recipe on your own machine](#try-a-recipe-on-your-own-machine) instead. The trial bundle is not a release people can install. |

To release with a key instead of a CI identity, use a domain project, as
[Sign with a key](/docs/other-ci/#sign-with-a-key) explains.

In another CI system:

1. Save the recipe as `release.toml`. Its artifact paths are relative to
   the directory where you run `packslip create`.
2. Create the bundle, signed keylessly with the job's OIDC token:

   ```sh
   packslip create --manifest release.toml --out packslip
   ```

   It writes `packslip/packslip.sigstore.json`.
3. Inspect the signed statement:

   ```sh
   packslip show packslip/packslip.sigstore.json
   ```

4. Verify the bundle against the files before you publish. This GitLab
   CI example requires a pipeline of this project as the signer:

   ```sh
   packslip verify packslip/packslip.sigstore.json \
     --identity-prefix "https://gitlab.com/$CI_PROJECT_PATH//" \
     --issuer https://gitlab.com \
     --artifact dist/mytool-1.2.3-linux-x64.tar.gz
   ```

   - `--identity-prefix` and `--issuer` name the signer you expect. For a
     domain project, they are the whole policy. With another OIDC issuer,
     pass the `--identity` and `--issuer` that
     [Sign keylessly](/docs/other-ci/#sign-keylessly) lists.
   - `--artifact` checks one file against the bundle. Pass it once for
     each `[[artifact]]` path in the recipe.
5. Publish the bundle with the artifacts and any separate resource
   assets, then a new signed release list for the domain project, as
   [Publish the files](/docs/other-ci/#publish-the-files) shows.

### Try a recipe on your own machine

A trial signs with a local key and runs offline. Put the recipe's files
at its paths, save it as `release.toml`, and run:

```sh
packslip keygen --out release.key
packslip create --manifest release.toml \
  --key release.key --no-log --out packslip
packslip show packslip/packslip.sigstore.json
packslip verify packslip/packslip.sigstore.json \
  --pubkey release.pub --allow-unlogged \
  --artifact dist/mytool-1.2.3-linux-x64.tar.gz
```

- `--key release.key --no-log` signs with the local key and skips the
  transparency log, as in
  [Create and sign the packslip](/docs/getting-started/#create-and-sign-the-packslip).
- `--pubkey release.pub --allow-unlogged` checks the signature against
  the public key `packslip keygen` wrote, and accepts a bundle with no log
  entry.
- Pass `--artifact` once for each `[[artifact]]` path in the recipe.

The monorepo recipe writes `packslip/packslip.lint.sigstore.json`
instead; use that path with `show` and `verify`.

## Rust CLI with bundled documentation

This layout ships the executable, a static zsh completion, and a man page
in one archive. Generate the completion and man page in your existing
build job. Every resource path includes the archive's top-level
directory.

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

`bin = ["mytool"]` finds the executable and records its full path,
`mytool-1.2.3/bin/mytool`. The resources are unscoped, so every archive
you add for another target must hold these files at the same paths. If
the top-level directory differs per target,
[scope each resource to its artifact](/docs/resources/#scope-resources-to-the-right-artifact).

## Go CLI with generated completions

This recipe assumes the tool implements `mytool completion SHELL`, which
prints a completion script to stdout. Adapt `exec` to your program's real
interface, because packslip does not add that command.

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

`packslip create` records the `exec` command without running it. The
consumer substitutes the requested shell for `{shell}` and caches
successful output for the installed version, executable, and shell.
[Running an exec entry](/release/v1/#running-an-exec-entry) in the
specification gives the rules.

To keep consumers from running your binary to generate completions, ship
static scripts or a usage spec instead, as
[Ship completions and CLI specifications](/docs/resources/#ship-completions-and-cli-specifications)
shows.

A binary built with `CGO_ENABLED=0` is statically linked.
`packslip create` reads that and records no `libc`, so the archive fits
glibc and musl hosts.

## Monorepo tool with an executable alias

A repository can release its tools independently or attach several tools
to one release. Give each tool its own project subpath and
`release.toml`, and list only that tool's artifacts. This recipe exposes
`lint-x86_64` as the command `lint`.

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

`packslip create` names the bundle after the project subpath:
`packslip.lint.sigstore.json`. A workflow of `owner/toolkit` still signs
it, so every tool in the repository is held to the same signer.
Consumers tell the tools apart by the signed `project`, not by the
bundle's file name. [Monorepos](/release/v1/#monorepos) in the
specification gives the rules.

Run `packslip create` and `packslip verify` once per tool, with that
tool's `release.toml` and bundle. Check that the verified `project` names
the tool you meant, as
[Verify against the expected repository](/docs/verifying/#verify-against-the-expected-repository)
explains. With the GitHub action, set its `project` and `version` inputs
per tool, as
[Release several tools from one repository](/docs/publishing/#release-several-tools-from-one-repository)
shows.

When several tools share one release, use that release's tag and
download URL in each tool's `release.toml`. If the shared tag does not
name a tool's version, map it in a supplementary release list, as
[Decide whether you need a list](/docs/release-lists/#decide-whether-you-need-a-list)
explains.

## Desktop application for Linux and macOS

The Linux archive holds a runnable command and desktop integration files.
The macOS zip holds an application bundle, `MyApp.app`, with no command
for PATH. Scoping each resource to its exact artifact keeps either
platform from receiving the other's resources.

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

An app-aware consumer can install `MyApp.app`; a CLI-only consumer is not
required to. To ship more icon sizes, add an `icon` entry per hicolor
size, as [Ship desktop files and icons](/docs/resources/#ship-desktop-files-and-icons)
shows. [Desktop integration](/release/v1/#desktop-integration) in the
specification gives the rules.

**Warning:** A packslip signature does not replace platform code signing
or notarization.

## Several platforms, from release to verification

A release with one archive per platform needs no `release.toml` when the
file names say the platform. `packslip create` reads each file's OS,
architecture, and format from its name;
[How `packslip create` reads file names](/docs/release-toml/#how-packslip-create-reads-file-names)
lists the spellings.

```text
dist/mytool-1.2.3-linux-x64.tar.gz
dist/mytool-1.2.3-linux-arm64.tar.gz
dist/mytool-1.2.3-darwin-arm64.tar.gz
dist/mytool-1.2.3-windows-x64.zip
```

Like [Try a recipe on your own machine](#try-a-recipe-on-your-own-machine),
this trial signs with a local key and runs offline:

```sh
packslip keygen --out release.key
packslip create --key release.key --no-log \
  --project mytool.example.com --version 1.2.3 --bin mytool \
  --url-base https://mytool.example.com/v1.2.3 \
  --out packslip dist/*
```

- `--project` and `--version` name the release.
- `--bin mytool` names the command. `create` finds `mytool` in each
  archive, or `mytool.exe` in the Windows zip, and records its path.
  Without `--bin`, the bundle names no command to install.
- `--url-base` gives each file the download URL
  `https://mytool.example.com/v1.2.3/FILENAME`.

Check what `create` inferred with
`packslip show packslip/packslip.sigstore.json`:

| File | `os` | `arch` | `libc` | `format` | `bin` |
| --- | --- | --- | --- | --- | --- |
| `mytool-1.2.3-linux-x64.tar.gz` | `linux` | `x86_64` | Read from `mytool` | `tar.gz` | `mytool` |
| `mytool-1.2.3-linux-arm64.tar.gz` | `linux` | `aarch64` | Read from `mytool` | `tar.gz` | `mytool` |
| `mytool-1.2.3-darwin-arm64.tar.gz` | `darwin` | `aarch64` | none | `tar.gz` | `mytool` |
| `mytool-1.2.3-windows-x64.zip` | `windows` | `x86_64` | none | `zip` | `mytool.exe` |

These names give no C library, so `create` reads it from each Linux
`mytool`: no `libc` for a static build, otherwise `musl` or `gnu`.
[C library](/docs/release-toml/#c-library) gives the whole rule.

Check all four files against the bundle:

```sh
packslip verify packslip/packslip.sigstore.json \
  --pubkey release.pub --allow-unlogged \
  --artifact dist/mytool-1.2.3-linux-x64.tar.gz \
  --artifact dist/mytool-1.2.3-linux-arm64.tar.gz \
  --artifact dist/mytool-1.2.3-darwin-arm64.tar.gz \
  --artifact dist/mytool-1.2.3-windows-x64.zip
```

`--artifact` matches each file by name, so keep the published names.

```text
ok: mytool.example.com 1.2.3 published 2026-10-22T01:49:41.915031799Z signed by 52EED790A006DFEE (sigstore-key) unlogged (4 of 4 artifact(s) checked)
  requires mytool-1.2.3-darwin-arm64.tar.gz: libs none
  requires mytool-1.2.3-linux-arm64.tar.gz: libs none
  requires mytool-1.2.3-linux-x64.tar.gz: libs none
  requires mytool-1.2.3-windows-x64.zip: libs none
```

Your key ID and time differ. `create` and `verify` print a `requires`
line for each artifact whose executable `create` could read; `libs none`
means it needs no shared libraries beyond the C runtime and the OS's own.
[Let `create` read shared libraries](/docs/host-requirements/#let-create-read-shared-libraries)
explains what `create` records, and
[Understand the result](/docs/verifying/#understand-the-result) explains
the rest.

With GitHub Actions, pass the files to the action's `artifacts` input
and the command to `bin`, as
[Add the release step to an existing job](/docs/publishing/#add-the-release-step-to-an-existing-job)
shows. On a tag push, the action derives the project, version, and
download URLs.

From other CI, sign as
[Choose how to sign](/docs/other-ci/#choose-how-to-sign) shows. A keyless
`create` drops `--key` and `--no-log`, and its verifiers replace
`--pubkey` and `--allow-unlogged` with identity flags, as
[Narrow the policy with identity flags](/docs/verifying/#narrow-the-policy-with-identity-flags)
shows.
