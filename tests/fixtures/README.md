# Test fixtures

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

  Both are the same three instructions, `exit(0)`, assembled from

  ```asm
  .globl _start
  .text
  _start:
    mov $60, %eax
    xor %edi, %edi
    syscall
  ```

  with `clang -target x86_64-unknown-linux -c start.s` and linked with
  `rust-lld -flavor gnu`: `-static -s -o static start.o` for the first, and
  for the second `-pie -s --dynamic-linker /lib/ld-musl-x86_64.so.1
  --no-as-needed -o needs-musl start.o libc.musl-x86_64.so.1`, against a
  stub made with `-shared -soname libc.musl-x86_64.so.1` from an empty
  object.

- `hk-v2.3.0.sigstore.json`: the packslip `jdx/hk` published with its
  v2.3.0 GitHub release, signed keylessly by its release workflow. Its
  Fulcio certificate records repository ID 922514152 and owner ID 216188,
  which is what `gh api repos/jdx/hk --jq '.id,.owner.id'` gives. Tests
  verify it offline against the embedded trusted root.
