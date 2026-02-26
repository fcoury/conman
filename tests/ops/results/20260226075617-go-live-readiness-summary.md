# Go-Live Readiness Check

- Generated at: `2026-02-26T07:56:17Z`
- Strict mode: `0`
- Pass: `5`
- Warn: `1`
- Fail: `0`

| Check | Result | Notes |
|---|---|---|
| Staged full-flow smoke evidence | pass | `20260226044825-full-e2e-summary.md` found. |
| Mongo backup/restore drill evidence | pass | `20260226044657-mongo-backup-restore-summary.md` found. |
| Observability wiring evidence | pass | `20260226044706-observability-wiring-summary.md` found. |
| Secrets rotation runbook | pass | `docs/runbooks/secrets-master-key-rotation.md` present. |
| Secrets master key configured | pass | `CONMAN_SECRETS_MASTER_KEY` is set in the current environment. |
| Runbook owner sign-off | warn | Incomplete checklist in `docs/runbooks/REVIEW-SIGNOFF.md`. |
