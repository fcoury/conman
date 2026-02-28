# Operations Validation Artifacts

This directory contains operational validation scripts and their generated
evidence artifacts.

- `run_plan_completion_gate.sh`:
  single gate for implementation completion that checks tracker status, executes
  tests + clippy + docs build, and writes a timestamped summary report. Use
  `--strict` to fail on warnings.
- `run_team_repo_app_acceptance.sh`:
  API-level acceptance checks for the team/repository/app direction,
  including `/api/repos` flows and runtime profile
  `app_endpoints` persistence.
  Typical invocation:
  `CONMAN_BASE_URL=http://127.0.0.1:3001 CONMAN_LOGIN_EMAIL=... CONMAN_LOGIN_PASSWORD=... CONMAN_ACCEPTANCE_REPO_PATH=... ./tests/ops/run_team_repo_app_acceptance.sh --strict`

Results are written to `tests/ops/results/`.
