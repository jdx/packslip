---
title: Glossary
weight: 84
group: reference
description: Definitions of the terms the packslip guides and specification use, each linked to the page that explains it.
---
# Glossary

Look up a term from the packslip guides or the specification; each links to
the page that explains it.
[How packslip works](/docs/release-workflow/) shows how they fit together.

## Projects and parties

- **[Consumer](/docs/installers/)**: software that verifies or installs
  releases from their packslips, such as a package manager or mirror.
- **[Domain project](/docs/self-hosting/)**: a project named after its
  vendor's host, such as `mytool.example.com`.
- **[Forge](/docs/trust/#what-a-project-name-says-about-the-signer)**: a code
  host, `github.com` or `gitlab.com`, whose project names also say who signs.
  A **forge project**, such as `github.com/owner/mytool`, is named after a
  forge repository, whose CI is expected to sign its releases.
- **[Mirror](/docs/installers/#repackage-another-vendors-release)**: a
  repackager that keeps the vendor's digests but lists its own download URLs.
- **[Monorepo subpath](/docs/publishing/#release-several-tools-from-one-repository)**:
  a path naming one tool of a GitHub repository, as in
  `github.com/owner/repo/mytool`.
- **[Project](/release/v1/#project-names)**: the name a consumer asks for: a
  host and optional path, without a URL scheme, such as
  `github.com/owner/mytool` or `mytool.example.com`.
- **[Repackager](/release/v1/#repackager-attestation)**: a third party, such
  as a registry or mirror, that signs a statement for another vendor's files.
- **[Stamping host](/docs/trust/#stamping-hosts)**: a third party, such as a
  registry, reviewer, or scanning service, that signs a release list of
  versions it has checked, each entry a **stamp**.
- **[Vendor](/release/v1/#reading-this-specification)**: the project whose
  releases a packslip describes, which normally builds and signs them, unlike
  a repackager.

## Release files

- **[Artifact](/docs/describing-releases/)**: a release file the statement
  lists for consumers to select and download, such as an archive, installer,
  bare executable, or source tarball.
- **[Bundle](/release/v1/#bundle-format)**: a Sigstore bundle, a signed
  statement plus its verification material. Release bundles and release
  lists are both bundles.
- **[Executable path](/docs/describing-releases/#name-the-executables)**: an
  executable's path inside an artifact, listed in its `bin` so consumers can
  put the command on PATH.
- **[Extensions](/docs/describing-releases/#add-custom-metadata)**: metadata
  under `extensions`, keyed by whoever defines it; packslip assigns it no
  meaning.
- **[Host requirements](/docs/host-requirements/)**: an artifact's needs
  from the host, such as a minimum OS or glibc version, listed in `requires`.
- **[packslip](/docs/release-workflow/)**: the format and its tools, and the
  everyday name for a release bundle.
- **[Predicate](/release/v1/#purpose-and-scope)**: the statement's packslip
  data, whose type, `https://packslip.dev/release/v1` or
  `https://packslip.dev/releases/v1`, marks a release or a release list.
- **[Provenance](/docs/describing-releases/#set-download-urls-and-provenance)**:
  SLSA build provenance an artifact links by URL in its `provenance` list,
  for consumers to verify separately.
- **[Release bundle](/docs/release-workflow/#three-files-with-different-jobs)**:
  a release's bundle, published as `packslip.sigstore.json`, or
  `packslip.<subpath>.sigstore.json` for a monorepo tool, with each `/`
  written as `-`. Also called a **release manifest**.
- **[Release list](/docs/release-lists/)**: a bundle that indexes a
  project's releases and records later changes, such as withdrawals.
  Consumers pin it separately from the releases. Also called a **signed
  list**.
- **[release.toml](/docs/release-toml/)**: the optional input to
  `packslip create --manifest`, neither signed nor published.
  Older CLI help calls it a **TOML manifest**.
- **[Resource](/docs/resources/)**: an item shipped for a purpose other than
  running the software, such as a completion script, man page, skill, or
  SBOM.
- **[Resource asset](/docs/resources/#choose-a-source)**: a resource
  published as its own release file, named in the resource's `asset` field
  and listed as a subject.
- **[Statement](/release/v1/#the-release-statement)**: the in-toto JSON
  inside a bundle: subjects with file digests, and a predicate with the
  release's or list's metadata.
- **[Subject](/release/v1/#subjects-and-digests)**: a file name and its
  digests in a release statement, one per artifact and resource asset. Any
  other file, such as a checksum file, is left out.
- **[Supplementary list](/docs/release-lists/#decide-whether-you-need-a-list)**:
  a GitHub repository's optional release list, kept on its default branch,
  that withdraws, marks, or recommends releases on top of the ones GitHub
  lists.
- **[Variant](/docs/describing-releases/#separate-builds-that-share-a-platform)**:
  a token, such as `fips`, that tells apart builds for the same platform.

## Signing

- **[Certificate identity](/docs/verifying/#narrow-the-policy-with-identity-flags)**:
  the signer a keyless certificate names, such as a workflow URL with its
  ref, which `--identity` matches in full and `--identity-prefix` by prefix.
- **[Fulcio](/docs/trust/#repository-identity-renames-and-transfers)**:
  Sigstore's certificate authority. The certificates it issues to GitHub
  Actions and GitLab CI jobs record the signing repository and its ID.
- **[Issuer](/docs/trust/#what-a-project-name-says-about-the-signer)**: the
  OIDC provider that vouched for a keyless signer, such as
  `https://token.actions.githubusercontent.com` for GitHub Actions.
- **[Key signing](/docs/trust/#two-ways-to-sign)**: signing with a long-lived
  Ed25519 key from `packslip keygen`, whose public key consumers pin.
- **[Keyless signing](/docs/trust/#two-ways-to-sign)**: signing with a CI
  job's OIDC identity through Sigstore. Fulcio's short-lived certificate
  names the workflow, so there is no long-lived key.
- **[Reusable workflow](/docs/trust/#what-a-project-name-says-about-the-signer)**:
  another repository's workflow, called by a release job, which can sign
  the caller's releases.
- **[Signer](/docs/trust/#two-ways-to-sign)**: the identity that signed a
  bundle: a certificate identity and its issuer, or a public key.
- **[Sigstore](/release/v1/#signing)**: the open signing project whose
  bundle format, Fulcio, and Rekor packslip uses.
- **[Source repository](/docs/trust/#what-a-project-name-says-about-the-signer)**:
  the repository whose workflow run signed a bundle, as its keyless
  certificate records. Consumers compare it with a forge project's
  repository.
- **[Transparency log](/docs/trust/#transparency-log-and-unlogged-bundles)**:
  Rekor, Sigstore's public log of signatures, which gives each a verified
  integration time.
- **[Trust root](/docs/trust/#transparency-log-and-unlogged-bundles)**: the
  Sigstore certificates and keys a verifier trusts.
- **[Unlogged bundle](/docs/trust/#transparency-log-and-unlogged-bundles)**:
  a key-signed bundle with no transparency-log entry, created with
  `--no-log`, which consumers accept only when told to, as with
  `--allow-unlogged`.

## Trust

- **[Evidence](/release/v1/#repackager-attestation)**: what a repackager or
  stamping host records having checked.
- **[First use](/docs/trust/#first-use-and-signer-fingerprints)**: a
  consumer's first install of a project, before it holds a pin.
- **[Lockfile](/docs/mise/#preserve-trust-across-upgrades-and-machines)**: a
  committed file, such as `mise.lock`, that records each project's pin so
  every machine enforces the same signer.
- **[No-downgrade](/docs/trust/#no-downgrade-and-approvals)**: the rule that
  refuses a **trust change**, meaning a changed signer or protection weaker
  than the last accepted release's, until a person approves it.
  `packslip install` takes approval as `--accept-trust-change=<id>`.
- **[Pin](/docs/trust/#first-use-and-signer-fingerprints)**: to hold a
  project's releases to one signer; as a noun, that policy: an issuer with
  an identity or prefix, a public key, or the repository a signer
  fingerprint names.
- **[Pin file](/docs/bootstrap/#require-signers-with-pin-files)**: a TOML
  file in `pins.d` naming projects and the signer `packslip install` must
  require for each.
- **[Repository ID](/docs/trust/#repository-identity-renames-and-transfers)**:
  a forge's number for a repository, which survives renames and transfers
  and is never reused. Consumers remember it for a forge project's accepted
  releases. `packslip install` also remembers the ID of the repository that
  signs a keyless domain project.
- **[Signer continuity](/docs/trust/#signer-continuity-and-workflow-pinning)**:
  the check that a release comes from the last accepted signer: for keyless
  signing, the same repository and, by default, the same workflow file.
- **[Signer fingerprint](/docs/trust/#first-use-and-signer-fingerprints)**:
  `ps1_` plus 26 characters derived from the forge's issuer and the
  repository ID, naming the signing repository whatever it is called.
  `packslip pin` prints it; `--pin` checks it.
- **[Workflow pinning](/docs/trust/#signer-continuity-and-workflow-pinning)**:
  the default that a keyless project's later releases come from the same
  workflow file. A signed `pin_workflow: false` asks consumers to check only
  the repository.

## Discovery and installation

- **[Bootstrap](/docs/bootstrap/)**: installing a tool with
  `packslip install` instead of piping a script to `sh`.
- **[Channel](/release/v1/#ordering-prereleases-and-channels)**: a version's
  first prerelease identifier when it is not numeric, such as `beta` in
  `1.2.0-beta.2`.
- **[Discovery](/release/v1/#discovery)**: how a consumer finds releases:
  through GitHub for a GitHub project, or through a domain project's
  release list.
- **[Expiry](/docs/trust/#release-list-freshness)**: a release list's
  `expires_at` time, after which consumers refuse it.
- **[Latest](/docs/release-lists/#recommend-a-default-version)**: what a
  request naming no version gets: the vendor's signed `latest` or, for a
  GitHub project without one, GitHub's latest release. With no
  recommendation, or when the recommended release is missing or ineligible,
  it gets the highest eligible version.
- **[Minimum release age](/docs/mise/#install-a-release-that-was-just-published)**:
  a consumer's wait after a release is logged before offering it, such as
  mise's `minimum_release_age`.
- **[mise](/docs/mise/)**: a tool version manager whose `packslip:` backend
  installs tools from their packslips.
- **[Prerelease](/release/v1/#ordering-prereleases-and-channels)**: a version
  with a prerelease part, such as `1.3.0-rc.1`, which consumers skip unless
  asked.
- **[Security release](/docs/release-lists/#refresh-or-withdraw-releases)**:
  a release marked `security: true` in a release list because it fixes a
  vulnerability.
- **[Sequence](/docs/release-lists/#choose-a-sequence)**: a release list's
  counter, higher in each new list so consumers can refuse an older one.
- **[Source tag](/release/v1/#tags)**: the vendor's tag for a release, such
  as `mytool-v1.2.3`, kept as spelled in `source.tag`.
- **[Version](/release/v1/#versions)**: a release's signed semver 2.0.0
  version, used to order and select releases.
- **[Well-known URL](/docs/release-lists/#decide-whether-you-need-a-list)**:
  where a domain project's release list lives:
  `https://<host>/.well-known/packslip.json`, or
  `https://<host>/.well-known/packslip/<path>.json` for a name with a path.
- **[Yanked](/docs/release-lists/#refresh-or-withdraw-releases)**: withdrawn
  in a release list, so consumers never select the release.
