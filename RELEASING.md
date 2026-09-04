# Releasing

Releases are manual and driven by [release-plz](https://release-plz.dev).
Nothing is released by pushing to `main`.

## How a release happens

1. Every push to `main` runs `release-plz release-pr`, which keeps one
   pull request titled `chore: release vX.Y.Z` up to date: the version
   bump in `Cargo.toml` and the changelog rendered from conventional
   commits through `cliff.toml`.
2. A human reviews and merges that PR. Nothing else is needed.
3. On the merge, `release-plz release` pushes the `vX.Y.Z` tag, and, once
   `publish = true` in `release-plz.toml`, publishes the crate to
   crates.io first.
4. The tag starts `release.yml`, which builds the five binaries, attests
   them, creates a draft GitHub release, rewrites its generated notes with
   [Communiqué](https://github.com/jdx/communique), publishes the release's
   own packslip, publishes the draft, and moves the matching major tag
   (`v0` for a `0.x.y` release, `v1` for `1.x.y`, and so on). Before
   building, it checks that the tag matches `Cargo.toml`.

Communiqué's project context and tone instructions live in
`communique.toml`. Its version is declared in `mise.toml` and resolved in
`mise.lock`; update the lock deliberately with `mise lock`. If note generation
fails, the release keeps GitHub's generated notes and continues.

The action reads its default packslip version from its own `Cargo.toml`,
so the release PR bumps it too; no other file needs editing. `action.yml`
is included in the Cargo package so release-plz detects action-only changes
and proposes a shared release for them too. Use conventional commits such
as `fix(action): ...` or `feat(action): ...`; breaking action changes are
breaking changes to the shared version.

An exact action tag such as `@v0.2.0` uses CLI 0.2.0 by default. The
`packslip-version` input is an explicit escape hatch, not a separate action
version. Moving major tags advance only after the CLI assets are published.

## One-time setup

- `RELEASE_PLZ_TOKEN`: a fine-grained personal access token for this
  repository with `contents: write` and `pull-requests: write`, stored as
  a repository secret. The built-in `GITHUB_TOKEN` cannot be used: pushes
  and pull requests it makes start no workflows.
- `ANTHROPIC_API_KEY`: the API key Communiqué uses to generate narrative
  release notes. Without it, releases fall back to GitHub's generated notes.
- Set the repository variable `RELEASE_PLZ_RELEASE` to `true` to let the
  `release` job run. Until it is set, merging the release PR does nothing,
  so enabling the workflow cannot release on its own.
- crates.io, when wanted: release-plz refuses to plan a release while the
  crate is missing from the registry but a version tag exists, so the
  first publish is manual: `cargo publish` from the tagged commit with an
  API token. Then register a Trusted Publisher for the crate (repository
  `jdx/packslip`, workflow `release-plz.yml`), and flip `publish = false`
  to `true` in `release-plz.toml`. From then on every merged release PR
  publishes.

## Tags

- `vX.Y.Z`: one shared action and CLI tag per release, pushed by release-plz.
- `vX`: the matching major tag, moved by `release.yml` after publication.
  It is excluded from `release.yml`'s trigger and from the changelog.
- The former `v1` alias for 0.x releases is no longer advanced by 0.x
  releases. Consumers of that alias should switch to `v0` or an exact
  version; `v1` is reserved for CLI 1.x releases.
