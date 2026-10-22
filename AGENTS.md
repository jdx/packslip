# Rules for coding agents

Read [CONTRIBUTING.md](CONTRIBUTING.md) before you change files. It says
where each kind of change belongs and what CI checks.

- **Never edit generated files.** `mise run render` writes
  `content/spec.md`, `content/cli/`, `content/docs/actions.md`,
  `packslip.1`, `packslip.usage.kdl`, and `static/schema/`. Edit their
  sources, run it, and commit the regenerated files with the source
  change. Workflows write `action/release.json`, `data/github.json`, and
  each release's `CHANGELOG.md` entry. Edit `CHANGELOG.md` by hand only
  when
  [publishing a version release-plz would not propose](RELEASING.md#publishing-a-version-release-plz-would-not-propose).
  Update `mise.lock` only with `mise lock`.
- **Treat spec text as normative.** Before you open a pull request that
  edits `docs/spec/packslip.md`, even editorially, check every sentence,
  original and new, for a change of meaning. Repeat until a pass finds no
  unintended change.
- **Title each pull request as a conventional commit and write its
  description as release notes**, following
  [Submit a pull request](CONTRIBUTING.md#submit-a-pull-request). Never
  fabricate screenshots, output, measurements, or validation results.
- **Use `fix` or `feat` only for a change that warrants a release.** A
  merged `fix` or `feat` title, with any scope or `!`, makes the release
  pull request eligible for the daily auto-merge. This holds even for a
  change outside the Cargo package, such as `fix(ci): ...`. The 10:00 UTC
  run then queues the release pull request once the last release is at
  least two days old. GitHub merges it when `final` passes, shipping
  everything pending. Give any other change another type, such as `ci`,
  `docs`, or `chore`. No other type, `security` included, triggers the
  auto-merge.
- **Never create, move, or push tags, and never push to the `release-plz`
  branch**, unless a maintainer asks you to run a
  [RELEASING.md](RELEASING.md) procedure that does. Then follow that
  procedure exactly.
- **Keep a vulnerability's details out of public issues, pull requests,
  commits, and discussions until its advisory is published.** This covers
  the fix itself: releases are built from `main`, so the fix's title,
  description, and commit messages are public before its release ships.
  Follow [SECURITY.md](SECURITY.md).
