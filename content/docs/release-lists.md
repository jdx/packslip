---
title: Manage release lists
description: Publish a signed release list to withdraw releases, mark security fixes, recommend a default version, or list a domain project's releases.
group: publish
weight: 42
---
# Manage release lists

Publishers use a signed release list to withdraw releases, mark security fixes, recommend a default version, or list a domain project's releases. You will build one with `packslip releases`, publish it, and keep it current.
To build a domain project's list in GitHub Actions with the `jdx/packslip/releases` action, follow [Host releases on your own domain](/docs/self-hosting/).

A release list tells consumers which versions are available. Each entry points to a release bundle and records its digest, so withdrawing or recommending a release changes only the list.

## Decide whether you need a list

| Project | List | Where consumers find it |
| --- | --- | --- |
| `mytool.example.com` | Required | `https://mytool.example.com/.well-known/packslip.json` |
| `example.com/tools/mytool` | Required | `https://example.com/.well-known/packslip/tools/mytool.json` |
| `github.com/owner/mytool` | Optional | `.well-known/packslip.json` on the repository's default branch |
| `github.com/owner/tools/mytool` | Optional | `.well-known/packslip/mytool.json` on the default branch |

A domain project's consumers find releases only in its signed list. They do not offer a release the list leaves out, and they refuse a project with no list.

github.com does not serve the well-known path, so a GitHub project commits its list to its default branch, where GitHub serves it at `https://raw.githubusercontent.com/owner/mytool/HEAD/.well-known/packslip.json`. GitLab is verify-only: version 1 defines no discovery for `gitlab.com` ([Discovery](/release/v1/#discovery)).

On GitHub, consumers find versions from release tags that name a version. A list there is supplementary: it overrides the releases it names, and a version it leaves out is still offered from its tag. Add one to withdraw a release (list it as yanked), mark a security fix, or recommend a default version. A list can also offer a release whose tag names no version of this tool, such as `nightly`, `release-1.2.3`, `2026-08-31`, or a monorepo tag that several tools share. Its entry maps the tag to the version in the bundle.

**Warning:** A published list is an ongoing commitment. Consumers of a domain or GitHub project refuse an expired list, and with it the project. Once a consumer has accepted a list, a missing one is an error, so deleting a GitHub project's list does not restore tag discovery. Re-sign the list before every expiry, even when nothing changed. Keep every withdrawal in each rebuild: one left out makes its release eligible again. To stop overriding releases, publish an empty list ([Retire every entry](#retire-every-entry)).

## Create the list

`packslip releases` reads local copies of the release bundles and writes a signed list: each bundle's URL, digest, version, tag, and publish time, plus the list's sequence and expiry. It does not verify the bundles' signatures, so verify each copy first. A GitHub project's list must be signed keylessly, as in [Publish a GitHub repository's list](#publish-a-github-repositorys-list). This example lists and recommends version 1.2.3 of a key-signed domain project:

```sh
packslip verify releases/v1.2.3/packslip.sigstore.json --pubkey release.pub

packslip releases \
  --project mytool.example.com \
  --sequence "$(date +%s)" --valid-for 30d \
  --latest 1.2.3 \
  --release https://mytool.example.com/v1.2.3/packslip.sigstore.json=releases/v1.2.3/packslip.sigstore.json \
  --key release.key \
  --out site/.well-known/packslip.json

packslip verify site/.well-known/packslip.json --pubkey release.pub
```

- `--project`: the project's name, which every listed bundle must name.
- `--sequence`: see [Choose a sequence](#choose-a-sequence).
- `--valid-for`: time until the list expires, a number and a unit (`s`, `m`, `h`, `d`, or `w`); default `30d`.
- `--latest`: the version to recommend, if any; see [Recommend a default version](#recommend-a-default-version).
- `--release URL=PATH`: where consumers fetch a bundle, and the local copy to read. Repeat it for every entry.
- `--key`: the secret key to sign with. In supported CI, omit it to sign keylessly with the job's OIDC identity.
- `--out`: where to write the list. The default, `packslip-releases.sigstore.json` (also the releases action's `out` default), is not a name consumers fetch.

The last two commands print:

```text
wrote site/.well-known/packslip.json (1 release(s), sequence 1792614248, expires 2026-11-20T20:24:08.981008304Z)
ok: release list for mytool.example.com sequence 1792614248 expires 2026-11-20T20:24:08.981008304Z signed by 27A4A6F511135F9A (sigstore-key) listing 1 release(s)
```

Upload the verified list to the well-known location yourself. For a key-signed list, sign with the key consumers pin for the project. A domain name implies no signer, so publish the public key apart from the list ([Publish the signer](/docs/self-hosting/#publish-the-signer)).

### Choose a sequence

Give each new list a higher `sequence` than the last, including a refresh that changes nothing else. Consumers refuse a list whose sequence is lower than one they have accepted ([Freshness and rollback protection](/release/v1/#freshness-and-rollback-protection)).

Use the current Unix time, `$(date +%s)`, as this page does: it increases with no counter to keep, and the releases action defaults to it. A project that adopts Unix-time sequences cannot go back to small hand-picked numbers.

## Refresh or withdraw releases

`packslip releases` does not append to a list, so every update is a full rebuild: pass a `--release` for every entry you keep, and a higher `--sequence`. This rebuild keeps two releases, withdraws 1.2.3, marks 1.2.4 as a security fix, and recommends 1.2.4:

```sh
packslip releases \
  --project mytool.example.com \
  --sequence "$(date +%s)" --valid-for 30d \
  --latest 1.2.4 \
  --release https://mytool.example.com/v1.2.3/packslip.sigstore.json=releases/v1.2.3/packslip.sigstore.json \
  --release https://mytool.example.com/v1.2.4/packslip.sigstore.json=releases/v1.2.4/packslip.sigstore.json \
  --yank https://mytool.example.com/v1.2.3/packslip.sigstore.json='Incorrect Linux archive' \
  --security https://mytool.example.com/v1.2.4/packslip.sigstore.json \
  --key release.key \
  --out site/.well-known/packslip.json
```

- `--yank URL=REASON`: withdraw a listed release. Consumers never select a yanked version. The next time a user who still has it installed runs `packslip install` for the project, install warns with the version and REASON ([Read withdrawn-release warnings](/docs/bootstrap/#read-withdrawn-release-warnings)). mise does not warn, and users who do not run install again hear nothing, so also tell them through your own channels.
- `--security URL`: mark a listed release as a security fix. Consumers may shorten their minimum release age for it.

Each `--yank` and `--security` URL must appear, exactly as given, in a `--release` argument. Repeat every `--yank` and `--security` on every rebuild, including the refresh before expiry.

### Keep withdrawals in the repository

Keep withdrawals in two files on the default branch, and build every list from them. Blank lines and lines starting with `#` are ignored:

- `.github/mytool/yanked`: one `TAG=REASON` line per withdrawn release.
- `.github/mytool/security`: one tag per line for each release that fixes a vulnerability. Put any explanation in a `#` line above the tag.

With the CLI, each `TAG=REASON` line becomes `--yank <bundle URL>=REASON`, and each security tag becomes `--security <bundle URL>`. The workflows in [Publish a GitHub repository's list](#publish-a-github-repositorys-list) and [Build and publish the list](/docs/self-hosting/#build-and-publish-the-list) read both files. To withdraw `v1.2.3` with either workflow, add this line to `.github/mytool/yanked` and merge it:

```text
v1.2.3=Incorrect Linux archive
```

The `push` trigger publishes the new list, and the release stays withdrawn until you remove the line.

The workflows require both files, even when empty: a missing file fails the run, so a rename or deletion cannot silently restore every withdrawn release. [packslip.dev's own list workflow](https://github.com/jdx/packslip/blob/main/.github/workflows/packslip-releases.yml) is a working example.

## Recommend a default version

`--latest 1.2.3` recommends a version for an unconstrained latest request, without reordering versions. Give the exact version of a `--release` entry, including any build metadata, not its tag. It may point to an older supported release while a newer major exists, and it does not affect exact versions, ranges, prefixes, or channels.

Consumers prefer the vendor's signed recommendation; a GitHub project without one gets GitHub's latest release as its recommendation. If the recommendation is ineligible, they fall back to the highest eligible version and say why ([Latest](/release/v1/#latest)).

## Publish a GitHub repository's list

Sign a GitHub project's list keylessly from one workflow file of its repository. That file may differ from the release workflow ([Release lists are pinned separately](/docs/trust/#release-lists-are-pinned-separately)). Once `packslip install` has accepted a list, it refuses one from another workflow file until a person approves the change. [Signer continuity and workflow pinning](/docs/trust/#signer-continuity-and-workflow-pinning) covers this and its opt-out, `packslip releases --no-pin-workflow`, which the releases action has no input for.

The list needs to name only the releases it changes. This workflow builds it with the `jdx/packslip/releases` action from the releases that the [withdrawal files](#keep-withdrawals-in-the-repository) name, then commits it. It runs weekly, so the list is re-signed before it expires, and whenever a change to either file merges:

```yaml
# .github/workflows/packslip-list.yml
name: packslip-list

on:
  schedule:
    - cron: "0 6 * * 1"
  push:
    branches: [main]
    paths:
      - ".github/mytool/**"
  workflow_dispatch:

concurrency:
  group: packslip-list

jobs:
  list:
    runs-on: ubuntu-latest
    permissions:
      contents: write  # Commit the list to the default branch.
      id-token: write  # Sign the list with the workflow's identity.
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7
        with:
          ref: ${{ github.event.repository.default_branch }}
          path: repo
          sparse-checkout: |
            .github/mytool
            .well-known
      - name: Read the withdrawals
        id: withdrawn
        run: |
          set -euo pipefail
          # The action takes entries only, so drop blank lines and comments.
          entries() {
            local file="repo/.github/mytool/$1"
            [ -f "$file" ] || { echo "$file is missing" >&2; exit 1; }
            grep -Ev '^[[:space:]]*(#|$)' "$file" || [ $? -eq 1 ]
          }
          delimiter="mytool_$(openssl rand -hex 16)"
          {
            echo "yank<<$delimiter"
            entries yanked
            echo "$delimiter"
            echo "security<<$delimiter"
            entries security
            echo "$delimiter"
          } >> "$GITHUB_OUTPUT"
      - name: Download the bundles the list names
        env:
          GH_TOKEN: ${{ github.token }}
          ENTRIES: |
            ${{ steps.withdrawn.outputs.yank }}
            ${{ steps.withdrawn.outputs.security }}
        run: |
          set -euo pipefail
          while IFS= read -r line; do
            tag="${line%%=*}"
            if [ -z "$tag" ] || [ -e "lists/$tag" ]; then continue; fi
            gh release download "$tag" --repo "$GITHUB_REPOSITORY" \
              --pattern packslip.sigstore.json --dir "lists/$tag"
          done <<< "$ENTRIES"
      - uses: jdx/packslip/releases@v1
        id: list
        with:
          project: github.com/owner/mytool
          dir: lists
          url-base: https://github.com/owner/mytool/releases/download
          yank: ${{ steps.withdrawn.outputs.yank }}
          security: ${{ steps.withdrawn.outputs.security }}
      - name: Commit the list
        env:
          LIST: ${{ steps.list.outputs.list }}
        run: |
          set -euo pipefail
          mkdir -p repo/.well-known
          cp "$LIST" repo/.well-known/packslip.json
          cd repo
          git config user.name "github-actions[bot]"
          git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
          git add .well-known/packslip.json
          git commit -m "chore: re-sign the release list"
          git push
```

- The download step fetches each named release's bundle into `lists/<tag>/`; the action verifies each one before listing it.
- `url-base` makes each entry's URL the bundle's release asset, such as `https://github.com/owner/mytool/releases/download/v1.2.3/packslip.sigstore.json`.
- The commit step pushes with the job's token, so branch protection must allow that push.
- `contents: write` reaches every step, the action included. To keep it from the action, commit the list from a second job, as [Keep the action away from release write access](/docs/publishing/#keep-the-action-away-from-release-write-access) shows.
- `workflow_dispatch` lets a person re-sign the list by hand.
- To recommend a release or map its tag, add its tag as a line of `ENTRIES`. For a recommendation, also pass its version as the action's `latest` input.
- With nothing in `ENTRIES`, the action fails instead of building an empty list; see [Retire every entry](#retire-every-entry).

For a monorepo tool, set `project` to its name, such as `github.com/owner/tools/mytool`, set `url-base` to `https://github.com/owner/tools/releases/download`, and commit the list at `.well-known/packslip/mytool.json`. `packslip create` names the tool's bundle after its subpath, with each `/` written as `-`: `packslip.mytool.sigstore.json` here. Use that name in `--pattern` and as the action's `bundle` input.

## Retire every entry

**Warning:** Older consumers refuse an empty list, and with it the project; [Version requirements](/docs/compatibility/#version-requirements) names them. Keep at least one entry while your users run them.

Run `packslip releases` with no `--release` to write an empty list, which retires every withdrawal, security mark, tag mapping, and recommendation earlier lists carried. A GitHub project's consumers then choose from its GitHub releases alone. A domain project offers nothing to install, and `packslip install` reports `no eligible release found`.

An empty list expires like any other, so keep re-signing it. The releases action does not build one, so in `packslip-list.yml`, replace the three steps from `Read the withdrawals` through `jdx/packslip/releases` with these two:

```yaml
      - name: Install packslip
        run: |
          curl -fsSL https://packslip.sh/vX.Y.Z | sh
          echo "$HOME/.local/bin" >> "$GITHUB_PATH"
      - name: Build an empty list
        id: list
        run: |
          set -euo pipefail
          packslip releases \
            --project github.com/owner/mytool \
            --sequence "$(date +%s)" --valid-for 30d \
            --out packslip-releases.sigstore.json
          packslip verify packslip-releases.sigstore.json
          echo "list=packslip-releases.sigstore.json" >> "$GITHUB_OUTPUT"
```

- The install step uses a [versioned install script](/docs/install/#pin-a-versioned-install-script); replace `X.Y.Z` with the release to run.
- The project's name implies the signer policy, so `packslip verify` needs no trust flags.
- The `Commit the list` step stays; it reads this step's `list` output.

## Publish a list as a stamping host

A registry, mirror, or scanning service that signs a list of the releases it has checked is a stamping host. Its list stamps each release it names and leaves the vendor's bundle unchanged. [Stamping hosts](/docs/trust/#stamping-hosts) explains how consumers choose and enforce them; [Lists from other publishers](/release/v1/#lists-from-other-publishers) defines the lists.

Publish one list per vendor project on your host, at the well-known path built from the vendor's full project name, such as `https://scanner.example/.well-known/packslip/github.com/owner/mytool.json`. Set `--project` to the vendor's project, because every listed bundle must name it:

```sh
packslip releases \
  --project github.com/owner/mytool \
  --sequence "$(date +%s)" --valid-for 30d \
  --release https://github.com/owner/mytool/releases/download/v1.2.3/packslip.sigstore.json=bundles/v1.2.3/packslip.sigstore.json \
  --evidence https://github.com/owner/mytool/releases/download/v1.2.3/packslip.sigstore.json=scan=https://scanner.example/reports/1.2.3 \
  --key scanner.key \
  --out site/.well-known/packslip/github.com/owner/mytool.json
```

- `--evidence URL=KIND` or `URL=KIND=DETAIL`: what you checked about the listed release at URL. `scan` says the artifacts were scanned, and its detail points to the report. [Repackager attestation](/release/v1/#repackager-attestation) lists the kinds.

## Fix a failed list build

| Error | What to do |
| --- | --- |
| `URL is not among the --release entries` | The `--yank`, `--security`, or `--evidence` URL differs from every `--release` URL. Copy it exactly, or add a `--release` for that release. |
| `latest "1.2.4" does not name a version in the release list` | List that release, or give `--latest` a listed bundle's version. |
| `version must be semver 2.0.0 (MAJOR.MINOR.PATCH, with an optional prerelease part), got "v1.2.3"` | `--latest` takes a version, not a tag. Pass `1.2.3`. |
| `PATH: not a packslip bundle: it is for github.com/owner/mytool, the list is for mytool.example.com` | Every listed bundle must name the `--project` value. |
| `duration "30": expected a unit of s, m, h, d or w` | Give `--valid-for` a unit, such as `30d`. |

## Diagnose a release that is not offered

Rows in code font are the errors `packslip install` prints; other consumers word them their own way.

| Symptom | What to check |
| --- | --- |
| `host project has no signed release list` | No list is at the project's well-known URL (for `example.com/tools/mytool`, `https://example.com/.well-known/packslip/tools/mytool.json`), or it was removed. Publish the signed list there. |
| A GitHub tag is invisible | The release must be published, not a draft, with its bundle attached. To offer a tag that does not map to a version for this project ([Tags](/release/v1/#tags)), list the release in the signed list. |
| A release is found but refused | Verify its bundle, and check that its signed project, version, and digest match the discovery metadata. |
| A withdrawn version reappears | A rebuild dropped its yanked entry, or a GitHub list left it out instead of yanking it; see [Keep withdrawals in the repository](#keep-withdrawals-in-the-repository). |
| `release list is stale, missing, or rolled back` | The old list expired, a GitHub project's accepted list is gone, or the new sequence is lower than an accepted one. Re-sign the retained entries with a later expiry and a higher sequence ([Choose a sequence](#choose-a-sequence)). |
| `no eligible release found` | The list names no release the request can take, for example because it is empty or all its releases are withdrawn. |
| A latest request selects another version | Check that the recommendation is eligible under withdrawal, prerelease, minimum age, stamping, and host policy. |
