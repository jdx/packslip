# Contributor Instructions

## Conventional Commits

Pull request titles must use the following format; intermediate commit subjects
should use it too:

```text
<type>[optional scope][optional !]: <description>
```

Start the description with a lowercase character and keep it concise and imperative. Use `!` before the colon for a
breaking change and explain it in the commit body with a `BREAKING CHANGE:`
footer.

Allowed types are `chore`, `ci`, `docs`, `feat`, `fix`, `perf`, `refactor`,
`release`, `revert`, `security`, `spec`, `style`, and `test`.

Examples:

- `feat(create): add archive resource metadata`
- `fix!: reject an unsupported manifest version`
- `docs: clarify keyless verification`

CI validates the pull request title and re-runs when it is edited. Intermediate
commit subjects are not checked because pull requests are squash-merged. CI
mechanically checks the allowed type, syntax, and lowercase-leading description;
imperative mood and breaking-change details remain review rules.
