---
title: Install packslip
description: Install the packslip CLI with mise, the install script, the container image, APT or DNF, a direct download, or a source build.
group: start
weight: 10
---
# Install packslip

Put the packslip CLI on a workstation, CI runner, or container image. At
the end, `packslip version` prints the release you installed.
Publishing from GitHub Actions needs no install step, because the action
fetches its own CLI ([Publish with GitHub Actions](/docs/publishing/)).
To install a tool with packslip, see [Bootstrap a tool](/docs/bootstrap/).

## Choose a method

The recommended method is [mise](#install-with-mise): it installs
packslip and manages its version. Other methods:

- [Install script](#run-the-install-script): a quick install on Linux,
  macOS, or Windows.
- [Versioned install script](#pin-a-versioned-install-script): Dockerfiles
  and CI jobs that must install the same bytes every time.
- [Container image](#copy-packslip-from-the-container-image): images
  without curl, tar, or a hash tool.
- [APT or DNF](#install-with-apt-or-dnf): Linux machines the package
  manager keeps up to date (install and verify only).
- [Direct download](#download-a-release): mirrors, offline machines, and
  checking a release before running it.
- [Source build](#build-from-source): Intel Macs, other platforms, and
  development changes.

## Check for a release build

Each release ships executables for five platforms (`darwin-arm64` is
macOS on Apple silicon):

| Release name | Built for |
| --- | --- |
| `linux-x64` | `x86_64-unknown-linux-musl` |
| `linux-arm64` | `aarch64-unknown-linux-musl` |
| `darwin-arm64` | `aarch64-apple-darwin` |
| `windows-x64` | `x86_64-pc-windows-msvc` |
| `windows-arm64` | `aarch64-pc-windows-msvc` |

- The Linux executables are statically linked and need no system
  libraries.
- The Windows executables link the C runtime statically, so
  `packslip.exe` starts without the Visual C++ Redistributable, such as
  in Windows Sandbox or a Server Core container.
- The macOS executable is signed with the
  `Developer ID Application: Jeffrey Dickey (4993Y37DX6)` certificate and
  notarized by Apple.

Intel Macs (`darwin-x64`) and other platforms have no release build;
[build from source](#build-from-source) there.
[Supported platforms](/docs/compatibility/#supported-platforms) says which
commands run there.

## Install with mise

With [mise](https://mise.jdx.dev/getting-started.html) 2026.9.2 or newer
installed and activated, run:

```sh
mise use -g packslip
packslip version
```

`-g` makes packslip available globally; omit it to manage packslip in the
current project.

Before installing a packslip release, mise checks it against the workflow
that signs it. [Use packslip with mise](/docs/mise/) explains the backend,
completions, and how mise installs other tools.

## Run the install script

On Linux or macOS, run:

```sh
curl -fsSL https://packslip.sh | sh
```

On Windows, in PowerShell:

```powershell
irm https://packslip.sh/install.ps1 | iex
```

The script:

- downloads your platform's executable from the script's own release,
  and refuses it unless its SHA-256 matches the digest the release build
  wrote into the script;
- installs into `~/.local/bin`, or `/usr/local/bin` when run as root
  (`~\.local\bin` on Windows);
- runs the installed `packslip version`;
- downloads and runs nothing else, and takes no arguments.

If the install directory is not on PATH, the script prints a line that
adds it (`export PATH=...`, or `$env:Path = ...` on Windows); run it. To
keep it, add the line to your shell's startup file, or
the directory to your Windows user Path variable.

Two environment variables change the defaults:

- `PACKSLIP_BIN_DIR` sets a different install directory.
- `PACKSLIP_RELEASE_URL` downloads the executable from a mirror instead of
  `https://packslip.dev/vX.Y.Z`. The checksums in the script still apply.

On Linux and macOS, the script needs curl or wget, and one of
`sha256sum`, `shasum`, or `openssl`. It downloads the bare executable
because many images cannot unpack an archive: Debian and Ubuntu images
lack xz, and Amazon Linux 2023 and UBI minimal images lack tar.

`https://packslip.sh` serves each release's scripts:

- `/` and `/install.sh` serve the latest release's `install.sh`, and
  `/install.ps1` its `install.ps1`.
- `/vX.Y.Z` and `/vX.Y.Z/install.sh` serve release X.Y.Z's `install.sh`,
  and `/vX.Y.Z/install.ps1` its `install.ps1`.

The latest release is the release list's `latest` recommendation, unless
it is withdrawn or a prerelease; then it is the highest release that is
neither. Plain HTTP gets only a redirect to HTTPS, which does not protect
an intercepted request, so use the `https://` URL. Every client gets the
same bytes, so the script a browser shows is the one a shell runs; the
user agent only counts downloads.

## Pin a versioned install script

To hold packslip at a chosen version, install it through a versioned
install script or a [digest-pinned container image](#copy-packslip-from-the-container-image).
One `install.sh` covers every Linux and macOS platform, so pinning its
checksum fixes the download on each; `install.ps1` does the same for both
Windows architectures.

Pin release 1.5.1 or newer. Scripts and the image start with 1.5.0:
packslip.sh returns 404 for an earlier script, and the image has no
earlier tag. In 1.5.0, `packslip install` fails under a
group-writable umask, such as Debian's `002` default.

1. Download the versioned script and calculate its checksum (on macOS,
   `shasum -a 256 install.sh`):

   ```sh
   curl -fsSL -o install.sh https://packslip.sh/vX.Y.Z
   sha256sum install.sh
   ```

2. Optionally, check that packslip's release workflow built the script,
   which is attested with the rest of the release:

   ```sh
   gh attestation verify install.sh --repo jdx/packslip
   ```

3. In a Dockerfile, fetch the script by that checksum, which Docker checks
   before running it:

   ```dockerfile
   ADD --checksum=sha256:<sha256 of install.sh> https://packslip.sh/vX.Y.Z /tmp/install-packslip.sh
   RUN sh /tmp/install-packslip.sh
   ```

For an image without curl or wget, [copy packslip from the container image](#copy-packslip-from-the-container-image)
instead.

## Copy packslip from the container image

`ghcr.io/jdx/packslip` is an otherwise empty image for linux/amd64 and
linux/arm64. It holds a release's static Linux executable at `/packslip`
and the CA certificates at `/etc/ssl/certs/ca-certificates.crt`, and runs
the same bytes as the release's downloads. It is tagged `X.Y.Z`, `X.Y`,
`X`, and `latest`; pin 1.5.1 or newer
([Pin a versioned install script](#pin-a-versioned-install-script) says
why).

In a Dockerfile, copy the executable out:

```dockerfile
COPY --from=ghcr.io/jdx/packslip:X.Y.Z@sha256:<digest> /packslip /usr/local/bin/packslip
```

One image digest pins both architectures, and Docker checks it, so the
image you copy into needs no curl, tar, or hash tool. For HTTPS, packslip
reads the system's CA certificates. If that image has none, copy the
certificate file too:

```dockerfile
COPY --from=ghcr.io/jdx/packslip:X.Y.Z@sha256:<digest> /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
```

To find the digest, run:

```sh
docker buildx imagetools inspect ghcr.io/jdx/packslip:X.Y.Z
```

The digest is attested like the release files; check it with:

```sh
gh attestation verify oci://ghcr.io/jdx/packslip:X.Y.Z --repo jdx/packslip
```

To run packslip from the image directly, mount your files and replace
`version` with any packslip command:

```sh
docker run --rm -v "$PWD:/work" -w /work ghcr.io/jdx/packslip:X.Y.Z version
```

## Install with APT or DNF

Signed APT and RPM repositories carry packslip for Linux x64 and ARM64,
starting with release 1.5.0. They are separate from your distribution's
official archive.

- The packages contain a static installer build of packslip, without
  `create`, `keygen`, `releases`, or `schema`. To publish, or to follow
  [Try packslip locally](/docs/getting-started/), install another way.
- The packages depend on the system's CA certificates, not on Rust,
  Cargo, or another language package manager. They install the man page
  and shell completions.

The packslip package key has fingerprint
`2A355C7DF63A62A5851534C583B336958530A3D2` and expires on October 2, 2028.
Its public key, at `https://packslip.dev/gpg-key.pub`, authenticates the
APT metadata and the RPM packages and metadata. It is separate from
publishers' release keys: tools packslip installs are checked against
their publishers' release signatures and your trust policy, as
[How trust works](/docs/trust/) explains.

1. Download the key and check its fingerprint. These commands need `curl`
   and `gpg`:

   ```sh
   curl --fail --silent --show-error --output packslip-key.asc https://packslip.dev/gpg-key.pub
   test "$(gpg --show-keys --with-colons packslip-key.asc | awk -F: '$1 == "fpr" {print $10; exit}')" = 2A355C7DF63A62A5851534C583B336958530A3D2
   ```

   Continue only if the fingerprint check succeeds.

2. Trust the key, add the repository, and install packslip. On an APT
   system:

   ```sh
   gpg --dearmor --output packslip-key.gpg packslip-key.asc
   sudo install -m 644 packslip-key.gpg /usr/share/keyrings/packslip.gpg
   printf '%s\n' 'deb [signed-by=/usr/share/keyrings/packslip.gpg] https://packslip.dev/apt stable main' | sudo tee /etc/apt/sources.list.d/packslip.list
   sudo apt-get update
   sudo apt-get install packslip
   packslip install --help
   ```

   On an RPM system:

   ```sh
   sudo rpm --import packslip-key.asc
   curl --fail --silent --show-error --output packslip.repo https://packslip.dev/rpm/packslip.repo
   sudo install -m 644 packslip.repo /etc/yum.repos.d/packslip.repo
   sudo dnf install packslip
   packslip install --help
   ```

   The first `dnf install` asks to import the key again, to check the
   repository metadata. Accept it only if the fingerprint it shows is
   `2A355C7DF63A62A5851534C583B336958530A3D2`.

The RPM repository file enables both package and metadata signature
checks; keep both enabled. If `dnf` reports
`repomd.xml GPG signature verification error: Bad GPG signature`
(`Bad PGP signature` on dnf5), retry: the error can occur briefly while
the repository is being updated.

A replacement package key needs your explicit trust. APT trusts only the
keyring that `signed-by` names: repeat these steps with the new
fingerprint. DNF offers to import a key it lacks
from the repository file's `gpgkey` URL. Accept it only if this page
publishes its fingerprint.

**Warning:** `dnf -y`, and dnf-automatic on dnf4 (RHEL 9 and similar),
accept that offer without a fingerprint check. On dnf5, `dnf automatic`
declines it and skips the packslip repository until you import the new
key.

## Download a release

Each release's files are on
[GitHub releases](https://github.com/jdx/packslip/releases) and at
`https://packslip.dev/vX.Y.Z/`. Each platform ships twice, under its
release name from [Check for a release build](#check-for-a-release-build):

- an archive, `packslip-vX.Y.Z-PLATFORM.tar.xz` (`.zip` on Windows);
- the executable alone, `packslip-vX.Y.Z-PLATFORM` (`.exe` on Windows).

Package managers take the archive; use it too unless you need the
executable alone. Before you put either file on PATH, you can
[check it against the release bundle](#verify-a-download).

To install from the archive:

1. Extract the archive.
2. Put the `packslip` executable (`packslip.exe` on Windows) from its
   `packslip-vX.Y.Z-PLATFORM/` directory on PATH.
3. Check the installation with `packslip version`.

To use the executable alone instead, rename it to `packslip`
(`packslip.exe` on Windows), make it executable, and put it on PATH.

## Verify a download

Releases of packslip are signed with packslip. Since release 1.1.0, each
is signed as the domain project `packslip.dev`, with its release bundle,
`packslip.sigstore.json`, at `https://packslip.dev/vX.Y.Z/` and on the
GitHub release. With packslip installed, this checks
the Linux x64 archive of release 1.10.1 against the bundle:

```sh
curl -fLO https://packslip.dev/v1.10.1/packslip.sigstore.json
curl -fLO https://packslip.dev/v1.10.1/packslip-v1.10.1-linux-x64.tar.xz
packslip verify packslip.sigstore.json \
  --identity-prefix https://github.com/jdx/packslip/.github/workflows/release.yml@ \
  --issuer https://token.actions.githubusercontent.com \
  --pin ps1_mcx64bcghek2t4vljgb3lho3ti \
  --artifact packslip-v1.10.1-linux-x64.tar.xz
```

- `--identity-prefix` and `--issuer` name the workflow that signs packslip
  releases: `release.yml` in `jdx/packslip`, run by GitHub Actions.
- `--pin` adds a check: the signing repository must have the signer
  fingerprint `ps1_mcx64bcghek2t4vljgb3lho3ti`, which is `jdx/packslip`'s.
- `--artifact` checks the downloaded archive against the digest in the
  bundle. Keep the file name it was published under.

The identity flags are required because `packslip.dev` is a domain
project, whose name implies no signer. Releases before 1.1.0 are signed as
`github.com/jdx/packslip`, with the bundle on the GitHub release only, and
need no identity flags. [How trust works](/docs/trust/) explains derived policies and signer
fingerprints.

[Understand the result](/docs/verifying/#understand-the-result) explains
the output and what a successful check establishes.
`packslip show packslip.sigstore.json` lists the release's other
artifacts.

Without packslip, check a download's build provenance with the GitHub
CLI:

```sh
gh attestation verify packslip-vX.Y.Z-linux-x64.tar.xz --repo jdx/packslip
```

## Build from source

Building the current release requires Rust 1.93 or newer:

```sh
cargo install packslip --locked
packslip version
```

- This builds the latest release from crates.io. Add `--version X.Y.Z` to
  pin a release.
- Cargo installs the executable in its bin directory, usually
  `~/.cargo/bin`, which must be on PATH.

To try development changes, clone the
[repository](https://github.com/jdx/packslip) and run
`cargo install --path . --locked` in it. Run the walkthrough in
[Try packslip locally](/docs/getting-started/) from an empty directory,
not from the checkout.

## Upgrade packslip

Upgrade with the method you installed with:

- mise: `mise upgrade packslip`.
- Install script: run it again, with the same `PACKSLIP_BIN_DIR` if you
  set one, to install the latest release.
- Versioned install script: change the version in the URL, and the
  checksum to the new script's.
- Container image: change the image tag and digest.
- APT: `sudo apt-get update && sudo apt-get install --only-upgrade packslip`.
- DNF: `sudo dnf upgrade packslip`.
- Direct download: download the new release, verify it, and replace the
  executable.
- Source build: run `cargo install packslip --locked` again; it rebuilds
  when crates.io has a newer release.

## Add shell completions

`packslip completion` prints a completion script to standard output;
redirect it to where your shell loads completions. For bash, for
your user:

```sh
mkdir -p ~/.local/share/bash-completion/completions && packslip completion bash > ~/.local/share/bash-completion/completions/packslip
```

Replace `bash` with `elvish`, `fish`, `nu`, `powershell`, or `zsh` for
another shell, and use the location that shell loads from. The script asks
the installed packslip for candidates, so it keeps working after upgrades.

The APT and RPM packages already install completions for bash, zsh, and
fish. With mise activated, packslip's completions follow its active
version without setup; see [Use packslip with mise](/docs/mise/).
