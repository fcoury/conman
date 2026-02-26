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
USER_ID=$(python3 - <<"PY"
import secrets
print(secrets.token_hex(12))
PY
)
REVIEWER_ID=$(python3 - <<"PY"
import secrets
print(secrets.token_hex(12))
PY
)
JWT_SECRET="test-secret-test-secret-test-1234"
MONGO_URI="${MONGO_URI:-mongodb://127.0.0.1:27019}"

cleanup() {
  tmux kill-session -t "$SESSION" >/dev/null 2>&1 || true
}
trap cleanup EXIT

make_token() {
  local roles_json="$1"
  python3 - "$USER_ID" "$JWT_SECRET" "$roles_json" <<"PY"
import sys, json, time, hmac, hashlib, base64
sub=sys.argv[1]
secret=sys.argv[2].encode()
roles=json.loads(sys.argv[3])
now=int(time.time())
header={"alg":"HS256","typ":"JWT"}
payload={"sub":sub,"email":"e2e@example.com","roles":roles,"iat":now,"exp":now+24*3600}
enc=lambda obj: base64.urlsafe_b64encode(json.dumps(obj,separators=(",",":"),sort_keys=True).encode()).decode().rstrip("=")
h=f"{enc(header)}.{enc(payload)}"
sig=base64.urlsafe_b64encode(hmac.new(secret,h.encode(),hashlib.sha256).digest()).decode().rstrip("=")
print(f"{h}.{sig}")
PY
}

request() {
  local method="$1" path="$2" token="$3" body="${4-}"
  local tmp status
  tmp=$(mktemp)
  if [[ -n "$body" ]]; then
    status=$(curl -sS -o "$tmp" -w "%{http_code}" -X "$method" "http://127.0.0.1:${PORT}${path}" \
      -H "Authorization: Bearer $token" -H "Content-Type: application/json" --data "$body")
  else
    status=$(curl -sS -o "$tmp" -w "%{http_code}" -X "$method" "http://127.0.0.1:${PORT}${path}" \
      -H "Authorization: Bearer $token")
  fi
  echo "$status|$tmp"
}

request_assert_200() {
  local method="$1" path="$2" token="$3" body="${4-}"
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
  local expected="$1" method="$2" path="$3" token="$4" body="${5-}"
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
  local app_id="$1" token="$2" job_type="$3" entity_id_pattern="$4"
  local file
  file=$(request_assert_200 GET "/api/apps/${app_id}/jobs?page=1&limit=100" "$token")
  jq -r ".data[] | select(.job_type==\"${job_type}\") | select(.entity_id|test(\"${entity_id_pattern}\")) | .id" "$file" | head -n1
  rm -f "$file"
}

job_state() {
  local app_id="$1" token="$2" job_id="$3"
  local file
  file=$(request_assert_200 GET "/api/apps/${app_id}/jobs/${job_id}" "$token")
  jq -r ".data.job.state" "$file"
  rm -f "$file"
}

wait_job() {
  local app_id="$1" token="$2" job_id="$3" label="$4"
  for _ in {1..120}; do
    state=$(job_state "$app_id" "$token" "$job_id")
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

TOKEN_BOOT=$(make_token "{}")

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

if command -v mongosh >/dev/null 2>&1; then
  mongosh "${MONGO_URI}/${DB}" --quiet --eval "db.users.updateOne(
    { _id: ObjectId(\"${USER_ID}\") },
    {
      \$set: {
        email: \"e2e@example.com\",
        name: \"E2E Runner\",
        password_hash: \"not-used-in-staged-smoke\",
        created_at: new Date(),
        updated_at: new Date()
      }
    },
    { upsert: true }
  )" >/dev/null
  mongosh "${MONGO_URI}/${DB}" --quiet --eval "db.users.updateOne(
    { _id: ObjectId(\"${REVIEWER_ID}\") },
    {
      \$set: {
        email: \"reviewer@example.com\",
        name: \"E2E Reviewer\",
        password_hash: \"not-used-in-staged-smoke\",
        created_at: new Date(),
        updated_at: new Date()
      }
    },
    { upsert: true }
  )" >/dev/null
fi

echo "[3/11] Creating app"
file=$(request_assert_200 POST "/api/apps" "$TOKEN_BOOT" "{\"name\":\"E2E ${TS}\",\"repo_path\":\"${REPO}\",\"integration_branch\":\"main\"}")
APP_ID=$(jq -r ".data.id" "$file")
rm -f "$file"

TOKEN_APP=$(make_token "{\"${APP_ID}\":\"app_admin\"}")
request_assert_200 POST "/api/apps/${APP_ID}/members" "$TOKEN_APP" "{\"user_id\":\"${REVIEWER_ID}\",\"role\":\"reviewer\"}" >/dev/null

echo "[4/11] Setting environments + creating workspace"
file=$(request_assert_200 PATCH "/api/apps/${APP_ID}/environments" "$TOKEN_APP" "{\"environments\":[{\"name\":\"prod\",\"position\":1,\"is_canonical\":true,\"runtime_profile_id\":null}]}")
ENV_ID=$(jq -r ".data[0].id" "$file")
rm -f "$file"

file=$(request_assert_200 GET "/api/apps/${APP_ID}/workspaces" "$TOKEN_APP")
WORKSPACE_ID=$(jq -r ".data[0].id" "$file")
rm -f "$file"

request_assert_200 PATCH "/api/apps/${APP_ID}/settings" "$TOKEN_APP" "{\"file_size_limit_bytes\":128}" >/dev/null
request_expect_status 403 PUT "/api/apps/${APP_ID}/workspaces/${WORKSPACE_ID}/files" "$TOKEN_APP" "{\"path\":\".git/config\",\"content\":\"YmxvY2tlZA==\",\"message\":\"blocked-path-check\"}"
LARGE_CONTENT_B64=$(python3 - <<'PY'
import base64
print(base64.b64encode(b"a" * 512).decode())
PY
)
request_expect_status 400 PUT "/api/apps/${APP_ID}/workspaces/${WORKSPACE_ID}/files" "$TOKEN_APP" "{\"path\":\"config/too-big.txt\",\"content\":\"${LARGE_CONTENT_B64}\",\"message\":\"size-guardrail-check\"}"

echo "[5/11] Writing workspace file (UserCommitFiles path)"
CONTENT_B64=$(printf 'feature: staged-%s\n' "$TS" | base64 | tr -d '\n')
file=$(request_assert_200 PUT "/api/apps/${APP_ID}/workspaces/${WORKSPACE_ID}/files" "$TOKEN_APP" "{\"path\":\"config/app.yaml\",\"content\":\"${CONTENT_B64}\",\"message\":\"update config\"}")
WRITE_SHA=$(jq -r ".data.commit_sha" "$file")
rm -f "$file"

echo "[6/11] Changeset create/submit/approve/queue"
file=$(request_assert_200 POST "/api/apps/${APP_ID}/changesets" "$TOKEN_APP" "{\"workspace_id\":\"${WORKSPACE_ID}\",\"title\":\"Staged changeset ${TS}\",\"description\":\"e2e\"}")
CHANGESET_ID=$(jq -r ".data.id" "$file")
rm -f "$file"

file=$(request_assert_200 POST "/api/apps/${APP_ID}/changesets/${CHANGESET_ID}/submit" "$TOKEN_APP" "{}")
SUBMIT_JOB_ID=$(jq -r ".data.job.id" "$file")
rm -f "$file"
wait_job "$APP_ID" "$TOKEN_APP" "$SUBMIT_JOB_ID" "msuite_submit"

request_assert_200 POST "/api/apps/${APP_ID}/changesets/${CHANGESET_ID}/review" "$TOKEN_APP" "{\"action\":\"approve\"}" >/dev/null
request_assert_200 POST "/api/apps/${APP_ID}/changesets/${CHANGESET_ID}/queue" "$TOKEN_APP" >/dev/null

echo "[7/11] Release draft/assemble/publish"
file=$(request_assert_200 POST "/api/apps/${APP_ID}/releases" "$TOKEN_APP")
RELEASE_ID=$(jq -r ".data.id" "$file")
RELEASE_TAG=$(jq -r ".data.tag" "$file")
rm -f "$file"

request_assert_200 POST "/api/apps/${APP_ID}/releases/${RELEASE_ID}/changesets" "$TOKEN_APP" "{\"changeset_ids\":[\"${CHANGESET_ID}\"]}" >/dev/null
file=$(request_assert_200 POST "/api/apps/${APP_ID}/releases/${RELEASE_ID}/assemble" "$TOKEN_APP")
ASSEMBLE_JOB_ID=$(jq -r ".data.job.id" "$file")
rm -f "$file"
wait_job "$APP_ID" "$TOKEN_APP" "$ASSEMBLE_JOB_ID" "release_assemble"

r=$(request POST "/api/apps/${APP_ID}/releases/${RELEASE_ID}/publish" "$TOKEN_APP")
status=${r%%|*}; file=${r#*|}
if [[ "$status" == "409" ]]; then
  rm -f "$file"
  MERGE_JOB_ID=$(latest_job_id "$APP_ID" "$TOKEN_APP" "msuite_merge" "${RELEASE_ID}")
  wait_job "$APP_ID" "$TOKEN_APP" "$MERGE_JOB_ID" "msuite_merge"
  file=$(request_assert_200 POST "/api/apps/${APP_ID}/releases/${RELEASE_ID}/publish" "$TOKEN_APP")
elif [[ "$status" != "200" ]]; then
  cat "$file" >&2
  rm -f "$file"
  exit 1
fi
PUBLISHED_SHA=$(jq -r ".data.release.published_sha" "$file")
rm -f "$file"

echo "[8/11] Deploy gates + deployment"
r=$(request POST "/api/apps/${APP_ID}/environments/${ENV_ID}/deploy" "$TOKEN_APP" "{\"release_id\":\"${RELEASE_ID}\"}")
status=${r%%|*}; file=${r#*|}
if [[ "$status" != "409" ]]; then
  cat "$file" >&2
  rm -f "$file"
  exit 1
fi
rm -f "$file"
DRIFT_JOB_ID=$(latest_job_id "$APP_ID" "$TOKEN_APP" "runtime_profile_drift_check" "${ENV_ID}")
wait_job "$APP_ID" "$TOKEN_APP" "$DRIFT_JOB_ID" "drift_check"

r=$(request POST "/api/apps/${APP_ID}/environments/${ENV_ID}/deploy" "$TOKEN_APP" "{\"release_id\":\"${RELEASE_ID}\"}")
status=${r%%|*}; file=${r#*|}
if [[ "$status" != "409" ]]; then
  cat "$file" >&2
  rm -f "$file"
  exit 1
fi
rm -f "$file"
DEPLOY_GATE_JOB_ID=$(latest_job_id "$APP_ID" "$TOKEN_APP" "msuite_deploy" "${ENV_ID}:${RELEASE_ID}")
wait_job "$APP_ID" "$TOKEN_APP" "$DEPLOY_GATE_JOB_ID" "msuite_deploy"

file=$(request_assert_200 POST "/api/apps/${APP_ID}/environments/${ENV_ID}/deploy" "$TOKEN_APP" "{\"release_id\":\"${RELEASE_ID}\"}")
DEPLOYMENT_JOB_ID=$(jq -r ".data.job.id" "$file")
DEPLOYMENT_ID=$(jq -r ".data.deployment.id" "$file")
rm -f "$file"
wait_job "$APP_ID" "$TOKEN_APP" "$DEPLOYMENT_JOB_ID" "deploy_release"

file=$(request_assert_200 POST "/api/apps/${APP_ID}/environments/${ENV_ID}/promote" "$TOKEN_APP" "{\"release_id\":\"${RELEASE_ID}\"}")
PROMOTE_JOB_ID=$(jq -r ".data.job.id" "$file")
PROMOTE_DEPLOYMENT_ID=$(jq -r ".data.deployment.id" "$file")
rm -f "$file"
wait_job "$APP_ID" "$TOKEN_APP" "$PROMOTE_JOB_ID" "promote_release"

file=$(request_assert_200 POST "/api/apps/${APP_ID}/environments/${ENV_ID}/rollback" "$TOKEN_APP" "{\"release_id\":\"${RELEASE_ID}\",\"mode\":\"redeploy_prior_tag\",\"approvals\":[\"${USER_ID}\",\"${REVIEWER_ID}\"]}")
ROLLBACK_JOB_ID=$(jq -r ".data.job.id" "$file")
ROLLBACK_DEPLOYMENT_ID=$(jq -r ".data.deployment.id" "$file")
rm -f "$file"
wait_job "$APP_ID" "$TOKEN_APP" "$ROLLBACK_JOB_ID" "rollback_release"

echo "[9/11] Temp environment lifecycle"
file=$(request_assert_200 POST "/api/apps/${APP_ID}/temp-envs" "$TOKEN_APP" "{\"kind\":\"workspace\",\"source_id\":\"${WORKSPACE_ID}\"}")
TEMP_ENV_ID=$(jq -r ".data.temp_env.id" "$file")
TEMP_ENV_URL=$(jq -r ".data.temp_env.url" "$file")
TEMP_PROVISION_JOB_ID=$(jq -r ".data.job.id" "$file")
rm -f "$file"
wait_job "$APP_ID" "$TOKEN_APP" "$TEMP_PROVISION_JOB_ID" "temp_env_provision"
request_assert_200 POST "/api/apps/${APP_ID}/temp-envs/${TEMP_ENV_ID}/extend" "$TOKEN_APP" "{\"seconds\":7200}" >/dev/null
file=$(request_assert_200 DELETE "/api/apps/${APP_ID}/temp-envs/${TEMP_ENV_ID}" "$TOKEN_APP")
TEMP_EXPIRE_JOB_ID=$(jq -r ".data.job.id" "$file")
rm -f "$file"
wait_job "$APP_ID" "$TOKEN_APP" "$TEMP_EXPIRE_JOB_ID" "temp_env_expire"
request_assert_200 POST "/api/apps/${APP_ID}/temp-envs/${TEMP_ENV_ID}/undo-expire" "$TOKEN_APP" >/dev/null

echo "[10/11] Collecting final states"
file=$(request_assert_200 GET "/api/apps/${APP_ID}/jobs?page=1&limit=100" "$TOKEN_APP")
JOBS_FILE="$file"

file=$(request_assert_200 GET "/api/apps/${APP_ID}/changesets/${CHANGESET_ID}" "$TOKEN_APP")
CHANGESET_STATE=$(jq -r ".data.changeset.state" "$file")
rm -f "$file"
file=$(request_assert_200 GET "/api/apps/${APP_ID}/releases/${RELEASE_ID}" "$TOKEN_APP")
RELEASE_STATE=$(jq -r ".data.state" "$file")
rm -f "$file"
file=$(request_assert_200 GET "/api/apps/${APP_ID}/deployments?page=1&limit=50" "$TOKEN_APP")
DEPLOY_STATE=$(jq -r ".data[] | select(.id==\"${DEPLOYMENT_ID}\") | .state" "$file")
PROMOTE_STATE=$(jq -r ".data[] | select(.id==\"${PROMOTE_DEPLOYMENT_ID}\") | .state" "$file")
ROLLBACK_STATE=$(jq -r ".data[] | select(.id==\"${ROLLBACK_DEPLOYMENT_ID}\") | .state" "$file")
rm -f "$file"
file=$(request_assert_200 GET "/api/apps/${APP_ID}/temp-envs?page=1&limit=50" "$TOKEN_APP")
TEMP_ENV_FINAL_STATE=$(jq -r ".data[] | select(.id==\"${TEMP_ENV_ID}\") | .state" "$file")
rm -f "$file"

cat > "$SUMMARY_FILE" <<EOF
# Full Staged E2E Smoke (${TS})

- app_id: ${APP_ID}
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

- Validated live gitaly-rs UserCommitFiles write path via PUT /workspaces/:id/files.
- Drove submit/merge/deploy/promote/rollback gates through async jobs to terminal success.
- Drove temp-env provision/expire/undo-expire paths through async jobs and API transitions.
EOF

cp "$CREATE_REPO_JSON" "$RESULTS/latest-create-repo.json"
cat > "$RESULTS/latest-gitaly-repo.json" <<EOF
{"timestamp":"${TS}","repo":"${REPO}","workdir":"${WORKDIR}"}
EOF

rm -f "$JOBS_FILE"

echo "[11/11] Done"
echo "$SUMMARY_FILE"
