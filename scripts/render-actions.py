#!/usr/bin/env python3
"""Write content/docs/actions.md from docs/actions.md and the actions' metadata.

docs/actions.md is the hand-written page. Each marker line in it, such as
`<!-- inputs: action.yml -->` or `<!-- outputs: releases/action.yml -->`, is
replaced by a table generated from that action file's inputs or outputs.
docs/actions-since.toml gives the release that added each one.
"""

import argparse
from pathlib import Path
import re
import sys

# Keep scripts/__pycache__ out of the tree: CI fails on untracked files.
sys.dont_write_bytecode = True
import minitoml  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
MARKER = re.compile(r"<!-- (inputs|outputs): ([A-Za-z0-9_./-]+) -->")
KEY = re.compile(r"([A-Za-z0-9_-]+):(?:[ \t]+(.*))?")
# A folded or literal block header, such as >, |-, or >2, with an optional comment.
BLOCK = re.compile(r"[>|](?:[1-9]?[+-]?|[+-][1-9])(?:[ \t]+#.*)?")
# The fields the tables show. Every other field, such as an output's value,
# is skipped without being read, so new metadata in an action file never
# breaks the render.
USED = {"inputs": ("description", "required", "default", "deprecationMessage"), "outputs": ("description",)}
VERSION = re.compile(r"\d+\.\d+\.\d+")


def show(path):
    """Name a file in a message: relative to the repository when inside it."""
    return path.relative_to(ROOT) if ROOT in path.parents else path


def uncomment(text):
    """Drop a comment: a # at the start or after a space ends a plain line."""
    return re.sub(r"(?:^|[ \t])#.*", "", text).strip()


def quoted(text, where):
    """Read a one-line quoted YAML scalar, with an optional trailing comment."""
    comment = r"(?:[ \t]+#.*)?[ \t]*"
    if text.startswith('"'):
        match = re.fullmatch(rf'"((?:[^"\\]|\\["\\/nt])*)"{comment}', text)
        if not match:
            raise ValueError(f"{where}: unsupported double-quoted string {text}")
        return re.sub(r"\\(.)", lambda m: {"n": "\n", "t": "\t"}.get(m[1], m[1]), match[1])
    match = re.fullmatch(rf"'((?:[^']|'')*)'{comment}", text)
    if not match:
        raise ValueError(f"{where}: unsupported single-quoted string {text}")
    return match[1].replace("''", "'")


def field(value, block, where):
    """Read a field from the text after its key and the lines that continue it.

    The value is a quoted scalar on one line, a plain scalar that more-indented
    lines may continue, or a folded or literal block.
    """
    while block and not block[-1].strip():
        block.pop()
    if value and BLOCK.fullmatch(value):
        body = [text.strip() for text in block]
        if value.startswith(">"):
            # Folding joins lines with spaces and keeps blank lines as breaks.
            return re.sub(r"(?<!\n)\n(?!\n)", " ", "\n".join(body)).strip()
        return "\n".join(body).strip()
    if value and value[0] in "\"'":
        if any(text.strip() for text in block):
            raise ValueError(f"{where}: multi-line quoted strings are unsupported")
        return quoted(value, where)
    text = " ".join(filter(None, (uncomment(line) for line in [value or ""] + block)))
    # A plain scalar cannot start with an indicator or hold ": ".
    if re.match(r"[\[\]{},&*!|>'\"%@`]|[-?:](?:\s|$)", text) or re.search(r":(?:\s|$)", text):
        raise ValueError(f"{where}: unsupported plain scalar {text}")
    return text


def read_action(path):
    """Return {"inputs": {...}, "outputs": {...}} from an action file.

    This reads only what the tables show: the description, required,
    default, and deprecationMessage fields under the top-level `inputs:` and
    `outputs:` mappings, at the two-space indentation the action files use.
    Other top-level keys, and other fields with whatever values they have,
    are skipped.
    """
    lines = path.read_text().splitlines()
    label = show(path)
    sections = {}
    kind = section = entry = None
    i = 0
    while i < len(lines):
        line = lines[i]
        where = f"{label}:{i + 1}"
        i += 1
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        indent = len(line) - len(line.lstrip(" "))
        if indent == 0:
            kind = line.split(":", 1)[0]
            section = sections.setdefault(kind, {}) if kind in USED else None
            entry = None
            continue
        if section is None:
            continue
        match = KEY.fullmatch(line.strip())
        value = match[2] if match else None
        if value is not None and value.startswith("#"):
            value = None
        if indent == 2:
            if not match or value is not None:
                raise ValueError(f"{where}: expected NAME: on a line of its own, then its fields")
            if match[1] in section:
                raise ValueError(f"{where}: duplicate {match[1]}")
            entry = section[match[1]] = {}
            continue
        if indent != 4 or entry is None:
            raise ValueError(f"{where}: expected a field of an entry in {kind}, indented four spaces")
        # Gather the lines that continue this field: blank or more indented.
        block = []
        while i < len(lines) and (not lines[i].strip() or lines[i].startswith(" " * 5)):
            block.append(lines[i])
            i += 1
        if not match or match[1] not in USED[kind]:
            continue
        if match[1] in entry:
            raise ValueError(f"{where}: duplicate field {match[1]}")
        entry[match[1]] = field(value, block, where)
    return sections


def escape(text):
    """Escape prose for a Markdown table cell, leaving code spans and URLs."""
    parts = []
    for index, part in enumerate(re.split(r"(`[^`]*`)", text)):
        if index % 2:
            parts.append(part.replace("|", "\\|"))
            continue
        for piece_index, piece in enumerate(re.split(r"(https?://[A-Za-z0-9][^\s<>|`]*)", part)):
            if piece_index % 2:
                parts.append(piece)
            else:
                parts.append(re.sub(r"([\\*_\[\]<>|&~])", r"\\\1", piece))
    return " ".join("".join(parts).split())


def code(value):
    fence = "``" if "`" in value else "`"
    pad = " " if "`" in value else ""
    return f"{fence}{pad}{value.replace('|', chr(92) + '|')}{pad}{fence}"


def table(kind, action, entries, since, where):
    missing = [name for name in entries if name not in since]
    stale = [name for name in since if name not in entries]
    if missing or stale:
        problems = [f"no release for {name}" for name in missing] + [f"{name} no longer exists" for name in stale]
        raise ValueError(f"{where}: " + "; ".join(problems))
    for name, version in since.items():
        if not isinstance(version, str) or not VERSION.fullmatch(version):
            raise ValueError(f"{where}: {name}: expected a release version such as 1.2.3, not {version!r}")
    # The short columns come before the description, so a narrow screen
    # shows them before it has to scroll.
    if kind == "inputs":
        rows = ["| Input | Default | Since | Description |", "| --- | --- | --- | --- |"]
    else:
        rows = ["| Output | Since | Description |", "| --- | --- | --- |"]
    for name, fields in entries.items():
        cells = [code(name)]
        if kind == "inputs":
            required = fields.get("required", "false")
            if required not in ("true", "false"):
                raise ValueError(f"{action}: {name}: required must be true or false")
            default = fields.get("default", "")
            if "\n" in default:
                raise ValueError(f"{action}: {name}: a multi-line default cannot go in a table cell")
            cells.append("Required" if required == "true" else code(default) if default else "")
        description = fields.get("description", "")
        message = fields.get("deprecationMessage", "").strip()
        if message:
            # GitHub warns a workflow that sets a deprecated input; say so first.
            message += "" if message[-1] in ".!?" else "."
            description = f"Deprecated: {message} {description}"
        cells += [since[name], escape(description)]
        rows.append("| " + " | ".join(cells) + " |")
    # static/style.css styles the tables by these classes.
    rows.append("{.action-table .action-inputs}" if kind == "inputs" else "{.action-table}")
    return "\n".join(rows)


def render(source, since_path):
    text = source.read_text()
    since = minitoml.load(since_path)
    if not text.startswith("---\n") or "\n---\n" not in text[4:]:
        raise ValueError(f"{show(source)}: expected front matter")
    end = text.index("\n---\n", 4) + 5
    used = set()
    actions = {}

    def replace(match):
        kind, name = match[1], match[2]
        path = (ROOT / name).resolve()
        if ROOT not in path.parents or path.name not in ("action.yml", "action.yaml") or not path.is_file():
            raise ValueError(f"{show(source)}: {match[0]} names no action file in this repository")
        if (name, kind) in used:
            raise ValueError(f"{show(source)}: {match[0]} appears twice")
        used.add((name, kind))
        if name not in actions:
            actions[name] = read_action(path)
        where = f'{show(since_path)} ["{name}".{kind}]'
        return table(kind, name, actions[name].get(kind, {}), since.get(name, {}).get(kind, {}), where)

    body = re.sub(rf"^{MARKER.pattern}$", replace, text[end:], flags=re.M)
    # A marker that is not alone on its line would vanish as a comment.
    stray = re.search(r"<!--\s*(?:inputs|outputs)\s*:.*", body)
    if stray:
        raise ValueError(f"{show(source)}: put {stray[0]} on a line of its own, exactly as <!-- inputs: action.yml -->")
    unused = [f'"{name}".{kind}' for name, kinds in since.items() for kind in kinds if (name, kind) not in used]
    if unused:
        raise ValueError(f"{show(since_path)}: no marker in {show(source)} uses " + ", ".join(unused))
    notice = ("<!-- Generated by scripts/render-actions.py from docs/actions.md, the action.yml files,"
              " and docs/actions-since.toml. Edit those and run `mise run render`. -->\n\n")
    return text[:end] + notice + body


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=ROOT / "docs/actions.md")
    parser.add_argument("--since", type=Path, default=ROOT / "docs/actions-since.toml")
    parser.add_argument("--out", type=Path, default=ROOT / "content/docs/actions.md")
    args = parser.parse_args()
    args.out.write_text(render(args.source.resolve(), args.since.resolve()))


if __name__ == "__main__":
    main()
