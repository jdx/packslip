---
title: Compatibility and support
weight: 57
group: consume
description: Understand what a pinned packslip binary can keep verifying, which platforms are tested, and when an update may be needed.
---
# Compatibility and support

You can pin packslip independently of the tools it installs. Ordinary tool
updates do not require a matching packslip release: the [version 1 format
contract](/release/v1/#stability) keeps release metadata stable, and
`packslip install` refreshes authenticated Sigstore trust material without
replacing the executable. [Install a tool](/docs/bootstrap/) explains the
install command and its trust state.

Pinning the binary does not freeze trust or discovery metadata. Installation
still requires current authenticated metadata, and a security fix or an
unsupported future signing format may require a new binary. The evidence
below describes what this repository tests, rather than a promised number
of years an unchanged binary will work.

`packslip install` ships in v1.5.0 and newer. The older v1.4.0 verifier is
retained as a compatibility baseline; it has no installation command or
automatic trust-root refresh.

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

The real hk fixture checks authenticated historical log time. The unlogged
fixtures explicitly opt in to unlogged verification and test format
compatibility without relying on a live log. The installer also accepts a
2020 release in an unlogged fixture when its current list is fresh: the
age of the software alone does not make it ineligible.

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

These tests establish root rotation and failure behavior using controlled
dates and keys. They do not replay the production Sigstore TUF repository's
full historical root chain. Supporting a particular old production root
requires checking that chain and recording its dates. The server must
retain the intermediate roots needed to authenticate each transition.

Old signatures can remain verifiable at their authenticated signing time while
their historical keys and certificates remain trusted and available. Installation
also needs an available, eligible release and valid current trust and list
metadata. Historical compatibility never accepts an expired list, rolls back a
sequence, or bypasses a security rejection.

## Installation platforms and tool handoff

Required native CI tests installation on Linux x64/ARM64, macOS ARM64,
and Windows x64/ARM64. It covers discovery, extraction, ownership and
recovery, command exports, and host checks. Intel macOS is not a supported
installation target.

Distribution packages use the `install-cli` feature, which includes
verification and installation without publishing commands; see
[Distribution packages](/docs/distributions/).

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
| mise | Checked on Linux x64 with packslip 1.5.1: user installation, Bash activation, self-update, and reinstall; system installation and execution in a Debian 13 container |
| rustup | Pending real release, setup, and self-update checks |
| uv | Pending real release, setup, and self-update checks |
| pnpm | Pending real release, bundled-runtime, setup, and self-update checks |

The mise user-scope check installed 2026.9.18 into isolated installation
and command directories, keeping the archive's `bin`, `man`, and `share`
trees, README, and license. Through the exported command, Bash activation
and `mise exec` worked; self-update to 2026.10.1 kept the command working,
and reinstalling 2026.9.18 with packslip succeeded without `--force`.
The 2026.9.18 bundle's SHA-256 was
`f059ba8b18d5e8c4fb3f5e068dd0df72e84f61c3514a2b7ef7776e3cb81eecb2`.
The [Docker example](/docs/bootstrap/#bootstrap-mise-in-docker) was also
checked with mise 2026.10.1. These are Linux x64 checks; the other platform
and scope combinations remain unverified for mise's lifecycle.

Before claiming that a tool's full setup and self-update lifecycle is
supported, record its version, bundle digest, platform, scope, and packslip
version. In an isolated account, check its normal command, setup, and
self-update through the exported path. Confirm that adjacent runtime files
survive and that setup uses the intended scope.

After self-update, check that the exported command still works, then
reinstall through packslip to check ownership conflicts and recovery.
Record shell or PATH changes the tool makes itself. A successful
`--version` call establishes less than this full lifecycle check.

## Maintaining packaged verifiers

Authenticated root refresh can keep an existing binary useful while its
supported formats and algorithms remain usable. New bundle or log
formats, missing historical material, vulnerabilities, or changed
algorithms can require an update. Upstream does not promise a fixed
update cadence or maintenance horizon.

Keep verification and security fixes suitable for backporting to packaged
verifiers. For a compatibility break, release notes must identify the affected
versions and fixture/format, the reason, and the required update or publisher
change. Security rejection takes precedence over keeping a fixture green; document
the rejection and replace an acceptance assertion with the expected failure.
Distributions decide which packaged versions they maintain. A passing matrix is
evidence for that decision, not a promise that upstream maintains every release.
