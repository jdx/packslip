# Test fixtures

- `needs-z`: a stripped x86_64 Linux ELF executable that does nothing and
  links `libz.so.1` and `libc.so.6`. Built with
  `gcc -Os -s -o needs-z tiny.c -Wl,--no-as-needed -lz` from
  `int main(void){return 0;}`. Tests wrap it in archives to check that
  `packslip create` records `requires.libs` from what the executable loads.
- `hk-v2.3.0.sigstore.json`: the packslip `jdx/hk` published with its
  v2.3.0 GitHub release, signed keylessly by its release workflow. Its
  Fulcio certificate records repository ID 922514152 and owner ID 216188,
  which is what `gh api repos/jdx/hk --jq '.id,.owner.id'` gives. Tests
  verify it offline against the embedded trusted root.
