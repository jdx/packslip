# packslip: a signed release manifest

Version 1, draft. Predicate types `https://packslip.dev/release/v1` and
`https://packslip.dev/releases/v1`.

## Goal

A vendor publishes one signed, machine-readable document per release that
says what the artifacts are and how to verify them. Any consumer (mise,
omapac and the Omarchy Package Repository, aqua, Homebrew, a corporate
mirror) verifies it against a single pinned identity or key and gets
checksums, platform mapping, executables, and provenance links, without
per-vendor logic and without a registry entry. The name is neutral on
purpose: a packing slip is the paper in the box listing exactly what
shipped.

packslip deliberately invents as little as possible. The document is an
in-toto statement in a sigstore bundle, the same shape GitHub artifact
attestations, npm provenance, and Homebrew bottles use. Identity comes
from sigstore's certificate authority and transparency log. What packslip
adds is the predicate: the release-level manifest that a registry entry
would otherwise hold.

## Names

A project is named the way Go names a module: a host, optionally followed
by a path. `github.com/jdx/mise`, `gitlab.com/group/tool`, `mise.jdx.dev`.
No scheme, lowercase host with at least one dot, no empty or dot segments,
no trailing slash.

The name is the location and, on a forge, the identity:

- `github.com/<owner>/<repo>`: releases and their packslips are GitHub
  release assets, and the packslip is expected to be signed by a workflow
  of that repository through GitHub's OIDC issuer
  (`https://token.actions.githubusercontent.com`).
- `gitlab.com/<path>`: likewise, signed by a pipeline of that project
  through `https://gitlab.com`.
- Any other host: the vendor controls the domain and publishes a release
  list at the well-known URL below, signed with the key or identity the
  consumer pins.

A consumer needs nothing else to start verifying a project on a known
forge. A short-name alias table (`mise` for `github.com/jdx/mise`) is a
convenience a consumer may add; it is not part of the format.

## The file

A release ships one file, `packslip.sigstore.json`: a
[sigstore bundle](https://github.com/sigstore/protobuf-specs) (v0.3)
whose content is a DSSE envelope of type `application/vnd.in-toto+json`
carrying the statement below, and whose verification material is either
the signer's Fulcio certificate or a public-key hint, plus the Rekor
transparency log entry for the signature.

Because the bundle carries the statement, there is no separate plain JSON
file and no canonical-bytes rule: whatever bytes are in the payload are
what was signed. `packslip show` prints them; so does
`jq -r .dsseEnvelope.payload | base64 -d`. `cosign` and `gh attestation`
understand the bundle as-is.

## The release statement

```json
{
  "_type": "https://in-toto.io/Statement/v1",
  "subject": [
    { "name": "mise-v2026.9.1-linux-x64.tar.xz", "digest": { "sha256": "..." } }
  ],
  "predicateType": "https://packslip.dev/release/v1",
  "predicate": {
    "project": "github.com/jdx/mise",
    "version": "2026.9.1",
    "published_at": "2026-09-01T12:00:00Z",
    "source": { "repo": "https://github.com/jdx/mise", "commit": "...", "tag": "v2026.9.1" },
    "artifacts": [
      {
        "name": "mise-v2026.9.1-linux-x64.tar.xz",
        "os": "linux", "arch": "x86_64", "libc": "gnu",
        "size": 12345678,
        "url": "https://github.com/jdx/mise/releases/download/v2026.9.1/mise-v2026.9.1-linux-x64.tar.xz",
        "format": "tar.xz",
        "bin": ["mise/bin/mise"],
        "provenance": ["https://api.github.com/repos/jdx/mise/attestations/sha256:..."]
      }
    ],
    "identity": {
      "scheme": "sigstore-oidc",
      "key_id": "https://github.com/jdx/mise/.github/workflows/release.yml@refs/tags/v2026.9.1",
      "issuer": "https://token.actions.githubusercontent.com"
    },
    "sbom": "https://.../sbom.cdx.json",
    "supersedes": "2026.9.0"
  }
}
```

Rules:

- `subject` lists every artifact by file name with its sha256; `artifacts`
  carries the same names with platform, size, download URL, format,
  executables, and provenance links. The two sets of names must match
  exactly, and neither may contain a duplicate. At least one artifact is
  required. Digests are 64 lowercase hex characters.
- `project` is a name as defined above. `version` is the vendor's version
  string, compared as opaque text. `published_at` is RFC 3339 UTC.
- `os`, `arch`, and `libc` use the values `linux`, `darwin`, `windows`,
  `freebsd`; `x86_64`, `aarch64`, `armv7`, `riscv64`, `i686`; `gnu`,
  `musl`. `format` is the archive or installer type.
- `bin` lists the executables inside the artifact as paths relative to the
  archive root, or the artifact's own name when it is a bare executable. A
  consumer puts them on PATH. Windows entries carry their `.exe`.
- `provenance` holds URLs of SLSA build provenance statements for that
  artifact. The packslip proves the manifest; verified provenance proves
  the build, at whatever SLSA build level its builder establishes.
- `supersedes` names the release this one replaces, so a consumer can
  detect a rollback without a version-ordering scheme.
- `identity` says how the document is signed and by whom, so a consumer
  can check what it pinned against what it received. For `sigstore-oidc`,
  `key_id` is the certificate's subject identity (a workflow URI for CI, an
  email for a person) and `issuer` the OIDC issuer. For `sigstore-key`,
  `key_id` is the key id in uppercase hex.

The JSON schema is printed by `packslip schema` and published at
`https://packslip.dev/schema/release-v1.json`.

## Signing

Both schemes produce the same file and are verified by the same code. A
vendor should prefer the first.

- `sigstore-oidc`: keyless. A CI job with an id-token permission signs
  with its own identity; Fulcio issues a short-lived certificate naming
  the workflow that ran, and Rekor logs the signature. There is no key to
  manage. `packslip create` does this by default when it finds an ambient
  CI credential (GitHub Actions, GitLab CI, and the others sigstore's
  clients know) or a token in `SIGSTORE_ID_TOKEN`.
- `sigstore-key`: a long-lived Ed25519 key from `packslip keygen`, kept
  in minisign's key-file format. The bundle carries a public-key hint and
  the Rekor entry, whose verifier is the public key. For vendors who
  release outside a CI system with OIDC, or who want a stable key their
  consumers pin. Consumers pin the public key, never the hint.

A key-signed bundle may be produced without a log entry
(`create --no-log`) for an air-gapped release. Consumers refuse such a
bundle unless they explicitly allow it (`verify --allow-unlogged`), and a
repository should record that choice per vendor.

Detached minisign signatures over a plain JSON file, which an earlier
draft used, are not a scheme. A consumer that wants a dependency-free
check still has one: the DSSE signature of a key-signed bundle is a raw
Ed25519 signature over the pre-authentication encoding of the payload.

## Discovery

Publish `packslip.sigstore.json` next to the artifacts: as a release
asset, or under the version directory of a download site.

For a project on a known forge, the forge's release listing is the
discovery mechanism and nothing more is needed. A project on its own
domain advertises recent releases at

```
https://<host>/.well-known/packslip/<path>.json
```

where `<path>` is the project name after the host, or `packslip.json`
directly under `.well-known` when the name is a bare host. The list is a
bundle of the same shape as a packslip, with the `releases/v1` predicate:

```json
{
  "_type": "https://in-toto.io/Statement/v1",
  "subject": [
    { "name": "https://dl.example.com/2026.9.1/packslip.sigstore.json",
      "digest": { "sha256": "..." } }
  ],
  "predicateType": "https://packslip.dev/releases/v1",
  "predicate": {
    "project": "mise.jdx.dev",
    "generated_at": "2026-09-01T12:00:00Z",
    "expires_at": "2026-10-01T12:00:00Z",
    "sequence": 42,
    "identity": { "scheme": "sigstore-key", "key_id": "5A0A0B8B9C6D7E1F" },
    "releases": [
      { "version": "2026.9.1", "published_at": "2026-09-01T12:00:00Z",
        "packslip": "https://dl.example.com/2026.9.1/packslip.sigstore.json" }
    ]
  }
}
```

Each subject is a listed packslip's URL with the digest of that file, so
the list pins the exact documents it points at. `expires_at` and
`sequence` are borrowed from TUF's timestamp role: a consumer refuses a
list that has expired, or whose sequence is lower than one it has already
accepted, so a mirror cannot freeze or roll back the vendor's view.
`packslip releases` produces the list from local copies of the released
bundles and the JSON schema is at
`https://packslip.dev/schema/releases-v1.json`.

The list separates the name from where the bytes live, the way a Go vanity
import does: the identity is anchored to the domain, and the artifacts can
be anywhere.

## Consumer rules

1. Pin the identity once. For a forge project, the name is the pin: accept
   only the forge's issuer and an identity under the repository. For other
   projects, pin the public key or identity from a list of pins you
   maintain, or from the well-known list on first use. Never take a key
   from the document itself, and never trust a bundle's key hint.
2. Verify the bundle: signature, certificate chain and log entry as
   sigstore defines them, then the statement's structure, then the
   subject digest and size of every artifact you downloaded.
3. Enforce no-downgrade: refuse a release whose `identity.scheme` is
   weaker than the last accepted one, whose signer changed without a human
   saying so, or that dropped per-artifact provenance the last release
   carried. For a keyless signer, compare the workflow path, not the ref:
   a new tag of the same workflow is the same signer.
4. Apply any minimum release age to the log's integration time, falling
   back to `published_at` only for an unlogged bundle you chose to accept.
5. Treat `supersedes` as the ordering hint for rollback detection.
6. For a release list, refuse one that has expired or whose `sequence` is
   below the last one accepted.

## What a verified packslip proves

A verified packslip proves that the named signer published exactly this
list of artifacts, with these digests, at a time the log recorded. It does
not by itself prove anything about how the artifacts were built. That is
what SLSA provenance is for: an artifact whose linked provenance a
consumer verifies earns the SLSA build level its builder establishes
(GitHub-hosted runners with `actions/attest-build-provenance` give Build
L3). Consumers record what they verified as a SLSA Verification Summary
or in their own terms; packslip defines no level scale of its own.

`packslip verify` reports the scheme, the signer, the log time, and
whether every artifact links provenance. It does not fetch or verify the
provenance statements.

## Tooling

The reference implementation is the `packslip` crate and binary in
[jdx/packslip](https://github.com/jdx/packslip), also usable as a GitHub
Action.

- In a release job: `uses: jdx/packslip@v1` with `artifacts: dist/*`
  attests build provenance for the artifacts, signs the packslip
  keylessly, links the provenance from it, verifies the result, and
  uploads `packslip.sigstore.json` to the release.
- `packslip create --project github.com/o/r --version X --out dist
  --url-base URL --source-repo URL --tag vX --bin NAME artifact...`
  digests the artifacts, infers platforms from file names
  (`path:os/arch[/libc]` overrides), and writes the signed bundle. Add
  `--key release.key` to sign with a key; `--no-log` skips Rekor.
- `packslip keygen -o release.key` writes an Ed25519 secret seed (mode
  0600) and `release.pub`.
- `packslip verify packslip.sigstore.json [--artifact file...]` verifies
  and exits 1 on any failure; `--json` prints the result. A keyless bundle
  is checked against the policy its project name implies, or against
  `--identity`, `--identity-prefix`, and `--issuer`; a key-signed bundle
  against `--pubkey`. `--allow-unlogged` accepts a bundle with no log
  entry; `--trusted-root` replaces the embedded sigstore root. The same
  command verifies a release list.
- `packslip show packslip.sigstore.json` prints the statement.
- `packslip releases --project NAME --sequence N --valid-for 30d
  --release URL=PATH... --key release.key` writes a signed release list.
- `packslip schema [--releases]` prints the JSON schemas.
