# Test fixtures

- `needs-z`: a stripped x86_64 Linux ELF executable that does nothing and
  links `libz.so.1` and `libc.so.6`. Built with
  `gcc -Os -s -o needs-z tiny.c -Wl,--no-as-needed -lz` from
  `int main(void){return 0;}`. Tests wrap it in archives to check that
  `packslip create` records `requires.libs` from what the executable loads.
