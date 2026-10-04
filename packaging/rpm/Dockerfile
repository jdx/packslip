FROM fedora:44
RUN dnf install -y --setopt=install_weak_deps=False \
    rust cargo gcc cmake pkgconf-pkg-config rpm-build make ca-certificates \
    && dnf clean all
WORKDIR /build
