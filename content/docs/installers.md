---
title: Build an installer or mirror
weight: 55
group: consume
description: Find, verify, and select release artifacts in your own installer or mirror, keep the trust state the consumer rules require, and use the packslip crate as a library.
---
# Build an installer or mirror

Build an installer, package manager, or mirror that consumes signed
packslip releases. The work has three parts: find an eligible release,
verify and select its files, and preserve trust across later installs.
This guide follows that sequence and shows the Rust library APIs you
can reuse.

If you need an existing installer, [Install a tool](/docs/bootstrap/)
covers `packslip install`. For manual checks of downloaded files, use
[`packslip verify`](/docs/verifying/). The rest of this page is for
implementing a consumer yourself.

## Install a release

1. **Find the bundle.** List the project's versions, choose one, and
   locate its bundle, as [Discovery](/release/v1/#discovery) says.
   - For a GitHub project, list the repository's releases.
     `packslip::tag_version` maps each tag to the version it names, so you
     can list versions without downloading a bundle per release. If the
     repository carries a supplementary list (`packslip::github_list_path`
     gives its path), verify it too: for a version it lists, its entry
     decides the tag, the status, and the bundle digest to accept, so you
     never choose a version it yanks. Then read the chosen release's
     `packslip*.sigstore.json` assets and keep the one whose
     `predicateType` is `https://packslip.dev/release/v1` and whose
     `project` is the name you asked for, or the same tool of the
     repository renamed. Never trust the file name;
     `packslip::peek_unverified` reads both fields without verifying.
   - For a project on its own domain, verify the signed release list at
     the URL `packslip::list_url` gives, choose an entry, and download the
     bundle its `packslip` URL names.
2. **Verify the bundle alone** against your pin, before you use anything
   else it says. When a release list entry names the bundle, first refuse
   the file unless its SHA-256 is the digest the list gives for that URL.
   After verifying, check that the verified project is the one you asked
   for and that its version is the one the list entry named or, without an
   entry, the tag named.
3. **Choose an artifact.** Read the verified statement's artifacts, with
   `packslip show` or by decoding the verified payload into
   `packslip::Statement`, and choose one as
   [Selecting an artifact](/release/v1/#selecting-an-artifact) says.
   `packslip::select_artifact` implements that selection. It takes a
   `packslip::Host` spelled in the specification's
   [vocabulary](/release/v1/#vocabularies), which differs from Rust's in
   places: `std::env::consts::OS` reports `macos` for `darwin`, and
   `consts::ARCH` reports `x86` for `i686`, `arm` for both `armv6` and
   `armv7`, and `powerpc64` for `powerpc64le`. Then check the chosen
   artifact's `requires` against the host.
4. **Download the artifact and verify it** against the same bundle with
   the same pin before you unpack it. With the CLI, pass it with
   `--artifact`, as [Verify a release](/docs/verifying/#check-every-file-you-use)
   shows. Verify each resource asset you download the same way.

Implementations in other languages can check their artifact and resource
selection, tag parsing, statement validation, forge identity, and signer
fingerprints against the
[conformance vectors](https://github.com/jdx/packslip/tree/main/tests/conformance).

## Follow the consumer rules

Verification is one part of an install. The [consumer
rules](/release/v1/#consumer-rules) define the complete contract; use this
checklist to find the parts your implementation still needs.

| Rule | Responsibility | What to implement beyond checking a signature |
| --- | --- | --- |
| 1 | Pin the signer | Establish trust from configuration, a public key, a forge identity, or a published signer fingerprint. Never take a trusted key from the bundle itself. Remember forge repository IDs across renames and transfers. |
| 2 | Verify the bundle and files | Validate the statement, compare its project with the request, and check every downloaded artifact and resource asset. Require log evidence unless the user has explicitly accepted unlogged releases. |
| 3 | Preserve signer continuity | Refuse a weaker scheme, lost provenance, or a change from vendor to repackager. Require approval for signer changes or relaxed workflow pinning. Compare with the last accepted release, including across repository renames. |
| 4 | Apply release-age policy | Measure any minimum age from authenticated log integration time; use `published_at` only for deliberately accepted unlogged bundles. |
| 5 | Discover eligible releases | Verify release lists, enforce expiry and sequence, honor withdrawals and version policy, and check the selected bundle's digest and version. Enforce configured stamping-host policy too. |
| 6 | Select one artifact | Apply the specified platform and format preferences; fail when two candidates tie. |
| 7 | Check the host | Check declared libraries, glibc, OS, and commands before installing. Requirements do not break an artifact-selection tie. See [Host requirements](/docs/host-requirements/). |
| 8 | Select resources | Choose the most specific matching resource and its strongest available source. Run vendor code only as the resource rules permit; missing resources are reported, while digest mismatches fail installation. |

The rule numbers above refer to the specification. In particular,
[discovery](/release/v1/#discovery), [artifact
selection](/release/v1/#selecting-an-artifact), and [resource
selection](/release/v1/#resources) have details that a signature
verifier cannot enforce for you.

## Keep trust state between installs

The [consumer rules](/release/v1/#consumer-rules) list the state to keep
for each project: the pin (the signer, its repository ID, and the
remembered `pin_workflow` value), the trust properties of the last
accepted release (its scheme, `attested_by`, and provenance), the highest
release-list `sequence` you accepted and whether you accepted a signed
list at all, and the choices a person made, such as an approved signer
change or an allowed unlogged bundle.

A lockfile can carry a project's pin alongside artifact URLs and digests
so another machine can enforce it on its first install. Local state can
separately remember signer history and release-list sequences. The
specification does not say where a consumer stores either.
[Use packslip with mise](/docs/mise/#preserve-trust-across-upgrades-and-machines)
shows one consumer that keeps both. For how a vendor publishes a list,
withdraws versions, and recommends a default, see
[Manage release lists](/docs/release-lists/).

## Use the Rust library

The `packslip` crate holds the schema, the verifier, and the selection
rules as well as the CLI and the generator. A consumer that only verifies
takes it without the parts it will not call:

```sh
cargo add packslip --no-default-features
```

This keeps the statement types, `verify`, `verify_release_list`,
`verify_forge`, `verify_forge_release_list`, `peek_unverified`,
`Fingerprint`, `select_artifact`, and `select_resources`. It omits archive
readers, executable inspection, signing, schema generation, and the CLI.
Enable the features you need; the default `cli` feature includes the
complete command-line application:

| Feature | Adds |
| --- | --- |
| `cli` (default) | The complete binary: installation, verification, publishing, and schema generation. |
| `verify-cli` | The verifier binary: `verify`, `pin`, `show`, completions, and version, without publishing, archive inspection, or schema generation. |
| `install-cli` | The verifier commands plus `install`, complete-tree extraction, native command exports, and host checks; excludes publishing and schema generation. |
| `create` | Build a statement from built artifacts; implies `archive`, `linkage`, and `sign`. |
| `archive` | Read tar and zip archives to resolve declared executable paths. |
| `linkage` | Derive `requires.libs` from ELF, Mach-O, and PE executables. |
| `sign` | Sign statements, keylessly through Fulcio or with a minisign key. |
| `manifest` | Read a TOML manifest (`release.toml`) for `create --manifest`. |
| `schema` | `Statement::schema()` and `ReleaseListStatement::schema()`. |

Distribution packagers can build the installer without the publishing stack:

```sh
cargo build --locked --release --no-default-features --features install-cli
```

The installer build omits `create`, `releases`, `keygen`, and `schema`.
Use `verify-cli` for a verifier-only binary, or `--no-default-features`
for the library alone. The default build includes every CLI command.

The samples below also use `serde_json`, and the release-list check
needs a `jiff::Timestamp` from `jiff` 0.2, the version packslip depends
on. packslip re-exports neither crate, so add both as your own
dependencies:

```sh
cargo add serde_json jiff@0.2
```

For a GitHub or GitLab project, `verify_forge` verifies a bundle under
the policy the forge implies and checks it against what the consumer
remembers, following renames and transfers by repository ID. The following
Rust fragment follows steps 2 to 4 for a bundle already downloaded. `load_pins`,
`load_signer`, `download`, `store_pin`, and `store_signer` stand for your
own storage and download functions; the fragment is not a complete
installer. Add host checks and the rest of the checklist above before
installing the verified file:

```rust
use packslip::forge::{Continuity, Expected, ForgePin, PinSource};

let bundle = std::fs::read_to_string("packslip.sigstore.json")?;
let root = packslip::sigstore::trusted_root(None)?;
let options = packslip::Options { require_log: true, trusted_root: &root };

// The pins this consumer stored after earlier installs: its own and the
// lockfile's. The release must satisfy each one.
let pins: Vec<(PinSource, ForgePin)> = load_pins("github.com/jdx/hk");
let expected = Expected::new("github.com/jdx/hk").pinned_by(&pins);

// Verify the bundle alone, then hold it to the signer accepted last time
// and to whether that signer pinned its workflow. Rule 3 also compares the
// scheme, attested_by, and provenance links with the last release.
let accepted = packslip::verify_forge(&bundle, &expected, options, &[])?;
if let Some((previous, pinned_workflow)) = load_signer("github.com/jdx/hk") {
    if !accepted.check.continues_signer(&previous)
        || (pinned_workflow && !accepted.check.pins_workflow())
    {
        return Err("the release lowers remembered trust; ask a person first".into());
    }
}
// Step 2: the version must be the one the tag or list entry named.
if accepted.verified.version != "2.3.0" {
    return Err("the packslip is not the version its tag named".into());
}
if let Continuity::Renamed { signed, .. } = &accepted.check.continuity {
    eprintln!("github.com/jdx/hk is now {signed}");
}

// Choose the artifact from the statement in the bundle that just verified.
let payload = packslip::sigstore::peek_statement(&bundle)?;
let statement: packslip::Statement = serde_json::from_slice(&payload)?;
let host = packslip::Host { os: "linux", arch: "x86_64", libc: Some("gnu") };
let artifact = packslip::select_artifact(
    &statement.predicate.artifacts,
    &host,
    None,
    &["tar.xz", "tar.gz", "zip"],
)?;

// Download it under its own file name and verify it against the same
// bundle. Only then remember the pin and the signer.
let file = download(artifact)?;
packslip::verify_forge(&bundle, &expected, options, &[file.as_path()])?;
if let Some(pin) = &accepted.check.pin {
    store_pin("github.com/jdx/hk", pin);
}
store_signer(
    "github.com/jdx/hk",
    &accepted.verified.key_id,
    accepted.check.pins_workflow(),
);
```

`packslip::select_resources(&statement, artifact)` then gives the resource
entries for the chosen artifact.

To hold a release to a signer fingerprint the user configured, check it
right after `verify_forge`, which has already confirmed that the
certificate's repository is the one the statement names:

```rust
let want: packslip::Fingerprint = "ps1_snirenkjwr7m5ozgcufameodnm".parse()?;
want.verify(accepted.verified.issuer.as_deref(), accepted.check.source.as_ref())?;
```

A malformed fingerprint fails to parse rather than counting as no pin.

The library provides these pieces for signer continuity:

- `accepted.check.continues_signer(&previous)` is the signer-continuity
  check of [rule 3](/release/v1/#consumer-rules). It compares a remembered
  signer with this release's across a rename or a transfer. Call it before
  you accept the release.
- `accepted.check.pin` is what to store once you accept the release: the
  repository ID and the name the release was signed under, not the owner.
  A repository that moves to another owner is reported as
  `Continuity::Renamed`, and releases from before the move still verify. A
  pin that packslip 1.4 stored with an owner ID still deserializes; the
  owner ID is ignored.
- `accepted.check.pins_workflow()` is `false` for a release that declares
  `pin_workflow: false`. A vendor whose releases are signed by more than
  one workflow file of its repository, such as separate stable and nightly
  workflows, declares it with `packslip create --no-pin-workflow`, so that
  consumers hold later releases to the repository instead of one workflow
  file. `continues_signer` then accepts any workflow of the repository.
  Store the value beside the signer and apply the downgrade rule in
  [Reusable workflows](/release/v1/#reusable-workflows).
  `packslip verify --json` reports `"pin_workflow": false` for such a
  release and omits the field otherwise.
- `packslip::forge::same_workflow` compares two remembered signers, each
  with the pin recorded beside it, when no release is at hand, such as
  when regenerating a lockfile. It always compares workflow paths, so for
  a signer accepted with `pin_workflow: false`, compare only the forge and
  the repository ID instead, as `ForgePin::continues` does.

A release from a different repository under a pinned name fails
`verify_forge` with an error that says which pin disagreed and ends with
`the name belongs to a different repository now`. The name has been
recreated after a rename or deletion: find where the project went, and do
not pin the name again.

`packslip::peek_unverified` reads the project and version a bundle claims
without verifying it, so an installer can choose a policy before
verifying. Treat both as untrusted, and take the project and version from
the verified result.

For release lists, `verify_release_list` verifies a domain project's list
under a `packslip::Trust` you hold, and `verify_forge_release_list` a
GitHub repository's supplementary list under the policy the forge implies.
Neither checks expiry or sequence. With `list` the verified statement
(`VerifiedList::list`), refuse it unless `list.is_current(now)` is true
for `now` from `jiff::Timestamp::now()` and `list.predicate.sequence` is
at least the highest you accepted. `list.digest_of(url)` then gives the
SHA-256 the list pins for a bundle URL, which step 2 compares with the
downloaded bundle; `packslip::digest_file` hashes the file.
