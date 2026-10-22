---
title: Host releases on your own domain
description: Publish a domain project's releases and signed release list on a host you run, from GitHub Actions, with Cloudflare R2 as the example.
group: publish
weight: 44
---
# Host releases on your own domain

For publishers who name a tool after a domain, such as `mytool.example.com`, and sign it on GitHub Actions: at the end, your host serves each release's files and bundle, and a workflow keeps the signed release list current.
If the tool is already published as `github.com/owner/mytool`, set up hosting here, then follow [Move a GitHub project to your domain](/docs/move-to-domain/) before switching consumers.

Name the project after your domain when consumers should discover its releases through your host. They then find releases only through a signed release list on that host, and refuse the project without one ([Manage release lists](/docs/release-lists/) explains lists). Artifacts and bundles can be served from another host than the list; the examples use one host.

## Before you start

If you release from another CI system or sign with a key, follow [Publish from other CI](/docs/other-ci/) and [Manage release lists](/docs/release-lists/) instead. To serve a `github.com/owner/mytool` project's files from your host without renaming it, see [Publish outside GitHub releases](/docs/publishing/#publish-outside-github-releases).

You need:

- A host that serves files over HTTPS. The examples use a Cloudflare R2 bucket named `releases`; the Worker in [Serve the files](#serve-the-files) serves its `mytool/` prefix at `https://mytool.example.com/`. On another host, keep the layout and swap the `aws s3` commands for its upload tool.
- For R2, the AWS CLI on the runner and repository secrets `R2_ACCESS_KEY_ID` and `R2_SECRET_ACCESS_KEY` with access to the bucket. Replace `<account>` in `https://<account>.r2.cloudflarestorage.com` with your account ID.
- A release job that builds the final archives and creates the GitHub release.

## Lay out the host

Use one directory per release, named by its tag, and serve the list at the well-known path:

```text
https://mytool.example.com/v1.2.3/mytool-1.2.3-linux-x64.tar.gz   an artifact
https://mytool.example.com/v1.2.3/mytool.usage.kdl               a resource asset
https://mytool.example.com/v1.2.3/packslip.sigstore.json         the release bundle
https://mytool.example.com/.well-known/packslip.json             the release list
```

Each artifact goes at the URL its statement records, with the bundle beside it. A project with a path, such as `example.com/tools/mytool`, serves its list at `/.well-known/packslip/<path>.json`; see the table in [Decide whether you need a list](/docs/release-lists/#decide-whether-you-need-a-list).

## Serve the files

A static site host can serve these files like any others, provided that:

- The list is served as `application/json`.
- `.well-known` is published; some site generators and hosts skip dot-directories.
- Release directories have a long, immutable cache lifetime: they never change, and consumers pin each file's digest.
- The list's cache lifetime is about five minutes, since it changes with every release and refresh.

An R2 custom domain serves a bucket from its root, so serve the `mytool/` prefix at `https://mytool.example.com/` with a Worker that reads each path from it:

```js
// src/index.js
export default {
  async fetch(request, env) {
    if (request.method !== "GET" && request.method !== "HEAD") {
      return new Response(null, { status: 405, headers: { allow: "GET, HEAD" } });
    }
    const key = "mytool" + new URL(request.url).pathname;
    const object = request.method === "HEAD"
      ? await env.RELEASES.head(key)
      : await env.RELEASES.get(key);
    if (object === null) {
      return new Response("not found\n", { status: 404 });
    }
    const headers = new Headers();
    object.writeHttpMetadata(headers);
    headers.set("etag", object.httpEtag);
    return new Response(object.body ?? null, { headers });
  },
};
```

```jsonc
// wrangler.jsonc
{
  "name": "mytool-releases",
  "main": "src/index.js",
  "compatibility_date": "2026-09-01",
  "routes": [{ "pattern": "mytool.example.com", "custom_domain": true }],
  "r2_buckets": [{ "binding": "RELEASES", "bucket_name": "releases" }]
}
```

- `writeHttpMetadata` serves the content type and cache lifetime each upload step stores with the file.
- `routes` attaches `mytool.example.com` to the Worker as a custom domain when you run `wrangler deploy`.
- `r2_buckets` gives the Worker the `releases` bucket as `env.RELEASES`.

The same Worker can count downloads and serve a site from static assets. For a site, list the release paths, such as `/v*` and `/.well-known/packslip.json`, in `run_worker_first`; otherwise, with a 404 page configured, Cloudflare answers a browser's request for them with that page.

## Publish the signer

The release workflow signs keylessly as your GitHub repository, but a domain name implies no signer. Publish the signer consumers should pin, for example on the install page:

- OIDC issuer: `https://token.actions.githubusercontent.com`, for GitHub Actions.
- Identity prefix: `https://github.com/owner/mytool/`, which matches that repository's workflows. It does not check which repository's run signed, so another repository that calls one of them as a reusable workflow, such as the list workflow below, matches too.
- Signer fingerprint: the `ps1_…` value `packslip pin` prints.

After the first release, download its bundle and fingerprint it:

```sh
curl -fsSLO https://mytool.example.com/v1.2.3/packslip.sigstore.json
packslip pin packslip.sigstore.json \
  --issuer https://token.actions.githubusercontent.com \
  --identity-prefix https://github.com/owner/mytool/
```

Each consumer uses them differently; [What each consumer enforces](/docs/trust/#what-each-consumer-enforces) compares them:

- `packslip verify`: `--issuer` with `--pin`, plus `--identity-prefix` to require your workflows.
- mise: the `issuer` and `identity_prefix` tool options ([Use packslip with mise](/docs/mise/)).
- `packslip install`: none needed. On first use it trusts the signer your host's list names, and remembers that workflow and your repository's ID. A later list or release signed in another repository, or by another workflow file, needs each user's approval. `--pin` with the fingerprint holds the project to your repository from the first install, on every run that passes it ([First use and signer fingerprints](/docs/trust/#first-use-and-signer-fingerprints)).

## Publish a release

In the release job, name the project and where its files will be, upload the files, run the action, then upload the bundle it wrote:

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

- `project`: the domain name.
- `url-base`: this release's directory on your host. The statement records each artifact's and asset's URL under it.
- `artifacts`, `bin`, and `resources`: as for any release; see [Publish with GitHub Actions](/docs/publishing/).

The action uploads nothing to your host, so the upload steps copy the artifacts, resource assets, and bundle there. It still attaches the bundle to the GitHub release. A forge project that sets `url-base` keeps that upload, because consumers find its bundles on its GitHub releases. A domain project's consumers read the copy the list names, so the attached one serves only people reading the release; set `upload: false` to skip it.

Give the storage credentials only to the upload steps. `contents: write` still reaches every step, the action included; to keep it from the action, split the job as [Keep the action away from release write access](/docs/publishing/#keep-the-action-away-from-release-write-access) shows.

Upload the bundle last: the next list run, scheduled or not, lists every bundle in the bucket, so everything a bundle describes must be in place first.

## Build and publish the list

The `jdx/packslip/releases` action turns a directory of published bundles, laid out as `<dir>/<tag>/packslip.sigstore.json`, into a signed list. It verifies every bundle under this repository's identity prefix, `https://github.com/owner/mytool/`, and refuses one for another project or in the wrong tag directory. Then it builds, signs, and verifies the list. It always signs keylessly with the job's identity; to sign with a key, run `packslip releases --key` as [Create the list](/docs/release-lists/#create-the-list) shows.

Put the list job in its own workflow, so the release workflow, a weekly schedule, a merged withdrawal, and a person can all run it. A run the release workflow calls still signs as this file, so every list has the same signer ([Signer continuity and workflow pinning](/docs/trust/#signer-continuity-and-workflow-pinning)).

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

- A missing withdrawal file fails the run; an empty one passes no entries.
- The fetch step copies only the bundles from the bucket.
- The action's sequence defaults to the current Unix time and its validity to 30 days; see [Choose a sequence](/docs/release-lists/#choose-a-sequence).
- [Release list action](/docs/actions/#release-list-action) lists every input and output.

## Keep the list current

Call the workflow from the release workflow too, once the bundle is uploaded, passing only the two storage secrets it declares:

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

A weekly run against a 30-day validity leaves room for a few failed runs. Keep the schedule running: consumers refuse an expired list, and with it the project; see the warning in [Decide whether you need a list](/docs/release-lists/#decide-whether-you-need-a-list).

The concurrency group allows one list run at a time. Run order is not guaranteed, and a new pending run cancels the existing pending run. Every run lists every bundle in the bucket, so a cancelled run loses nothing.

### Keep withdrawals in the repository

Every run builds the list from scratch, with the withdrawals and security fixes in `.github/mytool/yanked` and `.github/mytool/security` on the default branch. To change them, edit the files as [Keep withdrawals in the repository](/docs/release-lists/#keep-withdrawals-in-the-repository) shows and merge; the `push` trigger publishes the new list. Keep the bundle of every release these files name in the bucket, because each entry must name a release in `dir`.

## Rename the repository

After a rename or transfer, the repository's workflows sign as the new name, which the old identity prefix, such as `https://github.com/owner/old-mytool/`, does not cover. Consumers holding the old prefix refuse the next list, and so the project. The list job pins the current name, so it fails on every bundle signed under the old one. A change of letter case alone is not a rename: GitHub names match in any case. Users who already installed the tool with `packslip install` approve two trust changes, one for the list and one for the next release. Each takes the `--accept-trust-change` value install prints for it ([Release lists are pinned separately](/docs/trust/#release-lists-are-pinned-separately)).

Treat the rename as one change. To move `owner/old-mytool` to `owner/mytool`:

1. Rename the repository.
2. Describe each release you keep again from the renamed repository, with the job in [Describe a release under the new name](/docs/move-to-domain/#describe-a-release-under-the-new-name), changed in three places:
   - Download the files from the bucket, not the GitHub release, with `aws s3 cp "s3://releases/mytool/${TAG}/" dist/ --recursive` and the storage credentials in the step's environment.
   - Verify the original bundle with `--issuer https://token.actions.githubusercontent.com --identity-prefix https://github.com/owner/old-mytool/`: its domain name implies no policy, and it was signed under the old name.
   - Upload only the new bundle; drop the `aws s3 cp dist/` command, since the files are already on the host.
3. Delete from the bucket the bundles of releases you did not describe again. Remove any line naming one of them from `.github/mytool/yanked` and `.github/mytool/security`, or the list run fails.
4. Purge the old bundles from every cache in front of the host. Each new bundle replaces a file served as immutable, and a cached old bundle fails the new list's digest.
5. Run the list workflow.
6. Once the new list is out, and not before, change the identity prefix you publish and any registry entry that copies it, such as mise's. A consumer holding the new prefix refuses the old list. The signer fingerprint does not change, so consumers who verify with `--issuer` and `--pin` keep working.

## Troubleshoot

| Symptom | What to check |
| --- | --- |
| `no identity to verify against` | The project is not on a forge, so no signer policy is implied. Run `packslip verify` with `--issuer` and either `--identity-prefix` or `--pin` ([Publish the signer](#publish-the-signer) lists the values), or with `--pubkey`. The actions pass their own trust flags. |
| `no packslip.sigstore.json under lists/<tag>/; the action does not build an empty list` | Nothing is under `<dir>/<tag>/`. Check the sync, or backfill a release with [Describe a release under the new name](/docs/move-to-domain/#describe-a-release-under-the-new-name). |
| `lists/v1.2.4/packslip.sigstore.json is release v1.2.3 but sits under v1.2.4/` | Move the bundle to the directory named by the tag it records. |
| `https://mytool.example.com/v1.2.3/packslip.sigstore.json is not among the --release entries` | A `yank` or `security` entry names a tag or URL that is not in `dir`. Restore that release's bundle, or remove the entry. |
| A withdrawn release is eligible again after a scheduled or post-release run | The withdrawal was given to a single run. Commit it, as [Keep withdrawals in the repository](/docs/release-lists/#keep-withdrawals-in-the-repository) shows, so every run passes it. |
| `signed by "https://github.com/owner/old-mytool/...", expected an identity starting with "https://github.com/owner/mytool/"` | The repository was renamed or transferred after that release was signed, and the action pins the current name. Leave the bundle out of `dir`: delete it from the bucket or exclude it from the sync. To keep the release listed, follow [Rename the repository](#rename-the-repository). |
| `host project has no signed release list` | Nothing is served at `/.well-known/packslip.json`. Check that the host publishes `.well-known` and that the list workflow has run. |
| `release list is stale, missing, or rolled back`, or another consumer says the list expired | The scheduled run has not published one lately; GitHub disables a public repository's schedules after 60 days without activity. Check that the workflow is enabled, then dispatch it once by hand. |
