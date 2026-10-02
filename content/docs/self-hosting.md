---
title: Host releases on your own domain
weight: 45
group: publish
description: Name a project after its download host, publish its releases and signed release list there from GitHub Actions, and keep the list current.
---
# Host releases on your own domain

A project named after its host, such as `mytool.example.com`, is found
through the signed release list at that host and nowhere else. The bytes
can live anywhere; this guide puts them on the same host, publishes both
from a GitHub Actions release job, and keeps the list current between
releases. Releases are still signed by the repository's workflow, so
consumers pin that repository's workflows, as they would for a GitHub
project. If the
project already publishes as `github.com/owner/repo`, read
[Move a GitHub project](#move-a-github-project) before switching consumers.

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
[Manage release lists](/docs/release-lists/#choose-a-discovery-location).

## Serve the files

A static site host serves the release directories and the list like any
other files, as long as the list has the right path and content type:
`application/json` at `/.well-known/packslip.json`, or at
`/.well-known/packslip/<path>.json` for a project with a path. Check that
the host publishes the `.well-known` directory, because some site
generators and hosts skip dot-directories.

The release directories never change once published: a consumer pins the
digest of every file it downloads, so serve them with a long, immutable
cache lifetime. The list changes with every release and refresh, so give it
a short one, five minutes or so.

On Cloudflare, one Worker can serve a documentation site as static
assets, serve the releases from an R2 bucket on the same hostname, and
count downloads. packslip.dev works this way. Its
[configuration](https://github.com/jdx/packslip/blob/main/wrangler.jsonc)
sends the release paths, `/v*` and `/.well-known/packslip.json`, to the
Worker before the static assets with `run_worker_first`; with a 404 page
configured, Cloudflare would otherwise answer a browser's request for them
with that page. The
[Worker](https://github.com/jdx/packslip/blob/main/cloudflare/worker.js)
reads `<tool>/<tag>/<file>` from R2, the layout the upload steps in
[Publish a release](#publish-a-release) write. It matches only tags that
start with `v` and the bare-host list path, so adjust both patterns for
other tags or for a project with a path.

## Sign as the repository

A domain name implies no signer, so consumers have to be told what to pin:
the OIDC issuer and an identity prefix covering the repository's
workflows, `https://github.com/owner/repo/`. That is the identity a
`github.com/owner/repo` name implies, spelled out, except that the prefix
names the repository by its path rather than by its repository ID. In
mise, a tool or registry entry carries it as options, as
[Use packslip with mise](/docs/mise/) describes:

```toml
[tools]
"packslip:mytool.example.com" = { version = "latest", issuer = "https://token.actions.githubusercontent.com", identity_prefix = "https://github.com/owner/repo/" }
```

Because the prefix is a path, it does not follow a rename. Once the
repository is renamed or moves to another owner, its workflows sign as the
new name, and no prefix covers both names. Consumers holding the old
prefix refuse the next list, which is signed under the new name, and with
it the project. The list job pins the repository's current name, so it
fails on every bundle signed under the old one.

Treat the rename as one change. Rename the repository, then describe each
release you keep again from the renamed repository with the steps in
[Move a GitHub project](#move-a-github-project); only the new bundle needs
uploading, since the files are already on the host. Delete the other old
bundles from the bucket. A new bundle replaces a file the host serves as
immutable, so purge the old copy from every cache in front of the host: a
cached old bundle fails the new list's digest. Then run the list workflow,
and change the published pin (for mise, the registry entry) as soon as
the new list is out, not before: a consumer holding the new prefix refuses
the old list.

Every bundle a project publishes should come from one workflow file. A
consumer remembers which workflow file signed the releases it accepted,
and does not accept one signed by another until a person approves it. A
second workflow that signs bundles, such as a separate backfill workflow,
therefore looks like a change of signer. The list may be signed by a
different file in the same repository; consumers check it against the
pin, not against the bundles' signer.

A vendor that must sign releases from several files can ask consumers to
hold it to the repository instead: `packslip create --no-pin-workflow`
records `pin_workflow: false` in the release manifest. The action has no
input for it, so such a project runs `packslip create` itself, as
[Keep later releases acceptable](/docs/publishing/#keep-later-releases-acceptable)
explains. Consumers that already accepted a release refuse the first one
that declares it until a person approves it. Consumers written before the
field existed ignore it, so they still refuse a release from another file
until a person approves it; see
[Reusable workflows](/release/v1/#reusable-workflows).

## Publish a release

In the release job, name the project and where its files will be, upload
the files, then run the action and upload the bundle it wrote. The action
still attaches the bundle to the GitHub release unless `upload` is
`false`. Consumers of `mytool.example.com` read the copy the list names;
the attached copy is for people reading the GitHub release.

```yaml
permissions:
  contents: write      # Create the GitHub release and attach the bundle.
  id-token: write      # Sign with the workflow's identity.
  attestations: write  # Publish provenance for the artifacts.

steps:
  # Build the archives and create the GitHub release before these steps.
  - name: Upload the release files
    env:
      AWS_ACCESS_KEY_ID: ${{ secrets.R2_ACCESS_KEY_ID }}
      AWS_SECRET_ACCESS_KEY: ${{ secrets.R2_SECRET_ACCESS_KEY }}
      AWS_REGION: auto
      AWS_ENDPOINT_URL: https://<account>.r2.cloudflarestorage.com
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
    env:
      BUNDLE: ${{ steps.packslip.outputs.bundle }}
      AWS_ACCESS_KEY_ID: ${{ secrets.R2_ACCESS_KEY_ID }}
      AWS_SECRET_ACCESS_KEY: ${{ secrets.R2_SECRET_ACCESS_KEY }}
      AWS_REGION: auto
      AWS_ENDPOINT_URL: https://<account>.r2.cloudflarestorage.com
    run: |
      aws s3 cp "$BUNDLE" "s3://releases/mytool/${GITHUB_REF_NAME}/packslip.sigstore.json" \
        --content-type application/json --cache-control "public, max-age=31536000, immutable"
```

The example writes to a Cloudflare R2 bucket through its S3 endpoint; any
host that serves files over HTTPS works the same way. The examples on
this page assume the host serves the bucket's `mytool/` prefix at
`https://mytool.example.com/`. Give the storage credentials only to the
upload steps, so they never reach the action. The job still grants
`contents: write` to every step, the action included. To keep the action
away from that as well, split the job as
[Keep the action away from release write access](/docs/publishing/#keep-the-action-away-from-release-write-access)
shows.

A resource declared with `asset:` gets a URL under `url-base` like the
artifacts do, so upload it with them. Upload the bundle last. The list job
includes every bundle it finds in the bucket, so whichever run comes next,
scheduled or not, lists the release once its bundle is there. Everything
the bundle describes must be in place by then.

## Build and publish the list

The `jdx/packslip/releases` action turns a directory of published bundles,
laid out as `<dir>/<tag>/packslip.sigstore.json`, into a signed list. It
verifies every bundle under the pin first, refuses one for another project
or in the wrong directory, and verifies the list it wrote.

Put the list job in a workflow of its own, so the release workflow, a
weekly schedule, a merged withdrawal, and a person can all run it.
Withdrawals and security fixes live in two files committed to the
repository, which every run reads from the default branch:
`.github/mytool/yanked` holds one `TAG=REASON` line per withdrawn release,
and `.github/mytool/security` holds one tag per line for each release that
fixes a vulnerability. Blank lines and lines starting with `#` are
ignored. Both files must exist, even when empty; see
[Keep withdrawals in the repository](#keep-withdrawals-in-the-repository).

```yaml
# .github/workflows/packslip-releases.yml
name: packslip-releases

on:
  workflow_call:
    secrets:
      R2_ACCESS_KEY_ID:
        required: true
      R2_SECRET_ACCESS_KEY:
        required: true
  schedule:
    - cron: "0 6 * * 1"
  push:
    branches: [main]
    paths:
      - ".github/mytool/**"
  workflow_dispatch:

concurrency:
  group: packslip-releases

jobs:
  list:
    runs-on: ubuntu-latest
    permissions:
      contents: read   # Read the withdrawal and security files.
      id-token: write  # Sign the list with the workflow's identity.
    steps:
      # Read the files from the default branch, even when a release
      # workflow running on a tag calls this one: the tag can predate a
      # withdrawal merged since.
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7
        with:
          ref: ${{ github.event.repository.default_branch }}
          path: withdrawals
          sparse-checkout: .github/mytool
          persist-credentials: false
      - name: Read the withdrawals
        id: withdrawn
        run: |
          set -euo pipefail
          # The action takes entries only, so drop blank lines and comments.
          entries() {
            local file="withdrawals/.github/mytool/$1"
            # A missing file fails the run instead of publishing a list
            # that quietly restores every withdrawn release.
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
      - name: Fetch the published bundles
        env:
          AWS_ACCESS_KEY_ID: ${{ secrets.R2_ACCESS_KEY_ID }}
          AWS_SECRET_ACCESS_KEY: ${{ secrets.R2_SECRET_ACCESS_KEY }}
          AWS_REGION: auto
          AWS_ENDPOINT_URL: https://<account>.r2.cloudflarestorage.com
        run: aws s3 sync s3://releases/mytool/ lists/ --exclude '*' --include '*/packslip.sigstore.json'
      - uses: jdx/packslip/releases@v1
        id: list
        with:
          project: mytool.example.com
          dir: lists
          url-base: https://mytool.example.com
          yank: ${{ steps.withdrawn.outputs.yank }}
          security: ${{ steps.withdrawn.outputs.security }}
      - name: Publish the list
        env:
          LIST: ${{ steps.list.outputs.list }}
          AWS_ACCESS_KEY_ID: ${{ secrets.R2_ACCESS_KEY_ID }}
          AWS_SECRET_ACCESS_KEY: ${{ secrets.R2_SECRET_ACCESS_KEY }}
          AWS_REGION: auto
          AWS_ENDPOINT_URL: https://<account>.r2.cloudflarestorage.com
        run: |
          aws s3 cp "$LIST" s3://releases/mytool/.well-known/packslip.json \
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
| `sequence` | An integer that increases with every list; defaults to the current Unix time. Once consumers have accepted a list with the default, they refuse a smaller hand-picked number as a rollback. |
| `valid-for` | How long the list stays current, as a number and unit (`30d`, `12h`, `2w`); defaults to `30d`. |
| `latest` | Recommend this exact listed version, such as `1.2.3`: a version, not a tag as `yank` and `security` take. Empty leaves consumers to take the highest eligible version. |
| `yank` | Releases to withdraw, one per line, as `TAG=REASON` or `URL=REASON`. Applies to this run's list only; see [Keep withdrawals in the repository](#keep-withdrawals-in-the-repository). |
| `security` | Releases that fix a vulnerability, one tag or URL per line. Applies to this run's list only; see [Keep withdrawals in the repository](#keep-withdrawals-in-the-repository). |
| `identity-prefix`, `identity`, `issuer` | The pin the bundles and the list must verify under; default to this repository's workflows through GitHub's issuer. `identity` is an exact certificate identity, ref included; GitHub identities name the ref the workflow ran on, so bundles signed on different tags never share one. Most projects want `identity-prefix`. |
| `out` | Where to write the list; defaults to `packslip-releases.sigstore.json`. |
| `packslip-version`, `packslip-path`, `token` | As for the [release action](/docs/publishing/#action-inputs). |

Outputs: `list`, the path written, and `count`, how many releases it names.
The action signs keylessly with the job's identity; a project whose
consumers pin a key runs [`packslip releases`](/docs/release-lists/#create-the-list)
with `--key` instead.

## Keep the list current

A consumer refuses an expired list, and for a project on its own domain
that means refusing the project. The workflow above runs every week, when
a change to the withdrawal files merges, and whenever a person dispatches
it. Also call it from the release workflow once the bundle is uploaded,
passing only the two storage secrets the list workflow declares:

```yaml
jobs:
  # The release job from Publish a release goes here.
  list:
    needs: release # The job that uploads the bundle.
    uses: ./.github/workflows/packslip-releases.yml
    secrets:
      R2_ACCESS_KEY_ID: ${{ secrets.R2_ACCESS_KEY_ID }}
      R2_SECRET_ACCESS_KEY: ${{ secrets.R2_SECRET_ACCESS_KEY }}
    permissions:
      contents: read   # Read the withdrawal and security files.
      id-token: write  # Sign the list.
```

A weekly run against a 30-day validity leaves room for a few failed runs.

### Keep withdrawals in the repository

Every run builds the list from scratch, from that run's `yank` and
`security` inputs alone. A withdrawal given to one run, as a
`workflow_dispatch` input for instance, is missing from the next scheduled
or post-release list, and consumers are offered the withdrawn release
again. That is why the workflow reads both from committed files on every
run.

To withdraw `v1.2.3`, add `v1.2.3=Incorrect Linux archive` to
`.github/mytool/yanked` and merge. The `push` trigger publishes a new list
with the release marked `yanked`, and it stays withdrawn through every
later run until you remove its line. To mark a security fix, add its tag
to `.github/mytool/security` the same way. Put the reason for a security
mark in a `#` comment above its line. Keep a withdrawn release's bundle in
the bucket: an entry for a tag that `dir` does not hold fails the run with
`is not among the --release entries`.

The files must exist, even when empty. A missing file fails the run, so a
rename or deletion cannot publish a list that silently restores every
withdrawn release.
[packslip.dev's own workflow](https://github.com/jdx/packslip/blob/main/.github/workflows/packslip-releases.yml)
does this, and its
[procedure](https://github.com/jdx/packslip/blob/main/RELEASING.md#withdraw-a-release-or-mark-a-security-fix)
is a worked example.

A weekly run against a 30-day validity leaves room for a few failed runs,
and the concurrency group stops two runs from publishing out of order.

## Move a GitHub project

A release bundle names one project, so bundles published as
`github.com/owner/repo` cannot go in `mytool.example.com`'s list, and
`packslip releases` and the releases action refuse to build an empty
list. Before switching consumers over, describe at least the current
release again under the new name. Do it from the workflow file that signs
new releases, as [Sign as the repository](#sign-as-the-repository)
explains, in a manually dispatched job with `contents: read` and
`id-token: write`:

1. Download the release's files from GitHub into `dist/`, and delete any
   `packslip*.sigstore.json` among them: it names the old project, and an
   `artifacts` glob such as `dist/*` would take it for an artifact.
2. Run `jdx/packslip@v1` with the release job's `artifacts`, `bin`, and
   `resources`, reading the files from `dist/`, and with:
   - `project: mytool.example.com`
   - the release's `tag`, and its `url-base`, such as
     `https://mytool.example.com/v1.2.3`
   - `version`, only if the tag is not `v` followed by the version
   - `commit`, set to the tag's full commit SHA, since the default is the
     commit the dispatch ran on
   - `attest: link`
   - `upload: false`. Without it, the action tries to attach the new
     bundle to the GitHub release. With `contents: read` that step fails;
     with write access it would replace the original
     `packslip.sigstore.json`, since both bundles have that name.
3. Upload the files and then the new bundle to the release's tag
   directory, `s3://releases/mytool/<tag>/`, with the commands from
   [Publish a release](#publish-a-release). In a dispatched job
   `GITHUB_REF_NAME` is the branch, so pass the tag instead. Then run the
   list workflow.

The new bundle is logged when you sign it, so a consumer that enforces a
minimum release age treats the release as new until that age has passed.

The bundle attached to each existing GitHub release keeps naming the old
project and stays valid for anyone reading it there. New releases attach
bundles for `mytool.example.com`, so a consumer still asking for
`github.com/owner/repo` keeps what it installed but cannot install any
release published after the move: those releases' bundles name
`mytool.example.com`. Consumers rename the tool they ask for, from
`packslip:github.com/owner/repo` to `packslip:mytool.example.com` in
mise, and pin the identity as
[Sign as the repository](#sign-as-the-repository) shows. To them the new
name is a new project with no signer history.

## Troubleshoot

| Symptom | What to check |
| --- | --- |
| `no identity to verify against` | The project is not on a forge, so the pin is not implied: pass `--identity-prefix` and `--issuer`, or `--pubkey`, to `packslip verify`. The actions do. |
| `a release list cannot be empty` | Nothing under `<dir>/<tag>/`; check the sync, or backfill a release. |
| `is for github.com/owner/repo, not mytool.example.com` | A bundle from before the move is in the directory; describe that release again under the new name, or remove it. |
| `is release v1.2.3 but sits under v1.2.4/` | The directory is named by the tag the bundle records; move it. |
| `is not among the --release entries` | A `yank` or `security` entry names a tag or URL that is not in `dir`. |
| A withdrawn release is eligible again after a scheduled or post-release run | The withdrawal was given to a single run. [Commit it](#keep-withdrawals-in-the-repository) so every run passes it. |
| `signed by "https://github.com/old/repo/...", expected an identity starting with "https://github.com/new/repo/"` | The repository was renamed or transferred after that release was signed, and the action pins the current name. Leave that bundle out of `dir`: delete it from the bucket, or exclude it from the sync. To keep the release listed instead, describe it again from the renamed repository and purge the old copy from every cache, as [Sign as the repository](#sign-as-the-repository) explains. |
| A consumer says the list expired | The scheduled run has not published one lately. GitHub disables a public repository's schedules after 60 days without activity, so check that the workflow is enabled, then dispatch it once by hand. |
| A consumer refuses a backfilled release as a different signer | The backfill ran from another workflow file; run it from the one that signs releases, and have the consumer forget the pin it took. |
