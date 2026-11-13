FROM fedora:44@sha256:15da97ed9aa94dd3d92c3925eb89520f6c80e07ea7ee24cba848d1f8b6912b3e
RUN dnf install -y ca-certificates gnupg2 && dnf clean all
WORKDIR /build
