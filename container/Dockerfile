# packslip's container image, ghcr.io/jdx/packslip: the static Linux
# executable and a CA bundle on an otherwise empty image, for linux/amd64
# and linux/arm64. It is not built from source here: container/build.sh
# stages the executables a release already built and attested, so the
# image runs the same bytes as the release's own downloads.
#
# In a Dockerfile, one digest pins every architecture, and the image you
# copy into needs no curl, tar, or hash tool:
#
#   COPY --from=ghcr.io/jdx/packslip:X.Y.Z@sha256:<digest> /packslip /usr/local/bin/packslip
#
# Or run it directly:
#
#   docker run --rm -v "$PWD:/work" -w /work ghcr.io/jdx/packslip verify packslip.sigstore.json
FROM scratch
ARG TARGETARCH
# packslip's HTTPS reads the system's certificates, and an empty image has
# none.
COPY ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY packslip-${TARGETARCH} /packslip
ENTRYPOINT ["/packslip"]
