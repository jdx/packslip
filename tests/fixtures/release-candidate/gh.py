#!/usr/bin/env python3
"""Credential-free GitHub CLI protocol fixture; never contacts GitHub."""

import base64
import hashlib
import json
import os
from pathlib import Path
import sys

args = sys.argv[1:]
file = Path(os.environ["FAKE_GH_STATE"])
state = json.loads(file.read_text())
state["calls"].append(args)


def option(name):
    return args[args.index(name) + 1]


def save(value=None):
    file.write_text(json.dumps(state))
    if value is not None:
        print(json.dumps(value))


if args[0] == "api":
    path = next(value for value in args if value.startswith("repos/"))
    if "/actions/runs/" in path:
        save(state["run"])
    elif "/actions/workflows/" in path:
        save({"path": ".github/workflows/release.yml"})
    elif path.endswith("/releases?per_page=100"):
        releases = [state["release"]] if state["release"] and not os.environ.get("FAKE_GH_READ_ONLY") else []
        save([releases])
    elif path.endswith("/releases/latest"):
        save({"tag_name": state["latest"]})
    elif path.endswith("/assets?per_page=100"):
        save([{key: value for key, value in asset.items() if key not in ("content", "source", "attested_digest")}
              for asset in state["assets"].values()])
    elif "/git/matching-refs/" in path:
        major = state.get("major")
        save([{"ref": "refs/tags/v1", "object": {"type": "commit", "sha": major["sha"]}}] if major else [])
    elif "/contents/Cargo.toml?ref=" in path:
        save({"content": base64.b64encode(f'[package]\nversion = "{state["major"]["version"]}"\n'.encode()).decode()})
    elif "/git/refs" in path and "-X" in args:
        if state.get("fail_tag_update"):
            state["fail_tag_update"] = False
            save()
            sys.exit("simulated cancellation/error after publication before major tag update")
        fields = dict(args[index + 1].split("=", 1) for index, value in enumerate(args) if value == "-f")
        state["major"] = {"sha": fields["sha"], "version": "1.6.0"}
        save({"object": {"sha": fields["sha"], "type": "commit"}})
    elif "-X" in args and option("-X") == "PATCH":
        field = option("-f")
        key, value = field.split("=", 1)
        assert key == "target_commitish"
        assert state["release"]["draft"], "never patch an already published release"
        state["release"][key] = value
        save(state["release"])
    elif "-X" in args and option("-X") == "DELETE":
        identifier = int(path.rsplit("/", 1)[1])
        state["assets"] = {name: asset for name, asset in state["assets"].items() if asset["id"] != identifier}
        save()
    else:
        sys.exit(f"unsupported fake API call: {args}")
elif args[:2] == ["release", "create"]:
    assert "--draft" in args and not state["release"]
    state["release"] = {"id": 1, "tag_name": args[2], "draft": True, "target_commitish": option("--target")}
    save()
elif args[:2] == ["release", "upload"]:
    assert state["release"]["draft"]
    for path in args[args.index("--clobber") + 1:]:
        source = Path(path)
        content = source.read_bytes()
        digest = "sha256:" + hashlib.sha256(content).hexdigest()
        state["assets"][source.name] = {
            "id": len(state["assets"]) + 1, "name": source.name, "size": len(content),
            "digest": digest, "attested_digest": digest, "source": os.environ["GITHUB_SHA"],
            "content": content.hex(),
        }
    save()
elif args[:2] == ["release", "download"]:
    target = Path(option("-D"))
    names = [args[index + 1] for index, value in enumerate(args) if value == "-p"]
    assert names
    for name in names:
        (target / name).write_bytes(bytes.fromhex(state["assets"][name]["content"]))
    save()
elif args[:2] == ["release", "view"]:
    print(state["latest"])
    save()
elif args[:2] == ["attestation", "verify"]:
    artifact = Path(args[2])
    asset = state["assets"][artifact.name]
    assert option("--source-digest") == asset["source"]
    assert option("--signer-digest") == asset["source"]
    assert option("--source-ref") == "refs/heads/release-plz"
    assert option("--signer-workflow") == "jdx/packslip/.github/workflows/release.yml"
    assert "--deny-self-hosted-runners" in args
    save()
    if "sha256:" + hashlib.sha256(artifact.read_bytes()).hexdigest() != asset["attested_digest"]:
        sys.exit("fake attestation rejects changed bytes")
else:
    sys.exit(f"unsupported fake CLI call: {args}")
