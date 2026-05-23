# DBM_CLI_GITHUB_INTEGRATION_SAFE_COMMAND_SPEC v1.0

Implemented scope for safe GitHub command integration through the `gh` CLI.

## Supported Commands

Read-only commands execute immediately and return the DBM JSON output contract:

- `dbm github auth status`
- `dbm github repo view`
- `dbm github pr status`
- `dbm github pr view <number>`
- `dbm github pr diff <number>`
- `dbm github issue view <number>`
- `dbm github issue list`

`dbm gh ...` is accepted as an alias for `dbm github ...`.

## PR Create Flow

`dbm github pr create --title <title> --body <body> [--base <base>]` creates a preview only.
The preview writes `.dbm/pending_github_pr_create.json` and returns `confirmation_required`
with a confirmation token.

`dbm github pr create --confirm <token>` executes `gh pr create` only when the pending token,
current branch, HEAD commit, repository snapshot, and clean working tree check still match.

## Rejected Commands

Destructive or privilege-changing GitHub commands are rejected, including:

- `pr merge`
- `pr close`
- `issue close`
- `issue edit`
- `release ...`
- `repo edit`
- `secret ...`
- `workflow ...`

## Test Strategy

The late integration suite uses a fake `gh` binary injected through `PATH` so GitHub behavior is
verified without depending on a real authenticated GitHub environment.
