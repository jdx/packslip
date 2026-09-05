# Contributing

packslip is a stable format at version 1 and a Rust reference
implementation of it. For a bug report or format feedback, open an
[issue](https://github.com/jdx/packslip/issues) with the use case and,
where possible, a small release layout that demonstrates it. A proposal
that changes what version 1 fixes is a version 2 proposal; the
specification's [Stability](https://packslip.dev/release/v1/#stability)
section says where that line falls.

## Set up the repository

Use Rust 1.95 or newer. The documentation tools are pinned in `mise.toml`
and `mise.lock`.

```sh
mise install
cargo build
cargo test --all-features
```

If your default Rust toolchain is older, select a compatible installed
toolchain, for example `RUSTUP_TOOLCHAIN=1.95.0 mise run docs`.

## Find the right source

| Area | Edit |
| --- | --- |
| Overview and task guides | `README.md`, `content/_index.md`, `content/docs/` |
| Format rules | `docs/spec/packslip.md` (canonical specification) |
| CLI help | Command and argument documentation in `src/main.rs` and `src/cli.rs` |
| Schema and validation | `src/model.rs` |
| Conformance vectors | `tests/conformance/` (see its README) |
| Creation and verification | `src/create.rs`, `src/verify.rs`, `src/sigstore.rs` |
| GitHub Actions | `action.yml`, `releases/action.yml`, and `scripts/install-packslip.sh`, which both run |
| The site's Worker | `wrangler.jsonc`, `cloudflare/worker.js` |
| Site layout and styling | `layouts/`, `static/style.css` |
| Release process | `RELEASING.md`, `.github/workflows/`, `release-plz.toml` |

`content/spec.md`, `content/cli/`, `packslip.usage.kdl`, and
`static/schema/` are generated. Edit their source and regenerate them;
direct edits will be overwritten.

A change to a rule the [conformance vectors](tests/conformance/README.md)
cover changes the vectors too, in the same pull request. They are the
specification in executable form, so a vector that has to be edited to
keep the tests passing is a signal: either the change is a specification
change, or the vector was describing behaviour the specification does not
require. Say which in the pull request.

## Preview and build the docs

```sh
mise run docs        # Regenerate and serve with Hugo.
mise run docs:build  # Regenerate and build into public/.
mise run docs:check  # Build, check links, and run offline examples.
mise run lint        # Shellcheck the scripts the actions and workflows run.
```

`mise run render` regenerates the CLI reference, usage spec, JSON schemas,
and specification page without starting Hugo. Commit those generated
changes with their source changes. Handwritten guides live outside the
generated CLI directory so regeneration preserves them.

Keep guides focused on tasks and examples. Put normative format rules
in the specification and command details in CLI help. Check examples
against the implementation, distinguish draft integrations from shipped
features, and use site paths such as `/docs/verifying/` for internal links.
Preserve existing specification anchors when reorganizing sections.

The site workflow updates the GitHub star count in `data/github.json`
before building, then deploys the built site as the packslip.dev Worker's
static assets. Local previews use the checked-in value.

## Check a change

For documentation changes, run `mise run docs:check` and inspect the affected
pages in a browser. The check validates local links, anchors, and assets;
runs the four marked quickstart blocks from Markdown; and creates and
verifies the recipe manifests using sample archives. It uses temporary
directories and unlogged keys, without uploading files or logging signatures.
The homepage and quickstart share `docs/examples/release-excerpt.json`,
which is checked against the generated quickstart statement.
It needs Python 3.9 or newer, a POSIX shell, and `tar`.

Use `<!-- docs-test: quickstart -->` before each executable walkthrough
block. Installation commands are not run by this check. Recipe TOML blocks
use `<!-- docs-test: recipe NAME -->`; maintain their sample archive layouts
in `scripts/check-docs.py` when changing a recipe. Recipe checks validate
configuration and archive layout, not language builds or platform signing.

For Rust changes, run the checks used by CI:

```sh
cargo fmt --all -- --check
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
mise run render
mise exec -- usage lint packslip.usage.kdl
mise run docs:check
```

CI checks that regeneration leaves the working tree unchanged. Signing
integration jobs run on `main`, where a CI identity is available. Local
key-signed tests can use `--no-log` with `--allow-unlogged` to avoid
publishing test signatures to Rekor.

See [RELEASING.md](RELEASING.md) for maintainer setup and publication.
