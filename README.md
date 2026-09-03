# packslip.dev

The website for [packslip](https://packslip.dev), a signed release manifest
for vendor binaries: one signed document per release that says what shipped
and how to verify it, checked by any consumer with a single pinned key.

This repository holds only the site, served by GitHub Pages from `main`.

- `index.html` — overview, evidence levels, vendor and consumer quickstart.
- `release/v1/` — the specification. The predicate type
  `https://packslip.dev/release/v1` resolves here.
- `schema/release-v1.json` — the JSON schema, as printed by `packslip schema`.

The specification's canonical text and the reference implementation (Rust
crate and `packslip` binary) live in
[jdx/omapac](https://github.com/jdx/omapac/tree/main/crates/packslip) under
`docs/spec/packslip.md` and `crates/packslip`. Changes to the spec land there
first and are mirrored here.

MIT licensed.
