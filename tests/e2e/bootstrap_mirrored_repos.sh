#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:3000}"
RESULTS_DIR="${RESULTS_DIR:-$(cd "$(dirname "$0")" && pwd)/results}"
TS="${TS:-$(date +%Y%m%d%H%M%S)}"

ADMIN_NAME="${ADMIN_NAME:-Repo Bootstrap Admin}"
ADMIN_EMAIL="${ADMIN_EMAIL:-bootstrap-${TS}@example.com}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-AdminPassw0rd!!}"

mkdir -p "$RESULTS_DIR"
SUMMARY_JSON="$RESULTS_DIR/${TS}-bootstrap-mirrored-repos.json"

require_cmd() {
  local cmd="$1"
  command -v "$cmd" >/dev/null 2>&1 || {
    echo "Missing required command: $cmd" >&2
    exit 1
  }
}

require_cmd curl
require_cmd jq

request() {
  local method="$1" path="$2" token="${3-}" body="${4-}"
  local tmp status
  tmp=$(mktemp)

  if [[ -n "$body" ]]; then
    if [[ -n "$token" ]]; then
      status=$(curl -sS -o "$tmp" -w "%{http_code}" -X "$method" "${BASE_URL}${path}" \
        -H "Authorization: Bearer $token" -H "Content-Type: application/json" --data "$body")
    else
      status=$(curl -sS -o "$tmp" -w "%{http_code}" -X "$method" "${BASE_URL}${path}" \
        -H "Content-Type: application/json" --data "$body")
    fi
  else
    if [[ -n "$token" ]]; then
      status=$(curl -sS -o "$tmp" -w "%{http_code}" -X "$method" "${BASE_URL}${path}" \
        -H "Authorization: Bearer $token")
    else
      status=$(curl -sS -o "$tmp" -w "%{http_code}" -X "$method" "${BASE_URL}${path}")
    fi
  fi

  echo "$status|$tmp"
}

request_assert_200() {
  local method="$1" path="$2" token="${3-}" body="${4-}"
  local out status file
  out=$(request "$method" "$path" "$token" "$body")
  status=${out%%|*}
  file=${out#*|}
  if [[ "$status" != "200" ]]; then
    echo "HTTP ${status} for ${method} ${path}" >&2
    cat "$file" >&2
    rm -f "$file"
    exit 1
  fi
  echo "$file"
}

request_expect_statuses() {
  local method="$1" path="$2" allowed_csv="$3" token="${4-}" body="${5-}"
  local out status file
  out=$(request "$method" "$path" "$token" "$body")
  status=${out%%|*}
  file=${out#*|}
  if [[ ",${allowed_csv}," != *",${status},"* ]]; then
    echo "HTTP ${status} for ${method} ${path} (allowed: ${allowed_csv})" >&2
    cat "$file" >&2
    rm -f "$file"
    exit 1
  fi
  echo "$file"
}

echo "Checking Conman health at ${BASE_URL}/api/health ..."
health_file=$(request_expect_statuses GET "/api/health" "200,503")
echo "Health: $(jq -c '.data? // . | {status,components}' "$health_file")"
rm -f "$health_file"

echo "Signing up bootstrap admin: $ADMIN_EMAIL"
signup_file=$(request_assert_200 POST "/api/auth/signup" "" \
  "{\"name\":\"${ADMIN_NAME}\",\"email\":\"${ADMIN_EMAIL}\",\"password\":\"${ADMIN_PASSWORD}\"}")
TOKEN=$(jq -r '.data.token' "$signup_file")
TEAM_ID=$(jq -r '.data.team.id' "$signup_file")
USER_ID=$(jq -r '.data.user.id' "$signup_file")
rm -f "$signup_file"

echo "Team ID: $TEAM_ID"
echo "User ID: $USER_ID"

declare -a REPOS=(
  "Hepquant Config|hepquant-config.git|master"
  "Detoxu Config|detoxu-config.git|main"
  "Biofidelity Config|biofidelity-config.git|main"
  "Dxflow Examples|dxflow-examples.git|main"
)

created_repo_rows=()

for spec in "${REPOS[@]}"; do
  IFS='|' read -r repo_name repo_path integration_branch <<<"$spec"
  echo "Creating repo '${repo_name}' -> ${repo_path} (${integration_branch})"
  create_file=$(request_assert_200 POST "/api/teams/${TEAM_ID}/repos" "$TOKEN" \
    "{\"name\":\"${repo_name}\",\"repo_path\":\"${repo_path}\",\"integration_branch\":\"${integration_branch}\"}")
  repo_id=$(jq -r '.data.id' "$create_file")
  rm -f "$create_file"
  created_repo_rows+=("${repo_id}|${repo_name}|${repo_path}|${integration_branch}")
done

echo "Refreshing token after repo creation ..."
login_file=$(request_assert_200 POST "/api/auth/login" "" \
  "{\"email\":\"${ADMIN_EMAIL}\",\"password\":\"${ADMIN_PASSWORD}\"}")
TOKEN=$(jq -r '.data.token' "$login_file")
rm -f "$login_file"

summary_entries=()

for row in "${created_repo_rows[@]}"; do
  IFS='|' read -r repo_id repo_name repo_path integration_branch <<<"$row"
  echo "Setting environments for ${repo_path}"
  env_file=$(request_assert_200 PATCH "/api/repos/${repo_id}/environments" "$TOKEN" \
    '{"environments":[{"name":"dev","position":1,"is_canonical":false,"runtime_profile_id":null},{"name":"prod","position":2,"is_canonical":true,"runtime_profile_id":null}]}')
  env_ids=$(jq -c '[.data[] | {id,name,position,is_canonical}]' "$env_file")
  rm -f "$env_file"
  summary_entries+=("{\"repo_id\":\"${repo_id}\",\"repo_name\":\"${repo_name}\",\"repo_path\":\"${repo_path}\",\"integration_branch\":\"${integration_branch}\",\"environments\":${env_ids}}")
done

{
  echo "{"
  echo "  \"timestamp\": \"${TS}\","
  echo "  \"base_url\": \"${BASE_URL}\","
  echo "  \"team_id\": \"${TEAM_ID}\","
  echo "  \"admin_user_id\": \"${USER_ID}\","
  echo "  \"admin_email\": \"${ADMIN_EMAIL}\","
  echo "  \"repos\": ["
  for i in "${!summary_entries[@]}"; do
    if [[ "$i" -gt 0 ]]; then
      echo "    ,${summary_entries[$i]}"
    else
      echo "    ${summary_entries[$i]}"
    fi
  done
  echo "  ]"
  echo "}"
} > "$SUMMARY_JSON"

echo
echo "Bootstrap complete."
echo "Summary: $SUMMARY_JSON"
cat "$SUMMARY_JSON" | jq .
