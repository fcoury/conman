# Conman V1 Implementation Guide

**Goal:** Build a Git-backed configuration manager for DxFlow-style config
repositories. API-only backend in Rust.

**Architecture:** Axum HTTP server exposing a REST API. Git operations delegated
to a running gitaly-rs instance via gRPC (Tonic client). MongoDB stores workflow
state, audit trails, and tenant/repository metadata. Async job runner handles long-running
operations (msuite, revalidation, deployments). Runtime profiles model URL/env
vars/secrets/database/data configuration for environments and temp envs, including
per-surface endpoint mappings.

**Tech Stack:**

| Component        | Choice                       | Version                  |
| ---------------- | ---------------------------- | ------------------------ |
| Language         | Rust                         | edition 2024             |
| HTTP framework   | Axum                         | 0.8                      |
| gRPC client      | Tonic                        | 0.12                     |
| Protobuf         | Prost                        | 0.13                     |
| Async runtime    | Tokio                        | 1.x                      |
| MongoDB driver   | mongodb (official)           | latest                   |
| Serialization    | serde + serde_json           | latest                   |
| Error handling   | thiserror                    | latest                   |
| Password hashing | argon2                       | latest                   |
| JWT              | jsonwebtoken                 | latest                   |
| Time             | chrono                       | latest                   |
| Tracing          | tracing + tracing-subscriber | latest                   |
| UUID             | uuid                         | latest (`v7` generation) |

**Full scope:** [`docs/conman-v1-scope.md`](./conman-v1-scope.md)
and [`docs/runtime-profiles-draft.md`](./runtime-profiles-draft.md)

---

## 1. Crate Structure

Cargo workspace with 7 crates. Dependency arrows point downward.

```
conman (binary)
├── conman-api        (Axum router, handlers, middleware, extractors)
│   ├── conman-core
│   ├── conman-db
│   ├── conman-git
│   ├── conman-jobs
│   └── conman-auth
├── conman-core       (domain types, state machines, business rules — zero infra deps)
├── conman-db         (MongoDB repositories, index setup)
│   └── conman-core
├── conman-git        (Tonic client wrapping gitaly-rs gRPC)
│   └── conman-core
├── conman-jobs       (async job runner, workers)
│   ├── conman-core
│   ├── conman-db
│   └── conman-git
└── conman-auth       (password hashing, JWT, RBAC policy)
    └── conman-core
```

### Crate responsibilities

**`conman-core`** — Pure domain layer. No IO, no frameworks. Contains:

- Domain structs: `Tenant`, `App` (repository record), `AppSurface`,
  `Workspace`, `Changeset`, `Release`, `Deployment`, etc.
- Enums: `ChangesetState`, `ReleaseState`, `DeploymentState`, `Role`, `BaselineMode`
- State machine transition functions with guard conditions
- Validation logic (blocked paths, file size limits, branch naming)
- Error types (`ConmanError` enum via thiserror)

**`conman-db`** — MongoDB persistence. Contains:

- One repository struct per collection (e.g., `AppRepo`, `WorkspaceRepo`)
- Index creation at startup
- BSON serialization/deserialization
- Query builders for filtered/paginated listing
- Audit event writer

**`conman-git`** — Gitaly-rs gRPC client. Contains:

- `GitalyClient` struct holding tonic channel + service stubs
- Methods mapping domain operations to gRPC calls
- Type conversion between gitaly proto types and `conman-core` domain types
- Retry logic for transient gRPC failures

**`conman-auth`** — Authentication and authorization. Contains:

- Password hashing (argon2) and verification
- JWT token issuance and validation
- `AuthUser` struct (extracted from JWT claims)
- RBAC policy: `fn check_permission(user, app_id, capability) -> Result<()>`

**`conman-api`** — HTTP layer. Contains:

- Axum router with all route definitions
- Handler functions (resources include tenants, repos/surfaces, apps-compat,
  workspaces, changesets, etc.)
- Middleware: auth extraction, request ID injection, error mapping
- Request/response types (API-facing DTOs, not domain types)
- Pagination extractor

**`conman-jobs`** — Background processing. Contains:

- Job runner (polls MongoDB `jobs` collection)
- Worker implementations per job type
- Job state machine management
- Structured log writer

**`conman`** — Binary. Contains:

- `main.rs`: parse config → connect MongoDB → connect gitaly → build Axum app → start job runner → serve

---

## 2. Conventions

### Error handling

`conman-core` defines the error enum:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ConmanError {
    #[error("not found: {entity} {id}")]
    NotFound { entity: &'static str, id: String },

    #[error("conflict: {message}")]
    Conflict { message: String },

    #[error("forbidden: {message}")]
    Forbidden { message: String },

    #[error("validation: {message}")]
    Validation { message: String },

    #[error("invalid state transition: {from} -> {to}")]
    InvalidTransition { from: String, to: String },

    #[error("git error: {message}")]
    Git { message: String },

    #[error("internal: {message}")]
    Internal { message: String },
}
```

`conman-api` maps this to HTTP responses:

```rust
impl IntoResponse for ConmanError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            ConmanError::NotFound { .. } => (StatusCode::NOT_FOUND, "not_found"),
            ConmanError::Conflict { .. } => (StatusCode::CONFLICT, "conflict"),
            ConmanError::Forbidden { .. } => (StatusCode::FORBIDDEN, "forbidden"),
            ConmanError::Validation { .. } => (StatusCode::BAD_REQUEST, "validation_error"),
            ConmanError::InvalidTransition { .. } => (StatusCode::CONFLICT, "invalid_transition"),
            ConmanError::Git { .. } => (StatusCode::BAD_GATEWAY, "git_error"),
            ConmanError::Internal { .. } => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };
        // ... build JSON envelope
    }
}
```

### Response envelope

Success:

```json
{
  "data": { ... },
  "pagination": { "page": 1, "limit": 20, "total": 42 }
}
```

Error:

```json
{
  "error": {
    "code": "not_found",
    "message": "not found: changeset abc123",
    "request_id": "req-uuid-here"
  }
}
```

### Pagination

Query params `page` (1-based, default 1) and `limit` (default 20, max 100).
Axum extractor:

```rust
#[derive(Debug, Deserialize)]
pub struct Pagination {
    #[serde(default = "default_page")]
    pub page: u64,
    #[serde(default = "default_limit")]
    pub limit: u64,
}
```

### Request tracing

Every request gets a UUID via middleware (`X-Request-Id` header or generated).
Conman standard is UUIDv7 for generated IDs.
Propagated through tracing spans and included in error responses.

### MongoDB patterns

- Collection names: snake_case plural (`apps`, `workspaces`, `changesets`)
- Document IDs: `ObjectId`, serialized as hex strings in API responses
- Timestamps: `chrono::DateTime<Utc>` stored as BSON DateTime
- No soft deletes — audit log is the history
- Indexes created at startup by each repo's `ensure_indexes()` method
- Optimistic concurrency where needed via version field or expected state checks

### File paths in API

File paths are always sent as query parameters or JSON body fields.
Never as URL path segments (avoids encoding issues with `/` in paths).

### Testing strategy

- **Unit tests**: `#[cfg(test)]` modules in each crate. Pure logic, no IO.
- **Integration tests**: `tests/` directory in workspace root. Require:
  - Running MongoDB (testcontainers or local)
  - Mock gitaly gRPC server (tonic mock)
- **Test helpers**: factory functions for creating test fixtures
  (`test_app()`, `test_workspace()`, `test_user()`, etc.)
- All public functions have at least one test
- State machine transitions have exhaustive positive + negative tests

### Audit pattern

Every mutation handler emits an audit event after success:

```rust
audit_repo.emit(AuditEvent {
    occurred_at: Utc::now(),
    actor_user_id: auth_user.id,
    app_id: Some(app_id),
    entity_type: "changeset",
    entity_id: changeset_id.to_hex(),
    action: "submitted",
    before: Some(serde_json::to_value(&old_state)?),
    after: Some(serde_json::to_value(&new_state)?),
    git_sha: Some(head_sha),
    context: request_context.clone(),
}).await;
```

Audit writes are fire-and-forget (logged on failure, never block the request).

---

## 3. Cross-Cutting Concerns

### Authentication flow

1. `POST /api/auth/login` — validate email/password → issue JWT (24h expiry)
2. Axum middleware extracts `Authorization: Bearer <token>` header
3. Middleware decodes JWT → queries `app_memberships` → populates
   `Extension<AuthUser>`:
   ```rust
   pub struct AuthUser {
       pub user_id: ObjectId,
       pub email: String,
       pub roles: HashMap<ObjectId, Role>,  // app_id -> role
   }
   ```
4. Route handlers call `auth_user.require_role(app_id, Role::ConfigManager)?`
5. Returns `ConmanError::Forbidden` on failure

### Gitaly-rs connection

- Tonic `Channel` created at startup from `CONMAN_GITALY_ADDRESS` env var
- Single channel with HTTP/2 multiplexing (no manual pool needed)
- Service stubs created per-request from the shared channel
- Retry on `UNAVAILABLE` and `DEADLINE_EXCEEDED` (3 attempts, exponential backoff)
- Each `App` maps to a gitaly `Repository`:
  ```rust
  fn app_to_gitaly_repo(app: &App) -> gitaly::Repository {
      gitaly::Repository {
          storage_name: "default".to_string(),
          relative_path: app.repo_path.clone(),
          gl_repository: format!("app-{}", app.id.to_hex()),
          ..Default::default()
      }
  }
  ```

### Configuration

Loaded from environment variables with `CONMAN_` prefix:

| Variable                    | Default                     | Description                                           |
| --------------------------- | --------------------------- | ----------------------------------------------------- |
| `CONMAN_PORT`               | `3000`                      | HTTP listen port                                      |
| `CONMAN_MONGO_URI`          | `mongodb://localhost:27017` | MongoDB connection string                             |
| `CONMAN_MONGO_DB`           | `conman`                    | Database name                                         |
| `CONMAN_GITALY_ADDRESS`     | `http://localhost:8075`     | gitaly-rs gRPC address                                |
| `CONMAN_JWT_SECRET`         | (required)                  | JWT signing secret                                    |
| `CONMAN_JWT_EXPIRY_HOURS`   | `24`                        | JWT token lifetime                                    |
| `CONMAN_INVITE_EXPIRY_DAYS` | `7`                         | Invite token lifetime                                 |
| `CONMAN_SECRETS_MASTER_KEY` | (required)                  | Master key for envelope encryption of runtime secrets |
| `CONMAN_TEMP_URL_DOMAIN`    | (required)                  | Base domain for generated temp runtime URLs           |

---

## 4. Domain Quick Reference

### Terminology

| Term            | Definition                                                                                    |
| --------------- | --------------------------------------------------------------------------------------------- |
| Tenant          | Top-level customer/account boundary for repositories                                           |
| Repository      | Managed config repository (stored in `App`; `/api/repos` primary, `/api/apps` compatibility) |
| App Surface     | User-facing app within a repository (domains/branding/role hints)                             |
| Workspace       | User-owned mutable branch (`ws/<user>/<app>`)                                                 |
| Changeset       | Reviewable proposal: workspace HEAD vs integration baseline                                   |
| Release         | Immutable Git tag (`rYYYY.MM.DD.N`) of composed changesets                                    |
| Environment     | Deploy target stage (Dev, QA, UAT, Prod)                                                      |
| Runtime Profile | Versioned runtime blueprint (URL, surface endpoints, env vars, secrets, DB/data strategy)     |
| Canonical env   | Production-facing environment for baseline calculations                                       |
| Baseline        | The reference point workspaces branch from (integration branch HEAD or canonical env release) |

### Changeset states

```
draft
  → submitted
    → in_review
      → approved → queued → released (terminal)
      → changes_requested → draft
      → rejected (terminal)
    queued → conflicted → draft
         → needs_revalidation → draft
```

Rules:

- New commits while `submitted`/`in_review`: keep same changeset, create revision, reset approvals
- One open changeset per workspace branch
- After approval + further edits needed: create new changeset

### Release states

```
draft_release → assembling → validated → published
  → deployed_partial → deployed_full
  → rolled_back
```

### Deployment states

```
pending → running → succeeded | failed | canceled
```

### RBAC permission matrix

| Capability                                  | user | reviewer | config_manager | app_admin |
| ------------------------------------------- | :--: | :------: | :------------: | :-------: |
| Read app/repo metadata                      |  Y   |    Y     |       Y        |     Y     |
| Create/edit own workspace                   |  Y   |    Y     |       Y        |     Y     |
| Create/modify own changeset                 |  Y   |    Y     |       Y        |     Y     |
| Submit changeset                            |  Y   |    Y     |       Y        |     Y     |
| Comment in review                           |  Y   |    Y     |       Y        |     Y     |
| Approve/request changes/reject              |  -   |    Y     |       Y        |     Y     |
| Move conflicted/needs_revalidation to draft | Own  |   Own    |      Any       |    Any    |
| Assemble release from queue                 |  -   |    -     |       Y        |     Y     |
| Publish release                             |  -   |    -     |       Y        |     Y     |
| Deploy/promote release                      |  -   |    -     |       Y        |     Y     |
| Skip stage / concurrent deploy approval     |  -   |    Y     |       Y        |     Y     |
| Invite users                                |  -   |    -     |       -        |     Y     |
| Manage app settings/roles/envs              |  -   |    -     |       -        |     Y     |

`app_admin` inherits all `config_manager` capabilities.

### Baseline resolution

```rust
fn resolve_baseline(app: &App, envs: &[Environment], releases: &[Release]) -> String {
    match app.baseline_mode {
        BaselineMode::IntegrationHead => {
            format!("refs/heads/{}", app.integration_branch)
        }
        BaselineMode::CanonicalEnvRelease => {
            // Find latest deployed release to canonical environment
            // Fallback to integration branch HEAD if no release exists
        }
    }
}
```

### Runtime profile defaults

```rust
pub struct ValidationGates {
    // submit: temp profile only
    pub submit_scope: ValidationScope, // TempOnly
    // release publish: environment profiles only
    pub release_scope: ValidationScope, // EnvOnly
    // deploy: target environment profile only
    pub deploy_scope: ValidationScope, // TargetEnvOnly
}
```

Runtime profile rules in v1:

- Profiles are versioned and tied to releases.
- Precedence is `app defaults < environment profile < temp overrides`.
- `surface_endpoints` keys must map to existing app-surface keys for that repo.
- Secrets are encrypted at rest via envelope encryption (master key from
  config, per-record data keys).
- Secret plaintext reveal is `app_admin`-only; other roles get masked previews.
- Env vars are typed (`string | number | boolean | json`).
- Runtime profile schema is strict typed (no arbitrary top-level custom fields).
- Canonical environment profile changes default to stricter two-approval policy
  (configurable to `same_as_changeset`).
- `app_admin` emergency direct profile edits are allowed, audited, and still
  trigger deploy drift blocking until revalidation passes.
- Changeset profile overrides are auto-included on submit and shown in submit
  summary.
- Deploy is blocked on profile drift across env vars, secrets, URL, DB settings,
  or migration set differences.

---

## 5. Epic Index

Execution order is topological. Each epic file is self-contained with Rust types,
database schemas, API endpoints, proto definitions, implementation checklist, and
test cases.

| Epic                              | Name                  | Dependencies  | Summary                                                                |
| --------------------------------- | --------------------- | ------------- | ---------------------------------------------------------------------- |
| [E00](epics/E00-platform.md)      | Platform Foundation   | none          | Server skeleton, MongoDB bootstrap, config, error envelope, pagination |
| [E01](epics/E01-git-adapter.md)   | Git Adapter           | E00           | Tonic client wrapping gitaly-rs gRPC services                          |
| [E02](epics/E02-auth.md)          | Auth & RBAC           | E00           | Local auth, invites, memberships, role-based access                    |
| [E03](epics/E03-app-setup.md)     | Tenant/Repo Setup     | E01, E02      | Tenant + repo + surface APIs, settings, environment metadata, runtime profiles |
| [E04](epics/E04-workspaces.md)    | Workspaces            | E01, E03      | Workspace lifecycle, file operations, guardrails                       |
| [E05](epics/E05-changesets.md)    | Changesets            | E02, E04      | Changeset lifecycle, review, comments, diffs, profile overrides        |
| [E06](epics/E06-async-jobs.md)    | Async Jobs            | E00, E05      | Job framework, msuite workers, profile-aware gates/drift jobs          |
| [E07](epics/E07-queue.md)         | Queue Orchestration   | E05, E06      | Queue-first workflow, revalidation loop, override-key conflicts        |
| [E08](epics/E08-releases.md)      | Releases              | E01, E06, E07 | Release assembly, env-profile validation, tagging, publish             |
| [E09](epics/E09-deployments.md)   | Deployments           | E03, E06, E08 | Deploy, promote, skip-stage, rollback, drift blocking                  |
| [E10](epics/E10-temp-envs.md)     | Temp Environments     | E03, E06      | On-demand envs, profile derivation, TTL, cleanup                       |
| [E11](epics/E11-notifications.md) | Notifications & Audit | E05-E10       | Email notifications, audit completeness, runtime profile events        |
| [E12](epics/E12-hardening.md)     | Hardening             | E08-E11       | Load testing, fault injection, encryption/rotation runbooks            |

### Critical path

```
E00 → E01 → E03 → E04 → E05 → E06 → E07 → E08 → E09
```

Parallelizable after E06: E08 can proceed with E10. E11 can run alongside E09/E10.

### Milestone mapping

| Milestone                   | Epics   | Exit criteria                                     |
| --------------------------- | ------- | ------------------------------------------------- |
| M1: Authoring + Review      | E00–E06 | Users can author, submit, and review changesets   |
| M2: Queue + Release         | E07–E08 | Config managers can publish subset-based releases |
| M3: Environments + Recovery | E09–E10 | Full release movement and recovery paths          |
| M4: Operations + Launch     | E11–E12 | Production-readiness checklist passes             |
