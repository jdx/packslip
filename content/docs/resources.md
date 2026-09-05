---
title: Resources
weight: 32
description: Ship completions, CLI specifications, skills, SBOMs, and desktop files with a release.
---
# Resources

Resources describe what ships alongside a release's executables. Declare
them once so consumers can install the files or generate content for the
version a user actually runs.

Use resource flags with `packslip create`, or add `[[resource]]` tables
to a [TOML manifest](/docs/describing-releases/#use-a-toml-manifest).

## Choose a source

A resource has a kind and exactly one source. Repeat `--resource` for
multiple entries, or use one `[[resource]]` table per entry in TOML.

| Source | Example flag value | What pins the content |
| --- | --- | --- |
| `archive` | `man=archive:share/man/man1/mytool.1` | The containing artifact's digest. |
| `asset` | `sbom/cyclonedx=asset:dist/mytool.cdx.json` | The separate file's digest in the statement. |
| `repo` | `skill/mytool=repo:skills/mytool` | `source.commit`, which is required. |
| `exec` | `completion/zsh=exec:mytool completion zsh` | The executable is verified; its output is not separately signed. |

An asset is a local file when creating the manifest and a downloadable
release file when consuming it. Upload it too. Set its URL with
`--url FILENAME=URL`, an entry's `url`, or `--url-base`. A file declared
both as an artifact and an asset is treated as an asset.

## Completions and CLI specifications

```sh
# Add either or both options to packslip create:
--resource 'completion/bash,zsh,fish=exec:mytool completion {shell}'
--resource 'cli-spec/usage/mytool=archive:share/usage/mytool.kdl'
```

The first example describes a command that generates a script for each
shell. The second ships a usage spec from which consumers can generate
completions, man pages, and docs. Usage-derived completions require the
consumer to provide `usage` at shell runtime.

In releases with several executables, name the command explicitly:
`completion/zsh/mytool=archive:share/zsh/site-functions/_mytool` or
`cli-spec/usage/mytool=archive:share/usage/mytool.kdl`. This lets consumers
select and cache resources for the correct command.

The CLI splits `exec` values on whitespace; it does not interpret shell
quoting, pipes, or redirections. Use a TOML `exec` array when an argument
contains spaces. Leading `NAME=value` words become environment variables:

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

## Agent skills and desktop files

`skill/mytool=repo:skills/mytool` points at a directory containing
`SKILL.md` at the release commit. To ship a separate archive, use
`skill/mytool=asset:dist/mytool-skill.tar.gz`; put `SKILL.md` at its root
or under a single top-level directory. This lets consumers provide the
skill that matches the installed tool version.

Desktop releases can declare `desktop`, `icon`, and `app` resources.
There is no separate CLI/GUI category: each release declares the items
it provides. See [resource kinds](/release/v1/#resources) for details.

## Scope resources to the right artifact

Unscoped resources apply to every artifact. Use `os`, `arch`, or `libc`
for platform-specific resources. Use `artifact = "FILENAME"` for a
resource that belongs to a particular archive format or variant, as in the example below.

For the same resource, exact artifact scope wins over platform scope.
Consumers then prefer the most specific platform scope before considering
the source type. Different commands, shells, and named skills remain
separate resources.

```toml
[[resource]]
kind = "man"
artifact = "mytool-1.2.3-linux-x64.tar.gz"
archive = "mytool-1.2.3/share/man/man1/mytool.1"
```

This man page belongs only to the named archive, not to another format or
variant for the same platform. For complete configurations, see the
[release recipes](/docs/recipes/).

## Provide fallbacks deliberately

Consumers group entries by resource identity before selecting a source.
For completions that identity is the executable and shell; for a CLI spec,
it is the executable and format; for a skill, it is the skill name. A bash
completion and a zsh completion are separate needs, not fallback choices.

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

Do not use fallback to hide an incorrectly scoped path: less specific entries
have already been removed before sources are tried. Scope an archive resource
to its artifact when other artifacts do not contain that path.

An unavailable optional resource is reported and may leave the installation
without that resource. A digest or source-commit mismatch fails the installation;
it never permits trying a different source. See the full
[resource selection rules](/release/v1/#resources).
