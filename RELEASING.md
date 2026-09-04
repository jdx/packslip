# Releasing packslip

Maintainers release by reviewing and merging the release PR maintained by
release-plz. An ordinary push to `main` updates that PR; it does not by
itself publish a version. Publication also requires the release job to be
enabled as described below.

## Release sequence

1. A push to `main` runs `release-plz release-pr`, updating the
   `chore: release vX.Y.Z` PR with a `Cargo.toml` version bump and a
   changelog generated from conventional commits through `cliff.toml`.
   The workflow also regenerates the CLI documentation in that PR.
2. A maintainer reviews and merges the PR.
3. If `RELEASE_PLZ_RELEASE` is `true`, the release job publishes the crate
   to crates.io and then creates the `vX.Y.Z` tag.
4. The tag triggers `release.yml`, which checks that the tag matches
   `Cargo.toml`, builds five platform binaries, signs and notarizes the
   macOS one, attests them all, and creates a draft GitHub release. It
   generates narrative notes with Communiqué, publishes the release's
   packslip, publishes the GitHub release, and moves the action's matching
   major tag (`v0` for 0.x releases, `v1` for 1.x releases, and so on).

The action reads its default CLI version from its own `Cargo.toml`, so
the release PR's version bump also updates that default. The action and CLI
share one version, including for action-only changes. `action.yml` is
included in the Cargo package so release-plz detects those changes.
Use conventional commits such as `fix(action): ...` or `feat(action): ...`;
breaking action changes affect the shared version too.

## Platforms

| Asset           | Target                       | Runner             |
| --------------- | ---------------------------- | ------------------ |
| `linux-x64`     | `x86_64-unknown-linux-musl`  | `ubuntu-latest`    |
| `linux-arm64`   | `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` |
| `darwin-arm64`  | `aarch64-apple-darwin`       | `macos-latest`     |
| `windows-x64`   | `x86_64-pc-windows-msvc`     | `windows-latest`   |
| `windows-arm64` | `aarch64-pc-windows-msvc`    | `windows-11-arm`   |

There is no `darwin-x64` asset. Rosetta 2 runs the arm64 binary on the
Intel Macs that remain, which is a better trade than signing, notarizing,
and supporting a second macOS artifact.

Windows arm64 is built on a native arm64 runner rather than cross-compiled,
because `aws-lc-sys` — the crypto behind rustls — compiles C and assembly
for the host toolchain.

## macOS signing and notarization

The macOS binary is signed with the `Developer ID Application: Jeffrey
Dickey (4993Y37DX6)` certificate under `--options runtime --timestamp`,
which the notary service requires, and then submitted to `notarytool`.
The reported status must be `Accepted` or the job fails; `--wait` is not a
gate on its own, since it can return zero on an `Invalid` submission.

Nothing is stapled: `stapler` writes only into bundles, disk images, and
installer packages, and this is a bare Mach-O inside an archive. The ticket
is keyed to the binary's cdhash and lives on Apple's side, so Gatekeeper
resolves it online. That is what keeps a browser download from being held
behind the "cannot be verified" dialog.

## Repository setup

| Setting | Purpose |
| --- | --- |
| Secret `RELEASE_PLZ_TOKEN` | Fine-grained token with repository contents and pull-request write permissions. The workflows use it so generated PRs and tags can trigger subsequent workflows. |
| Secret `ANTHROPIC_API_KEY` | Lets Communiqué generate release notes. If generation fails or the key is unavailable, publication continues with GitHub's generated notes. |
| Variable `RELEASE_PLZ_RELEASE=true` | Enables the release job. Without it, merging the release PR does not publish a release. |
| Secrets `CERTIFICATES_P12`, `CERTIFICATES_P12_PASS` | The base64-encoded Developer ID Application certificate and its export password, the same pair the other jdx.dev CLIs use. The macOS build fails at signing without them. |
| Secrets `APPLE_API_KEY_P8`, `APPLE_API_KEY_ID`, `APPLE_API_ISSUER_ID` | A base64-encoded App Store Connect API key and its key and issuer IDs. The macOS job fails early and by name when any is missing, rather than shipping an unnotarized binary. |

Communiqué's context and tone are configured in `communique.toml`.
Its version is declared in `mise.toml` and resolved in `mise.lock`;
update the lock deliberately with `mise lock`.

## crates.io publication

crates.io needs no stored credential. 0.2.0 was published by hand from its
tag to create the crate — Trusted Publishing cannot create one — and a
Trusted Publisher is registered for repository `jdx/packslip`, workflow
`release-plz.yml`. The release job mints a short-lived token through OIDC
with `rust-lang/crates-io-auth-action`, so the API token used for that
first publish was revoked immediately afterwards.

`publish = true` in `release-plz.toml` follows from that, and `git_only` is
gone with it: release-plz measures a release against the registry rather
than against the last Git tag.

## Tags and verification

- `vX.Y.Z` pins both the action and its default CLI version and is created
  by release-plz.
- `v0`, `v1`, and later major tags track releases of that same CLI major.
  `release.yml` moves the matching tag after publishing the binaries.
  Major tags are excluded from the release trigger and changelog.

The former `v1` alias for 0.x releases is no longer advanced. Users of
that alias should switch to `v0` or an exact release tag; `v1` is reserved
for CLI 1.x releases.

After publication, check the workflow result, the platform assets, and
the release's packslip. Use the [verification guide](https://packslip.dev/docs/verifying/)
to check a downloaded artifact against the repository identity.
For local builds and documentation generation, see
[CONTRIBUTING.md](CONTRIBUTING.md).
