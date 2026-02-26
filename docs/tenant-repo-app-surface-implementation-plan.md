# Tenant/Repo/App Surface Implementation Plan

## Goal

Implement the new domain direction:

- `Tenant -> Config Repository -> App Surface`
- Keep Git lifecycle repo-scoped (`workspace -> changeset -> release -> deploy`)
- Add multi-surface support for URLs/runtime profile context

This plan is optimized for current reality: pre-production and one active user.

## What changes

1. Current `App` becomes a repository concept in practice.
2. Add first-class `Tenant`.
3. Add first-class `App Surface` under each repository.
4. Keep existing `/api/apps` working (backward-compatible) while adding clearer
   `/api/repos` and `/api/tenants` APIs.
5. Runtime profiles gain surface endpoint mapping.

## What stays the same

- Queue-first changesets/releases model.
- One integration branch per repository.
- Environment pipeline and deploy flow.
- gitaly-rs integration boundary.

## Implementation order (critical path)

## Step 1: Domain + storage model

Crates/files:

- `conman-core`: add `tenant.rs`, `app_surface.rs`; keep `app.rs` as repo model.
- `conman-db`: add `tenant_repo.rs`, `app_surface_repo.rs`.
- `conman-db/src/lib.rs`: export new repos and include index bootstrap.

Tasks:

1. Add domain structs:
   - `Tenant { id, name, slug, created_at, updated_at }`
   - `AppSurface { id, repo_id, key, title, domains, branding?, roles?, created_at, updated_at }`
2. Extend existing app/repo model with `tenant_id`.
3. Add Mongo collections/indexes:
   - `tenants.slug` unique
   - `app_surfaces (repo_id, key)` unique
   - `apps.tenant_id` non-unique index
4. One-time local backfill:
   - create a default tenant
   - assign all existing repos to it

Done when:

- service boots with new collections/indexes
- existing app/repo flows still work

## Step 2: API surface (tenant + repo + app-surface)

Crates/files:

- `conman-api/src/handlers/tenants.rs` (new)
- `conman-api/src/handlers/repos.rs` (new alias/clear naming over current app handlers)
- `conman-api/src/handlers/app_surfaces.rs` (new)
- router wiring in `conman-api`

Tasks:

1. Add tenant endpoints:
   - `POST /api/tenants`
   - `GET /api/tenants`
   - `GET /api/tenants/:tenantId`
2. Add repo endpoints:
   - `POST /api/tenants/:tenantId/repos`
   - `GET /api/repos`
   - `GET /api/repos/:repoId`
3. Keep `/api/apps` endpoints functional as compatibility alias.
4. Add app-surface endpoints:
   - `POST /api/repos/:repoId/surfaces`
   - `GET /api/repos/:repoId/surfaces`
   - `PATCH /api/repos/:repoId/surfaces/:surfaceId`

Done when:

- tenant/repo/surface can be created and listed end-to-end
- `/api/apps` existing calls still pass manual smoke tests

## Step 3: Runtime profile multi-surface support

Crates/files:

- `conman-core/src/runtime_profile.rs`
- `conman-db/src/runtime_profile_repo.rs`
- profile handlers under `conman-api`

Tasks:

1. Add `surface_endpoints` to runtime profile:
   - shape: `HashMap<String, String>` (`surface_key -> base_url`)
2. Validate keys:
   - must match existing repo surface keys
3. Keep existing single-url fields as compatibility path (if present).
4. Update temp env derivation logic so endpoint overrides can be per-surface.

Done when:

- environment profile can define endpoints for multiple app surfaces
- temp env creation keeps endpoint map and applies overrides correctly

## Step 4: Changeset/release visibility for surfaces

Crates/files:

- `conman-core/src/changeset.rs`
- `conman-core/src/release.rs`
- `conman-api` changeset/release handlers

Tasks:

1. Add `impacted_surface_keys: Vec<String>` to changeset metadata.
2. Populate it from:
   - changed file paths (first pass)
   - optional semantic diff enrichment (second pass)
3. Propagate impact summary into release detail responses.

Done when:

- review and release screens can show which surfaces are affected

## Step 5: Auth and membership alignment

Keep it simple for now:

- Membership remains repo-scoped (current behavior).
- `app_admin` remains the admin capability role.
- Tenant-level admin model can be deferred.

Tasks:

1. Ensure new tenant/repo/surface endpoints enforce current RBAC correctly.
2. Ensure invite/member flows still work with repo IDs unchanged.

Done when:

- no regression in login/invite/member assignment

## Step 6: Test and docs pass

Tasks:

1. Add/adjust unit and integration tests:
   - tenant/surface repos
   - new handlers
   - runtime profile endpoint map validation
2. Update manual test guide with new setup sequence:
   - create tenant -> create repo -> create surfaces
3. Keep OpenAPI docs aligned with the new endpoints.

Done when:

- `cargo test --workspace` passes
- manual API sequence works cleanly with tenant/repo/surface model

## Practical execution checklist

1. Implement Step 1 and Step 2 first (unblocks everything else).
2. Then Step 3 (runtime profiles).
3. Then Step 4 (impacted surface metadata).
4. Finish with Step 5 and Step 6 hardening.

## Notes for this repo right now

- Existing code already treats `App` like a repo (`repo_path`,
  `integration_branch`), so this change is mostly additive + naming clarity.
- Because this is pre-production and single-user, we can keep the backfill
  lightweight and local without rollout orchestration.
