---
title: Host requirements
weight: 34
group: publish
description: Declare the libraries, commands, and operating-system versions your release needs.
---
# Host requirements

Host requirements help consumers explain missing dependencies before a
user tries to run the tool. They describe what must already be available;
they do not tell a package manager what to install.

Each artifact's `requires` can hold four fields. `packslip create` reads
`libs` from the executables where it can, and you declare the others:

| Field | What it states | How to set it |
| --- | --- | --- |
| `libs` | Shared libraries the executables load from the host, by loader name (`libssl.so.3`) | Read by `create` (`--no-libs` turns this off), or TOML `requires.libs` for an artifact `create` cannot read |
| `bin` | Commands the executables run, with an optional minimum version | `--require bin:java@17`, a line in the action's `require` input, or TOML `requires.bin` |
| `glibc_min` | Minimum glibc for a `gnu` Linux build | TOML `requires` on the artifact |
| `os_min` | Minimum OS version, in the OS's own numbering | TOML `requires` on the artifact |

Requirements use loader or command names, such as `libssl.so.3` and
`java`, rather than distribution package names. Consumers decide how to
resolve them.

## Let `create` read shared libraries

`create` reads the ELF, Mach-O, or PE executables named by `bin` out of
each artifact and records the shared libraries they load from the host
as `requires.libs`. It leaves out the C runtime, libraries the OS always
provides, and any library the artifact ships itself. An empty list means
the executables were read and need nothing more. No list means nothing
was read, for one of these reasons: the artifact is an installer, disk
image, or 7z archive; its executables are scripts; it lists no `bin`; or
you passed `--no-libs`.

For an artifact `create` cannot read, you may write `libs` in its TOML
`requires`. When `create` can read the executables, a list you write
must match what it reads, or `create` fails.

`--no-libs` skips reading the executables, which also turns off libc
detection: a Linux artifact whose name does not say `musl` or `gnu` is
then recorded as `gnu`. Set `libc` explicitly for a static or musl build,
as [Artifact configuration](/docs/describing-releases/#check-inferred-metadata)
shows.

## Declare required commands

Declare a command the executables run with `--require bin:NAME[@MIN]`,
or with one line per command in the action's `require` input.
`--require bin:java@17` adds `java` 17 or later to every artifact that
has executables; `create` fails if neither `--bin` nor the TOML manifest
names an executable. In TOML, list the command in an artifact's
`requires` instead: `requires = { bin = [{ name = "java", min = "17" }] }`.

`NAME` is the bare command as the program runs it, with no directory and
no `.exe`. `MIN` is the lowest version that works, written as
dot-separated numbers such as `17` or `3.12`. `create` rejects a minimum
that does not start with a digit, such as `v17`, and refuses a command
the release itself provides.

Only declare commands the software cannot work without; put optional
integrations in [extensions](/release/v1/#extensions).

## Set minimum OS and glibc versions

`create` does not detect minimum OS or glibc versions, and no flag sets
them, so write them in each artifact's TOML `requires`. `os_min` uses the
OS's own version numbers, such as `12` for macOS Monterey or `10.0.17763`
for Windows. `glibc_min`, such as `2.31`, applies only to a `gnu` Linux
build. Keep both out of the top-level `requires`: every artifact without
its own table inherits it, so a top-level `glibc_min` would also be
recorded on your macOS and Windows artifacts.

## Combine defaults and per-artifact requirements

A top-level `requires` table is the default for every artifact without
its own, including artifacts given only on the command line. An
artifact's own `requires` replaces the default rather than merging with
it, so it must repeat anything shared. Here the Windows build takes `java`
from the default, and the Linux and macOS builds, which need tables of
their own, each list it again:

```toml
requires = { bin = [{ name = "java", min = "17" }] }

[[artifact]]
path = "dist/mytool-1.2.3-linux-x64.tar.gz"
bin = ["mytool"]
requires = { glibc_min = "2.31", bin = [{ name = "java", min = "17" }] }

[[artifact]]
path = "dist/mytool-1.2.3-darwin-arm64.tar.gz"
bin = ["mytool"]
requires = { os_min = "12", bin = [{ name = "java", min = "17" }] }

[[artifact]]
path = "dist/mytool-1.2.3-windows-x64.zip"
bin = ["mytool"]
```

Two things are added on top of whichever table applies: commands from
`--require`, and the libraries `create` reads. You could therefore drop
every `java` entry above and pass `--require bin:java@17` instead.
`create` refuses a command that `--require` and `requires.bin` both name
with different minimums.

## What consumers do when a requirement is not met

Consumers check requirements only after choosing an artifact, so a
requirement the host does not meet never steers them to another build.
Instead, the consumer refuses the install or warns:

| Requirement | If the host does not meet it |
| --- | --- |
| Shared library, minimum glibc, or minimum OS version | Refuse installation because the executable cannot start; a user may override the refusal. |
| Required command or its minimum version | Install with a warning naming the missing or outdated command. |
| A requirement the consumer cannot check | Warn instead of guessing. |

Numeric versions compare by component: `2.10` is newer than `2.9`, and
`17` is equivalent to `17.0.0`. If either version cannot be compared this
way, the check is unknown and the consumer warns.

See [Host requirements](/release/v1/#host-requirements) and
[consumer rule 7](/release/v1/#consumer-rules) in the specification for
the complete rules, and [Release recipes](/docs/recipes/) for example
layouts.
