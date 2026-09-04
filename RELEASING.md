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
3. If `RELEASE_PLZ_RELEASE` is `true`, the release job creates the
   `vX.Y.Z` tag. When crate publishing is enabled in `release-plz.toml`,
   it publishes to crates.io before tagging.
4. The tag triggers `release.yml`, which checks that the tag matches
   `Cargo.toml` and builds five platform binaries,
   attests them, and creates a draft GitHub release. It generates narrative
   notes with Communiqué, publishes the release's packslip, publishes the
   GitHub release, and moves the action's matching major tag (`v0` for
   0.x releases, `v1` for 1.x releases, and so on).

`publish = false` and `git_only = true` currently keep release-plz working
from Git tags without publishing the crate. See the registry setup below
before enabling crate publication.

The action reads its default CLI version from its own `Cargo.toml`, so
the release PR's version bump also updates that default. The action and CLI
share one version, including for action-only changes. `action.yml` is
included in the Cargo package so release-plz detects those changes.
Use conventional commits such as `fix(action): ...` or `feat(action): ...`;
breaking action changes affect the shared version too.

## Repository setup

| Setting | Purpose |
| --- | --- |
| Secret `RELEASE_PLZ_TOKEN` | Fine-grained token with repository contents and pull-request write permissions. The workflows use it so generated PRs and tags can trigger subsequent workflows. |
| Secret `ANTHROPIC_API_KEY` | Lets Communiqué generate release notes. If generation fails or the key is unavailable, publication continues with GitHub's generated notes. |
| Variable `RELEASE_PLZ_RELEASE=true` | Enables the release job. Without it, merging the release PR does not publish a release. |

Communiqué's context and tone are configured in `communique.toml`.
Its version is declared in `mise.toml` and resolved in `mise.lock`;
update the lock deliberately with `mise lock`.

## Enable crates.io publication

The first crate publication is manual. release-plz cannot plan against a
missing registry crate when a version tag already exists.

1. Run `cargo publish` from the intended tagged commit using a crates.io
   API token.
2. Register a Trusted Publisher for repository `jdx/packslip` and workflow
   `release-plz.yml`.
3. Set `publish = true` in `release-plz.toml` and review its `git_only`
   setting for the intended release baseline.
4. Enable `RELEASE_PLZ_RELEASE` when the repository is ready to publish
   merged release PRs.

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
