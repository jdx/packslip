FROM fedora:46@sha256:a9bab18d01cf2c2cf62f3e79c72623405bce14ac062995cc6651a3073c802e41
RUN dnf install -y ca-certificates gnupg2 && dnf clean all
WORKDIR /build
