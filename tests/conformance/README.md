# Conformance vectors

Run these JSON test cases to check a packslip implementation in any
language against the
[packslip specification](https://packslip.dev/release/v1/).

## The specification governs

A vector that contradicts the specification text, or requires something
the text does not, is a bug in the vector. Otherwise, an
implementation that disagrees with a vector disagrees with the
specification. If you believe a vector is wrong,
[open an issue](https://github.com/jdx/packslip/issues) instead of
working around it.

## Read a vector file

In each file's JSON object, `rule` links the specification section it
tests. `description` defines the case shape and what the inputs and
expected results mean. The shape differs between files, so read it
before you write a runner; the table below summarizes it. Each entry in
`cases` holds a `name`, the input fields, `expect`, and sometimes a
`reason` or `comment`. Report a failing case by its `name`, which traces
the failure to its rule.

## The files

| File | Rule | Inputs | `expect` |
| --- | --- | --- | --- |
| `artifact-selection.json` | [Selecting an artifact](https://packslip.dev/release/v1/#selecting-an-artifact) | `host` (`os`, `arch`, `libc`), `variant` (`null` for none), `formats` (best first), `artifacts` | `{"artifact": "<name>"}`, `{"error": "no-match"}`, or `{"error": "ambiguous"}` |
| `resource-selection.json` | [Resources](https://packslip.dev/release/v1/#resources) | `statement`; `artifact`, the selected artifact's name | Zero-based positions in `predicate.resources` of the entries that apply, in document order |
| `tag-versions.json` | [Tags](https://packslip.dev/release/v1/#tags) | `tag`, `project` | The version the forge tag names, or `null` |
| `statement-validity.json` | [The release statement](https://packslip.dev/release/v1/#the-release-statement) | `statement`, a decoded payload | `accept` or `reject` |
| `forge-identity.json` | [Repository identity and renames](https://packslip.dev/release/v1/#repository-identity-and-renames) | `pin`, `requested`, `release` (`project`, `signer`, `certificate`), optional `previous_signer` | `outcome` (`same`, `renamed`, or `refuse`); `reason` for a refusal; `signer_continues` for an accepted case with `previous_signer` |
| `signer-fingerprint.json` | [Signer fingerprints](https://packslip.dev/release/v1/#signer-fingerprints) | `kind` (`derive`, `parse`, or `check`) and that kind's inputs from `issuer`, `repository_id`, and `pin` (`check` cases also name `project` and `owner`, which a consumer ignores) | A keyless project's fingerprint or `null` for `derive`; `valid` or `invalid` for `parse`; `match`, `mismatch`, or `no-repository-id` for `check` |

## What the vectors leave out

The vectors do not test signature verification. A packslip is a
[sigstore bundle](https://github.com/sigstore/protobuf-specs): its
signature, certificate chain, and transparency log entry are verified
as sigstore defines and tested by sigstore's own
[conformance suite](https://github.com/sigstore/sigstore-conformance).
The statement vectors contain decoded payloads, not bundles. On every
push to `main`, this repository's CI signs and verifies end to end,
keyless and with a key, against the public log.

The vectors do not model state carried between installs. Test the
[consumer rules](https://packslip.dev/release/v1/#consumer-rules) that
depend on it (signer continuity, no-downgrade, release-list sequence)
against your consumer's own state store. The forge identity cases take
a remembered pin and previous signer as input and test the comparison,
not its storage.

## Run the vectors

From the repository root:

```sh
cargo test --test conformance
cargo test --no-default-features --test conformance
(cd go && go test ./...)
(cd packages/packslip && npm ci && npm test)
```

- [`tests/conformance.rs`](../conformance.rs) runs every file against
  the Rust crate, the reference implementation. `--no-default-features`
  checks the minimal library build that verification-only consumers use.
  The vectors exercise the always-compiled core, so both builds run
  every case.
- [`go/model_test.go`](https://github.com/jdx/packslip/blob/main/go/model_test.go)
  runs `statement-validity.json` through `Statement.Validate` in
  `TestStatementValidityVectors`.
- [`packages/packslip/test/model.test.ts`](https://github.com/jdx/packslip/blob/main/packages/packslip/test/model.test.ts)
  runs `statement-validity.json` through `readStatement`, which applies
  the statement checks that `verifyRelease` runs once sigstore has
  verified the signature.

The Go and npm tests skip themselves when `tests/conformance` is absent.
`npm test` needs network access for sigstore's trust root; to run the
vectors offline, run `node --test test/model.test.ts` in
`packages/packslip` after `npm ci`.

`tests/go_conformance.rs` reads none of these files: it checks that the
Rust verifier accepts the Go module's statements and signed bundles.

CI runs the Rust and npm suites on every pull request, and the Go suite
when `go/` or `statement-validity.json` changes
([What else CI runs](https://github.com/jdx/packslip/blob/main/CONTRIBUTING.md#what-else-ci-runs)).

## Add or change a case

If the rule itself changes, or a vector requires behavior the
specification does not, follow
[Change a format rule](https://github.com/jdx/packslip/blob/main/CONTRIBUTING.md#change-a-format-rule)
instead. It updates the specification, vectors, and other
implementations in one pull request and says what to write in its
description. Otherwise:

1. Find the file whose `rule` links the section your case tests.
2. Add the case in the shape the file's `description` defines. It needs
   no Rust change: [`tests/conformance.rs`](../conformance.rs) runs every
   case in each file.
3. Give it a descriptive `name`, which a failing test prints, and a
   `reason` or `comment` where the rule is not obvious.
4. Run the Rust suite. For `statement-validity.json`, also run the Go and
   npm suites.

To cover a rule no file covers, add a file with `rule`, `description`,
and `cases`. Then add its row to [The files](#the-files), a test
function to `tests/conformance.rs`, and the rule to the specification's
[Conformance coverage](https://packslip.dev/release/v1/#conformance-coverage)
list (then run `mise run render`).
