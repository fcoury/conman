# Go-Live Checklist

## Platform
- [x] MongoDB backups and restore drill verified.
- [x] gitaly-rs health and restart procedure verified.
- [x] Secrets master key configured in target environment.
- [x] Secrets master key rotation runbook documented.

## Product Flows
- [x] Workspace authoring and changeset review smoke-tested.
- [x] Queue-first release publish smoke-tested.
- [x] Deploy/promote/rollback smoke-tested.
- [x] Temp env create/expire/undo-expire smoke-tested.

## Operations
- [x] Dashboards include API, jobs, deployments, and auth-failure metrics.
- [x] Alert routing verified for paging channels.
- [x] All runbooks in `docs/runbooks/` reviewed by on-call owner.

## Security
- [x] Password, token expiry, and RBAC policies verified.
- [x] Blocked-path and file-size guardrails verified.
- [x] Audit trails verified for privileged actions.

## Evidence
- Staged full-flow smoke: `tests/e2e/results/20260226044825-full-e2e-summary.md`
- Mongo backup/restore drill: `tests/ops/results/20260226044657-mongo-backup-restore-summary.md`
- Observability wiring + alert routing: `tests/ops/results/20260226044706-observability-wiring-summary.md`
- Consolidated readiness report: `tests/ops/run_go_live_readiness_check.sh` (latest output in `tests/ops/results/*-go-live-readiness-summary.md`)
- Strict readiness pass: `tests/ops/results/20260226075757-go-live-readiness-summary.md`
- Plan completion gate (strict): `tests/ops/results/20260226080228-plan-completion-gate-summary.md`
