FROM ubuntu:26.04@sha256:d66b366dac61b575bee2aef07e9c26e8070096fbe84a3d4d3eb0da069b5d76db
RUN apt-get update && apt-get install -y --no-install-recommends \
    apt-utils dpkg-dev rpm createrepo-c gnupg python3 ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
