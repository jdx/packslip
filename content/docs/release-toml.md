---
title: release.toml reference
description: Every release.toml key, how the file combines with flags, and how packslip create reads platforms from file names.
group: reference
weight: 82
---
# release.toml reference

For publishers writing the release.toml that `packslip create --manifest`
(`-m`) reads: every key, its flag, and what `create` does when a key is
absent. To write your first one, follow
[Describe your artifacts](/docs/describing-releases/).

release.toml only configures `packslip create`. It is neither signed nor
published, and consumers never read it: flags and release.toml produce
the same signed release statement.

## Layout

Top-level keys come first, then the tables. A table header such as
`[artifact.extensions."example.com"]` belongs to the `[[artifact]]` entry
above it, so place it right after that entry's keys.

```toml
project = "github.com/owner/mytool"
version = "1.2.3"
url_base = "https://github.com/owner/mytool/releases/download/v1.2.3"
bin = ["mytool"]

[source]
repo = "https://github.com/owner/mytool"
tag = "v1.2.3"

[extensions."example.com"]
build_id = "20260901.3"

[[artifact]]
path = "dist/mytool-1.2.3-linux-x64.tar.gz"
requires = { glibc_min = "2.31" }

[artifact.extensions."example.com"]
runner = "ubuntu-22.04"

[[artifact]]
path = "dist/mytool-1.2.3-darwin-arm64.tar.gz"

[[resource]]
kind = "man"
artifact = "mytool-1.2.3-linux-x64.tar.gz"
archive = "mytool-1.2.3/share/man/man1/mytool.1"
```

## Keys in release.toml

Every flag is listed in [`packslip create`](/cli/create/).

### Top-level keys

| Key | Flag | Meaning |
| --- | --- | --- |
| `project` | `--project` | The [project name](/release/v1/#project-names), such as `github.com/owner/mytool`. Required here or as the flag. |
| `version` | `--version` | The semver version, such as `1.2.3`. Required here or as the flag. |
| `url_base` | `--url-base` | Download URL prefix. Each artifact and asset that sets no `url` gets `url_base/FILENAME`. |
| `notes_url` | `--notes-url` | URL of the release notes. |
| `published_at` | `--published-at` | RFC 3339 publish time, recorded in UTC (`2026-09-01T14:00:00+02:00` becomes `2026-09-01T12:00:00Z`). Defaults to now. |
| `bin` | `--bin` | Executables inside every artifact that lists none of its own: each a path, a plain name, or `{ name = "mytool", path = "bin/mytool-x86_64" }`. See [Name the executables](/docs/describing-releases/#name-the-executables). |
| `requires` | `--require` (commands only) | Default host requirements for every artifact that states none of its own; see [Requirement keys](#requirement-keys). |
| `extensions` | `--extension` | Release-level extensions, as `[extensions."NAME"]` tables. |

### The `[source]` table

| Key | Flag | Meaning |
| --- | --- | --- |
| `repo` | `--source-repo` | Source repository URL, such as `https://github.com/owner/mytool`. Required when the table is present. |
| `commit` | `--commit` | The commit the release was built from. A `repo` resource requires it. |
| `tag` | `--tag` | The release tag, such as `v1.2.3`. For a GitHub project, `create` warns when the tag does not name the version; see [Tags](/release/v1/#tags). |

`--commit` and `--tag` need a repository from `--source-repo` or this
table.

### `[[artifact]]` tables

| Key | Flag | Meaning |
| --- | --- | --- |
| `path` | The artifact argument | The local file. Required. |
| `os`, `arch`, `libc` | `PATH:OS/ARCH[/LIBC]` | The platform, when the file name gives the wrong one or none. |
| `portable` | `PATH:any` | `true` for a build that runs everywhere: leaves out `os`, `arch`, and `libc`, whatever the file name says. |
| `variant` | `PATH@VARIANT` | Marks a distinct build that shares a platform with another artifact, such as `fips`. |
| `format` | `--format FILENAME=FORMAT` | The format, when the file name does not give the right one: a [vocabulary](/release/v1/#vocabularies) format, `raw` for a bare executable, or a type of your own, which only a consumer that handles it selects. |
| `url` | `--url FILENAME=URL` | The download URL, when it is not `url_base/FILENAME`. |
| `bin` | None | This artifact's executables, replacing the top-level `bin`. `bin = []` declares none. |
| `requires` | None | This artifact's host requirements, replacing the top-level `requires`. |
| `provenance` | `--provenance FILENAME=URL` | URLs of build provenance statements for this artifact. |
| `extensions` | None | Artifact-level extensions, as an `[artifact.extensions."NAME"]` table. |

Each explicit `os`, `arch`, `libc`, `variant`, and `format` value is a
lowercase word: a letter or digit, then letters, digits, `.`, `_`, or `-`.
Platform values follow these rules:

- Explicit values are recorded exactly as written; only file names are
  translated. Write the [vocabulary](/release/v1/#vocabularies) spellings:
  `darwin` (not `macos`), `x86_64` (not `amd64`), and `aarch64` (not
  `arm64`). `create` accepts `:linux/amd64/gnu`, but no consumer
  selects it.
- An absent platform field means the build does not depend on it. A
  universal macOS binary has `os = "darwin"` and no `arch`: it fits every
  Mac.
- `any` as `arch` or `libc` leaves that field out. `os` has no `any`:
  for a build that runs on every OS, set `portable = true`. `os = "any"`
  would be recorded as written and match no host.

[Override the inferred platform](/docs/describing-releases/#override-the-inferred-platform)
shows the platform keys beside their argument forms.

### Artifact argument suffixes

An artifact argument takes the suffixes in the Flag column above, with
`@VARIANT` after the path or after a `:` suffix.

- A suffix is read only when it is well formed. Anything
  else stays part of the path, so colons in a timestamped directory name
  are safe.
- Each part of a suffix starts with a letter and holds only letters,
  digits, `_`, and `-`, such as `fips` or `install_only`.
- An `@VARIANT` suffix never contains a dot, so `scoped@pkg-1.0.tgz` stays
  a file name. The `variant` key accepts dots and a leading digit.

### `[[resource]]` tables

Each `[[resource]]` table needs a `kind` and exactly one source key:
`archive`, `asset`, `repo`, or `exec`.

| Key | In `--resource` | Meaning |
| --- | --- | --- |
| `kind` | `KIND` | `completion`, `man`, `cli-spec`, `skill`, `sbom`, `desktop`, `icon`, `app`, or a kind of your own. Required. |
| `archive` | `archive:PATH` | A path inside the artifact. |
| `asset` | `asset:PATH` | A separate release file, by local path, digested into `subject` under its file name. |
| `repo` | `repo:PATH` | A path in the source repository. |
| `exec` | `exec:COMMAND` | A command whose standard output is the file, as a list such as `["mytool", "completion", "{shell}"]`. |
| `env` | Leading `NAME=value` words in the `exec:` value | Environment variables for an `exec` command. |
| `artifact` | None | Limits the entry to one artifact, by its exact file name. |
| `os`, `arch`, `libc` | `@OS[/ARCH[/LIBC]]` | Limits the entry to artifacts of that platform. |
| `shell`, `shells`, `bin`, `format`, `name` | The qualifiers after `KIND` | `shell` is `completion/SHELL`; `shells` is `completion/SHELL,SHELL`, every shell an `exec` command generates; `bin` is `/BIN`, the executable the entry is for; `format` is `cli-spec/FORMAT` or `sbom/FORMAT`; `name` is `skill/NAME`, or `KIND/NAME` for a kind of your own. |
| `url` | None; use `--url FILENAME=URL` | An asset's download URL, when it is not `url_base/FILENAME`. |
| `extensions` | None | Resource-level extensions, as a `[resource.extensions."NAME"]` table. |

[Write a resource entry](/docs/resources/#write-a-resource-entry)
explains each kind and which qualifiers it takes.

### Requirement keys

A `requires` table, top-level or per artifact, takes these keys;
[Declare host requirements](/docs/host-requirements/) explains each one.

| Key | Meaning |
| --- | --- |
| `os_min` | Minimum OS version in the OS's own numbering, such as `"12"` for macOS or `"10.0.17763"` for Windows. |
| `glibc_min` | Minimum glibc for a `gnu` Linux build, such as `"2.31"`. |
| `libs` | Shared libraries the executables load from the host. `create` reads them; write them only for an artifact it cannot read. |
| `bin` | Commands the executables need on PATH, such as `[{ name = "java", min = "17" }]`. |

### Misspelled and repeated keys

A release.toml is refused when it:

- has an unknown key at the top level or in `[source]`, `[[artifact]]`,
  or `[[resource]]`, with ``unknown field `bins`, expected one of …``;
- lists the same artifact `path` twice, with
  `artifact "dist/mytool.tar.gz" is listed twice`.

**Warning:** Inside a `requires` table or a `{ name, path }` entry of
`bin`, `packslip create` ignores keys it does not know, so a misspelling
such as `glibc_mn` is dropped without an error.

### Flags with no key

Pass these next to `--manifest`: `--key`, `--sign`, `--no-log`, `--out`,
`--attested-by`, `--evidence`, `--no-sha512`, `--no-libs`, and
`--no-pin-workflow`.

## How paths are resolved

| Field | Relative to | Example |
| --- | --- | --- |
| Artifact `path` | The directory where `packslip create` runs, not the directory that holds release.toml. | `dist/mytool-linux-x64.tar.gz` |
| Resource `asset` (TOML, or `asset:` in `--resource`) | The same directory. | `dist/mytool.cdx.json` |
| Executable path or resource `archive` | The archive's root as stored, so it includes any top-level directory (as `tar -tzf` lists them, minus any leading `./`). | `mytool-1.2.3/bin/mytool` |
| Resource `repo` | The source repository at `source.commit`. | `skills/mytool` |
| Resource `artifact` | An exact artifact file name in the statement, with no local directory. | `mytool-linux-x64.tar.gz` |

## How release.toml and flags combine

- `--project`, `--version`, `--url-base`, `--notes-url`, `--published-at`,
  `--source-repo`, `--commit`, and `--tag` replace the values in
  release.toml.
- Top-level `bin` and `requires` are defaults for every artifact,
  including command-line artifacts. An artifact's own `bin`
  or `requires` replaces the default as a whole, not field by field.
- `--bin` adds to the top-level `bin`; naming an executable in both
  records it twice.
- `--require` adds its commands to every artifact that has executables,
  on top of release.toml's `requires`.
- Command-line artifacts join the ones release.toml lists. When
  one has the same file name as an `[[artifact]]` entry, the entry is used
  and the argument is ignored, including any `:platform` or `@variant`
  suffix.
- `--extension NAME=JSON` replaces release.toml's whole value for `NAME`,
  keeping the other names.
- `--url FILENAME=URL` applies only where the entry for that file sets no
  `url`, and `--format FILENAME=FORMAT` only where the `[[artifact]]` entry
  sets no `format`.
- `--provenance` URLs are added to the entry's own `provenance` list, and
  `--resource` entries to the `[[resource]]` entries.

Under the GitHub Action, its inputs replace `project`, `version`,
`url_base`, `notes_url`, and `[source]`; see
[How inputs reach packslip create](/docs/actions/#how-inputs-reach-packslip-create).

## How `packslip create` reads file names

`create` reads the OS, architecture, and format from the file name,
ignoring case, for each one that release.toml, an argument suffix, or
`--format` does not set. A portable artifact still takes its format from
the name.

### Operating system

The first row that matches wins.

| In the file name | Recorded as |
| --- | --- |
| `android` or `androideabi` as the word right after `linux` (`aarch64-linux-android`), or as a word anywhere in a name without `linux` | `android` |
| `ios` as the word right after `apple` (`aarch64-apple-ios`), or as a word anywhere in a name without `apple`, `darwin`, or `macos` | `ios` |
| `linux`, or a `.deb`, `.rpm`, or `.AppImage` file | `linux` |
| `darwin`, `macos`, `apple`, or a `.dmg` or `.pkg` file | `darwin` |
| `windows`, `win32`, `win64`, or a `.exe`, `.exe.gz`, `.exe.zip`, `.msi`, or `.msix` file | `windows` |
| `freebsd`, `netbsd`, `openbsd`, `illumos` | The same word |

A name with no OS records no `os`;
[Select platforms and variants](/docs/describing-releases/#select-platforms-and-variants)
lists this and other results `create` accepts without a warning.

### Format

The format comes from the extension: `.tar.xz`, `.tar.gz`, `.tar.zst`,
`.tar.bz2`, `.tgz`, `.tar`, `.zip`, `.7z`, `.gz`, `.xz`, `.zst`, `.bz2`,
`.deb`, `.rpm`, `.dmg`, `.pkg`, `.msix`, `.msi`, `.exe`, or `.AppImage`.
[Vocabularies](/release/v1/#vocabularies) says how consumers install each
one.

- A `.exe` is the program itself (`raw`) unless its name contains `setup`
  or `install`, which makes it an `exe` installer.
- A file with no extension is a `raw` executable when its OS is known or
  the artifact is portable.
- A dotted version is not an extension: `mytool-1.2.3-linux-x64` has none.
- Any other file whose name gives no format, such as a `.jar`, is
  refused until you set `format` or `--format FILENAME=FORMAT`.

### Architecture

| In the file name | Recorded as |
| --- | --- |
| `x86_64`, `x86-64`, `x64`, `amd64`, or `win64` when nothing else names one | `x86_64` |
| `aarch64`, `arm64` | `aarch64` |
| `armv7`, `armhf` | `armv7` |
| `armv6`, or a plain `arm` (the goreleaser and Rust default) | `armv6` |
| `i686`, `i386`, `x86`, `386`, `ia32` | `i686` |
| `ppc64le`, `ppc64el`, `powerpc64le` | `powerpc64le` |
| `loongarch64`, `loong64` | `loongarch64` |
| `mipsel`, `mipsle` | `mipsel` |
| `mips64el`, `mips64le` | `mips64el` |
| `ppc64`, `powerpc64` | `powerpc64` |
| `riscv64`, `s390x`, `mips`, `mips64` | The same word |

- `386`, `ia32`, a plain `arm`, the MIPS spellings, `ppc64`, and
  `powerpc64` count only as separate words. A number inside a dotted
  version, as in `mytool-1.386.0`, is not an architecture.
- The MIPS and `powerpc64` values follow Rust's target spellings and are
  outside the [vocabularies](/release/v1/#vocabularies).

An artifact for an OS other than macOS or Windows is refused when its
name gives no architecture, since it would fit every host of that OS. Name one, or set `any` (`PATH:linux/any`, `arch = "any"`) for a
build that runs on every architecture. `.noarch.rpm` and `_all.deb`
packages are exempt. A macOS or Windows artifact without one records no
`arch`.

### C library

The C library is the first of these that applies:

1. The explicit value: `libc`, or the third part of `PATH:OS/ARCH/LIBC`.
   `any` leaves the field out; set it for a Linux build that loads no C
   library, such as an archive of shell scripts.
2. No `libc` for an artifact whose OS is not Linux.
3. The file name: `musl` records `musl`; a word that is `gnu` or starts
   with `gnueabi`, `gnuabi`, `gnux32`, or `glibc` records `gnu`.
4. The listed executables, when every one is an ELF binary. A statically
   linked build, such as a Go binary built with `CGO_ENABLED=0`, records
   no `libc` and fits both glibc and musl hosts. A build that uses musl's
   loader records `musl`. Any other records `gnu`.
5. `gnu`, which does not fit a musl host. This happens when no
   executables are listed, an executable is a script, the format cannot be
   opened, or `--no-libs` is set.
