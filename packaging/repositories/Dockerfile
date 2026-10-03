FROM ubuntu:26.04
RUN apt-get update && apt-get install -y --no-install-recommends \
    apt-utils dpkg-dev rpm createrepo-c gnupg python3 ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
