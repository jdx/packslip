---
title: Getting started
weight: 10
group: start
description: Create and verify your first packslip locally with an Ed25519 key.
---
# Getting started

Create and verify a signed release on your own machine. This walkthrough
packages a small shell script, signs its release metadata with a local
Ed25519 key, and checks the archive against that signed record. You will
also change the archive to see verification fail.

After installing packslip, the example runs offline. It uses no CI
identity and leaves the test signature out of Rekor, sigstore's public
transparency log. [Publish a real release](#publish-a-real-release)
explains how to turn the example into a logged release.

If you want to install someone else's software instead, install packslip
below, then follow [Install a tool](/docs/bootstrap/). To check files you
already downloaded, see [Verify a release](/docs/verifying/).

## Install packslip

We recommend [mise](https://mise.jdx.dev/getting-started.html) to install
packslip and manage its version. You can also run the install script,
copy packslip from its container image, download a release, or build from
source. Release builds cover Linux and Windows on x64 and arm64, and macOS
on arm64. On an Intel Mac, build from source. Linux users can also install
from the [signed APT and RPM repositories](/docs/distributions/#signed-apt-and-rpm-repositories).

{{< tabs "Installation method" >}}
{{< tab "mise" >}}

With [mise](https://mise.jdx.dev/getting-started.html) 2026.9.2 or newer
installed and activated, run:

```sh
mise use -g packslip
packslip version
```

The registry entry for packslip uses mise's
[packslip backend](https://mise.jdx.dev/dev-tools/backends/packslip.html)
and names the workflow that signs packslip's releases. mise checks each
release against that workflow before installing it. `-g` makes packslip
available globally; omit it to manage packslip in the current project
instead.

{{< /tab >}}
{{< tab "Script" >}}

On Linux or macOS, run:

```sh
curl -fsSL https://packslip.sh | sh
packslip version
```

On Windows, in PowerShell:

```powershell
irm https://packslip.sh/install.ps1 | iex
```

The script downloads the packslip executable for your platform and refuses
it unless its SHA-256 matches the one written into the script when the
release was built. It installs the executable in `~/.local/bin`, or in
`/usr/local/bin` when run as root, and `~\.local\bin` on Windows. Set
`PACKSLIP_BIN_DIR` to install somewhere else. The script says when that
directory is not on PATH, and it downloads and runs nothing else.

Each release publishes its own copy of the scripts, and
`https://packslip.sh/vVERSION` serves that release's `install.sh`. Replace
`VERSION` with a release number, such as `1.5.1`, and `SHA256` with the
SHA-256 of that release's script. One script covers every supported
platform, so its checksum pins the bootstrap download across architectures.
In a Dockerfile, Docker checks that script before running it:

```dockerfile
ADD --checksum=sha256:SHA256 https://packslip.sh/vVERSION /tmp/install-packslip.sh
RUN sh /tmp/install-packslip.sh
```

The image needs curl or wget for the script's download. To calculate the
script's checksum, download the versioned script and run `sha256sum
install.sh` (Linux) or `shasum -a 256 install.sh` (macOS). The scripts are
attested with the rest of the release, so you can check a copy you saved
with `gh attestation verify install.sh --repo jdx/packslip`.

{{< /tab >}}
{{< tab "Container image" >}}

`ghcr.io/jdx/packslip` holds packslip and a CA bundle, for linux/amd64 and
linux/arm64. In a Dockerfile, copy the executable out of it:

```dockerfile
COPY --from=ghcr.io/jdx/packslip:VERSION@sha256:DIGEST /packslip /usr/local/bin/packslip
```

The digest pins every architecture at once, and the image you copy into
needs no curl, tar, or hash tool. `docker buildx imagetools inspect
ghcr.io/jdx/packslip:VERSION` shows the digest. The image is attested
like the release files:

```sh
gh attestation verify oci://ghcr.io/jdx/packslip:VERSION --repo jdx/packslip
```

You can also run it directly, with your files mounted:

```sh
docker run --rm -v "$PWD:/work" -w /work ghcr.io/jdx/packslip:VERSION version
```

{{< /tab >}}
{{< tab "Download" >}}

Download the archive for your platform from
[GitHub releases](https://github.com/jdx/packslip/releases): `linux-x64`,
`linux-arm64`, and `darwin-arm64` as `.tar.xz`, or `windows-x64` and
`windows-arm64` as `.zip`. Extract the archive, put the `packslip`
executable from its `packslip-vVERSION-PLATFORM/` directory on PATH, then
check the installation. Each release also has the executable alone,
`packslip-vVERSION-PLATFORM`, with `.exe` on Windows:

```sh
packslip version
```

{{< /tab >}}
{{< tab "Build from source" >}}

The current release requires Rust 1.93 or newer:

```sh
cargo install packslip --locked
packslip version
```

This builds the latest release from crates.io. To pin the bootstrapper,
add `--version 1.5.1`. To try development changes, clone the repository
and run `cargo install --path . --locked` there, then return to an empty
directory for the walkthrough. Cargo installs the executable in its bin
directory, usually `~/.cargo/bin`; make sure that directory is on PATH.

{{< /tab >}}
{{< /tabs >}}

## Create a sample release

The following commands use a POSIX shell and `tar`. Run them in an empty
directory. The sample needs no platform-specific compiler. On Windows,
use a POSIX environment such as WSL for these commands.

<!-- docs-test: quickstart -->
```sh
mkdir -p staging/bin dist
printf '#!/bin/sh\nprintf "hello from mytool\\n"\n' > staging/bin/mytool
chmod +x staging/bin/mytool
tar -czf dist/mytool-1.2.3.tar.gz -C staging bin
packslip keygen --out release.key
```

You now have `dist/mytool-1.2.3.tar.gz`, an archive with one executable at
`bin/mytool`. `packslip keygen` also generated a signing key pair and
printed its key ID (yours will differ):

```text
wrote release.key and release.pub (key id C8B574447E4F0ACA)
```

`release.key` is the secret key; keep it out of source control. Consumers
need only the public key, `release.pub`. The key ID names the signer in
later output. `keygen` refuses to overwrite a key, so to run the
walkthrough again, start in a new empty directory.

## Create and sign the packslip {#sign-the-manifest}

<!-- docs-test: quickstart -->
```sh
packslip create \
  --project mytool.example.com \
  --version 1.2.3 \
  --key release.key --no-log \
  --out dist \
  --url-base https://mytool.example.com/v1.2.3 \
  --bin mytool \
  dist/mytool-1.2.3.tar.gz:any
```

`create` hashes the archive and finds the executable inside it. It
describes the release in a JSON statement, signs the statement with your
key, and writes the signed bundle to `dist/packslip.sigstore.json`:

```text
wrote dist/packslip.sigstore.json (1 artifact(s), signed by C8B574447E4F0ACA, unlogged)
```

- `--project` is the name consumers ask for: a host and optional path,
  with no scheme. A project on its own domain is named after that host; a
  GitHub project is `github.com/owner/repo`.
- `--key release.key --no-log` signs with your key and does not record the
  signature in Rekor, so the command needs no network.
- `--out dist` writes the bundle into `dist`, beside the archive.
  `create` does not copy or move the archive.
- `--url-base` is the prefix of each file's recorded download URL, so
  the archive's URL is
  `https://mytool.example.com/v1.2.3/mytool-1.2.3.tar.gz`. `create`
  neither contacts it nor uploads anything.
- `--bin mytool` names the executable. `create` finds it in the archive
  and records its path, `bin/mytool`.
- The `:any` suffix declares the archive platform-independent, so
  `create` records no OS, architecture, or libc whatever the file name
  says. Without a suffix, `create` reads the platform from the file name,
  as in `mytool-1.2.3-linux-x64.tar.gz`. This sample's name gives no
  platform, so here the suffix only makes the intent explicit.

## Read the signed statement {#read-the-manifest}

`packslip show` prints the statement inside the bundle. It only decodes
the statement; the next step checks the signature.

<!-- docs-test: quickstart -->
```sh
packslip show dist/packslip.sigstore.json
```

The statement's core fields look like this (the sha256 digest is
abbreviated):

{{< release-example >}}

| Field | What the consumer does with it |
| --- | --- |
| `subject` | Checks the downloaded file against its signed digest. |
| `predicateType` | Recognizes the payload as a packslip release statement. |
| `project` and `version` | Confirms that this is the requested project and release. |
| `artifacts[].name` | Connects the artifact metadata to its entry in `subject`. |
| `url` and `format` | Finds the file and determines how to unpack it. |
| `bin` | Finds the executable at its actual archive path. |

This sample omits `os`, `arch`, and `libc` because the archive is
declared platform-independent. The full output also has a sha512 digest,
the artifact's size, the publication time, the signing identity (`scheme`
and `key_id`), and the in-toto `_type`. The surrounding bundle carries the
signature and verification material.

## Verify the archive

<!-- docs-test: quickstart -->
```sh
packslip verify dist/packslip.sigstore.json \
  --pubkey release.pub --allow-unlogged \
  --artifact dist/mytool-1.2.3.tar.gz
```

On success it prints one line (your key ID and time will differ):

```text
ok: mytool.example.com 1.2.3 published 2026-10-02T19:23:31.767734662Z signed by C8B574447E4F0ACA (sigstore-key) unlogged (1 of 1 artifact(s) checked)
```

A successful exit means the bundle passed verification against your key
and the supplied archive matched its signed digest and size.
`--allow-unlogged` is necessary because this example used `--no-log`.
Without `--artifact`, the line ends `(0 of 1 artifact(s) checked)`: the
bundle was checked, the archive was not.

Compare the project and version in the `ok:` line with the release you
expected. `verify` authenticates the document under your key but does not
know which project or version you intended to install.
[Verify a release](/docs/verifying/#understand-the-result) lists what a
successful check does and does not establish.

To see a failure, change the archive and verify again:

```sh
printf x >> dist/mytool-1.2.3.tar.gz
packslip verify dist/packslip.sigstore.json \
  --pubkey release.pub --allow-unlogged \
  --artifact dist/mytool-1.2.3.tar.gz
```

The command exits with status 1 and names the mismatch (your digests will
differ):

```text
verification failed: artifact mytool-1.2.3.tar.gz: sha256 is d8e80700490e50d80579a4469b3dd9eed898634ed00204396da9707b2054c7f5, document says 96597cf66f34b2001b8a45786979bf3e167e8a36d6b65e5c277e8e3f59270447
```

## Publish a real release

The walkthrough's shortcuts are for trying the format. For a release
people install:

- **On GitHub**, publish from your release workflow with the packslip
  action; see [Publish with GitHub Actions](/docs/publishing/). It signs
  with the release workflow's identity, so there is no key to manage, and
  the project defaults to `github.com/owner/repo`.
- **With a key**, drop `--no-log` from `create` so the signature is
  recorded in Rekor (this needs network access), and drop
  `--allow-unlogged` from `verify`. Sign every release with the same key,
  and publish `release.pub` through a channel consumers trust, separate
  from the releases themselves. A consumer refuses a release signed by a
  different key until a person approves the change.

On your own host, upload each artifact to the URL the statement records,
and put the bundle beside it, for example at
`https://mytool.example.com/v1.2.3/packslip.sigstore.json`. Consumers find
a project named after its own domain, like `mytool.example.com`, only
through a signed release list on that host, and they refuse the project
without one. Publish a list with every release, as
[Manage release lists](/docs/release-lists/) explains. To publish the
releases and the list from GitHub Actions, see
[Host releases on your own domain](/docs/self-hosting/). To adapt the
example to your own files, see
[Artifact configuration](/docs/describing-releases/).
