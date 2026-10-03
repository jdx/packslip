FROM fedora:44
RUN dnf install -y ca-certificates gnupg2 && dnf clean all
WORKDIR /build
