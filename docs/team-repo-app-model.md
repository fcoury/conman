# Team, Repo, and App Model

## 1) Why this change

Conman currently models one "App" as one Git repository.

That assumption breaks for real config repos like `hepquant-config`, where one
team has multiple user-facing apps in the same repo, each with its
own URL/domain and access model.

Examples observed in current config repos:

- Team is top-level (`config/team.yml`).
- Multiple apps are nested under team `app`.
- Config files target surfaces using `allowedApps`.
- Environment settings include multiple base URLs for different surfaces.

## 2) New domain model

Use three layers:

- **Team**: customer/account boundary.
- **Config Repository**: Git boundary (workspace, changeset, release, tag).
- **App**: user-facing app within a repo (domain, branding, roles).

Cardinality:

- Team `1..N` Config Repositories
- Config Repository `1..N` Apps
- App belongs to exactly one Config Repository

## 3) Scope boundaries (important)

Repo-scoped concerns stay repo-scoped:

- workspaces
- changesets
- queue/release assembly
- integration branch + tags
- git history and conflict handling

App-scoped concerns move to app level:

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
- Introduce **Team** and **App** resources.

Canonical API shape:

- `POST /api/teams`
- `GET /api/teams/:teamId`
- `POST /api/teams/:teamId/repos`
- `GET /api/repos/:repoId`
- `POST /api/repos/:repoId/apps`
- `GET /api/repos/:repoId/apps`

API strategy for v1 implementation:

- `/api/repos` is the repository surface.
- Nested apps are managed through `/api/repos/:repoId/apps`.

## 6) Data model changes (minimum)

Add `teams` collection:

- `id`, `name`, `slug`, `created_at`, `updated_at`

Update app/repo document (currently `apps`):

- add `team_id`
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

Phase 1:

- add `team_id` to repository docs
- add `team_memberships` and `app_surfaces`
- expose team and surface APIs

Phase 2:

- add surface-aware runtime profile endpoints
- add impacted-surface summaries to changeset/release APIs

## 9) Decisions captured

- Add top-level **Team** concept.
- Keep one repo as the Git/release unit.
- Support multiple user-facing apps inside one repo.
- Do **not** split releases/workspaces by surface in v1.
- Keep one configurable integration branch per repo (no per-environment Git
  branches).

Implementation plan:
[`docs/team-repo-app-implementation-plan.md`](./team-repo-app-implementation-plan.md)
