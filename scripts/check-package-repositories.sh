#!/usr/bin/env bash
# Exercise signatures and native package-manager installation with an ephemeral key.
set -euo pipefail
packages=$(realpath "${1:?usage: check-package-repositories.sh PACKAGE_DIRECTORY OUTPUT_DIRECTORY}")
out=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "${2:?}")
mkdir -p "$out"
work=$(mktemp -d)
containers=()
cleanup() {
  for container in "${containers[@]}"; do docker rm -f "$container" >/dev/null; done
  rm -rf "$work"
}
trap cleanup EXIT
docker build -t packslip-repository-tools -f packaging/repositories/Dockerfile packaging/repositories
docker build -t packslip-repository-fedora -f packaging/repositories/Fedora.Dockerfile packaging/repositories
mkdir -p "$work/input/scripts" "$work/input/packaging"
cp scripts/package-repositories.sh scripts/verify-package-repositories.sh "$work/input/scripts/"
cp packaging/packslip.repo "$work/input/packaging/"
cp -R "$packages" "$work/input/packages"
container=$(docker create --network none packslip-repository-tools bash -euo pipefail -c '
  cd /input
  export GNUPGHOME=/tmp/signing
  mkdir -m 700 "$GNUPGHOME"
  gpg --batch --pinentry-mode loopback --passphrase "" --quick-generate-key "Packslip CI <ci@example.invalid>" rsa2048 sign 1d
  PACKSLIP_GPG_FINGERPRINT=$(gpg --batch --with-colons --list-secret-keys | awk -F: '\''$1=="fpr" {print $10; exit}'\'')
  export PACKSLIP_GPG_FINGERPRINT
  bash scripts/package-repositories.sh packages /repository
  bash scripts/verify-package-repositories.sh /repository "$PACKSLIP_GPG_FINGERPRINT"
  gpg --verify /repository/apt/dists/stable/InRelease
  gpg --verify /repository/apt/dists/stable/Release.gpg /repository/apt/dists/stable/Release
  for arch in x86_64 aarch64; do
    gpg --verify "/repository/rpm/$arch/repodata/repomd.xml.asc" "/repository/rpm/$arch/repodata/repomd.xml"
  done
  python3 - <<'\''PY'\''
from pathlib import Path
import hashlib
root = Path("/repository/apt")
for path in root.glob("dists/stable/main/binary-*/Packages"):
    for entry in path.read_text().strip().split("\n\n"):
        fields = dict(line.split(": ", 1) for line in entry.splitlines() if ": " in line and not line.startswith(" "))
        assert hashlib.sha256((root/fields["Filename"]).read_bytes()).hexdigest() == fields["SHA256"]
        assert int(fields["Size"]) == (root/fields["Filename"]).stat().st_size
original = (root/"dists/stable/InRelease").read_bytes()
tampered = original.replace(b"Origin: Packslip", b"Origin: tampered", 1)
assert tampered != original
Path("/tmp/tampered").write_bytes(tampered)
PY
  if gpg --verify /tmp/tampered; then echo "accepted tampered metadata" >&2; exit 1; fi
  mkdir -m 700 /tmp/wrong-key
  GNUPGHOME=/tmp/wrong-key gpg --batch --pinentry-mode loopback --passphrase "" --quick-generate-key "Wrong CI <wrong@example.invalid>" rsa2048 sign 1d
  if GNUPGHOME=/tmp/wrong-key gpg --verify /repository/apt/dists/stable/InRelease; then echo "accepted wrong key" >&2; exit 1; fi
  rpm --import /repository/gpg-key.pub
  for package in /repository/rpm/*/packages/*.rpm; do rpm --checksig "$package"; done
  package=$(find /repository/rpm -name "*.rpm" -print -quit)
  python3 - "$package" <<'\''PY'\''
from pathlib import Path
import sys
data = bytearray(Path(sys.argv[1]).read_bytes())
data[-1] ^= 1
Path("/tmp/tampered.rpm").write_bytes(data)
PY
  if rpm --checksig /tmp/tampered.rpm; then echo "accepted tampered package" >&2; exit 1; fi
')
containers+=("$container")
docker cp "$work/input" "$container:/input"
docker start -a "$container"
test "$(docker inspect --format '{{.State.ExitCode}}' "$container")" = 0
docker cp "$container:/repository/." "$out/"
python3 scripts/check-repository-publication.py "$out"
container=$(docker create --network none packslip-repository-tools bash -euo pipefail -c '
  gpg --dearmor --output /tmp/packslip.gpg /repo/gpg-key.pub
  chmod 644 /tmp/packslip.gpg
  printf "deb [signed-by=/tmp/packslip.gpg] file:/repo/apt stable main\n" > /tmp/packslip.sources.list
  apt-get -o Dir::Etc::sourcelist=/tmp/packslip.sources.list -o Dir::Etc::sourceparts=- update
  apt-get -o Dir::Etc::sourcelist=/tmp/packslip.sources.list -o Dir::Etc::sourceparts=- install -y packslip
  packslip install --help
  packslip usage > /tmp/usage.kdl
  grep -q "cmd install" /tmp/usage.kdl
  ! grep -E "cmd (create|releases|keygen|schema)" /tmp/usage.kdl
  test -d /etc/packslip/pins.d
')
containers+=("$container")
docker cp "$out" "$container:/repo"
docker start -a "$container"
test "$(docker inspect --format '{{.State.ExitCode}}' "$container")" = 0
container=$(docker create --network none packslip-repository-fedora bash -euo pipefail -c '
  arch=$(uname -m)
  dnf -y --disablerepo="*" --repofrompath="packslip,file:///repo/rpm/$arch" \
    --enablerepo=packslip --setopt=packslip.gpgcheck=1 --setopt=packslip.repo_gpgcheck=1 \
    --setopt=packslip.gpgkey=file:///repo/gpg-key.pub install packslip
  packslip install --help
  packslip usage > /tmp/usage.kdl
  grep -q "cmd install" /tmp/usage.kdl
  ! grep -E "cmd (create|releases|keygen|schema)" /tmp/usage.kdl
  test -d /etc/packslip/pins.d
')
containers+=("$container")
docker cp "$out" "$container:/repo"
docker start -a "$container"
test "$(docker inspect --format '{{.State.ExitCode}}' "$container")" = 0
