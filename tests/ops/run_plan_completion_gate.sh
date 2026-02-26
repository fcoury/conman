#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TRACKER="$ROOT/docs/execution-tracker.md"
OPS_RESULTS_DIR="$ROOT/tests/ops/results"
STRICT=0

if [[ "${1:-}" == "--strict" ]]; then
  STRICT=1
fi

mkdir -p "$OPS_RESULTS_DIR"
STAMP="$(date -u +%Y%m%d%H%M%S)"
SUMMARY_FILE="$OPS_RESULTS_DIR/${STAMP}-plan-completion-gate-summary.md"

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

has_pattern() {
  local pattern="$1"
  local file="$2"
  if command -v rg >/dev/null 2>&1; then
    rg -q -- "$pattern" "$file"
  else
    grep -Eq -- "$pattern" "$file"
  fi
}

require_match() {
  local label="$1"
  local pattern="$2"
  local file="$3"
  if has_pattern "$pattern" "$file"; then
    record_pass "$label" "\`$(basename "$file")\` matches \`$pattern\`."
  else
    record_fail "$label" "\`$(basename "$file")\` is missing expected pattern \`$pattern\`."
  fi
}

require_match "Epics complete ratio" 'Epics complete:\s*`12 / 12`' "$TRACKER"
require_match "Gates passed ratio" 'Gates passed:\s*`5 / 5`' "$TRACKER"
require_match "Final sign-off checked" '^\- \[x\] Final sign-off \(names/date\)$' "$TRACKER"

if [[ -n "${CONMAN_SECRETS_MASTER_KEY:-}" ]]; then
  record_pass "Secrets key env available" "\`CONMAN_SECRETS_MASTER_KEY\` is set."
else
  record_warn "Secrets key env available" "\`CONMAN_SECRETS_MASTER_KEY\` is not set."
fi

if (cd "$ROOT" && cargo test --workspace -q >/dev/null); then
  record_pass "cargo test --workspace" "Workspace tests passed."
else
  record_fail "cargo test --workspace" "Workspace tests failed."
fi

if (cd "$ROOT" && cargo clippy --workspace --all-targets -- -D warnings >/dev/null); then
  record_pass "cargo clippy --workspace" "No clippy warnings."
else
  record_fail "cargo clippy --workspace" "Clippy reported warnings/errors."
fi

if (cd "$ROOT" && ./scripts/build-docs-site.sh >/dev/null); then
  record_pass "docs site build" "\`scripts/build-docs-site.sh\` succeeded."
else
  record_fail "docs site build" "Docs site build failed."
fi

{
  echo "# Execution Gate Summary"
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

echo "Wrote execution summary: $SUMMARY_FILE"

if [[ "$FAIL_COUNT" -gt 0 ]]; then
  echo "Execution gate failed with $FAIL_COUNT hard failures."
  exit 1
fi

if [[ "$STRICT" -eq 1 && "$WARN_COUNT" -gt 0 ]]; then
  echo "Execution gate strict mode failed with $WARN_COUNT warnings."
  exit 2
fi

echo "Execution gate passed."
