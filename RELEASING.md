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
   own packslip, publishes the draft, and moves the `v1` tag so
   `uses: jdx/packslip@v1` follows.

Communiqué's project context and tone instructions live in
`communique.toml`. Its version is declared in `mise.toml` and resolved in
`mise.lock`; update the lock deliberately with `mise lock`. If note generation
fails, the release keeps GitHub's generated notes and continues.

The action reads its default packslip version from its own `Cargo.toml`,
so the release PR bumps it too; no other file needs editing.

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

- `vX.Y.Z`: one per release, pushed by release-plz.
- `v1`: the action's major tag, moved by `release.yml` after each release.
  It is excluded from `release.yml`'s trigger and from the changelog.
