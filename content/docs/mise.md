---
title: Use packslip with mise
weight: 60
group: consume
description: Bootstrap mise with packslip, or use mise to install tools from signed releases with matching completions, man pages, and agent skills.
---
# Use packslip with mise

packslip and mise work together in two directions:

- **Install mise with packslip.** Start with a small verifier, authenticate
  mise's upstream release, and put its executable on PATH.
- **Install tools with mise.** mise's `packslip:` backend reads signed release
  manifests, manages tool versions, and provides the completions, man pages,
  and agent skills declared for the version active in your project.

The first installs mise itself. The second needs mise already installed and
does not need the packslip CLI: mise includes the verifier.

## Install mise with packslip

With [packslip 1.5.1 or newer](/docs/getting-started/#install-packslip), run:

```sh
packslip install github.com/jdx/mise \
  --pin ps1_nlhmwtfeufglxv5myvwvronk7a
~/.local/bin/mise --version
```

The command checks mise's signed release against its GitHub repository and
the signer pin, verifies the archive, and exports `mise` into `~/.local/bin`
for a Unix user (`/usr/local/bin` as root). Follow
[mise's shell setup](https://mise.jdx.dev/installing-mise.html#shells) if you
want automatic project activation; packslip does not edit your shell files.

The signer pin fixes the publisher, while omitting `--version` selects the
current release. You can therefore pin packslip in a base image or bootstrap
configuration and let mise float independently. Add `--version 2026.10.1`
when you also want to fix mise's release. Repeating `packslip install` fetches
and replaces mise; `mise self-update` can update the installed mise directly.

See [Install a tool with packslip](/docs/bootstrap/) for scopes, trust, and a
[Docker example](/docs/bootstrap/#bootstrap-mise-in-docker). mise's
[installation guide](https://mise.jdx.dev/installing-mise.html#packslip)
compares this route with its other installation methods.

## Use mise's packslip backend

The [consumer rules](/release/v1/#consumer-rules) set out what mise must
check, including the signer, the requested project and version, the
downloaded bytes, the release list, and the trust remembered from earlier
installs. They also say when mise may run a vendor's command to generate
a resource. Where mise stores that trust, which settings control it, and
how completions, man pages, and skills reach your shell and agent are
mise's own choices. The sections below show them; the
[mise documentation](https://mise.jdx.dev/dev-tools/backends/packslip.html)
lists every option.

## Install a tool

With mise 2026.9.2 or later installed and activated, install a tool by
giving its project name after the `packslip:` prefix:

```sh
mise use -g packslip:github.com/jdx/hk
```

Omit `-g` to record the tool in the current project's `mise.toml`
instead. mise's packslip backend (the `packslip:` prefix) reads the signed
release manifest to choose the artifact for your platform and find its
executables. Before unpacking the download, it verifies the signature
against the repository the name gives, checks that the signed project and
version are the ones you asked for, and checks the file's digest and size.

For one tool in a monorepo, add its subpath:
`packslip:github.com/owner/repo/mytool`.

A project named after its own domain, such as `packslip:mytool.example.com`,
is found through its signed release list, and its name does not say whom
to trust. The signer comes from tool options, as
[Host releases on your own domain](/docs/self-hosting/#sign-as-the-repository)
shows, or from a mise registry entry that carries them; without either,
mise refuses the tool. The
[mise backend documentation](https://mise.jdx.dev/dev-tools/backends/packslip.html#tool-options)
describes each tool option.

packslip itself is a domain project, `packslip.dev`. Install it with
`mise use -g packslip`. mise's registry entry for packslip names that
project and supplies the workflow identities that sign its releases and
its release list.

## Keep completions aligned with the active version

With mise activated in your shell, completions need no setup. When a
packslip tool is active in the current directory, mise registers a loader
for each of its commands whose release declares a completion or CLI spec.
When you press Tab, mise reads the script for the version active there.
Bash, zsh, fish, and PowerShell are supported. After a directory or
version change, the next completion uses the version now active.

Without shell activation, install a stub that asks mise for the script.
`--tool` takes the command name, not the `packslip:` identifier:

```sh
mise completion zsh --tool mytool --install
```

Omit `--install` to print the script instead.

mise uses a completion file the release ships when there is one, and
otherwise renders the release's usage-format CLI spec itself, so you do
not need `usage` installed. When the release offers a completion only as
an `exec` command, mise runs that command the first time the shell asks
for the completion and caches the output for that version, command, and
shell. The `packslip.exec` setting does not affect this; it covers only
resources generated at install time, such as an `exec` skill.

## Read the matching man pages

When a release declares a `man` resource as a file (in the archive, as a
separate asset, or in the repository), mise adds it to `MANPATH` while
that version is active: in an activated shell and under `mise exec`,
`mise run`, and `mise env`. mise does not install man pages that come
from an `exec` command or could only be generated from a CLI spec. This
needs mise 2026.9.4 or later. A tool that an earlier mise installed needs
one reinstall, such as `mise install --force packslip:github.com/jdx/hk`.

## Give agents the matching skill

A vendor can declare a skill directory in an artifact, in a separate
signed asset, or at the release's source commit, and by default mise
fetches it at install time without running the tool. A skill that only
an `exec` command generates is created at install time only when the
[`packslip.exec`](https://mise.jdx.dev/configuration/settings.html#packslip.exec)
setting is enabled; it is off by default. List the skills for the tool
versions active in your project, then link them into your agent's skills
directory:

```sh
mise skills ls
mise skills sync --dir .agents/skills
```

Run sync again after a version change to repoint the links mise made; it
leaves directories and links you created alone. Without `--dir`, sync uses
`.claude/skills` under the project root. Set `skills.dir` to change that,
and `skills.auto_sync = true` to sync after every `mise install` and
`mise use`. The links point into your local installs, so keep them out of
version control. See
[mise's skill settings](https://mise.jdx.dev/dev-tools/packslip-resources.html#skills)
for the other options, such as pruning stale links.

## Preserve trust across upgrades and machines

mise keeps what it has accepted in two places:

- Local state records each project's accepted signer, how its accepted
  releases were signed and attested (scheme, vendor or repackager,
  provenance links), and the highest release-list sequence it accepted.
  From mise 2026.9.16 it also records the forge's repository ID, so a
  renamed repository keeps its pin and a different repository created
  under the same name is refused; see
  [Renamed repositories](https://mise.jdx.dev/dev-tools/backends/packslip.html#renamed-repositories)
  in the mise documentation for transfers.
- `mise.lock` records the project's pin alongside artifact URLs and
  digests. Commit it with `mise.toml`, and teammates and CI that run
  `mise install --locked` enforce the same signer on their first install.

mise refuses a release from a different signer or signing scheme, a
repackager's release after it accepted the vendor's own, a release that
drops the provenance links every artifact carried before, and a release
list whose sequence is lower than the highest it accepted. If a release is
refused because its signer changed, confirm the change with the vendor.
Then run `mise packslip pins` to see the remembered signer and
`mise packslip forget PROJECT` to reset it. If `mise.lock` has a
conflicting entry for the project, remove the entry and run
`mise install` to regenerate it, as
[Signer changes](https://mise.jdx.dev/dev-tools/backends/packslip.html#pinned-signers)
in the mise documentation describes.

A GitHub repository can add an optional signed release list, called a
supplementary list, to withdraw or recommend releases. Once mise has
accepted one, it refuses the project if that list later disappears, so
deleting the list cannot undo a withdrawal.

These checks follow the [consumer rules](/release/v1/#consumer-rules);
the lockfile, the commands, and the settings are mise's own.

## Install a release that was just published

When mise picks a version for a request such as `latest`, it skips
releases younger than its
[`minimum_release_age`](https://mise.jdx.dev/configuration/settings.html#minimum_release_age)
setting, 24 hours by default, so a release published minutes ago is not
offered yet. mise measures the age from the transparency-log timestamp,
not from the publication time the vendor signs; only an unlogged bundle
you allowed is measured from its signed `published_at`. From mise
2026.9.10, an exact version, or one recorded in `mise.lock`, installs
right away. The setting and its 24-hour default are mise's own choice.

## Require review by a third party

mise requires no third-party review by default. A reviewer, registry, or
scanning service can sign a list of the releases it has checked; such a
service is a stamping host, and its list is a stamp on each release it
names. To install only versions a host you trust has stamped, list each
host with its pin in mise's `packslip.stampers` setting:

```toml
[settings.packslip]
stampers = ["reviews.example.com=https://github.com/example/reviews/"]
```

A version is then installable only when at least one of those hosts
lists it and has not withdrawn it, and a vendor withdrawal still excludes
it. A stamp limits which releases mise accepts; the vendor's signature
still authenticates them. Set the tool option `trust = "vendor"` to
exempt one tool. See
[Manage release lists](/docs/release-lists/#use-a-third-party-list) for
how stamping works, and
[mise's stamp settings](https://mise.jdx.dev/dev-tools/packslip-verification.html#stamps)
for the pin formats.
