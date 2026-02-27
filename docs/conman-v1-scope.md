# Conman V1 Scope Specification

## 1) Objective

Define and deliver v1 of Conman as a Git-backed configuration manager for
DxFlow-style config repositories.

Conman v1 model:

- `Team` -> `Repository` -> `App`
- Repository workflow: `Workspace` -> `Changeset` -> `Release`
- Git is source of truth for files and history
- MongoDB stores team/repository metadata, workflow state, and audit trails
- `gitaly-rs` is the Git backend interface

## 2) Domain Terminology

- **Team**: Top-level customer/account boundary. Owns repositories.
- **Repository**: A managed config repository (stored in `apps` collection in
  v1).
- **App**: A user-facing app within a repository (domain/branding/role hints).
- **Workspace**: A user-owned mutable branch used to edit repository files.
- **Changeset**: A reviewable proposal from one workspace branch against the
  repository integration baseline.
- **Release**: Immutable selected set of approved changesets, published to Git
  and promoted across environments.
- **Environment**: A deploy target stage (configurable per repository).
- **Canonical user-facing environment**: The environment designated as
  production-facing for baseline calculations.
- **Integration branch**: The repository-level branch where published releases
  are applied (default `main`, configurable per repository).
- **Baseline mode**: How new workspace baselines are resolved:
  `integration_head` or `canonical_env_release`.
- **Runtime profile**: The runtime blueprint for an environment or temp env.
  Contains URL, typed env vars, encrypted secrets, database settings,
  migration settings, data strategy, and lifecycle controls.
- **Changeset profile override**: Runtime-profile deltas attached to a
  changeset. They are auto-included on submit and travel with release/promotion.
- **Queue states**:
  - `queued`: Approved and waiting for release selection.
  - `conflicted`: Cannot be cleanly composed/revalidated against latest
    integration context.
  - `needs_revalidation`: Conflict-free but failed required validation.
- **Drift**: Runtime mismatch between expected profile state and target
  environment state (env vars, secrets, URL, DB settings, or migration set).
- **Temp environment**: On-demand ephemeral validation environment tied to a
  workspace or changeset, with idle TTL + grace lifecycle.
- **Release assemble**: Compose selected queued changesets in chosen order into
  a candidate release artifact.
- **Release publish**: Move the integration branch/tag to the assembled
  artifact, making it deployable.
- **Validation scopes**:
  - submit: temp profile only
  - release publish: environment profiles only
  - deploy: target environment profile only
- **Migration execution metadata**: Conman-tracked records of migration runs
  used by drift checks and deployment gating.
- **Emergency profile edit**: Direct `admin` runtime-profile update outside
  normal changeset flow; fully audited and still subject to drift blocking.

## 3) Core V1 Decisions

### 3.1 Baseline and branching

- Integration branch is configurable per repository (`integration_branch`), defaulting
  to `main`.
- New repositories default baseline mode: `latest deployed release of canonical
  user-facing environment`.
- Fallback baseline when no release exists: integration branch HEAD.
- Baseline mode is configurable per repository and editable by app admins.

### 3.2 Workspace model

- Default v1 behavior: one long-lived workspace branch per user per repository.
- Branch naming convention: `ws/<user>/<app>`.
- Metadata supports optional workspace title now, for future multi-workspace UI.
- Extra workspace creation API is supported in v1 backend; UI can defer it.

### 3.3 Changeset model

- One open changeset per workspace branch in v1.
- Changesets are incremental until submitted.
- On submit, freeze `head_sha` for review.
- If updated before approval, keep same changeset and create new revision
  internally; reset approvals to zero.
- If already approved and user wants further edits, create a new changeset.

### 3.4 Review and queue model

- Queue-first model:
  - Approved changesets are queued.
  - Releases are manually assembled from a selected subset of queued changesets.
  - Non-selected queued changesets remain queued.
- Queued changesets auto-revalidate after each published release:
  - conflict check
  - full `msuite` test run
- Revalidation failures:
  - conflict -> `conflicted`
  - test fail -> `needs_revalidation`
  - both can be moved back to draft by author or config manager.

### 3.5 Release and deployment model

- Release tag format: `rYYYY.MM.DD.N`.
- Release creation is manual (not automatic on merge).
- Release is immutable once created.
- Promotion moves the exact same release artifact across environments.
- Skip-stage flow and concurrent multi-environment deployment are exceptional
  flows requiring two approvals:
  - approvals must be from distinct users
  - at least one approver must be `reviewer`, `config_manager`, or `admin`
- Normal promotion requires no additional deploy-time approval beyond
  changeset/release approvals.
- Deployments may run concurrently across environments, with lock scope of one
  active deployment per environment.

### 3.6 Hotpatch and rollback

- Hotpatch flow in v1:
  - create workspace from current canonical user-facing release tag
  - go through changeset/review/queue
  - can fast-track to release with two approvals
- Rollback support in v1:
  - default: `revert on integration_branch + new release`
  - also support: redeploy prior release tag to an environment

### 3.7 Edit and commit strategy

- App setting `commit_mode` supports:
  - `submit_commit` (default): autosave to workspace working state; commit on
    submit
  - `manual_checkpoint`: user checkpoints become commits
- App default is configurable and can be overridden per user.
- `submit_commit` keeps detailed edit history in metadata/audit while keeping
  cleaner Git history.

### 3.8 File scope and guardrails

- Editable scope: full repo by default.
- Blocklist defaults (configurable per repository):
  - `.git/**`
  - `.gitignore`
  - `.github/**`
- File size limit default: 5 MB per file, configurable per repository.
- No Git LFS support in v1.

### 3.9 Temporary environments

- Types supported: workspace temp env and changeset temp env.
- Provisioning: on-demand only.
- TTL: 24h idle.
- Idle means no API activity, no test runs, and no deployment events.
- Manual TTL extension is allowed before expiry.
- On expiry:
  - soft-delete for 1h grace period
  - notify before expiry and at expiry
  - allow undo/restore during grace window

### 3.10 Auth, membership, and notifications

- Auth v1: local email/password.
- Signup: open to anyone via `POST /api/auth/signup`.
- Signup bootstrap: automatically creates first team and first repository, and
  assigns creator role `owner`.
- Team invites are created at team scope by `admin`/`owner`.
- Invite expiration: 7 days.
- Password policy: minimum length + reset via email token.
- User can belong to multiple teams and repositories, with roles scoped per
  repository.
- Email notifications are per-user single toggle in v1.

### 3.11 API conventions

- Base path starts at `/api` (no version prefix in v1).
- Pagination uses `page` + `limit`.
- File path is sent as query/body field (not `/:path`).
- Generated request IDs use UUIDv7.
- Long-running operations (`msuite`, deployments, compose) run async with
  status and logs.
- Conman uses an internal Git adapter service boundary; adapter
  implementation targets `gitaly-rs` in v1.

### 3.12 Runtime profiles

- Runtime configuration is modeled as a first-class `RuntimeProfile` and is
  versioned with releases.
- Runtime variable precedence is:
  `app defaults < environment profile < temp profile overrides`.
- Runtime profile overrides may be attached to changesets and travel with
  release/promotion.
- Runtime profile overrides are auto-included on changeset submit and surfaced
  in the submit summary payload.
- If two queued changesets override the same env var key, the later changeset
  becomes `conflicted`.
- If both overrides resolve to the same typed value for the same key/target,
  they are treated as non-conflicting.
- Canonical user-facing environment profile changes require configurable policy:
  `same_as_changeset` or `stricter_two_approvals` (default).
- Deployment is blocked on runtime profile drift until revalidation passes.
- `admin` may apply direct emergency edits to persistent runtime profiles;
  these edits are fully audited and still trigger drift/revalidation gating.
- v1 DB engine scope is MongoDB only.
- Temp DB provisioning defaults to snapshot clone with dump/restore fallback.
- Secrets are encrypted at rest in Conman (no external secret manager required
  for v1).
- Secret plaintext reveal is `admin`-only in v1; other roles get masked
  preview only.
- Secret reveal does not require forced re-auth or reason entry in v1.
- Secret masking policy:
  - length <= 8: reveal only last 4 chars
  - length > 8: reveal first 4 and last 4 chars
- Env vars are typed in v1 (`string | number | boolean | json`).
- Runtime profile schema is strict typed in v1 (no arbitrary top-level custom
  fields).
- Applied migration metadata is stored in Conman and used by drift checks.
- Temp environment URLs are auto-generated and human-readable.
- Validation defaults:
  - submit: temp profile only
  - release publish: environment profiles only
  - deploy: target environment profile only

## 4) Roles and RBAC

Roles:

- `member`
- `reviewer`
- `config_manager`
- `admin`
- `owner`

Notes:

- `owner` inherits `admin` capabilities.
- `admin` inherits `config_manager` capabilities.
- `reviewer`, `config_manager`, `admin`, and `owner` can approve changesets.
- `config_manager` is responsible for release assembly and governance.

### 4.1 Permission matrix (v1)

| Capability | member | reviewer | config_manager | admin | owner |
|---|---:|---:|---:|---:|---:|
| Read app/repo metadata | Y | Y | Y | Y | Y |
| Create/edit own workspace | Y | Y | Y | Y | Y |
| Create/modify own changeset | Y | Y | Y | Y | Y |
| Submit changeset | Y | Y | Y | Y | Y |
| Comment in review | Y | Y | Y | Y | Y |
| Approve/request changes/reject | N | Y | Y | Y | Y |
| Move `conflicted`/`needs_revalidation` to draft | Own | Own | Any | Any | Any |
| Assemble release from queue | N | N | Y | Y | Y |
| Publish release | N | N | Y | Y | Y |
| Deploy/promote release | N | N | Y | Y | Y |
| Skip stage / concurrent deploy approval | N | Y | Y | Y | Y |
| Invite users | N | N | N | Y | Y |
| Manage app settings/roles/envs | N | N | N | Y | Y |
| Manage persistent runtime profiles directly | N | N | N | Y | Y |
| Reveal secret plaintext | N | N | N | Y | Y |

`Own` means for items authored by that user.

## 5) State Machines

### 5.1 Changeset state machine

```text
draft
  -> submitted
  -> in_review
      -> approved
      -> changes_requested -> draft
      -> rejected
approved
  -> queued
queued
  -> released
  -> conflicted -> draft
  -> needs_revalidation -> draft
released
  -> terminal
rejected
  -> terminal (or clone/new changeset)
```

Rules:

- New commits while `submitted`/`in_review` keep same changeset revision chain,
  reset approvals, and continue review flow.
- One open changeset per workspace branch.

### 5.2 Release state machine

```text
draft_release
  -> assembling
  -> validated
  -> published
  -> deployed_partial
  -> deployed_full
  -> rolled_back (optional)
```

### 5.3 Deployment state machine

```text
pending -> running -> succeeded
                   -> failed
                   -> canceled
```

## 6) Git Mapping and Flow

## 6.1 Mapping

- Repository (`App` record): Git repository.
- Workspace: branch from repository baseline.
- Changeset: metadata object comparing workspace `head_sha` vs baseline.
- Release: immutable Git tag (`rYYYY.MM.DD.N`) representing selected queued
  changesets composed onto the repository integration branch.

## 6.2 Queue-first release composition

1. Select queued changesets subset.
2. Config manager orders selected items manually.
3. Compose candidate in selected order.
4. Resolve failures:
   - conflicts -> mark specific changesets `conflicted`
   - test failures -> mark `needs_revalidation`
5. Successful compose publishes release tag and updates integration branch
   accordingly.
6. Run post-publish revalidation for remaining queued items.

## 6.3 "Up to date with integration branch" gate

- Merge/release requires changeset branch to be up to date with the repository
  integration branch.
- Either merge-based sync or rebase-based sync is accepted.
- Conflict resolution in v1 is text-based UI flow.

## 7) Data Model (MongoDB)

Git remains file truth. MongoDB tracks workflow state and auditability.

### 7.1 Collections

- `teams`
- `apps`
- `app_surfaces`
- `app_memberships`
- `workspaces`
- `changesets`
- `changeset_revisions`
- `changeset_reviews`
- `changeset_comments`
- `changeset_comment_revisions`
- `changeset_profile_overrides`
- `release_batches`
- `release_changesets`
- `environments`
- `runtime_profiles`
- `runtime_profile_revisions`
- `migration_executions`
- `deployments`
- `temp_environments`
- `jobs`
- `audit_events`
- `notification_preferences`
- `invites`

### 7.2 Important fields

`teams`

- `id`, `name`, `slug`
- `created_at`, `updated_at`

`apps`

- `id`, `team_id`, `name`, `repo_path`
- `integration_branch` (default `main`)
- `baseline_mode` (`integration_head` | `canonical_env_release`)
- `canonical_env_id`
- `file_size_limit_bytes` (default 5 MB)
- `blocked_paths[]`
- `commit_mode_default` (`submit_commit` | `manual_checkpoint`)
- `runtime_profile_approval_policy` (`same_as_changeset` |
  `stricter_two_approvals`)
- `temp_url_domain`
- `validation_gates` (submit/release/deploy profile scope + command overrides)

`app_surfaces`

- `id`, `repo_id`, `key`, `title`
- `domains[]`, `branding?`, `roles[]`
- `created_at`, `updated_at`

`environments`

- `id`, `app_id`, `name`, `position`, `is_canonical`
- `runtime_profile_id`
- `created_at`, `updated_at`

`runtime_profiles`

- `id`, `app_id`, `name`, `kind`
- `base_url`, `surface_endpoints`, `env_vars_typed`, `secrets_encrypted`
- `database` (`engine=mongodb`, `connection_ref`, `provisioning_mode`,
  `base_profile_id?`)
- `migrations` (`repo_paths[]`, `command_ref`, `applied_state_ref`)
- `data_strategy` (`seed_mode`, `seed_source_ref?`)
- `lifecycle` (`ttl_idle_hours?`, `grace_hours?`, `auto_cleanup`)
- `created_by`, `created_at`, `updated_at`

`runtime_profile_revisions`

- `id`, `runtime_profile_id`, `revision_number`
- `snapshot`, `created_by`, `created_at`

`changeset_profile_overrides`

- `id`, `changeset_id`, `app_id`
- `target_environment_id?`, `target_profile_id?`
- `env_var_overrides_typed`, `secret_overrides_encrypted`
- `database_overrides`, `data_overrides`
- `created_by`, `updated_by`, `created_at`, `updated_at`

`migration_executions`

- `id`, `app_id`, `environment_id`, `release_id`
- `runtime_profile_revision_id`, `status`, `started_at`, `finished_at`
- `applied_migrations[]`, `logs_ref`, `triggered_by`

`workspaces`

- `id`, `app_id`, `owner_user_id`, `branch_name`, `title?`, `is_default`
- `base_ref_type`, `base_ref_value`, `head_sha`

`changesets`

- `id`, `app_id`, `workspace_id`, `author_user_id`
- `title`, `description`, `state`
- `base_sha`, `head_sha`, `current_revision`
- `approval_count`, `required_approval_count=1`
- `last_revalidation_status`, `last_revalidation_job_id`

`changeset_revisions`

- `id`, `changeset_id`, `revision_number`, `head_sha`, `created_by`, `created_at`

`release_batches`

- `id`, `app_id`, `tag`, `state`
- `ordered_changeset_ids[]`
- `published_sha`, `published_at`, `published_by`

`deployments`

- `id`, `app_id`, `environment_id`, `release_id`, `state`
- `skip_stage`, `approval_user_ids[]`, `logs_ref`

`temp_environments`

- `id`, `app_id`, `kind` (`workspace` | `changeset`)
- `workspace_id?`, `changeset_id?`
- `db_name`, `state`, `last_activity_at`, `expires_at`, `grace_until`
- `base_runtime_profile_id`, `runtime_profile_id`
- `base_url_generated`

`audit_events`

- `id`, `occurred_at`, `actor_user_id`, `app_id`
- `entity_type`, `entity_id`, `action`
- `before`, `after`, `git_sha?`, `context` (ip, user_agent, request_id)

## 8) API Contract (v1)

Base path: `/api`

## 8.1 Teams and repositories

- `GET /api/teams?page=&limit=`
- `POST /api/teams`
- `GET /api/teams/:teamId`
- `POST /api/teams/:teamId/repos`
- `POST /api/teams/:teamId/invites`
- `GET /api/repos?page=&limit=`
- `GET /api/repos/:appId`
- `PATCH /api/repos/:appId/settings`
- `GET /api/repos/:appId/members?page=&limit=`
- `POST /api/repos/:appId/members`
- `GET /api/repos/:appId/apps`
- `POST /api/repos/:appId/apps`
- `PATCH /api/repos/:appId/apps/:surfaceId`

## 8.2 Workspaces

- `GET /api/repos/:appId/workspaces?page=&limit=`
- `POST /api/repos/:appId/workspaces`
- `GET /api/repos/:appId/workspaces/:workspaceId`
- `PATCH /api/repos/:appId/workspaces/:workspaceId`
- `POST /api/repos/:appId/workspaces/:workspaceId/reset`
- `POST /api/repos/:appId/workspaces/:workspaceId/sync-integration`

## 8.3 Workspace files

- `GET /api/repos/:appId/workspaces/:workspaceId/files?path=`
- `PUT /api/repos/:appId/workspaces/:workspaceId/files` (body: `path`, `content`)
- `DELETE /api/repos/:appId/workspaces/:workspaceId/files` (body: `path`)
- `POST /api/repos/:appId/workspaces/:workspaceId/checkpoints`

## 8.4 Changesets

- `GET /api/repos/:appId/changesets?page=&limit=&state=`
- `POST /api/repos/:appId/changesets` (from workspace)
- `GET /api/repos/:appId/changesets/:changesetId`
- `PATCH /api/repos/:appId/changesets/:changesetId`
- `POST /api/repos/:appId/changesets/:changesetId/submit`
- `POST /api/repos/:appId/changesets/:changesetId/resubmit`
- `POST /api/repos/:appId/changesets/:changesetId/review`
- `POST /api/repos/:appId/changesets/:changesetId/queue`
- `POST /api/repos/:appId/changesets/:changesetId/move-to-draft`
- `GET /api/repos/:appId/changesets/:changesetId/profile-overrides`
- `PUT /api/repos/:appId/changesets/:changesetId/profile-overrides`

`POST .../submit` responses include an `included_profile_overrides` summary.

## 8.5 Diffs, comments, and AI

- `GET /api/repos/:appId/changesets/:changesetId/diff?mode=raw|semantic`
- `GET /api/repos/:appId/changesets/:changesetId/comments?page=&limit=`
- `POST /api/repos/:appId/changesets/:changesetId/comments`
- `PATCH /api/repos/:appId/changesets/:changesetId/comments/:commentId` (stores
  comment revision history)
- `POST /api/repos/:appId/changesets/:changesetId/analyze`
- `POST /api/repos/:appId/changesets/:changesetId/chat`

Semantic diff API contract (v1 standard):

```ts
type SemanticConfigType =
  | 'entity'
  | 'page'
  | 'queue'
  | 'provider'
  | 'workflow'
  | 'team'
  | 'menu'
  | 'asset'
  | 'script';

type SemanticOperation = 'added' | 'modified' | 'removed' | 'moved';

interface SemanticChange {
  id: string;
  configType: SemanticConfigType;
  operation: SemanticOperation;
  target: string;
  description: string;
  filePath: string;
  lineStart?: number;
  lineEnd?: number;
  details?: Record<string, unknown>;
}

interface SemanticDiffResponse {
  baseSha: string;
  headSha: string;
  summary: {
    totalChanges: number;
    byConfigType: Record<SemanticConfigType, number>;
  };
  changes: SemanticChange[];
}
```

## 8.6 Releases

- `GET /api/repos/:appId/releases?page=&limit=&state=`
- `POST /api/repos/:appId/releases` (create draft release)
- `POST /api/repos/:appId/releases/:releaseId/changesets` (add/remove subset)
- `POST /api/repos/:appId/releases/:releaseId/reorder`
- `POST /api/repos/:appId/releases/:releaseId/assemble`
- `POST /api/repos/:appId/releases/:releaseId/publish`
- `GET /api/repos/:appId/releases/:releaseId`

## 8.7 Environments and deployments

- `GET /api/repos/:appId/environments`
- `PATCH /api/repos/:appId/environments`
- `POST /api/repos/:appId/environments/:envId/deploy`
- `POST /api/repos/:appId/environments/:envId/promote`
- `POST /api/repos/:appId/environments/:envId/rollback`
- `POST /api/repos/:appId/environments/:envId/create-drift-fix-changeset`
- `GET /api/repos/:appId/deployments?page=&limit=`

## 8.8 Runtime profiles

- `GET /api/repos/:appId/runtime-profiles?page=&limit=`
- `POST /api/repos/:appId/runtime-profiles`
- `GET /api/repos/:appId/runtime-profiles/:profileId`
- `PATCH /api/repos/:appId/runtime-profiles/:profileId`
- `GET /api/repos/:appId/runtime-profiles/:profileId/revisions?page=&limit=`
- `POST /api/repos/:appId/runtime-profiles/:profileId/revert`
- `POST /api/repos/:appId/runtime-profiles/:profileId/rotate-key` (manual)
- `POST /api/repos/:appId/runtime-profiles/:profileId/secrets/:key/reveal`
  (`admin` only; audited)
- `surface_endpoints` keys must reference existing
  `/api/repos/:appId/apps` keys

`PATCH .../runtime-profiles/:profileId` allows direct emergency edits by
`admin`; resulting drift still blocks deployment until revalidation.

## 8.9 Temp environments and jobs

- `POST /api/repos/:appId/temp-envs` (workspace or changeset)
- `GET /api/repos/:appId/temp-envs?page=&limit=`
- `POST /api/repos/:appId/temp-envs/:tempEnvId/extend`
- `POST /api/repos/:appId/temp-envs/:tempEnvId/undo-expire`
- `DELETE /api/repos/:appId/temp-envs/:tempEnvId`
- `GET /api/repos/:appId/jobs/:jobId`
- `GET /api/repos/:appId/jobs?page=&limit=&type=&state=`

## 8.10 Notifications and auth

- `GET /api/me/notification-preferences`
- `PATCH /api/me/notification-preferences`
- `POST /api/auth/signup`
- `POST /api/auth/login`
- `POST /api/auth/logout`
- `POST /api/auth/forgot-password`
- `POST /api/auth/reset-password`
- `POST /api/auth/accept-invite`

## 9) Asynchronous Jobs

Job types:

- `msuite_submit`
- `msuite_merge`
- `msuite_deploy`
- `revalidate_queued_changeset`
- `release_assemble`
- `deploy_release`
- `runtime_profile_drift_check`
- `temp_env_provision`
- `temp_env_expire`

Job states:

- `queued`
- `running`
- `succeeded`
- `failed`
- `canceled`

Each job stores structured logs and terminal result payload.

## 10) Notifications (Email)

v1 event set:

- changeset submitted
- review requested
- changeset approved
- changes requested
- changeset rejected
- changeset queued
- release created
- release published
- deployment started
- deployment succeeded
- deployment failed
- deployment blocked by drift
- runtime profile changed
- temp environment expiry warning
- temp environment expired

Per-user single on/off toggle in v1.

## 11) Audit Requirements

Audit everything immutable and append-only:

- workspace creation/reset/sync
- file edits/deletes/checkpoints
- changeset submit/resubmit/review/queue/state changes
- changeset profile override changes
- comment creation and comment edits
- runtime profile create/update/revert/rotation/direct-edit
- runtime secret reveal events
- release compose/reorder/publish
- deployment/promotion/rollback
- temp env create/extend/expire/undo
- membership/invite/role changes
- settings changes

Retention: keep forever in v1.

## 12) UI Component Scope (v1)

- Dashboard: team/repository cards and role-aware quick actions.
- App shell: sidebar, breadcrumbs, app switcher.
- Workspace page:
  - repo tree with markers
  - editor/diff tabs
  - AI chat panel
  - checkpoint/submit controls
- Changeset list and detail/review UI.
- Queue view for config managers (prioritize and reorder).
- Runtime profile editor:
  - environment profile assignment
  - typed env vars and secret values (encrypted storage)
  - secret masked preview for non-admin roles, admin reveal action
  - DB provisioning mode and base profile selection
  - migration command/path config and applied migration history
  - revision history and manual key rotation actions
- Release builder page:
  - select queued changesets
  - reorder
  - assemble preview
  - publish
- Environment pipeline page:
  - deploy/promote/rollback
  - skip-stage approval controls
- Temp environments page:
  - provision
  - TTL and expiry UX
- Settings page:
  - baseline mode
  - commit mode defaults
  - blocked paths/file size limit
  - environment stages/canonical env
  - email notification defaults
- Auth and invite management screens.

## 13) Explicit V1 Constraints

- No Git LFS support.
- No semantic merge assistant; text conflict resolution only.
- Local auth only (no SSO).
- API path prefix is `/api`.

## 14) Delivery Plan

### Phase A: Foundations

- Auth/invites, team/repository setup, memberships, RBAC.
- Workspace CRUD and file editing with guardrails.
- Changeset creation/submit/review and approval reset behavior.
- Async `msuite` at submit.
- Runtime profile schema, storage, and environment linkage.

### Phase B: Queue and releases

- Queue-first workflow and revalidation.
- Release draft/assemble/reorder/publish.
- Tagging (`rYYYY.MM.DD.N`) and immutable release records.
- Changeset profile override flow and override conflict detection.

### Phase C: Environments and temp envs

- Environment pipeline and deployment orchestration.
- Skip-stage 2-approval flow.
- Rollback modes (revert+new release and redeploy prior tag).
- Temp env lifecycle with TTL, grace period, and undo.
- Runtime profile drift checks and deploy-block rules.

### Phase D: Hardening

- Auditing completeness verification.
- Email notifications and user toggle.
- Operational limits, retries, observability, and backfill tooling.
- Secrets encryption hardening and manual key-rotation procedures.

## 15) Open Items to Confirm During Implementation

- Final email provider and templates.
- Locking mechanism details for per-environment concurrent deployment safety.
- Runtime profile URL generator wordlist/domain strategy.
