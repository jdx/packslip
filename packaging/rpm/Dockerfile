FROM fedora:46@sha256:a9bab18d01cf2c2cf62f3e79c72623405bce14ac062995cc6651a3073c802e41
RUN dnf install -y --setopt=install_weak_deps=False \
    rust cargo gcc cmake pkgconf-pkg-config rpm-build make ca-certificates jq \
    && dnf clean all
WORKDIR /build
