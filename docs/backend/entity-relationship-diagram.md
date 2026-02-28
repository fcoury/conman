# Conman Entity Relationship Diagram

This document captures the entities currently implemented and how they relate.

## Naming note

- `Repo` is the managed configuration repository (`/api/repos`).
- `App` is a user-facing application under a repository (`/api/repos/:repoId/apps`).

## Primary ER diagram

```mermaid
erDiagram
    TEAM ||--o{ REPO : owns
    USER ||--o{ TEAM_MEMBERSHIP : belongs_to
    TEAM ||--o{ TEAM_MEMBERSHIP : has_members
    TEAM ||--o{ INVITE : issues
    USER ||--o{ PASSWORD_RESET_TOKEN : resets

    REPO ||--o{ REPO_MEMBERSHIP : grants_roles
    USER ||--o{ REPO_MEMBERSHIP : assigned
    REPO ||--o{ APP : exposes

    REPO ||--o{ ENVIRONMENT : defines
    REPO ||--o{ RUNTIME_PROFILE : defines
    ENVIRONMENT }o--|| RUNTIME_PROFILE : runtime_profile_id
    RUNTIME_PROFILE }o--|| RUNTIME_PROFILE : base_profile_id

    REPO ||--o{ WORKSPACE : has
    USER ||--o{ WORKSPACE : owns

    REPO ||--o{ CHANGESET : scopes
    WORKSPACE ||--o{ CHANGESET : source
    USER ||--o{ CHANGESET : authors
    CHANGESET ||--o{ CHANGESET_COMMENT : has
    USER ||--o{ CHANGESET_COMMENT : writes

    CHANGESET ||--o{ CHANGESET_PROFILE_OVERRIDE : overrides
    RUNTIME_PROFILE ||--o{ CHANGESET_PROFILE_OVERRIDE : targets

    REPO ||--o{ RELEASE_BATCH : creates
    RELEASE_BATCH }o--o{ CHANGESET : includes_ordered

    RELEASE_BATCH ||--o{ DEPLOYMENT : deploys
    ENVIRONMENT ||--o{ DEPLOYMENT : target
    USER ||--o{ DEPLOYMENT : created_by

    REPO ||--o{ TEMP_ENVIRONMENT : provisions
    USER ||--o{ TEMP_ENVIRONMENT : owns
    RUNTIME_PROFILE ||--o{ TEMP_ENVIRONMENT : runtime_profile
    WORKSPACE ||--o{ TEMP_ENVIRONMENT : source_workspace
    CHANGESET ||--o{ TEMP_ENVIRONMENT : source_changeset

    REPO ||--o{ JOB : runs
    JOB ||--o{ JOB_LOG_LINE : emits

    USER ||--|| NOTIFICATION_PREFERENCE : has
    USER ||--o{ NOTIFICATION_EVENT : receives
    REPO ||--o{ NOTIFICATION_EVENT : optional_scope

    USER ||--o{ AUDIT_EVENT : actor_optional
    REPO ||--o{ AUDIT_EVENT : optional_scope
```

## Key relationship fields

- `Repo.team_id -> Team.id`
- `App.repo_id -> Repo.id`
- `TeamMembership.user_id -> User.id`
- `TeamMembership.team_id -> Team.id`
- `RepoMembership.user_id -> User.id`
- `RepoMembership.repo_id -> Repo.id`
- `Invite.team_id -> Team.id`
- `Workspace.repo_id -> Repo.id`
- `Workspace.owner_user_id -> User.id`
- `Changeset.repo_id -> Repo.id`
- `Changeset.workspace_id -> Workspace.id`
- `Changeset.author_user_id -> User.id`
- `ChangesetComment.changeset_id -> Changeset.id`
- `ChangesetProfileOverride.changeset_id -> Changeset.id`
- `ChangesetProfileOverride.target_profile_id -> RuntimeProfile.id (optional)`
- `Environment.repo_id -> Repo.id`
- `Environment.runtime_profile_id -> RuntimeProfile.id (optional)`
- `RuntimeProfile.repo_id -> Repo.id`
- `RuntimeProfile.base_profile_id -> RuntimeProfile.id (optional)`
- `ReleaseBatch.repo_id -> Repo.id`
- `ReleaseBatch.ordered_changeset_ids[] -> Changeset.id`
- `Deployment.repo_id -> Repo.id`
- `Deployment.environment_id -> Environment.id`
- `Deployment.release_id -> ReleaseBatch.id`
- `TempEnvironment.repo_id -> Repo.id`
- `TempEnvironment.owner_user_id -> User.id`
- `TempEnvironment.runtime_profile_id -> RuntimeProfile.id (optional)`
- `TempEnvironment.source_id -> Workspace.id or Changeset.id (by kind)`
- `Job.repo_id -> Repo.id`
- `JobLogLine.job_id -> Job.id`
- `NotificationPreference.user_id -> User.id`
- `NotificationEvent.user_id -> User.id`
- `NotificationEvent.repo_id -> Repo.id (optional)`
- `AuditEvent.actor_user_id -> User.id (optional)`
- `AuditEvent.repo_id -> Repo.id (optional)`

## Runtime constraints worth remembering

- One workspace can have many historical changesets, but only one open changeset at a time.
- `ReleaseBatch` to `Changeset` is represented by ordered IDs, not a separate join collection.
- `TempEnvironment.source_id` is polymorphic (`workspace` or `changeset`) via `kind`.
