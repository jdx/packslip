---
title: Resources
weight: 32
group: publish
description: Ship completions, man pages, CLI specifications, skills, SBOMs, and desktop files with a release.
---
# Resources

Resources describe what ships alongside a release's executables: shell
completions, man pages, CLI specifications, agent skills, SBOMs, and
desktop files. Declaring them in the packslip lets any consumer install
the copy that matches the version a user actually runs.

Declare each resource with a `--resource` flag on `packslip create`, a
`[[resource]]` table in a [TOML manifest](/docs/describing-releases/#use-a-toml-manifest),
or a line in the GitHub action's [`resources` input](/docs/publishing/#add-resources-and-requirements).

## Write a resource entry

A `--resource` value, like each line of the action's `resources` input,
has this form:

```text
KIND[/QUALIFIERS][@SCOPE]=SOURCE:VALUE
```

`KIND` says what the file is. The qualifiers, separated by `/`, narrow
it down, for example to a completion's shell and executable. `SCOPE`
limits the entry to one platform; see
[Scope resources to the right artifact](#scope-resources-to-the-right-artifact).
`SOURCE` is `archive`, `asset`, `repo`, or `exec`, and `VALUE` is the
path it reads or the command it runs; see [Choose a source](#choose-a-source).
Repeat `--resource` for each entry.

In TOML, write one `[[resource]]` table per entry, with a field for each
qualifier and one for the source:
`completion/zsh=archive:share/zsh/site-functions/_mytool` becomes
`kind = "completion"`, `shell = "zsh"`, and
`archive = "share/zsh/site-functions/_mytool"`.

| Kind | Flag form | TOML fields | Notes |
| --- | --- | --- | --- |
| `completion` | `completion/SHELL[/BIN]`, or `completion/SHELL,SHELL[/BIN]` with `exec` | `shell`, or `shells` with `exec`; `bin` | Several shells only with `exec`, whose command or `env` contains `{shell}`. |
| `man` | `man[/BIN]` | `bin` | The file's suffix gives the section, as in `mytool.1`. |
| `cli-spec` | `cli-spec/FORMAT[/BIN]` | `format`, `bin` | The documented format is `usage`. |
| `skill` | `skill/NAME` | `name` | A directory that holds `SKILL.md`; as an `asset`, an archive of that directory. See [Agent skills and desktop files](#agent-skills-and-desktop-files). |
| `sbom` | `sbom/FORMAT` | `format` | `cyclonedx` or `spdx`. Never from `exec`. |
| `desktop`, `icon` | `desktop`, `icon` | | A freedesktop desktop entry, or an icon file. |
| `app` | `app` | | A macOS `.app` bundle inside a `dmg` or `zip`. `archive` source only. |
| A kind of your own, such as `font` | `KIND` or `KIND/NAME` | `name` | Consumers ignore kinds they do not know. |

`BIN` is the name of one of the release's executables; see
[Completions and CLI specifications](#completions-and-cli-specifications)
for when an entry must give it. See
[Resource kinds](/release/v1/#resource-kinds) for what each kind means to
a consumer.

## Choose a source

A resource has a kind and exactly one source. The source decides what
pins the content:

| Source | Example flag value | What pins the content |
| --- | --- | --- |
| `archive` | `man=archive:share/man/man1/mytool.1` | The containing artifact's digest. |
| `asset` | `sbom/cyclonedx=asset:dist/mytool.cdx.json` | The separate file's digest in the statement. |
| `repo` | `skill/mytool=repo:skills/mytool` | `source.commit`, which is required. |
| `exec` | `completion/zsh=exec:mytool completion zsh` | The executable is verified; its output is not separately signed. |

A `repo` entry needs the source repository and commit: pass
`--source-repo` with the repository URL and `--commit` with the full
commit SHA, or set `source.repo` and `source.commit` in TOML. The GitHub
action sets both.

An `asset` value is a local path: `create` digests the file and records
only its file name, such as `mytool.cdx.json`. Upload the file with the
release, and give its download URL with `--url mytool.cdx.json=URL` (the
file name, without `dist/`), the entry's `url` in TOML, or `--url-base`
(`url_base` in TOML). The GitHub action sets `--url-base` to the
release's download URL. A file matched both as an artifact, for example
by `dist/*`, and as an asset is recorded once, as the asset.

`create` checks that each entry is well formed, digests every asset, and
confirms that an `exec` command and any `bin` qualifier name a release
executable. It does not open archives to check `archive` paths, read the
repository at `repo` paths, or run `exec` commands. A mistyped path is
signed as written, and consumers install without that resource. An
`archive` path starts at the archive's root, including any top-level
directory such as `mytool-1.2.3/`. List the archive's contents
(`tar tf`) and inspect the result with `packslip show` before publishing.

## Completions and CLI specifications

```sh
packslip create ... \
  --resource 'completion/bash,zsh,fish=exec:mytool completion {shell}' \
  --resource 'cli-spec/usage/mytool=archive:share/usage/mytool.kdl'
```

Declare either entry, or both. The first runs `mytool completion {shell}`,
with `{shell}` replaced by each listed shell. The second ships a usage
spec, from which consumers generate completions, man pages, and docs. If
you declare both, consumers use completions generated from the spec and
run `exec` only when they cannot. Completions generated from a usage spec
need usage's completion engine each time the shell completes.
[mise embeds the engine](https://mise.jdx.dev/dev-tools/packslip-resources.html#generated-completions);
other consumers install `usage` beside the tool. If your users may
install the tool without mise or `usage`, ship static completion scripts
as well. Consumers use a static script before they generate completions
from the spec.

Every `cli-spec` names the executable it describes. The flag may leave
the name out (`cli-spec/usage=...`) only when exactly one executable is
given with `--bin` or the TOML manifest's top-level `bin`. A `completion`
or `man` entry must name its executable when the release has more than
one, as in `completion/zsh/mytool=archive:share/zsh/site-functions/_mytool`
or `man/mytool=archive:share/man/man1/mytool.1`; `create` refuses an
entry that leaves it out. In TOML, set `bin = "mytool"` on the entry.

An `exec` command runs one of the release's own executables: its first
word after any `NAME=value` words must be a `bin` name, and `create`
refuses any other program, such as `sh`. The CLI splits the value on
whitespace and does not interpret shell quoting, pipes, or redirections.
Leading `NAME=value` words become environment variables, so
`--resource 'completion/bash,zsh,fish/mytool=exec:COMPLETE={shell} mytool'`
runs `mytool` with `COMPLETE` set to each shell. In TOML, `exec` is an
array, so an argument may contain spaces. This TOML entry is equivalent
to that flag:

```toml
[[resource]]
kind = "completion"
bin = "mytool"
shells = ["bash", "zsh", "fish"]
exec = ["mytool"]
env = { COMPLETE = "{shell}" }
```

Consumers generate exec completions on demand and cache them. Other
exec resources require permission to run vendor code during installation.
See [execution rules](/release/v1/#running-an-exec-entry).

## Man pages and SBOMs

```sh
packslip create ... \
  --resource 'man=archive:share/man/man1/mytool.1' \
  --resource 'sbom/cyclonedx=asset:dist/mytool.cdx.json'
```

The first entry ships a man page from the archive; the file's suffix
gives its section. Name the executable (`man/mytool=...`) when the
release has more than one, as for a completion.

The second ships a CycloneDX SBOM as a separate release file whose
digest the packslip records; use `sbom/spdx` for SPDX. An SBOM must come
from an `archive`, `asset`, or `repo` source, so that a digest or commit
covers it, and `create` refuses one from `exec`. For one SBOM per
platform, declare one entry per platform with an `@` scope
(`sbom/cyclonedx@linux/x86_64=...`), or one per artifact with the TOML
`artifact` field; see
[Scope resources to the right artifact](#scope-resources-to-the-right-artifact).

## Agent skills and desktop files

`skill/mytool=repo:skills/mytool` points at a directory containing
`SKILL.md` at the release commit. To ship a separate archive, use
`skill/mytool=asset:dist/mytool-skill.tar.gz`; put `SKILL.md` at its root
or under a single top-level directory. This lets consumers provide the
skill that matches the installed tool version.

Desktop releases can declare `desktop`, `icon`, and `app` resources.
There is no separate CLI/GUI category: each release declares the items
it provides. See [resource kinds](/release/v1/#resource-kinds) for details.

## Scope resources to the right artifact

An entry without a scope applies to every artifact. When archive layouts
differ by platform, limit the entry to the artifacts of one platform. On
the command line, add the scope to the kind with `@`:

```sh
packslip create ... \
  --resource 'man@linux=archive:share/man/man1/mytool.1' \
  --resource 'completion/zsh@darwin/aarch64=archive:share/zsh/site-functions/_mytool'
```

The scope is `os`, `os/arch`, or `os/arch/libc`. It is read from the
kind rather than the value, so an `@` inside an `exec` argv or a path is
left alone. In TOML, set `os`, `arch`, or `libc` on the entry:

```toml
[[resource]]
kind = "man"
os = "linux"
archive = "share/man/man1/mytool.1"
```

Spell scope values as the [vocabularies](/release/v1/#vocabularies) do,
such as `darwin` and `aarch64`, not `macos` or `arm64`. `create` accepts
a scope that matches no artifact, and consumers never use such an entry.

Whatever its scope, an `archive` entry never applies to a bare executable
(format `raw`, `gz`, `xz`, `zst`, or `bz2`), which has no paths inside
it. To give the bare build the same man page, add an `asset` or `repo`
entry for it.

When archives for the same platform have different layouts, or a
resource belongs to one variant, scope the entry to one exact artifact.
The command line has no exact-artifact scope; use the `artifact` field
of a TOML `[[resource]]` table, which names one release file:

```toml
[[resource]]
kind = "man"
artifact = "mytool-1.2.3-linux-x64.tar.gz"
archive = "mytool-1.2.3/share/man/man1/mytool.1"
```

This man page belongs only to the named archive, not to another format or
variant for the same platform. The named artifact must be in the release
and match any platform scope on the entry, or `create` refuses the entry.
For complete configurations, see [Release recipes](/docs/recipes/).

When several entries for the same resource apply, the most specific wins;
see [How consumers choose among entries](#how-consumers-choose-among-entries).

## How consumers choose among entries

Consumers group entries by resource identity before selecting a source.
For a completion, the identity is the executable and shell; for a CLI
spec, the executable and format; for a skill, its name; for an SBOM, its
format. [Scope and identity](/release/v1/#scope-and-identity) gives the
identity of every other kind. A bash completion and a zsh completion are
separate needs, not fallback choices.

Within one identity, selection proceeds in this order:

1. Keep entries that apply to the selected artifact.
2. Prefer exact artifact scope, then the most specific platform scope.
3. Prefer `archive`, then `asset`, then `repo` sources. For a need a static
   CLI spec can generate, try that before running an `exec` resource.
4. Break ties within the same scope and source type by declaration order.
   Stop once a usable entry satisfies the need.

For example, these equally scoped entries try the current skill directory
first and the older location only if the first is unavailable. The release
must also declare `source.repo` and `source.commit`.

```toml
[[resource]]
kind = "skill"
name = "mytool"
repo = "skills/mytool"

[[resource]]
kind = "skill"
name = "mytool"
repo = ".agents/skills/mytool"
```

Fallback cannot rescue a wrongly scoped entry. A more specific entry
removes every less specific entry for the same resource before any source
is tried, so an unscoped entry never backs up a scoped one whose path is
wrong. Scope an archive resource to its artifact when other artifacts do
not contain that path.

Every resource is optional: if a consumer cannot fetch any entry for a
resource, it reports that and installs the executables without it. A
digest or source-commit mismatch fails the installation; it never
permits trying a different source. See
[Fallback and verification](/release/v1/#fallback-and-verification) and
[consumer rule 8](/release/v1/#consumer-rules) for the full rules.
