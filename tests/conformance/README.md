# Conformance vectors

The rules in the [packslip specification](https://packslip.dev/release/v1/)
in executable form. They exist for implementations other than this one: a
consumer written in any language can read these files and check that it
selects, parses, and refuses what the specification says it should.

An implementation that disagrees with a vector disagrees with the
specification. If you believe a vector is wrong, that is a specification
bug — open an [issue](https://github.com/jdx/packslip/issues) rather than
working around it.

| File | Rule |
| --- | --- |
| `artifact-selection.json` | [Selecting an artifact](https://packslip.dev/release/v1/#selecting-an-artifact) |
| `resource-selection.json` | [Resources](https://packslip.dev/release/v1/#resources) |
| `tag-versions.json` | [Tags](https://packslip.dev/release/v1/#tags) |
| `statement-validity.json` | [The release statement](https://packslip.dev/release/v1/#the-release-statement) |

Each file is a JSON object with a `rule` link, a `description` of what the
cases mean, and a `cases` array. Every case has a `name`; some carry a
`reason` or `comment` explaining the rule at issue. The `description`
field defines that file's case shape — read it before writing a runner.

## Scope

These cover what is packslip's own: which artifact a host installs, which
resource entries apply to it, which version a tag names, and whether a
statement is structurally valid.

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
depend on state a consumer carries between installs — signer continuity,
no-downgrade, release-list sequence, minimum release age. Those are
properties of a consumer's history, not of a document, so a vector cannot
express them; the specification states them normatively and a consumer
tests them against its own store.

## Running them

Against the reference implementation:

```bash
cargo test --test conformance
```

They pass with `--no-default-features` too: everything they touch is in
the crate's always-compiled core, which is what a verify-only consumer
depends on.
