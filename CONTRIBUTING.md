# Contributing

This repository contains the version 1 packslip specification, its Rust
implementation, the CLI, GitHub Actions, and the documentation site.
Start with the source map below to find the part you want to change.

For a bug report or format feedback, open an
[issue](https://github.com/jdx/packslip/issues) with the use case and,
where possible, a small release layout that demonstrates it. Changes to
the format must respect the version 1 [stability
contract](https://packslip.dev/release/v1/#stability); changes outside that
contract require a version 2 proposal.

## Set up the repository

Install Rust 1.93 or newer through rustup and mise 2026.9.7 or newer.
`mise.toml` does not pin a Rust toolchain. `mise install` installs the
other tools at the versions recorded in `mise.toml` and `mise.lock`:

| Tools | Used for |
| --- | --- |
| Hugo and usage | Building the site and generating command documentation |
| shellcheck and hk | Lint checks and local hooks |
| Communiqué | Release notes |
| mr-boxington (`mbx`) | Building through the repository's Cargo wrapper |

The wrapper makes `target/` a symlink into `~/.cache/mbx`. The documentation
check also needs Python 3.9 or newer, a POSIX shell, and `tar`; the Worker
tests need Node.js 20 or newer.

```sh
mise install
cargo build
cargo test --all-features
```

If your default Rust toolchain is older, select a compatible installed
toolchain, for example `RUSTUP_TOOLCHAIN=1.93.0 mise run docs`.

## Find the right source

| Area | Edit |
| --- | --- |
| Overview and task guides | `README.md`, `content/_index.md`, `content/docs/` |
| Format rules | `docs/spec/packslip.md` (canonical specification) |
| CLI overview and help | `docs/cli.md` for the landing page; command and argument documentation in `src/main.rs` and `src/cli.rs` for the reference |
| Schema and validation | `src/model.rs` |
| Conformance vectors | `tests/conformance/` (see its README) |
| Creation and verification | `src/create.rs`, `src/verify.rs`, `src/sigstore.rs`; key signing in `src/dsse.rs` and `src/minisign.rs` |
| `release.toml` manifests | `src/manifest.rs` |
| Archives and `requires.libs` | `src/archive.rs`, `src/linkage.rs` |
| Forge identity and signer fingerprints | `src/forge.rs`, `src/fingerprint.rs` |
| Agent skill, published with every release | `skills/packslip/SKILL.md`; update it with the guides when CLI flags or action inputs change |
| GitHub Actions | `action.yml`, `releases/action.yml`, and `scripts/install-packslip.sh`, which both run |
| Container image `ghcr.io/jdx/packslip` | `container/Dockerfile`, built from a release's executables by `container/build.sh` |
| Test fixtures | `tests/fixtures/` (see its README) |
| Release example on the homepage and quickstart | `docs/examples/release-excerpt.json` |
| The site's Worker | `wrangler.jsonc`, `cloudflare/worker.js` |
| Install scripts that packslip.sh serves | `installer/install.sh` and `installer/install.ps1`, filled in for each release by `installer/render.sh` |
| packslip.sh's Worker | `cloudflare/installer/` |
| Site layout and styling | `layouts/`, `static/style.css`, `static/docs.js` |
| Social preview images | `layouts/partials/social-image.html`, `assets/social/` (see its README) |
| Release process | `RELEASING.md`, `.github/workflows/`, `release-plz.toml`, `cliff.toml` (changelog), `communique.toml` (release notes), `scripts/describe-release.sh` (packslip.dev's own packslip) |

`mise run render` generates `content/spec.md`, `content/cli/`,
`packslip.usage.kdl`, `packslip.1`, and `static/schema/`. Do not edit
them directly: edit their sources and run `mise run render` again, because
the next render overwrites any direct edit. release-plz adds each
release's section to `CHANGELOG.md` from the merged pull request titles;
edit that file by hand only when
[publishing a version release-plz would not propose](RELEASING.md#publishing-a-version-release-plz-would-not-propose).

### Change a format rule

The [conformance vectors](tests/conformance/README.md) express format rules
as test cases. Change the affected vectors in the same pull request as a
rule change, and explain whether you changed the specification or corrected
a vector that required behavior outside it. An editorial clarification
should leave the cases and expected results unchanged.

## Preview and build the docs

```sh
mise run docs        # Regenerate and serve with Hugo.
mise run docs:build  # Regenerate and build into public/.
mise run docs:check  # Build, check links, and run offline examples.
```

`mise run render` regenerates the
[generated files](#find-the-right-source) without starting Hugo. It
deletes and recreates `content/cli/`, combining `docs/cli.md` with the
generated command overview. Keep other handwritten pages in `content/docs/`.

`site.yml` deploys packslip.dev on every push to `main`, separately from
releases. A merged guide or specification change is live at once, and the
CLI reference shows `main`'s commands, which can be ahead of the latest
release. Before building, the workflow writes the current GitHub star
count to `data/github.json`; local previews use the checked-in value.

Keep guides focused on tasks and examples. Put normative format rules
in the specification and command details in CLI help. Check examples
against the implementation, distinguish draft integrations from shipped
features, and use site paths such as `/docs/verifying/` for internal links.

When renaming a section, update its links in the guides, specification,
fixture metadata, and repository documentation. `mise run docs:check`
checks links within the built site; check repository-only links separately.
Use ordinary Markdown headings in `docs/spec/packslip.md`, because it is
also rendered on GitHub and Hugo's `{#id}` syntax is not portable there.

## Check a change

Before you open a pull request, run what CI runs:

```sh
cargo fmt --all -- --check
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
# The verify-only build that library consumers use:
cargo clippy --no-default-features --all-targets -- -D warnings
cargo test --no-default-features
mise run lint        # Shellcheck the scripts the actions, workflows, and installer run.
installer/test.sh    # Run install.sh against a stand-in release with each local shell.
node --test cloudflare/installer/worker.test.mjs  # packslip.sh's Worker.
mise run docs:check  # Also runs `mise run render`.
mise exec -- usage lint packslip.usage.kdl
git status --short   # Lists generated files to commit.
```

[hk](https://hk.jdx.dev) runs the format and lint steps (`cargo fmt`,
`cargo clippy -D warnings`, `shellcheck`) from `hk.pkl`, and as a pre-commit
hook after `hk install`: `hk check --all` reports problems and `hk fix --all`
fixes them.

CI sets `RUSTFLAGS=-D warnings`, so any compiler warning fails CI. CI also
fails if `mise run render` changes a committed file, so commit regenerated
files with the source change that caused them.
`installer/test.sh` needs Python 3 for a local HTTP server, and the Worker
test needs Node.js 20 or newer. CI also runs the installer test on macOS
and Linux ARM64 and in Alpine and busybox containers, and runs
`installer/test.ps1` under Windows PowerShell 5.1 and PowerShell 7 on
Windows x64 and ARM64. Its `image` job builds the container image with
`container/build.sh` from the latest release's archives, which needs
`docker buildx` to repeat locally.
`cargo test --all-features` includes `tests/action.rs`, which runs
`action.yml`'s create step against a mock CLI; `cargo test --test action`
runs it alone. The `signing` job runs only on pushes to `main`, where a CI
identity is available. It signs both keylessly and with a key, records
both signatures in the public Rekor log, and runs the releases action.
zizmor audits the workflows on every pull request, as a job in `ci.yml`.
The `final` job in `ci.yml` gates on `test`, `signing`, and `zizmor`; make it
the required status check. The pull request title check stays in its own
workflow, `conventional-commits.yml`, because it runs on `pull_request_target`.

### Validate documentation

Run `mise run docs:check` and inspect the affected pages in a browser.
The check covers:

- Local links, anchors, assets, and the guide URLs listed in
  `scripts/check-docs.py`.
- One H1 per page, unique element IDs, and spacing around inline markup.
- The quickstart and release recipes, run in temporary directories with
  unlogged keys and no uploads.
- The shared homepage/quickstart example in
  `docs/examples/release-excerpt.json`, compared with the statement the
  quickstart creates.

If you move a guide, update the required paths in `scripts/check-docs.py`
and links that point to it. An alias in the page's front matter can keep an
old path resolving when that is useful.

### Keep executable examples in sync

`content/docs/getting-started.md` has exactly four blocks marked
`<!-- docs-test: quickstart -->` (setup, create, show, verify). The check
runs them in order in one temporary directory, and a marker on any other
page does nothing. Adding or removing a marked block fails the check until
`check_quickstart` in `scripts/check-docs.py` is updated. Commands that
install packslip are not marked and never run.

Each TOML block in `content/docs/recipes.md` marked
`<!-- docs-test: recipe NAME -->` needs a matching `NAME` entry in
`RECIPE_FILES` in `scripts/check-docs.py`, which describes its sample
archives; the check fails when the markers and the entries differ. Recipe
checks validate configuration and archive layout, not language builds or
platform signing.

When you try key-signed commands locally, pass `--no-log` to
`packslip create` (and to `packslip releases`), and `--allow-unlogged` to
`packslip verify`. Without `--no-log`, `create` records the test signature
in the public Rekor log.

## Submit a pull request

Pull requests are squash-merged. The title becomes the commit subject on
`main`, and the description becomes the commit body; Communiqué reads both
to write the release notes. The title is also the change's line in
`CHANGELOG.md` when the pull request changes a file in the Cargo package.
The `exclude` list in `Cargo.toml` leaves out the site (`content/`,
`layouts/`, `static/`), `.github/`, and `RELEASING.md`, among others, so a
change to the guides alone gets no changelog line.

CI rejects a title that is not a conventional commit:
`fix(verify): reject artifacts with mismatched digests` passes, and
`Fix verify` does not. [AGENTS.md](AGENTS.md) lists the allowed types and
explains how to write a title and description for release notes; its
rules apply to every contributor, not only to agents. Mark a breaking
change with `!` and explain it in a `BREAKING CHANGE:` footer at the end
of the description (see [RELEASING.md](RELEASING.md#major-versions) for
what that does to the version).

See [RELEASING.md](RELEASING.md) for maintainer setup and publication.
