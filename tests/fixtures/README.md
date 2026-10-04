# Test fixtures

These files give tests fixed executable layouts and a real historical
signature to inspect. The three ELF files test metadata extraction;
the hk bundle tests verification and repository identity.

## Executable metadata

- `needs-z`: a stripped x86_64 Linux ELF executable that does nothing and
  links `libz.so.1` and `libc.so.6`. Built with
  `gcc -Os -s -o needs-z tiny.c -Wl,--no-as-needed -lz` from
  `int main(void){return 0;}`. Tests wrap it in archives to check that
  `packslip create` records `requires.libs` from what the executable loads.

- `static`: a stripped, statically linked x86_64 Linux ELF executable with
  no interpreter and no `DT_NEEDED` entries, like a Go binary built with
  `CGO_ENABLED=0`. Tests check that `packslip create` leaves `libc` out for
  it.

- `needs-musl`: a stripped x86_64 Linux ELF executable whose interpreter is
  `/lib/ld-musl-x86_64.so.1` and which links `libc.musl-x86_64.so.1`, as a
  program built on Alpine does. Tests check that `packslip create` records
  `libc = "musl"` for it when the file name does not say.

## Historical release bundle

`hk-v2.3.0.sigstore.json` is the packslip `jdx/hk` published with its
v2.3.0 GitHub release, signed keylessly by its release workflow. Its
Fulcio certificate records repository ID 922514152 and owner ID 216188,
which is what `gh api repos/jdx/hk --jq '.id,.owner.id'` gives. Tests
verify it offline against the embedded trusted root. Tests also check
that `packslip pin` prints its signer fingerprint,
`ps1_snirenkjwr7m5ozgcufameodnm`, and that `packslip verify --pin`
accepts that fingerprint and refuses another repository's.

## Compatibility coverage

`tests/compatibility.rs` also generates v0.3 key-signed release and list bundles
with unknown optional fields and resources. CI verifies these and the historical
hk fixture with both the current binary and the pinned v1.4.0 source build.
The generated fixtures explicitly allow unlogged signatures. The required TUF
tests use a synthetic three-root signed chain; they do not replay production
Sigstore history. See the [support matrix](../../content/docs/compatibility.md)
for the tested formats, baseline version, and limits of that evidence.

## Building `static` and `needs-musl`

`static` and `needs-musl` are the same three instructions, `exit(0)`.
Save them as `start.s`:

```asm
.globl _start
.text
_start:
  mov $60, %eax
  xor %edi, %edi
  syscall
```

Assemble and link them with clang and rust-lld. rustup installs `rust-lld`
with each toolchain under `lib/rustlib/<host>/bin/`, not on `PATH`. The
third command builds a stub `libc.musl-x86_64.so.1` from an empty object,
only so that `needs-musl` can link against it; the stub is not committed.

```sh
clang -target x86_64-unknown-linux -c start.s
: > empty.s && clang -target x86_64-unknown-linux -c empty.s
rust-lld -flavor gnu -shared -soname libc.musl-x86_64.so.1 -o libc.musl-x86_64.so.1 empty.o
rust-lld -flavor gnu -static -s -o static start.o
rust-lld -flavor gnu -pie -s --dynamic-linker /lib/ld-musl-x86_64.so.1 --no-as-needed -o needs-musl start.o libc.musl-x86_64.so.1
```

A different linker version can produce files a few bytes larger or
smaller than the committed ones.
