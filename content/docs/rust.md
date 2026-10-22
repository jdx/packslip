---
title: Use the Rust crate
description: Add the packslip crate to a Rust program to verify releases, select artifacts, hold projects to their signers, and create signed statements.
group: build
weight: 62
---
# Use the Rust crate

Rust developers can verify releases, select artifacts, and publish releases
from their own programs with the packslip crate, enabling only the features
they need. For an installer's steps, read
[Build an installer or mirror](/docs/installers/) first; for Go or
JavaScript, see [Choose a library](/docs/libraries/).

## Add the crate

A consumer that only verifies adds the crate without default features:

```sh
cargo add packslip --no-default-features
```

This verify-only build keeps the statement types, `verify`,
`verify_release_list`, `verify_forge`, `verify_forge_release_list`,
`peek_unverified`, `Fingerprint`, `select_artifact`, and
`select_resources`. It omits the statement generator, archive readers,
executable inspection, signing, schema generation, and the CLI.

The examples also use `serde_json` and a `jiff::Timestamp` from jiff 0.2,
the version packslip uses. The crate re-exports neither:

```sh
cargo add serde_json jiff@0.2
```

The crate needs Rust 1.93 or newer. It shares the CLI's version
([Match versions across packages](/docs/libraries/#match-versions-across-packages)),
and a breaking API change makes a new major version.
[docs.rs](https://docs.rs/packslip) documents every public item, with all
features enabled.

## Choose features

| Feature | Adds |
| --- | --- |
| `cli` (default) | The complete binary. Implies `install-cli`, `create`, `manifest`, and `schema`. |
| `verify-cli` | The verifier binary: `verify`, `pin`, `show`, `completion`, and `version`. No publishing, archive inspection, or schema generation. |
| `install-cli` | `verify-cli` plus `install`, complete-tree extraction, native command exports, and host checks. No publishing or schema generation. |
| `create` | Build a statement from built artifacts (`create`). Implies `archive`, `linkage`, and `sign`. |
| `archive` | Read tar and zip archives to resolve declared executable paths. |
| `linkage` | Derive `requires.libs` from ELF, Mach-O, and PE executables. Implies `archive`. |
| `sign` | Sign keylessly through Fulcio or with an Ed25519 key from `packslip keygen` (`sigstore::sign`). Verification never needs it. |
| `manifest` | Read release.toml (`Manifest`) for `create --manifest`. |
| `schema` | `Statement::schema()`, `ReleaseListStatement::schema()`, and `verify_report_schema()`. |
| `trust-root` | Refresh the Sigstore trust root over TUF, with an authenticated offline cache (`trust_root`). |
| `discovery`, `install-policy`, `install-extract`, `install-fs`, `install-launcher`, `install-host` | The modules of `packslip install`; see [Use the installer's modules](#use-the-installers-modules). |

To build the packslip binary for a distribution, see
[Package packslip for a distribution](/docs/distributions/).

## Discover versions and bundles

These functions find a release before anything is verified:

- `packslip::tag_version(tag, project)` maps a tag to the version it names,
  so an installer can list a forge project's versions without downloading
  a bundle per release. It tries every allowed tag prefix; a dashed date
  tag names no version.
- `packslip::github_list_path(project)` gives the path of a GitHub
  repository's supplementary release list on its default branch.
- `packslip::list_url(publisher, project)` gives the URL of `publisher`'s
  release list of `project`. Pass the project's own host for its own list.
- `packslip::peek_unverified(bundle)` reads a bundle's `predicateType`,
  `project`, and version without verifying it, to find the requested
  project's bundle and choose a policy. Treat them as untrusted, and take
  the project and version from the verified result.

```rust
assert_eq!(packslip::tag_version("v2.3.0", "github.com/jdx/hk").as_deref(), Some("2.3.0"));
assert_eq!(packslip::tag_version("jq-1.7.1", "github.com/jqlang/jq").as_deref(), Some("1.7.1"));
assert_eq!(packslip::tag_version("2026-08-31", "github.com/owner/mytool"), None);
assert_eq!(
    packslip::github_list_path("github.com/jdx/hk").as_deref(),
    Some(".well-known/packslip.json"),
);
assert_eq!(
    packslip::list_url("mytool.example.com", "mytool.example.com"),
    "https://mytool.example.com/.well-known/packslip.json",
);
```

## Verify a forge project's release

`verify_forge` verifies a GitHub or GitLab forge project's bundle under the
policy the forge name implies, holds it to the repository pins you stored,
and follows renames and transfers by repository ID. In this example,
`load_pins`, `load_signer`, `download`, `store_pin`, and `store_signer`
stand for your own storage and download functions:

```rust
use std::error::Error;
use std::path::PathBuf;

use packslip::forge::{Continuity, Expected, ForgePin, PinSource};
use packslip::{Host, Options, Statement};

/// `version` is the version the bundle's tag or list entry named.
fn verify_and_fetch(
    project: &str,
    version: &str,
    bundle: &str,
) -> Result<PathBuf, Box<dyn Error>> {
    // The built-in trust root, with a transparency-log entry required.
    let root = packslip::sigstore::trusted_root(None)?;
    let options = Options { require_log: true, trusted_root: &root };

    let pins: Vec<(PinSource, ForgePin)> = load_pins(project);
    let expected = Expected::new(project).pinned_by(&pins);

    // Verify the bundle alone, then hold it to the signer accepted last time.
    let accepted = packslip::verify_forge(bundle, &expected, options, &[])?;
    if let Some((previous, pinned_workflow)) = load_signer(project) {
        if !accepted.check.continues_signer(&previous)
            || (pinned_workflow && !accepted.check.pins_workflow())
        {
            return Err("the signer changed; ask a person before accepting".into());
        }
    }
    if accepted.verified.version != version {
        return Err("the bundle is not the version its tag named".into());
    }
    if let Continuity::Renamed { signed, .. } = &accepted.check.continuity {
        eprintln!("{project}: this release was signed as {signed}");
    }

    // Choose an artifact from the statement that just verified.
    let payload = packslip::sigstore::peek_statement(bundle)?;
    let statement: Statement = serde_json::from_slice(&payload)?;
    let host = Host { os: "linux", arch: "x86_64", libc: Some("gnu") };
    let artifact = packslip::select_artifact(
        &statement.predicate.artifacts,
        &host,
        None,
        &["tar.xz", "tar.gz", "zip"],
    )?;
    // Check artifact.requires against this host here (consumer rule 7).

    // Download it under its own file name and verify it against the same
    // bundle. Only then remember the pin and the signer.
    let file = download(artifact)?;
    packslip::verify_forge(bundle, &expected, options, &[file.as_path()])?;
    if let Some(pin) = &accepted.check.pin {
        store_pin(project, pin);
    }
    store_signer(project, &accepted.verified.key_id, accepted.check.pins_workflow());
    Ok(file)
}
```

- `pinned_by(&pins)` takes your stored `(PinSource, ForgePin)` pairs,
  `PinSource::Local` for this machine's and `PinSource::Lockfile` for a
  lockfile's; the release must satisfy every one. With no stored pin,
  `Expected::new(project).resolved(Some(id))` adds the repository ID the
  forge's API gives for the name, as `packslip install` does.
- `Continuity::Renamed { signed, .. }` reports a repository renamed or
  transferred to another owner; `signed` is the name this release was
  signed under. Releases from before the move still verify.
- The later snippets run inside a function like this one and reuse its
  `project`, `options`, and `accepted`.

The function covers steps 2, 3, and 5 of
[Install a release](/docs/installers/#install-a-release) and is not a
complete installer. It skips host checks and does not compare the previous
release's `scheme`, `attested_by`, or provenance; `packslip install` makes
those comparisons in its `install_policy` module. Add them and the rest of
[Follow the consumer rules](/docs/installers/#follow-the-consumer-rules)
before installing.

`verify_forge` refuses a bundle signed outside the project's repository,
such as a repackager's. Verify that with `packslip::verify` and a `Policy`
naming the repackager's signer.

## Hold a project to its signer

[The example above](#verify-a-forge-projects-release) stores two values
once it accepts a release:

- The pin, `accepted.check.pin`: the repository ID and the name the release
  was signed under, with no owner ID. It is `None` when the certificate
  records no repository ID.
- The signer, `accepted.verified.key_id`, with
  `accepted.check.pins_workflow()`, which is `false` for a release that
  declares `pin_workflow: false`.

`continues_signer(&previous)` is the signer-continuity check of
[consumer rule 3](/release/v1/#consumer-rules), across renames and
transfers. For a release that declares `pin_workflow: false`, it accepts
any workflow of the repository. When the stored signer pinned its workflow,
the example also refuses a release whose `pins_workflow()` is false. When a
person approves a change the example refused, store the new signer and its
`pins_workflow()` value, as
[Workflow pinning](/release/v1/#workflow-pinning) requires.
[Signer continuity and workflow pinning](/docs/trust/#signer-continuity-and-workflow-pinning)
explains which signers continue each other.

A pinned name that now belongs to another repository fails `verify_forge`
with an error naming the disagreeing pin:

```text
PROJECT is signed from repository ID ACTUAL, but the repository the lockfile pins is ID EXPECTED: the name belongs to a different repository now
```

The name was recreated after a rename or a deletion. Report the error so
the user can find where the project went, and keep the stored pin; never
re-pin the name automatically.
[Repository identity, renames, and transfers](/docs/trust/#repository-identity-renames-and-transfers)
explains why the repository ID decides.

With no release at hand, such as when regenerating a lockfile,
`packslip::forge::same_workflow` compares two remembered signers, each with
its recorded pin.

**Warning:** `same_workflow` always compares workflow paths. For a signer
accepted with `pin_workflow: false`, compare only the forge and the
repository ID, as `ForgePin::continues` does.

## Pin a signer with its fingerprint

To hold a release to a signer fingerprint the user configured, parse it as
`packslip::Fingerprint`. Check it right after `verify_forge`, which has
confirmed that the certificate's repository is the one the statement names:

```rust
let want: packslip::Fingerprint = "ps1_snirenkjwr7m5ozgcufameodnm".parse()?;
want.verify(accepted.verified.issuer.as_deref(), accepted.check.source.as_ref())?;
```

A malformed signer fingerprint fails to parse instead of counting as no
pin. A release from another repository fails with
`the release is signed by ps1_…, but the pin is ps1_…`. A release whose
certificate records no repository ID fails with
`the certificate records no repository ID, so it cannot be matched to a signer fingerprint`.
[How trust works](/docs/trust/) explains what a signer fingerprint pins.

## Verify a domain project or a key-signed release

`packslip::verify` verifies a bundle under the `Trust` you pass, then
checks any files by name. It does not bind the signer to a forge project's
repository, so use `verify_forge` for a forge project. For a key-signed
release:

```rust
let key = packslip::minisign::PublicKey::parse(&std::fs::read_to_string("release.pub")?)?;
let bundle = std::fs::read_to_string("packslip.sigstore.json")?;
let file = std::path::Path::new("mytool-1.2.3-linux-x86_64.tar.gz");
let verified = packslip::verify(&bundle, &packslip::Trust::Key(&key), options, &[file])?;
if verified.project != "mytool.example.com" || verified.version != "1.2.3" {
    return Err("this is not the release that was asked for".into());
}
```

For a keyless domain project, pass the signer's policy as
`&packslip::Trust::Identity(&policy)` instead:

```rust
let policy = packslip::Policy {
    issuer: Some(packslip::sigstore::GITHUB_ISSUER.into()),
    identity_prefix: Some("https://github.com/owner/mytool/".into()),
    ..Default::default()
};
```

An identity prefix also matches a repository recreated under the same
name, so check a published signer fingerprint too, passing
`packslip::sigstore::source_repository(&bundle)` to `Fingerprint::verify`
as in [Pin a signer with its fingerprint](#pin-a-signer-with-its-fingerprint).
[First use and signer fingerprints](/docs/trust/#first-use-and-signer-fingerprints)
explains where a domain project's first pin comes from.

## Verify a release list

`verify_release_list` verifies a domain project's release list under a
`packslip::Trust` you hold. `verify_forge_release_list` verifies a GitHub
repository's supplementary list under the policy the forge implies.
`verify_release_list` does not check the list's project, and neither checks
its expiry or sequence, so check them yourself:

```rust
// list_bundle: the downloaded list. key: as in the key-signed example.
// highest_sequence: the highest sequence (a u64) you stored.
let verified = packslip::verify_release_list(list_bundle, &packslip::Trust::Key(&key), options)?;
let list = &verified.list;
if list.predicate.project != "mytool.example.com" {
    return Err("the list is for another project".into());
}
if !list.is_current(jiff::Timestamp::now()) || list.predicate.sequence < highest_sequence {
    return Err("the release list is expired or older than one already accepted".into());
}
// Store list.predicate.sequence as the new highest.

let url = "https://dl.mytool.example.com/1.2.3/packslip.sigstore.json";
let pinned = list.digest_of(url).ok_or("the list does not pin this bundle")?;
let (sha256, _size) = packslip::digest_file(std::path::Path::new("packslip.sigstore.json"))?;
if sha256 != pinned {
    return Err("the bundle is not the one the list pins".into());
}
```

- Never select an entry of `list.predicate.releases` whose `is_yanked()`
  is true.
- Compare `list.digest_of(url)`, the SHA-256 the list pins for a bundle
  URL, with the downloaded bundle's before you verify the bundle.

Hold a GitHub repository's list to the list's own pins, as
[Release lists are pinned separately](/docs/trust/#release-lists-are-pinned-separately)
explains:

```rust
// list_pins: the (PinSource, ForgePin) pairs you stored for the list.
let expected = packslip::forge::Expected::new(project).pinned_by(&list_pins);
let accepted = packslip::verify_forge_release_list(list_bundle, &expected, options)?;
// Check accepted.verified.list's expiry and sequence as above. Once you
// accept it, store accepted.check.pin with the list's pins, not the releases'.
```

## Select an artifact and its resources

`packslip::select_artifact(artifacts, &host, variant, formats)` implements
[Selecting an artifact](/release/v1/#selecting-an-artifact). `variant` is
`None` unless the user asked for a variant such as `fips`. Without one,
only artifacts that name no variant fit. `formats` is the list your
installer can unpack, best first. It fails with
`no artifact fits this host`, or with
`artifacts "A" and "B" both fit this host and nothing tells them apart`.

It takes a `packslip::Host` spelled in the specification's
[vocabulary](/release/v1/#vocabularies), where Rust's `std::env::consts`
differs. Pass `darwin` for `macos`, `i686` for `x86`, and `armv6` or
`armv7` for `arm`, which Rust reports for both. Pass `powerpc64le` for
`powerpc64` on a little-endian host (`cfg!(target_endian = "little")`);
big-endian has no token. Set `libc` to the C library a Linux host uses.
With `libc: None`, only artifacts that name no `libc` fit.

`packslip::select_resources(&statement, artifact)` then gives, for each
thing the resource entries describe (one shell's completion, a man page),
the most specific entries that fit the chosen artifact. You choose each
entry's source, as [Resources](/release/v1/#resources) says.

## Refresh the trust root

`sigstore::trusted_root(None)` returns the built-in root that
`packslip verify` uses; `trusted_root(Some(json))` takes a
`trusted_root.json` file's contents instead. To refresh the root over TUF
in a long-running installer, as `packslip install` does, add the
`trust-root` feature, the crate's sigstore-trust-root version, and Tokio:

```sh
cargo add packslip --no-default-features --features trust-root
cargo add sigstore-trust-root@0.14
cargo add tokio --features rt
```

```rust
use std::error::Error;
use std::path::Path;

async fn fresh_trusted_root(
    state: &Path,
) -> Result<sigstore_trust_root::TrustedRoot, Box<dyn Error>> {
    let (repository, network_failed) = packslip::trust_root::HttpRepository::production()?;
    let root = packslip::trust_root::refresh(
        repository,
        network_failed,
        sigstore_trust_root::SigstoreInstance::PublicGood.tuf_root(),
        &state.join("tuf.json"),
        false,
        jiff::Timestamp::now(),
    )
    .await?;
    Ok(root)
}

/// Run the refresh from synchronous code.
fn trusted_root_now(state: &Path) -> Result<sigstore_trust_root::TrustedRoot, Box<dyn Error>> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(fresh_trusted_root(state))
}
```

- `refresh` authenticates the trust root from Sigstore's TUF repository,
  starting from the TUF root sigstore-trust-root embeds. It caches the
  authenticated metadata at the path you give once the whole refresh
  succeeds.
- `true` as the fifth argument replays an unexpired cache with no network.
  A connection failure, timeout, or interrupted download also falls back to
  it. A cached trust root is never trusted by itself.
- Hold a shared lock on the cache's directory during the call.
- Run it on a Tokio runtime with IO and time enabled, as
  `trusted_root_now` does.

[Trust-root lifetime and freshness](/docs/compatibility/#trust-root-lifetime-and-freshness)
describes how long a trust root lasts.

## Use the installer's modules

These features expose the modules `packslip install` is built from, with
APIs shaped for it. `install-cli` enables all of them, plus `install::run`,
the whole command.

| Feature | Module | What it provides |
| --- | --- | --- |
| `discovery` | `discovery` | An HTTPS client, GitHub release listing with repository IDs, list freshness (`list_freshness`), and version choice from a verified list (`choose`). Implies `trust-root`. |
| `install-policy` | `install_policy` | Pin constraints, release and list trust records, and the no-downgrade check behind `--accept-trust-change` (`continuity`). Implies `discovery`. |
| `install-extract` | `install_extract` | Extract a whole artifact into a private staging tree; no entry chooses an absolute path or a permission mode. Implies `archive`. |
| `install-fs` | `install_fs` | Locked, journaled replacement of an installation and its command exports, recoverable after an interruption. Implies `install-policy` and `install-extract`. |
| `install-launcher` | `install_launcher` | Command exports, with a native launcher on Windows. Implies `install-fs`. |
| `install-host` | `install_host` | Detect the host in packslip's vocabulary and check an artifact's `requires`, on the hosts `packslip install` supports. Implies `discovery`. |

## Publish a release

The `create` feature builds a statement from built artifacts, and
`sigstore::sign` signs it:

```sh
cargo add packslip --no-default-features --features create
```

```rust
use std::error::Error;
use std::path::Path;

use packslip::create::{ArtifactInput, Request};
use packslip::sigstore::Signer;
use packslip::Bin;

fn publish() -> Result<(), Box<dyn Error>> {
    let key = packslip::minisign::SecretKey::parse(&std::fs::read_to_string("release.key")?)?;
    let signer = Signer::Key { key, log: true };
    // Or keyless, with the CI job's OIDC identity:
    // let signer = Signer::Oidc(packslip::sigstore::ambient_identity()?);

    let archive = Path::new("dist/mytool-1.2.3-linux-x86_64.tar.gz");
    let created = packslip::create::create(&Request {
        url_base: Some("https://dl.mytool.example.com/1.2.3"),
        artifacts: vec![ArtifactInput {
            bin: vec![Bin::new("mytool")],
            ..ArtifactInput::new(archive)
        }],
        ..Request::new("mytool.example.com", "1.2.3", signer.identity())
    })?;
    let bundle = packslip::sigstore::sign(signer, &created.document)?;
    std::fs::write("packslip.sigstore.json", bundle)?;
    Ok(())
}
```

- `Request::new(project, version, identity)` starts a vendor-attested
  request; `signer.identity()` gives the matching `identity` block.
- `ArtifactInput::new(path)` infers the platform and format from the file
  name, as
  [How `packslip create` reads file names](/docs/release-toml/#how-packslip-create-reads-file-names)
  describes. Set `os`, `arch`, `libc`, or `format` to override it.
- `url_base` prefixes each artifact's name to form its download URL.
- `Signer::Key { key, log: true }` signs with a `packslip keygen` key and
  records the signature in Rekor.
- `create::create_release_list` builds a release list from released
  bundles; sign its `document` the same way.

`sigstore::sign` and `ambient_identity` block on their own async runtime,
so call them from synchronous code, not from inside an async task.
