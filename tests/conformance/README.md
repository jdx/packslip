# Conformance vectors

The rules in the [packslip specification](https://packslip.dev/release/v1/)
in executable form. They exist for implementations other than this one: a
consumer written in any language can read these files and check that it
selects, parses, and refuses what the specification says it should.

An implementation that disagrees with a vector disagrees with the
specification. If you believe a vector is wrong, open an
[issue](https://github.com/jdx/packslip/issues) instead of working around
it: either the specification needs to change, or the vector requires
something the specification does not.

| File | Rule |
| --- | --- |
| `artifact-selection.json` | [Selecting an artifact](https://packslip.dev/release/v1/#selecting-an-artifact) |
| `resource-selection.json` | [Resources](https://packslip.dev/release/v1/#resources) |
| `tag-versions.json` | [Tags](https://packslip.dev/release/v1/#tags) |
| `statement-validity.json` | [The release statement](https://packslip.dev/release/v1/#the-release-statement) |
| `forge-identity.json` | [Forge identity](https://packslip.dev/release/v1/#forge-identity) |
| `signer-fingerprint.json` | [Signer fingerprint](https://packslip.dev/release/v1/#signer-fingerprint) |

Each file is a JSON object with a `rule` link, a `description` of what the
cases mean, and a `cases` array. Every case has a `name`; some carry a
`reason` or `comment` explaining the rule at issue. The `description`
field defines that file's case shape — read it before writing a runner.

## Scope

These cover what is packslip's own: which artifact a host installs, which
resource entries apply to it, which version a tag names, whether a
statement is structurally valid, whether a verified forge release is
the repository a consumer pinned, and the signer fingerprint a keyless
project has and how a pin is checked against a verified certificate.

They deliberately do not cover signature verification. A packslip is a
[sigstore bundle](https://github.com/sigstore/protobuf-specs) and its
signature, certificate chain, and transparency log entry are verified as
sigstore defines, against sigstore's own conformance suite; restating that
here would test sigstore, not packslip. The statement vectors are payloads
rather than bundles for the same reason. This repository's CI signs and
verifies through both schemes end to end against the public log on every
push to `main`.

Nor do they cover the parts of the
[consumer rules](https://packslip.dev/release/v1/#consumer-rules) that
depend on state a consumer carries between installs — no-downgrade,
release-list sequence, minimum release age, where a pin is stored. Those
are properties of a consumer's history, not of a document, so a vector
cannot express them; the specification states them normatively and a
consumer tests them against its own store. The forge identity vectors are
an exception. Each case supplies the remembered pin as input, so a vector
can state how a release is compared with the pin without saying how the
pin was stored.

## Running them

Against the reference implementation:

```sh
cargo test --test conformance
```

They pass with `--no-default-features` too: everything they touch is in
the crate's always-compiled core, which is what a verify-only consumer
depends on.

## Changing the vectors

Each file has its own test in `tests/conformance.rs`, which runs every
case in the file. A new case in an existing file needs no Rust change as
long as it has the shape the file's `description` defines. Give it a
descriptive `name`, which a failing test prints, and a `reason` or
`comment` where the rule is not obvious. A rule that no file covers gets a
new file with `rule`, `description`, and `cases`, a row in the table
above, and a test function in `tests/conformance.rs`. Change vectors in
the same pull request as the specification text they test (see
[CONTRIBUTING.md](../../CONTRIBUTING.md)).
