---
title: Manage release lists
weight: 40
group: publish
description: Publish a signed release list, withdraw versions, and recommend a default version.
---
# Manage release lists

Publish a signed release list to tell consumers which versions are
available, withdraw a broken release, mark a security fix, or recommend a
default version. Each entry points to a release bundle and records its
digest. Updating the list leaves the individual release manifests intact.

A project on its own domain must publish a list; a GitHub project may add
one. This page covers the list itself and `packslip releases`. To publish
from GitHub Actions to a host you run, follow
[Host releases on your own domain](/docs/self-hosting/).

Publishing a list also means maintaining it: schedule a refresh before
expiry and retain withdrawals on every rebuild. Consumers that have
accepted a list reject a missing or expired replacement. See
[Refresh or withdraw releases](#refresh-or-withdraw-releases) for the
update procedure.

## Where consumers look for the list {#choose-a-discovery-location}

For a project named after its own domain, the signed list is the only
place consumers find releases. For a GitHub project, it supplements
discovery from release tags that name a version. Add a list when you need
withdrawals, security markers, a recommended version, or a release whose
tag does not name a version.

The project name sets the list's location:

| Project name | Where consumers find the signed list |
| --- | --- |
| `mytool.example.com` | `https://mytool.example.com/.well-known/packslip.json` |
| `example.com/tools/mytool` | `https://example.com/.well-known/packslip/tools/mytool.json` |
| `github.com/owner/repo` | Optional `.well-known/packslip.json` on the default branch. |
| `github.com/owner/repo/tools/mytool` | Optional `.well-known/packslip/tools/mytool.json` on the default branch. |

Other forges need the discovery mechanism the specification describes in
[Discovery](/release/v1/#discovery); a recognized signing issuer alone
does not give consumers a release list.

### A supplementary list adds to GitHub discovery {#github-lists-supplement-release-discovery}

A supplementary list overrides the releases it names; it does not replace
discovery through the repository's GitHub releases. A version it omits is
still discovered from its tag. To withdraw a release, list it with a
yanked status.

## Create the list

Keep local copies of the released bundles and verify each one before
listing it. `packslip releases` reads their version, tag, and publication
time and records their digests; it does not verify their signatures.
Each `--release` pairs a public bundle URL with its local path. This
example lists version `1.2.3` of a key-signed project and recommends it as
the default:

```sh
packslip verify releases/v1.2.3/packslip.sigstore.json --pubkey release.pub

packslip releases \
  --project mytool.example.com \
  --sequence "$(date +%s)" --valid-for 30d \
  --latest 1.2.3 \
  --release https://mytool.example.com/v1.2.3/packslip.sigstore.json=releases/v1.2.3/packslip.sigstore.json \
  --key release.key \
  --out site/.well-known/packslip.json

packslip verify site/.well-known/packslip.json --pubkey release.pub
```

`packslip releases` writes a local file
(`packslip-releases.sigstore.json` unless `--out` names it); upload it to
the well-known location yourself. Sign with the key consumers pin for the
project, and publish the public key where they can read it apart from the
list, since a domain name implies no signer. In supported CI, omit
`--key` to sign with the job's OIDC identity. On GitHub Actions, the
`jdx/packslip/releases` action builds and signs the list from a directory
of the published bundles; see
[Host releases on your own domain](/docs/self-hosting/#build-and-publish-the-list).

Verify the resulting list before uploading it, as the last command above
does. `packslip verify` checks its signature and structure. Consumers also
check expiry and remember the highest sequence they have accepted.

The example's sequence is the current Unix time, the action's default. A
Unix-time sequence increases on its own; a project that adopts it cannot
go back to small hand-picked numbers.

### Publish a GitHub repository's list

On GitHub, sign the list keylessly in a workflow of the repository, as
its releases are signed. Run the commands below in a job of that
workflow, with `GH_TOKEN` set to the job's token for `gh`. Without
`--key`, `packslip releases` signs with the job's identity, so the
commands work only there. A job that also commits the list needs these
permissions:

```yaml
permissions:
  contents: write  # Commit the list to the default branch.
  id-token: write  # Sign the list with the workflow's identity.
```

Commit the list to the default branch at `.well-known/packslip.json`, or
at `.well-known/packslip/<subpath>.json` for a monorepo tool. Consumers
read it from
`https://raw.githubusercontent.com/owner/repo/HEAD/.well-known/packslip.json`.

The list needs only the releases it changes. To withdraw `v1.2.3`,
download that release's bundle, verify it (the project name implies the
pin), and list it at its release-asset URL with `--yank`:

```sh
gh release download v1.2.3 --repo owner/repo \
  --pattern packslip.sigstore.json --dir bundles/v1.2.3
packslip verify bundles/v1.2.3/packslip.sigstore.json

packslip releases \
  --project github.com/owner/repo \
  --sequence "$(date +%s)" --valid-for 30d \
  --release https://github.com/owner/repo/releases/download/v1.2.3/packslip.sigstore.json=bundles/v1.2.3/packslip.sigstore.json \
  --yank https://github.com/owner/repo/releases/download/v1.2.3/packslip.sigstore.json='Incorrect Linux archive' \
  --out .well-known/packslip.json
```

Once consumers have accepted the list, they treat a missing or expired
one as an error. The repository then needs a scheduled job that rebuilds,
re-signs, and commits the list before it expires. The list workflow in
[Host releases on your own domain](/docs/self-hosting/#build-and-publish-the-list)
shows the triggers and concurrency group to copy. It builds from a bucket
and uploads the result, so run the commands above in place of its steps.

## Refresh or withdraw releases

Publish a higher `sequence` every time you update the list, including
expiry refreshes. Rebuild it with all entries you want to retain;
`packslip releases` does not append to an existing list. Refresh before
expiry even when no new release has shipped.

Keep withdrawn releases in the list, and add `--yank` with the bundle's
URL and a reason:

```sh
packslip releases ... \
  --release https://mytool.example.com/v1.2.3/packslip.sigstore.json=releases/v1.2.3/packslip.sigstore.json \
  --yank https://mytool.example.com/v1.2.3/packslip.sigstore.json='Incorrect Linux archive'
```

The URL must also appear in a `--release` argument. Consumers never
select that version, and they warn users who already have it.
`--security URL` marks a listed release as a security fix, and consumers
may shorten their minimum release age for it.

Because each rebuild starts from the arguments it is given, repeat every
`--yank` and `--security` on every rebuild, including the weekly refresh. A
withdrawal passed to one run only is gone from the next list, and the release
is eligible again. Keep the withdrawals in a file under version control and
build the arguments from it each time.
[Host releases on your own domain](/docs/self-hosting/#keep-withdrawals-in-the-repository)
shows this for a GitHub Actions release-list job.

Consumers reject expired lists and sequences below the highest they have
accepted. On GitHub, deleting a supplementary list does not return
consumers to tag discovery: a consumer that accepted the list treats its
absence as an error. A published list needs refreshing for as long as
consumers use it; one upload is not enough.

## Recommend a default version

`--latest 1.2.3` recommends an exact version already in the list. It can
point to an older supported release while a newer major version exists.
It does not affect exact version requests, ranges, prefixes, or channels.

For an unconstrained latest request, consumers prefer the vendor's signed
recommendation. Without one, GitHub's latest release can provide an
unsigned hint. If the recommendation is ineligible, consumers fall back
to the highest eligible semver and report why the recommendation was
skipped. An invalid signed list is an error, not a reason to fall back.
A latest request skips prereleases, meaning versions with a prerelease
part such as `1.3.0-rc.1`; GitHub's prerelease flag is not consulted. See
the full [latest selection rules](/release/v1/#latest).

## Publish or trust a third-party list {#use-a-third-party-list}

A registry, mirror, or scanning service can sign a list of releases it has
checked. The list places a *stamp* on each release it names; the service
is a *stamping host*.

The service publishes one list per vendor project on its own host, using
the well-known path with the vendor's full project name. Set `--project`
to the vendor's project, since every listed bundle must name it. Use
`--evidence` to record what the service checked:

```sh
packslip releases \
  --project github.com/owner/repo \
  --sequence "$(date +%s)" --valid-for 30d \
  --release https://github.com/owner/repo/releases/download/v1.2.3/packslip.sigstore.json=bundles/v1.2.3/packslip.sigstore.json \
  --evidence https://github.com/owner/repo/releases/download/v1.2.3/packslip.sigstore.json=scan=https://scanner.example/reports/1.2.3 \
  --key scanner.key \
  --out site/.well-known/packslip/github.com/owner/repo.json
```

A service at `scanner.example` serves it at
`https://scanner.example/.well-known/packslip/github.com/owner/repo.json`.
Consumers trust stamping hosts by configuration and pin each one
separately. A consumer that trusts one or more stamping hosts installs
only versions that at least one of them lists without a yank. A vendor
withdrawal excludes a version regardless of its stamps. A user can also
exempt a project and trust its vendor alone.

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
| A GitHub tag is invisible | Check that it maps to a version for this project, or add an explicit version and tag mapping in the signed list. |
| A release is found but refused | Verify its bundle and confirm the signed project, version, and bundle digest match discovery metadata. |
| A withdrawn version reappears | Keep its yanked entry in every rebuild, from a committed file rather than a one-off input; omission from a supplementary list does not withdraw it. |
| Consumers report an expired list | No list was published before the old one expired. Re-sign the retained entries with a later expiry and a higher sequence. |
| Consumers refuse a new list as rolled back | Its `sequence` is below one they accepted; publish a higher one. After a list that used the Unix-time default, any small hand-picked number is lower. |
| `latest` selects another version | Check whether the recommendation is eligible under withdrawal, prerelease, age, stamping, and host policy. |

These are consumer discovery checks. `packslip verify` alone does not fetch
a list, enforce its expiry, or remember its previously accepted sequence.
