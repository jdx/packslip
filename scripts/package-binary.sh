#!/usr/bin/env bash
# Package a native, complete installer build; this is separate from COPR/PPA sources.
set -euo pipefail
input=$(realpath "${1:?usage: package-binary.sh INSTALLER_DIRECTORY x64|arm64 OUTPUT_DIRECTORY}")
arch=${2:?}
out=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "${3:?}")
version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 1
case "$arch" in
  x64) deb_arch=amd64; rpm_arch=x86_64 ;;
  arm64) deb_arch=arm64; rpm_arch=aarch64 ;;
  *) exit 1 ;;
esac
python3 - "$input/packslip" "$arch" <<'PY'
from pathlib import Path
import struct, sys
data = Path(sys.argv[1]).read_bytes()
if data[:6] != b'\x7fELF\x02\x01':
    raise SystemExit('expected a little-endian 64-bit ELF installer')
machine = struct.unpack_from('<H', data, 18)[0]
if machine != {'x64': 62, 'arm64': 183}[sys.argv[2]]:
    raise SystemExit('installer architecture does not match the package')
offset = struct.unpack_from('<Q', data, 32)[0]
size, count = struct.unpack_from('<HH', data, 54)
for index in range(count):
    if struct.unpack_from('<I', data, offset + size * index)[0] == 3:
        raise SystemExit('repository installer must link statically')
PY
mkdir -p "$out"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
payload="$work/payload"
install -Dm755 "$input/packslip" "$payload/usr/bin/packslip"
install -d "$payload/etc/packslip/pins.d"
install -Dm644 packaging/packslip.1 "$payload/usr/share/man/man1/packslip.1"
install -Dm644 LICENSE "$payload/usr/share/doc/packslip/LICENSE"
install -Dm644 content/docs/bootstrap.md "$payload/usr/share/doc/packslip/bootstrap.md"
install -Dm644 "$input/packslip.bash" "$payload/usr/share/bash-completion/completions/packslip"
install -Dm644 "$input/packslip.zsh" "$payload/usr/share/zsh/site-functions/_packslip"
install -Dm644 "$input/packslip.fish" "$payload/usr/share/fish/vendor_completions.d/packslip.fish"
mkdir "$payload/DEBIAN"
cat > "$payload/DEBIAN/control" <<EOF
Package: packslip
Version: $version-1
Architecture: $deb_arch
Maintainer: Jeff Dickey <jdx@jdx.dev>
Section: utils
Priority: optional
Depends: ca-certificates
Homepage: https://packslip.dev
Description: Install authenticated upstream releases
 Installs complete release trees and exports declared commands while retaining
 trust and ownership state. Does not run downloaded setup code or edit shells.
EOF
dpkg-deb --build --root-owner-group "$payload" "$out/packslip_${version}-1_${deb_arch}.deb"
rm -r "$payload/DEBIAN"
mkdir -p "$work/rpm"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}
tar -czf "$work/rpm/SOURCES/payload.tar.gz" -C "$work" payload
cat > "$work/rpm/SPECS/packslip.spec" <<EOF
%global debug_package %{nil}
Name: packslip
Version: $version
Release: 1
Summary: Install authenticated upstream releases
License: MIT
URL: https://packslip.dev
Source0: payload.tar.gz
Requires: ca-certificates
AutoReqProv: no
%description
Installs complete authenticated release trees and exports declared commands,
retaining trust and ownership state without running downloaded setup code.
%prep
%setup -q -n payload
%install
mkdir -p %{buildroot}
cp -a . %{buildroot}/
%files
%license /usr/share/doc/packslip/LICENSE
%doc /usr/share/doc/packslip/bootstrap.md
/usr/bin/packslip
/usr/share/man/man1/packslip.1*
%dir /etc/packslip
%dir /etc/packslip/pins.d
/usr/share/bash-completion/completions/packslip
/usr/share/zsh/site-functions/_packslip
/usr/share/fish/vendor_completions.d/packslip.fish
EOF
rpmbuild --define "_topdir $work/rpm" --define '__brp_strip /bin/true' \
  --define '__brp_strip_static_archive /bin/true' --define '__brp_strip_comment_note /bin/true' \
  --target "$rpm_arch" -bb "$work/rpm/SPECS/packslip.spec"
cp "$work/rpm/RPMS/$rpm_arch/"*.rpm "$out/"
