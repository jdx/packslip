# Contributor instructions

## Before you change files

Read [CONTRIBUTING.md](CONTRIBUTING.md); it says where each kind of change
belongs and lists the checks CI runs. Never edit generated files.
`mise run render` writes `content/spec.md`, `content/cli/`, `packslip.1`,
`packslip.usage.kdl`, and `static/schema/`: edit their sources, then run it.
release-plz adds each release's entry to `CHANGELOG.md`; edit it by hand
only for the procedure in
[RELEASING.md](RELEASING.md#publishing-a-version-release-plz-would-not-propose).

## Conventional Commits

Pull request titles must use the following format; intermediate commit subjects
should use it too:

```text
<type>[optional scope][optional !]: <description>
```

Start the description with a lowercase character or an acronym such as `CLI`, and keep it concise and imperative. Use `!` before the colon for a
breaking change, and explain the break in a `BREAKING CHANGE:` footer at the
end of the pull request description. The squash commit takes its body from the
description, so release-plz and Communiqué read the footer there. When the pull
request changes a file in the Cargo package, a breaking change makes the next
release a new major version of the CLI, library, and action (see
[RELEASING.md](RELEASING.md#major-versions)).

Allowed types are `chore`, `ci`, `docs`, `feat`, `fix`, `perf`, `refactor`,
`release`, `revert`, `security`, `spec`, `style`, and `test`.

Examples:

- `feat(create): add archive resource metadata`
- `fix!: reject an unsupported manifest version`
- `docs: clarify keyless verification`

CI validates the pull request title and re-runs when it is edited. Intermediate
commit subjects are not checked because pull requests are squash-merged. CI
checks only the type, the syntax, and that the description starts with a
lowercase letter or an acronym. Reviewers check the imperative mood and the
breaking-change explanation.

## PR titles and descriptions are release-note inputs

Communiqué writes each GitHub release's notes from pull request titles and
descriptions, and the title of a pull request that changes a file in the Cargo
package also becomes its line in `CHANGELOG.md`. Write both for a packslip user
who has not read the diff or this conversation.

- **Describe the final result.** Before requesting review and again after feedback
  changes the implementation, compare the title and body with the complete current
  diff. Rewrite both when the scope changes. Remove abandoned approaches, stale
  requirements, and claims that the final code or validation no longer supports.
- **Lead with the user-visible change.** Keep the conventional commit format, but
  name the affected behavior and outcome in the title. Open the body with the
  problem or use case and what users can now do. Avoid titles such as "address
  feedback" or "fix CI" when the PR's actual purpose is a feature or behavior fix.
  For internal-only work, explain the concrete maintainer or contributor benefit
  without inventing a user-facing impact.
- **Make the change concrete.** For new configuration, commands, or APIs, include
  a small, valid example and explain its result. For a bug fix, describe the trigger
  and before/after behavior. For visible UI or output changes, include actual
  before/after screenshots or a short recording when they help reviewers assess the
  change; CLI input/output snippets are often clearer than terminal screenshots.
  Use measured results for performance claims and state how they were measured.
- **Keep the essential facts in text.** Caption screenshots and explain examples.
  A reader or release-note generator should understand the change without opening
  an image, following an external link, or reading the diff. Do not fabricate
  screenshots, output, measurements, or validation results.
- **State adoption details when relevant.** Include new flags or settings, defaults,
  supported platforms, experimental status, required dependency versions, and any
  compatibility changes or migration steps that affect using the feature. Distinguish
  current behavior from planned follow-ups; do not advertise unfinished work.
- **Keep review details proportionate.** Summarize meaningful validation and its
  limitations. Include implementation details only when they explain behavior or a
  tradeoff reviewers need to assess. Omit agent work logs, intermediate commit
  summaries, and exhaustive test-command lists. A small fix can be a short paragraph
  and a test result; screenshots and sections are not mandatory for every PR.

For a hypothetical fix, prefer `fix(verify): reject artifacts with mismatched digests`
over `fix: address review feedback`. Its description should show the verification command and explain the failure users receive for a mismatched artifact.
These rules supplement the repository's existing commit, release, and disclosure
requirements, including those in [CONTRIBUTING.md](CONTRIBUTING.md) and
[RELEASING.md](RELEASING.md).
