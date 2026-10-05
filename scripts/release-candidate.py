#!/usr/bin/env python3
"""Stage and verify exact release bytes, without rebuilding at the release tag.

Only action/release.json may differ between the attested source and the final
release tree. GitHub's original build attestation remains about that source.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib

REPO = "jdx/packslip"
LOCK = Path("action/release.json")
WORKFLOW = f"{REPO}/.github/workflows/release.yml"


def run(*args):
    return subprocess.check_output(args, text=True).strip()


def api(path, *args):
    return json.loads(run("gh", "api", f"repos/{REPO}/{path}", *args))


def version():
    with open("Cargo.toml", "rb") as file:
        value = tomllib.load(file)["package"]["version"]
    if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", value):
        raise ValueError("unsupported release version")
    return value


def names(value):
    result = {"packslip.usage.kdl", "packslip.1", "install.sh", "install.ps1"}
    result.update(f"packslip.{shell}" for shell in ("bash", "zsh", "fish", "powershell"))
    for target in ("linux-x64", "linux-arm64", "darwin-arm64", "windows-x64", "windows-arm64"):
        base = f"packslip-v{value}-{target}"
        result.add(base + (".zip" if target.startswith("windows") else ".tar.xz"))
        result.add(base + (".exe" if target.startswith("windows") else ""))
    return result


def validate(data, value):
    if set(data) != {"schema", "version", "source_commit", "run_id", "assets"}:
        raise ValueError("unexpected candidate metadata fields")
    if type(data["schema"]) is not int or data["schema"] != 1 or data["version"] != value:
        raise ValueError("candidate schema/version mismatch")
    if not re.fullmatch(r"[0-9a-f]{40}", data["source_commit"]):
        raise ValueError("invalid candidate source commit")
    if type(data["run_id"]) is not int or data["run_id"] <= 0:
        raise ValueError("invalid candidate run ID")
    if set(data["assets"]) != names(value):
        raise ValueError("candidate must contain exactly the 18 release files")
    for name, asset in data["assets"].items():
        if set(asset) != {"sha256", "size"} or not re.fullmatch(r"[0-9a-f]{64}", asset["sha256"]):
            raise ValueError(f"invalid digest for {name}")
        if type(asset["size"]) is not int or asset["size"] <= 0:
            raise ValueError(f"invalid size for {name}")
    return data


def load():
    # Reject duplicate keys, not merely the value kept by json.loads.
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"duplicate metadata key: {key}")
            result[key] = value
        return result

    return validate(json.loads(LOCK.read_text(), object_pairs_hook=unique), version())


def check_tree(data):
    source = data["source_commit"]
    try:
        run("git", "cat-file", "-e", f"{source}^{{commit}}")
    except subprocess.CalledProcessError:
        # A squash merge may delete release-plz: its original build commit is
        # retained by the PR but is not an ancestor of the release tag.
        run("git", "fetch", "--no-tags", "origin", source)
        run("git", "cat-file", "-e", f"{source}^{{commit}}")
    changes = run("git", "diff", "--name-only", source, "HEAD").splitlines()
    if changes != [str(LOCK)]:
        raise ValueError("candidate source and release must differ ONLY in action/release.json")
    if run("git", "status", "--porcelain", "--untracked-files=no"):
        raise ValueError("release checkout has tracked modifications")


def files(directory, value):
    paths = {file.name: file for file in Path(directory).iterdir()}
    if set(paths) != names(value):
        raise ValueError("local release files do not match the required asset inventory")
    result = {}
    for name, file in sorted(paths.items()):
        if file.is_symlink() or not file.is_file() or file.stat().st_size <= 0:
            raise ValueError(f"invalid release file: {name}")
        with file.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        result[name] = {"sha256": digest, "size": file.stat().st_size}
    return result


def release(value):
    pages = json.loads(run("gh", "api", "--paginate", "--slurp", f"repos/{REPO}/releases?per_page=100"))
    matches = [item for page in pages for item in page if item["tag_name"] == f"v{value}"]
    if len(matches) > 1:
        raise ValueError("multiple releases for the candidate version")
    return matches[0] if matches else None


def check_assets(data, item, allow_bundle=False):
    assets = api(f"releases/{item['id']}/assets?per_page=100")
    if allow_bundle:
        # A failed promotion may already have uploaded its signed manifest.
        # Never use it as an input; the tag job signs/verifies a fresh bundle.
        assets = [asset for asset in assets if asset["name"] != "packslip.sigstore.json"]
    if len(assets) != len(data["assets"]) or {asset["name"] for asset in assets} != set(data["assets"]):
        raise ValueError("draft asset inventory mismatch")
    for asset in assets:
        expected = data["assets"][asset["name"]]
        if asset.get("digest") != f"sha256:{expected['sha256']}" or asset["size"] != expected["size"]:
            raise ValueError(f"draft asset digest/size mismatch: {asset['name']}")


def check_run(data):
    item = api(f"actions/runs/{data['run_id']}")
    workflow = api(f"actions/workflows/{item['workflow_id']}")
    if (item["head_sha"] != data["source_commit"] or item["event"] != "push"
            or item["head_branch"] != "release-plz" or item["conclusion"] != "success"
            or item["head_repository"]["full_name"] != REPO
            or workflow["path"] != ".github/workflows/release.yml"):
        raise ValueError("candidate build run is not a successful trusted release-plz push")


def check_remote(data, allow_published=False):
    check_run(data)
    item = release(data["version"])
    if not item or (not item["draft"] and not allow_published):
        raise ValueError("matching unpublished draft release is required")
    check_assets(data, item, allow_bundle=True)
    return item


def stage(directory):
    if os.environ.get("GITHUB_REPOSITORY") != REPO or os.environ.get("GITHUB_REF") != "refs/heads/release-plz":
        raise ValueError("staging is only allowed on the repository's release-plz push")
    source = os.environ["GITHUB_SHA"]
    value = version()
    data = validate({"schema": 1, "version": value, "source_commit": source,
                     "run_id": int(os.environ["GITHUB_RUN_ID"]), "assets": files(directory, value)}, value)
    if run("git", "rev-parse", "HEAD") != source:
        raise ValueError("candidate checkout is not the workflow source")
    remote = run("git", "ls-remote", "origin", "refs/heads/release-plz").split()
    if not remote or remote[0] != source:
        raise ValueError("release branch changed while candidate was building")
    if run("git", "ls-remote", "origin", f"refs/tags/v{value}"):
        raise ValueError("version tag already exists; never move an immutable release tag")
    item = release(value)
    if item and not item["draft"]:
        raise ValueError("cannot replace a published release")
    if not item:
        run("gh", "release", "create", f"v{value}", "-R", REPO, "--draft", "--target", source,
            "--title", f"v{value}", "--notes", "Candidate release; awaiting release PR review and verification.")
        item = release(value)
    else:
        api(f"releases/{item['id']}", "-X", "PATCH", "-f", f"target_commitish={source}")
    if run("git", "ls-remote", "origin", f"refs/tags/v{value}"):
        raise ValueError("draft creation unexpectedly created a version tag; publication held")
    # Only a draft is replaceable. Delete stale extras rather than accepting them.
    for asset in api(f"releases/{item['id']}/assets?per_page=100"):
        if asset["name"] not in data["assets"]:
            run("gh", "api", "-X", "DELETE", f"repos/{REPO}/releases/assets/{asset['id']}")
    run("gh", "release", "upload", f"v{value}", "-R", REPO, "--clobber",
        *(str(Path(directory) / name) for name in sorted(data["assets"])))
    check_assets(data, item)
    LOCK.parent.mkdir(exist_ok=True)
    LOCK.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")


def download(directory):
    data = load()
    check_tree(data)
    head = check_tag(data)
    item = check_remote(data, allow_published=True)
    if not item["draft"] and item["target_commitish"] != head:
        raise ValueError("published release target does not match the immutable version tag")
    target = Path(directory)
    target.mkdir(exist_ok=True)
    if any(target.iterdir()):
        raise ValueError("download directory must be empty")
    patterns = [argument for name in sorted(data["assets"]) for argument in ("-p", name)]
    run("gh", "release", "download", f"v{data['version']}", "-R", REPO, "-D", str(target), *patterns)
    if files(target, data["version"]) != data["assets"]:
        raise ValueError("downloaded candidate bytes differ from the committed digests")
    for name in sorted(data["assets"]):
        run("gh", "attestation", "verify", str(target / name), "-R", REPO,
            "--source-digest", data["source_commit"], "--source-ref", "refs/heads/release-plz",
            "--signer-workflow", WORKFLOW, "--signer-digest", data["source_commit"],
            "--deny-self-hosted-runners")
    if item["draft"]:
        api(f"releases/{item['id']}", "-X", "PATCH", "-f", f"target_commitish={head}")
    with open(os.environ["GITHUB_ENV"], "a") as output:
        output.write(f"CANDIDATE_SOURCE={data['source_commit']}\n")
        output.write(f"CANDIDATE_PUBLISHED={str(not item['draft']).lower()}\n")


def check_tag(data):
    tag = f"v{data['version']}"
    if os.environ.get("GITHUB_REF") != f"refs/tags/{tag}":
        raise ValueError("promotion requires the matching version-tag event")
    head = run("git", "rev-parse", "HEAD")
    if run("git", "rev-list", "-n1", tag) != head:
        raise ValueError("local version tag does not identify the release checkout")
    remote = dict((ref, sha) for sha, ref in (line.split() for line in
        run("git", "ls-remote", "origin", f"refs/tags/{tag}", f"refs/tags/{tag}^{{}}").splitlines()))
    if remote.get(f"refs/tags/{tag}^{{}}", remote.get(f"refs/tags/{tag}")) != head:
        raise ValueError("remote version tag does not identify the release checkout")
    return head


def finish(directory):
    """Complete a missed major-tag update without overwriting release assets."""
    import base64

    data = load()
    check_tree(data)
    head = check_tag(data)
    item = check_remote(data, allow_published=True)
    if item["draft"] or item["target_commitish"] != head or files(directory, data["version"]) != data["assets"]:
        raise ValueError("major tag requires the verified, published release and its original bytes")
    major = f"v{data['version'].split('.')[0]}"
    refs = api("git/matching-refs/tags/" + major)
    match = [ref for ref in refs if ref["ref"] == "refs/tags/" + major]
    if match:
        current = match[0]["object"]
        while current["type"] == "tag":
            current = api("git/tags/" + current["sha"])["object"]
        if current["type"] != "commit":
            raise ValueError("major tag does not resolve to a commit")
        if current["sha"] == head:
            return
        manifest = api("contents/Cargo.toml?ref=" + current["sha"])
        old = tomllib.loads(base64.b64decode(manifest["content"]).decode())["package"]["version"]
        if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", old):
            raise ValueError("major tag has an unsupported version")
        if tuple(map(int, old.split("."))) > tuple(map(int, data["version"].split("."))):
            print("major tag already tracks a newer release; leaving it unchanged")
            return
        if old == data["version"]:
            raise ValueError("major tag has this version at a different commit")
        api("git/refs/tags/" + major, "-X", "PATCH", "-f", f"sha={head}", "-F", "force=true")
    else:
        api("git/refs", "-X", "POST", "-f", "ref=refs/tags/" + major, "-f", f"sha={head}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("mode", "check", "stage", "download", "gate", "finish"))
    parser.add_argument("--directory", default="dist")
    parser.add_argument("--remote", action="store_true")
    parser.add_argument("--build-run", action="store_true")
    args = parser.parse_args()
    if args.command == "mode":
        mode = "build"
        if LOCK.exists():
            try:
                check_tree(load())
                mode = "ready"
            except (ValueError, subprocess.CalledProcessError):
                pass
        print(mode)
    elif args.command == "stage":
        stage(args.directory)
    elif args.command == "download":
        download(args.directory)
    elif args.command == "finish":
        finish(args.directory)
    elif args.command == "gate":
        latest = run("gh", "release", "view", "-R", REPO, "--json", "tagName", "--jq", ".tagName")
        if latest != f"v{version()}":
            data = load()
            check_tree(data)
            check_remote(data)
    else:
        data = load()
        check_tree(data)
        if args.remote:
            check_remote(data)
        elif args.build_run:
            check_run(data)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, TypeError, FileNotFoundError, subprocess.CalledProcessError) as error:
        sys.exit(f"release candidate rejected: {error}")
