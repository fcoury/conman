# Team/Repo/App Implementation Plan

## Goal

Implement the new domain direction:

- `Team -> Config Repository -> App`
- Keep Git lifecycle repo-scoped (`workspace -> changeset -> release -> deploy`)
- Add multi-app support for URLs/runtime profile context

This plan is optimized for current reality: pre-production and one active user.

## What changes

1. Current `App` becomes a repository concept in practice.
2. Add first-class `Team`.
3. Add first-class `App` under each repository.
4. Keep `/api/repos` and `/api/teams` as the canonical API endpoints.
5. Runtime profiles gain app endpoint mapping.

## What stays the same

- Queue-first changesets/releases model.
- One integration branch per repository.
- Environment pipeline and deploy flow.
- gitaly-rs integration boundary.

## Implementation order (critical path)

## Step 1: Domain + storage model

Crates/files:

- `conman-core`: add `team.rs`, `repo.rs`, `app.rs`.
- `conman-db`: add `team_repo.rs`, `repo_repo.rs`, `app_repo.rs`.
- `conman-db/src/lib.rs`: export new repos and include index bootstrap.

Tasks:

1. Add domain structs:
   - `Team { id, name, slug, created_at, updated_at }`
   - `App { id, repo_id, key, title, domains, branding?, roles?, created_at, updated_at }`
2. Extend existing app/repo model with `team_id`.
3. Add Mongo collections/indexes:
   - `teams.slug` unique
   - `apps (repo_id, key)` unique
   - `apps.team_id` non-unique index
Done when:

- service boots with new collections/indexes

## Step 2: API endpoint (team + repo + app)

Crates/files:

- `conman-api/src/handlers/teams.rs` (new)
- `conman-api/src/handlers/repos.rs` (repository handlers)
- `conman-api/src/handlers/teams.rs` (team + app handlers)
- router wiring in `conman-api`

Tasks:

1. Add team endpoints:
   - `POST /api/teams`
   - `GET /api/teams`
   - `GET /api/teams/:teamId`
2. Add repo endpoints:
   - `POST /api/teams/:teamId/repos`
   - `GET /api/repos`
   - `GET /api/repos/:repoId`
3. Add app endpoints:
   - `POST /api/repos/:repoId/apps`
   - `GET /api/repos/:repoId/apps`
   - `PATCH /api/repos/:repoId/apps/:appId`

Done when:

- team/repo/app can be created and listed end-to-end

## Step 3: Runtime profile multi-app support

Crates/files:

- `conman-core/src/runtime_profile.rs`
- `conman-db/src/runtime_profile_repo.rs`
- profile handlers under `conman-api`

Tasks:

1. Add `app_endpoints` to runtime profile:
   - shape: `HashMap<String, String>` (`app_key -> base_url`)
2. Validate keys:
   - must match existing repo app keys
3. Update temp env derivation logic so endpoint overrides can be per-app.

Done when:

- environment profile can define endpoints for multiple apps
- temp env creation keeps endpoint map and applies overrides correctly

## Step 4: Changeset/release visibility for apps

Crates/files:

- `conman-core/src/changeset.rs`
- `conman-core/src/release.rs`
- `conman-api` changeset/release handlers

Tasks:

1. Add `impacted_app_keys: Vec<String>` to changeset metadata.
2. Populate it from:
   - changed file paths (first pass)
   - optional semantic diff enrichment (second pass)
3. Propagate impact summary into release detail responses.

Done when:

- review and release screens can show which apps are affected

## Step 5: Auth and membership alignment

Keep it simple for now:

- Membership remains repo-scoped (current behavior).
- `admin` remains the admin capability role.
- Team-level admin model can be deferred.

Tasks:

1. Ensure new team/repo/app endpoints enforce current RBAC correctly.
2. Ensure invite/member flows still work with repo IDs unchanged.

Done when:

- no regression in login/invite/member assignment

## Step 6: Test and docs pass

Tasks:

1. Add/adjust unit and integration tests:
   - team/repo/app repos
   - new handlers
   - runtime profile endpoint map validation
2. Update manual test guide with new setup sequence:
   - create team -> create repo -> create apps
3. Keep OpenAPI docs aligned with the new endpoints.

Done when:

- `cargo test --workspace` passes
- manual API sequence works cleanly with team/repo/app model

## Practical execution checklist

1. Implement Step 1 and Step 2 first (unblocks everything else).
2. Then Step 3 (runtime profiles).
3. Then Step 4 (impacted app metadata).
4. Finish with Step 5 and Step 6 hardening.

## Notes for this repo right now

- Existing code already treats `App` like a repo (`repo_path`,
  `integration_branch`), so this change is mostly additive + naming clarity.

## Automated acceptance criteria

When implementation is complete, these checks must pass.

| ID | Criteria | Automated check |
|---|---|---|
| TRS-AC-01 | Team can be created and queried. | `run_team_repo_app_acceptance.sh` |
| TRS-AC-02 | Repository can be created under a team and queried via `/api/repos/:id`. | `run_team_repo_app_acceptance.sh` |
| TRS-AC-03 | Two apps can be created and listed for one repo. | `run_team_repo_app_acceptance.sh` |
| TRS-AC-04 | Runtime profile stores and returns `app_endpoints`. | `run_team_repo_app_acceptance.sh` |
| TRS-AC-05 | Environment configuration can reference runtime profiles after model change. | `run_team_repo_app_acceptance.sh` |
| TRS-AC-06 | Existing lifecycle smoke remains functional after model change. | `tests/e2e/run_full_staged_smoke.sh` |

### Acceptance command set

1. `CONMAN_BASE_URL=... CONMAN_LOGIN_EMAIL=... CONMAN_LOGIN_PASSWORD=... CONMAN_ACCEPTANCE_REPO_PATH=... ./tests/ops/run_team_repo_app_acceptance.sh --strict`
2. `./tests/e2e/run_full_staged_smoke.sh`

The first command validates the new model contracts. The second command guards
against regressions in the current app/workspace/changeset/release/deploy flow.
