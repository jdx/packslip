---
title: Declare host requirements
description: Record the shared libraries, commands, and minimum OS and glibc versions each artifact needs, so consumers can check a host before installing.
group: publish
weight: 38
---
# Declare host requirements

For publishers: record what each artifact needs from the host. Consumers
can then catch a missing library or command, or an unsupported system
version, before a user runs the tool. They follow the rules in
[Host requirements](/release/v1/#host-requirements) in the specification.

Each artifact's `requires` can hold four fields:

- `libs`: shared libraries its executables load from the host.
- `bin`: commands they run.
- `glibc_min` and `os_min`: the oldest glibc and OS versions they support.

`packslip create` reads `libs` from the executables where it can; you
declare the others. `os_min` and `glibc_min` go in release.toml, passed
with `--manifest` (the action's `manifest` input); see
[Use release.toml](/docs/describing-releases/#use-a-toml-manifest) and the
[Requirement keys](/docs/release-toml/#requirement-keys) reference.

Requirements name what the host must provide, not packages to install;
each consumer decides how to resolve them. Write loader and command names
such as `libssl.so.3` and `java`, not `libssl3` or `openjdk-17-jre`.

## Let `create` read shared libraries

`packslip create` reads the ELF, Mach-O, or PE executables that `bin`
names in each artifact, including inside archives. It records the shared
libraries they load from the host as `requires.libs`, leaving out the C
runtime, libraries the OS always provides, and libraries the artifact
ships. It prints each artifact's requirements:

```text
  requires mytool-1.2.3-linux-x64.tar.gz: libs libcurl.so.4 libz.so.1
```

An empty list, printed as `libs none`, means the executables were read
and need no additional libraries. An absent list means no library
requirements were read. That happens when:

- the artifact is an installer or a 7z archive;
- its executables are scripts;
- it lists no `bin`; or
- `--no-libs` was passed.

`packslip install` then warns
`publisher did not declare shared-library requirements`, or
`publisher did not declare host requirements` for an artifact with no
`requires`.

Write `libs` in release.toml only for an artifact `create` cannot read.
For any other artifact, `create` fails if your list differs from what it
reads.

### Skip library reading

`--no-libs` stops `packslip create` from reading the executables for the
shared libraries and C library they load. `create` still finds each
`--bin` inside the archives, and records a `libs` list from release.toml
as written, unchecked. Without that reading, a Linux artifact whose file
name and `:OS/ARCH/LIBC` suffix name no C library is recorded as `gnu`.
That includes a static build, so set `libc` yourself:

- Static build: give the artifact as
  `dist/mytool-linux-x64.tar.gz:linux/x86_64/any`, or set `libc = "any"`
  in its `[[artifact]]` table.
- musl build: give it as `dist/mytool-linux-x64.tar.gz:linux/x86_64/musl`,
  or set `libc = "musl"`.

[C library](/docs/release-toml/#c-library) gives the full rule. The
GitHub action has no `--no-libs` input.

## Declare required commands

Pass `--require bin:NAME[@MIN]` once for each command your executables
need on PATH, or put one declaration per line in the action's `require`
input. This adds Java 17 or later to every artifact that has executables:

```yaml
- uses: jdx/packslip@v1
  with:
    artifacts: dist/*
    bin: mytool
    require: |
      bin:java@17
```

- `NAME` is the bare command as the program runs it, with no directory
  and no `.exe`.
- `MIN` is the lowest version that works, such as `17` or `3.12`; see
  [Write a version minimum](#write-a-version-minimum).

To require a command for one artifact only, list it in that artifact's
`requires.bin` in release.toml. An artifact's own table replaces the
default; see
[Combine defaults and per-artifact requirements](#combine-defaults-and-per-artifact-requirements).

Declare only commands the software cannot work without. Put optional
integrations in [extensions](/release/v1/#extensions).

## Set minimum OS and glibc versions

`packslip create` does not detect minimum OS or glibc versions, and no
flag sets them. Write them in each artifact's `requires` in release.toml:

| Field | Artifact | Version it names | Example |
| --- | --- | --- | --- |
| `os_min` | `os = "linux"` | The kernel version without local suffixes such as `-18-amd64`, never a distribution release | `"4.18"` |
| `os_min` | `os = "darwin"` | The macOS product version | `"12"` (Monterey) |
| `os_min` | `os = "windows"` | The Windows major, minor, and build numbers | `"10.0.17763"` |
| `glibc_min` | `libc = "gnu"` | The glibc version | `"2.31"` |

**Warning:** In a release with several platforms, keep `os_min` and
`glibc_min` out of the top-level `requires`. Artifacts without their own
table inherit it. A top-level `os_min = "12"` meant for macOS then makes
consumers refuse every Linux and Windows host, whose versions are below 12.
A top-level `glibc_min` makes macOS and Windows consumers warn that they
cannot check it. Set each minimum on its own artifact, as in
[Combine defaults and per-artifact requirements](#combine-defaults-and-per-artifact-requirements).

## Write a version minimum

Write plain dot-separated numbers, such as `17` or `2.31`, for a
command's `min`, `glibc_min`, and `os_min`. `packslip create` refuses a
minimum that does not start with a digit or holds anything but letters,
digits, `.`, `-`, and `+`, such as `v17` or `glibc-2.31`.

Consumers compare plain numbers component by component: `2.10` is newer
than `2.9`, and `17` accepts `17.0.1` and `21`. They cannot compare other
valid spellings, such as `17-ea`, and warn that the check is unknown.
[Comparing versions](/release/v1/#comparing-versions) gives the full rule.

## Combine defaults and per-artifact requirements

A top-level `requires` table is the default for every artifact without
its own, including artifacts given only on the command line. An
artifact's own table replaces the default instead of merging with it, so
it must repeat anything shared. Here the Windows build inherits `java`,
and the Linux and macOS builds repeat it:

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

`packslip create` adds `--require` commands to whichever table applies,
as it does the libraries it reads. You could drop every `java` entry
above and pass `--require bin:java@17` instead. A command named in both
places must have the same minimum.

[Release recipes](/docs/recipes/) has complete release.toml files to
start from.

## Plan for hosts that miss a requirement

Consumers check requirements only after choosing an artifact, so an
unmet one never steers them to another build. A missing library, or a
host below `os_min` or `glibc_min`, makes them refuse. A missing or
too-old command, or a check they cannot make, makes them warn; see
[Checking requirements before installation](/release/v1/#checking-requirements-before-installation).
[Override host checks](/docs/bootstrap/#override-host-checks) says which
checks `packslip install` makes and how to accept a refused host.

On a glibc host, consumers choose a `gnu` build over a static or `musl`
build for the same platform, even when the host is below its `glibc_min`.
[Selecting an artifact](/release/v1/#selecting-an-artifact) gives the
rule. To support older hosts, build the `gnu` artifact against the oldest
glibc you support and set `glibc_min` to that version, or ship a static
build in its place.

## Troubleshoot create errors

Each row starts with the exact error, shown with example names.

| Error | Fix |
| --- | --- |
| `--require says what the executables need; name them with --bin` | Name the executables with `--bin` or `bin` in release.toml. |
| `artifact "mytool-1.2.3-linux-x64.tar.gz" requires command "java" with minimum "v17"; a minimum starts with a digit, as in 17 or 3.12` | Write the minimum as a number: `bin:java@17`. |
| `artifact "mytool-1.2.3-linux-x64.tar.gz" requires glibc_min "glibc-2.31"; a minimum starts with a digit and holds only letters, digits, ., -, and +, as in 2.31 or 10.0.17763` | Write the version alone: `glibc_min = "2.31"`. The same check applies to `os_min`. |
| `artifact "mytool-1.2.3-linux-x64.tar.gz" requires command "java.exe"; a command is a bare name such as java` | Drop the directory and `.exe`. |
| `artifact "mytool-1.2.3-linux-x64.tar.gz" requires command "mytool", which the release itself provides` | Remove the requirement. |
| `artifact "mytool-1.2.3-linux-x64.tar.gz" requires command "java" twice` | Give `--require` and `requires.bin` the same minimum, or name the command in one place. |
| `artifact "mytool-1.2.3-linux-x64.tar.gz" says its executables need libraries [libssl.so.3], but they load [libcurl.so.4, libz.so.1]; drop the list and let create read it` | Remove the `libs` list from the artifact's `requires`, or from the top-level `requires` it inherits. |
