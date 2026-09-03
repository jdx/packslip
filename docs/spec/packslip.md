# packslip: a signed release manifest

Version 1, draft. Predicate type `https://packslip.dev/release/v1`.

## Goal

A vendor publishes one signed, machine-readable document per release that
says what the artifacts are and how to verify them. Any consumer (mise,
omapac and the Omarchy Package Repository, aqua, Homebrew, a corporate
mirror) verifies it with a single pinned identity and gets checksums,
platform mapping, executables, provenance links, and an evidence level,
without per-vendor logic and without a registry entry. The name is neutral
on purpose: a packing slip is the paper in the box listing exactly what
shipped.

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
  list at the well-known URL below, which names the signing identity or key.

A consumer needs nothing else to start verifying a project on a known
forge. A short-name alias table (`mise` for `github.com/jdx/mise`) is a
convenience a consumer may add; it is not part of the format.

## Document

The document is an [in-toto Statement v1](https://github.com/in-toto/attestation)
whose predicate type is the packslip, so attestation tooling parses it
unchanged.

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
        "provenance": ["https://.../mise-v2026.9.1-linux-x64.tar.xz.sigstore.json"]
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
- `provenance` holds URLs of build provenance statements (SLSA, sigstore
  bundles) for that artifact. A consumer that verifies them may raise the
  level to L3.
- `supersedes` names the release this one replaces, so a consumer can
  detect a rollback without a version-ordering scheme.
- `identity` says how the document is signed and by whom, so a consumer can
  check what it pinned against what it received. `key_id` is the minisign
  key id in uppercase hex, or the certificate's subject identity for the
  sigstore schemes: a workflow URI for CI, an email for a person. `issuer`
  is the OIDC issuer for the sigstore schemes.

The JSON schema is printed by `packslip schema` and published at
`https://packslip.dev/schema/release-v1.json`.

## Signing

The canonical bytes are the compact JSON serialisation with keys in the
order above, exactly as `packslip create` writes `packslip.json`. The
signature is over those bytes.

Schemes, in the order a vendor should prefer them:

- `sigstore-oidc`: a [sigstore bundle](https://github.com/sigstore/protobuf-specs)
  in `packslip.sigstore.json` carrying the document as an in-toto DSSE
  envelope, signed with an ephemeral key certified by Fulcio for the
  signer's OIDC identity and logged to Rekor. There is no key to manage: a
  CI job with an id-token permission signs with its own identity, and the
  certificate names the workflow that ran. `packslip create` does this by
  default when it finds an ambient CI credential (GitHub Actions, GitLab CI,
  and the others sigstore's clients know) or a token in `SIGSTORE_ID_TOKEN`.
  `identity.key_id` is the certificate identity, `identity.issuer` the
  issuer.
- `minisign`: a detached [minisign](https://jedisct1.github.io/minisign/)
  signature in `packslip.json.minisig`, prehashed (`ED`) or legacy (`Ed`),
  with a trusted comment covered by the global signature.
  `minisign -V -p vendor.pub -m packslip.json` verifies it, as does
  `packslip verify`. For vendors who release outside a CI system with OIDC,
  or who prefer a long-lived key. `identity.key_id` is the minisign key id
  in uppercase hex.
- `sigstore-key`: a sigstore bundle signed with a long-lived key logged to
  Rekor. Reserved; verification of this scheme is not in this build.

## Discovery

Publish `packslip.json` and its signature next to the artifacts: as
release assets, or under the version directory of a download site.

For a project on a known forge, the forge's release listing is the
discovery mechanism and nothing more is needed. A project on its own
domain, and optionally any project, advertises recent releases at

```
https://<host>/.well-known/packslip/<path>.json
```

where `<path>` is the project name after the host, or `packslip.json`
directly under `.well-known` when the name is a bare host. The list is
signed with the project's identity (its `.minisig` or `.sigstore.json`
beside it) and looks like this:

```json
{
  "project": "mise.jdx.dev",
  "identity": { "scheme": "minisign", "key_id": "5A0A0B8B9C6D7E1F",
                "pubkey": "RWS..." },
  "releases": [
    { "version": "2026.9.1", "published_at": "2026-09-01T12:00:00Z",
      "packslip": "https://github.com/jdx/mise/releases/download/v2026.9.1/packslip.json" }
  ]
}
```

The list separates the name from where the bytes live, the way a Go
vanity import does: the identity is anchored to the domain, and the
artifacts can be anywhere.

## Consumer rules

1. Pin the identity once. For a forge project, the name is the pin: accept
   only the forge's issuer and an identity under the repository. For other
   projects, record the key or identity from the well-known list on first
   use, or from a list of pins you maintain. Never take a key from the
   document itself.
2. Verify the signature, then the document structure, then the subject
   digest and size of every artifact you downloaded.
3. Enforce no-downgrade: refuse a release whose `identity.scheme` is
   weaker than the last accepted one, whose signer changed without a human
   saying so, or that dropped per-artifact provenance the last release
   carried.
4. Apply any minimum release age to `published_at`, and prefer the
   transparency log's integration time over it when there is one.
5. Treat `supersedes` as the ordering hint for rollback detection.

## Evidence levels

| level | meaning |
|---|---|
| L0 | checksums only, no signature |
| L1 | signed checksums or artifact signatures |
| L2 | a signed packslip |
| L3 | L2 plus per-artifact build provenance that the consumer verified |
| L4 | L3 plus reproducible or independently verified builds |

`packslip verify` reports L2 for a verified document, or L3 when every
artifact links provenance; it does not itself fetch or verify provenance
bundles.

## Tooling

The reference implementation is the `packslip` crate and binary in
[jdx/packslip](https://github.com/jdx/packslip), also usable as a GitHub
Action.

- In a release job: `uses: jdx/packslip@v1` with `artifacts: dist/*`
  signs keylessly and uploads `packslip.json` and `packslip.sigstore.json`
  to the release.
- `packslip create --project github.com/o/r --version X --out dist
  --url-base URL --source-repo URL --tag vX --bin NAME artifact...`
  digests the artifacts, infers platforms from file names
  (`path:os/arch[/libc]` overrides), and writes the document and its
  signature. Add `--key release.key` to sign with minisign instead.
- `packslip keygen -o release.key` writes a minisign secret seed (mode
  0600) and `release.pub`.
- `packslip verify dist/packslip.json [--artifact file...]` verifies and
  exits 1 on any failure; `--json` prints the result. A sigstore document
  is checked against the policy its project name implies, or against
  `--identity`, `--identity-prefix`, and `--issuer`; a minisign document
  against `--pubkey`. `--trusted-root` replaces the embedded sigstore root.
