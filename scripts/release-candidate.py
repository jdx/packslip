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
import time
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


def files(directory, value, subset=None):
    paths = {file.name: file for file in Path(directory).iterdir()}
    if set(paths) != (names(value) if subset is None else subset):
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
    matches = [item for item in releases() if item["tag_name"] == f"v{value}"]
    if len(matches) > 1:
        raise ValueError("multiple releases for the candidate version")
    return matches[0] if matches else None


def releases():
    pages = json.loads(run("gh", "api", "--paginate", "--slurp", f"repos/{REPO}/releases?per_page=100"))
    return [item for page in pages for item in page]


def freshness(tag):
    if not re.fullmatch(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", tag):
        raise ValueError("freshness requires a stable version tag")
    wanted = tuple(map(int, tag[1:].split(".")))
    stable = [tuple(map(int, item["tag_name"][1:].split("."))) for item in releases()
              if not item["draft"] and not item.get("prerelease", False)
              and re.fullmatch(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", item["tag_name"])]
    # GitHub's mutable latest flag is not a monotonic version ordering.
    return (not any(value[0] == wanted[0] and value > wanted for value in stable),
            not any(value > wanted for value in stable))


def output_freshness(tag):
    major, latest = freshness(tag)
    values = f"CURRENT_MAJOR={str(major).lower()}\nCURRENT_LATEST={str(latest).lower()}\n"
    if os.environ.get("GITHUB_ENV"):
        with open(os.environ["GITHUB_ENV"], "a") as output:
            output.write(values)
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a") as output:
            output.write(f"current-major={str(major).lower()}\ncurrent-latest={str(latest).lower()}\n")
    print(values, end="")


def publish():
    data = load()
    check_tree(data)
    check_tag(data)
    item = check_remote(data)
    _, latest = freshness(f"v{data['version']}")
    api(f"releases/{item['id']}", "-X", "PATCH", "-F", "draft=false",
        "-f", f"make_latest={str(latest).lower()}")


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


def wait_for_staged_assets(data, item):
    # GitHub metadata reads can lag successful writes. Never commit a map
    # until the exact inventory, sizes and SHA-256 values have converged.
    for attempt in range(30):
        try:
            check_assets(data, item)
            return
        except ValueError:
            if attempt == 29:
                raise
            time.sleep(1)


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
        # Use the creation response, not a release-list read that may still
        # hide the draft immediately after successful creation.
        item = api("releases", "-X", "POST", "-f", f"tag_name=v{value}", "-F", "draft=true",
                   "-f", f"target_commitish={source}", "-f", f"name=v{value}",
                   "-f", "body=Candidate release; awaiting release PR review and verification.")
    else:
        # Retargeting a draft without tag_name can reset it to untagged-*.
        # Keep the proposed tag explicit without creating that Git ref.
        item = api(f"releases/{item['id']}", "-X", "PATCH", "-f", f"target_commitish={source}",
                   "-f", f"tag_name=v{value}")
    if (not item["draft"] or item["tag_name"] != f"v{value}"
            or item["target_commitish"] != source):
        raise ValueError("draft response identity mismatch; staging held")
    if run("git", "ls-remote", "origin", f"refs/tags/v{value}"):
        raise ValueError("draft creation unexpectedly created a version tag; publication held")
    # Only a draft is replaceable. Replace its assets by ID, avoiding every
    # tag-based CLI discovery path while a new draft is not yet list-visible.
    for asset in api(f"releases/{item['id']}/assets?per_page=100"):
        run("gh", "api", "-X", "DELETE", f"repos/{REPO}/releases/assets/{asset['id']}")
    for name in sorted(data["assets"]):
        run("gh", "api", "-X", "POST", "--input", str(Path(directory) / name),
            "-H", "Content-Type: application/octet-stream",
            "-H", f"Content-Length: {data['assets'][name]['size']}",
            f"https://uploads.github.com/repos/{REPO}/releases/{item['id']}/assets?name={name}")
    wait_for_staged_assets(data, item)
    LOCK.parent.mkdir(exist_ok=True)
    LOCK.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")


def download(directory):
    data = load()
    check_tree(data)
    check_tag(data)
    item = check_remote(data, allow_published=True)
    target = Path(directory)
    target.mkdir(exist_ok=True)
    if any(target.iterdir()):
        raise ValueError("download directory must be empty")
    patterns = [argument for name in sorted(data["assets"]) for argument in ("-p", name)]
    run("gh", "release", "download", f"v{data['version']}", "-R", REPO, "-D", str(target), *patterns)
    if files(target, data["version"]) != data["assets"]:
        raise ValueError("downloaded candidate bytes differ from the committed digests")
    verify_attestations(data, target, set(data["assets"]))
    # target_commitish is ignored once the version tag exists. The actual
    # peeled local/remote tag above, not that release field, proves identity.
    with open(os.environ["GITHUB_ENV"], "a") as output:
        output.write(f"CANDIDATE_SOURCE={data['source_commit']}\n")
        output.write(f"CANDIDATE_PUBLISHED={str(not item['draft']).lower()}\n")


def verify_attestations(data, directory, selected):
    for name in sorted(selected):
        run("gh", "attestation", "verify", str(Path(directory) / name), "-R", REPO,
            "--source-digest", data["source_commit"], "--source-ref", "refs/heads/release-plz",
            "--signer-workflow", WORKFLOW, "--signer-digest", data["source_commit"],
            "--deny-self-hosted-runners")


def verify_image(directory):
    data = load()
    check_tree(data)
    check_tag(data)
    selected = {f"packslip-v{data['version']}-linux-{arch}" for arch in ("x64", "arm64")}
    if files(directory, data["version"], selected) != {name: data["assets"][name] for name in selected}:
        raise ValueError("image executables differ from the original candidate bytes")
    verify_attestations(data, directory, selected)


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
    if item["draft"] or files(directory, data["version"]) != data["assets"]:
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
            alias_outputs(data, True)
            return
        manifest = api("contents/Cargo.toml?ref=" + current["sha"])
        old = tomllib.loads(base64.b64decode(manifest["content"]).decode())["package"]["version"]
        if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", old):
            raise ValueError("major tag has an unsupported version")
        if tuple(map(int, old.split("."))) > tuple(map(int, data["version"].split("."))):
            print("major tag already tracks a newer release; leaving it unchanged")
            alias_outputs(data, False)
            return
        if old == data["version"]:
            raise ValueError("major tag has this version at a different commit")
        api("git/refs/tags/" + major, "-X", "PATCH", "-f", f"sha={head}", "-F", "force=true")
    else:
        api("git/refs", "-X", "POST", "-f", "ref=refs/tags/" + major, "-f", f"sha={head}")
    alias_outputs(data, True)


def alias_outputs(data, current_major):
    _, current_latest = freshness(f"v{data['version']}")
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a") as output:
            output.write(f"current-major={str(current_major).lower()}\n")
            output.write(f"current-latest={str(current_latest).lower()}\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("mode", "check", "stage", "download", "gate", "finish", "verify-image", "publish", "freshness"))
    parser.add_argument("--directory", default="dist")
    parser.add_argument("--remote", action="store_true")
    parser.add_argument("--build-run", action="store_true")
    parser.add_argument("--tag")
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
    elif args.command == "verify-image":
        verify_image(args.directory)
    elif args.command == "publish":
        publish()
    elif args.command == "freshness":
        output_freshness(args.tag)
    elif args.command == "gate":
        tag = f"v{version()}"
        # Existing tags mean no new publication is authorized. Do not merely
        # skip verification and continue into the publishing action.
        tagged = bool(run("git", "ls-remote", "origin", f"refs/tags/{tag}"))
        latest = run("gh", "release", "view", "-R", REPO, "--json", "tagName", "--jq", ".tagName")
        needed = not tagged and latest != tag
        if needed:
            data = load()
            check_tree(data)
            check_remote(data)
        if os.environ.get("GITHUB_OUTPUT"):
            with open(os.environ["GITHUB_OUTPUT"], "a") as output:
                output.write(f"publish={str(needed).lower()}\n")
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
