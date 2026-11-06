FROM ubuntu:26.04@sha256:6a06111c3b0a8600feadb270aec771dc239bf407b25625f93f44ab00e11f4d3c
RUN apt-get update && apt-get install -y --no-install-recommends \
    rustc cargo build-essential cmake pkg-config debhelper devscripts fakeroot ca-certificates jq \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
