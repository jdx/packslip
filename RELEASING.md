# Releasing packslip

Maintainers release by reviewing and merging the release PR maintained by
release-plz. An ordinary push to `main` updates that PR; it does not by
itself publish a version. Publication also requires the release job to be
enabled (see [Repository setup](#repository-setup)). For local builds and
documentation generation, see [CONTRIBUTING.md](CONTRIBUTING.md).

## Release sequence

1. A push to `main` runs `release-plz update`, which writes a
   `Cargo.toml` version bump and a changelog generated from conventional
   commits through `cliff.toml`. The workflow then regenerates the CLI
   documentation against that version and pushes all of it as one commit
   to the `release-plz` branch, creating or updating the
   `chore: release vX.Y.Z` PR. The PR never exists in a state without the
   regenerated documentation, so merging it at any moment ships a CLI
   reference that matches the version.
2. A maintainer reviews and merges the PR.
3. If `RELEASE_PLZ_RELEASE` is `true`, the `release` job in
   `release-plz.yml` publishes the crate to crates.io and then creates the
   `vX.Y.Z` tag.
4. The tag starts `release.yml`, which:
   - checks that the tag matches the version in `Cargo.toml`;
   - builds each of the five platforms' archive and its executable on its
     own (see [Platforms](#platforms)), and signs and notarizes the macOS
     binary;
   - adds the usage spec, the man page, and bash, zsh, fish, and
     PowerShell completions;
   - writes the install scripts `install.sh` and `install.ps1` with
     `installer/render.sh`, which fills in the SHA-256 of each executable,
     and attests every release file;
   - creates a draft GitHub release and rewrites its notes with
     Communiqué, keeping GitHub's generated notes if that step fails;
   - signs the release's packslip as the project `packslip.dev`, uploads
     the files and the bundle to the R2 bucket behind packslip.dev, and
     attaches the bundle to the GitHub release;
   - publishes the GitHub release and moves the action's major tag (`v1`
     for 1.x releases), creating the tag if it does not exist yet;
   - pushes the container image `ghcr.io/jdx/packslip` for linux/amd64 and
     linux/arm64, built from the release's own executables, and attests it
     (see [Container image](#container-image));
   - calls `packslip-releases.yml` to rebuild the signed release list at
     `https://packslip.dev/.well-known/packslip.json`. That workflow also
     re-signs the list every Monday, before its 30-day validity runs out,
     and every rebuild applies the withdrawals committed under
     `.github/packslip/`; see
     [Withdraw a release or mark a security fix](#withdraw-a-release-or-mark-a-security-fix).
     Once the list names the release, [packslip.sh](#packslipsh) serves
     its install scripts.

## After a release

Check the workflow result, the platform assets on the GitHub release, and
the release's packslip. `packslip.dev` names a host, not a GitHub
repository, so `packslip verify` has no identity to derive from it. Pass
the identity of jdx/packslip's GitHub Actions workflows: `release.yml`
signs each release's packslip, and `packslip-releases.yml` signs the list.
`--identity-prefix https://github.com/jdx/packslip/` accepts any workflow
in the repository, so both checks below use it.

```sh
v=X.Y.Z
curl -fsSLO https://packslip.dev/v$v/packslip.sigstore.json
curl -fsSLO https://packslip.dev/v$v/packslip-v$v-linux-x64.tar.xz
packslip verify packslip.sigstore.json \
  --identity-prefix https://github.com/jdx/packslip/ \
  --issuer https://token.actions.githubusercontent.com \
  --artifact packslip-v$v-linux-x64.tar.xz
```

Then confirm that the signed release list names the new version:

```sh
curl -fsSL https://packslip.dev/.well-known/packslip.json -o list.json
packslip verify list.json \
  --identity-prefix https://github.com/jdx/packslip/ \
  --issuer https://token.actions.githubusercontent.com
packslip show list.json | jq -r '.predicate.releases[].version'
```

[Verify a release](https://packslip.dev/docs/verifying/) explains what
verification checks and what its output means.

Last, confirm that packslip.sh serves the new release's install script.
`content-location` names the release it came from:

```sh
curl -fsSI https://packslip.sh | grep -i '^content-location'
```

And that the container image runs the new release and carries its
attestation:

```sh
docker run --rm ghcr.io/jdx/packslip:$v version
gh attestation verify oci://ghcr.io/jdx/packslip:$v --repo jdx/packslip
```

## packslip.sh

`https://packslip.sh` serves the install scripts each release publishes,
from the release files in R2. The Worker is `packslip-sh`, configured in
`cloudflare/installer/` and deployed by `site.yml` on every push to
`main`:

| Path | Serves |
| --- | --- |
| `/`, `/install.sh` | `install.sh` of the latest release |
| `/install.ps1` | `install.ps1` of the latest release |
| `/vX.Y.Z`, `/vX.Y.Z/install.sh` | `install.sh` of that release |
| `/vX.Y.Z/install.ps1` | `install.ps1` of that release |

The latest release is the one the signed release list recommends with
`latest`, or else its highest release that is neither withdrawn nor a
prerelease, so a withdrawal reaches packslip.sh with the next list. Each
Cloudflare data center caches a script it served, a release's own copy for
good and the latest for five minutes, so repeated installs read nothing
from R2; every request still runs the Worker once. A
release from before the install scripts existed has none, and its paths
answer 404. Every client gets the same bytes from a URL: the Worker reads
the user agent only to count downloads, so a script a browser shows is
the one a shell runs, and a checksum taken of a versioned URL holds for
everyone. Each script pins the executables by SHA-256, so its own
checksum pins the release for every platform, for example in a
Dockerfile for an image that has curl or wget:

```dockerfile
ADD --checksum=sha256:<sha256 of install.sh> https://packslip.sh/vX.Y.Z /tmp/install-packslip.sh
RUN sh /tmp/install-packslip.sh
```

The domain was set up once, by hand: the `packslip.sh` zone is in the
same Cloudflare account as `jdx-releases`, with Always Use HTTPS on, and
`packslip.sh` is attached to the `packslip-sh` Worker as a custom
domain. Like packslip.dev's, the attachment outlives deploys, so the
deploy token needs no zone access. The Worker redirects plain HTTP to
HTTPS itself as well, so a script is never served over HTTP even if that
setting is turned off.

## Container image

`ghcr.io/jdx/packslip` holds a release's static Linux executable at
`/packslip`, with a CA bundle at `/etc/ssl/certs/ca-certificates.crt`, on
an otherwise empty image for linux/amd64 and linux/arm64. The release job's
`image` job builds it with `container/build.sh` from the executables the
build job made, not from source, so it runs the same bytes as the release's
own downloads. It pushes the tags `X.Y.Z`, `X.Y`, `X`, and `latest`, and
attests the image's digest the way the release files are attested.

One digest pins both architectures, and Docker checks it, so the image a
Dockerfile copies into needs no curl, tar, or hash tool:

```dockerfile
COPY --from=ghcr.io/jdx/packslip:X.Y.Z@sha256:<digest> /packslip /usr/local/bin/packslip
```

CI's `image` job builds the image the same way from the latest release's
archives, runs it, and copies it into a Debian slim image, so a change to
`container/` is tested before a release depends on it. The first release
that pushes the image creates the package; if GitHub created it private,
make it public once in its package settings.

## Action versions and tags

The actions read their default CLI version from the repository's
`Cargo.toml`, so the release PR's version bump also updates that default.
The actions and CLI share one version, including for action-only changes.
release-plz counts only pull requests that change a file in the Cargo
package. `action.yml`, `releases/action.yml`, and `scripts/` are in the
package so that action changes count. The `exclude` list in `Cargo.toml`
leaves out the site, `.github/`, `RELEASING.md`, `CHANGELOG.md`,
`cliff.toml`, and `release-plz.toml`; a pull request that changes only
those files gets no changelog line and does not raise the version.
`communique.toml`, `mise.toml`, and `mise.lock` are in the package, so a
change to them counts, as a change under `scripts/` does.

Give action changes conventional-commit titles such as `fix(action): ...`
or `feat(action): ...`. A breaking action change raises the shared major
version too (see [Major versions](#major-versions)).

- `vX.Y.Z` pins both the action and its default CLI version and is created
  by release-plz.
- `v0`, `v1`, and later major tags track releases of that same CLI major.
  `release.yml` moves the matching tag after publishing the binaries.
  Major tags are excluded from the release trigger and changelog.

`v1` was briefly an alias for 0.x releases. From 1.0.0 it means what it
says, and `release.yml` moves it with each 1.x release. `v0` stops
advancing at 0.3.1: a workflow pinned to `@v0` keeps working and stops
receiving updates until it moves to `@v1`.

## Major versions

From 1.0.0, release-plz proposes the next major version itself. It does
so when a merged pull request that changes a packaged file (see
[Action versions and tags](#action-versions-and-tags)) carries `!` in its
title or a `BREAKING CHANGE:` footer in its description, or when
cargo-semver-checks (`semver_check = true` in `release-plz.toml`) reports
a public API break. cargo-semver-checks sees only the Rust API, so mark a
breaking change to the CLI or an action with `!` yourself. The action and
CLI share one version, so either kind of break makes a major release of
both. After that release, `release.yml` creates the `v2` tag, and workflows
pinned to `@v1` stop receiving updates until they move. The format stays
at version 1 either way.

### Publishing a version release-plz would not propose

To publish a version release-plz would not propose, open one pull request
that carries all three of the following. A version bump alone is not
enough: the `release` job publishes any version on `main` that crates.io
does not have yet, so a bare bump would publish with a changelog that
stops at the previous release.

1. `version` in `Cargo.toml`, and `Cargo.lock` updated with
   `cargo update -p packslip`.
2. The `CHANGELOG.md` entry for the new version, in the shape `cliff.toml`
   renders: the compare link, the date, and one line per change.
3. The change that calls for the new version.

Merging that PR publishes the version and pushes its tag, with no release
PR in between. The release-pr job closes a stray release PR when nothing
is left to release; confirm that none is still open.

1.0.0 was cut this way from 0.3.1. On a 0.x version release-plz treats a
breaking change as a minor bump, so it would have proposed 0.4.0, and no
configuration overrides that.

## Withdraw a release or mark a security fix

`packslip-releases.yml` builds the signed list from scratch on every run, so
a withdrawal only lasts if every run repeats it. Two files in
`.github/packslip/` hold them, and each run reads both from `main`, whatever
ref triggered it:

| File | One line per release | Effect on the list |
| --- | --- | --- |
| `yanked` | `TAG=REASON`, such as `v1.2.3=Incorrect Linux archive` | The release stays listed with status `yanked` and the reason. Consumers never select it and warn anyone who already has it. |
| `security` | `TAG`, such as `v1.2.4` | The release is listed with `security: true`, which lets a consumer's minimum release age shorten for it. |

Blank lines and lines starting with `#` are ignored, so put the reason for a
security mark in a comment above its line. The action also accepts a bundle
URL in place of a tag.

1. Add the line to the file in a pull request titled like
   `fix(ci): withdraw v1.2.3 from the packslip.dev list`.
2. Merge it. A push to `main` that touches `.github/packslip/` runs
   `packslip-releases.yml`, which signs and publishes a new list. Watch that
   run finish; the weekly schedule and every later release then keep the
   entry.
3. Check the published list:

   ```sh
   curl -fsS https://packslip.dev/.well-known/packslip.json -o packslip.json
   packslip verify packslip.json \
     --issuer https://token.actions.githubusercontent.com \
     --identity-prefix https://github.com/jdx/packslip/ --json |
     jq '.predicate.releases[] | select(.status == "yanked" or .security)'
   ```

To restore a release or drop a security mark, remove its line the same way.

Keep the release's bundle in the R2 bucket: the list has to keep naming a
withdrawn release, and a line for a tag with no bundle fails the run with
`is not among the --release entries`. A missing file also fails the run, so
a rename or deletion cannot silently restore everything.

This withdraws the release from the packslip.dev list only. It does not
touch the GitHub release, the crates.io version, or the files in R2.
`packslip-releases.yml` no longer takes `yank` or `security` dispatch inputs:
a dispatched withdrawal lasted until the next scheduled or post-release run
rebuilt the list. A manual dispatch now re-signs the list as it stands.

## Backfill an earlier release

A release that shipped before packslip.dev hosted packslip's releases can
be published there afterward. Run `release.yml` by hand with
`backfill-tag` set to its tag:

```sh
gh workflow run release.yml -f backfill-tag=vX.Y.Z
```

The job signs a new packslip as `packslip.dev` for the release's existing
GitHub assets, copies the assets and the packslip to packslip.dev, and
then rebuilds the list. It runs from the same workflow file that signs new
releases, so a consumer sees one signer throughout.

## Platforms

| Asset           | Target                       | Runner             |
| --------------- | ---------------------------- | ------------------ |
| `linux-x64`     | `x86_64-unknown-linux-musl`  | `ubuntu-latest`    |
| `linux-arm64`   | `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` |
| `darwin-arm64`  | `aarch64-apple-darwin`       | `macos-latest`     |
| `windows-x64`   | `x86_64-pc-windows-msvc`     | `windows-latest`   |
| `windows-arm64` | `aarch64-pc-windows-msvc`    | `windows-11-arm`   |

Each platform ships twice: as an archive (`.tar.xz`, or `.zip` on Windows)
and as the executable alone (`packslip-vX.Y.Z-<asset>`, with `.exe` on
Windows). Package managers take the archive. The install scripts download
the executable alone, because many container images cannot unpack an
archive: Debian and Ubuntu images have no `xz` for GNU tar to call, and
Amazon Linux 2023 and UBI minimal images have no tar at all.

There is no `darwin-x64` asset, so Intel Macs have no prebuilt binary:
Rosetta 2 runs x86_64 code on Apple silicon, not arm64 code on Intel.
Signing, notarizing, and supporting a second macOS artifact for a
shrinking set of machines is not worth it. Intel Mac users install with
`cargo install packslip --locked`, and on an Intel macOS runner the action
stops with instructions to build the CLI and pass `packslip-path`.

Windows arm64 is built on a native arm64 runner rather than cross-compiled,
because `aws-lc-sys` — the crypto behind rustls — compiles C and assembly
for the host toolchain.

Both Windows builds link the C runtime statically: `.cargo/config.toml`
sets `+crt-static` for MSVC targets, and `aws-lc-sys` compiles its C
against the matching static runtime. `packslip.exe` therefore starts on a
machine without the Visual C++ Redistributable, such as Windows Sandbox
or a Server Core container, and the release's packslip records an empty
`requires.libs` for the Windows archives. A `RUSTFLAGS` variable in the
build job would replace the setting and bring back the
`vcruntime140.dll` dependency.

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
| Secrets `APPLE_API_KEY_P8`, `APPLE_API_KEY_ID`, `APPLE_API_ISSUER_ID` | A base64-encoded App Store Connect API key and its key and issuer IDs. The signing step checks all three before it signs and fails with "Notarization credentials missing" rather than shipping an unnotarized binary. |
| Secrets `CLOUDFLARE_ACCESS_KEY_ID`, `CLOUDFLARE_SECRET_ACCESS_KEY` | S3 credentials for the `jdx-releases` R2 bucket, from an R2 token scoped to that bucket alone. The release job and `packslip-releases.yml` write the release files, bundles, and list under `packslip/`. |
| Secret `CLOUDFLARE_TOKEN` | An API token with account `Workers Scripts:Edit` and read on the `jdx-releases` bucket, and no zone access at all. `site.yml` deploys packslip.dev and packslip.sh with it. The custom domains are attached to the Workers by hand rather than by wrangler, so deploys never need `DNS:Edit`. |
| Secrets `MISE_LOCK_APP_ID`, `MISE_LOCK_APP_PRIVATE_KEY` | A GitHub App that `mise-lock.yml` uses to push a regenerated `mise.lock`, and the files `mise run render` changes with it, to Renovate branches. |

Communiqué's context and tone are configured in `communique.toml`.
Its version is declared in `mise.toml` and resolved in `mise.lock`;
update the lock deliberately with `mise lock`.

## crates.io publication

crates.io needs no stored credential. 0.2.0 was published by hand from its
tag to create the crate — Trusted Publishing cannot create one — and a
Trusted Publisher is registered for repository `jdx/packslip`, workflow
`release-plz.yml`. The release job mints a short-lived token through OIDC
with `rust-lang/crates-io-auth-action`, so the API token used for that
first publish was revoked immediately afterward.

`release-plz.toml` therefore sets `publish = true` and leaves `git_only`
unset. release-plz decides whether there is anything to release by
comparing `main` with the latest version on crates.io, not with the last
Git tag.
