---
title: Build an installer or mirror
weight: 55
group: consume
description: Find, verify, and select release artifacts in your own installer or mirror, keep the trust state the consumer rules require, and use the packslip crate as a library.
---
# Build an installer or mirror

This guide is for developers of installers, package managers, and mirrors
that read packslips. `packslip verify` checks one document at a time. A
consumer also finds the bundle, chooses the artifact for the host, and
remembers what it trusted from one install to the next. This page walks
through one install, summarizes the
[consumer rules](/release/v1/#consumer-rules), and shows the Rust library,
which implements the verification and selection steps.

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

The steps above are the main path. The
[consumer rules](/release/v1/#consumer-rules) are the complete contract,
and they also require state that lasts from one install to the next. Each
item below summarizes one rule, in the specification's order, and links
that rule by its number. Cite the rules by those numbers.

- **Pin the signer before the first install.** For a forge project, the
  name gives the first pin: the forge's issuer and an identity under the
  repository. Remember the repository ID the first accepted certificate
  records; from then on it pins the project across renames and transfers.
  A signer fingerprint the project publishes, read apart from its
  releases, such as in its README, is also part of the first pin, so no
  release has to be trusted on first use. For other projects, pin a key
  or identity as the rule describes. Trust a list from another publisher
  per host, by configuration. Never take a key from the document, and
  never trust a bundle's key hint. ([rule 1](/release/v1/#consumer-rules))
- **Verify the bundle, then every file you download.** Check the
  signature, certificate chain, and log entry, then the statement's
  structure, then that it names the project you asked for (or, on a
  forge, the same repository renamed), then the digest of every artifact
  and asset you downloaded and the size of every artifact. Refuse a bundle
  without a log entry unless you chose to accept unlogged bundles from
  this vendor. ([rule 2](/release/v1/#consumer-rules))
- **Keep signer continuity.** Remember the accepted signer, its scheme,
  its `attested_by`, its provenance links, its repository ID, and its
  `pin_workflow` value. Refuse a release whose scheme is weaker than the
  last accepted one, or that dropped per-artifact provenance the last
  release carried. Until a person approves it, also refuse one whose
  signer changed, that went from vendor to repackager (see
  [Repackager attestation](/release/v1/#repackager-attestation)), or that
  declares `pin_workflow: false` where you remembered `true`. Compare a
  keyless signer by workflow path, not ref, and across a rename or
  transfer only when the repository IDs match. A release that declares
  `pin_workflow: false` is compared by repository alone.
  ([rule 3](/release/v1/#consumer-rules))
- **Measure release age from the log.** Apply any minimum release age to
  the verified log integration time, and fall back to `published_at` only
  for an unlogged bundle you chose to accept.
  ([rule 4](/release/v1/#consumer-rules))
- **Discover through the release list**, as
  [rule 5](/release/v1/#consumer-rules) says:
  - Use GitHub's releases endpoint with the repository's supplementary
    list if it has one, or the signed list at the well-known URL, and
    refuse a project that has neither. Once you have accepted a
    supplementary list, treat its disappearance as an error.
  - Refuse an expired list or one whose `sequence` is below the last one
    you accepted, and persist the highest.
  - Never select a yanked entry, skip prereleases unless asked for them,
    rank by semver precedence, and take the vendor's recommendation for an
    unconstrained request as [Latest](/release/v1/#latest) says.
  - Refuse a packslip whose digest is not the one the list pins, or whose
    version is not the one its list entry named or, without an entry, its
    tag named.
  - If you trust stamping hosts, select only versions one of them lists,
    unless the user chose the vendor alone for that project.
- **Select one artifact.** Follow
  [Selecting an artifact](/release/v1/#selecting-an-artifact), and refuse
  to guess between two artifacts that tie.
  ([rule 6](/release/v1/#consumer-rules))
- **Check host requirements.** Before installing, check the selected
  artifact's `requires`: refuse when a library, glibc, or OS version means
  its executables cannot start, warn when a command is missing or too old,
  and treat a requirement you cannot check as a warning, never a refusal.
  Requirements do not resolve a tie.
  ([rule 7](/release/v1/#consumer-rules))
- **Select resources for the artifact.** For each thing the resources
  describe, keep the entries that fit the selected artifact and the most
  specific of those, then take the most verifiable source in the order
  [Source types](/release/v1/#source-types) gives. Run an `exec`
  completion on demand and cache it, and run any other `exec` entry only
  if the user chose to run vendor code at install time. Ignore kinds you
  do not know. A resource you cannot fetch is reported, not fatal; one
  whose digest does not match fails the install.
  ([rule 8](/release/v1/#consumer-rules))

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

That leaves the statement types, `verify`, `verify_release_list`,
`verify_forge`, `verify_forge_release_list`, `peek_unverified`,
`Fingerprint`, `select_artifact`, and `select_resources`, and drops the
archive readers, the executable decoder that derives `requires.libs`, the
signing path, the JSON Schema generator, and the CLI: about seventy fewer
crates in the dependency graph. The features are additive and all on by
default, so the binary and any dependent that keeps the default features
are unaffected:

| Feature | Adds |
| --- | --- |
| `cli` (default) | The `packslip` binary; implies the rest. |
| `create` | Build a statement from built artifacts; implies `archive`, `linkage`, and `sign`. |
| `archive` | Read tar and zip archives to resolve declared executable paths. |
| `linkage` | Derive `requires.libs` from ELF, Mach-O, and PE executables. |
| `sign` | Sign statements, keylessly through Fulcio or with a minisign key. |
| `manifest` | Read a TOML manifest (`release.toml`) for `create --manifest`. |
| `schema` | `Statement::schema()` and `ReleaseListStatement::schema()`. |

The samples below also use `serde_json`, and the release-list check
needs a `jiff::Timestamp` from `jiff` 0.2, the version packslip depends
on. packslip re-exports neither crate, so add both as your own
dependencies:

```sh
cargo add serde_json jiff@0.2
```

For a GitHub or GitLab project, `verify_forge` verifies a bundle under
the policy the forge implies and checks it against what the consumer
remembers, following renames and transfers by repository ID. This sample
follows steps 2 to 4 for a bundle already downloaded. `load_pins`,
`load_signer`, `download`, `store_pin`, and `store_signer` stand for the
consumer's own storage and download code:

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
