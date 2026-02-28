# Conman V1 Implementation Backlog

Source scope: [`docs/conman-v1-scope.md`](./conman-v1-scope.md)

## 1) Dependency-Ordered Execution Plan

Execution order (topological):

1. E00 Platform foundation
2. E01 Git adapter (`gitaly-rs`) + repository abstraction
3. E02 Auth, invites, memberships, RBAC
4. E03 Team/repo setup + settings + environments metadata
5. E04 Workspace lifecycle + file operations + guardrails
6. E05 Changeset lifecycle + review + comments + revisions
7. E06 Async jobs + `msuite` execution pipeline
8. E07 Queue-first orchestration + revalidation loop
9. E08 Release assembly + tagging + publish
10. E09 Deployment/promotion/rollback orchestration
11. E10 Temp environments (workspace/changeset) + TTL/grace
12. E11 Notifications + audit completeness

Parallelizable tracks after E06:

- E08 (release assembly) can proceed with E10 (temp envs).
- E11 (notifications/audit polish) can run alongside E09/E10.

## 2) Epics and Issues

## E00 Platform Foundation

Goal: Establish service skeleton and shared primitives.

Issues:

1. E00-01: Create server modules and routing skeleton under `/api`.
2. E00-02: Add MongoDB connection, health checks, and collection bootstrap.
3. E00-03: Add config system (env vars, feature flags, limits, runtime profile
   encryption keys, temp URL domain).
4. E00-04: Standard error envelope + request tracing IDs.
5. E00-05: Add pagination helpers (`page`, `limit`) and validation middleware.

Acceptance:

- Server boots with health endpoint and typed route stubs.
- Mongo connection resilient with startup validation.
- Shared request/response and validation utilities are used by all new routes.

Depends on: none.

## E01 Git Adapter Service (`gitaly-rs` boundary)

Goal: Isolate Git operations behind a Conman adapter interface.

Issues:

1. E01-01: Define `GitAdapter` interface (branch, read/write file, diff, commit,
   rebase/merge, tag, revert).
2. E01-02: Implement `GitalyRsAdapter` against current `gitaly-rs` API.
3. E01-03: Build fake/in-memory adapter for tests.
4. E01-04: Add optimistic operation guards and retry semantics for transient
   Git failures.
5. E01-05: Add integration tests for critical flows (workspace create, submit,
   release publish, rollback).

Acceptance:

- No route calls `gitaly-rs` directly.
- Adapter can be swapped in tests without networked Git backend.

Depends on: E00.

## E02 Auth, Invites, Memberships, RBAC

Goal: Secure access and per-repository role model.

Issues:

1. E02-01: Local email/password auth with password hashing and sessions/JWT.
2. E02-02: Invite-only onboarding (`admin`), 7-day token expiry.
3. E02-03: Forgot/reset password via email token.
4. E02-04: App membership model with roles: `user`, `reviewer`,
   `config_manager`, `admin`.
5. E02-05: Authorization middleware + policy checks per endpoint.

Acceptance:

- Unauthorized access denied by default.
- Role matrix enforced according to scope doc.

Depends on: E00.

## E03 Team/Repo Setup, Settings, Environment Metadata

Goal: Manage team/repo setup, repository-level configuration, and baseline behavior.

Issues:

1. E03-01: `teams` create/list/get APIs.
2. E03-02: Repository creation under team + `repos` list/get APIs.
3. E03-03: App CRUD/list APIs per repository.
4. E03-04: Settings API for baseline mode, canonical env, commit mode default,
   blocked paths, file size limit.
5. E03-05: Environment stage CRUD with canonical user-facing environment flag.
6. E03-06: Membership listing and role assignment APIs.
7. E03-07: Runtime profile CRUD/revisions, environment linkage, canonical
   approval policy config.
8. E03-08: Runtime profile secret visibility rules (`admin` reveal endpoint,
   masked previews for other roles) and typed env var schema validation.
9. E03-09: Runtime profile `app_endpoints` persistence and validation
   against app keys.
10. E03-10: Direct app-admin runtime profile emergency edit flow (audited).

Acceptance:

- App admin can create teams, repositories, and apps before
  workspace/changeset flow.
- Repository admin can configure baseline mode (`integration_head` or
  `canonical_env_release`).
- Environment pipeline metadata is repository-configurable.

Depends on: E02, E01.

## E04 Workspace Lifecycle + File Operations

Goal: Deliver editable workspaces with Git-backed persistence.

Issues:

1. E04-01: Create default workspace branch (`ws/<user>/<app>`) on first use.
2. E04-02: Workspace CRUD (reserve multi-workspace APIs, UI can hide extras).
3. E04-03: File tree/list/read/write/delete endpoints using `path` query/body.
4. E04-04: Guardrails for blocked paths and max file size (default 5 MB,
   app-configurable).
5. E04-05: Workspace reset/sync-integration flow with rebase/merge fallback.
6. E04-06: Conflict detection primitives for later changeset/release flows.

Acceptance:

- Users can edit full repo except blocked paths.
- Workspace sync produces deterministic conflict status for UI.

Depends on: E03, E01.

## E05 Changesets, Review, Comments, Revisions

Goal: Implement full changeset lifecycle through approval.

Issues:

1. E05-01: Changeset CRUD from workspace (one open changeset per workspace).
2. E05-02: Submit/resubmit logic with frozen `head_sha` and revision increment.
3. E05-03: Approval workflow with reset-on-new-commit behavior.
4. E05-04: Review actions (approve/request changes/reject).
5. E05-05: Diff endpoints (`raw`, `semantic`) and semantic diff contract.
6. E05-06: Comment threads with editable comments + revision history.
7. E05-07: AI analyze/chat endpoints scoped to workspace/changeset.
8. E05-08: Changeset profile overrides (`changeset_profile_overrides`) with
   release-travel semantics.
9. E05-09: Auto-include profile overrides on submit with explicit submit
   summary payload.

Acceptance:

- State transitions match spec.
- New commits during review reset approvals and preserve revision history.

Depends on: E04, E02.

## E06 Async Jobs + `msuite` Pipeline

Goal: Run mandatory checks asynchronously with logs and status APIs.

Issues:

1. E06-01: Generic jobs framework (`queued/running/succeeded/failed/canceled`).
2. E06-02: Job worker for `msuite_submit`, `msuite_merge`, `msuite_deploy`,
   and runtime-profile drift check jobs.
3. E06-03: Structured job logs and result payload storage.
4. E06-04: Gate hooks in submit/queue/release/deploy flows with configurable
   runtime profile scope and command.
5. E06-05: Retry and timeout policies with failure reason codes.
6. E06-06: Persist migration execution metadata for release/deploy validation.

Acceptance:

- Submit, release, deploy are blocked on failing `msuite`.
- Job status pollable via API.

Depends on: E00, E05.

## E07 Queue-First Orchestration + Revalidation

Goal: Move approved changesets into managed queue with automatic revalidation.

Issues:

1. E07-01: `approved -> queued` transition and queue ordering metadata.
2. E07-02: Queue selection and manual reorder APIs (audited).
3. E07-03: Revalidation trigger after each published release.
4. E07-04: Conflict + full `msuite` revalidation for queued changesets.
5. E07-05: Transition to `conflicted` or `needs_revalidation` and return-to-draft
   operations (author or config manager).
6. E07-06: Detect override-key collisions between queued changesets and mark
   later ones `conflicted`.
7. E07-07: Treat equal typed override values for same key/target as
   non-conflicting.

Acceptance:

- Non-selected queued changesets remain queued.
- Revalidation updates statuses correctly and emits notifications.

Depends on: E06, E05.

## E08 Release Assembly, Publish, and Tagging

Goal: Compose subset releases from queue and publish immutable artifacts.

Issues:

1. E08-01: Draft release creation and selected changeset association.
2. E08-02: Ordered composition engine (manual order by config manager).
3. E08-03: Publish flow to `integration_branch` + lightweight tag
   `rYYYY.MM.DD.N`.
4. E08-04: Persist release metadata (`published_sha`, actor, timestamps).
5. E08-05: Release state machine enforcement.
6. E08-06: Env-profile-only validation gate at publish.

Acceptance:

- Release can include subset of queued changesets.
- Publish is immutable and auditable.

Depends on: E07, E01, E06.

## E09 Deploy, Promote, Skip, Rollback

Goal: Deliver environment movement and recovery workflows.

Issues:

1. E09-01: Deploy release to environment (async).
2. E09-02: Promote same immutable release across stages.
3. E09-03: Skip-stage and concurrent multi-env deploy approvals:
   2 distinct users, at least one privileged role.
4. E09-04: Deployment lock scope per environment.
5. E09-05: Rollback mode A: `revert(integration_branch) + new release`.
6. E09-06: Rollback mode B: redeploy prior release tag.
7. E09-07: Runtime profile drift check (env vars, secrets, URL, DB settings,
   migrations) and deploy block until revalidation.
8. E09-08: Drift remediation helper: create drift-fix changeset from blocked
   deployment context.

Acceptance:

- Concurrent deploy allowed only with required approvals.
- Both rollback modes available and audited.

Depends on: E08, E06, E03.

## E10 Temp Environments + TTL Lifecycle

Goal: Enable on-demand validation environments for workspace/changeset.

Issues:

1. E10-01: Create temp env (`workspace` or `changeset`) on demand.
2. E10-02: TTL tracking (24h idle) based on API/test/deploy activity.
3. E10-03: Soft expiry + 1h grace + undo-expire.
4. E10-04: Manual TTL extension endpoint.
5. E10-05: Cleanup workers and DB teardown.
6. E10-06: Derive temp runtime profiles from base profile with readable URL
   generation and Mongo snapshot->dump/restore strategy.
7. E10-07: One URL per temp-env instance (no workspace-stable host reuse).

Acceptance:

- Temp envs expire on idle and can be restored during grace.
- Lifecycle events generate audit + email.

Depends on: E06, E03.

## E11 Notifications + Audit Completeness

Goal: Full observability of user-visible events and immutable history.

Issues:

1. E11-01: Email templates and provider integration.
2. E11-02: Per-user on/off notification preferences.
3. E11-03: Event fanout for required notifications.
4. E11-04: Append-only audit event writer + schema enforcement.
5. E11-05: Backfill audit for critical legacy transitions (if any).
6. E11-06: Runtime profile and drift notifications.

Acceptance:

- All scoped events emit notifications (when user enabled).
- All privileged/critical actions captured in immutable audit log.

Depends on: E05, E07, E08, E09, E10.

## 3) Milestone Cuts

## M1: Authoring + Review Baseline

Includes: E00-E06 partially

Scope:

- Auth/invite/RBAC
- App/workspace setup
- File editing + guardrails
- Changeset submit/review/revisions/comments
- Async `msuite` at submit

Exit criteria:

- Users can author and submit changesets.
- Reviewers can approve/reject/request changes.

## M2: Queue + Release Management

Includes: E07-E08

Scope:

- Queue-first workflow
- Auto revalidation after release
- Release composition/reorder/publish
- Immutable tagging

Exit criteria:

- Config manager can publish subset-based releases safely.

## M3: Environments + Recovery

Includes: E09-E10

Scope:

- Deploy/promote
- Skip-stage/concurrent deployment approvals
- Rollback modes
- Temp environments with TTL/grace

Exit criteria:

- Full release movement and recovery paths are operational.

## M4: Notifications and Audit

Includes: E11

Scope:

- Email notifications
- Audit completeness

Exit criteria:

- Notification coverage and audit completeness pass.

## 4) Critical Path

Critical path items (must finish in order):

1. E00 -> E01 -> E04 -> E05 -> E06 -> E07 -> E08 -> E09

Fast-follow but not blocking first release assembly:

1. E10 (temp envs)
2. E11 (notifications/audit polish)

## 5) Suggested First Sprint (Execution-Ready)

1. E00-01/02/04
2. E01-01/03
3. E02-01/02/04/05
4. E03-01/02/03/04
5. E04-01/03/04
6. E05-01/02

Definition of done for Sprint 1:

- Authenticated user can create team + repository, get default workspace,
  edit files with guardrails, create changeset, and submit it with persisted
  revision + `head_sha`.
