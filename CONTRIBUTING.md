# Contributing to packslip

This guide takes anyone changing packslip's code, specification, or
documentation from a fresh clone to a pull request that passes CI. To
publish a release, follow [RELEASING.md](RELEASING.md).

## Report a problem or propose a format change

Open an [issue](https://github.com/jdx/packslip/issues) for a bug or
format feedback, with the use case and, ideally, a small release layout
that shows it. Report vulnerabilities through [SECURITY.md](SECURITY.md),
never publicly. A format change must respect the version 1
[stability contract](https://packslip.dev/release/v1/#stability); one
outside it needs a version 2 proposal.

## Set up the repository

You need:

- Rust 1.93+ through rustup (`rust-version` in `Cargo.toml`). `mise.toml`
  pins no toolchain; with an older default, prefix commands with
  `RUSTUP_TOOLCHAIN=1.93.0`.
- mise 2026.9.7+ (`min_version` in `mise.toml`).
- Python 3.11+ for release-candidate tests and packaging, 3.9+ for the
  docs scripts.
- Node.js 20+ for the verifier and Worker tests; `packages/packslip/`
  needs 22.22.2+ on 22.x, 24.15.0+ on 24.x, or 26+.
- Go 1.25.8+ for `go/` and the Go conformance test.
- `jq`, which the Linux action tests use to read the release digest map.
- Optionally, Docker with `buildx` for the container, package,
  repository, and containerized installer jobs. Locally, these also need
  an authenticated `gh` (image); `devscripts`, `debhelper`, and `rpm`
  (packages); and `musl-tools` on a native Linux host (repository
  installer).

`mise install` adds the rest at the versions in `mise.lock` (change it
only with `mise lock`): Hugo and usage (site, command docs), shellcheck,
hk, and jactionlint (lint, hooks), Communiqué (release notes), and
mr-boxington (`mbx`). The Cargo wrapper in `mise.toml` runs `cargo`
through `mbx`, so `target/` is a symlink into `~/.cache/mbx`.

```sh
mise install
cargo build
cargo test --all-features
```

`cargo test` checks that a `minisign` on `PATH` accepts packslip's key
signatures, or skips that check when there is none. A mise shim for a
minisign this repository does not activate fails it; take the shim off
`PATH` or put a working minisign first.

To try key-signed commands by hand, pass `--no-log` to `packslip create`
and `packslip releases` (keeping test signatures out of the public Rekor
log) and `--allow-unlogged` to `packslip verify`.

## Find the right source

### Specification, actions, and other implementations

- **Format rules:** `docs/spec/packslip.md`.
- **Conformance vectors:** `tests/conformance/`
  ([README](tests/conformance/README.md)), run by `tests/conformance.rs`.
- **Actions:** `action.yml` (publish) and `releases/action.yml` (release
  list), both running `scripts/install-packslip.sh`.
- **Actions reference:** `docs/actions.md` (with marker lines such as
  `<!-- inputs: action.yml -->` alone on a line),
  `docs/actions-since.toml` ("Since" column), and
  `scripts/render-actions.py` (tables).
- **Go publisher library:** `go/` ([README](go/README.md));
  `go/cmd/conformance` feeds `tests/go_conformance.rs`.
- **JavaScript:** `packages/packslip/` (the npm package that verifies
  from Node.js) and `verifier/packslip-verify.mjs` with
  `verifier/test.mjs` (the dependency-free verifier projects commit).

### Rust crate and CLI

- **Schema and validation:** `src/model.rs`.
- **Creation:** `src/create.rs`, `src/manifest.rs` (release.toml),
  `src/archive.rs` and `src/linkage.rs` (archives, `requires.libs`).
- **Signing and verification:** `src/verify.rs`, `src/sigstore.rs`,
  `src/trust_root.rs`, `src/dsse.rs` and `src/minisign.rs` (key
  signing), `src/forge.rs` and `src/fingerprint.rs` (forge identity,
  signer fingerprints).
- **`packslip install`:** `src/install.rs`, `src/install_*.rs`, and
  `src/discovery.rs`.
- **Windows launcher:** `src/launcher_windows.rs` and
  `src/launcher_payload.rs`, built by `build.rs`; `.cargo/config.toml`
  (static C runtime).
- **Commands and help:** `src/main.rs`; `src/cli.rs` (`version`, help,
  and usage errors); `docs/cli.md` (the CLI reference's landing page);
  `docs/templates/cli-command.md.tera` (each command page's title,
  description, and sidebar order).
- **Library and tests:** `src/lib.rs`; `Cargo.toml` features
  (`--no-default-features` builds the verify-only library); `tests/*.rs`
  and `tests/fixtures/` ([README](tests/fixtures/README.md)).
- **Agent skill, shipped every release:** `skills/packslip/SKILL.md`.

Each command needs these Cargo features:

| Command | Cargo features |
| --- | --- |
| `install` | `install-cli` |
| `create`, `releases` | `create`, `manifest` |
| `keygen` | `sign` |
| `schema` | `schema` |
| `completion`, `pin`, `show`, `verify`, `version` | `verify-cli` (the binary requires it) |

### Install scripts, container, and distribution packages

- **Install scripts:** `installer/install.sh` and `installer/install.ps1`
  (filled per release by `installer/render.sh`), tested by
  `installer/test.sh` and `installer/test.ps1`; packslip.sh's Worker in
  `cloudflare/installer/`.
- **Container:** `container/Dockerfile`; `container/build.sh` builds
  `ghcr.io/jdx/packslip` from a release's executables.
- **Debian and RPM packages:** `packaging/debian/`, `packaging/rpm/`,
  `scripts/package-*source.sh`, and `scripts/check-distro-packages.sh`.
  `packaging/packslip.1` is their hand-written `install-cli` man page,
  not the generated `packslip.1`.
- **APT and DNF repositories:** `packaging/repositories/`,
  `packaging/packslip.repo`, `scripts/package-binary.sh`, and
  `scripts/*repositor*`.

### Documentation site

- **Guides:** `README.md`, `content/_index.md` (also the hero's title
  and lede), and `content/docs/`.
- **Layout:** `layouts/` (hero buttons and release-job snippet in
  `layouts/index.html`, navigation in `layouts/partials/site-nav.html`),
  `static/style.css`, `static/docs.js` (also search, with
  `layouts/index.searchindex.json`), and `hugo.toml`.
- **Legacy anchors:** `data/legacy_anchors.toml`, applied by
  `layouts/_default/_markup/render-heading.html`,
  `layouts/partials/legacy-anchors.html`, and `static/docs.js`.
- **Shortcodes** in `layouts/shortcodes/`: `release-example.html`
  (renders `docs/examples/release-excerpt.json` on the homepage and in
  the quickstart), `tabs.html`, `tab.html`, and `adopters.html` (renders
  `data/adopters.toml`, whose header says how to check a row, for
  [Who is using packslip](https://packslip.dev/docs/who-is-using-packslip/)).
- **Social images:** `layouts/partials/social-image.html` and
  `assets/social/` ([README](assets/social/README.md));
  `scripts/check-social-images.mjs` checks built images, by hand only.
- **packslip.dev's Worker:** `wrangler.jsonc`, `cloudflare/worker.js`.
- **Docs check:** `scripts/check-docs.py` and its TOML reader,
  `scripts/minitoml.py`.

### Releases and repository automation

- **Releases:** `RELEASING.md`, `.github/workflows/`,
  `release-plz.toml`, `cliff.toml` (changelog), `communique.toml`
  (release notes), `scripts/describe-release.sh` (packslip.dev's own
  packslip), and `scripts/release-candidate.py` with
  `tests/release_candidate_test.py`.
- **packslip.dev's release list:** `.github/packslip/yanked` and
  `.github/packslip/security`, read by `packslip-releases.yml`.
- **Dependencies:** `.github/renovate.json`; on Renovate branches,
  `mise-lock.yml` regenerates `mise.lock` and the rendered files.

### Generated files

`mise run render` overwrites these; edit the source and rerun it.

| Generated | Source |
| --- | --- |
| `content/spec.md` | `docs/spec/packslip.md`; front matter from the `render` task in `mise.toml` |
| `packslip.usage.kdl` | `src/main.rs` and `src/cli.rs`, via the hidden `packslip usage` command (a [usage](https://usage.jdx.dev) spec) |
| `content/cli/`, `packslip.1` | That spec (plus `docs/cli.md` and `docs/templates/cli-command.md.tera` for `content/cli/`) |
| `static/schema/` | `src/model.rs` (statement, release list), `src/verify.rs` and `src/sigstore.rs` (verify report), via `packslip schema` |
| `content/docs/actions.md` | `docs/actions.md`, both `action.yml` files, `docs/actions-since.toml` |

Workflows write `action/release.json` (the action's release digest map),
`data/github.json` (the star count, refreshed each deploy; previews use
the committed value),
and each release's `CHANGELOG.md` section (release-plz, from merged pull
request titles). Edit none by hand, except `CHANGELOG.md` when
[publishing a version release-plz would not propose](RELEASING.md#publishing-a-version-release-plz-would-not-propose).

## Change a format rule

The specification text governs; the
[conformance vectors](tests/conformance/README.md) restate it as test
cases, so a vector that contradicts or exceeds it is a bug.

1. Edit `docs/spec/packslip.md` with plain Markdown headings; GitHub
   renders it, and Hugo's `{#id}` breaks there.
2. Change the affected vectors in the same pull request
   ([Add or change a case](tests/conformance/README.md#add-or-change-a-case));
   editorial clarifications leave cases and results unchanged.
3. Update `src/` and each implementation the rule reaches:
   - Statement validation: `go/model.go` and
     `packages/packslip/src/model.ts`, tested on
     `statement-validity.json`.
   - Signer checks and binding to a forge project's repository:
     `packages/packslip/src/` and `verifier/packslip-verify.mjs`.
4. Run `mise run render` and commit `content/spec.md`.
5. Run `cargo test --test conformance`, plus the Go and npm checks in
   [Check a change](#check-a-change) if `statement-validity.json`
   changed.
6. In the description, say whether you changed the specification or
   fixed a vector that required behavior outside it.

Review every specification edit, even an editorial one, sentence by
sentence for a change of meaning, repeating until a pass finds none.

## Change a command or an action input

- Help text: `src/main.rs` (`version`: `src/cli.rs`). Render and commit.
  For `install`, `verify`, `pin`, `show`, `completion`, or `version`, also
  update `packaging/packslip.1` by hand.
- Command added, renamed, or removed: update the page list in the
  `Verify generated CLI documentation` step of `.github/workflows/ci.yml`,
  the command's sidebar weight in `docs/templates/cli-command.md.tera`,
  and the feature table in [Rust crate and CLI](#rust-crate-and-cli).
- Flags or action inputs changed: update `skills/packslip/SKILL.md` and
  the guides.
- Action input or output added or removed: add each new input or output
  to `docs/actions-since.toml` with the next release's version, and
  delete a removed one's entry; render fails otherwise.
- The actions reference copies each input's `description`, `required`,
  and `default`, and each output's `description`, from both `action.yml`
  files. Keep each name alone on its line at two spaces and its fields
  at four; render rejects other layouts.

## Keep old releases verifying

`tests/compatibility.rs` runs the current binary and a pinned v1.4.0
baseline against the same documents.

- Before claiming support for another format, add a fixture, its signer
  policy, and a row in the
  [verification matrix](https://packslip.dev/docs/compatibility/#verification-matrix).
- Keep existing baselines when adding one
  ([Change the baseline](tests/fixtures/README.md#change-the-baseline));
  never advance the only baseline to the latest release.
- Trust root tests use a synthetic TUF chain, not the production Sigstore
  root history. To claim support for an old production root, check that
  root's full historical chain and record its dates.
- When security forces a rejection, document it and replace the
  fixture's acceptance assertion with the expected failure.

To run the cross-version check locally (without `PACKSLIP_BASELINE_BIN`,
only the current binary is tested; CI sets it):

```sh
git clone --depth 1 --branch v1.4.0 https://github.com/jdx/packslip ../packslip-v1.4.0
cargo build --locked --manifest-path ../packslip-v1.4.0/Cargo.toml
PACKSLIP_BASELINE_BIN="$(cd ../packslip-v1.4.0 && pwd)/target/debug/packslip" \
  cargo test --locked --no-default-features --features install-cli --test compatibility
```

## Work on the documentation

### Preview and build the docs

```sh
mise run docs        # Render, then serve with Hugo.
mise run docs:build  # Render, then build into public/.
mise run docs:check  # Build, check links, run the offline examples.
mise run render      # Regenerate the generated files only.
```

Render deletes and recreates `content/cli/`, so keep handwritten pages
in `content/docs/`. `site.yml` deploys packslip.dev on every push to
`main`, independently of releases: merged guide and specification
changes go live at once, and the CLI reference can show unreleased
commands.

### Validate documentation

Run `mise run docs:check`, then view the affected pages in a browser. It
checks local links, anchors, assets, and the required guide URLs; one H1
per page, unique IDs, and spacing around inline markup; every
`data/legacy_anchors.toml` entry; that links use current anchor IDs, so a
link to an old ID fails; and the
[executable examples](#keep-executable-examples-in-sync), offline with
unlogged keys.

### Write a guide

- Format rules belong in the specification and command details in CLI
  help; a guide says what the reader does because of a rule and links
  it. Check examples against the code, and tell draft integrations apart
  from shipped features.
- Front matter: `title`, a one-sentence `description` under 160
  characters, `group` (`start`, `publish`, `consume`, `build`, or
  `reference`), `weight`, `aliases` only if needed, and
  `toc_end_level: 4` to list H4s. Write the one H1 by hand, and list new
  guides in `scripts/check-docs.py`.
- Link with site paths ending in a slash (`/docs/verifying/`), except in
  `content/docs/bootstrap.md`: it ships in the Debian and RPM packages,
  so its links are absolute `https://packslip.dev/` URLs. The
  typographer is off; type `—`.
- Write packslip and mise in lowercase, never opening a sentence with
  either. Use the [Glossary](https://packslip.dev/docs/glossary/) terms
  exactly: release bundle, statement, release.toml, release list, domain
  project, forge project, signer fingerprint, pin, repackager, and
  stamping host.
- Open with who the page is for and what they will have, in two
  sentences at most, and route other readers away in one line. Give each
  page one type, with imperative titles for tasks ("Verify a release")
  and noun titles otherwise ("Glossary"). Link text is the target's
  title or a plain description.
- Keep headings short and sentence case, H3 sparingly, and each
  sentence to one condition and about 25 words; chain conditions as
  steps or a table.
- Commands copy and run, using `mytool`, `owner/mytool`,
  `mytool.example.com`, `X.Y.Z`, `<full-commit-sha>`, and
  `jdx/packslip@v1` (full-commit pins only in the actions reference).
  Show real output where it matters, and explain each flag in a list
  after the command, not inline. One good example beats three partial
  ones.
- Troubleshooting rows start with the exact CLI error. State each limit
  once, and each warning once where the reader decides, as a
  "**Warning:**" paragraph or a table row. Security claims say what they
  establish and what they do not.
- Only the install, bootstrap, and compatibility pages and the actions
  "Since" column give minimum packslip versions; a toolchain or runtime
  minimum belongs on its library's page. Maintainer internals stay off
  the site.
- American English. No emoji, exclamation marks, hedging, narration,
  "This page covers", or restating the title.

### Rename or move a section

Fix every link `git grep -n 'old-id'` finds, including CLI help,
`action.yml` descriptions, `docs/cli.md`, and `docs/actions.md`. Links
must use the current ID: `mise run docs:check` fails a built page that
links an old one. It sees only built pages, not the skill,
`packages/packslip/README.md`, vector `rule` links,
`installer/install.sh`, packslip.sh's Worker, or repository docs.

Shipped binaries, actions, install scripts, vectors, and other sites
keep linking the old anchor, so map it in `data/legacy_anchors.toml`,
whose header explains both forms:

```toml
["/docs/verifying/"]
old-id = "new-id"                     # Moved on the same page.
other-old-id = "/docs/trust/#new-id"  # Moved to another page.
```

Add the entry in the change that removes the old ID. The target, like
content links, must be a real heading ID in the same build. Cross-page
moves redirect on guides, CLI pages, and the specification, not the
homepage. Moves to another page need JavaScript; prefer a same-page entry
when a heading on the old page still covers the topic. To move a guide,
update its path in `scripts/check-docs.py` and links to it; a front matter
alias keeps the old path working.

### Keep executable examples in sync

- **Quickstart:** `content/docs/getting-started.md` has exactly four
  `<!-- docs-test: quickstart -->` markers (setup, create, show, verify),
  each directly above a fenced `sh` block. The check runs them in order
  in one temporary directory, then confirms `packslip verify` refuses a
  modified artifact. Changing their number fails until
  `check_quickstart` in `scripts/check-docs.py` is updated. The marker
  does nothing elsewhere; commands that install packslip itself are
  never marked or run.
- **Recipes:** the `<!-- docs-test: recipe NAME -->` markers above
  `toml` blocks in `content/docs/recipes.md` (`rust`, `go`, `monorepo`,
  `desktop`) must match the `RECIPE_FILES` entries in
  `scripts/check-docs.py`, which describe each recipe's sample archives.
  The check covers configuration and archive layout, not language builds
  or platform signing.
- **Release example:** the check compares
  `docs/examples/release-excerpt.json` with what the quickstart's
  `packslip create` produces.

## Check a change

Before opening a pull request, run the core of CI's `test` job:

```sh
cargo fmt --all -- --check
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
# The verify-only library build:
cargo clippy --no-default-features --all-targets -- -D warnings
cargo test --no-default-features
python3 -B -m unittest discover -s tests -p 'release_candidate_test.py'
mise run lint        # Shellcheck what the actions, workflows, and installer run.
hk check --all
mise run docs:check  # Also runs mise run render.
mise exec -- usage lint packslip.usage.kdl
git status --short   # The regenerated files to commit.
```

- CI fails on any compiler warning (`RUSTFLAGS=-D warnings`) and when
  `mise run render` would change a committed file, so commit regenerated
  files with their source.
- [hk](https://hk.jdx.dev) runs `cargo fmt`, `cargo clippy`,
  `shellcheck`, and `jactionlint` from `hk.pkl`; `hk fix --all` fixes
  findings, and `hk install` adds a pre-commit hook. CI's
  `JACTIONLINT_ONLINE=1` also flags vulnerable, impostor, and stale
  action references.
- `cargo test --all-features` includes `tests/action.rs`, which runs
  `action.yml`'s create step against a mock CLI (alone:
  `cargo test --test action`).

Install scripts and JavaScript (`installer/test.sh` needs Python 3; the
npm tests fetch sigstore's trust root):

```sh
installer/test.sh                                 # install.sh against a stand-in release, each local shell.
INSTALLER_TEST_IMAGES="alpine:3 busybox:latest" installer/test.sh  # Also inside containers, with Docker.
node --test cloudflare/installer/worker.test.mjs  # packslip.sh's Worker.
node --test cloudflare/worker.test.mjs            # packslip.dev's Worker.
node --test verifier/test.mjs                     # The committable verifier, offline.
(cd packages/packslip && npm ci && npm run typecheck && npm run build && npm test && npm pack --dry-run)
```

Go module or `statement-validity.json` changes:

```sh
(cd go && go mod tidy && git diff --exit-code -- go.mod go.sum && test -z "$(gofmt -l .)" && go test ./...)
PACKSLIP_GO_CONFORMANCE=1 cargo test --test go_conformance
```

Cargo feature or feature-gated changes:

```sh
for features in trust-root verify-cli install-cli; do
  cargo clippy --no-default-features --features "$features" --all-targets -- -D warnings
  cargo test --no-default-features --features "$features"
done
cargo clippy --no-default-features --features verify-cli,sign --all-targets -- -D warnings
cargo test --no-default-features --features verify-cli,sign --test verifier_cli
```

`src/install_*.rs`, `src/discovery.rs`, or launcher changes (the
`extraction` job, one feature at a time):

```sh
for features in install-extract install-fs install-launcher install-host; do
  cargo clippy --no-default-features --features "$features" --lib -- -D warnings
done
cargo test --no-default-features --features install-extract --lib install_extract
cargo test --no-default-features --features discovery --lib discovery
cargo test --no-default-features --features install-fs --lib install_fs
cargo test --no-default-features --features install-launcher --lib install_launcher
cargo test --no-default-features --features install-launcher --lib launcher_payload
cargo test --no-default-features --features install-host --lib install_host
```

Installation or discovery changes: also test under umask 002, the
Debian and Ubuntu login default, since the installer must accept the
directories it creates.

```sh
(umask 002 && cargo test --all-features --lib -- install discovery)
```

### What else CI runs

`ci.yml` runs on every pull request and push to `main`:

| Job | Checks |
| --- | --- |
| `test` | The first block in [Check a change](#check-a-change), feature sets, umask 002, packslip.sh's Worker, and the `Verify generated CLI documentation` step |
| `extraction` | Each installation feature alone on Linux x64/ARM64, macOS ARM64, and Windows x64/ARM64 |
| `msrv` | Clippy and tests on Rust 1.93.0, same runners |
| `compatibility` | v1.4.0 cross-version, trust root, and installation tests |
| `installer`, `installer-windows` | `installer/test.sh` on Linux x64/ARM64, macOS, Alpine, and busybox; `installer/test.ps1` on Windows PowerShell 5.1 (also 32-bit) and 7, Windows x64/ARM64 |
| `image` | Container build from the latest release's Linux archives, after checking their attestations |
| `distro-source`, `distro-build` | Debian and RPM packages for both Linux architectures, checked in Docker without network |
| `repository-installer`, `repository-packages`, `repository-install` | APT and DNF repositories with an ephemeral key: native installs on both architectures; altered metadata, wrong key, and modified RPM rejected |
| `verifier`, `npm-package`, `repository-worker` | Verifier on Node.js 20 and 24, npm package on 24, packslip.dev's Worker |
| `signing` | Pushes to `main` only, which have the id-token permission: keyless and key signing, logged in Rekor, and the releases action |
| `staged-release` | Release candidate, on release-plz's pull request |
| `final` | Requires every job (`signing` and `staged-release` may skip); the one required check on `main` |

`packslip-go.yml` runs the Go checks on `main` and on pull requests
touching `go/`, `statement-validity.json`, `tests/go_conformance.rs`, or
itself. `conventional-commits.yml` checks titles separately because it
runs on `pull_request_target`.

## Submit a pull request

Pull requests are squash-merged: the title becomes the commit subject on
`main` and the description its body, and Communiqué writes release notes
from both. A change to a file in the Cargo package also puts the title
in `CHANGELOG.md`. One that changes only files `Cargo.toml` excludes,
such as the site, `go/`, `packages/`, `verifier/`, `.github/`, or this
guide, gets no changelog line
([What counts toward a release](RELEASING.md#what-counts-toward-a-release)).

### Write the title

```text
<type>[optional scope][optional !]: <description>
```

Pull request titles must use this format; intermediate commit subjects
should, though CI does not check them.

- Types: `chore`, `ci`, `docs`, `feat`, `fix`, `perf`, `refactor`,
  `release`, `revert`, `security`, `spec`, `style`, `test`.
- Start the description with a lowercase letter or an acronym such as
  `CLI`; keep it concise and imperative.
- `!` marks a breaking change. Explain it in a `BREAKING CHANGE:` footer
  at the end of the pull request description, where release-plz and
  Communiqué read it. In a Cargo package change, it makes the next
  release a new major version of the CLI, library, and action
  ([Major versions](RELEASING.md#major-versions)).
- Examples: `feat(create): add archive resource metadata`,
  `fix!: reject an unsupported statement version`,
  `docs: clarify keyless verification`.

CI checks the type, the syntax, and the description's first letter on
open and every edit, so `fix: Reject bad digests` fails. Reviewers check
the imperative mood and the breaking-change explanation.

A merged `fix` or `feat` title, with any scope or `!`, even outside the
Cargo package, makes a waiting release eligible for the
[daily auto-merge](RELEASING.md#daily-auto-merge). Give a change that
warrants no release another type, such as `ci`, `docs`, or `chore`.

### Write the description

Write for a packslip user who has not read the diff or the conversation.

- **Describe the final result.** Before review, and after feedback
  changes the code, compare title and body with the full diff; drop
  abandoned approaches, stale requirements, and unsupported claims.
- **Lead with the user-visible change.** Name the behavior and outcome
  in the title, then open the body with the problem or use case and
  what users can now do. Internal work states its concrete maintainer or
  contributor benefit, never an invented user impact.
- **Make it concrete:** a small valid example and its result for new
  configuration, commands, or APIs; the trigger, before, and after for a
  bug fix; before-and-after output or screenshots for visible changes,
  when they help; measurements and method for performance.
- **Keep the facts in text:** caption screenshots and explain examples
  so no reader needs an image, link, or the diff. Never fabricate
  screenshots, output, measurements, or validation results.
- **State adoption details when relevant:** flags, settings, defaults,
  platforms, experimental status, dependency versions, compatibility
  changes, and migration steps. Separate current behavior from planned
  follow-ups.
- **Keep review details proportionate:** meaningful validation and its
  limits, and implementation details only where reviewers must assess a
  tradeoff. No work logs, commit summaries, or exhaustive test lists; a
  small fix can be a paragraph and a test result.

For example, title a fix
`fix(verify): reject artifacts with mismatched digests`, not
`fix: address review feedback`, and show the verification command and
the failure users get.

These rules bind people and coding agents alike;
[AGENTS.md](AGENTS.md) adds hard rules for agents.
