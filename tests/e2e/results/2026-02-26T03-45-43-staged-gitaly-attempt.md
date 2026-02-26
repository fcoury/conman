# Staged Gitaly Attempt (blocked)

Date: 2026-02-26

## Goal
Run full staged smoke against live `gitaly-rs` + gateway + Conman API.

## Result
Blocked at workspace file write.

Endpoint:
- `PUT /api/apps/:appId/workspaces/:workspaceId/files`

Observed error:
- HTTP `502`
- `git error: commit_files returned empty branch update`

## What was validated before failure
1. Conman booted with live gitaly adapter (`CONMAN_GITALY_ADDRESS=http://127.0.0.1:2305`).
2. Real repo created in gitaly and seeded via gateway HTTP push.
3. App creation and workspace creation succeeded against staged services.

## Interpretation
`UserCommitFiles` on current staged `gitaly-rs` returns no `branch_update` and does
not expose a resolvable branch head for Conman to consume in this flow. Conman now
contains adapter/handler fallbacks, but the staged backend still needs the RPC behavior
completed for end-to-end authoring writes.

## Evidence
- Conman log capture: companion `.log` file with the same timestamp.
