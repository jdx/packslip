---
title: Artifact configuration
weight: 30
description: Configure artifact platforms, variants, executable paths, and per-file metadata.
---
# Artifact configuration

A packslip describes the files you already build. Start with artifact
paths and executable names, then add explicit metadata wherever your
filenames or archive layouts leave room for ambiguity.

## Choose flags or a manifest

Use CLI flags when all artifacts share executable names and resource layouts.
Use `release.toml` when individual files need different paths, platforms,
requirements, or formats. Both produce the same signed release statement;
TOML is configuration for the generator, not a second interchange format.

## Select platforms and variants

`create` infers OS, architecture, libc, and format from artifact names.
Inspect the result with `packslip show` before publishing. Use these
argument forms to override platform inference:

| Artifact argument | Meaning |
| --- | --- |
| `dist/mytool-linux-x64.tar.gz` | Infer the platform from the filename. |
| `dist/mytool.tar.gz:linux/x86_64/gnu` | Set OS, architecture, and libc explicitly. |
| `dist/mytool.tar.gz:any` | Clear platform fields for a portable artifact. |
| `dist/mytool-fips-linux-x64.tar.gz@fips` | Selectable `fips` variant. |
| `dist/mytool.tar.gz:linux/x86_64/gnu@fips` | Explicit platform and variant together. |

Use `darwin` for macOS, `x86_64` for x64, and `aarch64` for arm64 in
explicit metadata. See the complete [vocabularies](/release/v1/#vocabularies).

Two artifacts may share a platform if their formats differ and they carry
the same build. Distinct builds for the same platform need a `variant`.
Consumers consider only artifacts without a variant unless one is requested.
If two artifacts have the same OS, architecture, libc, variant, and
format, creation fails.

An absent platform field means no restriction on that dimension. A
universal macOS binary has `os = "darwin"` with no `arch`; it does not run
on every OS. Use `portable = true` only when all platform fields should
be absent.

### Check inferred metadata

Inference reads filenames, not your build configuration. An ambiguous name
can leave a platform field unrestricted or describe the wrong target.
For Linux artifacts, the generator defaults libc to `gnu` unless overridden
or declared portable. Inspect the generated statement and set explicit
values for anything the filename does not establish.

`os`, `arch`, and `libc` describe where a build runs. `variant` distinguishes
builds a user chooses, such as `fips` or `debug`. A minimum OS or glibc version
belongs in `requires`, checked after selection; it cannot break an artifact tie.

## Name the executables

For `--bin mytool`, packslip searches each archive and records the actual
path, such as `mytool-1.2.3/bin/mytool`. An explicit path is relative to the
archive root, including any top-level directory.

Use `--bin mytool=bin/mytool-x86_64` when the command name differs from the
file's name. In TOML, write the equivalent as:

```toml
bin = [{ name = "mytool", path = "bin/mytool-x86_64" }]
```

For a bare executable, `--bin mytool` gives its installed command name.
On Windows, command names omit `.exe`; the file path retains it. An
ambiguous `.exe` can be declared `format = "raw"` in a manifest when it
is the program itself rather than an installer.

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
format = "raw"

[[resource]]
kind = "man"
artifact = "mytool-1.2.3-linux-x64.tar.gz"
archive = "mytool-1.2.3/share/man/man1/mytool.1"
```

In a CI job with a supported OIDC identity:

```sh
packslip create --manifest release.toml --out dist
```

For local key signing, add `--key release.key`. In the action, set
`manifest: release.toml` and still supply the required `artifacts` input.

### Paths in the input and output

| Field | Interpreted relative to | Example |
| --- | --- | --- |
| Artifact `path` | The working directory running `create`. | `dist/mytool-linux-x64.tar.gz` |
| Resource `asset` in TOML | The same working directory. | `dist/mytool.cdx.json` |
| Executable path or resource `archive` | The true archive root, retaining its top-level directory. | `mytool-1.2.3/bin/mytool` |
| Resource `repo` | The source repository at `source.commit`. | `skills/mytool` |
| Resource `artifact` | An exact artifact filename in the statement, with no local directory. | `mytool-linux-x64.tar.gz` |

In the output, an asset's local path becomes its filename in `subject`.
Its URL tells the consumer where to download it. Do not put `dist/` in an
`archive` path unless that directory is actually inside the archive.

### Defaults and overrides

Configuration follows these rules:

- Local artifact and asset paths are relative to the **working directory**,
  not the manifest's directory.
- Top-level `bin` and `requires` supply defaults. An artifact's own value
  replaces the corresponding default; it is not a field-by-field merge.
  `bin = []` explicitly declares no executables.
- Command-line artifacts join the manifest's artifacts. For a path in both,
  the manifest's artifact entry takes precedence.
- CLI flags override release metadata such as `project`, `version`,
  `url_base`, and source fields. Extension overrides apply key by key.

See [create](/cli/create/) for all flags and the
[manifest types](https://github.com/jdx/packslip/blob/main/src/manifest.rs)
for the full TOML input structure.

## Add extensions or repackager evidence

Put custom metadata in a namespace you define:

```sh
--extension 'example.com={"build_id":"20260901.3"}'
```

Consumers ignore extension namespaces they do not understand. Avoid
inventing top-level fields that could conflict with future spec fields.

If you describe another vendor's artifacts, declare
`--attested-by repackager` and the checks you performed, for example
`--evidence vendor-signature`. This is a claim by your signer; consumers
must explicitly trust it. See [repackager attestation](/release/v1/#repackager-attestation).

## Add resources

See [Resources](/docs/resources/) for completions, man pages, CLI specs,
skills, SBOMs, and desktop files.

### Completions and CLI specifications

[Declare static or generated completions and CLI specifications](/docs/resources/#completions-and-cli-specifications).

### Agent skills and desktop files

[Ship versioned skills and desktop integration files](/docs/resources/#agent-skills-and-desktop-files).

### Scope resources to the right artifact

[Match each resource to its archive, platform, or variant](/docs/resources/#scope-resources-to-the-right-artifact).

## Declare host requirements

See [Host requirements](/docs/host-requirements/) for shared-library
scanning, required commands, and minimum OS versions.
