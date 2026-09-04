---
title: Host requirements
weight: 34
description: Declare the libraries, commands, and operating-system versions your release needs.
---
# Host requirements

Host requirements help consumers explain missing dependencies before a
user tries to run the tool. They describe what must already be available;
they do not tell a package manager what to install.

## Detect shared libraries

`create` reads shared-library dependencies from supported executables and
records them as `requires.libs`, excluding baseline system libraries and
libraries supplied by the artifact itself. `--no-libs` disables this scan.
An empty list means the scan found no additional libraries; an absent
list means no result is available.

Declare commands the program needs with `--require bin:java@17`.
Use per-artifact TOML `requires` for `os_min` and `glibc_min`; a Linux
glibc requirement should not become a Windows default. Requirements use
loader or command names, such as `libssl.so.3` and `java`, rather than
distribution package names. Consumers decide how to resolve them.

## Set requirements per artifact

Keep platform-specific requirements on the artifact they describe:

```toml
[[artifact]]
path = "dist/mytool-1.2.3-linux-x64.tar.gz"
bin = ["mytool"]
requires = { glibc_min = "2.31", bin = [{ name = "java", min = "17" }] }

[[artifact]]
path = "dist/mytool-1.2.3-darwin-arm64.tar.gz"
bin = ["mytool"]
requires = { os_min = "12", bin = [{ name = "java", min = "17" }] }
```

Top-level `requires` supplies defaults. An artifact's own `requires`
replaces the default object, so repeat any shared requirements it still
needs. Library scanning supplies detected library requirements unless
`--no-libs` disables it.

Only declare commands the software needs to work. Optional integrations
belong in extensions; executables shipped by the release do not belong
in `requires.bin`.

## Understand consumer behavior

| Requirement | If the host does not meet it |
| --- | --- |
| Shared library, minimum glibc, or minimum OS version | Refuse installation because the executable cannot start; a user may override the refusal. |
| Required command or its minimum version | Install with a warning naming the missing or outdated command. |
| A requirement the consumer cannot check | Warn instead of guessing. |

Consumers select an artifact first, then check its requirements.
Requirements do not resolve a selection tie or silently choose another
build. Numeric versions compare by component: `2.10` is newer than `2.9`,
and `17` is equivalent to `17.0.0`. Unrecognized version formats produce
an unknown result.

See the [specification](/release/v1/#host-requirements) for the complete
rules and [release recipes](/docs/recipes/) for example layouts.
