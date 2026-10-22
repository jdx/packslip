---
title: Build an installer or mirror
description: Discover, verify, select, and install packslip releases in your own installer or mirror, and keep the trust state the consumer rules require.
group: build
weight: 60
---
# Build an installer or mirror

This guide is for developers whose installer, package manager, or mirror
consumes signed packslip releases. You will find, verify, and select a
release, keep trust across installs, and sign a mirror's bundle.

For a ready-made installer, use `packslip install`
([Bootstrap a tool](/docs/bootstrap/)); to check files by hand, use
`packslip verify` ([Verify a release](/docs/verifying/)); for the Rust API
behind these steps, see [Use the Rust crate](/docs/rust/).

## Install a release

1. **Discover the release.** List the project's versions, choose one, and
   locate its release bundle, as [Discovery](/release/v1/#discovery) says:

   | Project | Versions | Bundle |
   | --- | --- | --- |
   | Forge project on GitHub | Its non-draft GitHub releases. Take each version from its tag ([Tags](/release/v1/#tags)), and its order and prerelease status from that version, never from GitHub's order or prerelease flag. | The `packslip*.sigstore.json` asset whose statement has the `predicateType` `https://packslip.dev/release/v1` and the requested `project`, or the same tool under the renamed repository. Compare owner and repository in any letter case. |
   | Domain project | Its signed release list at the [well-known URL](/release/v1/#publishing-a-list), once verified. Refuse a project with none. | The URL in the chosen entry's `packslip` field. |
   | Forge project on gitlab.com | The specification defines none. | A trusted publisher's list or your own configuration; otherwise refuse the project. |

   If a GitHub repository carries a
   [supplementary release list](/release/v1/#supplementary-release-lists),
   verify it too. For each version it lists, the entry decides the tag, the
   status, and the bundle digest to accept; never choose a version it yanks.

   **Warning:** Never trust a bundle's file name for its type or project;
   read both from the statement.
2. **Verify the bundle alone** against your pin before you use anything
   else it says.
   - When a release list entry names the bundle, refuse it before verifying
     unless its SHA-256 equals the digest the entry gives for that URL.
   - After verifying, check that the project is the one requested, or the
     same repository renamed. Check that the version is the one the list
     entry named or, without an entry, the tag named.
3. **Select an artifact** from the verified result, never the unverified
   bundle. `packslip verify --json` lists every artifact with its signed
   digest; see
   [Read the verified artifacts from a script](/docs/verifying/#read-the-verified-artifacts-from-a-script).
   Choose one as [Selecting an artifact](/release/v1/#selecting-an-artifact)
   says. Your installer decides which formats and resource kinds it
   supports.
4. **Check host requirements.** Check the chosen artifact's `requires` as
   [Checking requirements before installation](/release/v1/#checking-requirements-before-installation)
   says. Refuse only when a library, glibc, or OS version keeps the
   executables from starting. Warn about a missing or old command and any
   requirement you cannot check.
   [Declare host requirements](/docs/host-requirements/) shows what
   publishers declare.
5. **Download, verify, and install.** Download the artifact and verify it
   against the same bundle and pin before you unpack it. With the CLI, use
   `packslip verify --artifact`; see
   [Check every file you use](/docs/verifying/#check-every-file-you-use).
   Verify each resource asset you download the same way.

Implementations in other languages can test their artifact and resource
selection, tag parsing, statement validation, forge identity, and signer
fingerprints against the
[conformance vectors](https://github.com/jdx/packslip/tree/main/tests/conformance).

## Use private hosts and mirrors

Once a signer is pinned, where a file came from does not change trust
([How trust works](/docs/trust/)). The exception is a keyless domain
project's first use: its signer comes only from the list fetched over
HTTPS from the project's host, never from a copy (rule 1 in
[Follow the consumer rules](#follow-the-consumer-rules)).
`packslip verify` makes no network requests, so it works the same from
an internal download host or across an air gap. When you call it or the
crate, you fetch the bundle and artifacts and handle credentials,
proxies, and caching.

Serve a vendor's release from your own host in two ways:

- **Serve a copy** of the vendor's unmodified bundle and files. Consumers
  verify it with the vendor's pin, using `packslip verify --artifact` or
  their own installer; the copy adds no signer.
  - Keep the published file names. `--artifact` matches files by name, so
    a renamed file fails with
    `verification failed: artifact NAME: not listed in the document`.
  - Copy the signed release list and the bundles together. Consumers
    refuse an expired or older list
    ([consumer rule 5](/release/v1/#consumer-rules)). A stale copy serves
    old releases that are still validly signed.
- **Run a mirror** by signing your own repackager bundle that lists your
  URLs, as [Repackage another vendor's release](#repackage-another-vendors-release)
  shows.

`packslip install` can use neither. It finds a bundle only through the
project's own discovery and downloads the files that bundle names. It has
no setting to point an artifact URL at another host. When a host the
bundle names needs a token, see
[Download from a private host](/docs/bootstrap/#download-from-a-private-host).

## Repackage another vendor's release

A repackager signs its own release bundle for another vendor's files: for a
vendor that publishes no packslip, or to mirror files a vendor's packslip
covers. The bundle names the vendor's project and carries the vendor's
digests, but lists your URLs and is signed with your identity, as
[Repackager attestation](/release/v1/#repackager-attestation) describes.

To mirror a vendor release whose verified bundle you saved as
`vendor/packslip.sigstore.json`:

```sh
vendor_sha256=$(sha256sum vendor/packslip.sigstore.json | cut -d ' ' -f 1)
packslip create \
  --project mytool.example.com \
  --version 1.2.3 \
  --attested-by repackager \
  --evidence "vendor-packslip=$vendor_sha256" \
  --url-base https://mirror.example.net/mytool/1.2.3/ \
  --bin mytool \
  --key mirror.key \
  --out mirror \
  dist/mytool-1.2.3-linux-x86_64.tar.gz
```

- `--attested-by repackager` says that you, not the vendor, make the
  claim; the default is `vendor`.
- `--evidence KIND[=DETAIL]` records one check you made; pass it once per
  check. `vendor-packslip` takes the SHA-256 of the vendor's bundle file.
  [Repackager attestation](/release/v1/#repackager-attestation) lists the
  documented kinds, such as `vendor-signature` and `pkgbuild-checksums`.
- `--url-base` points the artifact URLs at your mirror.
- `--key mirror.key` signs with your key; in CI with an OIDC identity,
  omit it to sign keyless.
- `--out mirror` writes your bundle to `mirror/packslip.sigstore.json`.

`--evidence` without `--attested-by repackager` fails with `--evidence
describes what a repackager checked; pass --attested-by repackager`.
Neither flag has a release.toml equivalent; pass both even with
`--manifest`.

A verified repackager bundle establishes that the repackager signed these
digests and what it says it checked. It does not establish anything on the
vendor's behalf: consumers must trust the repackager's signer explicitly
and rank its bundle below a vendor packslip. A consumer whose last
accepted release was vendor-attested asks a person before accepting yours
([No-downgrade and approvals](/docs/trust/#no-downgrade-and-approvals)).

With the CLI, a consumer verifies your key-signed bundle with
`--pubkey mirror.pub` alone. For a keyless one, see
[Verify a repackager's release](/docs/verifying/#verify-a-repackagers-release).

If you also publish a release list of your bundles, record each check you
made on a listed release with `packslip releases --evidence URL=KIND[=DETAIL]`,
once per check.
[Publish a list as a stamping host](/docs/release-lists/#publish-a-list-as-a-stamping-host)
covers lists from publishers other than the vendor.

## Follow the consumer rules

A successful `packslip verify` alone does not meet the
[consumer rules](/release/v1/#consumer-rules), the complete contract for an
install. `packslip verify` and the crate's `verify_forge`,
`select_artifact`, and `select_resources` cover verification and selection.
The crate's [installer modules](/docs/rust/#use-the-installers-modules)
also check list freshness, choose versions, enforce no-downgrade, and check
host requirements, with APIs shaped for `packslip install`. Beyond the
[Install a release](#install-a-release) steps, your code handles each rule:

1. **Pin the identity once.** For a forge project, take the first pin
   from the name: accept only the forge's issuer and a certificate whose
   source repository is that repository (`packslip verify` with no trust
   flags and `verify_forge` do this). Then remember the repository ID it
   records; the ID holds across renames and transfers.
   Take other projects' pins from your configuration or, on a keyless
   domain project's first use, from the list fetched over HTTPS from its
   host.
   Add any signer fingerprint you read independently to the first pin.
   Take a public key only from your own pins, never from the bundle or its
   key hint. Trust another publisher's list per host, through
   configuration. See
   [First use and signer fingerprints](/docs/trust/#first-use-and-signer-fingerprints).
2. **Verify the bundle and downloaded files.** Pass `--allow-unlogged`, or
   in Rust `require_log: false`, only when the user explicitly accepted
   unlogged releases.
3. **Enforce no-downgrade.** Ask a person before accepting a signer
   change, relaxed workflow pinning, or another weakening the rule lists,
   compared with the last accepted release across renames; see
   [No-downgrade and approvals](/docs/trust/#no-downgrade-and-approvals).
4. **Apply the minimum release age.** Measure it from the authenticated
   log integration time, which the verifier reports as `logged_at`. Use
   `published_at` only for an unlogged bundle the user deliberately
   accepted.
5. **Use the project's discovery metadata.** Refuse an expired list, a list
   whose `sequence` is below the last you accepted, and a project whose
   accepted list has gone missing. Never select a yanked entry. Skip
   prereleases unless asked, rank by semver precedence, and follow
   [Latest](/release/v1/#latest) for an unconstrained request. Apply any
   stamping-host policy you configured.
6. **Select one artifact.** Step 3. Refuse a tie the preferences leave
   unresolved; host requirements never break one. In Rust,
   `select_artifact` does this (pass the formats you can unpack).
7. **Check host requirements before installing.** Step 4; in Rust,
   `install_host` checks `requires`.
8. **Install resources.** Apply [Resources](/release/v1/#resources): keep
   each resource's most specific entries that fit the artifact, and take
   each from its strongest available source. Run vendor code only as
   [Running an exec entry](/release/v1/#running-an-exec-entry) permits.
   Ignore unknown kinds. Report a resource you cannot fetch and finish
   without it; fail on a digest mismatch.

## Keep trust state between installs

Keep, for each project, the remembered state the
[consumer rules](/release/v1/#consumer-rules) list:

- **Pins:** the signer, its repository ID, and `pin_workflow`, kept for the
  releases and separately for the release list
  ([Release lists are pinned separately](/docs/trust/#release-lists-are-pinned-separately)),
  plus any signer fingerprint or lockfile commitment. Rule 1 holds later
  releases and lists to them; rule 3 compares signers.
- **Accepted trust properties:** the last accepted release's scheme,
  `attested_by`, and provenance. Rule 3 asks a person before any of them
  weakens.
- **List history:** the highest accepted `sequence`, and whether a signed
  list was ever accepted. Rule 5 refuses a lower sequence or a missing list.
- **User choices:** approved trust changes, permission to accept unlogged
  bundles, and trusting a vendor without a stamp. Rules 2 to 5 honor them.

A lockfile can carry a project's pin with artifact URLs and digests, so
another machine enforces it on its first install. Local state can
separately remember signer history and list sequences. The specification
leaves where to store either to you. One consumer, mise, keeps both; see
[Preserve trust across upgrades and machines](/docs/mise/#preserve-trust-across-upgrades-and-machines).

For the vendor's side (publishing a list, withdrawing versions, and
recommending a default), see [Manage release lists](/docs/release-lists/).
