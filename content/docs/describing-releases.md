---
title: Artifact configuration
weight: 30
group: publish
description: Configure artifact platforms, variants, executable paths, and per-file metadata.
---
# Artifact configuration

A packslip describes the release files your build already produces. Use
`packslip create` to record their platforms, executable paths, download
URLs, and other metadata. It infers common file names and archive layouts;
you supply the details it cannot infer.

Start with flags for a uniform set of archives, or keep per-artifact
configuration in `release.toml`. For complete example layouts and
manifests, see [Release recipes](/docs/recipes/).

## Choose flags or a TOML manifest {#choose-flags-or-a-manifest}

Flags and `release.toml` produce the same signed release statement:
`release.toml` configures `packslip create` and is not a second
interchange format. This page also calls it the TOML manifest, and calls
the signed output the bundle.

Flags are enough when every artifact contains the same executables. For
example, in a CI job with a supported OIDC identity:

```sh
packslip create \
  --project github.com/owner/mytool --version 1.2.3 \
  --url-base https://github.com/owner/mytool/releases/download/v1.2.3 \
  --bin mytool --out packslip \
  dist/mytool-1.2.3-linux-x64.tar.gz \
  dist/mytool-1.2.3-darwin-arm64.tar.gz
```

This finds `mytool` in both archives and writes
`packslip/packslip.sigstore.json`. For local key signing, add
`--key release.key`; to try it without recording a signature in the public
log, also add `--no-log`, as in [Getting started](/docs/getting-started/).

Artifact suffixes (`PATH:os/arch/libc@variant`) and per-file flags
(`--format FILENAME=FORMAT`, `--url FILENAME=URL`) cover explicit platforms,
variants, formats, and URLs. Use `release.toml` when:

- artifacts hold different executables, or need an explicit executable
  path that differs between them;
- artifacts need `os_min` or `glibc_min`, or requirements that differ per
  artifact;
- an artifact carries its own extensions;
- a resource belongs to one exact artifact.

`release.toml` also keeps the configuration in the repository.

## Select platforms and variants

`packslip create` infers OS, architecture, and format from each
artifact's file name, and libc from the name or the executables inside.
Run `packslip show` on the bundle to check what it inferred before
publishing, and set explicit values for anything it got wrong.

### How `packslip create` reads file names {#check-inferred-metadata}

Inference reads file names and executables, not your build
configuration. The OS comes from a word in the name or from its
extension:

| In the file name | Recorded as |
| --- | --- |
| `linux`, or a `.deb`, `.rpm`, or `.AppImage` file | `linux` |
| `darwin`, `macos`, `apple`, or a `.dmg` or `.pkg` file | `darwin` |
| `windows`, `win32`, `win64`, or a `.exe`, `.msi`, or `.msix` file | `windows` |
| `freebsd`, `netbsd`, `openbsd`, `illumos`, `android`, `ios` | The same word |

A name with no OS records no `os`. Consumers then consider the artifact
compatible with every OS: `mytool-x86_64.tar.gz` is offered to any x86_64
host. `packslip create` does not warn about this, so check the inferred
platform before publishing a native build.

The format comes from the extension, such as `.tar.gz`, `.zip`, or
`.deb`; [Name the executables](#name-the-executables) explains how a
`.exe` is read. A file with no extension is a `raw` executable when its
OS is known, from the name or set explicitly, or when the artifact is
portable. A dotted version does not count as an extension, so
`mytool-1.2.3-linux-x64` has none. `packslip create` refuses any other
file whose name gives no format until you set one with `format` or
`--format FILENAME=FORMAT`.

Architectures are read in the spellings of Rust targets, goreleaser, and
common release names:

| In the file name | Recorded as |
| --- | --- |
| `x86_64`, `x86-64`, `x64`, `amd64` | `x86_64` |
| `aarch64`, `arm64` | `aarch64` |
| `armv7`, `armhf` | `armv7` |
| `armv6`, or a plain `arm` | `armv6` |
| `i686`, `i386`, `x86`, `386`, `ia32` | `i686` |
| `riscv64` | `riscv64` |
| `ppc64le`, `ppc64el`, `powerpc64le` | `powerpc64le` |
| `s390x` | `s390x` |
| `loongarch64`, `loong64` | `loongarch64` |
| `mips` | `mips` |
| `mipsel`, `mipsle` | `mipsel` |
| `mips64` | `mips64` |
| `mips64el`, `mips64le` | `mips64el` |
| `ppc64`, `powerpc64` | `powerpc64` |

A plain `arm` is ARMv6, the default of both goreleaser and Rust's `arm-`
targets. A name whose only hint is `win64` is recorded as `x86_64`. The
MIPS and `powerpc64` values follow Rust's target spellings and are
outside the [vocabularies](/release/v1/#vocabularies), so they match no
host.

`packslip create` refuses an artifact for any OS other than macOS and
Windows when its name gives no architecture, since it would fit every
host of that OS. Name one, or give `any` (`PATH:linux/any` or
`arch = "any"`) if the build really runs on all of them. A `.noarch.rpm`
or `_all.deb` package is exempt. A macOS or Windows artifact whose name
gives no architecture is recorded without `arch`, so every Mac or Windows
host can select it. By convention such a build is universal or x86_64,
and both systems run x86_64 builds on ARM.

For a Linux artifact whose name does not say `musl` or `gnu`,
`packslip create` reads libc from the listed executables. A statically
linked build, such as a Go binary built with `CGO_ENABLED=0`, gets no
`libc` and fits both glibc and musl hosts. A build that uses musl's loader
gets `musl`.

If `create` cannot read an ELF binary for every listed executable, libc
defaults to `gnu`, which does not fit a musl host. This happens when no
executables are listed, an executable is a script, the format cannot be
opened, or `--no-libs` is set. For a build that loads no C library, such
as an archive of shell scripts, set `libc = "any"` or use
`PATH:linux/x86_64/any` explicitly.

### Override the inferred platform

Use these argument forms, or the matching `[[artifact]]` fields in
`release.toml`, to set what inference gets wrong:

| Artifact argument | In `[[artifact]]` | Meaning |
| --- | --- | --- |
| `dist/mytool-linux-x64.tar.gz` | `path` only | Infer the platform from the file name. |
| `dist/mytool.tar.gz:linux/x86_64/gnu` | `os = "linux"`, `arch = "x86_64"`, `libc = "gnu"` | Set OS, architecture, and libc explicitly. |
| `dist/mytool.tar.gz:linux/x86_64/any` | `os = "linux"`, `arch = "x86_64"`, `libc = "any"` | A Linux x86_64 build that loads no C library from the host. |
| `dist/mytool.tar.gz:linux/any` | `os = "linux"`, `arch = "any"` | A Linux build for every architecture. Its libc is still inferred, so use `linux/any/any` when it needs none. |
| `dist/mytool.tar.gz:any` | `portable = true` | Clear platform fields for a portable artifact. |
| `dist/mytool-fips-linux-x64.tar.gz@fips` | `variant = "fips"` | Infer the platform and mark the build as the `fips` variant. |
| `dist/mytool.tar.gz:linux/x86_64/gnu@fips` | `os`, `arch`, `libc`, and `variant` | Explicit platform and variant together. |

Explicit values are recorded exactly as written; only file names are
translated. Write `darwin` for macOS, `x86_64` for x64 or amd64, and
`aarch64` for arm64. `packslip create` accepts any lowercase word, so
`:linux/amd64/gnu` succeeds but produces an artifact that matches no
host. See the complete [vocabularies](/release/v1/#vocabularies).

An absent platform field means the build does not depend on it. A
universal macOS binary has `os = "darwin"` and no `arch`: it runs on every
Mac, not on every OS. `arch = "any"` or `libc = "any"` (`/any` in the
argument) leaves that one field absent. `os` has no `any`: `os = "any"`
is recorded as written and matches no host. `portable = true`, or
`PATH:any`, leaves out all three.

### Separate builds that share a platform

`os`, `arch`, and `libc` describe where a build runs. `variant` distinguishes
builds a user chooses between, such as `fips` or `debug`. A minimum OS or
glibc version belongs in `requires`, which consumers check after choosing
an artifact, so it cannot tell apart two artifacts that share a platform.

Two artifacts may share a platform if their formats differ and they carry
the same build. Distinct builds for the same platform need a `variant`.
Consumers consider only artifacts without a variant unless one is requested.
If two artifacts have the same OS, architecture, libc, variant, and
format, `packslip create` refuses them.

## Name the executables

For `--bin mytool`, `packslip create` searches each archive it can open
(`tar`, `tar.gz`, `tgz`, `tar.xz`, `tar.zst`, `tar.bz2`, and `zip`) and
records the actual path, such as `mytool-1.2.3/bin/mytool`. A plain name
is matched by file name, and the shallowest match is recorded. Several
matches at that depth fail with a list of candidates, so give the path.
In a Windows artifact, `mytool` also finds `mytool.exe`.

A value that contains `/` is a path from the archive root, including any
top-level directory, and must exist exactly as written. For a format
`packslip create` cannot open, such as `7z` or an installer, the entry is
recorded as written, except that in a Windows artifact a path with no
extension gets `.exe`.

Top-level `bin` and `--bin` apply to every artifact that does not list
its own, so `packslip create` fails when one of those archives lacks the
executable. Give that artifact its own `bin` in `release.toml`, or
`bin = []` if it has none.

Use `--bin mytool=bin/mytool-x86_64` when the command name differs from the
file's name. Consumers expose `bin/mytool-x86_64` as the command `mytool`.
In TOML, write the equivalent as:

```toml
bin = [{ name = "mytool", path = "bin/mytool-x86_64" }]
```

A bare executable is an artifact that is the program itself: a `raw`
file, or one compressed executable (`.gz`, `.xz`, `.zst`, `.bz2`). For a
bare executable, `--bin mytool` gives the name the file gets on PATH. In
a Windows artifact, command names omit `.exe`; the file path keeps it.

`packslip create` records a `.exe` as the program itself (`raw`) unless
its name contains `setup` or `install`, which makes it an `exe`
installer. When the name misleads, set the format yourself:
`format = "exe"` or `--format FILENAME=exe` for an installer whose name
says neither, and `format = "raw"` or `--format FILENAME=raw` for a
program whose name contains `install`, such as
`cargo-binstall-x86_64.exe`.

## Use a TOML manifest

Keep configuration in `release.toml` when artifacts need different
metadata. This example describes a Linux archive and a Windows executable:

```toml
project = "github.com/owner/mytool"
version = "1.2.3"
url_base = "https://github.com/owner/mytool/releases/download/v1.2.3"
bin = ["mytool"]

[source]
repo = "https://github.com/owner/mytool"
tag = "v1.2.3"

[[artifact]]
path = "dist/mytool-1.2.3-linux-x64.tar.gz"
bin = ["mytool-1.2.3/bin/mytool"]
requires = { glibc_min = "2.31" }

[[artifact]]
path = "dist/mytool-1.2.3-windows-x64.exe"

[[resource]]
kind = "man"
artifact = "mytool-1.2.3-linux-x64.tar.gz"
archive = "mytool-1.2.3/share/man/man1/mytool.1"
```

In a CI job with a supported OIDC identity:

```sh
packslip create --manifest release.toml --out packslip
```

For local key signing, add `--key release.key`. For an offline trial, also
add `--no-log`; the [quickstart](/docs/getting-started/) shows how to
generate a temporary key and verify an unlogged bundle.

In the GitHub Action, set `manifest: release.toml` and set `artifacts` to
the files its `[[artifact]]` entries name. The action fails when
`artifacts` and `download` match no file, and it attests and links
provenance only for the files they match. `download` saves files in a
temporary directory that no `[[artifact]]` `path` can name, so use it only
for files `release.toml` does not list. The action's project, version,
download URL prefix, release-notes URL, and source settings, including
their defaults, replace those in `release.toml`; see
[Action inputs](/docs/publishing/#action-inputs).

### Keys in `release.toml` {#manifest-keys}

Each table lists the keys `release.toml` accepts at that level and the
flag, if any, that sets the same value.

At the top level:

| Key | Flag | Meaning |
| --- | --- | --- |
| `project` | `--project` | The project name. Required here or as the flag. |
| `version` | `--version` | The semver version. Required here or as the flag. |
| `url_base` | `--url-base` | Download URL prefix for artifacts and assets that set no `url`. |
| `notes_url` | `--notes-url` | URL of the release notes. |
| `published_at` | `--published-at` | RFC 3339 publish time; defaults to now. |
| `bin` | `--bin`, which adds to this list | Executables inside every artifact that does not list its own: a path, a plain name, or `{ name = "…", path = "…" }`. |
| `requires` | `--require`, which adds commands only | Host requirements for every artifact that does not state its own: `os_min`, `glibc_min`, `libs`, and `bin`. See [Host requirements](/docs/host-requirements/). |
| `extensions` | `--extension` | Release-level extensions; see [Add custom metadata](#add-custom-metadata). |

In the `[source]` table:

| Key | Flag | Meaning |
| --- | --- | --- |
| `repo` | `--source-repo` | Source repository URL. Required when the table is present. |
| `commit` | `--commit` | Source commit. A `repo` resource requires it. |
| `tag` | `--tag` | Release tag. |

In each `[[artifact]]` table:

| Key | Flag | Meaning |
| --- | --- | --- |
| `path` | The artifact argument | The local file, relative to the working directory. Required. |
| `os`, `arch`, `libc`, `portable`, `variant` | `PATH:os/arch/libc`, `PATH:any`, `PATH@variant` | Platform and variant; see [Override the inferred platform](#override-the-inferred-platform). |
| `format` | `--format FILENAME=FORMAT` | The format, when the file name does not give the right one. |
| `url` | `--url FILENAME=URL` | Download URL, when it is not `url_base/FILENAME`. |
| `bin`, `requires` | None | This artifact's executables and host requirements, replacing the top-level values. `bin = []` declares none. |
| `provenance` | `--provenance FILENAME=URL` | URLs of build provenance statements for this artifact. |
| `extensions` | None | Artifact-level extensions. |

Each `[[resource]]` table needs a `kind` and exactly one source:
`archive`, `asset`, `repo`, or `exec`. It can also take a scope
(`artifact`, `os`, `arch`, `libc`), qualifiers (`shell`, `shells`, `bin`,
`format`, `name`), `url` for an asset, `env` for an `exec` command, and
`extensions`. On the command line, `--resource` sets every key except
`artifact`, `url`, and `extensions`, and `--url FILENAME=URL` sets an
asset's URL. See [Resources](/docs/resources/).

`packslip create` refuses a `release.toml` that lists an artifact `path`
twice or has an unknown key at the top level or in `[source]`,
`[[artifact]]`, or `[[resource]]`. Inside `requires` or a `bin` table it
ignores keys it does not know, so a misspelled key such as `glibc_mn` is
silently dropped; spell those keys as shown here.

See [`packslip create`](/cli/create/) for every flag. The same keys are
defined in [src/manifest.rs](https://github.com/jdx/packslip/blob/main/src/manifest.rs).

### Paths in the input and output

| Field | Interpreted relative to | Example |
| --- | --- | --- |
| Artifact `path` | The directory where `packslip create` runs, not the directory that holds `release.toml`. | `dist/mytool-linux-x64.tar.gz` |
| Resource `asset` (TOML or `asset:` in `--resource`) | The same directory. | `dist/mytool.cdx.json` |
| Executable path or resource `archive` | The archive's root as stored, so include any top-level directory (`tar -tzf` shows these paths). | `mytool-1.2.3/bin/mytool` |
| Resource `repo` | The source repository at `source.commit`. | `skills/mytool` |
| Resource `artifact` | An exact artifact file name in the statement, with no local directory. | `mytool-linux-x64.tar.gz` |

In the output, an asset's local path becomes its file name in `subject`.
Its download URL is `url_base/FILENAME` unless the entry sets `url` or you
pass `--url FILENAME=URL`. `packslip create` checks executable paths
against each archive it can open, but it records resource `archive` paths
without opening the archive, and `packslip verify` does not check them
either. Copy them from the archive listing, and do not put `dist/` in an
`archive` path unless that directory is inside the archive.

### How `release.toml` and flags combine {#defaults-and-overrides}

When you pass both, `packslip create` combines them as follows:

- Top-level `bin` and `requires` are defaults for every artifact,
  including artifacts given on the command line. An artifact's own `bin`
  or `requires` replaces the default as a whole; it is not merged field by
  field. `bin = []` declares that an artifact has no executables.
- `--bin` adds to the top-level `bin` rather than replacing it, so name
  each executable in one place only.
- `--require` adds its commands to every artifact that has executables, on
  top of any `requires` in `release.toml`.
- Command-line artifacts are added to the ones `release.toml` lists. When
  one has the same file name as an `[[artifact]]` entry, the entry is used
  and the argument is ignored, including any `:platform` or `@variant`
  suffix.
- `--project`, `--version`, `--url-base`, `--notes-url`, `--published-at`,
  `--source-repo`, `--commit`, and `--tag` replace the values in
  `release.toml`. `--extension NAME=JSON` replaces the whole value for
  `NAME` and keeps the other names.
- `--url FILENAME=URL` and `--format FILENAME=FORMAT` apply only where the
  entry sets no `url` or `format`, and `--provenance` URLs are added to the
  entry's own `provenance` list.

## Add custom metadata

Put data the specification has no field for under `extensions`, keyed by
who defines it: a consumer by its name (`mise`), or you by a domain you
control.

```sh
packslip create ... \
  --extension 'example.com={"build_id":"20260901.3"}'
```

`--extension` sets release-level extensions only. In `release.toml`, the
same data goes in an `[extensions."example.com"]` table:

```toml
[extensions."example.com"]
build_id = "20260901.3"
```

An `[[artifact]]` or `[[resource]]` entry carries its own in an
`[artifact.extensions."example.com"]` or
`[resource.extensions."example.com"]` table placed after that entry.

An `--extension NAME=JSON` replaces the whole value `release.toml` gives
for `NAME`. Consumers read the keys they define and ignore the rest, and
nothing under `extensions` can collide with a field a later revision of
the specification adds. See [Extensions](/release/v1/#extensions).

## Describe another vendor's release

If you sign for artifacts another vendor publishes, pass
`--attested-by repackager` and one `--evidence KIND[=DETAIL]` per check,
such as `--evidence vendor-signature`. These flags have no `release.toml`
equivalent. The document proves that you signed these digests and says
what you checked; it proves nothing on the vendor's behalf. It is a
claim by your signer, which consumers must explicitly trust. Consumers
rank it below a vendor packslip and do not replace a vendor's document
with it without a person's approval. See
[Repackager attestation](/release/v1/#repackager-attestation) for the
documented evidence kinds.

## Next steps

- [Resources](/docs/resources/): completions, man pages, CLI
  specifications, skills, SBOMs, and desktop files, and how to
  [scope a resource to the right artifact](/docs/resources/#scope-resources-to-the-right-artifact).
- [Host requirements](/docs/host-requirements/): shared-library scanning,
  required commands, and minimum OS and glibc versions.
- [Release recipes](/docs/recipes/): complete `release.toml` files for
  Rust, Go, monorepo, and desktop layouts.
