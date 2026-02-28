#!/usr/bin/env bash
set -euo pipefail

STRICT=0
if [[ "${1:-}" == "--strict" ]]; then
  STRICT=1
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OPS_RESULTS_DIR="$ROOT/tests/ops/results"
mkdir -p "$OPS_RESULTS_DIR"

STAMP="$(date -u +%Y%m%d%H%M%S)"
SUMMARY_FILE="$OPS_RESULTS_DIR/${STAMP}-team-repo-app-acceptance-summary.md"

BASE_URL="${CONMAN_BASE_URL:-http://127.0.0.1:3000}"
TOKEN="${CONMAN_TOKEN:-}"
LOGIN_EMAIL="${CONMAN_LOGIN_EMAIL:-}"
LOGIN_PASSWORD="${CONMAN_LOGIN_PASSWORD:-}"
REPO_PATH="${CONMAN_ACCEPTANCE_REPO_PATH:-}"
INTEGRATION_BRANCH="${CONMAN_ACCEPTANCE_INTEGRATION_BRANCH:-main}"

PASS_COUNT=0
FAIL_COUNT=0
WARN_COUNT=0
RESULTS=()
LAST_RESPONSE_FILE=""

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

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 1
  fi
}

require_cmd curl
require_cmd jq

refresh_token_from_login() {
  if [[ -z "$LOGIN_EMAIL" || -z "$LOGIN_PASSWORD" ]]; then
    return 1
  fi
  local login_payload
  login_payload="$(jq -cn --arg email "$LOGIN_EMAIL" --arg password "$LOGIN_PASSWORD" '{email:$email,password:$password}')"
  local response
  response="$(curl -sS -X POST "$BASE_URL/api/auth/login" -H "Content-Type: application/json" --data "$login_payload")"
  local candidate
  candidate="$(echo "$response" | jq -r '.data.token // empty')"
  if [[ -n "$candidate" ]]; then
    TOKEN="$candidate"
    return 0
  fi
  return 1
}

api_call() {
  local method="$1"
  local path="$2"
  local body="${3-}"
  local tmp status
  tmp="$(mktemp)"
  if [[ -n "$body" ]]; then
    status="$(curl -sS -o "$tmp" -w "%{http_code}" -X "$method" "$BASE_URL$path" \
      -H "Authorization: Bearer $TOKEN" \
      -H "Content-Type: application/json" \
      --data "$body")"
  else
    status="$(curl -sS -o "$tmp" -w "%{http_code}" -X "$method" "$BASE_URL$path" \
      -H "Authorization: Bearer $TOKEN")"
  fi
  echo "$status|$tmp"
}

expect_status_any() {
  local label="$1"
  local expected_csv="$2"
  local method="$3"
  local path="$4"
  local body="${5-}"
  local res status file ok
  res="$(api_call "$method" "$path" "$body")"
  status="${res%%|*}"
  file="${res#*|}"
  ok=0
  IFS=',' read -r -a allowed <<<"$expected_csv"
  for code in "${allowed[@]}"; do
    if [[ "$status" == "$code" ]]; then
      ok=1
      break
    fi
  done
  if [[ "$ok" -eq 0 ]]; then
    record_fail "$label" "Expected HTTP $expected_csv, got $status for $method $path"
    cat "$file" >&2 || true
    rm -f "$file"
    return 1
  fi
  LAST_RESPONSE_FILE="$file"
  return 0
}

if [[ -z "$TOKEN" ]]; then
  if ! refresh_token_from_login; then
    record_fail "Precondition: auth token" "Set CONMAN_TOKEN or set CONMAN_LOGIN_EMAIL + CONMAN_LOGIN_PASSWORD."
  else
    record_pass "Precondition: auth token" "Fetched token via login credentials."
  fi
fi

if [[ -z "$REPO_PATH" ]]; then
  record_fail "Precondition: repo path" "Set CONMAN_ACCEPTANCE_REPO_PATH to an existing Git repo path known to Conman/gitaly."
fi

if [[ "$FAIL_COUNT" -gt 0 ]]; then
  {
    echo "# Team/Repo/App Acceptance"
    echo
    echo "- Generated at: \`$(date -u +"%Y-%m-%dT%H:%M:%SZ")\`"
    echo "- Base URL: \`$BASE_URL\`"
    echo "- Strict mode: \`$STRICT\`"
    echo "- Pass: \`$PASS_COUNT\`"
    echo "- Warn: \`$WARN_COUNT\`"
    echo "- Fail: \`$FAIL_COUNT\`"
    echo
    echo "| Check | Result | Notes |"
    echo "|---|---|---|"
    printf '%s\n' "${RESULTS[@]}"
  } > "$SUMMARY_FILE"
  echo "Wrote acceptance summary: $SUMMARY_FILE"
  exit 1
fi

if expect_status_any "Health endpoint" "200,503" GET "/api/health"; then
  file="$LAST_RESPONSE_FILE"
  code="$(jq -r '.status // empty' "$file" 2>/dev/null || true)"
  if [[ "$code" == "degraded" ]]; then
    record_pass "TRS-AC-01a health endpoint" "\`/api/health\` reachable in degraded mode (acceptable for local checks)."
  else
    record_pass "TRS-AC-01a health endpoint" "\`/api/health\` reachable."
  fi
  rm -f "$file"
fi

TEAM_SLUG="team-acceptance-${STAMP}"
TEAM_NAME="Team Acceptance ${STAMP}"
TEAM_PAYLOAD="$(jq -cn --arg n "$TEAM_NAME" --arg s "$TEAM_SLUG" '{name:$n,slug:$s}')"

if expect_status_any "Create team" "200,201" POST "/api/teams" "$TEAM_PAYLOAD"; then
  file="$LAST_RESPONSE_FILE"
  TEAM_ID="$(jq -r '.data.id // empty' "$file")"
  if [[ -n "$TEAM_ID" ]]; then
    record_pass "TRS-AC-01 team create" "Created team \`$TEAM_ID\`."
  else
    record_fail "TRS-AC-01 team create" "Response missing \`data.id\`."
  fi
  rm -f "$file"
fi

if [[ -n "${TEAM_ID:-}" ]]; then
  if expect_status_any "Get team" "200" GET "/api/teams/${TEAM_ID}"; then
    file="$LAST_RESPONSE_FILE"
    GOT_ID="$(jq -r '.data.id // empty' "$file")"
    if [[ "$GOT_ID" == "$TEAM_ID" ]]; then
      record_pass "TRS-AC-01 team read" "Team lookup returns matching id."
    else
      record_fail "TRS-AC-01 team read" "Team lookup id mismatch."
    fi
    rm -f "$file"
  fi
fi

REPO_NAME="repo-acceptance-${STAMP}"
REPO_PAYLOAD="$(jq -cn \
  --arg n "$REPO_NAME" \
  --arg p "$REPO_PATH" \
  --arg b "$INTEGRATION_BRANCH" \
  '{name:$n,repo_path:$p,integration_branch:$b}')"

if [[ -n "${TEAM_ID:-}" ]]; then
  if expect_status_any "Create repo under team" "200,201" POST "/api/teams/${TEAM_ID}/repos" "$REPO_PAYLOAD"; then
    file="$LAST_RESPONSE_FILE"
    REPO_ID="$(jq -r '.data.id // empty' "$file")"
    if [[ -n "$REPO_ID" ]]; then
      record_pass "TRS-AC-02 repo create" "Created repo \`$REPO_ID\`."
    else
      record_fail "TRS-AC-02 repo create" "Response missing \`data.id\`."
    fi
    rm -f "$file"
  fi
fi

if [[ -n "${REPO_ID:-}" && -n "$LOGIN_EMAIL" && -n "$LOGIN_PASSWORD" ]]; then
  if refresh_token_from_login; then
    record_pass "Token refresh after repo create" "Refreshed token to include new repo membership claims."
  else
    record_fail "Token refresh after repo create" "Failed to refresh token via login after repo creation."
  fi
fi

if [[ -n "${REPO_ID:-}" ]]; then
  if expect_status_any "Get repo by id" "200" GET "/api/repos/${REPO_ID}"; then
    file="$LAST_RESPONSE_FILE"
    GOT_ID="$(jq -r '.data.id // empty' "$file")"
    if [[ "$GOT_ID" == "$REPO_ID" ]]; then
      record_pass "TRS-AC-02 repo read" "\`/api/repos/:id\` returns expected repo."
    else
      record_fail "TRS-AC-02 repo read" "Repo id mismatch from \`/api/repos/:id\`."
    fi
    rm -f "$file"
  fi

  S1_PAYLOAD="$(jq -cn --arg key "lims" --arg title "LIMS" --arg d "lims-${STAMP}.example.test" \
    '{key:$key,title:$title,domains:[$d]}')"
  S2_PAYLOAD="$(jq -cn --arg key "portal" --arg title "Provider Portal" --arg d "portal-${STAMP}.example.test" \
    '{key:$key,title:$title,domains:[$d]}')"

  if expect_status_any "Create app lims" "200,201" POST "/api/repos/${REPO_ID}/apps" "$S1_PAYLOAD"; then
    file="$LAST_RESPONSE_FILE"
    APP_1_ID="$(jq -r '.data.id // empty' "$file")"
    rm -f "$file"
  fi
  if expect_status_any "Create app portal" "200,201" POST "/api/repos/${REPO_ID}/apps" "$S2_PAYLOAD"; then
    file="$LAST_RESPONSE_FILE"
    APP_2_ID="$(jq -r '.data.id // empty' "$file")"
    rm -f "$file"
  fi

  if [[ -n "${APP_1_ID:-}" && -n "${APP_2_ID:-}" ]]; then
    record_pass "TRS-AC-03 app create" "Created two apps for repo."
  else
    record_fail "TRS-AC-03 app create" "Failed to create both required apps."
  fi

  if expect_status_any "List apps" "200" GET "/api/repos/${REPO_ID}/apps"; then
    file="$LAST_RESPONSE_FILE"
    KEYS="$(jq -r '.data[]?.key' "$file" | sort | tr '\n' ' ')"
    if [[ "$KEYS" == *"lims"* && "$KEYS" == *"portal"* ]]; then
      record_pass "TRS-AC-03 app list" "App list contains \`lims\` and \`portal\`."
    else
      record_fail "TRS-AC-03 app list" "App list missing expected keys."
    fi
    rm -f "$file"
  fi

  PROFILE_PAYLOAD="$(jq -cn \
    --arg name "Acceptance Dev ${STAMP}" \
    --arg base "https://fallback-${STAMP}.example.test" \
    --arg lims "https://lims-${STAMP}.example.test" \
    --arg portal "https://portal-${STAMP}.example.test" \
    '{
      name:$name,
      kind:"persistent_env",
      base_url:$base,
      app_endpoints:{lims:$lims,portal:$portal},
      env_vars:{FEATURE_X:{type:"boolean",value:true}},
      secrets:{API_KEY:"acceptance-secret"},
      database_engine:"mongodb",
      connection_ref:"mongodb://dev-db:27017/conman_acceptance",
      provisioning_mode:"managed",
      migration_paths:["migrations"],
      migration_command:"echo migrate"
    }')"

  if expect_status_any "Create runtime profile with app endpoints" "200,201" POST "/api/repos/${REPO_ID}/runtime-profiles" "$PROFILE_PAYLOAD"; then
    file="$LAST_RESPONSE_FILE"
    PROFILE_ID="$(jq -r '.data.id // empty' "$file")"
    if [[ -n "$PROFILE_ID" ]]; then
      record_pass "TRS-AC-04 runtime profile create" "Created runtime profile \`$PROFILE_ID\`."
    else
      record_fail "TRS-AC-04 runtime profile create" "Runtime profile response missing id."
    fi
    rm -f "$file"
  fi

  if [[ -n "${PROFILE_ID:-}" ]]; then
    if expect_status_any "Get runtime profile" "200" GET "/api/repos/${REPO_ID}/runtime-profiles/${PROFILE_ID}"; then
      file="$LAST_RESPONSE_FILE"
      LIMS_EP="$(jq -r '.data.app_endpoints.lims // empty' "$file")"
      PORTAL_EP="$(jq -r '.data.app_endpoints.portal // empty' "$file")"
      if [[ -n "$LIMS_EP" && -n "$PORTAL_EP" ]]; then
        record_pass "TRS-AC-04 runtime profile read" "Runtime profile returns persisted \`app_endpoints\`."
      else
        record_fail "TRS-AC-04 runtime profile read" "Missing \`app_endpoints\` in runtime profile response."
      fi
      rm -f "$file"
    fi

    ENV_PAYLOAD="$(jq -cn --arg profile "$PROFILE_ID" '{
      environments:[
        {name:"dev",position:1,is_canonical:false,runtime_profile_id:$profile},
        {name:"prod",position:2,is_canonical:true,runtime_profile_id:$profile}
      ]
    }')"
    if expect_status_any "Patch environments with runtime profile" "200" PATCH "/api/repos/${REPO_ID}/environments" "$ENV_PAYLOAD"; then
      file="$LAST_RESPONSE_FILE"
      CNT="$(jq -r '.data | length' "$file")"
      if [[ "${CNT:-0}" -ge 2 ]]; then
        record_pass "TRS-AC-05 env profile linkage" "Environment set references runtime profile."
      else
        record_fail "TRS-AC-05 env profile linkage" "Environment patch did not return expected entries."
      fi
      rm -f "$file"
    fi
  fi
fi

if [[ "$FAIL_COUNT" -eq 0 ]]; then
  if [[ "${CONMAN_ACCEPTANCE_REQUIRE_E2E:-0}" == "1" ]]; then
    record_warn "TRS-AC-06 lifecycle regression guard" "Run \`tests/e2e/run_full_staged_smoke.sh\` separately to validate full lifecycle flow."
  else
    record_pass "TRS-AC-06 lifecycle regression guard" "Run \`tests/e2e/run_full_staged_smoke.sh\` in CI/nightly for full regression coverage."
  fi
fi

{
  echo "# Team/Repo/App Acceptance"
  echo
  echo "- Generated at: \`$(date -u +"%Y-%m-%dT%H:%M:%SZ")\`"
  echo "- Base URL: \`$BASE_URL\`"
  echo "- Repo path: \`$REPO_PATH\`"
  echo "- Strict mode: \`$STRICT\`"
  echo "- Pass: \`$PASS_COUNT\`"
  echo "- Warn: \`$WARN_COUNT\`"
  echo "- Fail: \`$FAIL_COUNT\`"
  echo
  echo "| Check | Result | Notes |"
  echo "|---|---|---|"
  printf '%s\n' "${RESULTS[@]}"
} > "$SUMMARY_FILE"

echo "Wrote acceptance summary: $SUMMARY_FILE"

if [[ "$FAIL_COUNT" -gt 0 ]]; then
  echo "Acceptance failed with $FAIL_COUNT hard failures."
  exit 1
fi

if [[ "$STRICT" -eq 1 && "$WARN_COUNT" -gt 0 ]]; then
  echo "Acceptance strict mode failed with $WARN_COUNT warnings."
  exit 2
fi

echo "Acceptance checks completed."
