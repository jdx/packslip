---
title: Manage release lists
weight: 50
description: Publish a signed release index, withdraw versions, and recommend a default release.
---
# Manage release lists

A release manifest records what shipped. A signed release list records
which releases are available, which were withdrawn, and optionally which
version the vendor recommends. Updating a list does not replace the
individual release manifests.

## Choose a discovery location

| Project name | Where consumers find the signed list |
| --- | --- |
| `mytool.example.com` | `https://mytool.example.com/.well-known/packslip.json` |
| `example.com/tools/mytool` | `https://example.com/.well-known/packslip/tools/mytool.json` |
| `github.com/owner/repo` | Optional `.well-known/packslip.json` on the default branch. |
| `github.com/owner/repo/tools/mytool` | Optional `.well-known/packslip/tools/mytool.json` on the default branch. |

A project on its own domain must publish a signed list. GitHub projects
can use GitHub releases and version-bearing tags without one. A
supplementary signed list adds withdrawals, explicit version/tag
mappings, and a signed recommendation. Other forges need the discovery
mechanism described in the [specification](/release/v1/#discovery);
a recognized signing issuer alone does not provide a release index.

### GitHub lists supplement release discovery

A GitHub list overrides the releases it names; it does not replace the
repository's release index. An omitted version can still be discovered from
its tag. To withdraw a release, keep it in the list with a yanked status.

This differs from a project on its own domain, where the signed list supplies
the release index. In both cases, consumers require a previously accepted
signed list to remain available, valid, and current.

## Create the list

Keep local copies of the released bundles. Each `--release` pairs the
public bundle URL with its local path:

```sh
packslip releases \
  --project mytool.example.com \
  --sequence 1 --valid-for 30d \
  --latest 1.2.3 \
  --release https://mytool.example.com/releases/1.2.3/packslip.sigstore.json=releases/1.2.3/packslip.sigstore.json \
  --key release.key \
  --out site/.well-known/packslip.json

packslip verify site/.well-known/packslip.json --pubkey release.pub
```

The verification command checks the list's signature and structure;
expiry and sequence continuity remain consumer checks.

The creation command copies version, tag, and publication metadata from the local
bundles and records their digests. Use trusted, previously verified input
bundles: creating a list is not a substitute for verifying its entries.
Upload the output to the well-known location; `releases` writes a local
file and does not publish it.

The key should be the one consumers pin for the project. In supported CI,
omit `--key` to sign with the job's OIDC identity.

## Refresh or withdraw releases

Publish a higher `sequence` every time you update the list, including
expiry refreshes. Rebuild it with all entries you want to retain; the
command does not append to an existing list. Refresh before expiry even
when no new release has shipped.

Keep withdrawn releases in the list and add:

```sh
--yank https://mytool.example.com/releases/1.2.3/packslip.sigstore.json='Incorrect Linux archive'
```

The URL must also appear in a `--release` argument. Consumers exclude
that version and can warn users who already have it. `--security URL`
marks a listed release as a security fix.

Consumers reject expired lists and sequences below the highest they have
accepted. Once a supplementary GitHub list has been accepted, removing
it must not silently restore withdrawn releases. Signed lists need
ongoing maintenance, not just one upload.

## Recommend a default version

`--latest 1.2.3` recommends an exact version already in the list. It can
point to an older supported release while a newer major version exists.
It does not affect exact version requests, ranges, prefixes, or channels.

For an unconstrained latest request, consumers prefer the vendor's signed
recommendation. Without one, GitHub's latest release can provide an
unsigned hint. If the recommendation is ineligible, consumers fall back
to the highest eligible semver and report why the recommendation was
skipped. An invalid signed list is an error, not a reason to fall back.
See the full [latest selection rules](/release/v1/#latest).

Versions use semver. Prereleases come from the version string
(`1.3.0-rc.1`), not an editable GitHub flag. The original tag belongs in
`source.tag`; `version` holds the normalized semver spelling.

## Use a third-party list

A reviewer or scanning service can publish a signed list of releases it
has checked. Consumers pin that publisher separately and may require its
approval before admitting a version. This is called stamping.

A stamp does not replace the vendor's signature: consumers check the
list's digest of the release bundle and still verify the release against
the vendor's pin. A mirror or repackager that signs its own release
manifest makes a different claim. See
[lists from other publishers](/release/v1/#lists-from-other-publishers)
and [repackager attestation](/release/v1/#repackager-attestation).

## Diagnose a release that is not offered

| Symptom | What to check |
| --- | --- |
| A domain project has no releases | Publish the signed list at the well-known path, including the project subpath. |
| A GitHub tag is invisible | Check that it maps to a version for this project, or add an explicit version/tag mapping in the signed list. |
| A release is found but refused | Verify its bundle and confirm the signed project, version, and bundle digest match discovery metadata. |
| A withdrawn version reappears | Keep its yanked entry; omission from a supplementary GitHub list does not withdraw it. |
| The list expires without a new release | Re-sign the retained entries with a later expiry and increased sequence. |
| `latest` selects another version | Check whether the recommendation is eligible under withdrawal, prerelease, age, stamping, and host policy. |

These are consumer discovery checks. `packslip verify` alone does not fetch
an index, enforce its expiry, or remember its previously accepted sequence.
