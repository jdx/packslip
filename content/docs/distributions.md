---
title: Package packslip for a distribution
description: Build packslip's install-cli binary from an offline source archive and package it for Debian, Ubuntu, Fedora, or another distribution.
group: build
weight: 68
---

# Package packslip for a distribution

For distribution packagers: build a packslip package that installs and
verifies tools, without the publisher commands, offline from a source
archive. To install packslip yourself, including from the signed APT and
RPM repositories, see [Install packslip](/docs/install/).

## Choose a build

The package manager installs packslip; packslip then installs tools
from their signed upstream releases. Build without default features and
select one:

| Feature | Commands | Use it for |
| --- | --- | --- |
| `install-cli` | `install`, `verify`, `pin`, `show`, `completion`, `version` | A distribution package that bootstraps tools |
| `verify-cli` | `verify`, `pin`, `show`, `completion`, `version` | A verifier-only package |

```sh
cargo build --locked --release --no-default-features --features install-cli
cargo build --locked --release --no-default-features --features verify-cli
```

- `install-cli` includes installation, verification, discovery, host
  checks, archive extraction, and native command exports. It leaves out
  publishing, signing, deriving `requires.libs` from executables, and
  schema generation, so it has no publisher commands (`create`,
  `releases`, `keygen`, `schema`).
- The recipes and checks on this page build `install-cli`. A
  verifier-only package needs its own recipe.
- Both recipes build only for x64 and ARM64 (Debian `amd64` and `arm64`,
  RPM `x86_64` and `aarch64`). On other architectures, `packslip install`
  fails with `unsupported bootstrap host linux/<arch>`; package
  `verify-cli` there.
  [Supported platforms](/docs/compatibility/#supported-platforms) lists
  the tested platforms.

[The Rust crate's features](/docs/rust/#choose-features) lists the
library features behind them.

## Build offline packages

The source recipes in `packaging/debian/` and `packaging/rpm/packslip.spec`
build from a source archive that bundles every Cargo dependency, so the
package build needs no network. Neither recipe runs rustup or another
language package manager. They do not claim admission to Debian main or
compliance with Fedora's packaging policy.

### Prepare the source archive

This step needs network access, a Rust toolchain compatible with
`Cargo.toml`, Bash, Git, `tar`, and Python 3.11 or newer. From a checkout
of the release you are packaging, run:

```sh
git clone https://github.com/jdx/packslip
cd packslip
git checkout vX.Y.Z
scripts/package-source.sh source
```

The script ends with:

```text
Prepared packslip-X.Y.Z (complete installer, offline Cargo sources)
```

The script exports the committed `HEAD`, so commit local changes before
testing them. It writes three files to `source/`:

- `packslip-X.Y.Z.tar.gz`: the exported tree, with the locked Cargo
  dependencies in `vendor/` and a `.cargo/config.toml` that points Cargo at
  them.
- `packslip_X.Y.Z.orig.tar.gz`: the same archive under its Debian name.
- `packslip.spec`: the RPM spec with the version filled in.

Each vendored crate keeps its original checksum file and license, and
Cargo refuses one that no longer matches. The archive vendors every
platform's dependencies; a build compiles only those its target needs.

### Build and test the packages

On Linux with Docker, build and install each package in its native
distribution image:

```sh
scripts/check-distro-packages.sh deb source deb-results
scripts/check-distro-packages.sh rpm source rpm-results
```

- `deb` or `rpm`: the package type.
- `source`: the directory `package-source.sh` wrote.
- The third argument: the output directory, created if missing. It
  receives the packages (`.deb`, or `.rpm` and `.src.rpm`) and the
  packaged CLI's `installer.usage.kdl`.

The `deb` check uses an Ubuntu 26.04 image and the `rpm` check a Fedora 44
image, each with its distribution's Rust toolchain. Preparing the image
fetches the build tools. The package build, tests, and installation then
run with networking disabled and `cargo --frozen`.

Each check fails unless the installed package:

- runs `packslip install --help`;
- omits the publisher commands;
- provides `/etc/packslip/pins.d` for administrator policy.

### Build unsigned source packages

Install `devscripts`, `debhelper`, and `rpm`, then run:

```sh
scripts/package-debian-source.sh source resolute
scripts/package-rpm-source.sh source
```

- `package-debian-source.sh` builds a `3.0 (quilt)` source package with
  the separate orig archive. The second argument is the target series
  (default `resolute`). The unsigned `.dsc`, `.debian.tar.*`,
  `.changes`, and `.buildinfo` files land in `source/`.
- `package-rpm-source.sh` writes a `.src.rpm` to `source/` with the same
  vendored source and a spec that builds the `install-cli` binary.

The Debian source package carries upstream's changelog entry and
maintainer, so use it for testing. `PACKAGE_REVISION` sets that entry's
revision number (default `1`). To build one under your own name:

1. Unpack the archive beside its orig copy and add the Debian recipe:

   ```sh
   cd source
   tar -xf packslip-X.Y.Z.tar.gz
   cd packslip-X.Y.Z
   cp -R packaging/debian debian
   ```

2. Set `Maintainer` in `debian/control`.
3. Write your own `debian/changelog` (for example with `dch`) with a
   version `X.Y.Z-<revision>`, so `dpkg-source` finds the orig archive.
4. Run `dpkg-buildpackage -S -d`. It signs the upload with your GPG key.
   `-d` skips checking the build dependencies in `debian/control`, which
   a source-only build does not use.

## Write your own recipe

The recipes build and test with:

```sh
cargo build --release --frozen --no-default-features --features install-cli --bin packslip
cargo test --frozen --no-default-features --features install-cli
```

The build environment needs a Rust toolchain at least as new as
`rust-version` in `Cargo.toml`, a C compiler, CMake, pkg-config, and `jq`.
Only the action integration tests use `jq`; the installed CLI does not.
The package needs the system's CA certificates at run time.

Both recipes set `AWS_LC_SYS_NO_JITTER_ENTROPY=1` while building and
testing, because the jitter-entropy sources of `aws-lc-sys` must compile
without the distribution's optimization flags. The RPM spec also adds
`-fPIE` to `LDFLAGS` for Fedora's hardening flags.

Both recipes install:

| Installed path | Source |
| --- | --- |
| `/usr/bin/packslip` | The `install-cli` build |
| `/etc/packslip/pins.d/` | An empty directory for administrators' pin files; see [Require signers with pin files](/docs/bootstrap/#require-signers-with-pin-files) |
| `/usr/share/man/man1/packslip.1` | `packaging/packslip.1`, the `install-cli` man page |
| `/usr/share/bash-completion/completions/packslip` | `packslip completion bash` |
| `/usr/share/zsh/site-functions/_packslip` | `packslip completion zsh` |
| `/usr/share/fish/vendor_completions.d/packslip.fish` | `packslip completion fish` |
| `bootstrap.md` in the documentation directory | `content/docs/bootstrap.md` |
| The license | `LICENSE`: Debian installs it as `/usr/share/doc/packslip/copyright-upstream`; the RPM marks it `%license` |

Packaging compresses the man page to `packslip.1.gz`, and Debian's tools
also compress `bootstrap.md`. The RPM also ships `README.md`.

## Keep a packaged version current

An unchanged package keeps working while its formats are supported and its
trust root is current; a `verify-cli` build never refreshes its trust root.
[Maintaining packaged
verifiers](/docs/compatibility/#maintaining-packaged-verifiers) says what
forces an update and what upstream backports.
