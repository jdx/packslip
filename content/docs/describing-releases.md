---
title: Describe your artifacts
description: Record each artifact's platform, variant, executables, and download URL with packslip create flags or release.toml.
group: publish
weight: 34
---
# Describe your artifacts

Follow this guide when your build already produces release files. By
the end, your release bundle records each file's platform, executables,
and download URL, so consumers pick the right file for each host. To add
packslip to a GitHub release first, see
[Publish with GitHub Actions](/docs/publishing/).

`packslip create` builds nothing. It hashes your files and infers what
it can from common file names and archive layouts; you supply the rest.
List installable binaries and archives as artifacts, and companion files
such as completions and man pages as resources; see
[Ship resources](/docs/resources/). Every key and file-name rule is in
the [release.toml reference](/docs/release-toml/).

## Choose flags or release.toml

Flags are enough when every artifact contains the same executables. In
a CI job with a supported OIDC identity:

```sh
packslip create \
  --project github.com/owner/mytool --version 1.2.3 \
  --url-base https://github.com/owner/mytool/releases/download/v1.2.3 \
  --bin mytool --out packslip \
  dist/mytool-1.2.3-linux-x64.tar.gz \
  dist/mytool-1.2.3-darwin-arm64.tar.gz
```

- `--project` and `--version` name the release.
- `--url-base` gives each file the download URL `PREFIX/FILENAME`.
- `--bin mytool` finds `mytool` inside both archives.
- `--out` names the output directory, so the bundle is
  `packslip/packslip.sigstore.json`.

To sign with a key instead, see [Publish from other CI](/docs/other-ci/).

An artifact argument suffix sets one file's platform and variant; see
[Override the inferred platform](#override-the-inferred-platform).
`--format` and `--url` set one file's format and download URL. Use
release.toml, passed with `--manifest`, when:

- artifacts hold different executables, or the same one at different
  paths you must spell out (such as `bin/mytool-x86_64` and
  `bin/mytool-aarch64`);
- artifacts need `os_min` or `glibc_min`, or
  [host requirements](/docs/host-requirements/) that differ per artifact;
- an artifact carries its own extensions;
- a resource belongs to one exact artifact.

release.toml also keeps the release configuration in your repository.
[Use release.toml](#use-a-toml-manifest) shows a complete file.

## Select platforms and variants

`packslip create` infers OS, architecture, and format from each file
name, and libc from the name or the executables inside, never from your
build configuration.
[How packslip create reads file names](/docs/release-toml/#how-packslip-create-reads-file-names)
lists every spelling it recognizes. Before you publish, check what it
inferred:

```sh
packslip show packslip/packslip.sigstore.json
```

`packslip create` does not warn about these results:

- **No `os` on an OS-specific build.** Consumers offer the file to every OS:
  `mytool-x86_64.tar.gz` goes to any x86_64 host. Name the OS in the file
  name, or set it.
- **No `arch` on a macOS or Windows build.** Every Mac or Windows host can
  select it. By convention such a build is universal or x86_64, which both
  systems run on ARM. Set the architecture of any other build.
- **`"libc": "gnu"` on a static or musl build.** `create` assumed glibc
  because it could not read every listed executable as an ELF binary:
  none listed, any of them a script, a format it cannot open, or
  `--no-libs`. A musl host does not select the file. List the
  executables, or set `libc = "musl"`. Set `libc = "any"` for a build
  that loads no C library, such as a static binary or shell scripts.
- **An `arch` such as `mips` or `powerpc64`.** This Rust target spelling
  is outside the [vocabularies](/release/v1/#vocabularies): only a
  consumer that knows it selects the file. Correct it if the build is
  for a documented architecture.

### Override the inferred platform

Correct inference with an artifact argument suffix, or the matching
`[[artifact]]` keys in release.toml:

- `PATH:linux/x86_64/gnu` sets `os`, `arch`, and `libc`.
- `PATH:linux/x86_64/any` (`libc = "any"`) is a build that loads no C
  library from the host.
- `PATH:linux/any` (`arch = "any"`) is a Linux build for every
  architecture. Its libc is still inferred, so use `linux/any/any` when
  it needs none.
- `PATH:any` (`portable = true`) is a portable artifact: no OS,
  architecture, or libc.
- `PATH@fips` (`variant = "fips"`) marks the `fips` variant, alone or
  after a platform suffix (`PATH:linux/x86_64/gnu@fips`).

Explicit values are recorded exactly as written, so use the
[vocabulary](/release/v1/#vocabularies) spellings, such as `darwin`,
`x86_64`, and `aarch64`. A suffix that is not well formed stays part of
the path. [`[[artifact]]` tables](/docs/release-toml/#artifact-tables)
and [Artifact argument suffixes](/docs/release-toml/#artifact-argument-suffixes)
give the rules for `any`, absent fields, and suffix forms.

The GitHub Action's `artifacts` input takes only paths and globs. Set
platforms in the release.toml you pass as `manifest`. The `formats`
input sets a format by file name. The `variants` input sets a variant
the same way, except for a file release.toml lists, which takes
`variant` from its entry. See
[How inputs reach packslip create](/docs/actions/#how-inputs-reach-packslip-create).

### Separate builds that share a platform

`os`, `arch`, and `libc` describe where a build runs. `variant` marks
builds a user chooses between, such as `fips` or `debug`; see
[Platform, format, and variant](/release/v1/#platform-format-and-variant).
A minimum OS or glibc version cannot separate two builds: it belongs in
`requires`, which consumers check only after choosing an artifact.

- Leave the main build of each platform without a variant. Consumers
  consider only artifacts without one unless the user requests a variant.
- Give every other distinct build of that platform its own variant,
  whatever its format.
- Let two artifacts share a platform and variant only when their formats
  differ and they carry the same build, because consumers choose among
  them by format preference. `create` refuses two with the same format.

## Name the executables

List an archive before you write a path into it:

```sh
tar -tzf dist/mytool-1.2.3-linux-x64.tar.gz
```

```text
mytool-1.2.3/
mytool-1.2.3/bin/
mytool-1.2.3/bin/mytool
```

Repeat `--bin` for each executable. Each value takes one of three forms;
release.toml writes the first two as strings and the third as a table:

- `mytool` records the shallowest file of that name in each archive, such
  as `mytool-1.2.3/bin/mytool`.
- `mytool-1.2.3/bin/mytool`, a value containing `/`, records that path
  from the archive root, including any top-level directory. It must exist
  exactly as written.
- `mytool=bin/mytool-x86_64` records that path, exposed as the command
  `mytool` (`{ name = "mytool", path = "bin/mytool-x86_64" }` in
  release.toml).

`packslip create` searches each archive it can open: `tar`, `tar.gz`,
`tgz`, `tar.xz`, `tar.zst`, `tar.bz2`, and `zip`. In a Windows artifact,
`mytool` also finds `mytool.exe`. For a format it cannot open, such as
`7z` or an installer, the entry is recorded as written, and a Windows
path with no extension gets `.exe`.

Top-level `bin` and `--bin` apply to every artifact that lists no `bin` of
its own, and `create` fails when one lacks the executable. Give that
artifact its own `bin` in release.toml, or `bin = []` for none.

A bare executable is an artifact that is the program itself: a `raw` file,
or one compressed executable (`.gz`, `.xz`, `.zst`, `.bz2`). For one,
`--bin mytool` sets its command name on PATH. Windows command names
omit `.exe`; the file path keeps it.

A `.exe` whose name contains `setup` or `install` is read as an `exe`
installer, and any other as the program itself (`raw`); see
[Format](/docs/release-toml/#format). When the name misleads, set
`format` (or `--format FILENAME=FORMAT`) to `exe` for an installer, or to
`raw` for a program whose name happens to contain `install`, such as
`cargo-binstall-x86_64.exe`.

## Set download URLs and provenance

`--url-base PREFIX` covers every artifact and resource asset. Two flags
set one file at a time; repeat them per file:

- `--url FILENAME=URL` sets one file's download URL instead, as a `url`
  key in release.toml does.
- `--provenance FILENAME=URL` links a build provenance statement to one
  artifact. Bare URLs apply to the artifact arguments in order.

`--url`, `--format`, `--provenance`, and a `[[resource]]` `artifact` key
name a file without its directory, as in
`--url mytool-1.2.3-linux-x64.tar.gz=URL`.

**Warning:** `packslip create` does not refuse a `--provenance` name with
a directory: it reads the whole value as a bare URL and attaches it to an
artifact argument by position.

The GitHub Action links provenance itself; see its `attest` input in the
[GitHub Actions reference](/docs/actions/).

## Use release.toml {#use-a-toml-manifest}

This release.toml describes a Linux archive with a minimum glibc, a
Windows executable, and a man page that only the Linux archive ships:

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

In a CI job with a supported OIDC identity, create the bundle from it:

```sh
packslip create --manifest release.toml --out packslip
```

The Windows artifact lists no `bin`, so it takes the top-level
`bin = ["mytool"]`: the `.exe` is the program itself, installed as the
command `mytool`. [Release recipes](/docs/recipes/) has complete files for
Rust, Go, monorepo, and desktop layouts.

Local paths are relative to where you run `packslip create`; see
[How paths are resolved](/docs/release-toml/#how-paths-are-resolved).
Artifacts on the command line join the ones the file lists, and flags
combine with it as
[How release.toml and flags combine](/docs/release-toml/#how-releasetoml-and-flags-combine)
lists. In the GitHub Action, pass the file as the `manifest` input and
list the same files in `artifacts`; see
[Use release.toml with the action](/docs/publishing/#use-releasetoml-with-the-action).

## Add custom metadata

Put data the specification has no field for under `extensions`, keyed by
who defines it: a consumer by its name (`mise`), or you by a domain you
control. In release.toml, add a table:

```toml
[extensions."example.com"]
build_id = "20260901.3"
```

With flags, pass `--extension 'example.com={"build_id":"20260901.3"}'`,
which sets release-level extensions only.
An `[[artifact]]` or `[[resource]]` entry carries its own in an
`[artifact.extensions."example.com"]` or
`[resource.extensions."example.com"]` table placed after that entry.

Consumers ignore keys they do not define, and no extension can collide
with a field a later revision of the specification adds; see
[Extensions](/release/v1/#extensions).

## Describe another vendor's release

A repackager signs for files another vendor publishes: add
`--attested-by repackager` and one `--evidence KIND[=DETAIL]` per check.
Neither flag has a release.toml key;
[Repackage another vendor's release](/docs/installers/#repackage-another-vendors-release)
shows the full command.

## Troubleshoot create errors

Messages use example file names.

| Error | Fix |
| --- | --- |
| `artifact "mytool-linux.tar.gz" is for linux, but nothing in the name says which architecture` | Put the architecture in the name, or set it (`PATH:linux/x86_64`, `arch = "x86_64"`). Use `any` only for a build that runs on every architecture. `.noarch.rpm` and `_all.deb` files are exempt. |
| `artifact "mytool-data": nothing in the name says what kind of file it is` | Set `format` or `--format FILENAME=FORMAT`, or leave out a file that is not installable, such as a library, header, or checksum. |
| `artifacts "mytool-linux-x64.tar.gz" and "mytool-fips-linux-x64.tar.gz" both describe linux/x86_64/gnu/tar.gz; give one a variant` | Give one build a variant; see [Separate builds that share a platform](#separate-builds-that-share-a-platform). For a file release.toml lists, set `variant` in its entry; a suffix or the action's `variants` input does not apply. |
| `dist/mytool-linux-x64.tar.gz: several files named "mytool" inside the archive` | Give the path from the archive root; the error lists the candidates. |
| `dist/mytool-linux-x64.tar.gz: no file named "mytool" inside the archive` | Check the path against `tar -tzf`. An artifact without that executable needs its own `bin` in release.toml, or `bin = []`. |
| `dist/mytool.tar.gz:linux: No such file or directory` | A suffix needs an OS and an architecture: write `:linux/any` or `:linux/x86_64`. |
| `--url names "dist/mytool.tar.gz", which is not among the artifacts or assets` | Write the file name without its directory; `--format` fails the same way. |
