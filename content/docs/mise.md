---
title: Use packslip with mise
description: Bootstrap mise with packslip, or install tools from release bundles with mise's packslip backend and get matching completions, man pages, and skills.
group: consume
weight: 56
---
# Use packslip with mise

For mise users: install mise with the small packslip CLI, then install tools
with mise's `packslip:` backend, which brings the completions, man pages, and
agent skills each active version declares. To install one tool without mise,
see [Bootstrap a tool](/docs/bootstrap/).

The backend needs no packslip CLI; mise includes the verifier. Every option
is in
[mise's packslip backend documentation](https://mise.jdx.dev/dev-tools/backends/packslip.html),
and [Version requirements](/docs/compatibility/#version-requirements) gives
the mise releases that added backend features. The
[consumer rules](/release/v1/#consumer-rules) set what mise checks and when
it may run a vendor's command; its settings, trust storage, and lockfile are
its own.

## Install mise with packslip

[Install packslip](/docs/install/), then run:

```sh
packslip install github.com/jdx/mise --pin ps1_nlhmwtfeufglxv5myvwvronk7a
~/.local/bin/mise --version
```

- `ps1_nlhmwtfeufglxv5myvwvronk7a` is the signer fingerprint of mise's
  signing repository. Install checks the signed release against mise's GitHub
  repository and this fingerprint, then verifies the archive.
- Install prints the command's path (`Command: …`): `~/.local/bin/mise` for
  a Unix user, or `/usr/local/bin/mise` as root. The installation keeps the
  archive's `bin`, `man`, and `share` trees, README, and license.

Install edits no shell files; to activate mise in projects, follow
[mise's shell setup](https://mise.jdx.dev/installing-mise.html#shells).
[Bootstrap a tool](/docs/bootstrap/) covers scopes, pin files, and other
install options, and mise's
[installation guide](https://mise.jdx.dev/installing-mise.html#packslip)
compares this route with its other methods.

## Choose when mise updates

The fingerprint fixes mise's publisher, not its version: without
`--version`, install takes the latest stable release. You can fix packslip's
version in a base image or bootstrap configuration and let mise float, since
mise must track upstream releases to keep working with changing tool
registries.

- Add `--version X.Y.Z` to fix mise's release too.
- To update, repeat `packslip install`, which replaces mise, or run
  `mise self-update`, which updates it in place.

[Check a tool's handoff](/docs/compatibility/#check-a-tools-handoff) lists
where `mise self-update` through the exported command has been checked.
[Pin the bootstrapper and let the tool float](/docs/bootstrap/#pin-the-bootstrapper-and-let-the-tool-float)
explains why the two versions are fixed separately.

## Bootstrap mise in Docker

This Debian image copies packslip from its digest-pinned image, then verifies
and installs mise. For `X.Y.Z` and `<digest>`, see
[Copy packslip from the container image](/docs/install/#copy-packslip-from-the-container-image).

```dockerfile
FROM ghcr.io/jdx/packslip:X.Y.Z@sha256:<digest> AS bootstrap
FROM debian:13-slim

COPY --from=bootstrap /packslip /usr/local/bin/packslip
COPY --from=bootstrap /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

ARG MISE_VERSION=latest
RUN packslip install github.com/jdx/mise --version "$MISE_VERSION" \
      --pin ps1_nlhmwtfeufglxv5myvwvronk7a \
    && mise --version

CMD ["mise", "--version"]
```

```sh
docker build --no-cache -t mise-bootstrap .
```

- `--no-cache` resolves `latest` again on every build.
- `--build-arg MISE_VERSION=X.Y.Z` fixes mise's release.
- To copy mise into another stage, copy `/opt/packslip` too;
  `/usr/local/bin/mise` points into it.

[Install in a container](/docs/bootstrap/#install-in-a-container) explains
each step, and mise's
[Docker cookbook](https://mise.jdx.dev/mise-cookbook/docker.html#bootstrap-with-packslip)
shows how to add project tools and use mise in the container.

## Install a tool

Give the project name after the `packslip:` prefix:

```sh
mise use -g packslip:github.com/jdx/hk
```

Omit `-g` to record the tool in the current project's `mise.toml`. For one
tool in a monorepo, add its subpath: `packslip:github.com/owner/repo/mytool`.

The backend picks your platform's artifact and its executables from the
signed statement. Before unpacking, mise checks the signature against the
repository the name gives, unless the `identity`, `identity_prefix`,
`issuer`, `pubkey`, or `workflow` tool options replace that policy. Then
it checks the signed project and version against your request, and the
file's digest and size.

A domain project, such as `packslip:mytool.example.com`, is found through its
signed release list. Its name implies no signer, so mise refuses it unless a
mise registry entry or tool options give one:

```toml
[tools]
"packslip:mytool.example.com" = { version = "latest", issuer = "https://token.actions.githubusercontent.com", identity_prefix = "https://github.com/owner/mytool/" }
```

Use the values the publisher states;
[Publish the signer](/docs/self-hosting/#publish-the-signer) shows what a
publisher lists, and mise's
[tool options](https://mise.jdx.dev/dev-tools/backends/packslip.html#tool-options)
describes each option.

## Keep completions aligned with the active version

With mise activated in your shell, completions for packslip tools, including
packslip itself, need no setup. For each command whose active release declares a
completion or CLI spec, mise registers a loader that reads that version's
script on Tab, so completions follow directory and version changes. Bash,
zsh, fish, and PowerShell are supported.

Without shell activation, install a stub that asks mise for the script:

```sh
mise completion zsh --tool mytool --install
```

- With `--install`, `--tool` takes the command name, not the `packslip:`
  identifier.
- Omit `--install` to print the completion script instead.

A shipped completion file comes first; otherwise mise renders the release's
usage-format CLI spec, so `usage` need not be installed. An
`exec`-only completion runs the first time the shell asks and is cached per
version, command, and shell. It runs even with `packslip.exec` off, because
that setting covers only install-time resources.

## Read the matching man pages

When a release declares a
[`man` resource](/docs/resources/#ship-man-pages-and-sboms) as a file
(`archive`, `asset`, or `repo`), mise adds it to `MANPATH` while that version
is active: in an activated shell and under `mise exec`, `mise run`, and
`mise env`. Man pages from an `exec` command, or that only a CLI spec could
generate, are not installed.

## Give agents the matching skill

By default mise fetches a release's
[declared skill](/docs/resources/#ship-an-agent-skill) at install time
without running the tool. A skill that only an `exec` command generates is
created only when mise's
[`packslip.exec`](https://mise.jdx.dev/configuration/settings.html#packslip.exec)
setting is on; it is off by default.

List the skills for your project's active tool versions, then link them into
your agent's skills directory:

```sh
mise skills ls
mise skills sync --dir .agents/skills
```

- Rerun `mise skills sync` after a version change to repoint its links. It
  leaves directories and links you created alone.
- The links point into your local installs, so keep them out of version
  control.
- Without `--dir`, sync uses `.claude/skills` under the project root.

This configuration changes that default with
[`skills.dir`](https://mise.jdx.dev/configuration/settings.html#skills.dir)
and turns on
[`skills.auto_sync`](https://mise.jdx.dev/configuration/settings.html#skills.auto_sync),
which syncs after `mise install`, `mise upgrade`, and a `mise use` that
changes a version:

```toml
[settings.skills]
dir = ".agents/skills"
auto_sync = true
```

For other options, such as pruning stale links, see
[mise's skill settings](https://mise.jdx.dev/dev-tools/packslip-resources.html#skills).

To give agents packslip's own skill, install packslip with mise
(`mise use packslip`, as [Install with mise](/docs/install/#install-with-mise)
describes), then sync. Each packslip release declares the skill for its
version, so the link matches the CLI the project uses. That command installs
the domain project `packslip.dev`, and mise's registry names the workflows
that sign its releases and release list.

## Preserve trust across upgrades and machines

Accepted trust is kept in two places:

- **Local state** on each machine records each project's accepted signer,
  how its releases were signed (scheme, vendor or repackager, provenance
  links), the highest release-list sequence, and a forge project's
  repository ID.
- **`mise.lock`** records the project's pin with artifact URLs and digests.
  Create it with `mise lock`, then commit it with `mise.toml`. Teammates and
  CI that run `mise install --locked` enforce the same signer on their first
  install.

A renamed repository keeps its pin, and mise warns once with the new name.
Update the tool's `packslip:` name, which `workflow` expands from, and any
`identity` or `identity_prefix` that names the old repository.
[Repository identity, renames, and transfers](/docs/trust/#repository-identity-renames-and-transfers)
explains what mise checks, and mise's
[renamed repositories](https://mise.jdx.dev/dev-tools/backends/packslip.html#renamed-repositories)
section says what to do when a repository is re-created under the old name.

Once it has accepted a project, mise refuses a weaker or changed signer
until you reset it.
[No-downgrade and approvals](/docs/trust/#no-downgrade-and-approvals) and
[What each consumer enforces](/docs/trust/#what-each-consumer-enforces)
list what mise refuses.

If mise refuses a release because its signer changed
([mise's steps](https://mise.jdx.dev/dev-tools/backends/packslip.html#pinned-signers)
match these):

1. Confirm the change with the vendor.
2. Update any `pubkey`, `identity`, `identity_prefix`, `workflow`, or
   `list_identity_prefix` you set for the tool.
3. List the remembered signers, then reset the project:

   ```sh
   mise packslip pins
   mise packslip forget github.com/jdx/hk
   ```

4. If `mise.lock` records the old signer, delete the tool's entries, run
   `mise lock`, and commit the change.

## Install a release that was just published

A request such as `latest` skips releases younger than mise's
[`minimum_release_age`](https://mise.jdx.dev/configuration/settings.html#minimum_release_age)
setting, 24 hours by default. To install a release sooner, name its exact
version:

```sh
mise use packslip:github.com/jdx/hk@X.Y.Z
```

A version recorded in `mise.lock` also installs right away. Age counts from
the transparency-log timestamp, not the vendor-signed `published_at`, except
for an unlogged bundle you allowed.

## Require review by a third party

Third-party review is off by default. To install only versions that a
trusted [stamping host](/docs/trust/#stamping-hosts) has stamped, list each
host in mise's `packslip.stampers` setting with the key or identity allowed
to sign its lists:

```toml
[settings.packslip]
stampers = ["reviews.example.com=https://github.com/example/reviews/"]
```

The tool option `trust = "vendor"` exempts one tool.
[Publish a list as a stamping host](/docs/release-lists/#publish-a-list-as-a-stamping-host)
shows how a host publishes its list, and mise's
[stamps](https://mise.jdx.dev/dev-tools/packslip-verification.html#stamps)
section gives the accepted formats.
