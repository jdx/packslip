# Changelog

## [0.2.0](https://github.com/jdx/packslip/compare/v0.1.0..v0.2.0) - 2026-09-03

### 🚀 Features

- One file per release, `packslip.sigstore.json`: a sigstore bundle
  carrying the statement as a DSSE envelope.
- `sigstore-key`: an Ed25519 key signs the envelope and the signature is
  logged to Rekor; `--no-log` and `verify --allow-unlogged` for air-gapped
  releases. Detached minisign signatures are no longer a scheme.
- Signed release lists (`releases/v1`) with expiry and sequence;
  `packslip releases` produces them.
- `packslip show` prints the statement.
- The action attests SLSA build provenance and links it from the packslip.

### 🚜 Refactor

- The L0 to L4 evidence scale left the spec; `verify` reports whether
  every artifact links provenance.

## [0.1.0] - 2026-09-03

### 🚀 Features

- First release: the crate, the `packslip` binary, the GitHub Action, the
  specification, and packslip.dev.
