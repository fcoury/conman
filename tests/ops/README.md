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

Results are written to `tests/ops/results/`.
