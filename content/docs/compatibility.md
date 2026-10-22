---
title: Compatibility and support
description: What an unchanged packslip binary keeps verifying, what can force an update, and which platforms and versions are tested.
group: build
weight: 70
---

# Compatibility and support

Packagers and teams that keep packslip at a fixed version can find what an
unchanged binary keeps doing, what forces an update, and what each release
is tested against. Publishers can find what to check before recommending
`packslip install`. To install packslip, see [Install
packslip](/docs/install/).

## What an unchanged binary keeps doing

An older packslip can stay your bootstrapper while the tools it installs
move on. Ordinary tool updates need no matching packslip release, because
the [version 1 format contract](/release/v1/#stability) keeps release
metadata stable. A new major version of the CLI, library, or action leaves
the format at version 1. [Maintaining packaged
verifiers](#maintaining-packaged-verifiers) lists what can force an update.

## Supported platforms

`packslip install` runs on Linux x64 and ARM64, macOS ARM64, and Windows
x64 and ARM64. Elsewhere, Intel macOS included, it fails with
`unsupported bootstrap host <os>/<arch>`, such as `darwin/x86_64`.
`verify` and `pin` have no host check, so a source build that compiles
there can run them. [Check for a release
build](/docs/install/#check-for-a-release-build) lists the release builds.

Every release must pass the [install tests](#check-a-tools-handoff)
natively on those five platforms. It must also build and install the
Debian and RPM packages from the offline source archive on Linux x64 and
ARM64; see [Package packslip for a distribution](/docs/distributions/).

## Version requirements

First packslip release with each feature:

- 1.5.0: `packslip install` and its trust-root refresh, `packslip pin`,
  `--pin` with a `ps1_` signer fingerprint, the `install-cli` and
  `verify-cli` builds, and the agent skill declared in packslip's own
  releases.
- 1.5.1: `packslip install` under a group-writable umask, such as Debian's
  `002`.
- 1.9.1: the Go module through the Go module proxy.

Releases up to 1.10.1 differ from later ones:

- They refuse an empty release list, as do mise releases built on those
  packslip versions; see [Manage release lists](/docs/release-lists/).
- Without trust flags, `verify` and `pin` refuse a release signed through
  another repository's reusable workflow.
- `verify` has no `--allow-other-repository`.
- Identity flags on `verify` replace the policy a forge project's name
  implies instead of narrowing it. So `--issuer` alone accepts a bundle
  signed in any repository.
- The npm package matches `project` as a string and does not bind a GitHub
  or GitLab project to its repository. A verifier file copied from those
  release tags behaves the same way.

Update packslip, the npm package, or the copied verifier to get the
[repository binding](/docs/trust/#what-a-project-name-says-about-the-signer).
[Check what install supports](/docs/bootstrap/#check-what-install-supports) lists how
`packslip install` differs in those releases.

Other version differences:

- A forge pin that the 1.4 Rust crate stored with an owner ID still
  deserializes; later versions ignore the owner ID.
- The `jdx/packslip@v0` action tag stopped advancing at 0.3.1. A workflow
  on `@v0` keeps working but gets no updates until it moves to `@v1`.

The mise `packslip:` backend ([Use packslip with mise](/docs/mise/))
first shipped in mise 2026.9.2. Later releases added:

- 2026.9.4: man pages from a release's `man` resource. A tool that an older
  mise installed needs one reinstall to get them:

  ```sh
  mise install --force packslip:github.com/jdx/hk
  ```

- 2026.9.10: installing an exact or locked version without waiting for
  `minimum_release_age`.
- 2026.9.16: recording the forge's repository ID with a pin.

## Verification matrix

Each release's tests run packslip 1.4.0 and the current verifier on the
same inputs, and both must give these results:

- The real hk v2.3.0 release (Sigstore bundle v0.3, keyless Fulcio
  certificate, Rekor `dsse` 0.0.1 entry, explicit release-workflow identity
  and issuer) verifies at its authenticated historical log time.
- A newly generated release bundle and release list (Sigstore bundle v0.3,
  unlogged Ed25519 signatures, separately supplied public key) verify, even
  with unknown optional fields and extensions.
- A new release with an unknown resource kind verifies; the resource stays
  outside the consumer's known behavior.
- A modified artifact fails the artifact check.
- A signed `release/v2` statement is rejected as an unsupported predicate
  version.

The checked support claim covers only Sigstore bundle v0.3 and Rekor
`dsse/0.0.1` entries.

## Trust-root lifetime and freshness

- `packslip install` starts from the TUF root built into the binary,
  refreshes the trust root from Sigstore's TUF repository, and caches the
  authenticated metadata; `--offline` uses only that cache.
  `--trusted-root` replaces the refresh with an administrator-supplied
  `trusted_root.json`.
- `packslip verify` and `packslip pin`, in every build, use the
  `trusted_root.json` built into the binary and never refresh it. Once
  Sigstore signs with a certificate authority or log key that root lacks,
  new signatures fail until you update packslip. An administrator can
  instead pass a current `trusted_root.json`, authenticated through
  Sigstore's TUF repository, with `--trusted-root`.

The trust-root tests refresh from a signed synthetic TUF repository with
distinct keys and root versions 1, 2, and 3:

- With root 1 expired in 2020, first online use in 2026 authenticates two
  successive rotations before accepting the current target.
- Offline reuse in 2036 makes no fetches and succeeds only while the cached
  authenticated metadata is unexpired.
- A later packaged root 3 accepts the history a root-1 consumer cached,
  online and offline.
- After a network failure, a fresh authenticated cache can be reused.
- Expired timestamp metadata, rollback, target corruption, and invalid
  online signatures fail.

These tests establish rotation and failure behavior but do not replay the
production Sigstore TUF repository's historical root chain. An old
binary's `install` can catch up only while that repository keeps every
intermediate root each transition needs.

Installing also needs an eligible release and current trust and list
metadata. Age alone does not make a release ineligible: an installer test
accepts a release published in 2020 when its list is fresh. Compatibility
never makes packslip accept an expired list, a list sequence lower than
one already accepted, or a release a security fix rejects; see [Freshness
and rollback protection](/release/v1/#freshness-and-rollback-protection).

## Maintaining packaged verifiers

These can require a packslip update:

- a Sigstore certificate authority or log key newer than the built-in
  trust root, for `verify`, `pin`, and `verify-cli` builds, which never
  refresh it ([Trust-root lifetime and
  freshness](#trust-root-lifetime-and-freshness))
- a security fix
- a new Sigstore bundle or log format
- historical keys or certificates that are no longer available
- a changed signing algorithm

Upstream promises no fixed update cadence, maintenance horizon, or number
of years an unchanged binary will work. It keeps verification and security
fixes suitable for backporting to packaged verifiers. When a release breaks
compatibility, its release notes name the affected versions, the format or
test input, the reason, and the update or publisher change you need. A
security rejection takes precedence over keeping a compatibility test
passing.

Distributions decide which packaged packslip versions they maintain, with
the [verification matrix](#verification-matrix) as evidence.

## Check a tool's handoff

Every release must pass tests of how `packslip install` lays out and hands
off a tool:

- A tree with a command and adjacent runtime data installs, only that
  command is exported, and arguments and exit status pass through.
- Reinstalling to new tree and command directories keeps trust state.
- Discovery, extraction, host checks, ownership and recovery, key
  rotation, administrator constraints, list rollback, withdrawal, and
  failed replacement are exercised.
- Signature, digest, version, and policy conflicts are refused, even with
  `--force`.
- On Windows, a real-console test checks a child process that handles
  Ctrl+C and Ctrl+Break.

These tests do not establish that a tool's own setup and self-update work
with the layout. Only mise has been checked by hand, on Linux x64 with
packslip 1.5.1:

- In user scope, packslip installed mise 2026.9.18, whose bundle's SHA-256
  was `f059ba8b18d5e8c4fb3f5e068dd0df72e84f61c3514a2b7ef7776e3cb81eecb2`.
  Bash activation and `mise exec` worked through the exported command.
  Self-update to 2026.10.1 kept it working, and packslip then reinstalled
  the earlier mise without `--force`.
- In system scope, mise installed and ran in a Debian 13 container.
- [Bootstrap mise in Docker](/docs/mise/#bootstrap-mise-in-docker) was
  checked with mise 2026.10.1.

Other platform and scope combinations are unverified for mise's lifecycle.

Before you document `packslip install` for a tool on a new platform or
scope:

1. Record the tool's exact release and bundle digest, the platform, the
   scope, the packslip version, and the tree and command paths.
2. In an isolated account, run the tool's normal command, setup, and
   self-update through the exported command.
3. Check that runtime files beside the command survive and are found, that
   arguments and exit status pass through, and that setup uses the
   intended scope.
4. Note whether self-update replaces the executable or its exported
   command. Record any shell or PATH changes the tool makes itself.
5. After self-update, check that the exported command still works. Then
   reinstall through packslip and check that it handles the changed
   ownership, including conflicts and recovery.

A successful `--version` call establishes less than this check.
