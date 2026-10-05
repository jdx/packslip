FROM ubuntu:26.04
RUN apt-get update && apt-get install -y --no-install-recommends \
    rustc cargo build-essential cmake pkg-config debhelper devscripts fakeroot ca-certificates jq \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
