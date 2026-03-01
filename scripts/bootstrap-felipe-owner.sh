#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:3000}"
RESULTS_DIR="${RESULTS_DIR:-$(cd "$(dirname "$0")/../tests/e2e/results" && pwd)}"
TS="${TS:-$(date +%Y%m%d%H%M%S)}"

ADMIN_EMAIL="${ADMIN_EMAIL:-}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-}"
FELIPE_EMAIL="${FELIPE_EMAIL:-felipe.coury@gmail.com}"
FELIPE_NAME="${FELIPE_NAME:-Felipe Coury}"
FELIPE_PASSWORD="${FELIPE_PASSWORD:-}"
TEAM_SLUGS="${TEAM_SLUGS:-hepquant-team,detoxu-team,biofidelity-team,dxflow-examples-team}"

SUMMARY_JSON="${RESULTS_DIR}/${TS}-bootstrap-felipe-owner.json"

require_cmd() {
  local cmd="$1"
  command -v "$cmd" >/dev/null 2>&1 || {
    echo "Missing required command: $cmd" >&2
    exit 1
  }
}

require_env() {
  local key="$1"
  local value="$2"
  if [[ -z "$value" ]]; then
    echo "Missing required env: $key" >&2
    exit 1
  fi
}

require_cmd curl
require_cmd jq
require_cmd mktemp

require_env "ADMIN_EMAIL" "$ADMIN_EMAIL"
require_env "ADMIN_PASSWORD" "$ADMIN_PASSWORD"
require_env "FELIPE_PASSWORD" "$FELIPE_PASSWORD"

mkdir -p "$RESULTS_DIR"

TMP_FILES=()
cleanup_tmp() {
  if [[ "${#TMP_FILES[@]}" -gt 0 ]]; then
    rm -f "${TMP_FILES[@]}"
  fi
}
trap cleanup_tmp EXIT

request() {
  local method="$1"
  local path="$2"
  local token="${3-}"
  local body="${4-}"
  local tmp status
  tmp="$(mktemp)"
  TMP_FILES+=("$tmp")

  if [[ -n "$body" ]]; then
    if [[ -n "$token" ]]; then
      status="$(curl -sS -o "$tmp" -w "%{http_code}" -X "$method" "${BASE_URL}${path}" \
        -H "Authorization: Bearer $token" -H "Content-Type: application/json" --data "$body")"
    else
      status="$(curl -sS -o "$tmp" -w "%{http_code}" -X "$method" "${BASE_URL}${path}" \
        -H "Content-Type: application/json" --data "$body")"
    fi
  else
    if [[ -n "$token" ]]; then
      status="$(curl -sS -o "$tmp" -w "%{http_code}" -X "$method" "${BASE_URL}${path}" \
        -H "Authorization: Bearer $token")"
    else
      status="$(curl -sS -o "$tmp" -w "%{http_code}" -X "$method" "${BASE_URL}${path}")"
    fi
  fi

  echo "$status|$tmp"
}

request_assert_200() {
  local method="$1"
  local path="$2"
  local token="${3-}"
  local body="${4-}"
  local out status file

  out="$(request "$method" "$path" "$token" "$body")"
  status="${out%%|*}"
  file="${out#*|}"

  if [[ "$status" != "200" ]]; then
    echo "HTTP $status for $method $path" >&2
    cat "$file" >&2
    exit 1
  fi

  echo "$file"
}

login_user_or_fail() {
  local email="$1"
  local password="$2"
  local out_token_var="$3"
  local out_user_id_var="$4"
  local body out status file token user_id

  body="$(jq -cn --arg email "$email" --arg password "$password" '{email:$email,password:$password}')"
  out="$(request POST "/api/auth/login" "" "$body")"
  status="${out%%|*}"
  file="${out#*|}"

  if [[ "$status" != "200" ]]; then
    echo "Login failed for $email with HTTP $status" >&2
    cat "$file" >&2
    exit 1
  fi

  token="$(jq -r '.data.token // empty' "$file")"
  user_id="$(jq -r '.data.user.id // empty' "$file")"
  if [[ -z "$token" || -z "$user_id" ]]; then
    echo "Login response missing token/user id for $email" >&2
    cat "$file" >&2
    exit 1
  fi

  printf -v "$out_token_var" '%s' "$token"
  printf -v "$out_user_id_var" '%s' "$user_id"
}

ensure_felipe_account() {
  local out_token_var="$1"
  local out_user_id_var="$2"
  local email_lc signup_body signup_out signup_status signup_file out status file token user_id

  email_lc="$(printf '%s' "$FELIPE_EMAIL" | tr '[:upper:]' '[:lower:]')"

  out="$(request POST "/api/auth/login" "" "$(jq -cn --arg email "$email_lc" --arg password "$FELIPE_PASSWORD" '{email:$email,password:$password}')")"
  status="${out%%|*}"
  file="${out#*|}"

  if [[ "$status" == "200" ]]; then
    token="$(jq -r '.data.token // empty' "$file")"
    user_id="$(jq -r '.data.user.id // empty' "$file")"
    if [[ -z "$token" || -z "$user_id" ]]; then
      echo "Felipe login response missing token/user id" >&2
      cat "$file" >&2
      exit 1
    fi
    printf -v "$out_token_var" '%s' "$token"
    printf -v "$out_user_id_var" '%s' "$user_id"
    return
  fi

  if [[ "$status" != "401" ]]; then
    echo "Unexpected HTTP $status while checking Felipe account" >&2
    cat "$file" >&2
    exit 1
  fi

  signup_body="$(jq -cn \
    --arg name "$FELIPE_NAME" \
    --arg email "$email_lc" \
    --arg password "$FELIPE_PASSWORD" \
    '{name:$name,email:$email,password:$password}')"
  signup_out="$(request POST "/api/auth/signup" "" "$signup_body")"
  signup_status="${signup_out%%|*}"
  signup_file="${signup_out#*|}"

  if [[ "$signup_status" == "409" ]]; then
    echo "Felipe account already exists but login failed. Check FELIPE_PASSWORD." >&2
    cat "$signup_file" >&2
    exit 1
  fi
  if [[ "$signup_status" != "200" ]]; then
    echo "Unexpected HTTP $signup_status while creating Felipe account" >&2
    cat "$signup_file" >&2
    exit 1
  fi

  token="$(jq -r '.data.token // empty' "$signup_file")"
  user_id="$(jq -r '.data.user.id // empty' "$signup_file")"
  if [[ -z "$token" || -z "$user_id" ]]; then
    echo "Felipe signup response missing token/user id" >&2
    cat "$signup_file" >&2
    exit 1
  fi

  printf -v "$out_token_var" '%s' "$token"
  printf -v "$out_user_id_var" '%s' "$user_id"
}

fetch_team_ids_csv() {
  local token="$1"
  local list_file ids
  list_file="$(request_assert_200 GET "/api/teams?page=1&limit=100" "$token")"
  ids="$(jq -r '.data[].id' "$list_file" | paste -sd ',' -)"
  echo "$ids"
}

contains_csv_id() {
  local csv="$1"
  local id="$2"
  [[ ",$csv," == *",$id,"* ]]
}

echo "Checking Conman health at ${BASE_URL}/api/health ..."
health_out="$(request GET "/api/health")"
health_status="${health_out%%|*}"
health_file="${health_out#*|}"
if [[ "$health_status" != "200" && "$health_status" != "503" ]]; then
  echo "Health check failed with HTTP $health_status" >&2
  cat "$health_file" >&2
  exit 1
fi

echo "Logging in admin user: $ADMIN_EMAIL"
ADMIN_TOKEN=""
ADMIN_USER_ID=""
login_user_or_fail "$ADMIN_EMAIL" "$ADMIN_PASSWORD" ADMIN_TOKEN ADMIN_USER_ID

teams_file="$(request_assert_200 GET "/api/teams?page=1&limit=100" "$ADMIN_TOKEN")"

IFS=',' read -r -a RAW_TEAM_SLUGS <<<"$TEAM_SLUGS"
CLEAN_TEAM_SLUGS=()
TARGET_TEAM_IDS=()
missing_slugs=()

for slug in "${RAW_TEAM_SLUGS[@]}"; do
  trimmed_slug="$(printf '%s' "$slug" | xargs)"
  if [[ -z "$trimmed_slug" ]]; then
    continue
  fi
  CLEAN_TEAM_SLUGS+=("$trimmed_slug")
  team_id="$(jq -r --arg slug "$trimmed_slug" '.data[] | select(.slug == $slug) | .id' "$teams_file" | head -n1)"
  if [[ -z "$team_id" ]]; then
    missing_slugs+=("$trimmed_slug")
  else
    TARGET_TEAM_IDS+=("$team_id")
  fi
done

if [[ "${#missing_slugs[@]}" -gt 0 ]]; then
  printf 'Missing required team slugs: %s\n' "$(IFS=,; echo "${missing_slugs[*]}")" >&2
  exit 1
fi

echo "Ensuring Felipe account exists: $FELIPE_EMAIL"
FELIPE_TOKEN=""
FELIPE_USER_ID=""
ensure_felipe_account FELIPE_TOKEN FELIPE_USER_ID

FELIPE_TEAM_IDS_CSV="$(fetch_team_ids_csv "$FELIPE_TOKEN")"

felipe_email_lc="$(printf '%s' "$FELIPE_EMAIL" | tr '[:upper:]' '[:lower:]')"
summary_entries=()

for index in "${!CLEAN_TEAM_SLUGS[@]}"; do
  team_slug="${CLEAN_TEAM_SLUGS[$index]}"
  team_id="${TARGET_TEAM_IDS[$index]}"

  if contains_csv_id "$FELIPE_TEAM_IDS_CSV" "$team_id"; then
    summary_entries+=("{\"team_slug\":\"${team_slug}\",\"team_id\":\"${team_id}\",\"status\":\"already_member\"}")
    continue
  fi

  echo "Granting owner on ${team_slug} (${team_id})"
  invite_payload="$(jq -cn --arg email "$felipe_email_lc" --arg role "owner" '{email:$email,role:$role}')"
  invite_out="$(request POST "/api/teams/${team_id}/invites" "$ADMIN_TOKEN" "$invite_payload")"
  invite_status="${invite_out%%|*}"
  invite_file="${invite_out#*|}"

  invite_id=""
  invite_token=""
  invite_source=""

  if [[ "$invite_status" == "200" ]]; then
    invite_id="$(jq -r '.data.id // empty' "$invite_file")"
    invite_token="$(jq -r '.data.token // empty' "$invite_file")"
    invite_source="created"
  elif [[ "$invite_status" == "409" ]]; then
    pending_file="$(request_assert_200 GET "/api/teams/${team_id}/invites?page=1&limit=100" "$ADMIN_TOKEN")"
    invite_id="$(jq -r --arg email "$felipe_email_lc" '.data[] | select((.email | ascii_downcase) == $email) | .id' "$pending_file" | head -n1)"
    invite_token="$(jq -r --arg email "$felipe_email_lc" '.data[] | select((.email | ascii_downcase) == $email) | .token' "$pending_file" | head -n1)"
    invite_source="existing_pending"
  else
    echo "Failed to create invite for team ${team_slug}, HTTP ${invite_status}" >&2
    cat "$invite_file" >&2
    exit 1
  fi

  if [[ -z "$invite_token" ]]; then
    echo "Could not resolve invite token for team ${team_slug}" >&2
    cat "$invite_file" >&2
    exit 1
  fi

  accept_payload="$(jq -cn \
    --arg token "$invite_token" \
    --arg name "$FELIPE_NAME" \
    --arg password "$FELIPE_PASSWORD" \
    '{token:$token,name:$name,password:$password}')"

  accept_file="$(request_assert_200 POST "/api/auth/accept-invite" "" "$accept_payload")"
  accepted_user_id="$(jq -r '.data.user.id // empty' "$accept_file")"
  if [[ "$accepted_user_id" != "$FELIPE_USER_ID" ]]; then
    echo "Invite acceptance user mismatch for team ${team_slug}" >&2
    cat "$accept_file" >&2
    exit 1
  fi

  # Refresh token so new memberships are reflected in JWT claims.
  login_user_or_fail "$felipe_email_lc" "$FELIPE_PASSWORD" FELIPE_TOKEN FELIPE_USER_ID
  FELIPE_TEAM_IDS_CSV="$(fetch_team_ids_csv "$FELIPE_TOKEN")"

  summary_entries+=("{\"team_slug\":\"${team_slug}\",\"team_id\":\"${team_id}\",\"status\":\"assigned_owner\",\"invite_id\":\"${invite_id}\",\"invite_source\":\"${invite_source}\"}")
done

# Final verification after all assignments.
login_user_or_fail "$felipe_email_lc" "$FELIPE_PASSWORD" FELIPE_TOKEN FELIPE_USER_ID
FELIPE_TEAM_IDS_CSV="$(fetch_team_ids_csv "$FELIPE_TOKEN")"

verification_missing=()
for team_id in "${TARGET_TEAM_IDS[@]}"; do
  if ! contains_csv_id "$FELIPE_TEAM_IDS_CSV" "$team_id"; then
    verification_missing+=("$team_id")
  fi
done

if [[ "${#verification_missing[@]}" -gt 0 ]]; then
  printf 'Felipe is still missing team memberships: %s\n' "$(IFS=,; echo "${verification_missing[*]}")" >&2
  exit 1
fi

felipe_repos_file="$(request_assert_200 GET "/api/repos?page=1&limit=100" "$FELIPE_TOKEN")"
felipe_repo_total="$(jq -r '.pagination.total // (.data | length)' "$felipe_repos_file")"

team_repo_checks=()
for index in "${!TARGET_TEAM_IDS[@]}"; do
  team_id="${TARGET_TEAM_IDS[$index]}"
  team_slug="${CLEAN_TEAM_SLUGS[$index]}"
  repo_count="$(jq -r --arg team_id "$team_id" '[.data[] | select(.team_id == $team_id)] | length' "$felipe_repos_file")"
  team_repo_checks+=("{\"team_slug\":\"${team_slug}\",\"team_id\":\"${team_id}\",\"instance_count\":${repo_count}}")
done

felipe_team_count=0
if [[ -n "$FELIPE_TEAM_IDS_CSV" ]]; then
  OLD_IFS="$IFS"
  IFS=','
  read -r -a FELIPE_TEAM_IDS_ARRAY <<<"$FELIPE_TEAM_IDS_CSV"
  IFS="$OLD_IFS"
  felipe_team_count="${#FELIPE_TEAM_IDS_ARRAY[@]}"
fi

{
  echo "{"
  echo "  \"timestamp\": \"${TS}\","
  echo "  \"base_url\": \"${BASE_URL}\","
  echo "  \"admin_email\": \"${ADMIN_EMAIL}\","
  echo "  \"admin_user_id\": \"${ADMIN_USER_ID}\","
  echo "  \"felipe_email\": \"${felipe_email_lc}\","
  echo "  \"felipe_user_id\": \"${FELIPE_USER_ID}\","
  echo "  \"team_slugs\": ["
  for i in "${!CLEAN_TEAM_SLUGS[@]}"; do
    slug="${CLEAN_TEAM_SLUGS[$i]}"
    if [[ "$i" -gt 0 ]]; then
      echo "    ,\"${slug}\""
    else
      echo "    \"${slug}\""
    fi
  done
  echo "  ],"
  echo "  \"items\": ["
  for i in "${!summary_entries[@]}"; do
    if [[ "$i" -gt 0 ]]; then
      echo "    ,${summary_entries[$i]}"
    else
      echo "    ${summary_entries[$i]}"
    fi
  done
  echo "  ],"
  echo "  \"verification\": {"
  echo "    \"target_team_count\": ${#TARGET_TEAM_IDS[@]},"
  echo "    \"felipe_team_count\": ${felipe_team_count},"
  echo "    \"felipe_repo_total\": ${felipe_repo_total},"
  echo "    \"team_instances\": ["
  for i in "${!team_repo_checks[@]}"; do
    if [[ "$i" -gt 0 ]]; then
      echo "      ,${team_repo_checks[$i]}"
    else
      echo "      ${team_repo_checks[$i]}"
    fi
  done
  echo "    ]"
  echo "  }"
  echo "}"
} > "$SUMMARY_JSON"

echo "Bootstrap complete."
echo "Summary: $SUMMARY_JSON"
cat "$SUMMARY_JSON" | jq .
