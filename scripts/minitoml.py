"""Read the small TOML subset that the docs data files use.

The documentation scripts run on Python 3.9, which has no tomllib. The data
files they read need only comments, table headers made of bare or
double-quoted keys joined by dots, and `key = "string"` pairs. Anything else
is an error, so a file that Hugo would read differently fails loudly instead
of being misread.
"""

import json
import re

_KEY = r'(?:[A-Za-z0-9_-]+|"(?:[^"\\\n]|\\.)*")'
_STRING = r'"(?:[^"\\\n]|\\.)*"'
_LINE = re.compile(
    rf"(?:\[\s*(?P<table>{_KEY}(?:\s*\.\s*{_KEY})*)\s*\]"
    rf"|(?P<key>{_KEY})\s*=\s*(?P<value>{_STRING}))"
    r"\s*(?:#.*)?"
)


def _key(text):
    return json.loads(text) if text.startswith('"') else text


def load(path):
    """Return the file's tables as nested dicts of strings."""
    data = {}
    table = data
    defined = set()
    for number, line in enumerate(path.read_text().splitlines(), 1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        match = _LINE.fullmatch(stripped)
        if not match:
            raise ValueError(f"{path}:{number}: unsupported TOML; use [table] headers and key = \"string\" pairs")
        if match["table"]:
            keys = tuple(_key(part) for part in re.findall(_KEY, match["table"]))
            if keys in defined:
                raise ValueError(f"{path}:{number}: table {match['table']} is defined twice")
            defined.add(keys)
            table = data
            for key in keys:
                table = table.setdefault(key, {})
                if not isinstance(table, dict):
                    raise ValueError(f"{path}:{number}: {key} is a value, not a table")
        else:
            key = _key(match["key"])
            if key in table:
                raise ValueError(f"{path}:{number}: duplicate key {key}")
            table[key] = json.loads(match["value"])
    return data
