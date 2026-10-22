---
title: CLI reference
description: The packslip commands grouped by task, with shell completion setup and a reference generated from each command's help.
---

# CLI reference

Use packslip to sign a release in your release job, check an artifact you
downloaded, or install an upstream tool without its installer script. Below,
its commands are grouped by task, each with its reference page and the guide
that uses it. To install packslip with mise, a script, a package, the container
image, a direct download, or a source build, see
[Install packslip](/docs/install/).

## Publish a release

| Command | What it does | Guide |
| --- | --- | --- |
| [`packslip create`](/cli/create/) | Create and sign the release bundle for a release | [Publish with GitHub Actions](/docs/publishing/) or [Try packslip locally](/docs/getting-started/) |
| [`packslip keygen`](/cli/keygen/) | Generate a key pair for key-signed releases | [Publish from other CI](/docs/other-ci/) |
| [`packslip releases`](/cli/releases/) | Create and sign a release list, which lists a project's releases and can withdraw or recommend them; a domain project needs one | [Manage release lists](/docs/release-lists/) |

## Verify a release

| Command | What it does | Guide |
| --- | --- | --- |
| [`packslip verify`](/cli/verify/) | Check a release bundle or release list, and the files you downloaded | [Verify a release](/docs/verifying/) |
| [`packslip pin`](/cli/pin/) | Print the signer fingerprint of a release you trust | [Pin a signer with its fingerprint](/docs/verifying/#pin-a-signer-with-its-fingerprint) |
| [`packslip show`](/cli/show/) | Print a bundle's statement without verifying it | [Read the signed statement](/docs/getting-started/#read-the-signed-statement) |

`show` reads a statement without checking anything. `verify` checks a bundle
and the local files you give it.

## Install a tool

| Command | What it does | Guide |
| --- | --- | --- |
| [`packslip install`](/cli/install/) | Install a tool from its verified release, as in `packslip install github.com/jdx/hk` | [Bootstrap a tool](/docs/bootstrap/) |

Beyond what `verify` checks, `install` finds the release, selects and
downloads the artifact for this host, remembers which signer it trusted, and
installs the commands the release declares.

## Utilities

| Command | What it does | Guide |
| --- | --- | --- |
| [`packslip schema`](/cli/schema/) | Print the JSON schema of a statement or of a `verify --json` report | [JSON schemas](/release/v1/#json-schemas) |
| [`packslip completion`](/cli/completion/) | Print a shell completion script | [Set up shell completions](#set-up-shell-completions) |
| [`packslip version`](/cli/version/) | Print packslip's version | |

<!-- `completion --help` in binaries built from this source links this
heading's id, set-up-shell-completions. Keep the heading, or map the old id
to its new place in data/legacy_anchors.toml. -->
## Set up shell completions

Two install methods set up completions for you:

- The APT and DNF packages install completions for bash, zsh, and fish.
- With mise activated in bash, zsh, fish, or PowerShell, packslip's
  completions follow its active version; see
  [Use packslip with mise](/docs/mise/).

Otherwise, `packslip completion SHELL` prints a script that asks the
installed packslip for candidates, so it keeps working after upgrades. Save
it where your shell loads completions, then start a new shell.

For bash, with the bash-completion package installed:

```sh
mkdir -p ~/.local/share/bash-completion/completions
packslip completion bash > ~/.local/share/bash-completion/completions/packslip
```

For zsh, write the script to a directory of your own:

```sh
mkdir -p ~/.zfunc
packslip completion zsh > ~/.zfunc/_packslip
```

Then add `fpath=(~/.zfunc $fpath)` to `~/.zshrc`, above the line that runs
`compinit`.

For fish:

```sh
mkdir -p ~/.config/fish/completions
packslip completion fish > ~/.config/fish/completions/packslip.fish
```

For Elvish, add this line to `~/.config/elvish/rc.elv`:

```elvish
eval (packslip completion elvish | slurp)
```

For Nushell, run this in Nushell to write the script to its autoload
directory. The shell name can be `nu` or `nushell`.

```nu
mkdir ($nu.default-config-dir | path join autoload)
packslip completion nu | save --force ($nu.default-config-dir | path join autoload packslip.nu)
```

For PowerShell, run this to load the script from your profile. The shell
name can be `powershell` or `pwsh`.

```powershell
New-Item -ItemType Directory -Force (Split-Path $PROFILE) | Out-Null
Add-Content -Path $PROFILE -Value 'packslip completion powershell | Out-String | Invoke-Expression'
```

## Get help

- `packslip --help` describes packslip and lists its commands.
  `packslip <command> --help`, such as `packslip install --help`, prints a
  command's full help, and `-h` prints a shorter version. Help goes to stdout
  with exit code 0.
- `packslip --version`, `packslip -V`, and `packslip version` print
  `packslip X.Y.Z` and exit with code 0. `packslip version --json` adds the
  OS and architecture.
- An invalid command line, such as a command missing a required argument,
  prints an error and the command's usage to stderr and exits with code 2.
  `packslip` alone prints its help to stderr, also with exit code 2.
- A command that fails exits with code 1. The
  [`verify`](/cli/verify/#exit-status) and [`pin`](/cli/pin/#exit-status)
  pages list what each exit code means.

The command pages are generated from the development branch's help, so they
can include options added after the latest release. Run
`packslip <command> --help` for the options your installed version supports.
