#!/usr/bin/env bash
# Build the vendored source with distro toolchains and no build-time network.
set -euo pipefail
kind=${1:?usage: check-distro-packages.sh deb|rpm SOURCE_DIRECTORY OUTPUT_DIRECTORY}
source_dir=$(realpath "${2:?}")
out=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "${3:?}")
mkdir -p "$out"
case "$kind" in
  deb)
    docker build -t packslip-deb-builder -f packaging/debian/Dockerfile packaging/debian
    docker run --rm --network=none -v "$source_dir:/source:ro" -v "$out:/out" packslip-deb-builder bash -euo pipefail -c '
      tar -xf /source/packslip-*.tar.gz -C /build
      cd /build/packslip-*
      cp -R packaging/debian debian
      chmod +x debian/rules
      version=$(python3 -c '\''import tomllib; print(tomllib.load(open("Cargo.toml","rb"))["package"]["version"])'\'')
      printf "packslip (%s-0ppa1~resolute) resolute; urgency=medium\n\n  * Test build.\n\n -- Jeff Dickey <jdx@jdx.dev>  %s\n" "$version" "$(date -R)" > debian/changelog
      dpkg-buildpackage -b -us -uc
      cp /build/*.deb /out/
      dpkg -i /build/*.deb
      packslip install --help
      packslip usage > /out/installer.usage.kdl
      grep -q "cmd install" /out/installer.usage.kdl
      ! grep -E "cmd (create|releases|keygen|schema)" /out/installer.usage.kdl
      test -d /etc/packslip/pins.d
    '
    ;;
  rpm)
    docker build -t packslip-rpm-builder -f packaging/rpm/Dockerfile packaging/rpm
    docker run --rm --network=none -v "$source_dir:/source:ro" -v "$out:/out" packslip-rpm-builder bash -euo pipefail -c '
      mkdir -p /build/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}
      cp /source/packslip-*.tar.gz /build/SOURCES/
      cp /source/packslip.spec /build/SPECS/
      rpmbuild --define "_topdir /build" -ba /build/SPECS/packslip.spec
      cp /build/RPMS/*/*.rpm /build/SRPMS/*.rpm /out/
      rpm -i /build/RPMS/*/*.rpm
      packslip install --help
      packslip usage > /out/installer.usage.kdl
      grep -q "cmd install" /out/installer.usage.kdl
      ! grep -E "cmd (create|releases|keygen|schema)" /out/installer.usage.kdl
      test -d /etc/packslip/pins.d
    '
    ;;
  *) echo "unknown package kind: $kind" >&2; exit 1 ;;
esac
