---
title: Who is using packslip
description: Tools whose releases carry a signed release bundle, published upstream, by a repackager, or as a third-party build.
group: reference
weight: 86
---
# Who is using packslip

If you are evaluating packslip, these tools show the format in real use. Each
has a published release that carries a signed release bundle, and each row
links to that release and its bundle. To publish your own bundle, start with
[Publish with GitHub Actions](/docs/publishing/).

The list covers {{< adopters count >}} tools:
{{< adopters count upstream >}} published upstream,
{{< adopters count repackager >}} signed by a repackager, and
{{< adopters count build >}} released as third-party builds. The last search for
published bundles was on {{< adopters surveyed >}}, so the list is a snapshot,
not an exhaustive census or a count of installed users. Proposed integrations
are left out, and so are merged changes that have not shipped a signed bundle.

## Upstream-published tools

These projects publish the bundle in the software's own upstream release
repository.

{{< adopters table upstream >}}

## Repackagers and third-party builds

These publishers sign releases of software that another project develops. A
release from one of them establishes publication by the named publisher. It
does not establish adoption or endorsement by the upstream project.

- **Repackager:** signs a bundle that describes the vendor's own files, and
  records in the statement what it checked before signing. `packslip verify`
  reports such a release as `repackager-attested`. See
  [Repackager attestation](/release/v1/#repackager-attestation) in the
  specification.
- **Third-party build:** the publisher builds the upstream source itself and
  signs the build as an ordinary release of its own project.

{{< adopters table third_party >}}

## How tools are counted

The count in the introduction follows these rules:

- GNU Emacs' master, 31, and 30 release channels count as one tool; the table
  lists the master channel.
- Separately installable tools count separately: Jenesis and jpx, and the FSL
  compiler and its language server, are two tools each.
- Aliases, bundled helpers, and plugins do not count.

## How the list was checked

On {{< adopters checked >}}, each linked bundle was compared with the SHA-256
digest GitHub records for that release asset. Each was then verified with
packslip {{< adopters checked_with >}} against the GitHub Actions issuer, with
the publisher's repository as the certificate's source repository.

The check establishes two things:

- Each bundle was the file GitHub served for that release when it was checked.
- A GitHub Actions run of the publisher's repository signed it.

It does not establish anything about the listed binary artifacts or the build
attestations the bundles link to. A listing is evidence of publication, not an
endorsement of the software.

To check a row yourself, download its bundle and follow
[Verify a release](/docs/verifying/). The statement in packslip's own row
names the domain project `packslip.dev`, whose name implies no signer. Verify
that bundle with the flags in
[Verify a download](/docs/install/#verify-a-download). The statement in every
other row names a forge project in its publisher's repository, so
`packslip verify` checks it with no trust flags.
