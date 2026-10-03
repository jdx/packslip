---
title: "Distribute the installer"
description: "Build offline source packages and publish interim distribution repositories."
weight: 19
---

# Distribute the installer

The distribution build includes `packslip install` and verification, with
publisher commands omitted. It uses the distribution's package manager to
install Packslip; Packslip can then bootstrap upstream package managers from
their authenticated releases. See [installation scopes](/docs/bootstrap/) and
the [support evidence](/docs/compatibility/).

The repository contains packaging recipes and publishing workflows for an
Ubuntu PPA, Fedora COPR, and signed APT/RPM repositories. These are interim upstream repositories, separate
from admission to a distribution's official archive. Their configuration must
be completed and their first published packages checked before recommending
them to users. The recipes do not claim Debian main or Fedora package-policy
compliance: they bundle Cargo dependencies for offline builds.

## Build offline packages

From a committed checkout, prepare the source archive:

```sh
scripts/package-source.sh source
```

The archive includes the locked Cargo dependencies with their original
checksum files and licenses. Collecting sources into this build archive
preserves the dependencies as separate crates. Platform-specific dependencies
are compiled only for their applicable targets.

On Linux, with Docker available, build and install each package in its native
distribution image:

```sh
scripts/check-distro-packages.sh deb source deb-results
scripts/check-distro-packages.sh rpm source rpm-results
```

Image preparation fetches the distribution's build tools. The package build,
tests, and installation run with networking disabled and `cargo --frozen`.
The checks also confirm the packaged CLI exposes `install`, omits publisher
commands, and provides `/etc/packslip/pins.d` for administrator policy.
CI requires these checks for both Linux architectures.

For unsigned source packages, install `devscripts`, `debhelper`, and `rpm`,
then run:

```sh
scripts/package-debian-source.sh source resolute
scripts/package-rpm-source.sh source
```

The Debian source uses a separate quilt orig archive. The RPM source package
includes the same vendored source and a spec for the complete installer.
Neither recipe invokes rustup or another language package manager during the
distribution build. The distribution must supply a Rust toolchain at least as
new as `rust-version` in `Cargo.toml`.

## Configure PPA and COPR publication

Create a Launchpad PPA and COPR project before enabling publication. Defaults
are `ppa:jdxcode/packslip`, Ubuntu `resolute`, and COPR `jdxcode/packslip` with
`fedora-44-x86_64` and `fedora-44-aarch64` chroots. Enable both architectures in
the PPA. Register the source-signing key with Launchpad; COPR signs its own
resulting repository packages.

The `distro-source.yml` workflow accepts an already published stable release
tag. It resolves that tag to a commit, checks the package version, prepares
offline source packages, signs the Debian upload, and waits for both COPR
architecture builds. Publishing credentials are available only in the
publication jobs, after source preparation.

| Setting | Purpose |
| --- | --- |
| Secret `PACKSLIP_GPG_KEY` | Private source-signing key accepted by Launchpad. |
| Variable `PACKSLIP_GPG_FINGERPRINT` | Full 40-character fingerprint of that key. |
| Secrets `COPR_API_LOGIN`, `COPR_API_TOKEN` | COPR build submission credentials. |
| Variable `PPA_NAME` | PPA destination; defaults to `ppa:jdxcode/packslip`. |
| Variables `PPA_DISTRIBUTION`, `PPA_REVISION` | Ubuntu series and source revision; defaults to `resolute` and `1`. Increment the revision for a source-package rebuild of the same upstream version. |
| Variables `COPR_OWNER`, `COPR_PROJECT`, `COPR_CHROOTS` | COPR destination and architecture chroots. |

Use the `ppa-publishing` and `copr-publishing` GitHub environments to restrict
credential access. Once the accounts and settings are ready, dispatch a release:

```sh
gh workflow run distro-source.yml -f tag=vX.Y.Z
```

An accepted PPA upload is not evidence that Launchpad built or published both
architectures. Check those builds separately. COPR submission waits for its
build results. Before announcing either repository, install its package on a
clean native machine, confirm `packslip install --help`, and run the
[publisher adoption checks](/docs/compatibility/#bootstrap-platform-and-handoff-coverage).

The PPA/COPR signature authenticates Packslip's package distribution. It does
not approve the publishers installed by `packslip install`: their release
signatures, remembered identities, freshness, and administrator constraints
are still checked independently.

## Signed APT and RPM repositories

The direct repositories package native static Linux installers for x64 and
ARM64. They use the distribution's package manager and CA certificates, with
no dependency on another language package manager. Their first publication
requires a stable release that contains the installer and these recipes;
the commands below are for use after that publication has been verified.

The dedicated Packslip package key has fingerprint
`2A355C7DF63A62A5851534C583B336958530A3D2` and expires on October 2, 2028.
Its [public key](/gpg-key.pub) authenticates both APT metadata and RPM packages
and metadata. It is separate from the keys that upstream publishers use for
their releases.

For APT, download the key and check its fingerprint before installing it:

```sh
curl --fail --silent --show-error --output packslip-key.asc https://packslip.dev/gpg-key.pub
test "$(gpg --show-keys --with-colons packslip-key.asc | awk -F: '$1 == "fpr" {print $10; exit}')" = 2A355C7DF63A62A5851534C583B336958530A3D2
gpg --dearmor --output packslip-key.gpg packslip-key.asc
sudo install -m 644 packslip-key.gpg /usr/share/keyrings/packslip.gpg
printf '%s\n' 'deb [signed-by=/usr/share/keyrings/packslip.gpg] https://packslip.dev/apt stable main' | sudo tee /etc/apt/sources.list.d/packslip.list
sudo apt-get update
sudo apt-get install packslip
packslip install --help
```

For RPM systems, use the same downloaded key and fingerprint check, then:

```sh
sudo rpm --import packslip-key.asc
curl --fail --silent --show-error --output packslip.repo https://packslip.dev/rpm/packslip.repo
sudo install -m 644 packslip.repo /etc/yum.repos.d/packslip.repo
sudo dnf install packslip
packslip install --help
```

The repository file enables both package and metadata signature checks.
Repository CI uses an ephemeral key, tests APT and DNF installation on both
native Linux architectures, and rejects altered metadata, a wrong key, and
a modified RPM. Publication signs with the dedicated production key; PR
validation never receives that private key.

Enable `PACKAGE_REPOSITORIES_ENABLED=true` only after the first eligible
release and deployment of the repository-serving Worker. The release workflow
then publishes packages, and a weekly refresh keeps APT's fourteen-day signed
metadata expiry current. `DISTRO_SOURCE_ENABLED=true` separately enables PPA
and COPR source uploads. Publication jobs use the `package-repositories`,
`ppa-publishing`, and `copr-publishing` environments.

APT indices and package payloads have content-addressed names; publication
retains older objects so clients using an earlier authenticated index can
finish their downloads. The signed APT `InRelease` object is published last.
RPM's metadata and detached signature are separate requests: a concurrent
refresh can briefly fail signature verification and should be retried.
Keep signature checks enabled.

Before key expiry, publish the replacement key and fingerprint through the
documentation, update the publishing secret, and give repository users time
to install the new key. Retain the old public key through the transition;
clients must explicitly trust a replacement key.
