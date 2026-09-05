---
title: Host releases on your own domain
weight: 55
description: Name a project after its download host, publish its releases and signed release list there from GitHub Actions, and keep the list current.
---
# Host releases on your own domain

A project named after its host, such as `mytool.example.com`, is found
through the signed release list at that host and nowhere else. The bytes
can live anywhere; this guide puts them on the same host, publishes both
from a GitHub Actions release job, and keeps the list current between
releases. The signing identity stays the repository's workflow, so
consumers pin the same thing they would for a GitHub project.

## Lay out the host

Use one directory per release, named by its tag, and the well-known path
for the list:

```text
https://mytool.example.com/v1.2.3/mytool-1.2.3-linux-x64.tar.gz   an artifact
https://mytool.example.com/v1.2.3/mytool.usage.kdl               a resource asset
https://mytool.example.com/v1.2.3/packslip.sigstore.json         the release bundle
https://mytool.example.com/.well-known/packslip.json             the release list
```

A project with a path, `example.com/tools/mytool`, serves its list at
`https://example.com/.well-known/packslip/tools/mytool.json` instead; see
[where consumers find the list](/docs/release-lists/#choose-a-discovery-location).

The release directories never change once published: a consumer pins the
digest of every file it downloads, so serve them with a long, immutable
cache lifetime. The list changes with every release and refresh, so give it
a short one, five minutes or so.

## Sign as the repository

A consumer derives nothing from a domain name, so it is told what to pin:
the OIDC issuer and an identity prefix covering the repository's
workflows, `https://github.com/owner/repo/`. That is the policy a
`github.com/owner/repo` name implies, spelled out. In
[mise](/docs/mise/), a tool or registry entry carries it as options:

```toml
[tools]
"packslip:mytool.example.com" = { version = "latest", issuer = "https://token.actions.githubusercontent.com", identity_prefix = "https://github.com/owner/repo/" }
```

Every bundle a project publishes should come from one workflow file. A
consumer remembers which workflow signed the releases it accepted and asks
a person before taking one from another, so a second file that signs
bundles, for a backfill say, looks like a change of signer. The list may
be signed by a different file in the same repository; consumers check it
against the pin, not against the bundles' signer.

## Publish a release

In the release job, name the project and where its files will be, upload
the files, then run the action and upload the bundle it wrote. The action
still attaches the bundle to the GitHub release unless `upload` is
`false`, which is fine: what consumers read is the copy the list names.

```yaml
permissions:
  contents: write
  id-token: write
  attestations: write

env:
  AWS_ACCESS_KEY_ID: ${{ secrets.R2_ACCESS_KEY_ID }}
  AWS_SECRET_ACCESS_KEY: ${{ secrets.R2_SECRET_ACCESS_KEY }}
  AWS_REGION: auto
  AWS_ENDPOINT_URL: https://<account>.r2.cloudflarestorage.com

steps:
  # Build the archives and create the GitHub release before these steps.
  - name: Upload the release files
    run: |
      aws s3 cp dist/ "s3://releases/mytool/${GITHUB_REF_NAME}/" --recursive \
        --cache-control "public, max-age=31536000, immutable"
  - uses: jdx/packslip@v1
    id: packslip
    with:
      project: mytool.example.com
      url-base: https://mytool.example.com/${{ github.ref_name }}
      artifacts: dist/*.tar.gz dist/*.zip
      bin: mytool
      resources: cli-spec/usage=asset:dist/mytool.usage.kdl
  - name: Upload the bundle
    run: |
      aws s3 cp "${{ steps.packslip.outputs.bundle }}" \
        "s3://releases/mytool/${GITHUB_REF_NAME}/packslip.sigstore.json" \
        --content-type application/json --cache-control "public, max-age=31536000, immutable"
```

The example writes to a Cloudflare R2 bucket through its S3 endpoint; any
host that serves files over HTTPS works the same way. A resource declared
with `asset:` gets a URL under `url-base` like the artifacts do, so upload
it with them. Upload the bundle last: a release is only offered once the
list names its bundle, and the bundle should only appear once everything
it describes is in place.

## Build and publish the list

The `jdx/packslip/releases` action turns a directory of published bundles,
laid out as `<dir>/<tag>/packslip.sigstore.json`, into a signed list. It
verifies every bundle under the pin first, refuses one for another project
or in the wrong directory, and verifies the list it wrote.

```yaml
permissions:
  contents: read
  id-token: write

steps:
  - uses: actions/checkout@v5
  - name: Fetch the published bundles
    run: aws s3 sync s3://releases/mytool/ lists/ --exclude '*' --include '*/packslip.sigstore.json'
  - uses: jdx/packslip/releases@v1
    id: list
    with:
      project: mytool.example.com
      dir: lists
      url-base: https://mytool.example.com
  - name: Publish the list
    run: |
      aws s3 cp "${{ steps.list.outputs.list }}" s3://releases/mytool/.well-known/packslip.json \
        --content-type application/json --cache-control "public, max-age=300"
```

The list's sequence defaults to the current Unix time, which increases on
its own with no counter to keep. Its validity defaults to 30 days.

### Action inputs

| Input | Purpose and default |
| --- | --- |
| `project` | Required. The project's name, as its bundles spell it. |
| `dir` | Required. A directory of bundles as `<dir>/<tag>/<bundle>`. |
| `url-base` | Required. Where the bundles are served, without the tag: `https://<host>`. |
| `bundle` | The bundle file name in every tag directory; defaults to `packslip.sigstore.json`. |
| `sequence` | An integer that increases with every list; defaults to the current Unix time. |
| `valid-for` | How long the list stays current, as a number and unit (`30d`, `12h`, `2w`); defaults to `30d`. |
| `latest` | Recommend this exact listed version. Empty leaves consumers to take the highest eligible version. |
| `yank` | Releases to withdraw, one per line, as `TAG=REASON` or `URL=REASON`. |
| `security` | Releases that fix a vulnerability, one tag or URL per line. |
| `identity-prefix`, `identity`, `issuer` | The pin the bundles and the list must verify under; default to this repository's workflows through GitHub's issuer. |
| `out` | Where to write the list; defaults to `packslip-releases.sigstore.json`. |
| `packslip-version`, `packslip-path`, `token` | As for the [release action](/docs/publishing/#action-inputs). |

Outputs: `list`, the path written, and `count`, how many releases it names.
The action signs keylessly with the job's identity; a project whose
consumers pin a key runs [`packslip releases`](/docs/release-lists/#create-the-list)
with `--key` instead.

## Keep the list current

A consumer refuses an expired list, and for a project on its own domain
that means refusing the project. Put the list job in its own workflow with
three triggers, and call it from the release workflow after the release is
public:

```yaml
on:
  workflow_call:
  schedule:
    - cron: "0 6 * * 1"
  workflow_dispatch:
    inputs:
      yank:
        description: "Releases to withdraw, one per line, as TAG=REASON"
        default: ""
```

A weekly run against a 30-day validity leaves room for a few failed runs.
Pass the dispatch inputs through to the action's `yank` and `security`
inputs to withdraw a release or mark one a security fix without a new
release; the withdrawn release stays in the list with its status.

## Serve the files

A static site host serves the release directories and the list as it
serves anything else, as long as the list's path and content type are
right: `application/json` at `/.well-known/packslip.json`. On Cloudflare,
one Worker can serve a documentation site as static assets and the
releases from an R2 bucket on the same hostname, since a request for a
path the site has no file for is what reaches the Worker's code. That is
also a place to count downloads. packslip.dev works this way; its
[Worker](https://github.com/jdx/packslip/blob/main/cloudflare/worker.js)
and [configuration](https://github.com/jdx/packslip/blob/main/wrangler.jsonc)
are a starting point.

## Move a GitHub project

A release bundle names one project, so bundles published as
`github.com/owner/repo` cannot go in `mytool.example.com`'s list, and the
list cannot be empty. Before switching consumers over, describe at least
the current release again under the new name: download its files from the
GitHub release, run the action with the tag, `version`, `project`, and
`url-base` set and `attest: link`, and upload the result beside the files.
Do it from the workflow file that signs new releases, as
[above](#sign-as-the-repository). Then run the list job.

The bundle attached to the GitHub release keeps naming the old project and
stays valid for anyone reading it there. Consumers rename the tool they
ask for, `packslip:github.com/owner/repo` to `packslip:mytool.example.com`
in mise, and pin the identity as shown above; a consumer that remembered
the old project's signer starts afresh under the new name.

## Troubleshoot

| Symptom | What to check |
| --- | --- |
| `no identity to verify against` | The project is not on a forge, so the pin is not implied: pass `--identity-prefix` and `--issuer`, or `--pubkey`, to `packslip verify`. The actions do. |
| `a release list cannot be empty` | Nothing under `<dir>/<tag>/`; check the sync, or backfill a release. |
| `is for github.com/owner/repo, not mytool.example.com` | A bundle from before the move is in the directory; describe that release again under the new name, or remove it. |
| `is release v1.2.3 but sits under v1.2.4/` | The directory is named by the tag the bundle records; move it. |
| `is not among the --release entries` | A `yank` or `security` entry names a tag or URL that is not in `dir`. |
| A consumer says the list expired | The scheduled run has not published one lately; check it, and dispatch it once by hand. |
| A consumer refuses a backfilled release as a different signer | The backfill ran from another workflow file; run it from the one that signs releases, and have the consumer forget the pin it took. |
