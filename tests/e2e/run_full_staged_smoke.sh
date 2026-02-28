#!/usr/bin/env bash
set -euo pipefail

ROOT="${ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
GITALY_ROOT="${GITALY_ROOT:-/Users/fcoury/code-external/gitaly}"
RESULTS="$ROOT/tests/e2e/results"
mkdir -p "$RESULTS"
TS=$(date +%Y%m%d%H%M%S)
PORT="${PORT:-3925}"
DB="conman_e2e_full_${TS}"
REPO="conman-e2e-${TS}.git"
WORKDIR="/tmp/conman-e2e/${TS}"
SESSION="conman-e2e-${TS}"
LOG_FILE="$RESULTS/${TS}-full-e2e.log"
SUMMARY_FILE="$RESULTS/${TS}-full-e2e-summary.md"
CREATE_REPO_JSON="$RESULTS/${TS}-create-repo.json"
JWT_SECRET="test-secret-test-secret-test-1234"
MONGO_URI="${MONGO_URI:-mongodb://127.0.0.1:27019}"

cleanup() {
  tmux kill-session -t "$SESSION" >/dev/null 2>&1 || true
}
trap cleanup EXIT

request() {
  local method="$1" path="$2" token="${3-}" body="${4-}"
  local tmp status
  tmp=$(mktemp)
  if [[ -n "$body" ]]; then
    if [[ -n "$token" ]]; then
      status=$(curl -sS -o "$tmp" -w "%{http_code}" -X "$method" "http://127.0.0.1:${PORT}${path}" \
        -H "Authorization: Bearer $token" -H "Content-Type: application/json" --data "$body")
    else
      status=$(curl -sS -o "$tmp" -w "%{http_code}" -X "$method" "http://127.0.0.1:${PORT}${path}" \
        -H "Content-Type: application/json" --data "$body")
    fi
  else
    if [[ -n "$token" ]]; then
      status=$(curl -sS -o "$tmp" -w "%{http_code}" -X "$method" "http://127.0.0.1:${PORT}${path}" \
        -H "Authorization: Bearer $token")
    else
      status=$(curl -sS -o "$tmp" -w "%{http_code}" -X "$method" "http://127.0.0.1:${PORT}${path}")
    fi
  fi
  echo "$status|$tmp"
}

request_assert_200() {
  local method="$1" path="$2" token="${3-}" body="${4-}"
  local r status file
  r=$(request "$method" "$path" "$token" "$body")
  status=${r%%|*}; file=${r#*|}
  if [[ "$status" != "200" ]]; then
    cat "$file" >&2
    rm -f "$file"
    exit 1
  fi
  echo "$file"
}

request_expect_status() {
  local expected="$1" method="$2" path="$3" token="${4-}" body="${5-}"
  local r status file
  r=$(request "$method" "$path" "$token" "$body")
  status=${r%%|*}; file=${r#*|}
  if [[ "$status" != "$expected" ]]; then
    echo "expected HTTP ${expected}, got ${status} for ${method} ${path}" >&2
    cat "$file" >&2
    rm -f "$file"
    exit 1
  fi
  rm -f "$file"
}

latest_job_id() {
  local repo_id="$1" token="$2" job_type="$3" entity_id_pattern="$4"
  local file
  file=$(request_assert_200 GET "/api/repos/${repo_id}/jobs?page=1&limit=100" "$token")
  jq -r ".data[] | select(.job_type==\"${job_type}\") | select(.entity_id|test(\"${entity_id_pattern}\")) | .id" "$file" | head -n1
  rm -f "$file"
}

job_state() {
  local repo_id="$1" token="$2" job_id="$3"
  local file
  file=$(request_assert_200 GET "/api/repos/${repo_id}/jobs/${job_id}" "$token")
  jq -r ".data.job.state" "$file"
  rm -f "$file"
}

wait_job() {
  local repo_id="$1" token="$2" job_id="$3" label="$4"
  for _ in {1..120}; do
    state=$(job_state "$repo_id" "$token" "$job_id")
    case "$state" in
      succeeded) return 0 ;;
      failed|canceled)
        echo "job ${label} ended in ${state}" >&2
        return 1
        ;;
      *) sleep 1 ;;
    esac
  done
  echo "job ${label} timed out" >&2
  return 1
}

echo "[1/11] Creating staged git repo $REPO"
grpcurl -plaintext -import-path "$GITALY_ROOT/proto" -proto repository.proto \
  -d "{\"repository\":{\"storage_name\":\"default\",\"relative_path\":\"$REPO\"}}" \
  localhost:2305 gitaly.RepositoryService/CreateRepository > "$CREATE_REPO_JSON"

mkdir -p "$WORKDIR"
cd "$WORKDIR"
rm -rf src
mkdir src
cd src
git init >/dev/null
git checkout -b main >/dev/null
echo "# Conman staged ${TS}" > README.md
mkdir -p config
echo "feature: baseline" > config/app.yaml
git add README.md config/app.yaml
git -c user.name=e2e -c user.email=e2e@example.com commit -m "seed" >/dev/null
git remote add forge "http://localhost:8080/${REPO}"
git push forge main >/dev/null

echo "[2/11] Starting Conman API on :$PORT"
tmux new-session -d -s "$SESSION" "cd $ROOT && CONMAN_HOST=127.0.0.1 CONMAN_PORT=$PORT CONMAN_MONGO_URI=$MONGO_URI CONMAN_MONGO_DB=$DB CONMAN_GITALY_ADDRESS=http://127.0.0.1:2305 CONMAN_JWT_SECRET=$JWT_SECRET CONMAN_SECRETS_MASTER_KEY=master-key CONMAN_TEMP_URL_DOMAIN=config.example.test CONMAN_DEPLOY_RELEASE_CMD=true cargo run > $LOG_FILE 2>&1"

for _ in {1..120}; do
  if [[ -f "$LOG_FILE" ]] && rg -q "server listening" "$LOG_FILE"; then
    break
  fi
  sleep 1
done
if ! [[ -f "$LOG_FILE" ]] || ! rg -q "server listening" "$LOG_FILE"; then
  echo "Conman failed to start" >&2
  [[ -f "$LOG_FILE" ]] && tail -n 120 "$LOG_FILE" >&2 || true
  exit 1
fi

echo "[3/11] Signup + create team repo + invite reviewer"
ADMIN_EMAIL="e2e-admin-${TS}@example.com"
ADMIN_PASS="AdminPassw0rd!!"
REVIEWER_EMAIL="e2e-reviewer-${TS}@example.com"
REVIEWER_PASS="ReviewerPassw0rd!!"

file=$(request_assert_200 POST "/api/auth/signup" "" "{\"name\":\"E2E Admin\",\"email\":\"${ADMIN_EMAIL}\",\"password\":\"${ADMIN_PASS}\"}")
TOKEN_ADMIN=$(jq -r '.data.token' "$file")
USER_ID=$(jq -r '.data.user.id' "$file")
TEAM_ID=$(jq -r '.data.team.id' "$file")
rm -f "$file"

file=$(request_assert_200 POST "/api/teams/${TEAM_ID}/repos" "$TOKEN_ADMIN" "{\"name\":\"E2E Repo ${TS}\",\"repo_path\":\"${REPO}\",\"integration_branch\":\"main\"}")
REPO_ID=$(jq -r '.data.id' "$file")
rm -f "$file"

# Refresh token so repo membership claim includes the newly created repo.
file=$(request_assert_200 POST "/api/auth/login" "" "{\"email\":\"${ADMIN_EMAIL}\",\"password\":\"${ADMIN_PASS}\"}")
TOKEN_ADMIN=$(jq -r '.data.token' "$file")
rm -f "$file"

file=$(request_assert_200 POST "/api/teams/${TEAM_ID}/invites" "$TOKEN_ADMIN" "{\"email\":\"${REVIEWER_EMAIL}\",\"role\":\"reviewer\"}")
INVITE_TOKEN=$(jq -r '.data.token' "$file")
rm -f "$file"

file=$(request_assert_200 POST "/api/auth/accept-invite" "" "{\"token\":\"${INVITE_TOKEN}\",\"name\":\"E2E Reviewer\",\"password\":\"${REVIEWER_PASS}\"}")
TOKEN_REVIEWER=$(jq -r '.data.token' "$file")
REVIEWER_ID=$(jq -r '.data.user.id' "$file")
rm -f "$file"

# Optional explicit role assignment endpoint coverage.
request_assert_200 POST "/api/repos/${REPO_ID}/members" "$TOKEN_ADMIN" "{\"user_id\":\"${REVIEWER_ID}\",\"role\":\"reviewer\"}" >/dev/null

echo "[4/11] Setting environments + creating workspace"
file=$(request_assert_200 PATCH "/api/repos/${REPO_ID}/environments" "$TOKEN_ADMIN" "{\"environments\":[{\"name\":\"prod\",\"position\":1,\"is_canonical\":true,\"runtime_profile_id\":null}]}")
ENV_ID=$(jq -r ".data[0].id" "$file")
rm -f "$file"

file=$(request_assert_200 GET "/api/repos/${REPO_ID}/workspaces" "$TOKEN_ADMIN")
WORKSPACE_ID=$(jq -r ".data[0].id" "$file")
rm -f "$file"

request_assert_200 PATCH "/api/repos/${REPO_ID}/settings" "$TOKEN_ADMIN" "{\"file_size_limit_bytes\":128}" >/dev/null
request_expect_status 403 PUT "/api/repos/${REPO_ID}/workspaces/${WORKSPACE_ID}/files" "$TOKEN_ADMIN" "{\"path\":\".git/config\",\"content\":\"YmxvY2tlZA==\",\"message\":\"blocked-path-check\"}"
LARGE_CONTENT_B64=$(python3 - <<'PY'
import base64
print(base64.b64encode(b"a" * 512).decode())
PY
)
request_expect_status 400 PUT "/api/repos/${REPO_ID}/workspaces/${WORKSPACE_ID}/files" "$TOKEN_ADMIN" "{\"path\":\"config/too-big.txt\",\"content\":\"${LARGE_CONTENT_B64}\",\"message\":\"size-guardrail-check\"}"

echo "[5/11] Writing workspace file"
CONTENT_B64=$(printf 'feature: staged-%s\n' "$TS" | base64 | tr -d '\n')
file=$(request_assert_200 PUT "/api/repos/${REPO_ID}/workspaces/${WORKSPACE_ID}/files" "$TOKEN_ADMIN" "{\"path\":\"config/app.yaml\",\"content\":\"${CONTENT_B64}\",\"message\":\"update config\"}")
WRITE_SHA=$(jq -r ".data.commit_sha" "$file")
rm -f "$file"

echo "[6/11] Changeset create/submit/approve/queue"
file=$(request_assert_200 POST "/api/repos/${REPO_ID}/changesets" "$TOKEN_ADMIN" "{\"workspace_id\":\"${WORKSPACE_ID}\",\"title\":\"Staged changeset ${TS}\",\"description\":\"e2e\"}")
CHANGESET_ID=$(jq -r ".data.id" "$file")
rm -f "$file"

file=$(request_assert_200 POST "/api/repos/${REPO_ID}/changesets/${CHANGESET_ID}/submit" "$TOKEN_ADMIN" "{}")
SUBMIT_JOB_ID=$(jq -r ".data.job.id" "$file")
rm -f "$file"
wait_job "$REPO_ID" "$TOKEN_ADMIN" "$SUBMIT_JOB_ID" "msuite_submit"

request_assert_200 POST "/api/repos/${REPO_ID}/changesets/${CHANGESET_ID}/review" "$TOKEN_REVIEWER" "{\"action\":\"approve\"}" >/dev/null
request_assert_200 POST "/api/repos/${REPO_ID}/changesets/${CHANGESET_ID}/queue" "$TOKEN_ADMIN" >/dev/null

echo "[7/11] Release draft/assemble/publish"
file=$(request_assert_200 POST "/api/repos/${REPO_ID}/releases" "$TOKEN_ADMIN")
RELEASE_ID=$(jq -r ".data.id" "$file")
RELEASE_TAG=$(jq -r ".data.tag" "$file")
rm -f "$file"

request_assert_200 POST "/api/repos/${REPO_ID}/releases/${RELEASE_ID}/changesets" "$TOKEN_ADMIN" "{\"changeset_ids\":[\"${CHANGESET_ID}\"]}" >/dev/null
file=$(request_assert_200 POST "/api/repos/${REPO_ID}/releases/${RELEASE_ID}/assemble" "$TOKEN_ADMIN")
ASSEMBLE_JOB_ID=$(jq -r ".data.job.id" "$file")
rm -f "$file"
wait_job "$REPO_ID" "$TOKEN_ADMIN" "$ASSEMBLE_JOB_ID" "release_assemble"

r=$(request POST "/api/repos/${REPO_ID}/releases/${RELEASE_ID}/publish" "$TOKEN_ADMIN")
status=${r%%|*}; file=${r#*|}
if [[ "$status" == "409" ]]; then
  rm -f "$file"
  MERGE_JOB_ID=$(latest_job_id "$REPO_ID" "$TOKEN_ADMIN" "msuite_merge" "${RELEASE_ID}")
  wait_job "$REPO_ID" "$TOKEN_ADMIN" "$MERGE_JOB_ID" "msuite_merge"
  file=$(request_assert_200 POST "/api/repos/${REPO_ID}/releases/${RELEASE_ID}/publish" "$TOKEN_ADMIN")
elif [[ "$status" != "200" ]]; then
  cat "$file" >&2
  rm -f "$file"
  exit 1
fi
PUBLISHED_SHA=$(jq -r ".data.release.published_sha" "$file")
rm -f "$file"

echo "[8/11] Deploy gates + deployment"
r=$(request POST "/api/repos/${REPO_ID}/environments/${ENV_ID}/deploy" "$TOKEN_ADMIN" "{\"release_id\":\"${RELEASE_ID}\"}")
status=${r%%|*}; file=${r#*|}
if [[ "$status" != "409" ]]; then
  cat "$file" >&2
  rm -f "$file"
  exit 1
fi
rm -f "$file"
DRIFT_JOB_ID=$(latest_job_id "$REPO_ID" "$TOKEN_ADMIN" "runtime_profile_drift_check" "${ENV_ID}")
wait_job "$REPO_ID" "$TOKEN_ADMIN" "$DRIFT_JOB_ID" "drift_check"

r=$(request POST "/api/repos/${REPO_ID}/environments/${ENV_ID}/deploy" "$TOKEN_ADMIN" "{\"release_id\":\"${RELEASE_ID}\"}")
status=${r%%|*}; file=${r#*|}
if [[ "$status" != "409" ]]; then
  cat "$file" >&2
  rm -f "$file"
  exit 1
fi
rm -f "$file"
DEPLOY_GATE_JOB_ID=$(latest_job_id "$REPO_ID" "$TOKEN_ADMIN" "msuite_deploy" "${ENV_ID}:${RELEASE_ID}")
wait_job "$REPO_ID" "$TOKEN_ADMIN" "$DEPLOY_GATE_JOB_ID" "msuite_deploy"

file=$(request_assert_200 POST "/api/repos/${REPO_ID}/environments/${ENV_ID}/deploy" "$TOKEN_ADMIN" "{\"release_id\":\"${RELEASE_ID}\"}")
DEPLOYMENT_JOB_ID=$(jq -r ".data.job.id" "$file")
DEPLOYMENT_ID=$(jq -r ".data.deployment.id" "$file")
rm -f "$file"
wait_job "$REPO_ID" "$TOKEN_ADMIN" "$DEPLOYMENT_JOB_ID" "deploy_release"

file=$(request_assert_200 POST "/api/repos/${REPO_ID}/environments/${ENV_ID}/promote" "$TOKEN_ADMIN" "{\"release_id\":\"${RELEASE_ID}\"}")
PROMOTE_JOB_ID=$(jq -r ".data.job.id" "$file")
PROMOTE_DEPLOYMENT_ID=$(jq -r ".data.deployment.id" "$file")
rm -f "$file"
wait_job "$REPO_ID" "$TOKEN_ADMIN" "$PROMOTE_JOB_ID" "promote_release"

file=$(request_assert_200 POST "/api/repos/${REPO_ID}/environments/${ENV_ID}/rollback" "$TOKEN_ADMIN" "{\"release_id\":\"${RELEASE_ID}\",\"mode\":\"redeploy_prior_tag\",\"approvals\":[\"${USER_ID}\",\"${REVIEWER_ID}\"]}")
ROLLBACK_JOB_ID=$(jq -r ".data.job.id" "$file")
ROLLBACK_DEPLOYMENT_ID=$(jq -r ".data.deployment.id" "$file")
rm -f "$file"
wait_job "$REPO_ID" "$TOKEN_ADMIN" "$ROLLBACK_JOB_ID" "rollback_release"

echo "[9/11] Temp environment lifecycle"
file=$(request_assert_200 POST "/api/repos/${REPO_ID}/temp-envs" "$TOKEN_ADMIN" "{\"kind\":\"workspace\",\"source_id\":\"${WORKSPACE_ID}\"}")
TEMP_ENV_ID=$(jq -r ".data.temp_env.id" "$file")
TEMP_ENV_URL=$(jq -r ".data.temp_env.url" "$file")
TEMP_PROVISION_JOB_ID=$(jq -r ".data.job.id" "$file")
rm -f "$file"
wait_job "$REPO_ID" "$TOKEN_ADMIN" "$TEMP_PROVISION_JOB_ID" "temp_env_provision"
request_assert_200 POST "/api/repos/${REPO_ID}/temp-envs/${TEMP_ENV_ID}/extend" "$TOKEN_ADMIN" "{\"seconds\":7200}" >/dev/null
file=$(request_assert_200 DELETE "/api/repos/${REPO_ID}/temp-envs/${TEMP_ENV_ID}" "$TOKEN_ADMIN")
TEMP_EXPIRE_JOB_ID=$(jq -r ".data.job.id" "$file")
rm -f "$file"
wait_job "$REPO_ID" "$TOKEN_ADMIN" "$TEMP_EXPIRE_JOB_ID" "temp_env_expire"
request_assert_200 POST "/api/repos/${REPO_ID}/temp-envs/${TEMP_ENV_ID}/undo-expire" "$TOKEN_ADMIN" >/dev/null

echo "[10/11] Collecting final states"
file=$(request_assert_200 GET "/api/repos/${REPO_ID}/jobs?page=1&limit=100" "$TOKEN_ADMIN")
JOBS_FILE="$file"

file=$(request_assert_200 GET "/api/repos/${REPO_ID}/changesets/${CHANGESET_ID}" "$TOKEN_ADMIN")
CHANGESET_STATE=$(jq -r ".data.changeset.state" "$file")
rm -f "$file"
file=$(request_assert_200 GET "/api/repos/${REPO_ID}/releases/${RELEASE_ID}" "$TOKEN_ADMIN")
RELEASE_STATE=$(jq -r ".data.state" "$file")
rm -f "$file"
file=$(request_assert_200 GET "/api/repos/${REPO_ID}/deployments?page=1&limit=50" "$TOKEN_ADMIN")
DEPLOY_STATE=$(jq -r ".data[] | select(.id==\"${DEPLOYMENT_ID}\") | .state" "$file")
PROMOTE_STATE=$(jq -r ".data[] | select(.id==\"${PROMOTE_DEPLOYMENT_ID}\") | .state" "$file")
ROLLBACK_STATE=$(jq -r ".data[] | select(.id==\"${ROLLBACK_DEPLOYMENT_ID}\") | .state" "$file")
rm -f "$file"
file=$(request_assert_200 GET "/api/repos/${REPO_ID}/temp-envs?page=1&limit=50" "$TOKEN_ADMIN")
TEMP_ENV_FINAL_STATE=$(jq -r ".data[] | select(.id==\"${TEMP_ENV_ID}\") | .state" "$file")
rm -f "$file"

cat > "$SUMMARY_FILE" <<EOF2
# Full Staged E2E Smoke (${TS})

- repo_id: ${REPO_ID}
- team_id: ${TEAM_ID}
- repo: ${REPO}
- workspace_id: ${WORKSPACE_ID}
- changeset_id: ${CHANGESET_ID}
- release_id: ${RELEASE_ID}
- release_tag: ${RELEASE_TAG}
- deployment_id: ${DEPLOYMENT_ID}
- promote_deployment_id: ${PROMOTE_DEPLOYMENT_ID}
- rollback_deployment_id: ${ROLLBACK_DEPLOYMENT_ID}
- temp_env_id: ${TEMP_ENV_ID}
- temp_env_url: ${TEMP_ENV_URL}
- workspace_write_commit_sha: ${WRITE_SHA}
- release_published_sha: ${PUBLISHED_SHA}
- final_changeset_state: ${CHANGESET_STATE}
- final_release_state: ${RELEASE_STATE}
- final_deployment_state: ${DEPLOY_STATE}
- final_promote_state: ${PROMOTE_STATE}
- final_rollback_state: ${ROLLBACK_STATE}
- final_temp_env_state: ${TEMP_ENV_FINAL_STATE}
- blocked_path_guardrail_verified: true
- file_size_guardrail_verified: true
- terminal_job_succeeded: $(jq "[.data[] | select(.state==\"succeeded\")] | length" "$JOBS_FILE")
- terminal_job_failed: $(jq "[.data[] | select(.state==\"failed\")] | length" "$JOBS_FILE")
- terminal_job_canceled: $(jq "[.data[] | select(.state==\"canceled\")] | length" "$JOBS_FILE")

## Notes

- Validated signup bootstrap and team-scoped invite flow.
- Validated live gitaly-rs UserCommitFiles write path via PUT /workspaces/:id/files.
- Drove submit/merge/deploy/promote/rollback gates through async jobs to terminal success.
- Drove temp-env provision/expire/undo-expire paths through async jobs and API transitions.
EOF2

cp "$CREATE_REPO_JSON" "$RESULTS/latest-create-repo.json"
cat > "$RESULTS/latest-gitaly-repo.json" <<EOF2
{"timestamp":"${TS}","repo":"${REPO}","workdir":"${WORKDIR}"}
EOF2

rm -f "$JOBS_FILE"

echo "[11/11] Done"
echo "$SUMMARY_FILE"
