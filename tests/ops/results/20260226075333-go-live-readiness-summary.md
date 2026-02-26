# Go-Live Readiness Check

- Generated at: `2026-02-26T07:53:33Z`
- Strict mode: `0`
- Pass: `4`
- Warn: `2`
- Fail: `0`

| Check | Result | Notes |
|---|---|---|
| Staged full-flow smoke evidence | pass | `20260226044825-full-e2e-summary.md` found. |
| Mongo backup/restore drill evidence | pass | `20260226044657-mongo-backup-restore-summary.md` found. |
| Observability wiring evidence | pass | `20260226044706-observability-wiring-summary.md` found. |
| Secrets rotation runbook | pass | `docs/runbooks/secrets-master-key-rotation.md` present. |
| Secrets master key configured | warn | `CONMAN_SECRETS_MASTER_KEY` is not set in the current shell. Validate in production runtime env. |
| Runbook owner sign-off | warn | Incomplete checklist in `docs/runbooks/REVIEW-SIGNOFF.md`. |
