# Test fixtures

These files give tests fixed executable layouts, real historical release
bundles, and a fake GitHub CLI. Read this before you rebuild, restore, or
add one, or change the compatibility baseline.

The fixtures ship in the published crate because unit tests in `src/`
read the executables and the hk bundle, some with `include_str!`. Keep
them in the crate and small. Any change here, this README included,
raises the version and gets a `CHANGELOG.md` line; see
[What counts toward a release](https://github.com/jdx/packslip/blob/main/RELEASING.md#what-counts-toward-a-release).

## Executables

Each is a stripped x86_64 Linux ELF executable that does nothing.
`src/linkage.rs`, `src/create.rs`, and `tests/cli.rs` test what
`packslip create` reads from them:

| File | Interpreter | `DT_NEEDED` | What `create` records |
| --- | --- | --- | --- |
| `needs-z` | `/lib64/ld-linux-x86-64.so.2` | `libz.so.1`, `libc.so.6` | `libc = "gnu"` |
| `static` | None, like a `CGO_ENABLED=0` Go binary | None | No `libc` |
| `needs-musl` | `/lib/ld-musl-x86_64.so.1`, as on Alpine | `libc.musl-x86_64.so.1` | `libc = "musl"` |

`create` infers libc this way only when the file name names none. Tests
also wrap `needs-z` in archives to check that `create` records
`requires.libs` from what it loads.

Rebuild in a scratch directory, copy the file into `tests/fixtures/`,
check `readelf -lWd <file>` against the table, and run `cargo test`.

### Rebuild `needs-z`

Use a glibc system with gcc and the zlib development files. Ubuntu
24.04's GCC 13.3.0, named in the committed file's `.comment`, reproduces
it byte for byte:

```sh
printf 'int main(void){return 0;}\n' > tiny.c
gcc -Os -s -o needs-z tiny.c -Wl,--no-as-needed -lz
```

### Rebuild `static` and `needs-musl`

Save these three instructions, which call `exit(0)`, as `start.s`:

```asm
.globl _start
.text
_start:
  mov $60, %eax
  xor %edi, %edi
  syscall
```

rustup keeps `rust-lld` under each toolchain's `lib/rustlib/<host>/bin/`,
not on `PATH`; the first command adds it. The fourth makes a stub
`libc.musl-x86_64.so.1` for `needs-musl` to link against; don't commit
it.

```sh
export PATH="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin:$PATH"
clang -target x86_64-unknown-linux -c start.s
: > empty.s && clang -target x86_64-unknown-linux -c empty.s
rust-lld -flavor gnu -shared -soname libc.musl-x86_64.so.1 -o libc.musl-x86_64.so.1 empty.o
rust-lld -flavor gnu -static -s -o static start.o
rust-lld -flavor gnu -pie -s --dynamic-linker /lib/ld-musl-x86_64.so.1 --no-as-needed -o needs-musl start.o libc.musl-x86_64.so.1
```

rustup's rust-lld, LLD 21.1.8 through 23.1.4, matches both files except
for the linker string in `.comment`, where the committed files have
`Linker: LLD 22.1.8 (https://github.com/rust-lang/llvm-project.git 52ed14fcd56afc30f9cccd8ca8ce237c2eef7e04)`.
Compare after `objcopy --remove-section .comment`. Another linker or
version can produce different bytes; tests read only the interpreter
and libraries.

### Add an executable

Build the smallest stripped program with the layout under test, then
add a table row and a rebuild section with its recipe and toolchain.

## Release bundles

The two bundles test verification and repository identity. GitHub
Actions release workflows signed both keylessly, each with a Rekor
`dsse` 0.0.1 log entry. Tests verify them offline against the embedded
trusted root. Never regenerate, re-sign, or edit them; tests depend on
their exact certificates, log entries, and fingerprints. A change that
stops one verifying is a compatibility break
([Keep old releases verifying](https://github.com/jdx/packslip/blob/main/CONTRIBUTING.md#keep-old-releases-verifying)).

### `hk-v2.3.0.sigstore.json`

This is the packslip jdx/hk published with its v2.3.0 release, signed by
`https://github.com/jdx/hk/.github/workflows/release.yml@refs/tags/v2.3.0`.
Its Fulcio certificate records repository ID 922514152 and owner ID
216188, matching `gh api repos/jdx/hk --jq '.id,.owner.id'`.

`tests/cli.rs` and `tests/verifier_cli.rs` check:

- the IDs `packslip verify` reports;
- that `packslip pin` prints `ps1_snirenkjwr7m5ozgcufameodnm`, which
  `verify --pin` accepts while refusing another repository's;
- how identity flags and `--source-repository` narrow a forge project's
  policy, with names matched in any case, and what
  `--allow-other-repository` requires.

`verifier/test.mjs` uses it too. A real bundle signed outside its
repository needs a live Fulcio certificate, so signer tests in
`src/main.rs` check hk's certificate as if its statement named
`github.com/jdx/mise`. CI's `signing` job covers the live case on pushes
to `main`.

### `mkghtag-v0.1.14-10.sigstore.json`

This is the packslip suzuki-shunsuke/mkghtag published with its
v0.1.14-10 prerelease, signed through another repository's reusable
workflow. Its certificate identity is
`https://github.com/suzuki-shunsuke/go-release-workflow/.github/workflows/release.yaml@refs/heads/add-packslip`;
its Source Repository URI is `https://github.com/suzuki-shunsuke/mkghtag`
(repository ID 701940692, owner ID 13323303).

`tests/cli.rs` checks that `packslip verify` and `packslip pin` accept it
without trust flags, because the source repository binds it to the
project, and that `pin` prints `ps1_xfjx4bjm6cpir2jd2yovxlyqxm`.
`--identity-prefix https://github.com/suzuki-shunsuke/mkghtag/` still
refuses it, as does a `--source-repository` naming go-release-workflow.

### Check a bundle against its release

From `tests/fixtures/`, download each release asset onto its committed
path and compare the hashes with the table:

```sh
gh release download v2.3.0 --repo jdx/hk --pattern packslip.sigstore.json --output hk-v2.3.0.sigstore.json --clobber
gh release download v0.1.14-10 --repo suzuki-shunsuke/mkghtag --pattern packslip.sigstore.json --output mkghtag-v0.1.14-10.sigstore.json --clobber
sha256sum hk-v2.3.0.sigstore.json mkghtag-v0.1.14-10.sigstore.json
```

| File | Bytes | SHA-256 |
| --- | --- | --- |
| `hk-v2.3.0.sigstore.json` | 20395 | `e613bc35013a998825ca9a7e9fe5f72303cfb658c8626f3aeb0c818d2209d8db` |
| `mkghtag-v0.1.14-10.sigstore.json` | 17536 | `7a9d4a665e9d24f924cf1365352ef99c7ad6d928c96740ac2936393cb0686d11` |

If a hash differs, the asset changed. Restore the committed record from
the repository root; this also repairs a damaged staged copy:

```sh
git restore --source=HEAD --staged --worktree tests/fixtures/hk-v2.3.0.sigstore.json tests/fixtures/mkghtag-v0.1.14-10.sigstore.json
```

### Add a bundle

1. Download its release's `packslip.sigstore.json` into
   `tests/fixtures/`, named after the tool and tag. Add its download
   line, size, and SHA-256 to
   [Check a bundle against its release](#check-a-bundle-against-its-release).
2. Check that `packslip verify <file>` accepts it, and get its signer
   fingerprint from `packslip pin <file>`.
3. Give it a section naming its release, signer, signer fingerprint, and
   what its tests check.

## Fake GitHub CLI

`release-candidate/gh.py` stands in for `gh` in
`tests/release_candidate_test.py`. Those tests check offline how
`scripts/release-candidate.py` stages, checks, and publishes packslip's
releases ([RELEASING.md](https://github.com/jdx/packslip/blob/main/RELEASING.md)
describes the sequence). `CandidateProtocolTests` put the fake first on
`PATH` as `gh`; `CandidateTests` patch the script's API calls and never
run `gh`. CI's `test` job runs them:

```sh
python3 -B -m unittest discover -s tests -p 'release_candidate_test.py'
```

The hand-written fake models only the script's calls and asserts the
arguments it must pass, such as those to `gh attestation verify`; update
the assertions when they change. Any other call exits with
`unsupported fake API call: ...` or `unsupported fake CLI call: ...`;
add a branch that models GitHub's response.

`FAKE_GH_STATE` names the JSON file holding the fake release, its
assets, every call, and the starting `published` tags, `latest` release,
and `major` (`v1`) tag and version. These settings reproduce forge
behavior:

| Setting | Effect |
| --- | --- |
| `asset_lag_after_upload` | Asset listings stay empty for that many reads after an upload |
| `hide_created_draft` | Listings and `gh release upload` cannot see a new draft |
| `wrong_retarget_tag` | A retarget returns the tag `untagged-test` |
| `fail_tag_update` | The `v1` tag update fails once, after publication |
| `FAKE_GH_READ_ONLY=1` (environment) | The token cannot list drafts |

## Compatibility baseline

v1.4.0 is the CI baseline. The `compatibility` job in
`.github/workflows/ci.yml`, which the required `final` check needs,
builds it from commit `87479dfc6443253dff69601cace5fc6ea07e6df5`
alongside the current verifier. `tests/compatibility.rs` runs each
cross-version check with the current binary, and with the baseline when
`PACKSLIP_BASELINE_BIN` is set, as the job does:

- The hk bundle verifies with an explicit issuer and identity.
- Generated key-signed release and list bundles (Sigstore bundle v0.3)
  verify with unknown optional fields, an extension, and an unknown
  resource kind. Having no log entry, they verify with `--pubkey` and
  opt in with `--allow-unlogged`, so no live log is needed.
- A modified artifact fails, and a signed `release/v2` statement is
  refused.

v1.4.0 predates `--pin`, so repository-ID signer fingerprint checks run
on the current binary only. It alone checks the mkghtag bundle. The job
also runs `trust_root::tests` and `install::tests`.
[Compatibility and support](https://packslip.dev/docs/compatibility/)
lists the tested formats and the limits of that evidence.

### Change the baseline

Keep existing baselines when you add one; never advance the only
baseline to the latest release. The baseline shows that release bundles
and lists made today still verify with an older release.

1. In the `compatibility` job, add:
   - a checkout of the release's full commit SHA at its own `path:` under
     `.compatibility/`, with a comment naming the release;
   - `<path> -> target` to the `Swatinem/rust-cache` `workspaces` list;
   - a `cargo build --locked --manifest-path <path>/Cargo.toml` step.

   Test it locally first with the command in
   [Keep old releases verifying](https://github.com/jdx/packslip/blob/main/CONTRIBUTING.md#keep-old-releases-verifying).
2. Add the new binary's absolute path,
   `${{ github.workspace }}/<path>/target/debug/packslip`, to the `env:`
   of the "Cross-version verification and installer fixtures" step.
   Extend `verifiers()` in `tests/compatibility.rs`, which reads one
   `PACKSLIP_BASELINE_BIN` today and asserts it is absolute, to read
   each baseline.
3. Name the new baseline next to v1.4.0 here, in the module comment of
   `tests/compatibility.rs`, on
   [Compatibility and support](https://packslip.dev/docs/compatibility/),
   in CONTRIBUTING.md's
   [Keep old releases verifying](https://github.com/jdx/packslip/blob/main/CONTRIBUTING.md#keep-old-releases-verifying)
   with its local test command, and in the `compatibility` row of
   [What else CI runs](https://github.com/jdx/packslip/blob/main/CONTRIBUTING.md#what-else-ci-runs).
