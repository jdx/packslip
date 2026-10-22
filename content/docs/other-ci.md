---
title: Publish from other CI
description: Sign a release bundle with packslip create from GitLab CI or with your own key, and publish it beside your files.
group: publish
weight: 32
---
# Publish from other CI

Use this guide when your release job runs outside GitHub Actions or signs
with your own key. At the end, your job signs a release bundle with
`packslip create`, and you know what to publish beside it.

To sign keylessly on GitHub Actions, follow
[Publish with GitHub Actions](/docs/publishing/) instead. The APT and RPM
packages leave out the publishing commands;
[install packslip another way](/docs/install/#choose-a-method).

## Choose how to sign

`packslip create` signs keylessly with the CI job's OIDC identity,
certified by Sigstore, or with an Ed25519 key from `packslip keygen`.
Prefer keyless in GitHub Actions and GitLab CI: there is no secret key to
protect. Other CI systems may need a key
([Sign keylessly](#sign-keylessly)). The npm package
([Verify from JavaScript](/docs/verify-js/)) and the committable verifier
([Commit a verifier into your project](/docs/verify-script/)) refuse
key-signed bundles. [Two ways to sign](/docs/trust/#two-ways-to-sign)
explains how consumers hold each kind of signer.

## Choose a project name

A forge project's name implies its signer: GitHub Actions or GitLab CI in
that repository
([What a project name says about the signer](/docs/trust/#what-a-project-name-says-about-the-signer)).
Only GitHub Actions in `owner/mytool` can sign a `github.com/owner/mytool`
project.

A `gitlab.com/group/mytool` project is verify-only. Version 1 of the
format defines no discovery for `gitlab.com`
([Discovery](/release/v1/#discovery)), so `packslip install` refuses it:

```text
bootstrap forge discovery currently supports github.com; other forges require a host-named project
```

For any other signer, or to let people install a GitLab-signed tool,
name the project after a domain you control, such as `mytool.example.com`.
Consumers discover its releases through a release list served from that
domain ([Publishing a list](/release/v1/#publishing-a-list)). Artifacts
and bundles can stay anywhere their URLs point, including GitLab.

## Sign keylessly

By default, `packslip create` signs with an OIDC token for the audience
`sigstore`: `SIGSTORE_ID_TOKEN` when it is set, or else the CI job's own
identity, such as a GitHub Actions job with `id-token: write`. Outside
GitHub Actions, put the job's token in `SIGSTORE_ID_TOKEN`. Sigstore's
public Fulcio service must accept its issuer.

`packslip create` declares the token's identity in the statement without
checking that Fulcio certifies the same one. Consumers refuse the bundle
when the two differ:

| Token from | Identity the statement declares | Bundle verifies |
| --- | --- | --- |
| GitHub Actions | The workflow file and ref, such as `https://github.com/owner/mytool/.github/workflows/release.yml@refs/tags/v1.2.3` | Yes |
| GitLab CI | The pipeline file and ref, such as `https://gitlab.com/group/mytool//.gitlab-ci.yml@refs/tags/v1.2.3` | Yes |
| Any other issuer | The token's verified email, or else its subject | Only when Fulcio certifies that value, as for an `accounts.google.com` email |

Buildkite and CircleCI certificates name a pipeline URL instead, so
[sign those releases with a key](#sign-with-a-key).

Verify each bundle in the job, with the issuer and identity you expect,
before you publish it. `packslip create` prints the identity it declared
in its `signed by` line.

Sign every release from the same pipeline or workflow file. Consumers
compare it with the last release they accepted
([Workflow pinning](/release/v1/#workflow-pinning)).

### Sign from GitLab CI

This job signs a domain project in a tag pipeline, then verifies the
result:

```yaml
release:
  stage: deploy
  rules:
    - if: $CI_COMMIT_TAG
  id_tokens:
    SIGSTORE_ID_TOKEN:
      aud: sigstore
  script:
    - export PATH="$HOME/.local/bin:$PATH"
    - curl -fsSL https://packslip.sh | sh
    - >
      packslip create
      --project mytool.example.com
      --version "${CI_COMMIT_TAG#v}"
      --source-repo "$CI_PROJECT_URL"
      --tag "$CI_COMMIT_TAG"
      --url-base "https://mytool.example.com/$CI_COMMIT_TAG"
      --bin mytool
      --out packslip
      dist/*
    - >
      packslip verify packslip/packslip.sigstore.json
      --identity-prefix "https://gitlab.com/$CI_PROJECT_PATH//"
      --issuer https://gitlab.com
  artifacts:
    paths:
      - packslip/
```

- `id_tokens` puts an OIDC token for the `sigstore` audience in
  `SIGSTORE_ID_TOKEN`.
- The install script installs into `~/.local/bin` for a non-root user,
  and the `export` line puts that on PATH.
  [Run the install script](/docs/install/#run-the-install-script) lists
  the image's requirements and how to change the directory; a
  [versioned install script](/docs/install/#pin-a-versioned-install-script)
  holds packslip at one version.
- `--source-repo` and `--tag` optionally record the repository and tag the
  release was built from; `--tag` needs `--source-repo`.
- `dist/*` holds the built files, exactly as you will publish them.
- `--url-base` gives each file the download URL `PREFIX/FILENAME`. For
  per-artifact executables or host requirements, pass release.toml with
  `--manifest` ([Describe your artifacts](/docs/describing-releases/)).
- `packslip verify` checks the signature and that this GitLab project's
  pipeline signed. For a domain project, these identity flags are the
  whole policy.
- `artifacts` passes the bundle to the job that publishes your files.

For a `gitlab.com/group/mytool` project, pass that `--project`. Point
`--url-base` where the job uploads the files, such as the generic package
registry:
`"$CI_API_V4_URL/projects/$CI_PROJECT_ID/packages/generic/mytool/${CI_COMMIT_TAG#v}"`.
Drop the identity flags from `packslip verify`: the name implies the
signer, for consumers too.

## Sign with a key

Sign with a key when your job cannot sign keylessly or you want consumers
to pin a stable public key. Use a domain project: `packslip install`
refuses a key-signed forge project.

1. Generate the key pair once, on a machine you trust:

   ```sh
   packslip keygen --out release.key
   ```

   It prints a key ID that differs on every run:

   ```text
   wrote release.key and release.pub (key id 7BC66E2D6A2F031D)
   ```

   `release.key` is the secret key; `release.pub` is the public key, in
   minisign format. `packslip keygen` refuses to overwrite either file.

2. Store `release.key` as a protected, masked CI secret. Never commit it
   or print it in a log.

3. Publish `release.pub` through a channel consumers trust, separate from
   the releases, such as your website. A key-signed project has no signer
   fingerprint, so consumers pin this key with `--pubkey` on
   `packslip verify` and `packslip install`.

4. In the release job, write the secret to a file and sign with it:

   ```sh
   trap 'rm -f release.key' EXIT
   (umask 077 && printf '%s\n' "$PACKSLIP_KEY" > release.key)
   packslip create \
     --project mytool.example.com \
     --version 1.2.3 \
     --bin mytool \
     --url-base https://mytool.example.com/v1.2.3 \
     --key release.key \
     --out packslip \
     dist/mytool-1.2.3-linux-x64.tar.gz
   packslip verify packslip/packslip.sigstore.json --pubkey release.pub
   ```

   - Run these lines in one shell, so the `trap` deletes `release.key` on
     exit, even after a command fails.
   - `PACKSLIP_KEY` is the CI secret from step 2.
   - `--key release.key` signs with the local key instead of a CI OIDC
     identity.
   - The job can read the public `release.pub` from your repository.

5. Sign every release with the same key. A consumer refuses a release
   signed by a different key until a person approves the change.

Sign the release list with the same key; `packslip releases` takes the
same `--key` and `--no-log` flags.

### Decide whether to log the signature

Keyless signatures are always recorded in Rekor; `packslip create --key`
records them by default. `--no-log` skips the entry, so signing needs no
network access, but no public log records when you signed; consumers see
only the signed `published_at`. `packslip verify` and `packslip install`
then need `--allow-unlogged` with `--pubkey`.

Keep `--no-log` for air-gapped releases and local trials
([Unlogged bundles](/release/v1/#unlogged-bundles));
[Transparency log and unlogged bundles](/docs/trust/#transparency-log-and-unlogged-bundles)
says which consumers accept them.

## Link build provenance

Build provenance describes how an artifact was built. `packslip create`
links it but does not produce it. Produce it with your CI system's
tooling, then list the statements' URLs in the artifact's `provenance` key
in release.toml:

```toml
[[artifact]]
path = "dist/mytool-1.2.3-linux-x64.tar.gz"
provenance = ["https://example.com/provenance/mytool-1.2.3-linux-x64.intoto.jsonl"]
```

Or pass `--provenance FILENAME=URL` to `packslip create` once per
artifact to add the URL to that entry's list
([Set download URLs and provenance](/docs/describing-releases/#set-download-urls-and-provenance),
[release.toml reference](/docs/release-toml/)).

Keep linking provenance once you start. When a build both releases offer
has fewer provenance links than before, `packslip install` asks a person
to approve the new release ([consumer rule 3](/release/v1/#consumer-rules)).

## Publish the files

`packslip create` writes the signed bundle into `--out` and uploads
nothing. Your job then publishes:

1. Each artifact, and each separate file a resource names as an `asset`,
   at the URL the statement records (from `--url-base`, `--url`, or
   release.toml).
2. `packslip.sigstore.json`, beside the artifacts, such as at
   `https://mytool.example.com/v1.2.3/packslip.sigstore.json`.
3. For a domain project, a new signed release list at
   `https://mytool.example.com/.well-known/packslip.json` for every
   release, with a higher `--sequence` than the last
   ([Create the list](/docs/release-lists/#create-the-list)).

[Lay out the host](/docs/self-hosting/#lay-out-the-host) shows how to lay out and serve these files.

## Tell consumers what to pin

Publish what consumers pin where they can read it without trusting a
release, such as your README:

| Signer | Publish |
| --- | --- |
| GitLab CI or GitHub Actions, domain project | The issuer, the identity prefix you verify with, and a trusted release's signer fingerprint |
| GitLab CI, `gitlab.com` project | The signer fingerprint; the name implies the rest |
| An email Fulcio certifies | The issuer and the email; it has no signer fingerprint |
| A key | `release.pub` ([Sign with a key](#sign-with-a-key), step 3) |

`packslip pin` prints the signer fingerprint. For a domain project, pass
the identity your verify step uses:

```sh
packslip pin packslip/packslip.sigstore.json \
  --identity-prefix https://gitlab.com/group/mytool// \
  --issuer https://gitlab.com
```

For a `gitlab.com` project, leave the flags out.
[First use and signer fingerprints](/docs/trust/#first-use-and-signer-fingerprints)
explains what a pin establishes and how consumers first trust a domain
project.

## Troubleshoot signing

| Error | What to do |
| --- | --- |
| `identity token: Token error: failed to detect ambient credentials: GitLab CI detection error` | Add the `id_tokens` entry from [Sign from GitLab CI](#sign-from-gitlab-ci). |
| `identity token: Token error: failed to detect ambient credentials: Buildkite detection error`, or `CircleCI detection error` | No token came from `buildkite-agent` or the `circleci` CLI. Sign with `--key`. |
| `identity token: Token error: invalid JWT format`, or another `identity token:` error that names no CI system | `SIGSTORE_ID_TOKEN` is not an OIDC token (a JWT with a `sub` claim). Check what the job puts in it. |
| `no OIDC identity available: run inside a CI job with an id-token permission, or set SIGSTORE_ID_TOKEN` | No detected CI system supplies a token. Export one as `SIGSTORE_ID_TOKEN`, or sign with `--key`. |
| `keyless signatures are always logged; --no-log is for --key` | Remove `--no-log`, or sign with `--key`. |
| `verification failed: statement declares signer "…", the bundle was signed by "…"` | Fulcio certified a different identity from the token's ([Sign keylessly](#sign-keylessly)). Sign from GitHub Actions or GitLab CI, or with a key. |
| `verification failed: bundle has no transparency log entry; pass --allow-unlogged to accept one` | The bundle was signed with `--no-log`. Pass `--allow-unlogged` with `--pubkey`, or sign again without `--no-log`. |
