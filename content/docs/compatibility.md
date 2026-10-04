---
title: Compatibility and support
weight: 57
group: consume
description: The checked verification baseline, trust-root rotation scenarios, bootstrap platforms, and publisher adoption checks.
---
# Compatibility and support

The [version 1 format contract](/release/v1/#stability) fixes the meaning of
release metadata. A long-lived verifier also depends on bundle and log formats,
trusted signing material, and fresh discovery metadata. The matrix below records
what this repository tests. It does not promise that an unchanged binary will
verify every future release for a fixed number of years.

This page describes the current development branch. `install` and its platform
matrix have not shipped in a release. Packslip v1.4.0 is a verification baseline;
it has no bootstrap installer or TUF refresh command.

## Verification matrix

The required `compatibility` CI job builds v1.4.0 from commit
`87479dfc6443253dff69601cace5fc6ea07e6df5`, alongside the current verifier.
It runs both against the same documents in `tests/compatibility.rs`:

| Document | Checked behavior |
| --- | --- |
| Real hk v2.3.0 release | Sigstore bundle v0.3, keyless Fulcio certificate, Rekor `dsse` entry version `0.0.1`, explicit release-workflow identity and issuer |
| Newly generated release/v1 and releases/v1 | Sigstore bundle v0.3, unlogged Ed25519 signatures with a separately supplied public key; unknown optional fields and extensions preserve verification |
| New release with an unknown resource kind | Verification succeeds while the resource remains outside the consumer's known behavior |
| Modified artifact; signed release/v2 | Artifact verification fails; an unsupported predicate version is rejected |

The unlogged fixtures explicitly opt in to unlogged verification. They do not
exercise transparency-log integration or authorize unlogged production installs.
The real hk fixture does exercise authenticated historical log time. The installer
also accepts a 2020 release in its unlogged fixture when its current list is fresh;
there is no maximum release age.

This matrix currently covers bundle v0.3 and Rekor `dsse/0.0.1`. Other bundle or
log formats are not part of this checked support claim. Add a fixture, its signer
policy, and a matrix row before claiming another format. Keep existing baselines
as new baselines are added; do not advance the only baseline to the latest release.

Run the cross-version check locally after building the pinned baseline:

```sh
PACKSLIP_BASELINE_BIN=/absolute/path/to/v1.4.0/packslip \
  cargo test --locked --no-default-features --features install-cli --test compatibility
```

Without that variable, the tests cover the current binary alone. CI supplies the
baseline explicitly. Repository-ID fingerprint checks have their own current
verifier tests; v1.4.0 predates `--pin`.

## Trust-root lifetime and freshness

The required compatibility job also runs `trust_root::tests`. These use a signed
synthetic TUF repository with distinct keys and root versions 1, 2, and 3:

- Root 1 expired in 2020; first online use in 2026 authenticates two successive
  rotations before accepting the current target. Offline reuse in 2036 makes no
  fetches and succeeds only with unexpired authenticated metadata.
- A later packaged root 3 accepts authenticated history cached by the root-1
  consumer, online and offline.
- Network failure can reuse a fresh authenticated cache. Expired timestamp
  metadata, rollback, target corruption, and invalid online signatures fail.

These tests establish the rotation mechanism and failure behavior. They do not
replay the production Sigstore TUF repository's full historical root chain. A
production starting-root compatibility claim needs a checked historical chain
and its dates in this matrix. Rotation also depends on the server retaining the
intermediate roots; a client cannot invent a missing authenticated transition.

Old signatures can remain verifiable at their authenticated signing time while
their historical keys and certificates remain trusted and available. Installation
also needs an available, eligible release and valid current trust and list
metadata. Historical compatibility never accepts an expired list, rolls back a
sequence, or bypasses a security rejection.

## Bootstrap platform and handoff coverage

Required native CI runs extraction, ownership/recovery, command exports, host
checks, discovery, and installer fixtures on Linux x64/ARM64, macOS ARM64, and
Windows x64/ARM64. Intel macOS is unsupported. The installer feature includes the
verifier and excludes publishing, so distributions can maintain that feature
without taking publisher functionality.

The handoff fixture installs a complete tree containing a command and adjacent
runtime data, exports only that command, and verifies arguments and exit status.
It also relocates commands while retaining trust state and exercises key rotation,
administrator constraints, rollback, withdrawal, and failed replacement. Windows
has an additional real-console test for a child that handles Ctrl+C and Ctrl+Break.

These fixtures establish the installation mechanism. Publisher adoption remains
separate, because each tool's setup and self-update behavior determines whether
the resulting layout is suitable:

| Publisher | Adoption status |
| --- | --- |
| mise | Pending real release, setup, and self-update checks |
| rustup | Pending real release, setup, and self-update checks |
| uv | Pending real release, setup, and self-update checks |
| pnpm | Pending real release, bundled-runtime, setup, and self-update checks |

Before publishing instructions for a tool, record its version, bundle digest,
platform, installation scope, and Packslip version. Verify its ordinary command,
setup command, and self-update through the exported path in an isolated account.
Check that the complete runtime survives, setup uses the intended scope, and
self-update does not leave a launcher pointing to a removed executable. After
self-update, reinstall through Packslip and check ownership conflicts and recovery.
Record any deliberate shell or PATH changes made by the tool itself. Do not infer
adoption from a fixture or from a successful `--version` invocation alone.

## Maintaining packaged verifiers

Authenticated root refresh should not require a new binary when the supported
formats and algorithms remain usable. New bundle/log formats, missing historical
material, vulnerabilities, or changed algorithms can require an update. There is
no fixed annual update cadence, patch-size promise, or guaranteed maintenance
horizon.

Keep verification and security fixes suitable for backporting to packaged
verifiers. For a compatibility break, release notes must identify the affected
versions and fixture/format, the reason, and the required update or publisher
change. Security rejection takes precedence over keeping a fixture green; document
the rejection and replace an acceptance assertion with the expected failure.
Distributions decide which packaged versions they maintain. A passing matrix is
evidence for that decision, not a promise that upstream maintains every release.
