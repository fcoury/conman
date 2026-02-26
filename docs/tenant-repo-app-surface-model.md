# Tenant, Repo, and App Surface Model

## 1) Why this change

Conman currently models one "App" as one Git repository.

That assumption breaks for real config repos like `hepquant-config`, where one
tenant has multiple user-facing app surfaces in the same repo, each with its
own URL/domain and access model.

Examples observed in current config repos:

- Tenant is top-level (`config/tenant.yml`).
- Multiple app surfaces are nested under tenant `app`.
- Config files target surfaces using `allowedApps`.
- Environment settings include multiple base URLs for different surfaces.

## 2) New domain model

Use three layers:

- **Tenant**: customer/account boundary.
- **Config Repository**: Git boundary (workspace, changeset, release, tag).
- **App Surface**: user-facing app within a repo (domain, branding, roles).

Cardinality:

- Tenant `1..N` Config Repositories
- Config Repository `1..N` App Surfaces
- App Surface belongs to exactly one Config Repository

## 3) Scope boundaries (important)

Repo-scoped concerns stay repo-scoped:

- workspaces
- changesets
- queue/release assembly
- integration branch + tags
- git history and conflict handling

Surface-scoped concerns move to app-surface level:

- domains / URLs
- branding metadata
- role visibility where needed
- "impacted surfaces" metadata for review/release visibility

This preserves atomic Git/release behavior while supporting multi-app repos.

## 4) Runtime profile impact

Current runtime profile design assumes a single URL. We need multi-surface
routing in each profile.

Add per-profile surface routing config, for example:

- `surface_endpoints`: map of `surface_key -> base_url`

Keep existing precedence model:

- `app defaults < environment profile < temp overrides`

Apply precedence per surface endpoint and per variable/secret.

## 5) API and naming changes

Current Conman "App" is effectively a repo object (`repo_path`,
`integration_branch`). To reduce confusion:

- Domain rename in docs/spec: **App -> Config Repository**.
- Introduce **Tenant** and **App Surface** resources.

Suggested API shape (incremental):

- `POST /api/tenants`
- `GET /api/tenants/:tenantId`
- `POST /api/tenants/:tenantId/repos`
- `GET /api/repos/:repoId`
- `POST /api/repos/:repoId/surfaces`
- `GET /api/repos/:repoId/surfaces`

Compatibility strategy for v1 implementation:

- Keep existing `/api/repos` endpoints as repo endpoints for now.
- Add a documented alias plan in v2 (`/api/repos` primary, `/api/repos`
  compatibility layer/deprecated alias).

## 6) Data model changes (minimum)

Add `tenants` collection:

- `id`, `name`, `slug`, `created_at`, `updated_at`

Update app/repo document (currently `apps`):

- add `tenant_id`
- keep `repo_path`, `integration_branch`, baseline settings

Add `app_surfaces` collection:

- `id`, `repo_id`, `key`, `title`
- `domains[]`
- `branding` (optional)
- `roles[]` (optional surface-scoped role hints)
- `created_at`, `updated_at`

Update environments/runtime profiles:

- environment remains repo-scoped
- runtime profile adds `surface_endpoints`

## 7) Release/deploy semantics

No change to queue-first release model:

- releases are still repo artifacts
- deployment remains per environment
- a deployment updates all included surface config in that repo commit

Add visibility metadata:

- changeset/release should expose impacted surfaces (derived from changed paths
  and/or config semantics)

## 8) Implementation phases

Phase 1 (non-breaking):

- add `tenant_id` to existing app/repo docs
- backfill one default tenant for existing records
- add `app_surfaces`
- keep all current `/api/repos` behavior

Phase 2:

- expose tenant and surface APIs
- add surface-aware runtime profile endpoints
- add impacted-surface summaries to changeset/release APIs

Phase 3:

- promote `/api/repos` naming, keep `/api/repos` alias for compatibility window

## 9) Decisions captured

- Add top-level **Tenant** concept.
- Keep one repo as the Git/release unit.
- Support multiple user-facing app surfaces inside one repo.
- Do **not** split releases/workspaces by surface in v1.
- Keep one configurable integration branch per repo (no per-environment Git
  branches).

Implementation plan:
[`docs/tenant-repo-app-surface-implementation-plan.md`](./tenant-repo-app-surface-implementation-plan.md)
