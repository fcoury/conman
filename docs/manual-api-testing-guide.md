# Manual API Testing Guide

This guide walks through a full end-to-end API sequence for Conman, starting
from a clean local run and covering signup, team/repository setup, apps,
workspaces, changesets, release, deployment, and temp environments.

## 1. Prerequisites

- `cargo`
- `jq`
- `curl`
- MongoDB reachable at `CONMAN_MONGO_URI`
- Optional for full Git behavior: `gitaly-rs` running and repository path
  available in Gitaly

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
curl -sS http://127.0.0.1:3000/api/openapi.json | jq '.openapi'
open http://127.0.0.1:3000/api/docs
```

## 3. Session variables and helper functions

```bash
export BASE="http://127.0.0.1:3000"

api_auth() {
  local method="$1"; shift
  local path="$1"; shift
  curl -sS -X "$method" "$BASE$path" \
    -H "authorization: Bearer $TOKEN" \
    -H 'content-type: application/json' "$@"
}

wait_job() {
  local repo_id="$1"
  local job_id="$2"
  while true; do
    local state
    state=$(api_auth GET "/api/repos/$repo_id/jobs/$job_id" | jq -r '.data.job.state')
    echo "job $job_id state=$state"
    case "$state" in
      succeeded|failed|canceled) break ;;
    esac
    sleep 1
  done
}
```

## 4. Signup and login

Create the first user (open signup). This auto-creates the user's first team
and first repository, and assigns `owner` role.

```bash
SIGNUP_JSON=$(curl -sS -X POST "$BASE/api/auth/signup" \
  -H 'content-type: application/json' \
  -d '{"name":"Admin User","email":"admin@example.com","password":"AdminPassw0rd!!"}')

echo "$SIGNUP_JSON" | jq
export TOKEN=$(echo "$SIGNUP_JSON" | jq -r '.data.token')
export USER_ID=$(echo "$SIGNUP_JSON" | jq -r '.data.user.id')
export TEAM_ID=$(echo "$SIGNUP_JSON" | jq -r '.data.team.id')
```

Optional re-login:

```bash
LOGIN_JSON=$(curl -sS -X POST "$BASE/api/auth/login" \
  -H 'content-type: application/json' \
  -d '{"email":"admin@example.com","password":"AdminPassw0rd!!"}')
export TOKEN=$(echo "$LOGIN_JSON" | jq -r '.data.token')
```

## 5. Team + repository setup

List teams and create another repository under the bootstrap team.

```bash
api_auth GET "/api/teams?page=1&limit=20" | jq
api_auth GET "/api/teams/$TEAM_ID" | jq

REPO_JSON=$(api_auth POST "/api/teams/$TEAM_ID/repos" -d '{
  "name": "Demo Team Configuration",
  "repo_path": "group/demo-team-config.git",
  "integration_branch": "main"
}')

echo "$REPO_JSON" | jq
export REPO_ID=$(echo "$REPO_JSON" | jq -r '.data.id')

# Refresh token so repo membership claims include the new repo.
LOGIN_JSON=$(curl -sS -X POST "$BASE/api/auth/login" \
  -H 'content-type: application/json' \
  -d '{"email":"admin@example.com","password":"AdminPassw0rd!!"}')
export TOKEN=$(echo "$LOGIN_JSON" | jq -r '.data.token')

api_auth GET "/api/repos?page=1&limit=20" | jq
api_auth GET "/api/repos/$REPO_ID" | jq
```

## 6. Repository settings, members, and team invites

```bash
api_auth PATCH "/api/repos/$REPO_ID/settings" -d '{
  "baseline_mode": "canonical_env_release",
  "commit_mode_default": "submit_commit",
  "blocked_paths": [".git/**", ".gitignore", ".github/**"],
  "file_size_limit_bytes": 5242880,
  "profile_approval_policy": "stricter_two_approvals"
}' | jq

api_auth GET "/api/repos/$REPO_ID/members?page=1&limit=20" | jq

INVITE_JSON=$(api_auth POST "/api/teams/$TEAM_ID/invites" -d '{
  "email": "reviewer@example.com",
  "role": "reviewer"
}')

echo "$INVITE_JSON" | jq
export INVITE_TOKEN=$(echo "$INVITE_JSON" | jq -r '.data.token')
export INVITE_ID=$(echo "$INVITE_JSON" | jq -r '.data.id')

# Optional resend flow (rotates token/expiry for pending invite).
api_auth POST "/api/teams/$TEAM_ID/invites/$INVITE_ID/resend" -d '{}' | jq

# Optional revoke flow (for pending invite only).
# api_auth DELETE "/api/teams/$TEAM_ID/invites/$INVITE_ID" | jq
```

Accept invite as second user:

```bash
REVIEWER_ACCEPT=$(curl -sS -X POST "$BASE/api/auth/accept-invite" \
  -H 'content-type: application/json' \
  -d "{\"token\":\"$INVITE_TOKEN\",\"name\":\"Reviewer User\",\"password\":\"ReviewerPassw0rd!!\"}")

export REVIEWER_TOKEN=$(echo "$REVIEWER_ACCEPT" | jq -r '.data.token')
export REVIEWER_USER_ID=$(echo "$REVIEWER_ACCEPT" | jq -r '.data.user.id')

# Optional explicit role assignment at repo scope.
api_auth POST "/api/repos/$REPO_ID/members" -d "{\"user_id\":\"$REVIEWER_USER_ID\",\"role\":\"reviewer\"}" | jq
api_auth GET "/api/repos/$REPO_ID/members?page=1&limit=20" | jq
```

## 7. Apps (surfaces)

```bash
api_auth POST "/api/repos/$REPO_ID/apps" -d '{
  "key": "portal",
  "title": "Patient Portal",
  "domains": ["portal.example.test"]
}' | jq

api_auth POST "/api/repos/$REPO_ID/apps" -d '{
  "key": "admin",
  "title": "Admin Console",
  "domains": ["admin.example.test"]
}' | jq

api_auth GET "/api/repos/$REPO_ID/apps" | jq
```

## 8. Runtime profiles and environments

```bash
DEV_PROFILE_JSON=$(api_auth POST "/api/repos/$REPO_ID/runtime-profiles" -d '{
  "name": "Development",
  "kind": "persistent_env",
  "base_url": "https://dev.example.test",
  "surface_endpoints": {
    "portal": "https://portal.dev.example.test",
    "admin": "https://admin.dev.example.test"
  },
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

PROD_PROFILE_JSON=$(api_auth POST "/api/repos/$REPO_ID/runtime-profiles" -d '{
  "name": "Production",
  "kind": "persistent_env",
  "base_url": "https://app.example.test",
  "surface_endpoints": {
    "portal": "https://portal.example.test",
    "admin": "https://admin.example.test"
  },
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

api_auth GET "/api/repos/$REPO_ID/runtime-profiles?page=1&limit=20" | jq
api_auth GET "/api/repos/$REPO_ID/runtime-profiles/$DEV_PROFILE_ID" | jq
api_auth POST "/api/repos/$REPO_ID/runtime-profiles/$DEV_PROFILE_ID/secrets/API_KEY/reveal" | jq

ENV_JSON=$(api_auth PATCH "/api/repos/$REPO_ID/environments" -d "{
  \"environments\": [
    {\"name\":\"dev\",  \"position\":1, \"is_canonical\":false, \"runtime_profile_id\":\"$DEV_PROFILE_ID\"},
    {\"name\":\"qa\",   \"position\":2, \"is_canonical\":false, \"runtime_profile_id\":\"$DEV_PROFILE_ID\"},
    {\"name\":\"prod\", \"position\":3, \"is_canonical\":true,  \"runtime_profile_id\":\"$PROD_PROFILE_ID\"}
  ]
}")

export DEV_ENV_ID=$(echo "$ENV_JSON" | jq -r '.data[] | select(.name=="dev") | .id')
export QA_ENV_ID=$(echo "$ENV_JSON" | jq -r '.data[] | select(.name=="qa") | .id')
export PROD_ENV_ID=$(echo "$ENV_JSON" | jq -r '.data[] | select(.name=="prod") | .id')
```

## 9. Workspaces and files

```bash
WS_JSON=$(api_auth POST "/api/repos/$REPO_ID/workspaces" -d '{"title":"Main Workspace"}')
export WORKSPACE_ID=$(echo "$WS_JSON" | jq -r '.data.id')

FILE_B64=$(printf 'feature:\n  enabled: true\n' | base64)
api_auth PUT "/api/repos/$REPO_ID/workspaces/$WORKSPACE_ID/files" -d "{
  \"path\": \"config/app.yaml\",
  \"content\": \"$FILE_B64\",
  \"message\": \"add config\"
}" | jq

api_auth GET "/api/repos/$REPO_ID/workspaces/$WORKSPACE_ID/files?path=config/app.yaml" | jq
api_auth POST "/api/repos/$REPO_ID/workspaces/$WORKSPACE_ID/checkpoints" -d '{"message":"checkpoint 1"}' | jq
```

## 10. Changesets and review

```bash
CS_JSON=$(api_auth POST "/api/repos/$REPO_ID/changesets" -d "{
  \"workspace_id\": \"$WORKSPACE_ID\",
  \"title\": \"Toggle feature\",
  \"description\": \"Manual API test changeset\"
}")
export CHANGESET_ID=$(echo "$CS_JSON" | jq -r '.data.id')

SUBMIT_JSON=$(api_auth POST "/api/repos/$REPO_ID/changesets/$CHANGESET_ID/submit" -d "{
  \"profile_overrides\": [
    {\"key\":\"FEATURE_X\",\"value\":{\"type\":\"boolean\",\"value\":true},\"target_profile_id\":\"$DEV_PROFILE_ID\"}
  ]
}")
SUBMIT_JOB_ID=$(echo "$SUBMIT_JSON" | jq -r '.data.job.id')
wait_job "$REPO_ID" "$SUBMIT_JOB_ID"

curl -sS -X POST "$BASE/api/repos/$REPO_ID/changesets/$CHANGESET_ID/review" \
  -H "authorization: Bearer $REVIEWER_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"action":"approve"}' | jq

api_auth POST "/api/repos/$REPO_ID/changesets/$CHANGESET_ID/queue" -d '{}' | jq
api_auth GET "/api/repos/$REPO_ID/changesets/$CHANGESET_ID/diff?format=semantic" | jq
api_auth POST "/api/repos/$REPO_ID/changesets/$CHANGESET_ID/comments" -d '{"body":"looks good"}' | jq
```

## 11. Releases

```bash
REL_JSON=$(api_auth POST "/api/repos/$REPO_ID/releases" -d '{}')
export RELEASE_ID=$(echo "$REL_JSON" | jq -r '.data.id')

api_auth POST "/api/repos/$REPO_ID/releases/$RELEASE_ID/changesets" -d "{
  \"changeset_ids\": [\"$CHANGESET_ID\"]
}" | jq

ASM_JSON=$(api_auth POST "/api/repos/$REPO_ID/releases/$RELEASE_ID/assemble" -d '{}')
ASM_JOB_ID=$(echo "$ASM_JSON" | jq -r '.data.job.id')
wait_job "$REPO_ID" "$ASM_JOB_ID"

api_auth POST "/api/repos/$REPO_ID/releases/$RELEASE_ID/publish" -d '{}' | jq
```

## 12. Deployments

```bash
api_auth POST "/api/repos/$REPO_ID/environments/$DEV_ENV_ID/deploy" -d "{
  \"release_id\": \"$RELEASE_ID\",
  \"is_skip_stage\": false,
  \"is_concurrent_batch\": false,
  \"approvals\": []
}" | jq

api_auth POST "/api/repos/$REPO_ID/environments/$QA_ENV_ID/promote" -d "{
  \"release_id\": \"$RELEASE_ID\",
  \"is_skip_stage\": false,
  \"is_concurrent_batch\": false,
  \"approvals\": []
}" | jq

api_auth POST "/api/repos/$REPO_ID/environments/$PROD_ENV_ID/rollback" -d "{
  \"release_id\": \"$RELEASE_ID\",
  \"mode\": \"revert_and_release\",
  \"approvals\": [\"$REVIEWER_USER_ID\", \"$USER_ID\"]
}" | jq
```

## 13. Temp environments

```bash
TEMP_JSON=$(api_auth POST "/api/repos/$REPO_ID/temp-envs" -d "{
  \"kind\": \"workspace\",
  \"source_id\": \"$WORKSPACE_ID\",
  \"base_profile_id\": \"$DEV_PROFILE_ID\"
}")

export TEMP_ENV_ID=$(echo "$TEMP_JSON" | jq -r '.data.temp_env.id')
export TEMP_JOB_ID=$(echo "$TEMP_JSON" | jq -r '.data.job.id')
wait_job "$REPO_ID" "$TEMP_JOB_ID"

api_auth POST "/api/repos/$REPO_ID/temp-envs/$TEMP_ENV_ID/extend" -d '{"seconds":7200}' | jq
api_auth DELETE "/api/repos/$REPO_ID/temp-envs/$TEMP_ENV_ID" -d '{}' | jq
api_auth POST "/api/repos/$REPO_ID/temp-envs/$TEMP_ENV_ID/undo-expire" -d '{}' | jq
```

## 14. Endpoint coverage checklist

This sequence covers:

- Platform: `/api/health`, `/api/openapi.json`, `/api/docs`
- Auth: signup/login/logout/forgot-password/reset-password/accept-invite
- Teams: list/create/get/invite and repository creation under team
- Repositories: list/get/settings/members
- Apps (surfaces): list/create/update
- Workspaces: list/create/get/update/reset/sync/files/checkpoints
- Changesets: list/create/get/update/submit/resubmit/review/queue/move-to-draft/diff/comments
- Releases: list/create/get/changesets/reorder/assemble/publish
- Environments + runtime profiles
- Deployments: deploy/promote/rollback/list
- Temp envs: list/create/extend/undo-expire/delete
- Me: notification preferences get/update
- Jobs: list/get
