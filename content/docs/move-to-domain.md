---
title: Move a GitHub project to your domain
description: Describe a github.com project's releases again under a domain name, list them on your host, and switch consumers to the new name.
group: publish
weight: 46
---
# Move a GitHub project to your domain

For publishers moving a tool from `github.com/owner/mytool` to a domain name, such as `mytool.example.com`: at the end, your host lists the current release under the new name, and consumers know what to change.
Set up the host, the release job, and the list workflow first with [Host releases on your own domain](/docs/self-hosting/).

## Before you start

A release bundle names one project, so bundles published as `github.com/owner/mytool` cannot go in `mytool.example.com`'s list. The releases action also refuses to build an empty list from an empty bundle directory. Before switching consumers to the domain name, describe at least the current release again under the new name.

Describe it in a dispatched job of the workflow file that signs new releases, so that consumers see one signer; see [Signer continuity and workflow pinning](/docs/trust/#signer-continuity-and-workflow-pinning).

## Describe a release under the new name

1. Add a `workflow_dispatch` trigger with a `tag` input to the release workflow, and a job that runs only on dispatch:

   ```yaml
   # .github/workflows/release.yml
   on:
     push:
       tags: ["v*"] # Your existing trigger.
     workflow_dispatch:
       inputs:
         tag:
           description: The release to describe under the new name, such as v1.2.3
           required: true

   jobs:
     release:
       if: github.event_name != 'workflow_dispatch'
       # The release job from Host releases on your own domain.

     redescribe:
       if: github.event_name == 'workflow_dispatch'
       runs-on: ubuntu-latest
       permissions:
         contents: read   # Download the release's files.
         id-token: write  # Sign with the workflow's identity.
       env:
         TAG: ${{ inputs.tag }}
       steps:
         - name: Install packslip
           run: |
             curl -fsSL https://packslip.sh/vX.Y.Z | sh
             echo "$HOME/.local/bin" >> "$GITHUB_PATH"
         - name: Download and check the release's files
           id: release
           env:
             GH_TOKEN: ${{ github.token }}
           run: |
             set -euo pipefail
             gh release download "$TAG" --repo "$GITHUB_REPOSITORY" --dir dist
             # Bundles name github.com/owner/mytool, and an artifacts glob
             # such as dist/* would take them for artifacts. Keep the
             # release's bundle to check the files against; delete any other.
             mv dist/packslip.sigstore.json original.sigstore.json
             rm -f dist/packslip*.sigstore.json
             # Sign again only the files the original bundle describes.
             shopt -s nullglob
             args=()
             for file in dist/*.tar.gz dist/*.zip dist/mytool.usage.kdl; do
               args+=(--artifact "$file")
             done
             packslip verify original.sigstore.json "${args[@]}"
             echo "commit=$(gh api "repos/$GITHUB_REPOSITORY/commits/$TAG" --jq .sha)" >> "$GITHUB_OUTPUT"
         - uses: jdx/packslip@v1
           id: packslip
           with:
             project: mytool.example.com
             tag: ${{ inputs.tag }}
             url-base: https://mytool.example.com/${{ inputs.tag }}
             commit: ${{ steps.release.outputs.commit }}
             attest: link
             upload: false
             artifacts: dist/*.tar.gz dist/*.zip
             bin: mytool
             resources: cli-spec/usage=asset:dist/mytool.usage.kdl
         - name: Upload the files, then the new bundle
           env:
             BUNDLE: ${{ steps.packslip.outputs.bundle }}
             AWS_ACCESS_KEY_ID: ${{ secrets.R2_ACCESS_KEY_ID }}
             AWS_SECRET_ACCESS_KEY: ${{ secrets.R2_SECRET_ACCESS_KEY }}
             AWS_REGION: auto
             AWS_ENDPOINT_URL: https://<account>.r2.cloudflarestorage.com
           run: |
             set -euo pipefail
             aws s3 cp dist/ "s3://releases/mytool/${TAG}/" --recursive \
               --cache-control "public, max-age=31536000, immutable"
             aws s3 cp "$BUNDLE" "s3://releases/mytool/${TAG}/packslip.sigstore.json" \
               --content-type application/json --cache-control "public, max-age=31536000, immutable"
   ```

   - The install step puts the packslip CLI on PATH for the download step's check, with a [versioned install script](/docs/install/#pin-a-versioned-install-script). Replace `X.Y.Z` with the release to run.
   - The download step fetches the release's files from GitHub into `dist/` and looks up the tag's commit. It sets the release's bundle aside and deletes every other `packslip*.sigstore.json` from `dist/`, so none is uploaded or signed as an artifact.
   - The download step then verifies the files that `artifacts` and `resources` name against the old bundle. A file that was replaced on the GitHub release after publication stops the job before anything is signed. The old bundle's project name implies the signer policy, so `packslip verify` needs no trust flags.
   - `project`: the new name.
   - `tag`: the release's tag. A dispatched run has no tag of its own.
   - `url-base`: the release's tag directory on your host, such as `https://mytool.example.com/v1.2.3`.
   - `commit`: the tag's full commit SHA. The default is the commit the dispatch ran on.
   - `attest: link`: link the build provenance the original release job attested, instead of attesting again. Use `false` if the original release was not attested.
   - `upload: false`: without it, the action tries to attach the new bundle to the GitHub release. With `contents: read` that step fails. With write access it would replace the original `packslip.sigstore.json`, since both bundles have that name.
   - `artifacts`, `bin`, and `resources`: the release job's values, reading the files from `dist/`.
   - Add `version` only if the tag is not `v` followed by the version.
   - The upload step copies the files and then the new bundle to the release's tag directory, `s3://releases/mytool/<tag>/`, with the commands from [Publish a release](/docs/self-hosting/#publish-a-release). In a dispatched job, `GITHUB_REF_NAME` is the branch, so the paths use the `tag` input instead.

2. Run the job for the current release:

   ```sh
   gh workflow run release.yml -f tag=v1.2.3
   ```

   Repeat it for each older release you want to offer under the new name. A release you do not describe again cannot be installed as `mytool.example.com`.

3. When the runs succeed, run the list workflow:

   ```sh
   gh workflow run packslip-releases.yml
   ```

The new bundle is logged when it is signed. A consumer that enforces a minimum release age therefore treats the release as new until that age has passed.

## Check the new name

Before you tell consumers to switch, fetch the list from your host and verify it under the signer you publish:

```sh
curl -fsSLo list.json https://mytool.example.com/.well-known/packslip.json
packslip verify list.json \
  --issuer https://token.actions.githubusercontent.com \
  --identity-prefix https://github.com/owner/mytool/
```

The command prints an `ok: release list for mytool.example.com` line with the list's sequence, expiry, signer, and number of releases. Check that every release you described again is counted. Then run `packslip install mytool.example.com` on a scratch machine to install the current release under the new name.

## Switch consumers to the new name

The bundle attached to each existing GitHub release keeps naming `github.com/owner/mytool` and stays valid for anyone reading it there. Bundles for new releases name `mytool.example.com`. A consumer still asking for `github.com/owner/mytool` keeps what it installed, but it cannot install any release published after the move.

To consumers, the domain name is a new project with no signer history. Tell them to change two things:

1. The name they ask for. In mise, `packslip:github.com/owner/mytool` becomes `packslip:mytool.example.com`.
2. The signer they pin. Use the issuer and identity prefix from [Publish the signer](/docs/self-hosting/#publish-the-signer). The signer fingerprint stays the same, because the same repository signs, so a `--pin ps1_…` value carries over. `packslip install` without `--pin` takes the list's signer and the releases' signer from your host on first use, as [First use and signer fingerprints](/docs/trust/#first-use-and-signer-fingerprints) describes. [What each consumer enforces](/docs/trust/#what-each-consumer-enforces) covers the rest.

If `github.com/owner/mytool` has a supplementary release list, keep re-signing it while consumers still use the old name.

## Troubleshoot

| Symptom | What to check |
| --- | --- |
| `verification failed: artifact …: sha256 is …, document says …` | That file on the GitHub release differs from the one the original bundle describes. Restore the original file, then run the job again. |
| `verification failed: artifact …: not listed in the document` | The download step's patterns match a file that the original bundle does not describe. Narrow the patterns, and the action's `artifacts` and `resources`, to the files the release describes. |
| `PATH: not a packslip bundle: it is for github.com/owner/mytool, the list is for mytool.example.com` | A bundle from before the move is in the list job's directory. Describe that release again under the new name, or remove the bundle. |
| `trust change …` naming `signer changed from`, or another consumer refuses a re-described release as a different signer | The release was described from another workflow file. Describe it again from the file that signs releases. A consumer that already took its pin from the other file approves the change once; `packslip install` prints the `--accept-trust-change` value to pass. |
