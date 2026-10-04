# packslip CLI

The `packslip` command publishes signed release manifests, verifies downloads,
and installs tools from their manifests. Use it to add signing to a release
job, check an artifact you downloaded, or install an upstream tool such as
mise without running its installer script.

## Install the CLI

With [mise](https://mise.jdx.dev/getting-started.html) installed and activated:

```sh
mise use -g packslip
packslip version
```

Or install the standalone executable on Linux or macOS:

```sh
curl -fsSL https://packslip.sh | sh
```

On Windows, run `irm https://packslip.sh/install.ps1 | iex` in PowerShell.
The scripts install into `~/.local/bin` (`~\.local\bin` on Windows), or
`/usr/local/bin` when run as root on Unix. Add the reported directory to PATH
if needed. Release executables cover Linux and Windows on x64 and ARM64, and
macOS on ARM64.

For a container, a pinned script, a direct download, or a source build,
see [installation methods](/docs/getting-started/#install-packslip).

## Choose a task

| You want to… | Command | Guide |
| --- | --- | --- |
| Install a tool from its signed release | `packslip install github.com/jdx/mise` | [Install a tool with packslip](/docs/bootstrap/) |
| Create and sign a release manifest | `packslip create` | [Getting started](/docs/getting-started/) or [publish from CI](/docs/publishing/) |
| Check a signer and downloaded files | `packslip verify` | [Verify a release](/docs/verifying/) |
| Read the metadata inside a bundle | `packslip show` | [Read the manifest](/docs/getting-started/#read-the-signed-statement) |
| Get a repository's signer fingerprint | `packslip pin` | [Pin a signer](/docs/verifying/#pin-a-signer-with-its-fingerprint) |
| Withdraw or recommend releases | `packslip releases` | [Manage release lists](/docs/release-lists/) |

`show` reads metadata without verifying it. `verify` checks a bundle and the
local files you supply; `install` also discovers the release, selects an
artifact, downloads it, remembers trust, and installs its declared commands.
Installation requires packslip 1.5.0 or newer; use 1.5.1 or newer on systems
with a group-writable umask such as Debian's `002` default.

For a local walkthrough that needs no CI account, [create and verify a sample
release](/docs/getting-started/#create-a-sample-release).

## Get help and completions

```sh
packslip --help
packslip install --help
packslip completion zsh
```

Replace `zsh` with `bash`, `fish`, or `powershell` for your shell. With mise
activated, completions for packslip follow its active version automatically;
see [packslip and mise](/docs/mise/).

The command reference below is generated from the development branch's CLI
help. It can include options added after the latest release; your installed
command's `--help` describes the options it supports.
