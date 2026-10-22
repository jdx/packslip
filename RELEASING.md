# Releasing packslip

This runbook is for jdx/packslip maintainers who ship, check, or recover a
release; local builds and pull request titles are in
[CONTRIBUTING.md](CONTRIBUTING.md). The crate, CLI, both GitHub Actions, npm
package, and Go module share one version, and every release publishes them
all.

## Ship the next release

An ordinary release needs no action. release-plz keeps a release pull request
open from the `release-plz` branch, `release.yml` stages its candidate, and a
daily workflow merges the pull request once `final` passes
([How a release works](#how-a-release-works)).

### Daily auto-merge

`auto-merge-release.yml` runs daily at 10:00 UTC, only in jdx/packslip. A
scheduled run merges only when both cadence guards pass:

- The newest release tag on `main`
  (`git describe --tags --match 'v[0-9]*' --abbrev=0`) is at least two days
  old, or none exists.
- A commit subject since that tag starts with `fix` or `feat`, with an
  optional scope and `!`.

Every run, scheduled or dispatched, also needs an open pull request from
jdx/packslip's `release-plz` branch with a non-empty body. It then enables
auto-merge (`gh pr merge --squash --auto`) with `RELEASE_PLZ_TOKEN`.

Even `fix(ci): ...` makes a waiting release eligible, so type changes that
need no release `ci:`, `docs:`, or `chore:`. `security:`, `perf:`, and
`refactor:` changes alone never trigger a scheduled merge; to ship a
`security:` fix promptly, dispatch or merge by hand.

### Merge or hold the release pull request

To release sooner, merge once `staged-release` and `final` pass, or dispatch
the workflow, which skips the cadence guards:

```sh
gh workflow run auto-merge-release.yml -R jdx/packslip
```

**Warning:** Never merge past a failing `final`, even as an admin. A release
merged before its candidate staged has no digest map, and
[the publication gate holds it](#the-publication-gate-held-a-merged-release).

To hold a release, cancel auto-merge, which survives release-plz refreshes,
and pause the workflow:

```sh
pr=$(gh pr list -R jdx/packslip --head release-plz --state open --json number --jq '.[0].number')
gh pr merge "$pr" -R jdx/packslip --disable-auto
gh workflow disable auto-merge-release.yml -R jdx/packslip
```

Resume with `gh workflow enable auto-merge-release.yml -R jdx/packslip`.

When release-plz proposes a version other than the one
`docs/actions-since.toml` gives an unreleased input or output, correct the
entry on `main` before the release pull request merges, and run
`mise run render`. `release-pr` then rebuilds the release pull request with
it; a commit pushed only to `release-plz` would not survive that rebuild.

## Check a release

Once the tag run finishes (step 1 sets `v`, which later steps reuse):

1. Confirm that the run and each job succeeded:

   ```sh
   v=X.Y.Z
   run=$(gh run list -R jdx/packslip --workflow release.yml --branch v$v --limit 1 --json databaseId --jq '.[0].databaseId')
   gh run view "$run" -R jdx/packslip
   ```

2. Verify the files. A domain project such as packslip.dev needs a signer
   policy. The prefix covers `release.yml`, which signs bundles, and
   `packslip-releases.yml`, which signs the list. The signer fingerprint
   requires a run in jdx/packslip itself, which the prefix alone does not:

   ```sh
   curl -fsSLO https://packslip.dev/v$v/packslip.sigstore.json
   curl -fsSLO https://packslip.dev/v$v/packslip-v$v-linux-x64.tar.xz
   packslip verify packslip.sigstore.json \
     --identity-prefix https://github.com/jdx/packslip/ \
     --issuer https://token.actions.githubusercontent.com \
     --pin ps1_mcx64bcghek2t4vljgb3lho3ti \
     --artifact packslip-v$v-linux-x64.tar.xz
   ```

   The first line starts with `ok: packslip.dev X.Y.Z` and names
   `https://github.com/jdx/packslip/.github/workflows/release.yml@refs/tags/vX.Y.Z`
   as signer.

3. Confirm that the signed release list names the version; no output means
   it does not:

   ```sh
   curl -fsSL https://packslip.dev/.well-known/packslip.json -o packslip.json
   packslip verify packslip.json \
     --identity-prefix https://github.com/jdx/packslip/ \
     --issuer https://token.actions.githubusercontent.com \
     --pin ps1_mcx64bcghek2t4vljgb3lho3ti --json |
     jq -r '.predicate.releases[].version' | grep -Fx "$v"
   ```

4. Confirm that packslip.sh's `content-location` header names the release,
   such as `/vX.Y.Z`. Allow five minutes after the `list` job for the cache:

   ```sh
   curl -fsSI https://packslip.sh | grep -i '^content-location'
   ```

5. Confirm the crate (`packslip = "X.Y.Z"`) and npm's `latest` dist-tag:

   ```sh
   cargo search packslip --limit 1
   npm view packslip dist-tags
   ```

6. Confirm that the Go proxy serves the version and that the release, Go,
   and major tags share a commit:

   ```sh
   curl -fsS https://proxy.golang.org/github.com/jdx/packslip/go/@v/v$v.info
   for tag in v$v go/v$v v${v%%.*}; do
     echo "$tag $(gh api repos/jdx/packslip/commits/$tag --jq .sha)"
   done
   ```

7. Confirm that the image runs and is attested, and compare its tags.
   `X.Y.Z` is always pushed; `X.Y` and `X` move only if no newer stable
   release of that major is published, and `latest` only if none is newer
   at all.

   ```sh
   docker run --rm ghcr.io/jdx/packslip:$v version
   gh attestation verify oci://ghcr.io/jdx/packslip:$v --repo jdx/packslip
   for tag in $v ${v%.*} ${v%%.*} latest; do
     echo "$tag $(docker buildx imagetools inspect ghcr.io/jdx/packslip:$tag --format '{{.Manifest.Digest}}')"
   done
   ```

8. If distribution publishing is on, confirm the `distro-source / ...` and
   `package-repositories / ...` jobs, which submit only for the newest stable
   release, and
   [the PPA's builds](https://launchpad.net/~jdxcode/+archive/ubuntu/packslip/+builds):
   Launchpad can accept an upload without building and publishing both
   architectures. Test the APT and RPM setup in
   [Install packslip](https://packslip.dev/docs/install/) on a native machine.

9. Confirm that the notes no longer read
   `Candidate release; awaiting release PR review and verification.`
   ([Release notes are missing](#release-notes-are-missing)):

   ```sh
   gh release view v$v -R jdx/packslip --json body --jq .body | head -n 20
   ```

10. Confirm that `site.yml` deployed packslip.dev and the
    [CLI reference](https://packslip.dev/cli/), and that no release pull
    request is open:

    ```sh
    gh run list -R jdx/packslip --workflow site.yml --branch main --limit 1
    gh pr list -R jdx/packslip --head release-plz --state open
    ```

## Withdraw a release or mark a security fix

`packslip-releases.yml` rebuilds packslip.dev's signed list from scratch on
every run, from two files in `.github/packslip/` read from `main` whatever ref
triggered it.
Each line names a release by tag or bundle URL; blank lines and `#` comments
are ignored.

- `yanked`: `TAG=REASON`, such as `v1.2.3=Incorrect Linux archive`. The
  release stays listed as `yanked` with the reason, and consumers never
  select it. `packslip install` warns users who still have it installed the
  next time they run it for packslip.dev; mise does not warn.
- `security`: `TAG`, such as `v1.2.4`, with the reason in a comment above;
  add it once the fix's release is published. The listed `security: true` lets
  consumers shorten their minimum release age.

1. Add the line in a pull request typed other than `fix` or `feat`, such as
   `ci: withdraw v1.2.3 from the packslip.dev list`, so the
   [daily auto-merge](#daily-auto-merge) does not merge a waiting release.
2. Merge it, which runs `packslip-releases.yml`; the weekly run and later
   releases keep the entry:

   ```sh
   gh run list -R jdx/packslip --workflow packslip-releases.yml --limit 1
   ```

3. Check the list:

   ```sh
   curl -fsSL https://packslip.dev/.well-known/packslip.json -o packslip.json
   packslip verify packslip.json \
     --identity-prefix https://github.com/jdx/packslip/ \
     --issuer https://token.actions.githubusercontent.com \
     --pin ps1_mcx64bcghek2t4vljgb3lho3ti --json |
     jq '.predicate.releases[] | select(.status == "yanked" or .security)'
   ```

Remove a line the same way to undo it. A dispatch
(`gh workflow run packslip-releases.yml -R jdx/packslip`) takes no withdrawal
inputs; it re-signs the list as it stands.

Keep withdrawn releases' bundles in R2: a line for a tag with no bundle fails
the run with `is not among the --release entries`. A missing `yanked` or
`security` file also fails it, so deleting one cannot silently restore every
withdrawn release.

Only the packslip.dev list changes, and with it packslip.sh's latest script.
The GitHub release, crates.io version, R2 files, npm `latest`, container
tags, Go module version, and APT and RPM repositories stay. Elsewhere, run
`cargo yank packslip --version X.Y.Z` and
`npm deprecate packslip@X.Y.Z "<reason>"` from your own accounts, or add a
`retract` directive to `go/go.mod` for the next release.

## Publishing a version release-plz would not propose

Use this when release-plz would propose the wrong version. On 0.x it treats a
breaking change as a minor bump, and no setting overrides that, so 1.0.0 was
published this way instead of the 0.4.0 it would have proposed after 0.3.1.

**Warning:** Use the `release-plz` branch. A bump from another branch has no
candidate metadata, so the publication gate fails, and CI misses it:
`staged-release` runs only on the `release-plz` pull request, and `final`
passes without it.

Merge nothing else into `main` meanwhile, or `release-pr` replaces or closes
the proposal. Merge it yourself in step 5, or a scheduled auto-merge takes it
once its cadence guards pass;
[hold it](#merge-or-hold-the-release-pull-request) to review first.

1. From current `main`, commit together:
   - `version` in `Cargo.toml`, and `Cargo.lock` from
     `cargo update -p packslip`.
   - A `CHANGELOG.md` entry in the latest entry's `cliff.toml` layout:
     `## [X.Y.Z](<compare link>) - YYYY-MM-DD`, `###` change-type headings,
     one linked `- **(scope)** subject by @user in #N` line per change, and
     the closing `<!-- generated by git-cliff -->` line.
   - The change that calls for the version.
2. Run `mise run render` and commit the result.
3. Push to `release-plz`:

   ```sh
   git push --force origin HEAD:refs/heads/release-plz
   ```

4. Write the changelog section to `entry.md`, the pull request body the
   auto-merge requires:

   ```sh
   awk -v v=X.Y.Z '/^## \[/ { p = index($0, "[" v "]") == 4 } p && !/generated by git-cliff/' CHANGELOG.md > entry.md
   ```

   If a release pull request is open, the push kept its title and body;
   replace them:

   ```sh
   pr=$(gh pr list -R jdx/packslip --head release-plz --state open --json number --jq '.[0].number')
   gh pr edit "$pr" -R jdx/packslip --title "chore: release vX.Y.Z" --body-file entry.md
   ```

   Otherwise, open one:

   ```sh
   gh pr create -R jdx/packslip --head release-plz --base main \
     --title "chore: release vX.Y.Z" --body-file entry.md --label release
   ```

5. Merge once the candidate run commits the digest map and `staged-release`
   and `final` pass. The merge publishes the crate and tag; promotion reuses
   the verified candidate bytes.
6. Confirm that no stray release pull request is open; `release-pr` closes
   one when nothing is left to release.

## Major versions

release-plz proposes a major version when a merged pull request changing a
packaged file has `!` in its title or a `BREAKING CHANGE:` footer, or when
cargo-semver-checks (`semver_check = true` in `release-plz.toml`) reports a
public API break. That check sees only the Rust API, so mark breaking CLI or
action changes with `!`; the two share one version, so a break in either
makes a major release of both. The release format stays at version 1.

For the first release of a new major, such as 2.0.0:

1. Add its tag, such as `refs/tags/v2`, to the `protect-tags` exclusions,
   because `release.yml` moves major tags with `github.token`, which no
   bypass role covers. This needs an admin login
   ([why `env -u`](#the-major-tag-did-not-move)):

   ```sh
   env -u GH_TOKEN -u GITHUB_TOKEN gh api repos/jdx/packslip/rulesets/24686022 --jq .conditions.ref_name
   env -u GH_TOKEN -u GITHUB_TOKEN gh api -X PUT repos/jdx/packslip/rulesets/24686022 --input - <<'EOF'
   {"conditions": {"ref_name": {"include": ["~ALL"], "exclude": ["refs/tags/v1", "refs/tags/v2", "refs/tags/go/v*"]}}}
   EOF
   ```

2. Before the release pull request merges, give the Go module its `/v2` path
   in one pull request. Otherwise the proxy rejects the `go/v2.0.0` tag that
   `go-module-tag` creates regardless, with
   `module contains a go.mod file, so module path must match major version`.
   1. Change the first line of `go/go.mod` to
      `module github.com/jdx/packslip/go/v2`.
   2. Update every other file that
      `git grep -l 'github.com/jdx/packslip/go' -- ':!content/spec.md' ':!CHANGELOG.md'`
      prints: imports, `go get` commands, the path in
      `docs/spec/packslip.md`, and the proxy URL in step 6 of
      [Check a release](#check-a-release).
   3. Run `mise run render` to regenerate `content/spec.md`.

   The tag prefix stays `go/`; Go users import
   `github.com/jdx/packslip/go/v2` for 2.x.
3. Let the release ship. `release.yml` creates `v2` at the release commit;
   workflows pinned to `@v1` get no updates until they move.
4. Only once `v2` exists, move the `@v1` action examples and SECURITY.md's
   supported versions to it, because `site.yml` deploys every merge. List
   the lines with:

   ```sh
   git grep -nE 'jdx/packslip(/releases)?@v1([^.0-9]|$)|1\.x' -- ':!CHANGELOG.md'
   ```

## Go-only and npm-only releases

A change to `go/`, `packages/`, or `verifier/` alone waits for the next
release ([What counts toward a release](#what-counts-toward-a-release)). To
ship it sooner:

1. Merge a pull request that also changes a packaged file: the Go or npm
   pull request itself, or a later one, such as a `README.md` line
   documenting it.
2. Title it with the user-visible change, such as `fix(go): ...` or
   `fix(npm): ...`; the title becomes the changelog line, and Communiqué
   reads the description.
3. Let the release ship. Its tag run creates `go/vX.Y.Z` and publishes the
   npm package.

[Publishing a version release-plz would not propose](#publishing-a-version-release-plz-would-not-propose)
also works, with more steps. Also:

- Never create a `go/v*` tag by hand: `go-module-tag` fails when
  `go/vX.Y.Z` exists at another commit.
- Only `release.yml` publishes npm versions; the package disallows tokens.
- Projects that copy `verifier/packslip-verify.mjs` from a release tag, not
  a commit, get changes only with the next release, so ship verifier
  security fixes in a release.

## Backfill an earlier release

To publish at packslip.dev a release from before packslip.dev hosted
packslip:

```sh
gh workflow run release.yml -R jdx/packslip -f backfill-tag=vX.Y.Z
```

The `backfill` job signs a bundle as packslip.dev for the release's GitHub
assets and copies both there; `backfill-list` rebuilds the list. It runs from
the workflow that signs new releases, so consumers see one signer.

## Recover a failed release

- Never move a version tag; to rebuild a stale candidate, refresh the release
  pull request before merging.
- Staging cannot replace a published release; inspect partial publication
  before retrying.
- Rerun a promotion canceled at the 100-run queue limit
  ([How a release works](#how-a-release-works)).
- Rerun failed jobs by run ID. For the tag run of `vX.Y.Z`:

  ```sh
  run=$(gh run list -R jdx/packslip --workflow release.yml --branch vX.Y.Z --limit 1 --json databaseId --jq '.[0].databaseId')
  gh run rerun "$run" -R jdx/packslip --failed
  ```

  For a candidate run, use `--branch release-plz`; for the `release` job,
  `--workflow release-plz.yml --branch main`.

### A candidate is stale or failed

Rerun the failed jobs of a candidate whose branch has not moved. A candidate
whose branch moved mid-build fails with
`release branch changed while candidate was building`; the newer push builds
its own.

If `main` moves after the build, `staged-release` fails with
`release candidate rejected: candidate source and release must differ ONLY in action/release.json`.
The same push makes `release-pr` rebuild the release commit without the
digest map and force-push it, so a fresh candidate builds. Dispatch
`release-plz.yml` only if that run failed or did not push:

```sh
gh run list -R jdx/packslip --workflow release-plz.yml --branch main --limit 3
gh workflow run release-plz.yml -R jdx/packslip
```

### Restore a draft file

The publication gate and tag promotion both refuse a draft file that differs
from the digest map, `action/release.json`. Restore it only from its original
bytes:

1. With `run` set to the failed run (the merge's `release-plz.yml` run or the
   tag run), check out its commit and download the archives and executables
   that the candidate run, the map's `run_id`, kept:

   ```sh
   ref=$(gh run view "$run" -R jdx/packslip --json headSha --jq .headSha)
   git fetch origin main
   git checkout --detach "$ref"
   gh run download "$(jq .run_id action/release.json)" -R jdx/packslip -D candidate
   mkdir -p files && cp candidate/*/* files/
   ```

2. On Linux x64, regenerate the CLI resources and install scripts, which no
   run keeps. The checkout matches the candidate source apart from the map:

   ```sh
   mkdir -p bin
   tar -xJf files/packslip-vX.Y.Z-linux-x64.tar.xz --strip-components=1 -C bin
   bin/packslip usage > files/packslip.usage.kdl
   cp packslip.1 files/packslip.1
   for shell in bash zsh fish powershell; do
     bin/packslip completion "$shell" > "files/packslip.$shell"
   done
   installer/render.sh X.Y.Z files
   ```

3. Set `file` to the file to restore. The upload runs only if its SHA-256
   matches the map; `--clobber` replaces the bad draft file:

   ```sh
   file=packslip.1
   test "$(sha256sum "files/$file" | cut -d ' ' -f 1)" = "$(jq -r --arg f "$file" '.assets[$f].sha256' action/release.json)" &&
     gh release upload vX.Y.Z "files/$file" -R jdx/packslip --clobber
   ```

### The publication gate held a merged release

The `release` job of `release-plz.yml` fails at "Gate new publication on the
staged release" and creates no tag. The merge's `release-pr` run finds
nothing to release, so nothing recovers on its own. The step prints
`release candidate rejected:` and usually one of:

| Error | Cause | Recovery |
| --- | --- | --- |
| `[Errno 2] No such file or directory: 'action/release.json'` | Merged before the candidate staged | Rebuild |
| `candidate source and release must differ ONLY in action/release.json` | The merge tree differs from the candidate source beyond the map, as after `main` moved | Rebuild |
| `candidate build run is not a successful trusted release-plz push` | The map's `run_id` is not a successful release-plz push run | Rebuild |
| `matching unpublished draft release is required` | No unpublished draft for the version | Rebuild |
| `draft asset inventory mismatch` | The draft's files differ from the map's 18 | Repair |
| `draft asset digest/size mismatch:` and a file name | That file was replaced | Repair |

To rebuild:

1. Merge nothing else into `main` until done, or `release-pr` replaces or
   closes the recovery pull request.
2. Push current `main` without its digest map to `release-plz`, so the
   candidate run rebuilds instead of only verifying a matching map. Wait for
   its `chore: pin release candidate archives` commit:

   ```sh
   git fetch origin main
   git checkout --detach origin/main
   git rm -q --ignore-unmatch action/release.json
   git commit --allow-empty -m "chore: rebuild the release candidate"
   git push --force origin HEAD:refs/heads/release-plz
   gh run list -R jdx/packslip --workflow release.yml --branch release-plz --limit 2
   ```

3. Open a pull request containing only the digest map, and merge it once
   `final` passes. The gate then passes, and the `release` job publishes the
   crate and creates the tag:

   ```sh
   gh pr create -R jdx/packslip --head release-plz --base main \
     --title "chore: pin release candidate archives for vX.Y.Z" \
     --body "Pins the vX.Y.Z candidate rebuilt from current main, so the release can publish."
   ```

To repair, [restore each replaced or missing file](#restore-a-draft-file),
delete each file the map does not list with
`gh release delete-asset vX.Y.Z <file> -R jdx/packslip --yes`, and rerun the
failed `release` job.

### Tag promotion failed

1. With `run` set to the tag run as in
   [Recover a failed release](#recover-a-failed-release), find the failing
   step:

   ```sh
   gh run view "$run" -R jdx/packslip --log-failed
   ```

2. [Restore each deleted or replaced draft file](#restore-a-draft-file).
3. Rerun the failed jobs: `gh run rerun "$run" -R jdx/packslip --failed`.

`go-module-tag` runs beside `release`, so `go/vX.Y.Z` already exists and may
be cached by the Go proxy; do not move or delete it. A rerun after the draft
was published verifies the public files and original attestations, skips
every release and bundle write, and finishes the major-tag update. It never
rolls the tag back from a newer release.

### The major tag did not move

Usually the `protect-tags` ruleset blocked it ([Major versions](#major-versions)).
Fix the ruleset or move the tag yourself, then rerun the failed jobs; the
`release` job treats a major tag already at the release commit as done.

Moving the tag needs a login that bypasses the ruleset, which the built-in
token cannot. If your shell's `GH_TOKEN` lacks that,
`env -u GH_TOKEN -u GITHUB_TOKEN` makes `gh` use your stored admin login:

```sh
commit=$(gh api repos/jdx/packslip/commits/vX.Y.Z --jq .sha)
env -u GH_TOKEN -u GITHUB_TOKEN gh api -X PATCH repos/jdx/packslip/git/refs/tags/v1 -f sha="$commit" -F force=true
```

For a major tag that does not exist yet:
`env -u GH_TOKEN -u GITHUB_TOKEN gh api -X POST repos/jdx/packslip/git/refs -f ref=refs/tags/v2 -f sha="$commit"`.

### A publishing job failed

Rerun the failed jobs; each is safe to retry:

- `image`, `distro-source`, and `package-repositories` recheck the complete
  published stable version list just before writing, under the
  `packslip-release` lock. They never trust cached job outputs or GitHub's
  mutable latest flag. An `image` retry for an older version restores its
  `X.Y.Z` image without moving the aliases back; a distribution retry for a
  tag no longer newest publishes nothing.
- `npm` never publishes twice or moves `latest` backwards.
- `list` rebuilds the list from the bundles in R2.

To run a distribution workflow outside a tag run, dispatch it for a published
stable tag. Standalone runs take the `packslip-release` lock; runs called
from `release.yml` use nested groups so they do not deadlock their caller:

```sh
gh workflow run package-repositories.yml -R jdx/packslip -f tag=vX.Y.Z
gh workflow run distro-source.yml -R jdx/packslip -f tag=vX.Y.Z
```

### Release notes are missing

The Communiqué step continues on error, leaving the candidate body.
Regenerate the notes from a checkout of `main` with `ANTHROPIC_API_KEY` set:

```sh
GITHUB_TOKEN=$(gh auth token) mise exec -- communique generate vX.Y.Z --github-release
```

Or write `notes.md` and run
`gh release edit vX.Y.Z -R jdx/packslip --notes-file notes.md`.

### Fixed-in comments are missing

`comment-release-fixes` skips issues and discussions it already commented on
for the tag. Dispatch the standalone workflow; it defaults to a dry run, so
read that log before the second command:

```sh
gh workflow run comment-release-fixes.yml -R jdx/packslip -f tag=vX.Y.Z
gh workflow run comment-release-fixes.yml -R jdx/packslip -f tag=vX.Y.Z -f dry_run=false
```

## Maintainer setup

### Repository settings

- **Branch protection on `main`** requires `final` for non-admins.
- **Allow auto-merge** is on, for the daily auto-merge.
- **The `protect-tags` ruleset** (ID 24686022) lets only its bypass roles
  create, move, or delete tags other than `refs/tags/v1` and
  `refs/tags/go/v*`.
- **The `package-repositories`, `ppa-publishing`, and `copr-publishing`
  environments** limit the APT and RPM, PPA, and COPR publishing jobs to
  `main` and `v*` tags but do not yet hold their credentials
  ([Confine the package credentials](#confine-the-package-credentials)).

### Secrets and variables

Each entry is a repository secret unless marked (variable).

- **`RELEASE_PLZ_TOKEN`**: fine-grained, with contents and pull-request write,
  so the workflows' pushes, tags, and pull requests start later workflows.
  Its account must bypass `protect-tags`, since release-plz creates `vX.Y.Z`
  with it.
- **`RELEASE_PLZ_RELEASE=true`** (variable): enables the `release` job;
  without it, merging publishes nothing.
- **`ANTHROPIC_API_KEY`**: for Communiqué's release notes; without it, the
  candidate body stays.
- **`CERTIFICATES_P12`, `CERTIFICATES_P12_PASS`**: the base64 Developer ID
  Application certificate and export password the other jdx.dev CLIs use;
  macOS signing fails without them.
- **`APPLE_API_KEY_P8`, `APPLE_API_KEY_ID`, `APPLE_API_ISSUER_ID`**: a base64
  App Store Connect API key and its key and issuer IDs. Without all three,
  signing fails with "Notarization credentials missing" rather than ship an
  unnotarized binary.
- **`CLOUDFLARE_ACCESS_KEY_ID`, `CLOUDFLARE_SECRET_ACCESS_KEY`**: S3
  credentials from an R2 token scoped to the `jdx-releases` bucket alone;
  the release jobs, `packslip-releases.yml`, and `package-repositories.yml`
  write under `packslip/` with them.
- **`CLOUDFLARE_TOKEN`**: account `Workers Scripts:Edit` and `jdx-releases`
  read access, for `site.yml`'s Worker deploys. It has no zone access,
  because the custom domains were attached by hand.
- **`MISE_LOCK_APP_ID`, `MISE_LOCK_APP_PRIVATE_KEY`** (optional): an App for
  `mise-lock.yml` to push a regenerated `mise.lock` and rendered files to
  Renovate branches. It is not configured (only the unread
  `MISE_LOCK_APP_CLIENT_ID` exists), so the push uses `GITHUB_TOKEN`, which
  starts no workflow: close and reopen the Renovate pull request to run checks.
- **`PACKSLIP_GPG_KEY`** (secret) and **`PACKSLIP_GPG_FINGERPRINT`**
  (variable): the dedicated package signing key and its full 40-character
  fingerprint. The key signs the APT and RPM repositories and the Debian
  source upload, so Launchpad must accept it.
- **`COPR_API_LOGIN`, `COPR_API_TOKEN`**: COPR build submission credentials.
- **`PPA_NAME`, `PPA_DISTRIBUTION`, `PPA_REVISION`** (variables): default
  `ppa:jdxcode/packslip`, Ubuntu series `resolute`, and `1`. Increment the
  revision to rebuild the source package for the same upstream version.
- **`COPR_OWNER`, `COPR_PROJECT`, `COPR_CHROOTS`** (variables): default
  `jdxcode`, `packslip`, and the chroots `fedora-44-x86_64 fedora-44-aarch64`.
- **`DISTRO_SOURCE_ENABLED=true`** (variable): enables the tag run's PPA and
  COPR source uploads.
- **`PACKAGE_REPOSITORIES_ENABLED=true`** (variable): enables the tag run's
  APT and RPM publication and the weekly refresh.

Before enabling either, configure and verify both native architectures; the
first eligible release must contain the complete installer and packaging
recipes. `release.yml` calls these workflows itself, because a release
created with GitHub's built-in token starts no other release-triggered
workflow.

### Confine the package credentials

Any workflow in jdx/packslip can read a repository secret. To confine
`PACKSLIP_GPG_KEY`, `COPR_API_LOGIN`, and `COPR_API_TOKEN` to their
environments:

1. Add `PACKSLIP_GPG_KEY` to the `package-repositories` and `ppa-publishing`
   environments, and the COPR pair to `copr-publishing`. Confirm:

   ```sh
   for env in package-repositories ppa-publishing copr-publishing; do
     gh secret list -R jdx/packslip --env "$env"
   done
   ```

2. In one pull request, remove them from the `secrets:` blocks of the
   `distro-source` and `package-repositories` calls in `release.yml` and from
   both workflows' `workflow_call` declarations. A job that names an
   environment reads its secrets without the caller passing them.
3. Delete the repository secrets; until then a dispatched run still reads
   them, so earlier tests prove nothing:

   ```sh
   for name in PACKSLIP_GPG_KEY COPR_API_LOGIN COPR_API_TOKEN; do
     gh secret delete "$name" -R jdx/packslip
   done
   ```

4. Dispatch `package-repositories.yml` for the latest stable tag
   ([A publishing job failed](#a-publishing-job-failed)) and confirm that it
   still signs. Before testing `distro-source.yml`, increment `PPA_REVISION`:
   Launchpad rejects a repeated source version, by email after `dput`
   succeeds.

The `CLOUDFLARE_*` secrets stay repository secrets, because the `release`
job and `packslip-releases.yml` use them outside any environment.

### crates.io

Publication stores no credential: a crates.io Trusted Publisher is
registered for repository `jdx/packslip` and workflow `release-plz.yml`, and
the `release` job mints a short-lived token through OIDC with
`rust-lang/crates-io-auth-action`. `release-plz.toml` sets `publish = true`
and leaves `git_only` unset, so release-plz decides what to release by
comparing `main` with the latest crates.io version, not the last Git tag. It
publishes only the root Cargo crate.

Trusted Publishing cannot create a crate, so 0.2.0 was published by hand from
its tag, and that API token was revoked right afterward. To redo this,
publish the first version by hand, revoke the token, register the Trusted
Publisher, and only then set `RELEASE_PLZ_RELEASE` to `true`, so enabling
the workflow cannot publish by itself.

### npm package

`packages/packslip/` holds the `packslip` npm package; its `package.json`
keeps the placeholder version `0.0.0`. Once the GitHub release is public, the
`npm` job of `release.yml` sets the tag's version with `npm version`, then
builds, tests, and publishes through npm
[trusted publishing](https://docs.npmjs.com/trusted-publishers) (OIDC), so no
npm token is stored. A rerun skips a version already on npm. A release older
than `latest` goes under the `backport` dist-tag, so `latest` never moves
backwards.

npm configures trusted publishing only for an existing package, and a new
configuration must publish within two days:

1. Publish the placeholder once from your machine, with your npm login and
   2FA; the first real release supersedes it:

   ```sh
   cd packages/packslip
   npm ci && npm run build && npm publish --access public --provenance=false
   ```

2. On npmjs.com, under the package's **Settings**, **Trusted publisher**,
   choose GitHub Actions, organization `jdx`, repository `packslip`, workflow
   filename `release.yml`, no environment, and `npm publish` allowed. npm
   does not validate these. It checks the top-level calling workflow, which
   is why the publish step is in `release.yml`.
3. Merge a release pull request within two days, or delete the expired
   configuration and add it again.
4. After the first release, set **Publishing access** to "Require two-factor
   authentication and disallow tokens".

### PPA and COPR publication

1. Create the Launchpad PPA, with both architectures enabled, and the COPR
   project, using the defaults in [Secrets and variables](#secrets-and-variables).
2. Register `PACKSLIP_GPG_KEY` with Launchpad. COPR signs its repository
   packages itself.
3. Set the secrets and variables, and
   [confine the package credentials](#confine-the-package-credentials).
4. Dispatch `distro-source.yml` for a published tag
   ([A publishing job failed](#a-publishing-job-failed)).

`distro-source.yml` hands its publication jobs credentials only after it
prepares the source packages offline. It versions the Debian source
`X.Y.Z-0ppa<PPA_REVISION>~<series>`. Its COPR job waits for both
architecture builds; the PPA upload does not, so check those builds as in
step 8 of [Check a release](#check-a-release).

The user docs mention neither repository. Before announcing one, install its
package on a clean native machine, confirm `packslip install --help`, and run
the checks in
[Check a tool's handoff](https://packslip.dev/docs/compatibility/#check-a-tools-handoff).

### APT and RPM repositories

`package-repositories.yml` publishes signed APT and RPM repositories for Linux
x64 and ARM64, which the packslip.dev Worker serves at `/apt/`, `/rpm/`, and
`/gpg-key.pub`. Pull request validation never receives the production key;
CI signs with an ephemeral one.

APT indices and payloads have content-addressed names, and older objects are
kept, so clients on an earlier authenticated index can finish downloading;
the signed `InRelease` is published last. APT metadata is valid for fourteen
days; a weekly run (Mondays, 08:15 UTC) re-signs it for the latest release.

### Rotate the repository key

The key, fingerprint `2A355C7DF63A62A5851534C583B336958530A3D2`, expires on
October 2, 2028. Every publication exports the key `PACKSLIP_GPG_FINGERPRINT`
names to `/gpg-key.pub` in R2, replacing the previous file; the Worker serves
the site's `static/gpg-key.pub` only when R2 has none.

1. Create the replacement key well before the current one expires.
2. Export it to a separate file and merge it. The site serves it from static
   assets at `https://packslip.dev/gpg-key-<new-fingerprint>.pub`, a path the
   Worker does not route:

   ```sh
   gpg --armor --export <new-fingerprint> > static/gpg-key-<new-fingerprint>.pub
   ```

3. In [Install packslip](https://packslip.dev/docs/install/)
   (`content/docs/install.md`), publish the new key's URL and fingerprint,
   and tell repository users to trust it beside the current key: APT users
   add it to the keyring their `signed-by` option names, and RPM users run
   `rpm --import`. Test those commands on native machines first.
4. Give users time to add it; publications sign with the current key until
   the switch.
5. Switch: update `PACKSLIP_GPG_KEY` and `PACKSLIP_GPG_FINGERPRINT`, register
   the new key with Launchpad, and replace `static/gpg-key.pub`. Then
   dispatch `package-repositories.yml` for the latest stable tag, which
   re-signs both repositories and replaces `/gpg-key.pub`.
6. Make the install page name only the new key at `/gpg-key.pub`, and delete
   the separate file.

## Reference

### How a release works

**The release pull request.** Each push to `main` runs the `release-pr` job
of `release-plz.yml`. `release-plz update` bumps `Cargo.toml` and writes a
`CHANGELOG.md` entry from conventional commits through `cliff.toml`. The job
deletes `action/release.json`, runs `mise run render` so the CLI reference
matches the release, commits `chore: release vX.Y.Z`, and force-pushes
`release-plz`. It opens or updates the pull request, labeled `release`, with
the changelog section as its body, or closes a stray one when nothing is
left to release.

**The candidate.** Each push to `release-plz` runs `release.yml`:

1. `build` builds the five [platforms](#platforms) and signs and notarizes
   the macOS executable.
2. `stage` generates the CLI resources (usage spec, man page, and bash, zsh,
   fish, and PowerShell completions) and `install.sh` and `install.ps1`,
   which embed every executable's SHA-256.
3. `stage` attests these eight files and the build's ten, and uploads all 18
   to an unpublished draft release `vX.Y.Z`; no tag exists yet.
4. `stage` commits the digest map, `action/release.json`, as
   `chore: pin release candidate archives`, with each file's checked SHA-256
   and size, the build source commit, and the run ID.
5. That push reruns pull request CI and runs `release.yml` in `ready` mode,
   which verifies the candidate without rebuilding it.

The release commit may differ from the candidate source (the map's
`source_commit`) only in `action/release.json`, which the Cargo package
excludes. CI's `staged-release` checks this for the merge tree, so it and
`final` fail until the map lands. A release-plz refresh removes the map and
builds a new candidate.

Candidate builds and tag promotions share the `packslip-release` concurrency
group, so they never change one draft at once. `queue: max` keeps a later
candidate push from replacing a pending promotion, up to 100 pending runs. A
queued stale candidate still fails the remote branch check before staging,
so it cannot fast-forward over a newer metadata commit.

The files are attested before the map is committed, so their provenance names
the candidate commit on `refs/heads/release-plz`, not the tag. The statement
signed for packslip.dev records that commit and no source tag.

**Warning:** A consumer that requires a tag-built attestation does not accept
packslip's branch-built release. Do not re-attest the old bytes as if the tag
had built them.

**Publication.** On merge, the `release` job of `release-plz.yml` runs if
`RELEASE_PLZ_RELEASE` is `true`. The workflow runs only on `main`, so a
dispatch from another branch publishes nothing. Its gate,
`scripts/release-candidate.py gate`, rechecks the merged tree and the staged
draft; the job then publishes the crate to crates.io and creates the
`vX.Y.Z` tag, which pins the action and its default CLI version. There is no
separate action tag. The tag starts `release.yml`, which checks it against
the `Cargo.toml` version and promotes the draft bytes without rebuilding or
re-attesting them. Its `release` job:

1. Checks the draft's files against the committed digests and original
   attestations: the exact source digest, the `release-plz` branch, the
   `release.yml` signer workflow, GitHub-hosted runners, and a successful
   candidate run.
2. Rewrites the notes with Communiqué, keeping the candidate body if that
   fails.
3. Signs the bundle as project `packslip.dev` with the verified candidate
   binary, uploads the files and bundle to R2, and attaches the bundle to
   the GitHub release as its 19th file.
4. Checks that the tag points at the final release commit, then publishes
   the draft, which becomes GitHub's latest release only if no newer stable
   release exists.
5. Moves or creates the major tag (`v1` for 1.x) at that commit. Before
   1.0.0, `v1` briefly pointed at 0.x; `v0` stays at 0.3.1, so workflows
   pinned to `@v0` keep working without updates.

Major tags start no release, because `release.yml` runs only for
`v[0-9]+.[0-9]+.[0-9]+` tags, and `tag_pattern` in `cliff.toml` keeps them out
of the changelog. The tag run's other jobs:

- `go-module-tag` creates `go/vX.Y.Z` at the same commit. A module in a
  subdirectory needs the `go/` prefix, and the Go module proxy serves it from
  that tag, with no registry upload or separate release dispatch.
- `list` publishes the signed release list at
  `https://packslip.dev/.well-known/packslip.json` through
  `packslip-releases.yml`.
- `image` and `npm` publish the [container image](#container-image) and the
  [npm package](#npm-package).
- `comment-release-fixes` comments on each closed issue or discussion that a
  released pull request names with a closing keyword.
- `distro-source` and `package-repositories`, when enabled, publish PPA and
  COPR source packages and the signed APT and RPM repositories.

**The action's CLI.** The actions default to the CLI version in
`Cargo.toml`, and each released action commit carries the digest map.
`scripts/install-packslip.sh` checks the downloaded archive against the map,
then requires the exact candidate build source, the `release-plz` branch, the
`release.yml` signer workflow, and GitHub-hosted runners in its provenance.
Without the map it stops with
`the action checkout has no release digest map; use a published action commit`.
The `packslip-version` input bypasses the map with a warning, leaving only
repository provenance and any `packslip-sha256` checked.

### What counts toward a release

release-plz counts only pull requests that change a file in the Cargo
package; one that changes only excluded files gets no changelog line and does
not raise the version. The `exclude` list in `Cargo.toml` leaves out:

- The site: `content/`, `data/`, `layouts/`, `static/`, `hugo.toml`, and
  `assets/`.
- The Workers: `cloudflare/` and `wrangler.jsonc`.
- The Go module, the npm package, and the committable verifier: `go/`,
  `packages/`, and `verifier/`.
- Release files: `.github/`, `action/release.json`, `CHANGELOG.md`,
  `RELEASING.md`, `cliff.toml`, and `release-plz.toml`.
- Contributor and agent files: `CONTRIBUTING.md`, `AGENTS.md`, `CLAUDE.md`,
  `.agents/`, `.claude/`, `.codex/`, and `.entire/`.

`action.yml`, `releases/action.yml`, `scripts/`, `communique.toml`,
`mise.toml`, and `mise.lock` are packaged, so changes to them count. Give
action changes conventional-commit titles, such as `fix(action): ...` or
`feat(action): ...`.

### The packslip.dev and packslip.sh Workers

`site.yml` deploys both Workers on every push to `main`, packslip.sh in a
separate job.

The `packslip-dev` Worker serves packslip.dev.
[`wrangler.jsonc`](wrangler.jsonc) routes `/v*`,
`/.well-known/packslip.json`, `/apt/*`, `/rpm/*`, and `/gpg-key.pub` to it
ahead of the static assets (`run_worker_first`), and
[`cloudflare/worker.js`](cloudflare/worker.js) reads `<tool>/<tag>/<file>`
from the `jdx-releases` R2 bucket, the layout the upload steps write. It
matches only tags starting with `v` and the list at the bare host. The list is valid for 30 days;
`packslip-releases.yml` re-signs it every Monday at 06:00 UTC.

The `packslip-sh` Worker, configured in `cloudflare/installer/`, serves
packslip.sh's install scripts from the release files in R2
([Install packslip](https://packslip.dev/docs/install/) lists the URLs). The
packslip.dev list sets no `latest`, so packslip.sh serves the highest listed
release that is neither yanked nor a prerelease, once the list names it. Each
Cloudflare data center caches versioned scripts for a year and the latest
script for five minutes; cached downloads skip R2, but every request runs the
Worker. The `packslip.sh` zone, in the same Cloudflare account as
`jdx-releases`, was set up once by hand with Always Use HTTPS on, and the
Worker also redirects plain HTTP to HTTPS.

### Container image

The tag run's `image` job builds `ghcr.io/jdx/packslip` for linux/amd64 and
linux/arm64 with `container/build.sh` from the build job's executables,
checked against the digest map and provenance before anything runs, and
attests it. CI's `image` job builds it the same way from the latest release,
runs it, and copies the executable into a Debian slim image, testing
`container/` changes before a release depends on them. The first release
that pushes the image creates the GHCR package; if GitHub made it private,
make it public once in its settings.

### Platforms

| Asset | Target | Runner |
| --- | --- | --- |
| `linux-x64` | `x86_64-unknown-linux-musl` | `ubuntu-latest` |
| `linux-arm64` | `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` |
| `darwin-arm64` | `aarch64-apple-darwin` | `macos-latest` |
| `windows-x64` | `x86_64-pc-windows-msvc` | `windows-latest` |
| `windows-arm64` | `aarch64-pc-windows-msvc` | `windows-11-arm` |

Each platform ships as an archive (`.tar.xz`, or `.zip` on Windows) and as the
bare executable (`packslip-vX.Y.Z-<asset>`, `.exe` on Windows) that the
install scripts download.

There is no `darwin-x64`: Rosetta 2 does not run arm64 code on Intel, and a
second notarized artifact is not worth it for a shrinking set of machines.
On an Intel macOS runner, the action's install step:

- works with the `packslip-path` input;
- with only `packslip-version`, prints build instructions and fails;
- with neither, exits with code 4 and no message, because the digest map has
  no `darwin-x64` entry.

Windows arm64 builds on a native runner, because `aws-lc-sys` (rustls'
crypto) compiles C and assembly for the host.
`.cargo/config.toml` sets `+crt-static` for MSVC targets, so both Windows
builds link the C runtime statically, and packslip's own release bundle lists
only the system library `combase.dll` in `requires.libs` for the Windows
files.

**Warning:** A `RUSTFLAGS` variable in the build job would replace
`+crt-static` and bring back the `vcruntime140.dll` dependency.

### macOS signing and notarization

The macOS binary is signed with the
`Developer ID Application: Jeffrey Dickey (4993Y37DX6)` certificate using
`--options runtime --timestamp`, which notarization requires, then submitted
to `notarytool`. The job fails unless the status is `Accepted`, because
`--wait` alone can return zero on an `Invalid` submission. `stapler` cannot
write into a bare Mach-O, so nothing is stapled; Gatekeeper looks the ticket
up online, and a browser download is not held behind the "cannot be
verified" dialog.
