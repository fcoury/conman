# Manual API Testing Guide

This guide walks through a full logical test sequence for the current Conman API,
including first-user bootstrap, app setup, review/release flow, deployment flow,
and temp environments.

## 1. Prerequisites

- `cargo`
- `jq`
- `curl`
- MongoDB reachable at `CONMAN_MONGO_URI`
- Optional but recommended for full Git behavior: `gitaly-rs` running and target
  repository path available in Gitaly

Notes:
- For full release/deploy validation behavior, keep the default gate commands
  (`true`) from `.env.example`, or replace them with your real commands.
- Default role hierarchy: `user < reviewer < config_manager < app_admin`.

## 2. Local setup and startup

```bash
cp .env.example .env
cargo run            # default command is `serve`
# or explicitly:
# cargo run -- serve
```

Health checks:

```bash
curl -sS http://127.0.0.1:3000/api/health | jq
curl -sS http://127.0.0.1:3000/api/metrics | head -n 20
curl -sS http://127.0.0.1:3000/api/openapi.json | jq '.openapi'
open http://127.0.0.1:3000/api/docs
```

## 3. Bootstrap the first login user (admin operator)

There is no public signup endpoint in v1. Create/update a first user directly via
Conman subcommand:

```bash
cargo run -- bootstrap-admin admin@example.com "Admin User" "AdminPassw0rd!!"
```

Optional DB override:

```bash
cargo run -- bootstrap-admin admin@example.com "Admin User" "AdminPassw0rd!!" \
  --mongo-uri mongodb://localhost:27017 \
  --mongo-db conman
```

## 4. Session variables and helpers

```bash
export BASE="http://127.0.0.1:3000"

# Login as bootstrap user.
ADMIN_LOGIN=$(curl -sS -X POST "$BASE/api/auth/login" \
  -H 'content-type: application/json' \
  -d '{"email":"admin@example.com","password":"AdminPassw0rd!!"}')
export ADMIN_TOKEN=$(echo "$ADMIN_LOGIN" | jq -r '.data.token')
export ADMIN_USER_ID=$(echo "$ADMIN_LOGIN" | jq -r '.data.user.id')

echo "$ADMIN_TOKEN" | head -c 24; echo

# Helper to call authenticated endpoints.
api() {
  local method="$1"; shift
  local path="$1"; shift
  curl -sS -X "$method" "$BASE$path" \
    -H "authorization: Bearer $ADMIN_TOKEN" \
    -H 'content-type: application/json' "$@"
}

# Helper to poll async jobs until terminal.
wait_job() {
  local app_id="$1"
  local job_id="$2"
  while true; do
    local state
    state=$(api GET "/api/apps/$app_id/jobs/$job_id" | jq -r '.data.job.state')
    echo "job $job_id state=$state"
    case "$state" in
      succeeded|failed|canceled) break ;;
    esac
    sleep 1
  done
}
```

## 5. Auth and self endpoints

```bash
# Logout endpoint (stateless response).
api POST /api/auth/logout | jq

# Forgot/reset flow (returns token in current implementation).
RESET_TOKEN=$(curl -sS -X POST "$BASE/api/auth/forgot-password" \
  -H 'content-type: application/json' \
  -d '{"email":"admin@example.com"}' | jq -r '.data.reset_token')

curl -sS -X POST "$BASE/api/auth/reset-password" \
  -H 'content-type: application/json' \
  -d "{\"token\":\"$RESET_TOKEN\",\"new_password\":\"AdminPassw0rd!!\"}" | jq

# Notification preferences.
api GET /api/me/notification-preferences | jq
api PATCH /api/me/notification-preferences -d '{"email_enabled":true}' | jq
```

## 6. Create app and base settings

```bash
APP_JSON=$(api POST /api/apps -d '{
  "name": "Demo App",
  "repo_path": "group/demo-app.git",
  "integration_branch": "main"
}')

echo "$APP_JSON" | jq
export APP_ID=$(echo "$APP_JSON" | jq -r '.data.id')

api GET /api/apps | jq
api GET "/api/apps/$APP_ID" | jq

api PATCH "/api/apps/$APP_ID/settings" -d '{
  "baseline_mode": "canonical_env_release",
  "commit_mode_default": "submit_commit",
  "blocked_paths": [".git/**", ".gitignore", ".github/**"],
  "file_size_limit_bytes": 5242880,
  "profile_approval_policy": "stricter_two_approvals"
}' | jq
```

## 7. Runtime profiles and environments

Create two persistent runtime profiles:

```bash
DEV_PROFILE_JSON=$(api POST "/api/apps/$APP_ID/runtime-profiles" -d '{
  "name": "Development",
  "kind": "persistent_env",
  "base_url": "https://dev.example.test",
  "env_vars": {
    "FEATURE_X": {"type":"boolean", "value": true},
    "MAX_ITEMS": {"type":"number", "value": 100}
  },
  "secrets": {
    "API_KEY": "dev-secret-key"
  },
  "database_engine": "mongodb",
  "connection_ref": "mongodb://dev-db:27017/conman_dev",
  "provisioning_mode": "managed",
  "migration_paths": ["migrations"],
  "migration_command": "echo migrate"
}')
export DEV_PROFILE_ID=$(echo "$DEV_PROFILE_JSON" | jq -r '.data.id')

PROD_PROFILE_JSON=$(api POST "/api/apps/$APP_ID/runtime-profiles" -d '{
  "name": "Production",
  "kind": "persistent_env",
  "base_url": "https://app.example.test",
  "env_vars": {
    "FEATURE_X": {"type":"boolean", "value": false}
  },
  "secrets": {
    "API_KEY": "prod-secret-key"
  },
  "database_engine": "mongodb",
  "connection_ref": "mongodb://prod-db:27017/conman_prod",
  "provisioning_mode": "managed",
  "migration_paths": ["migrations"],
  "migration_command": "echo migrate"
}')
export PROD_PROFILE_ID=$(echo "$PROD_PROFILE_JSON" | jq -r '.data.id')

api GET "/api/apps/$APP_ID/runtime-profiles" | jq
api GET "/api/apps/$APP_ID/runtime-profiles/$DEV_PROFILE_ID" | jq
api PATCH "/api/apps/$APP_ID/runtime-profiles/$DEV_PROFILE_ID" -d '{
  "base_url": "https://dev2.example.test"
}' | jq
api POST "/api/apps/$APP_ID/runtime-profiles/$DEV_PROFILE_ID/secrets/API_KEY/reveal" | jq
```

Replace environment set and mark canonical env:

```bash
ENV_JSON=$(api PATCH "/api/apps/$APP_ID/environments" -d "{
  \"environments\": [
    {\"name\":\"dev\",  \"position\":1, \"is_canonical\":false, \"runtime_profile_id\":\"$DEV_PROFILE_ID\"},
    {\"name\":\"qa\",   \"position\":2, \"is_canonical\":false, \"runtime_profile_id\":\"$DEV_PROFILE_ID\"},
    {\"name\":\"prod\", \"position\":3, \"is_canonical\":true,  \"runtime_profile_id\":\"$PROD_PROFILE_ID\"}
  ]
}")

echo "$ENV_JSON" | jq
export DEV_ENV_ID=$(echo "$ENV_JSON" | jq -r '.data[] | select(.name=="dev") | .id')
export QA_ENV_ID=$(echo "$ENV_JSON" | jq -r '.data[] | select(.name=="qa") | .id')
export PROD_ENV_ID=$(echo "$ENV_JSON" | jq -r '.data[] | select(.name=="prod") | .id')

api GET "/api/apps/$APP_ID/environments" | jq
```

## 8. Members and invites (create reviewer user)

```bash
INVITE_JSON=$(api POST "/api/apps/$APP_ID/invites" -d '{
  "email": "reviewer@example.com",
  "role": "reviewer"
}')

echo "$INVITE_JSON" | jq
export INVITE_TOKEN=$(echo "$INVITE_JSON" | jq -r '.data.token')

# Accept invite as second user.
REVIEWER_LOGIN=$(curl -sS -X POST "$BASE/api/auth/accept-invite" \
  -H 'content-type: application/json' \
  -d "{\"token\":\"$INVITE_TOKEN\",\"name\":\"Reviewer User\",\"password\":\"ReviewerPassw0rd!!\"}")
export REVIEWER_TOKEN=$(echo "$REVIEWER_LOGIN" | jq -r '.data.token')
export REVIEWER_USER_ID=$(echo "$REVIEWER_LOGIN" | jq -r '.data.user.id')

# Optional explicit role assignment endpoint.
api POST "/api/apps/$APP_ID/members" -d "{\"user_id\":\"$REVIEWER_USER_ID\",\"role\":\"reviewer\"}" | jq
api GET "/api/apps/$APP_ID/members" | jq
```

## 9. Workspace and file operations

```bash
WS_JSON=$(api POST "/api/apps/$APP_ID/workspaces" -d '{"title":"Main Workspace"}')
export WORKSPACE_ID=$(echo "$WS_JSON" | jq -r '.data.id')

echo "$WS_JSON" | jq
api GET "/api/apps/$APP_ID/workspaces" | jq
api GET "/api/apps/$APP_ID/workspaces/$WORKSPACE_ID" | jq
api PATCH "/api/apps/$APP_ID/workspaces/$WORKSPACE_ID" -d '{"title":"Main Workspace v2"}' | jq

# Write file (content must be base64).
FILE_B64=$(printf 'feature:\n  enabled: true\n' | base64)
api PUT "/api/apps/$APP_ID/workspaces/$WORKSPACE_ID/files" -d "{
  \"path\": \"config/app.yaml\",
  \"content\": \"$FILE_B64\",
  \"message\": \"add config\"
}" | jq

api GET "/api/apps/$APP_ID/workspaces/$WORKSPACE_ID/files?path=config/app.yaml" | jq
api GET "/api/apps/$APP_ID/workspaces/$WORKSPACE_ID/files?path=config" | jq

api POST "/api/apps/$APP_ID/workspaces/$WORKSPACE_ID/checkpoints" -d '{"message":"checkpoint 1"}' | jq
api POST "/api/apps/$APP_ID/workspaces/$WORKSPACE_ID/sync-integration" -d '{}' | jq
api DELETE "/api/apps/$APP_ID/workspaces/$WORKSPACE_ID/files" -d '{"path":"config/app.yaml"}' | jq
api POST "/api/apps/$APP_ID/workspaces/$WORKSPACE_ID/reset" -d '{}' | jq
```

## 10. Changesets: create -> submit -> review -> queue

Create a fresh file change first:

```bash
FILE_B64=$(printf 'feature:\n  enabled: false\n' | base64)
api PUT "/api/apps/$APP_ID/workspaces/$WORKSPACE_ID/files" -d "{
  \"path\": \"config/app.yaml\",
  \"content\": \"$FILE_B64\",
  \"message\": \"toggle feature\"
}" | jq
```

Create and submit changeset:

```bash
CS_JSON=$(api POST "/api/apps/$APP_ID/changesets" -d "{
  \"workspace_id\": \"$WORKSPACE_ID\",
  \"title\": \"Toggle feature\",
  \"description\": \"Manual API test changeset\"
}")
export CHANGESET_ID=$(echo "$CS_JSON" | jq -r '.data.id')

echo "$CS_JSON" | jq
api GET "/api/apps/$APP_ID/changesets?page=1&limit=20" | jq
api GET "/api/apps/$APP_ID/changesets/$CHANGESET_ID" | jq
api PATCH "/api/apps/$APP_ID/changesets/$CHANGESET_ID" -d '{"description":"Updated description"}' | jq

SUBMIT_JSON=$(api POST "/api/apps/$APP_ID/changesets/$CHANGESET_ID/submit" -d "{
  \"profile_overrides\": [
    {\"key\":\"FEATURE_X\",\"value\":{\"type\":\"boolean\",\"value\":true},\"target_profile_id\":\"$DEV_PROFILE_ID\"}
  ]
}")

echo "$SUBMIT_JSON" | jq
SUBMIT_JOB_ID=$(echo "$SUBMIT_JSON" | jq -r '.data.job.id')
wait_job "$APP_ID" "$SUBMIT_JOB_ID"
```

Review (as reviewer) and queue (as admin/config manager):

```bash
curl -sS -X POST "$BASE/api/apps/$APP_ID/changesets/$CHANGESET_ID/review" \
  -H "authorization: Bearer $REVIEWER_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"action":"approve"}' | jq

api POST "/api/apps/$APP_ID/changesets/$CHANGESET_ID/queue" -d '{}' | jq

# Diff + comments.
api GET "/api/apps/$APP_ID/changesets/$CHANGESET_ID/diff?format=semantic" | jq
api GET "/api/apps/$APP_ID/changesets/$CHANGESET_ID/diff?format=raw" | jq
api POST "/api/apps/$APP_ID/changesets/$CHANGESET_ID/comments" -d '{"body":"looks good"}' | jq
api GET "/api/apps/$APP_ID/changesets/$CHANGESET_ID/comments" | jq

# Optional draft transition from queued state.
api POST "/api/apps/$APP_ID/changesets/$CHANGESET_ID/move-to-draft" -d '{}' | jq
# Re-submit path (after moving draft and making additional edit).
api POST "/api/apps/$APP_ID/changesets/$CHANGESET_ID/resubmit" -d '{"profile_overrides":[]}' | jq
```

## 11. Release flow (queue-first)

Make sure the changeset is approved + queued before continuing.

```bash
REL_JSON=$(api POST "/api/apps/$APP_ID/releases" -d '{}')
export RELEASE_ID=$(echo "$REL_JSON" | jq -r '.data.id')

echo "$REL_JSON" | jq
api GET "/api/apps/$APP_ID/releases?page=1&limit=20" | jq
api GET "/api/apps/$APP_ID/releases/$RELEASE_ID" | jq

api POST "/api/apps/$APP_ID/releases/$RELEASE_ID/changesets" -d "{
  \"changeset_ids\": [\"$CHANGESET_ID\"]
}" | jq

api POST "/api/apps/$APP_ID/releases/$RELEASE_ID/reorder" -d "{
  \"changeset_ids\": [\"$CHANGESET_ID\"]
}" | jq

ASM_JSON=$(api POST "/api/apps/$APP_ID/releases/$RELEASE_ID/assemble" -d '{}')
ASM_JOB_ID=$(echo "$ASM_JSON" | jq -r '.data.job.id')
wait_job "$APP_ID" "$ASM_JOB_ID"

# Publish can return 409 first time if merge gate job gets enqueued.
PUBLISH_STATUS=$(api POST "/api/apps/$APP_ID/releases/$RELEASE_ID/publish" -d '{}' | tee /tmp/publish.json | jq -r '.data.release.id // empty')
if [ -z "$PUBLISH_STATUS" ]; then
  echo "publish returned non-success; poll jobs and retry"
  api GET "/api/apps/$APP_ID/jobs?page=1&limit=50" | jq
  # Retry publish until success:
  api POST "/api/apps/$APP_ID/releases/$RELEASE_ID/publish" -d '{}' | jq
else
  cat /tmp/publish.json | jq
fi
```

## 12. Deploy, promote, rollback

Deploy uses gate jobs (drift check and msuite deploy). First deploy calls may return
`409` with gate-enqueued messages; retry after gate job success.

```bash
# Attempt deploy to dev.
api POST "/api/apps/$APP_ID/environments/$DEV_ENV_ID/deploy" -d "{
  \"release_id\": \"$RELEASE_ID\",
  \"is_skip_stage\": false,
  \"is_concurrent_batch\": false,
  \"approvals\": []
}" | jq

# Inspect/poll jobs, then retry deploy until you get data.deployment + data.job.
api GET "/api/apps/$APP_ID/jobs?page=1&limit=50" | jq

# Promote to QA.
api POST "/api/apps/$APP_ID/environments/$QA_ENV_ID/promote" -d "{
  \"release_id\": \"$RELEASE_ID\",
  \"is_skip_stage\": false,
  \"is_concurrent_batch\": false,
  \"approvals\": []
}" | jq

# Exceptional concurrent/skip-stage deploy example (requires two distinct approvers,
# with at least one config_manager/app_admin).
api POST "/api/apps/$APP_ID/environments/$PROD_ENV_ID/deploy" -d "{
  \"release_id\": \"$RELEASE_ID\",
  \"is_skip_stage\": true,
  \"is_concurrent_batch\": false,
  \"approvals\": [\"$REVIEWER_USER_ID\", \"$ADMIN_USER_ID\"]
}" | jq

# Rollback (mode: revert_and_release or redeploy_prior_tag).
api POST "/api/apps/$APP_ID/environments/$PROD_ENV_ID/rollback" -d "{
  \"release_id\": \"$RELEASE_ID\",
  \"mode\": \"revert_and_release\",
  \"approvals\": [\"$REVIEWER_USER_ID\", \"$ADMIN_USER_ID\"]
}" | jq

api GET "/api/apps/$APP_ID/deployments?page=1&limit=50" | jq
```

## 13. Temp environments

```bash
TEMP_JSON=$(api POST "/api/apps/$APP_ID/temp-envs" -d "{
  \"kind\": \"workspace\",
  \"source_id\": \"$WORKSPACE_ID\",
  \"base_profile_id\": \"$DEV_PROFILE_ID\"
}")

echo "$TEMP_JSON" | jq
export TEMP_ENV_ID=$(echo "$TEMP_JSON" | jq -r '.data.temp_env.id')
export TEMP_JOB_ID=$(echo "$TEMP_JSON" | jq -r '.data.job.id')
wait_job "$APP_ID" "$TEMP_JOB_ID"

api GET "/api/apps/$APP_ID/temp-envs?page=1&limit=20" | jq
api POST "/api/apps/$APP_ID/temp-envs/$TEMP_ENV_ID/extend" -d '{"seconds":7200}' | jq
api DELETE "/api/apps/$APP_ID/temp-envs/$TEMP_ENV_ID" -d '{}' | jq
api POST "/api/apps/$APP_ID/temp-envs/$TEMP_ENV_ID/undo-expire" -d '{}' | jq
```

## 14. Jobs endpoint usage for all async flows

```bash
api GET "/api/apps/$APP_ID/jobs?page=1&limit=100" | jq

# Pick one job id and inspect logs.
JOB_ID=$(api GET "/api/apps/$APP_ID/jobs?page=1&limit=1" | jq -r '.data[0].id')
api GET "/api/apps/$APP_ID/jobs/$JOB_ID" | jq
```

## 15. Endpoint coverage checklist

This sequence exercises every currently wired route in `conman-api/src/router.rs`:

- Platform: `/api/health`, `/api/metrics`, `/api/openapi.json`, `/api/docs`
- Auth: login/logout/forgot-password/reset-password/accept-invite
- Apps: list/create/get/settings/members/invites
- Workspaces: list/create/get/update/reset/sync/files/checkpoints
- Changesets: list/create/get/update/submit/resubmit/review/queue/move-to-draft/diff/comments
- Releases: list/create/get/changesets/reorder/assemble/publish
- Environments + runtime profiles: list/replace + profile list/create/get/update/reveal-secret
- Deployments: deploy/promote/rollback/list
- Temp envs: list/create/extend/undo-expire/delete
- Me: notification preferences get/update
- Jobs: list/get

## 16. Common failure modes

- `403 missing bearer token`:
  token missing or expired.
- `403 role/capability required`:
  wrong role for endpoint (`app_admin` vs `config_manager` vs `reviewer`).
- `409 gate not satisfied`:
  async gate job was enqueued; poll jobs and retry action.
- `400 content must be base64` for workspace writes:
  file payload content must be base64.
- `409 changeset must be approved before queueing`:
  review with `approve` first.
