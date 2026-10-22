---
title: Try packslip locally
description: Verify a published release with packslip, then create, sign, and verify a sample release offline with a local key.
group: start
weight: 15
---
# Try packslip locally

Start here if you are new to packslip. You will verify a published
release, then create, sign, and verify one of your own with a local key,
and watch a modified file fail verification.

To install a tool, see [Bootstrap a tool](/docs/bootstrap/). To check files
you already downloaded, see [Verify a release](/docs/verifying/).

## Install packslip

The commands on this page use a POSIX shell. On Windows, run them in WSL
and install packslip there as on Linux.

With mise, run `mise use -g packslip`. Otherwise, on Linux or macOS, run
the install script:

```sh
curl -fsSL https://packslip.sh | sh
```

If the script prints an `export PATH=...` line, run it. `packslip version`
then prints the installed release.

[Install packslip](/docs/install/) lists every method. The APT and RPM
packages leave out the publishing commands;
[install packslip another way](/docs/install/#choose-a-method). On a platform without a
release build, such as an Intel Mac,
[build from source](/docs/install/#build-from-source).

## Verify a published release

The hk project attaches a packslip, a signed release bundle named
`packslip.sigstore.json`, to its GitHub releases. Download the bundle from
hk 2.8.0 and one of its archives, then verify the archive:

```sh
curl -fLO https://github.com/jdx/hk/releases/download/v2.8.0/packslip.sigstore.json
curl -fLO https://github.com/jdx/hk/releases/download/v2.8.0/hk-x86_64-unknown-linux-gnu.tar.gz
packslip verify packslip.sigstore.json --artifact hk-x86_64-unknown-linux-gnu.tar.gz
```

`packslip verify` itself downloads nothing. It prints:

```text
ok: github.com/jdx/hk 2.8.0 published 2026-10-20T19:21:05.952758937Z signed by https://github.com/jdx/hk/.github/workflows/release.yml@refs/tags/v2.8.0 (sigstore-oidc) logged 2026-10-20T19:21:06Z (1 of 7 artifact(s) and 0 of 5 asset(s) checked, provenance linked, 7 resource(s))
  repository https://github.com/jdx/hk (id 922514152), owner https://github.com/jdx (id 216188)
  requires hk-aarch64-apple-darwin.tar.gz: libs none
  requires hk-aarch64-unknown-linux-gnu.tar.gz: libs none
  requires hk-aarch64-unknown-linux-musl.tar.gz: libs none
  requires hk-x86_64-unknown-linux-gnu.tar.gz: libs none
  requires hk-x86_64-unknown-linux-musl.tar.gz: libs none
  requires hk-aarch64-pc-windows-msvc.zip: libs combase.dll vcruntime140.dll
  requires hk-x86_64-pc-windows-msvc.zip: libs combase.dll vcruntime140.dll vcruntime140_1.dll
```

You passed no trust flags, so `verify` derives the policy from the
project the bundle names, `github.com/jdx/hk`. The signature must come
from GitHub Actions, in a certificate that records `github.com/jdx/hk` as
its source repository.

- The `ok:` line names the project, the version, and the workflow that
  signed. Check that it names `github.com/jdx/hk` 2.8.0. It counts 1 of 7
  artifacts checked: the archive you passed.
- The `repository` line gives the IDs of the signing repository and its
  owner.
- Each `requires` line lists what one artifact needs from the host.

[Understand the result](/docs/verifying/#understand-the-result) explains
the rest of the `ok:` line. How trust works explains
[what a project name says about the signer](/docs/trust/#what-a-project-name-says-about-the-signer)
and [when to add a signer fingerprint](/docs/trust/#first-use-and-signer-fingerprints).

## Create a sample release

The rest of this page runs offline. It signs with a local Ed25519 key
instead of a CI identity, and keeps the test signature out of Rekor,
Sigstore's public transparency log.
[Publish a real release](#publish-a-real-release) explains what to change
for a logged release.

Run the commands in an empty directory. They also need `tar`, but no
platform-specific compiler.

<!-- docs-test: quickstart -->
```sh
mkdir -p staging/bin dist
printf '#!/bin/sh\nprintf "hello from mytool\\n"\n' > staging/bin/mytool
chmod +x staging/bin/mytool
tar -czf dist/mytool-1.2.3.tar.gz -C staging bin
packslip keygen --out release.key
```

You now have `dist/mytool-1.2.3.tar.gz`, an archive with one executable at
`bin/mytool`. `keygen` wrote a signing key pair, `release.key` and
`release.pub`, and printed its key ID (yours will differ):

```text
wrote release.key and release.pub (key id 8270AD3B51601889)
```

`release.key` is the secret key; keep it out of source control. Consumers
need only the public key, `release.pub`. The key ID names the signer in
later output. `keygen` refuses to overwrite an existing key, so to rerun
the walkthrough, use a new empty directory.

## Create and sign the packslip

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
wrote dist/packslip.sigstore.json (1 artifact(s), signed by 8270AD3B51601889, unlogged)
```

- `--project` is the name consumers ask for: a host and optional path,
  with no scheme.
- `--key release.key` signs with your local key. `--no-log` keeps the
  signature out of Rekor, so the command needs no network.
- `--out dist` writes the bundle into the `dist` directory, beside the
  archive. `create` neither copies nor moves the archive.
- `--url-base` is the prefix of each file's recorded download URL, so the
  archive's URL is `https://mytool.example.com/v1.2.3/mytool-1.2.3.tar.gz`.
  `create` neither contacts that host nor uploads anything.
- `--bin mytool` names the executable; `create` records its archive path,
  `bin/mytool`.
- The `:any` suffix declares the archive platform-independent, so `create`
  records no OS, architecture, or libc, whatever the file name says.
  Without a suffix, `create` reads the platform from the file name, as in
  `mytool-1.2.3-linux-x64.tar.gz`. This sample's name gives no platform,
  so the suffix only makes the intent explicit.

**Warning:** Without `--no-log`, `create` records a key-signed signature
in Rekor's public log. Keep `--no-log` for test signatures like this one.

For a fuller release, `create` also prints a [resource](/docs/resources/)
count, `repackager-attested` for a repackager's bundle, and a `requires`
line for each artifact with host requirements.

## Read the signed statement

`packslip show` decodes and prints the statement inside the bundle. It
does not check the signature; the next step does.

<!-- docs-test: quickstart -->
```sh
packslip show dist/packslip.sigstore.json
```

The statement's core fields, with the sha256 digest abbreviated:

{{< release-example >}}

| Field | What the consumer does with it |
| --- | --- |
| `subject` | Checks the downloaded file against its signed digest. |
| `predicateType` | Recognizes the payload as a packslip release statement. |
| `project` and `version` | Confirms that this is the requested project and release. |
| `artifacts[].name` | Connects the artifact metadata to its entry in `subject`. |
| `url` and `format` | Finds the file and determines how to unpack it. |
| `bin` | Finds the executable at its actual archive path. |

The full output also has a sha512 digest, the artifact's size, the
publication time, the signing identity (`scheme` and `key_id`), and the
in-toto `_type`. The surrounding bundle, not the statement, carries the
signature and verification material.

## Verify the archive

<!-- docs-test: quickstart -->
```sh
packslip verify dist/packslip.sigstore.json \
  --pubkey release.pub --allow-unlogged \
  --artifact dist/mytool-1.2.3.tar.gz
```

- `--pubkey release.pub` authenticates the bundle under your public key.
- `--allow-unlogged` accepts a bundle created with `--no-log`.
- `--artifact` checks the archive against its signed digest and size.

On success it prints one line (your key ID and time will differ):

```text
ok: mytool.example.com 1.2.3 published 2026-10-21T20:22:49.794836741Z signed by 8270AD3B51601889 (sigstore-key) unlogged (1 of 1 artifact(s) checked)
```

A successful exit means the bundle and the supplied archive both passed.
Without `--artifact`, the line ends `(0 of 1 artifact(s) checked)`: the
bundle was checked, but the archive was not.

Without `--allow-unlogged`, `verify` refuses this bundle:

```text
verification failed: bundle has no transparency log entry; pass --allow-unlogged to accept one
```

**Warning:** `--allow-unlogged` is an exception for offline tests. Never
add it to a real release's verification to work around a failure.

Compare the project and version in the `ok:` line with the release you
expected: your key authenticates the bundle, but `verify` does not know
what you expected.
[Understand the result](/docs/verifying/#understand-the-result) lists what
a successful check does and does not establish.

To see a failure, change the archive and verify again:

```sh
printf x >> dist/mytool-1.2.3.tar.gz
packslip verify dist/packslip.sigstore.json \
  --pubkey release.pub --allow-unlogged \
  --artifact dist/mytool-1.2.3.tar.gz
```

It exits with status 1 and names the mismatch (your digests will differ):

```text
verification failed: artifact mytool-1.2.3.tar.gz: sha256 is 812e915afcad577787a94fbfe89da7ceea4beef134d915262e13fdf2a59f6943, document says 1f72b5add6abc8a973ac11519d006a75863a8febb5361892223a183e9f6bd227
```

## Publish a real release

For a release people install, record the signature in Rekor, which needs
network access: drop `--no-log` from `create` and `--allow-unlogged` from
`verify`. Sign in CI, not with a key on your machine.

- **On GitHub**, publish from your release workflow with the packslip
  action; see [Publish with GitHub Actions](/docs/publishing/). It signs
  with the release workflow's identity, so there is no key to manage.
- **In other CI**, sign keylessly or with a key kept as a CI secret; see
  [Publish from other CI](/docs/other-ci/).
- **On your own domain**, a domain project such as `mytool.example.com`
  also publishes a signed release list.
  [Host releases on your own domain](/docs/self-hosting/) shows how to
  publish both from GitHub Actions.

To adapt the example to your own files, see
[Describe your artifacts](/docs/describing-releases/).
