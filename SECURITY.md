# Security policy

Report vulnerabilities privately; other bugs go in
[issues](https://github.com/jdx/packslip/issues).

## Report a vulnerability

Open a private advisory at
<https://github.com/jdx/packslip/security/advisories/new>. Include what you
can:

- The component and version: `packslip version` output, package version,
  action ref, packslip.sh URL, `packslip-verify.mjs` commit, or image
  digest.
- The command and files, including any wrongly accepted release bundle,
  release list, or artifact.
- What an attacker gains and needs first.
- Whether it is public or reported elsewhere, and how to credit you.

Keep details out of public issues, pull requests, and discussions until the
advisory is published. If the form fails, open an issue asking only for a
private contact.

## What is in scope

Everything this repository ships, and the packslip.dev and packslip.sh
services:

- The CLI and its APT and RPM packages.
- The crate, npm package, Go module, and image (`ghcr.io/jdx/packslip`).
- The `jdx/packslip` and `jdx/packslip/releases` actions, `installer/`,
  `verifier/packslip-verify.mjs`, `skills/packslip/SKILL.md`, the
  [specification](https://packslip.dev/release/v1/), `tests/conformance/`,
  and the release workflows.

Examples:

- A verifier (the CLI, crate, npm package, or committable verifier), or a
  spec rule or conformance vector, lets through a bundle, list, or file
  that should be refused, such as a disallowed signer or a mismatched
  digest.
- Without `--allow-other-repository`, a forge project's release verifies
  although its certificate's source repository is not the project's.
  Signing with another repository's reusable workflow is accepted by design.
- `packslip install` installs a project or version other than the one
  requested, selects a withdrawn release, accepts an expired or rolled-back
  release list, writes outside its directories, runs downloaded code, or
  replaces a pinned signer without approval.
- An install script installs an executable whose SHA-256 differs from the
  one written in it, or runs anything else it downloads.
- A publishing tool (`packslip create`, `packslip releases`, the Go module,
  an action) signs something that misdescribes its input, lets an input run
  a command, or exposes a token.
- The agent skill tells an agent to weaken verification without cause, such
  as with `--allow-unlogged`.
- Either service serves a file a release did not publish, or someone other
  than the maintainers gets a release signed by packslip's release workflow.

## What is out of scope

- A vulnerability in a tool that publishes with packslip, or a release
  signed by its compromised signer: report it to that project. A verified
  bundle establishes who signed and that the files match, not that the
  software is safe ([How trust works](https://packslip.dev/docs/trust/)).
- Sigstore's services and GitHub's OIDC tokens and attestations: report to
  them; packslip mishandling their material is in scope.
- The packslip support in mise and the mise registry
  ([mise's security policy](https://github.com/jdx/mise/blob/main/SECURITY.md)).
- Trust you grant by flag (`--allow-*`, `--accept-trust-change`,
  `--trusted-root`) or by naming a signer, key, or identity (`--pin`,
  `--pubkey`), when the flag does what its help says. A flag that grants
  more is in scope.
- A mistyped or lookalike name.
- [Trust on first use](https://packslip.dev/docs/trust/#first-use-and-signer-fingerprints):
  for a keyless domain project, `packslip install` trusts the signer its
  list names (fetched over HTTPS from its host) and the repository that
  signed its first list and release. For a forge project, it trusts the
  repository the forge reports for the name. Passing the signer fingerprint
  with `--pin` on every install avoids both.
- Documented limits: `packslip verify` neither fetches build provenance nor
  checks a release list's expiry or sequence, and the committable verifier
  skips the checks its header lists. A check a consumer is
  [documented to enforce](https://packslip.dev/docs/trust/#what-each-consumer-enforces)
  but skips is in scope.
- A dependency's vulnerability: report it to that project, and here too
  when packslip is exploitable through it.
- Attacks that need code execution as the user running packslip.
- Denial of service by traffic volume.
- Scanner output without demonstrated impact.

## What to expect

1. The maintainers discuss the report with you in the advisory, with no
   fixed response time, and close a report that is not a packslip
   vulnerability with an explanation.
2. A confirmed vulnerability is fixed, possibly in the advisory's private
   fork. The fix is public on `main` before its release, but its pull
   request and commits do not describe the vulnerability until the
   advisory is published.
3. The release is marked a security fix in packslip.dev's signed release
   list, so consumers can shorten their minimum release age for it.
4. The advisory is then published with the affected and fixed versions.

## Supported versions

Only the latest 1.x release is supported; earlier releases are not patched.
The CLI, crate, npm package, Go module, actions, install scripts, and image
share one version number. The committable verifier is supported as it
stands at the latest release's tag.

Reinstall the CLI the way you installed it, and move each pinned copy to
the fixed release:

- A `https://packslip.sh/vX.Y.Z` or `https://packslip.sh/vX.Y.Z/install.ps1`
  URL, and any checksum for it (such as `ADD --checksum`).
- An `@vX.Y.Z` or `@<full-commit-sha>` action ref, and its
  `packslip-version` and `packslip-sha256` inputs (or remove them).
- An `X.Y.Z` image tag or digest, and the executable `packslip-path` names.
- A committed `packslip-verify.mjs`: copy it from the fixed tag.
- A dependency: `cargo update -p packslip`, `npm install packslip@latest`,
  or `go get github.com/jdx/packslip/go@latest`.

`@v1`, the unversioned packslip.sh URLs, and the `latest` and `X` image
tags move to each release (pull the image again). An `X.Y` tag moves only
within its minor line. Under `@v1`, `packslip-version` and `packslip-path`
still pin the CLI; `packslip-sha256` set alone fails the job until updated.

Distributions choose the versions they maintain; fixes are kept suitable
for backporting
([Maintaining packaged verifiers](https://packslip.dev/docs/compatibility/#maintaining-packaged-verifiers)).
