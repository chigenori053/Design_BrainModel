# DBM CLI Git Integration Safe Command Spec

Implemented scope for `DBM_CLI_GIT_INTEGRATION_SAFE_COMMAND_SPEC v1.0`.

## Allowed

- `dbm git status`
- `dbm git diff`
- `dbm git diff -- <workspace-relative-file>`
- `dbm git add <explicit-workspace-file>`
- `dbm git commit -m <message>` as confirmation preview only
- `dbm git commit -m <message> --confirm DBM_CONFIRM_GIT_COMMIT_V1`
- `dbm git push --dry-run`

## Rejected

- Ambiguous add scopes: `.`, `-A`, `--all`, globs
- Workspace escapes, including parent directory traversal and symlink escapes
- Missing files for path-scoped operations
- Commit without a non-empty explicit message
- Push without `--dry-run`
- Force push
- Reset, clean, checkout, restore, rebase, merge, branch deletion, tag deletion

All command output is structured JSON with `schema_version: "v1"`.
