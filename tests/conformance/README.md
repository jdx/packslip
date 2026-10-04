# Conformance vectors

Use these JSON test vectors to check an implementation against the
[packslip specification](https://packslip.dev/release/v1/). A consumer in
any language can run the cases to test parsing, artifact and resource
selection, repository identity, and signer fingerprints.

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
| `forge-identity.json` | [Repository identity and renames](https://packslip.dev/release/v1/#repository-identity-and-renames) |
| `signer-fingerprint.json` | [Signer fingerprint](https://packslip.dev/release/v1/#signer-fingerprints) |

## Read a vector file

Each file is a JSON object with these fields:

| Field | Meaning |
| --- | --- |
| `rule` | Link to the specification section the file tests |
| `description` | Case shape and what the inputs and expected results mean |
| `cases` | Named test cases; some also carry a `reason` or `comment` |

Read `description` before implementing a runner: the case shape differs
between files. Report each case's `name` when a result differs so a failure
can be traced to the corresponding rule.

## Scope

The vectors cover packslip's format and selection rules: which artifact
fits a host, which resources apply, which version a tag names, whether a
statement is structurally valid, and whether a verified certificate
matches the expected repository and fingerprint.

Signature verification has separate coverage. A packslip is a
[sigstore bundle](https://github.com/sigstore/protobuf-specs) and its
signature, certificate chain, and transparency log entry are verified as
sigstore defines, against sigstore's own conformance suite. The statement
vectors contain decoded payloads rather than bundles. This repository's CI
also signs and verifies through both schemes end to end against the public
log on every push to `main`.

The vectors do not model a full installation history. Some
[consumer rules](https://packslip.dev/release/v1/#consumer-rules) depend
on state a consumer carries between installs: trust continuity,
release-list sequence, minimum release age, and pin storage. Test those
rules against the consumer's own state store. The forge identity cases
do include a remembered pin as input, which tests comparison with that
pin without prescribing storage or an installation history.

## Running them

Against the reference implementation:

```sh
cargo test --test conformance
```

To check the minimal library build used by verification-only consumers:

```sh
cargo test --no-default-features --test conformance
```

The vectors exercise the crate's always-compiled core, so both builds
run the same cases.

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
