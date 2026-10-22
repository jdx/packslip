---
title: How trust works
description: What a verified release proves, which signer a project name implies, and what each packslip consumer remembers and enforces.
group: start
weight: 20
---
# How trust works

Read this before you choose how to sign your releases, or what to pin when
you install someone else's. You will know what a verified release proves
and what each packslip consumer enforces. For commands, go to
[Publish with GitHub Actions](/docs/publishing/),
[Verify a release](/docs/verifying/), or [Bootstrap a tool](/docs/bootstrap/).

## What a verified release proves

A release bundle carries a signed statement: the project, the version, and
each artifact's digest and size. Trust comes from the signature, the signer
policy, and the signed digests, not from where a file was downloaded: a
download host, mirror, or CDN only locates files. The one exception is the
first `packslip install` of a keyless domain project; see
[First use and signer fingerprints](#first-use-and-signer-fingerprints).

| A verified release establishes… | It does not establish… |
| --- | --- |
| A signer your policy allows made this statement about the release. | The signer or its build environment was uncompromised. |
| Each file you checked has the bytes the signer described: its signed digest and, for an artifact, its signed size. | The software is safe, or how it was built. |
| A logged signature has a verified log time. | This is the newest release, or the vendor has not withdrawn it. |

A checksum file served next to the binary cannot establish the second row:
whoever can replace the binary can replace the checksum too. A signer can
sign other projects and older versions, so consumers also check the signed
project and version.

`packslip verify` does not fetch or check linked build provenance, so
verify that evidence separately. Detecting a withdrawal or preventing
rollback takes discovery metadata and remembered state. See
[Release-list freshness](#release-list-freshness) and
[What a verified packslip proves](/release/v1/#what-a-verified-packslip-proves).

## Two ways to sign

- **Keyless (`sigstore-oidc`):** a release job on a CI system with an OIDC
  identity, such as GitHub Actions or GitLab CI, signs as that identity.
  Fulcio's short-lived certificate names the workflow that ran, so there is
  no key to manage. Consumers pin the identity and issuer, or a signer
  fingerprint.
- **Key (`sigstore-key`):** a vendor whose CI has no OIDC identity, or that
  wants a stable key for consumers to pin, signs with an Ed25519 key from
  `packslip keygen`. Consumers pin its `.pub` public key, obtained apart
  from the release, never from the bundle's unverified key hint.

Every packslip verifier refuses a statement whose `identity` block does
not name the actual signer and scheme; see [Signing](/release/v1/#signing).
`packslip install` accepts a key only for a domain project, so a GitHub
project must sign keylessly to be installable with it.

## Transparency log and unlogged bundles

Signatures are recorded in the Rekor transparency log by default. Keyless
signatures always are. A key-signed release made without network access
can skip the log: pass `--no-log` with `--key` to `packslip create`.
A logged signature gives the consumer a verified integration time, from
which a minimum release age is measured.

Consumers refuse an unlogged bundle unless you allow unlogged bundles for
that vendor: `packslip verify --allow-unlogged`,
`packslip install --allow-unlogged` for one attempt, or mise's
`allow_unlogged` tool option. The npm package and
the committable verifier refuse every unlogged bundle. See
[Unlogged bundles](/release/v1/#unlogged-bundles).

Checking a log entry and a certificate needs Sigstore's trust root.
`packslip install` refreshes it over TUF by default; `packslip verify` and
`packslip pin` use the root embedded in the binary. `--trusted-root`
replaces either with your own `trusted_root.json`.
[Compatibility and support](/docs/compatibility/) says how long an
embedded root stays usable.

## What a project name says about the signer

On a forge, a project name also says who may sign its releases:

| Project | Kind | Implied signer |
| --- | --- | --- |
| `github.com/owner/mytool` | Forge project | A GitHub Actions run of `owner/mytool`, through `https://token.actions.githubusercontent.com` |
| `gitlab.com/group/mytool` | Forge project, verify-only | A GitLab CI pipeline of `group/mytool`, through `https://gitlab.com` |
| `mytool.example.com` | Domain project | None |

An installer derives a forge project's signer policy from the name the
user asked for, never from the statement. `packslip verify` has no request,
so it uses the project the statement names.

Fulcio takes the certificate's source repository from the forge's token:
the repository whose run signed, even through another repository's
reusable workflow. Consumers require it to be the project's repository. A
certificate that records no source repository must come from a workflow of
the project's repository.

A monorepo subpath, such as `github.com/owner/repo/mytool`, names one tool,
but its signer is checked against the repository. GitHub and GitLab
repository names compare case-insensitively, as the forges resolve them:
`github.com/JDX/mise` is `github.com/jdx/mise`. Subpaths and workflow
paths compare exactly.

A domain name implies no signer, so `packslip verify` needs `--pubkey` or
identity flags for a domain project, and they are the whole policy. The
domain anchors discovery instead; see
[How packslip works](/docs/release-workflow/),
[Project names](/release/v1/#project-names), and
[Monorepos](/release/v1/#monorepos).

## First use and signer fingerprints

Without a pin, a consumer that remembers trust accepts a first release on
these grounds, as [Consumer rule 1](/release/v1/#consumer-rules) sets out:

| Project | First use trusts |
| --- | --- |
| Forge project | A certificate whose source repository is the name's repository, remembered by the repository ID it records. `packslip install` also requires the ID GitHub's API gives for the name. |
| Keyless domain project | For the list, the signer the list names, when the list came over HTTPS from the project's host. For releases, the signer of a bundle whose digest that list pins. |
| Key-signed domain project | Nothing. The public key comes only from the consumer's own pins, such as `--pubkey`. |

`packslip install` follows all three rows. For a forge project without
signer tool options, mise follows the first without asking the forge; it
takes a domain project's signer only from tool options or a registry entry.

The first two rows are trust on first use. An installer remembers the
repository ID of a forge project's first release and later refuses any
other repository under the name. A name recreated before the first
install is trusted, though, and the release cannot reveal it.
`packslip install` also remembers the repository that signed a keyless
domain project's first list and release, and asks for approval before it
accepts another; see
[Repository identity, renames, and transfers](#repository-identity-renames-and-transfers).

`packslip verify` remembers nothing between runs. Without `--pin`, it
accepts a repository recreated under the name, whether its policy comes
from the name or from an identity prefix such as
`https://github.com/owner/mytool/`. Both name a repository by its current
name, which someone else can take after a rename or deletion.

A signer fingerprint closes the gap. It is `ps1_` followed by 26
characters, derived from the forge's OIDC issuer and the repository ID
only. A key-signed release, or a certificate without a repository ID, has
none. It survives a rename, a transfer to another owner, and a change of
signing workflow, and covers every tool in a monorepo. A repository that
later takes over the old name gets a different one.

Vendors publish it where users can read it without trusting a release,
such as a README; see
[Publish your signer fingerprint](/docs/publishing/#publish-your-signer-fingerprint).
Users record it apart from the release, in a Dockerfile, CI workflow, or
lockfile, and check the first install against it with `--pin`, as for
`github.com/jdx/hk`:

```sh
packslip install github.com/jdx/hk --pin ps1_snirenkjwr7m5ozgcufameodnm
```

The fingerprint carries no authority by itself: you trust its publisher
and the place it is published. `packslip pin` prints the fingerprint of
whichever repository signed the bundle you give it, so run it only on a
release you already trust; see
[Pin a signer with its fingerprint](/docs/verifying/#pin-a-signer-with-its-fingerprint).
[Signer fingerprints](/release/v1/#signer-fingerprints) gives the
derivation.

## Repository identity, renames, and transfers

A repository keeps its ID through renames and transfers, and no other
repository reuses it, so consumers pin the ID, not the name or the owner.
Fulcio records it in every GitHub Actions and GitLab CI certificate. This
prints the two IDs a GitHub Actions certificate records, the repository's
and its owner's:

```sh
gh api repos/jdx/hk --jq '.id,.owner.id'
```

```text
922514152
216188
```

The owner's ID is not part of the identity. Only a repository's current
owner can transfer it, and that owner already controls what it signs.

Under the specification, a consumer that remembers the first release's
repository ID accepts releases from both sides of a rename or transfer and
reports the name each was signed under. It refuses a name deleted and
recreated by anyone, even with the same workflow path. Without a
repository ID in the certificate or the remembered pin, it checks by name
alone, as for releases signed before Fulcio recorded IDs.

For a GitHub project, `packslip install` keys its installation by
repository ID and monorepo subpath, so a rename or transfer keeps its
install and trust history. Unlike the
specification, it prints no notice for a release signed under a new name.
When a later certificate lacks the ID it remembered, it asks for approval
with the reason `repository pin lost` instead of checking by name.

For a keyless domain project, install remembers the signer's workflow URL
and, from the certificates of the first list and release it accepts, the
signing repository's ID. Later lists and releases must come from that
repository and that workflow file, at any ref. Each of these is a changed
signer that needs approval, separately for the list and for releases:

- A repository recreated under the old name.
- Another repository's run that calls the signing workflow as a reusable
  workflow.
- A rename or transfer of the signing repository, which keeps its ID but
  changes the workflow URL.
- A certificate without the remembered ID (`repository pin lost`).

When no ID was remembered, because the certificate recorded none, install
compares the workflow URL alone.

For a forge project, mise also follows a rename by repository ID. The
`identity`, `identity_prefix`, and `workflow` tool options name the
repository and have no repository-ID check: a rename needs new ones, and a
repository recreated under the old name matches them. See
[Repository identity and renames](/release/v1/#repository-identity-and-renames).

## Signer continuity and workflow pinning

A consumer that keeps state remembers which workflow file signed the
releases it accepted. It compares later releases by path, without the ref,
so a new tag from `.github/workflows/release.yml` is the same signer.
Another workflow file is a changed signer, refused until a person approves
it. A reusable workflow of another repository is compared as that
repository's workflow, so moving between it and the project's own workflow
needs one approval, even at the same path. After a rename or transfer, the
path inside the repository is compared once the repository ID matches.
[Keep later releases acceptable](/docs/publishing/#keep-later-releases-acceptable)
says how a publisher avoids these approvals.

A vendor that must sign from several workflow files can declare
`pin_workflow: false` with `packslip create --no-pin-workflow` or
`packslip releases --no-pin-workflow`. A release that declares it asks
consumers to hold later releases to the repository instead of one workflow
file; a list's declaration does the same for later lists. See
[Workflow pinning](/release/v1/#workflow-pinning). Consumers differ:

- **In `packslip install`**, the first release that declares `false` after
  one that did not needs approval. After that, any workflow of a GitHub
  project's repository continues the signer. For a domain project, install
  asks the same once, then keeps comparing the workflow file.
- **In mise**, the declaration is ignored. Approve a release from another
  workflow file with `mise packslip forget`, or, for a GitHub project, name
  every signing file in the
  [`workflow`](https://mise.jdx.dev/dev-tools/backends/packslip.html#workflow)
  tool option.

## No-downgrade and approvals

Compared with the last release it accepted for a project, a consumer that
keeps state refuses these changes until a person approves them:

- A weaker signing scheme, such as keyless signing replaced by a key.
- A changed signer: another workflow file, repository ID, or key.
- A repackager's bundle after the vendor's own.
- Dropped provenance. In `packslip install`, a build both releases offer
  lost provenance links, or the new release offers none of the builds that
  carried them. In mise, not every artifact links provenance, after a
  release in which all did.
- Workflow pinning disabled: `pin_workflow` went from `true` to `false`.
  Only `packslip install` checks this.

[Consumer rule 3](/release/v1/#consumer-rules) defines each case. To
approve one, follow
[Approve a trust change](/docs/bootstrap/#approve-a-trust-change) for
`packslip install`, or
[Preserve trust across upgrades and machines](/docs/mise/#preserve-trust-across-upgrades-and-machines)
for mise.

## Release lists are pinned separately

A project's release list and its releases have independent continuity
records, so a scheduled workflow can re-sign the list from another
workflow file. On a forge, both must still be signed from the
project's repository. `packslip install` checks each against its own
record. In mise, the list is checked against the release policy or the
`list_identity_prefix` tool option, and only its sequence is recorded, with
the repository ID when the list was checked under the forge project's
policy, so that it follows a rename.

`packslip install` also remembers a key-signed domain project's public
key. When the vendor changes keys, each user does this:

1. Pass the new key with `--pubkey`, or put it in place of the old one in
   the project's `pins.d` file. Until then, install verifies against the
   remembered key, and the new list fails.
2. Leave no `pins.d` file naming the old key, or install stops with
   `caller and administrator public keys disagree`.
3. Approve the two trust changes, one for the list and one for the
   release; see [Approve a trust change](/docs/bootstrap/#approve-a-trust-change).

## Release-list freshness

A signed release list pins each bundle's digest, so the consumer checks
the exact bundle the list selected. It also provides expiry, sequence
numbers, withdrawals, and an optional recommended version. The installers
enforce them:

- They refuse an expired list, and one whose `sequence` is lower than the
  highest they accepted.
- `packslip install` saves the sequence before it fetches any bundle or
  artifact, so a failed attempt cannot make an older list acceptable.
- Once they have accepted a project's list, a missing list is an error,
  so deleting it cannot undo a withdrawal.

The rules are in
[Freshness and rollback protection](/release/v1/#freshness-and-rollback-protection);
to publish a list, see [Manage release lists](/docs/release-lists/).

## Stamping hosts

A stamping host is a registry, mirror, or scanning service that signs a
release list of the versions it has checked; each entry is a stamp.
Consumers trust stamping hosts through configuration and pin each one
separately. Only mise applies stamping-host policy.

- A consumer that trusts stamping hosts installs only versions at least
  one of them lists without a yank.
- One host's withdrawal does not veto another host's stamp. A vendor
  withdrawal excludes a version whatever its stamps.
- A user can exempt a project and trust its vendor alone.

A stamp limits which releases are accepted, but the vendor's signature
still authenticates them: consumers check the stamp's digest of the bundle
and verify the release against the vendor's pin. A mirror or repackager
that signs its own bundle instead vouches for the files under its own
identity, and consumers rank it below the vendor's bundle. See
[Lists from other publishers](/release/v1/#lists-from-other-publishers)
and [Repackager attestation](/release/v1/#repackager-attestation).

## What each consumer enforces

Each consumer applies part of the [consumer rules](/release/v1/#consumer-rules).
An application built on a library keeps the remembered state itself.

| Consumer | Signer policy | Remembered trust | Release lists and stamps |
| --- | --- | --- | --- |
| [`packslip verify`](/docs/verifying/) | Derived from a GitHub or GitLab name and narrowed by identity flags or a `--pin` fingerprint. `--allow-other-repository` accepts a named signer outside the repository. `--pubkey` is the whole policy for any project, with no repository check; so are identity flags for a domain project. | None | Checks a list's signature, not its expiry or sequence. No stamps. |
| [`packslip install`](/docs/bootstrap/) | Derived from a GitHub name, or from a keyless domain project's list on [first use](#first-use-and-signer-fingerprints). `--pin`, identity flags, and [pin files](/docs/bootstrap/#require-signers-with-pin-files) add requirements. `--pubkey` only for a domain project; `gitlab.com` is refused. Prints the fingerprint after a GitHub install. | Separately for releases and the list: signer, scheme, `pin_workflow`, and the repository ID of a GitHub project or of the repository that signed a keyless domain project. Also attestation, provenance, the list's sequence, and a domain project's key. Not `--pin` or identity flags; pass them on every run, or use a pin file, which applies to every run. | Enforces expiry, sequence, withdrawals, and a missing list. Warns when the list withdraws the release it last installed. No stamps. |
| [mise](/docs/mise/) | Derived from a GitHub name, or replaced by the `pubkey`, `identity`, `identity_prefix`, `issuer`, and `workflow` tool options. A domain project needs options or a registry entry. Reads no fingerprint; `mise.lock` carries the pin. | Signer, scheme, attestation, provenance, a forge project's repository ID, and the list's sequence. | Enforces lists. Requires a stamp from a host in `packslip.stampers` when that setting lists any. |
| [npm package](/docs/verify-js/) | Yours: `identity` and `issuer`. `project` binds a forge project to its repository, unless `allowOtherRepository` is set. Keyless only; no fingerprint. | None | Refuses release lists. |
| [Committable verifier](/docs/verify-script/) | Yours: `--issuer` with `--identity`, `--identity-prefix`, or `--source-repository`. `--project` binds a forge project to its repository, unless `--allow-other-repository` is passed. Keyless only; no fingerprint. | None | Refuses release lists. |
| [Rust crate](/docs/rust/) | `verify` checks exactly the `Trust` you pass. `verify_forge` derives a forge project's policy and binds the signer to its repository. `Fingerprint::verify` checks a fingerprint. | None. `forge::Check` gives the pin to store and compares signers. | `verify_release_list` checks the signature; you enforce expiry and sequence. |
