# Changelog

## [unreleased]

### 🚀 Features

- Monorepo subpath names (`github.com/owner/repo/tool`), one packslip per
  tool, `packslip.<subpath>.sigstore.json` file naming.
- Artifact `variant`, per-artifact `url`, `bin` entries with a PATH name
  distinct from the file (`{ path, name }`), `requires` (`os_min`,
  `glibc_min`), optional `sha512` digests, formats `tar.zst`, `tar.bz2`,
  `7z`, `msix`, `appimage`, `raw`.
- Release `prerelease`, `channel`, `notes_url`; release-list entries carry
  `prerelease`, `channel`, `status: yanked` with `status_reason`, and
  `security`.
- Repackager attestation: `attested_by: repackager` with `evidence`, for a
  repository that signs a packslip about a vendor's artifacts.
- `create` refuses two artifacts that share os, arch, libc, format, and
  variant.
- The action reads its default version from its own `Cargo.toml`, names
  the bundle after the project, and accepts `variants`, `prerelease`,
  `channel`, and `notes-url`.

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
