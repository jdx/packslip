---
title: Ship resources
description: Declare the completions, man pages, CLI specifications, agent skills, SBOMs, desktop files, and icons that ship with each release of your tool.
group: publish
weight: 36
---
# Ship resources

For publishers: declare the extra files each release ships, such as
completions, man pages, skills, SBOMs, and desktop files, so consumers can
install the ones that match the version a user runs. To install a tool, see
[Bootstrap a tool](/docs/bootstrap/); to install resources from your own
installer, see [Build an installer or mirror](/docs/installers/).

Declare only the resources the release provides. Consumers choose which
resource kinds they support.

## Write a resource entry

Declare each resource entry as one of these:

- A `--resource` flag on `packslip create`. Quote it, since values can
  hold spaces and braces.
- A line of the GitHub action's `resources` input; see
  [Add resources and requirements](/docs/publishing/#add-resources-and-requirements)
  and [Publish action inputs](/docs/actions/#publish-action-inputs).
- A `[[resource]]` table in release.toml, passed with `--manifest` or the
  action's `manifest` input; see
  [Use release.toml](/docs/describing-releases/#use-a-toml-manifest).

A flag value or action line has this form:

```text
KIND[/QUALIFIERS][@SCOPE]=SOURCE:VALUE
```

- `KIND`: what the file is (see the table).
- `QUALIFIERS`: narrow the kind, separated by `/`. `BIN` names one of the
  release's executables; see [Name the executable](#name-the-executable).
- `SCOPE`: one platform; see
  [Scope resources to the right artifact](#scope-resources-to-the-right-artifact).
- `SOURCE`: `archive`, `asset`, `repo`, or `exec`; see
  [Choose a source](#choose-a-source).
- `VALUE`: a path, or the command whose standard output is the file.

| Kind | Flag form | Notes |
| --- | --- | --- |
| `completion` | `completion/SHELL[/BIN]`, or `completion/SHELL,SHELL[/BIN]` with `exec` | See [Write an exec command](#write-an-exec-command). |
| `man` | `man[/BIN]` | The file's suffix gives the man section, as in `mytool.1`. |
| `cli-spec` | `cli-spec/FORMAT[/BIN]` | The documented format is `usage`. |
| `skill` | `skill/NAME` | A directory that holds `SKILL.md`; from an `asset`, an archive of that directory. |
| `sbom` | `sbom/FORMAT` | `cyclonedx` or `spdx`. |
| `desktop` | `desktop` | A freedesktop desktop entry. |
| `icon` | `icon` | An icon file; see [Ship desktop files and icons](#ship-desktop-files-and-icons). |
| `app` | `app` | A macOS application (`.app`) inside a `dmg` or `zip`. `archive` source only. |
| A kind of your own, such as `font` | `KIND` or `KIND/NAME` | Consumers ignore kinds they do not know. |

`completion/zsh=archive:share/zsh/site-functions/_mytool` declares a zsh
completion script inside the archive. In release.toml the same entry is a
`[[resource]]` table, with a field for each qualifier (`shell` or `shells`,
`bin`, `format`, `name`) and one for the source:

```toml
[[resource]]
kind = "completion"
shell = "zsh"
archive = "share/zsh/site-functions/_mytool"
```

[Resource kinds](/release/v1/#resource-kinds) defines each kind for
consumers, [`[[resource]]` tables](/docs/release-toml/#resource-tables)
lists every field, and [Release recipes](/docs/recipes/) shows complete
configurations.

## Choose a source

Each resource has exactly one source, which says where the content is and
what pins it:

- `archive`: a path inside the artifact, pinned by the artifact's digest.
  The path starts at the archive's actual root, including any top-level
  directory such as `mytool-1.2.3/`.
- `asset`: a separate release file, pinned by its own digest in the
  statement.
- `repo`: a path in the source repository, pinned by `source.commit`.
  It needs `--source-repo` and `--commit` (the full commit SHA), or `repo`
  and `commit` in the `[source]` table of release.toml. The GitHub action
  passes both flags.
- `exec`: the standard output of a release executable, pinned by the
  verified executable. Its output is not separately signed, and the
  consumer's execution policy applies.

`archive` and `repo` paths are relative, with no empty or `..` segments.

An `asset` value is a local file in the signing job. `packslip create`
digests it and records only its file name, such as `mytool.cdx.json`.
Then:

1. Set its download URL with `--url mytool.cdx.json=URL` (the file name,
   without `dist/`), the entry's `url` in release.toml, or `--url-base`
   (`url_base`) for `URL_BASE/FILENAME`. The GitHub action sets
   `--url-base` to the release's download URL.
2. Upload the file to that URL yourself. The GitHub action uploads only
   the release bundle.

A file matched both as an artifact, for example by `dist/*`, and as an
asset is recorded once, as the asset, not as an installable artifact.

## Ship completions and CLI specifications

These entries ship completions the tool generates and a usage spec from
the archive:

```text
completion/bash,zsh,fish=exec:mytool completion {shell}
cli-spec/usage/mytool=archive:share/usage/mytool.kdl
```

- The first has consumers run `mytool completion {shell}`, with `{shell}`
  replaced by the requested shell.
- The second ships a [usage](https://usage.jdx.dev) spec for `mytool`,
  from which consumers can generate completions, man pages, and docs. A
  spec can also come from the tool: `cli-spec/usage/mytool=exec:mytool usage`.

Declare either or both. For completions, consumers use a static script
first, then a spec, then `exec`;
[Fallback and verification](/release/v1/#fallback-and-verification)
gives the full order.

A consumer that generates completions from a spec needs usage's completion
engine each time the shell completes, so it installs `usage` beside the
tool. The engine is built into mise; see
[Keep completions aligned with the active version](/docs/mise/#keep-completions-aligned-with-the-active-version).
If users may install the tool without mise or `usage`, also ship static
completion scripts.

### Name the executable

Every `cli-spec` names the executable it describes. The flag may leave
the name out (`cli-spec/usage=...`) only when exactly one executable is
given with `--bin` (the action's `bin` input) or the top-level `bin` of
release.toml. A release.toml `cli-spec` table always sets `bin`.

A `completion` or `man` entry must name its executable when the release
has more than one, or `packslip create` refuses it:

```text
completion/zsh/mytool=archive:share/zsh/site-functions/_mytool
man/mytool=archive:share/man/man1/mytool.1
```

### Write an exec command

An `exec` command's first word after any `NAME=value` words must be one
of the release's `bin` names, not another program such as `sh`.

The CLI splits an `exec` value on whitespace, without interpreting shell
quoting, pipes, or redirections. Leading `NAME=value` words become
environment variables, as at a shell prompt. This entry runs `mytool`
with `COMPLETE` set to each shell:

```text
completion/bash,zsh,fish/mytool=exec:COMPLETE={shell} mytool
```

In release.toml, `exec` is an array, so an argument may contain spaces.
The equivalent table:

```toml
[[resource]]
kind = "completion"
bin = "mytool"
shells = ["bash", "zsh", "fish"]
exec = ["mytool"]
env = { COMPLETE = "{shell}" }
```

An `exec` completion must list its shells and put `{shell}` in its command
or `env`, so `packslip create` refuses `completion/zsh=exec:...`.

Consumers generate exec completions on demand and cache them. Other exec
resources run during installation only if the user allows vendor code to
run then; see [Running an exec entry](/release/v1/#running-an-exec-entry).

## Ship man pages and SBOMs

These entries ship a man page from the archive and a CycloneDX SBOM as a
separate release file:

```text
man=archive:share/man/man1/mytool.1
sbom/cyclonedx=asset:dist/mytool.cdx.json
```

Use `sbom/spdx` for SPDX. An SBOM cannot come from `exec`, so a digest or
commit always covers it.

For one SBOM per platform or per artifact, scope each entry, as in
`sbom/cyclonedx@linux/x86_64=...`; see
[Scope resources to the right artifact](#scope-resources-to-the-right-artifact).

## Ship an agent skill

`skill/mytool=repo:skills/mytool` points at a directory in the source
repository. At `source.commit`, that directory must contain `SKILL.md`
and every file it links by relative path.

To ship the skill as a separate archive, use
`skill/mytool=asset:dist/mytool-skill.tar.gz`. Put `SKILL.md` at the
archive's root, or in one top-level directory that is the only entry at
the root.

Prefer either form over `exec`, so installing the skill does not run the
tool.

## Ship desktop files and icons

On macOS, an `app` entry points to the application (`.app`) inside the
artifact. On Linux, a `desktop` entry pairs with the application's icons.

The hicolor icon theme keeps each size in its own directory under the
same file name. In a `hicolor/SIZE/CONTEXT/FILE` path, consumers count
`SIZE` (such as `48x48`, `48x48@2`, or `scalable`) in the icon's
identity. List one `icon` entry per size; a consumer that installs icons
installs them all:

```text
desktop@linux=archive:share/applications/mytool.desktop
icon@linux=archive:share/icons/hicolor/48x48/apps/mytool.png
icon@linux=archive:share/icons/hicolor/256x256/apps/mytool.png
icon@linux=archive:share/icons/hicolor/scalable/apps/mytool.svg
app@darwin=archive:MyTool.app
```

Icons outside a hicolor size directory, such as
`share/pixmaps/mytool.png`, are told apart by file name alone, so a
consumer installs only one of several that share a name.

The recipe [Desktop application for Linux and macOS](/docs/recipes/#desktop-application-for-linux-and-macos)
shows a complete desktop release, and
[Desktop integration](/release/v1/#desktop-integration) gives the full rules.

## Scope resources to the right artifact

An entry without a scope applies to every artifact, so do not declare an
unscoped `archive` path that exists in only some artifacts. When archive
layouts differ by platform, add a scope to the kind with `@`:

```text
man@linux=archive:share/man/man1/mytool.1
completion/zsh@darwin/aarch64=archive:share/zsh/site-functions/_mytool
```

The scope is `os`, `os/arch`, or `os/arch/libc`. Unlike an artifact's
`PATH:OS/ARCH` suffix, the `os` alone is enough: `man@linux` applies to
every Linux build. An `@` before the `=` always starts the scope, so an
`@` inside an `exec` argument or a path is left alone. In release.toml,
set `os`, `arch`, or `libc` on the entry.

Spell scope values as the [vocabularies](/release/v1/#vocabularies) do,
such as `darwin` and `aarch64`, not `macos` or `arm64`. `packslip create`
accepts a scope that matches no artifact, but consumers never use that
entry.

An `archive` entry never applies to a bare executable (format `raw`,
`gz`, `xz`, `zst`, or `bz2`), whatever its scope. To give a bare
executable the same resource, add an `asset` or `repo` entry.

When archives for one platform have different layouts, or a resource
belongs to one variant, scope the entry to one exact artifact. Only
release.toml can, with the `artifact` field of a `[[resource]]` table:

```toml
[[resource]]
kind = "man"
artifact = "mytool-1.2.3-linux-x64.tar.gz"
archive = "mytool-1.2.3/share/man/man1/mytool.1"
```

The entry then belongs only to that file. The named artifact must be in
the release and match any platform scope on the entry.

## Offer fallback sources

Entries compete only when they share a
[resource identity](/release/v1/#resource-identity). Different
executables, shells, skill names, CLI-spec and SBOM formats, and hicolor
icon sizes are different resources. So are `man`, `desktop`, `icon`, and
other entries whose source files have different names.

For a second place to get a resource, declare another entry with the same
identity. Consumers try entries of one source in statement
order and stop at the first usable one. Entries of different sources
follow the order in
[Fallback and verification](/release/v1/#fallback-and-verification).
These entries try the older `.agents/skills/mytool` only if
`skills/mytool` is unavailable:

```text
skill/mytool=repo:skills/mytool
skill/mytool=repo:.agents/skills/mytool
```

Entries from release.toml come before those from `--resource` flags or
the action's `resources` input, so declare all alternatives in one place
to control their order.

Give alternatives the same scope. A more specific entry removes every
less specific one for the same resource before any source is tried. An
unscoped entry therefore never backs up a scoped one whose path is wrong.

If no entry for a resource is usable, consumers report it and install
the executables without it. A digest or commit mismatch fails the
installation and never falls through to another entry.
[Selecting resources](/release/v1/#selecting-resources) and
[consumer rule 8](/release/v1/#consumer-rules) give the full rules.

## Check entries before you publish

`packslip create` checks that each entry is well formed, digests every
asset, and confirms that `exec` commands and `BIN` qualifiers name release
executables. It also finds each executable (`--bin`, or `bin` in
release.toml) in the archives it can read. Neither `packslip create` nor
`packslip verify` checks `archive` or `repo` paths or runs `exec` commands.

**Warning:** A mistyped path is signed as written, and consumers then
install without that resource. Before you publish:

1. List each archive, and confirm every `archive` path in every artifact
   it applies to. Copy paths from the listing; include `dist/` only if
   the archive itself contains that directory.

   ```sh
   tar tf dist/mytool-1.2.3-linux-x64.tar.gz
   unzip -l dist/mytool-1.2.3-windows-x64.zip
   ```

2. Confirm each `repo` path at the release commit; this exits with
   status 0 when the file exists:

   ```sh
   git cat-file -e <full-commit-sha>:skills/mytool/SKILL.md
   ```

3. Before you upload the bundle, inspect the resources it records. It is
   in the `--out` directory (the action's default is `packslip/`). In the
   GitHub action, set `upload: false`, inspect the bundle, then upload it
   yourself; see
   [Keep the action away from release write access](/docs/publishing/#keep-the-action-away-from-release-write-access).

   ```sh
   packslip show packslip/packslip.sigstore.json
   ```

## Troubleshoot resource errors

`packslip create` prints these errors, shown here with example names.

| Error | Fix |
| --- | --- |
| `--resource "cli-spec/usage=archive:share/usage/mytool.kdl": say which executable, as cli-spec/usage/BIN` | Name the executable: `cli-spec/usage/mytool=...`. |
| `cli-spec "cli-spec/usage" needs format and bin` | Set `format` and `bin` on the table. |
| `resource "man" does not say which executable it is for, and the release has several: mytool, mytool-helper` | Add `/BIN` (`man/mytool=...`), or `bin = "mytool"` in release.toml. |
| `resource "completion/zsh" runs "sh", which is not a bin name of any artifact` | Run a release executable directly; `exec` uses no shell. |
| `completion "completion/zsh" lists shells, so its source must be exec with a {shell} placeholder` | Give a static completion one `shell`. Give an `exec` completion `shells` and a `{shell}` placeholder; for one shell, write `shells = ["zsh"]` in release.toml. |
| `sbom "sbom/cyclonedx" needs a format (cyclonedx or spdx) and a static source, not exec` | Use an `archive`, `asset`, or `repo` source. |
| `resource "skill/mytool" comes from the repository, which needs source.commit` | Pass `--source-repo` and `--commit`, or set `repo` and `commit` in `[source]`. |
| `resource "man" names artifact "mytool-1.2.3-linux-x64.tar.gz", which is absent or outside its platform scope` | Use the artifact's exact file name, within the entry's `os`, `arch`, and `libc`. |
| `resource "man": archive path must be relative with no empty or .. segments` | Write the path from the archive or repository root, with no leading or trailing `/`, no `//`, and no `..`. |
| `app "app" must come from an archive` | Use the `.app` path inside the `dmg` or `zip` as an `archive` source. |
