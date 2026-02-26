# Conman V1 Scope Specification

## 1) Objective

Define and deliver v1 of Conman as a Git-backed configuration manager for
DxFlow-style config repositories.

Conman v1 model:

- `App` -> `Workspace` -> `Changeset` -> `Release`
- Git is source of truth for files and history
- MongoDB stores app metadata, workflow state, and audit trails
- `gitaly-rs` is the Git backend interface

## 2) Domain Terminology

- **App**: A managed config repository (1 app = 1 Git repo).
- **Workspace**: A user-owned mutable branch used to edit app files.
- **Changeset**: A reviewable proposal from one workspace branch against the
  app integration baseline.
- **Release**: Immutable selected set of approved changesets, published to Git
  and promoted across environments.
- **Environment**: A deploy target stage (configurable per app).
- **Canonical user-facing environment**: The environment designated as
  production-facing for baseline calculations.

## 3) Core V1 Decisions

### 3.1 Baseline and branching

- Integration branch is always `main`.
- New apps default baseline mode: `latest deployed release of canonical
  user-facing environment`.
- Fallback baseline when no release exists: `main` HEAD.
- Baseline mode is configurable per app and editable by app admins.

### 3.2 Workspace model

- Default v1 behavior: one long-lived workspace branch per user per app.
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
  - at least one approver must be `reviewer`, `config_manager`, or `app_admin`
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
  - default: `revert on main + new release`
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
- Blocklist defaults (configurable per app):
  - `.git/**`
  - `.gitignore`
  - `.github/**`
- File size limit default: 5 MB per file, configurable per app.
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
- Provisioning: invite-only by app admins.
- Invite expiration: 7 days.
- Password policy: minimum length + reset via email token.
- User can belong to multiple apps with different roles per app.
- Email notifications are per-user single toggle in v1.

### 3.11 API conventions

- Base path starts at `/api` (no version prefix in v1).
- Pagination uses `page` + `limit`.
- File path is sent as query/body field (not `/:path`).
- Long-running operations (`msuite`, deployments, compose) run async with
  status and logs.
- Conman uses an internal Git adapter service boundary; adapter
  implementation targets `gitaly-rs` in v1.

## 4) Roles and RBAC

Roles:

- `user`
- `reviewer`
- `config_manager`
- `app_admin`

Notes:

- `app_admin` inherits `config_manager` capabilities.
- `reviewer`, `config_manager`, and `app_admin` can approve changesets.
- Config manager is responsible for release assembly and governance.

### 4.1 Permission matrix (v1)

| Capability | user | reviewer | config_manager | app_admin |
|---|---:|---:|---:|---:|
| Read app/repo metadata | Y | Y | Y | Y |
| Create/edit own workspace | Y | Y | Y | Y |
| Create/modify own changeset | Y | Y | Y | Y |
| Submit changeset | Y | Y | Y | Y |
| Comment in review | Y | Y | Y | Y |
| Approve/request changes/reject | N | Y | Y | Y |
| Move `conflicted`/`needs_revalidation` to draft | Own | Own | Any | Any |
| Assemble release from queue | N | N | Y | Y |
| Publish release | N | N | Y | Y |
| Deploy/promote release | N | N | Y | Y |
| Skip stage / concurrent deploy approval | N | Y | Y | Y |
| Invite users | N | N | N | Y |
| Manage app settings/roles/envs | N | N | N | Y |

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

- App: Git repository.
- Workspace: branch from app baseline.
- Changeset: metadata object comparing workspace `head_sha` vs baseline.
- Release: immutable Git tag (`rYYYY.MM.DD.N`) representing selected queued
  changesets composed onto `main`.

## 6.2 Queue-first release composition

1. Select queued changesets subset.
2. Config manager orders selected items manually.
3. Compose candidate in selected order.
4. Resolve failures:
   - conflicts -> mark specific changesets `conflicted`
   - test failures -> mark `needs_revalidation`
5. Successful compose publishes release tag and updates `main` accordingly.
6. Run post-publish revalidation for remaining queued items.

## 6.3 "Up to date with main" gate

- Merge/release requires changeset branch to be up to date with `main`.
- Either merge-based sync or rebase-based sync is accepted.
- Conflict resolution in v1 is text-based UI flow.

## 7) Data Model (MongoDB)

Git remains file truth. MongoDB tracks workflow state and auditability.

### 7.1 Collections

- `apps`
- `app_memberships`
- `workspaces`
- `changesets`
- `changeset_revisions`
- `changeset_reviews`
- `changeset_comments`
- `changeset_comment_revisions`
- `release_batches`
- `release_changesets`
- `environments`
- `deployments`
- `temp_environments`
- `jobs`
- `audit_events`
- `notification_preferences`
- `invites`

### 7.2 Important fields

`apps`

- `id`, `name`, `repo_url`, `default_branch=main`
- `baseline_mode` (`main_head` | `canonical_env_release`)
- `canonical_env_id`
- `file_size_limit_bytes` (default 5 MB)
- `blocked_paths[]`
- `commit_mode_default` (`submit_commit` | `manual_checkpoint`)

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

`audit_events`

- `id`, `occurred_at`, `actor_user_id`, `app_id`
- `entity_type`, `entity_id`, `action`
- `before`, `after`, `git_sha?`, `context` (ip, user_agent, request_id)

## 8) API Contract (v1)

Base path: `/api`

## 8.1 Apps

- `GET /api/apps?page=&limit=`
- `POST /api/apps`
- `GET /api/apps/:appId`
- `PATCH /api/apps/:appId/settings`
- `GET /api/apps/:appId/members?page=&limit=`
- `POST /api/apps/:appId/invites`
- `POST /api/apps/:appId/invites/:inviteId/resend`
- `DELETE /api/apps/:appId/invites/:inviteId`

## 8.2 Workspaces

- `GET /api/apps/:appId/workspaces?page=&limit=`
- `POST /api/apps/:appId/workspaces`
- `GET /api/apps/:appId/workspaces/:workspaceId`
- `PATCH /api/apps/:appId/workspaces/:workspaceId`
- `POST /api/apps/:appId/workspaces/:workspaceId/reset`
- `POST /api/apps/:appId/workspaces/:workspaceId/sync-main`

## 8.3 Workspace files

- `GET /api/apps/:appId/workspaces/:workspaceId/files?path=`
- `PUT /api/apps/:appId/workspaces/:workspaceId/files` (body: `path`, `content`)
- `DELETE /api/apps/:appId/workspaces/:workspaceId/files` (body: `path`)
- `POST /api/apps/:appId/workspaces/:workspaceId/checkpoints`

## 8.4 Changesets

- `GET /api/apps/:appId/changesets?page=&limit=&state=`
- `POST /api/apps/:appId/changesets` (from workspace)
- `GET /api/apps/:appId/changesets/:changesetId`
- `PATCH /api/apps/:appId/changesets/:changesetId`
- `POST /api/apps/:appId/changesets/:changesetId/submit`
- `POST /api/apps/:appId/changesets/:changesetId/resubmit`
- `POST /api/apps/:appId/changesets/:changesetId/review`
- `POST /api/apps/:appId/changesets/:changesetId/queue`
- `POST /api/apps/:appId/changesets/:changesetId/move-to-draft`

## 8.5 Diffs, comments, and AI

- `GET /api/apps/:appId/changesets/:changesetId/diff?mode=raw|semantic`
- `GET /api/apps/:appId/changesets/:changesetId/comments?page=&limit=`
- `POST /api/apps/:appId/changesets/:changesetId/comments`
- `PATCH /api/apps/:appId/changesets/:changesetId/comments/:commentId` (stores
  comment revision history)
- `POST /api/apps/:appId/changesets/:changesetId/analyze`
- `POST /api/apps/:appId/changesets/:changesetId/chat`

Semantic diff API contract (v1 standard):

```ts
type SemanticConfigType =
  | 'entity'
  | 'page'
  | 'queue'
  | 'provider'
  | 'workflow'
  | 'tenant'
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

- `GET /api/apps/:appId/releases?page=&limit=&state=`
- `POST /api/apps/:appId/releases` (create draft release)
- `POST /api/apps/:appId/releases/:releaseId/changesets` (add/remove subset)
- `POST /api/apps/:appId/releases/:releaseId/reorder`
- `POST /api/apps/:appId/releases/:releaseId/assemble`
- `POST /api/apps/:appId/releases/:releaseId/publish`
- `GET /api/apps/:appId/releases/:releaseId`

## 8.7 Environments and deployments

- `GET /api/apps/:appId/environments`
- `PATCH /api/apps/:appId/environments`
- `POST /api/apps/:appId/environments/:envId/deploy`
- `POST /api/apps/:appId/environments/:envId/promote`
- `POST /api/apps/:appId/environments/:envId/rollback`
- `GET /api/apps/:appId/deployments?page=&limit=`

## 8.8 Temp environments and jobs

- `POST /api/apps/:appId/temp-envs` (workspace or changeset)
- `GET /api/apps/:appId/temp-envs?page=&limit=`
- `POST /api/apps/:appId/temp-envs/:tempEnvId/extend`
- `POST /api/apps/:appId/temp-envs/:tempEnvId/undo-expire`
- `DELETE /api/apps/:appId/temp-envs/:tempEnvId`
- `GET /api/apps/:appId/jobs/:jobId`
- `GET /api/apps/:appId/jobs?page=&limit=&type=&state=`

## 8.9 Notifications and auth

- `GET /api/me/notification-preferences`
- `PATCH /api/me/notification-preferences`
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
- temp environment expiry warning
- temp environment expired

Per-user single on/off toggle in v1.

## 11) Audit Requirements

Audit everything immutable and append-only:

- workspace creation/reset/sync
- file edits/deletes/checkpoints
- changeset submit/resubmit/review/queue/state changes
- comment creation and comment edits
- release compose/reorder/publish
- deployment/promotion/rollback
- temp env create/extend/expire/undo
- membership/invite/role changes
- settings changes

Retention: keep forever in v1.

## 12) UI Component Scope (v1)

- Dashboard: app cards and role-aware quick actions.
- App shell: sidebar, breadcrumbs, app switcher.
- Workspace page:
  - repo tree with markers
  - editor/diff tabs
  - AI chat panel
  - checkpoint/submit controls
- Changeset list and detail/review UI.
- Queue view for config managers (prioritize and reorder).
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

- Auth/invites, app setup, memberships, RBAC.
- Workspace CRUD and file editing with guardrails.
- Changeset creation/submit/review and approval reset behavior.
- Async `msuite` at submit.

### Phase B: Queue and releases

- Queue-first workflow and revalidation.
- Release draft/assemble/reorder/publish.
- Tagging (`rYYYY.MM.DD.N`) and immutable release records.

### Phase C: Environments and temp envs

- Environment pipeline and deployment orchestration.
- Skip-stage 2-approval flow.
- Rollback modes (revert+new release and redeploy prior tag).
- Temp env lifecycle with TTL, grace period, and undo.

### Phase D: Hardening

- Auditing completeness verification.
- Email notifications and user toggle.
- Operational limits, retries, observability, and backfill tooling.

## 15) Open Items to Confirm During Implementation

- Final email provider and templates.
- Locking mechanism details for per-environment concurrent deployment safety.
