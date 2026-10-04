#!/usr/bin/env python3
"""Check publication ordering and failure handling without contacting R2."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

repository = str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory() as scratch:
    scratch = Path(scratch)
    aws = scratch / "aws"
    aws.write_text('''#!/usr/bin/env python3
import json, os, sys
with open(os.environ["PACKSLIP_UPLOAD_LOG"], "a") as log:
    log.write(json.dumps(sys.argv[1:]) + "\\n")
sys.exit(1 if os.environ.get("PACKSLIP_UPLOAD_FAIL") == "true" else 0)
''')
    aws.chmod(0o755)
    log = scratch / "uploads.jsonl"
    env = dict(os.environ, PATH=f"{scratch}{os.pathsep}{os.environ['PATH']}",
               PACKSLIP_UPLOAD_LOG=str(log))
    subprocess.run(["bash", "scripts/publish-package-repositories.sh", repository], env=env, check=True)
    uploads = [json.loads(line) for line in log.read_text().splitlines()]
    assert all(command[:2] == ["s3", "cp"] and "--delete" not in command for command in uploads)
    destinations = [command[3] for command in uploads]
    first_mutable = next(index for index, command in enumerate(uploads) if "public, max-age=300" in command)
    assert all("public, max-age=31536000, immutable" in command for command in uploads[:first_mutable])
    for arch in ("amd64", "arm64"):
        index = next(i for i, dest in enumerate(destinations) if dest.endswith(f"/apt/dists/stable/main/binary-{arch}/by-hash/"))
        assert index < first_mutable
    assert destinations[0].endswith("/apt/pool/")
    assert destinations[-1].endswith("/apt/dists/stable/InRelease")
    for arch in ("x86_64", "aarch64"):
        packages = next(i for i, dest in enumerate(destinations) if dest.endswith(f"/rpm/{arch}/packages/"))
        metadata = next(i for i, dest in enumerate(destinations) if dest.endswith(f"/rpm/{arch}/repodata/repomd.xml"))
        assert packages < first_mutable < metadata
    log.unlink()
    failed = subprocess.run(["bash", "scripts/publish-package-repositories.sh", repository],
                            env=dict(env, PACKSLIP_UPLOAD_FAIL="true"), check=False)
    assert failed.returncode != 0
    assert len(log.read_text().splitlines()) == 1, "failed payload upload published metadata"
print("Publication preserves old payloads, publishes indices last, and stops on upload failure.")
