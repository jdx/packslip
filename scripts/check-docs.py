#!/usr/bin/env python3
"""Check built local links and run the marked documentation examples offline."""

import argparse
from html.parser import HTMLParser
import io
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile
from urllib.parse import unquote, urljoin, urlsplit
import zipfile

ROOT = Path(__file__).resolve().parent.parent


class Page(HTMLParser):
    def __init__(self, path):
        super().__init__()
        self.ids = set()
        self.references = []
        self.duplicates = []
        self.headings = 0
        # An alias page is a redirect stub: a canonical link and a meta
        # refresh, with no content of its own to check.
        self.redirect = False
        self.feed(path.read_text())

    def handle_starttag(self, tag, attributes):
        attrs = dict(attributes)
        if tag == "meta" and attrs.get("http-equiv", "").lower() == "refresh":
            self.redirect = True
        if "id" in attrs:
            if attrs["id"] in self.ids:
                self.duplicates.append(attrs["id"])
            self.ids.add(attrs["id"])
        if tag == "h1":
            self.headings += 1
        for attr in ("href", "src"):
            if attrs.get(attr):
                self.references.append(attrs[attr])


def check_links(site):
    site = site.resolve()
    pages = {path.resolve(): Page(path) for path in site.rglob("*.html")}
    # Both predicate types are published URLs and must resolve;
    # releases/v1 is an alias onto the specification page.
    required = ["index.html", "release/v1/index.html", "releases/v1/index.html",
                "docs/index.html", "cli/index.html"]
    required += [f"docs/{name}/index.html" for name in (
        "release-workflow", "getting-started", "describing-releases", "resources", "host-requirements",
        "recipes", "publishing", "verifying", "release-lists", "mise",
    )]
    errors = [f"missing page: {name}" for name in required if not (site / name).is_file()]
    count = 0
    for path, page in pages.items():
        if page.headings != 1 and not page.redirect:
            errors.append(f"{path}: expected one H1, found {page.headings}")
        for identifier in page.duplicates:
            errors.append(f"{path}: duplicate id {identifier}")
        route = path.relative_to(site).as_posix().removesuffix("index.html")
        for ref in page.references:
            url = urlsplit(urljoin(f"https://packslip.dev/{route}", ref))
            if url.scheme not in ("https", "http") or url.netloc != "packslip.dev":
                continue
            target = site / unquote(url.path.lstrip("/"))
            if url.path.endswith("/"):
                target /= "index.html"
            target = target.resolve()
            count += 1
            if not target.is_file():
                errors.append(f"{path.relative_to(site)}: missing target {ref}")
            elif url.fragment and target in pages and unquote(url.fragment) not in pages[target].ids:
                errors.append(f"{path.relative_to(site)}: missing anchor {ref}")
    if errors:
        raise RuntimeError("\n".join(errors))
    print(f"Checked {len(pages)} pages and {count} local links, anchors, and assets.")


def run(args, cwd, env, success=True):
    result = subprocess.run(args, cwd=cwd, env=env, capture_output=True, text=True, timeout=60)
    if (result.returncode == 0) != success:
        raise RuntimeError(f"Unexpected exit {result.returncode}: {args}\n{result.stdout}{result.stderr}")
    return result.stdout


def check_excerpt(example, actual, path="statement"):
    if isinstance(example, dict):
        for key, value in example.items():
            check_excerpt(value, actual[key], f"{path}.{key}")
    elif isinstance(example, list):
        assert len(example) == len(actual), path
        for index, value in enumerate(example):
            check_excerpt(value, actual[index], f"{path}[{index}]")
    elif example == "...":
        assert re.fullmatch(r"[0-9a-f]{64}", actual), path
    else:
        assert example == actual, f"{path}: example {example!r} differs from {actual!r}"


def check_quickstart(binary):
    source = (ROOT / "content/docs/getting-started.md").read_text()
    blocks = re.findall(r"<!-- docs-test: quickstart -->\s*```sh\n(.*?)```", source, re.S)
    if len(blocks) != 4:
        raise RuntimeError("Expected the four marked quickstart blocks: setup, create, verify, show")
    with tempfile.TemporaryDirectory(prefix="packslip-docs-") as directory:
        cwd = Path(directory)
        env = os.environ.copy()
        env["PATH"] = str(binary.parent) + os.pathsep + env["PATH"]
        for block in blocks:
            run(["sh", "-eu", "-c", block], cwd, env)
        statement = json.loads(run([str(binary), "show", "dist/packslip.sigstore.json"], cwd, env))
        check_excerpt(json.loads((ROOT / "docs/examples/release-excerpt.json").read_text()), statement)
        artifact = statement["predicate"]["artifacts"][0]
        assert statement["predicate"]["project"] == "mytool.example.com"
        assert statement["predicate"]["version"] == "1.2.3"
        assert artifact["bin"] == ["bin/mytool"]
        assert artifact["format"] == "tar.gz"
        assert not any(field in artifact for field in ("os", "arch", "libc"))
        assert len(statement["subject"][0]["digest"]["sha256"]) == 64
        # A green walkthrough must not hide a verifier that accepts modified files.
        with (cwd / "dist/mytool-1.2.3.tar.gz").open("ab") as file:
            file.write(b"tampered")
        run([str(binary), "verify", "dist/packslip.sigstore.json", "--pubkey", "release.pub",
             "--allow-unlogged", "--artifact", "dist/mytool-1.2.3.tar.gz"], cwd, env, success=False)
    print("Quickstart passed, including manifest fields and rejection of a modified artifact.")


# Small fixtures test the documented layouts and manifests, not Rust/Go builds
# or platform signing. Nothing in these archives is executed.
RECIPE_FILES = {
    "rust": {"mytool-1.2.3-linux-x64.tar.gz": [
        "mytool-1.2.3/bin/mytool", "mytool-1.2.3/share/zsh/site-functions/_mytool",
        "mytool-1.2.3/share/man/man1/mytool.1",
    ]},
    "go": {"mytool-1.2.3-linux-x64.tar.gz": ["mytool"]},
    "monorepo": {"lint-1.2.3-linux-x64.tar.gz": ["bin/lint-x86_64"]},
    "desktop": {
        "myapp-1.2.3-linux-x64.tar.gz": ["bin/myapp", "share/applications/myapp.desktop",
                                            "share/icons/hicolor/256x256/apps/myapp.png"],
        "myapp-1.2.3-darwin-arm64.zip": ["MyApp.app/Contents/Info.plist"],
    },
}


def check_recipes(binary):
    source = (ROOT / "content/docs/recipes.md").read_text()
    recipes = re.findall(r"<!-- docs-test: recipe (\w+) -->\s*```toml\n(.*?)```", source, re.S)
    if {name for name, _ in recipes} != RECIPE_FILES.keys():
        raise RuntimeError("Recipe markers and fixture layouts must match")
    for name, manifest in recipes:
        with tempfile.TemporaryDirectory(prefix=f"packslip-recipe-{name}-") as directory:
            cwd = Path(directory)
            (cwd / "dist").mkdir()
            for filename, members in RECIPE_FILES[name].items():
                path = cwd / "dist" / filename
                content = b"#!/bin/sh\necho example\n"
                if filename.endswith(".zip"):
                    with zipfile.ZipFile(path, "w") as archive:
                        for member in members:
                            archive.writestr(member, content)
                else:
                    with tarfile.open(path, "w:gz") as archive:
                        for member in members:
                            entry = tarfile.TarInfo(member)
                            entry.size = len(content)
                            entry.mode = 0o755
                            archive.addfile(entry, io.BytesIO(content))
            (cwd / "release.toml").write_text(manifest)
            run([str(binary), "keygen", "--out", "release.key"], cwd, None)
            run([str(binary), "create", "--manifest", "release.toml", "--out", "out",
                 "--key", "release.key", "--no-log"], cwd, None)
            bundle, = (cwd / "out").glob("packslip*.sigstore.json")
            args = [str(binary), "verify", str(bundle), "--pubkey", "release.pub", "--allow-unlogged"]
            for filename in RECIPE_FILES[name]:
                args += ["--artifact", f"dist/{filename}"]
            run(args, cwd, None)
    print(f"Checked {len(recipes)} recipe manifests against sample archives.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--site", type=Path, default=ROOT / "public")
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/packslip")
    args = parser.parse_args()
    check_links(args.site.resolve())
    binary = args.binary.resolve()
    check_quickstart(binary)
    check_recipes(binary)


if __name__ == "__main__":
    main()
