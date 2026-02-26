#!/usr/bin/env bash
set -euo pipefail

STRICT=0
if [[ "${1:-}" == "--strict" ]]; then
  STRICT=1
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OPS_RESULTS_DIR="$ROOT/tests/ops/results"
E2E_RESULTS_DIR="$ROOT/tests/e2e/results"
SIGNOFF_FILE="$ROOT/docs/runbooks/REVIEW-SIGNOFF.md"
SECRETS_RUNBOOK="$ROOT/docs/runbooks/secrets-master-key-rotation.md"

mkdir -p "$OPS_RESULTS_DIR"
STAMP="$(date -u +%Y%m%d%H%M%S)"
SUMMARY_FILE="$OPS_RESULTS_DIR/${STAMP}-go-live-readiness-summary.md"

find_latest() {
  local pattern="$1"
  local dir="$2"
  find "$dir" -maxdepth 1 -type f -name "$pattern" -print | sort | tail -1
}

PASS_COUNT=0
FAIL_COUNT=0
WARN_COUNT=0
RESULTS=()

record_pass() {
  PASS_COUNT=$((PASS_COUNT + 1))
  RESULTS+=("| $1 | pass | $2 |")
}

record_fail() {
  FAIL_COUNT=$((FAIL_COUNT + 1))
  RESULTS+=("| $1 | fail | $2 |")
}

record_warn() {
  WARN_COUNT=$((WARN_COUNT + 1))
  RESULTS+=("| $1 | warn | $2 |")
}

FULL_E2E_SUMMARY="$(find_latest '*-full-e2e-summary.md' "$E2E_RESULTS_DIR")"
MONGO_DRILL_SUMMARY="$(find_latest '*-mongo-backup-restore-summary.md' "$OPS_RESULTS_DIR")"
OBS_WIRING_SUMMARY="$(find_latest '*-observability-wiring-summary.md' "$OPS_RESULTS_DIR")"

if [[ -n "$FULL_E2E_SUMMARY" ]]; then
  record_pass "Staged full-flow smoke evidence" "\`$(basename "$FULL_E2E_SUMMARY")\` found."
else
  record_fail "Staged full-flow smoke evidence" "No \`*-full-e2e-summary.md\` found in \`tests/e2e/results/\`."
fi

if [[ -n "$MONGO_DRILL_SUMMARY" ]]; then
  record_pass "Mongo backup/restore drill evidence" "\`$(basename "$MONGO_DRILL_SUMMARY")\` found."
else
  record_fail "Mongo backup/restore drill evidence" "No \`*-mongo-backup-restore-summary.md\` found in \`tests/ops/results/\`."
fi

if [[ -n "$OBS_WIRING_SUMMARY" ]]; then
  record_pass "Observability wiring evidence" "\`$(basename "$OBS_WIRING_SUMMARY")\` found."
else
  record_fail "Observability wiring evidence" "No \`*-observability-wiring-summary.md\` found in \`tests/ops/results/\`."
fi

if [[ -f "$SECRETS_RUNBOOK" ]]; then
  record_pass "Secrets rotation runbook" "\`docs/runbooks/secrets-master-key-rotation.md\` present."
else
  record_fail "Secrets rotation runbook" "\`docs/runbooks/secrets-master-key-rotation.md\` is missing."
fi

if [[ -n "${CONMAN_SECRETS_MASTER_KEY:-}" ]]; then
  record_pass "Secrets master key configured" "\`CONMAN_SECRETS_MASTER_KEY\` is set in the current environment."
else
  record_warn "Secrets master key configured" "\`CONMAN_SECRETS_MASTER_KEY\` is not set in the current shell. Validate in production runtime env."
fi

if [[ -f "$SIGNOFF_FILE" ]]; then
  if rg -q '^- \[ \]' "$SIGNOFF_FILE"; then
    record_warn "Runbook owner sign-off" "Incomplete checklist in \`docs/runbooks/REVIEW-SIGNOFF.md\`."
  elif rg -q '^Date:\s*$' "$SIGNOFF_FILE" || rg -q '^Reviewer:\s*$' "$SIGNOFF_FILE"; then
    record_warn "Runbook owner sign-off" "Checklist complete but date/reviewer metadata missing."
  else
    record_pass "Runbook owner sign-off" "Runbook sign-off file is complete."
  fi
else
  record_fail "Runbook owner sign-off" "\`docs/runbooks/REVIEW-SIGNOFF.md\` is missing."
fi

{
  echo "# Go-Live Readiness Check"
  echo
  echo "- Generated at: \`$(date -u +"%Y-%m-%dT%H:%M:%SZ")\`"
  echo "- Strict mode: \`$STRICT\`"
  echo "- Pass: \`$PASS_COUNT\`"
  echo "- Warn: \`$WARN_COUNT\`"
  echo "- Fail: \`$FAIL_COUNT\`"
  echo
  echo "| Check | Result | Notes |"
  echo "|---|---|---|"
  printf '%s\n' "${RESULTS[@]}"
} > "$SUMMARY_FILE"

echo "Wrote readiness summary: $SUMMARY_FILE"

if [[ "$FAIL_COUNT" -gt 0 ]]; then
  echo "Readiness check failed with $FAIL_COUNT hard failures."
  exit 1
fi

if [[ "$STRICT" -eq 1 && "$WARN_COUNT" -gt 0 ]]; then
  echo "Readiness check strict mode failed with $WARN_COUNT warnings."
  exit 2
fi

echo "Readiness check completed."
