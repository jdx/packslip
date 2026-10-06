FROM fedora:44@sha256:43b29f65a41eb9c35e1cd5323e3bdf3b655c2357a9f4f1ff2f9c2798e5045d80
RUN dnf install -y --setopt=install_weak_deps=False \
    rust cargo gcc cmake pkgconf-pkg-config rpm-build make ca-certificates jq \
    && dnf clean all
WORKDIR /build
