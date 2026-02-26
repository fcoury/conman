# Operations Validation Artifacts

This directory contains operational validation scripts and their generated
evidence artifacts.

- `run_mongo_backup_restore_drill.sh`:
  validates backup/restore by seeding a drill database, creating a compressed
  dump archive, restoring it, and checking data signatures.
- `run_observability_wiring_check.sh`:
  brings up the local observability stack, verifies Prometheus/Alertmanager/
  Grafana health, confirms alert rules and routing, and checks dashboard metric
  coverage.
- `run_go_live_readiness_check.sh`:
  verifies go-live evidence presence and reports any remaining human sign-off
  requirements (master-key runtime config and runbook owner review). Use
  `--strict` to fail on warnings.
- `complete_runbook_signoff.sh`:
  helper for on-call owners to complete `docs/runbooks/REVIEW-SIGNOFF.md`
  consistently (`date`, `reviewer`, and all runbook checkboxes).
- `run_plan_completion_gate.sh`:
  single gate for milestone completion that checks tracker/checklists,
  executes tests + clippy + docs build, and runs readiness checks; writes a
  timestamped summary report. Use `--strict` to fail on warnings.
- `run_tenant_repo_surface_acceptance.sh`:
  API-level acceptance checks for the tenant/repository/app-surface direction,
  including compatibility with `/api/apps` and runtime profile
  `surface_endpoints` persistence.
  Typical invocation:
  `CONMAN_BASE_URL=http://127.0.0.1:3001 CONMAN_LOGIN_EMAIL=... CONMAN_LOGIN_PASSWORD=... CONMAN_ACCEPTANCE_REPO_PATH=... ./tests/ops/run_tenant_repo_surface_acceptance.sh --strict`

Results are written to `tests/ops/results/`.
