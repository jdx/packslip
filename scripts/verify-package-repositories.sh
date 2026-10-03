#!/usr/bin/env bash
# Verify the exact publishing key, signed metadata, indices, and package bytes.
set -euo pipefail
repo=$(realpath "${1:?usage: verify-package-repositories.sh REPOSITORY_DIRECTORY FINGERPRINT}")
fingerprint=${2:?}
[[ "$fingerprint" =~ ^[0-9A-Fa-f]{40}$ ]] || exit 1
fingerprint=$(printf '%s' "$fingerprint" | tr '[:lower:]' '[:upper:]')
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
export GNUPGHOME="$work/keyring"
mkdir -m 700 "$GNUPGHOME"
gpg --batch --import "$repo/gpg-key.pub"
gpg --batch --export "$fingerprint" > "$work/key.gpg"
test -s "$work/key.gpg"
gpgv --output "$work/release" --keyring "$work/key.gpg" "$repo/apt/dists/stable/InRelease"
cmp "$work/release" "$repo/apt/dists/stable/Release"
gpgv --keyring "$work/key.gpg" "$repo/apt/dists/stable/Release.gpg" "$repo/apt/dists/stable/Release"
gpg --batch --armor --export "$fingerprint" > "$work/key.asc"
rpm --dbpath "$work/rpmdb" --import "$work/key.asc"
for arch in x86_64 aarch64; do
  gpgv --keyring "$work/key.gpg" "$repo/rpm/$arch/repodata/repomd.xml.asc" "$repo/rpm/$arch/repodata/repomd.xml"
  for package in "$repo/rpm/$arch/packages/"*.rpm; do
    rpm --dbpath "$work/rpmdb" --checksig "$package"
  done
done
python3 - "$repo" <<'PY'
from pathlib import Path
from datetime import datetime, timezone
from email.utils import parsedate_to_datetime
import hashlib, sys, xml.etree.ElementTree as ET
repository = Path(sys.argv[1])
root = repository/'apt'
release = (root/'dists/stable/Release').read_text()
expiry = next(line.removeprefix('Valid-Until: ') for line in release.splitlines() if line.startswith('Valid-Until: '))
assert parsedate_to_datetime(expiry) > datetime.now(timezone.utc), 'expired repository metadata'
sha256 = release.split('SHA256:\n', 1)[1].split('\nSHA512:', 1)[0]
for line in sha256.strip().splitlines():
    digest, size, name = line.split()
    data = (root/'dists/stable'/name).read_bytes()
    assert hashlib.sha256(data).hexdigest() == digest
    assert len(data) == int(size)
for arch in ('amd64', 'arm64'):
    index = root/f'dists/stable/main/binary-{arch}'
    path = index/'Packages'
    assert path.stat().st_size > 0
    for entry in path.read_text().strip().split('\n\n'):
        fields = dict(line.split(': ', 1) for line in entry.splitlines() if ': ' in line and not line.startswith(' '))
        assert fields['Architecture'] == arch
        package = root/fields['Filename']
        assert hashlib.sha256(package.read_bytes()).hexdigest() == fields['SHA256']
        assert package.stat().st_size == int(fields['Size'])
    for name in ('Packages', 'Packages.gz'):
        data = (index/name).read_bytes()
        assert (index/'by-hash/SHA256'/hashlib.sha256(data).hexdigest()).read_bytes() == data
ns = {'r': 'http://linux.duke.edu/metadata/repo'}
for arch in ('x86_64', 'aarch64'):
    rpm = repository/f'rpm/{arch}'
    for item in ET.parse(rpm/'repodata/repomd.xml').getroot().findall('r:data', ns):
        checksum = item.find('r:checksum', ns)
        assert checksum.attrib['type'] == 'sha256'
        path = rpm/item.find('r:location', ns).attrib['href']
        data = path.read_bytes()
        assert hashlib.sha256(data).hexdigest() == checksum.text
        assert len(data) == int(item.find('r:size', ns).text)
        assert path.name.startswith(checksum.text + '-')
    for path in (rpm/'packages').glob('*.rpm'):
        assert path.name == f'packslip-{hashlib.sha256(path.read_bytes()).hexdigest()}.rpm'
PY
