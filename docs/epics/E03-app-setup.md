# E03 App Setup, Settings, Environment Metadata

## 1. Goal

Deliver app-level CRUD, per-app settings management, environment pipeline
metadata, and membership listing/role assignment. After this epic, an
authenticated app admin can create an app backed by a gitaly repository,
configure baseline mode, define the environment promotion pipeline, and manage
team membership. This epic also establishes runtime profiles and links them to
environments.

**Issues:**

- E03-01: `apps` CRUD and repository registration.
- E03-02: Settings API for baseline mode, canonical env, commit mode default,
  blocked paths, file size limit.
- E03-03: Environment stage CRUD with canonical user-facing environment flag.
- E03-04: Membership listing and role assignment APIs.
- E03-05: Runtime profile CRUD/revisions, environment linkage, and canonical
  profile approval policy settings.
- E03-06: Runtime profile typed env-var validation + secret visibility policy
  (`app_admin` reveal, others masked).
- E03-07: Direct app-admin runtime profile emergency edits (audited).

---

## 2. Dependencies

| Dependency | What it provides |
|------------|------------------|
| E01 Git Adapter | `GitalyClient` with `repository_exists` and `create_repository` methods |
| E02 Auth & RBAC | `AuthUser` extractor, `check_permission()`, `Role` enum, `AppMembershipRepo` |

---

## 3. Rust Types

### conman-core/src/models/app.rs

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Baseline resolution strategy for workspace branching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaselineMode {
    /// Workspaces branch from the current HEAD of `integration_branch`.
    IntegrationHead,
    /// Workspaces branch from the latest release deployed to the canonical environment.
    /// Falls back to integration branch HEAD when no release exists.
    CanonicalEnvRelease,
}

impl Default for BaselineMode {
    fn default() -> Self {
        Self::CanonicalEnvRelease
    }
}

/// Per-app commit strategy controlling when workspace edits become Git commits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitMode {
    /// Autosave to workspace working state; single commit created on submit.
    SubmitCommit,
    /// User-triggered checkpoints become individual commits.
    ManualCheckpoint,
}

impl Default for CommitMode {
    fn default() -> Self {
        Self::SubmitCommit
    }
}

/// App-level settings that control workspace and changeset behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub baseline_mode: BaselineMode,
    /// References an Environment id. None until first environment is created.
    pub canonical_env_id: Option<String>,
    pub commit_mode_default: CommitMode,
    /// Glob patterns that cannot be edited via workspace file operations.
    pub blocked_paths: Vec<String>,
    /// Maximum file size in bytes that can be written to a workspace (default 5 MB).
    pub file_size_limit_bytes: u64,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            baseline_mode: BaselineMode::default(),
            canonical_env_id: None,
            commit_mode_default: CommitMode::default(),
            blocked_paths: vec![
                ".git/**".to_string(),
                ".gitignore".to_string(),
                ".github/**".to_string(),
            ],
            file_size_limit_bytes: 5 * 1024 * 1024, // 5 MB
        }
    }
}

/// A managed configuration repository. One App = one Git repo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct App {
    /// MongoDB ObjectId hex string.
    pub id: String,
    /// Human-readable app name (unique).
    pub name: String,
    /// Gitaly-relative path to the repository (e.g. "conman/my-app.git").
    pub repo_path: String,
    /// Integration branch name. Defaults to "main" in v1.
    pub integration_branch: String,
    /// Embedded settings document.
    pub settings: AppSettings,
    /// User who created the app.
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### conman-core/src/models/runtime_profile.rs

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeProfileKind {
    PersistentEnv,
    TempWorkspace,
    TempChangeset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum EnvVarValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Json(serde_json::Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeProfile {
    pub id: String,
    pub app_id: String,
    pub name: String,
    pub kind: RuntimeProfileKind,
    pub base_url: String,
    pub env_vars: std::collections::BTreeMap<String, EnvVarValue>,
    pub secrets_encrypted: std::collections::BTreeMap<String, String>,
    pub database_engine: String, // mongodb in v1
    pub connection_ref: String,
    pub provisioning_mode: String,
    pub base_profile_id: Option<String>,
    pub migration_paths: Vec<String>,
    pub migration_command: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### conman-core/src/models/environment.rs

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A deploy target stage within an app's promotion pipeline.
/// Environments are ordered by `position` (0-based, lower = earlier stage).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    /// MongoDB ObjectId hex string.
    pub id: String,
    pub app_id: String,
    /// Unique within the app (e.g. "Development", "QA", "UAT", "Production").
    pub name: String,
    /// 0-based position in the promotion pipeline.
    pub position: u32,
    /// True for the canonical user-facing environment used in baseline calculations.
    /// Exactly one environment per app may be canonical.
    pub is_canonical: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### conman-api/src/handlers/apps.rs — Request/Response types

```rust
use serde::{Deserialize, Serialize};

// ── App CRUD ──

#[derive(Debug, Deserialize)]
pub struct CreateAppRequest {
    /// Human-readable name (must be unique).
    pub name: String,
    /// Gitaly-relative repository path. Verified to exist (or optionally created).
    pub repo_path: String,
}

#[derive(Debug, Serialize)]
pub struct AppResponse {
    pub id: String,
    pub name: String,
    pub repo_path: String,
    pub integration_branch: String,
    pub settings: AppSettingsResponse,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct AppSettingsResponse {
    pub baseline_mode: String,
    pub canonical_env_id: Option<String>,
    pub commit_mode_default: String,
    pub blocked_paths: Vec<String>,
    pub file_size_limit_bytes: u64,
}

// ── Settings ──

#[derive(Debug, Deserialize)]
pub struct UpdateAppSettingsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_env_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_mode_default: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_paths: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size_limit_bytes: Option<u64>,
}

// ── Environments ──

#[derive(Debug, Deserialize)]
pub struct CreateEnvironmentRequest {
    pub name: String,
    pub position: u32,
    #[serde(default)]
    pub is_canonical: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEnvironmentsRequest {
    /// Full ordered list of environments. Replaces all positions in one call.
    pub environments: Vec<EnvironmentEntry>,
}

#[derive(Debug, Deserialize)]
pub struct EnvironmentEntry {
    pub id: String,
    pub name: String,
    pub position: u32,
    pub is_canonical: bool,
}

#[derive(Debug, Serialize)]
pub struct EnvironmentResponse {
    pub id: String,
    pub app_id: String,
    pub name: String,
    pub position: u32,
    pub is_canonical: bool,
    pub created_at: String,
    pub updated_at: String,
}

// ── Membership ──

#[derive(Debug, Serialize)]
pub struct MemberResponse {
    pub user_id: String,
    pub email: String,
    pub role: String,
    pub joined_at: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMemberRoleRequest {
    pub role: String,
}
```

---

## 4. Database

### Collection: `apps`

| Field | Type | Description |
|-------|------|-------------|
| `_id` | `ObjectId` | Primary key |
| `name` | `String` | Human-readable name, unique |
| `repo_path` | `String` | Gitaly-relative repository path, unique |
| `integration_branch` | `String` | Configurable integration branch, defaults to `"main"` |
| `settings.baseline_mode` | `String` | `"integration_head"` or `"canonical_env_release"` |
| `settings.canonical_env_id` | `ObjectId?` | References `environments._id` |
| `settings.commit_mode_default` | `String` | `"submit_commit"` or `"manual_checkpoint"` |
| `settings.blocked_paths` | `[String]` | Glob patterns |
| `settings.file_size_limit_bytes` | `i64` | Default 5242880 |
| `created_by` | `ObjectId` | References `users._id` |
| `created_at` | `DateTime` | BSON DateTime |
| `updated_at` | `DateTime` | BSON DateTime |

**Indexes:**

```javascript
// Unique app name
{ "name": 1 }  // unique: true

// Unique repo path (one app per repo)
{ "repo_path": 1 }  // unique: true
```

**Example document:**

```json
{
  "_id": ObjectId("664f1a2b3c4d5e6f70809010"),
  "name": "payments-config",
  "repo_path": "conman/payments-config.git",
  "integration_branch": "main",
  "settings": {
    "baseline_mode": "canonical_env_release",
    "canonical_env_id": ObjectId("664f1a2b3c4d5e6f70809020"),
    "commit_mode_default": "submit_commit",
    "blocked_paths": [".git/**", ".gitignore", ".github/**"],
    "file_size_limit_bytes": 5242880
  },
  "created_by": ObjectId("664f1a2b3c4d5e6f70809001"),
  "created_at": ISODate("2025-06-01T10:00:00Z"),
  "updated_at": ISODate("2025-06-01T10:00:00Z")
}
```

### Collection: `environments`

| Field | Type | Description |
|-------|------|-------------|
| `_id` | `ObjectId` | Primary key |
| `app_id` | `ObjectId` | References `apps._id` |
| `name` | `String` | Stage name, unique within app |
| `position` | `i32` | 0-based pipeline order, unique within app |
| `is_canonical` | `bool` | At most one per app is `true` |
| `created_at` | `DateTime` | BSON DateTime |
| `updated_at` | `DateTime` | BSON DateTime |

**Indexes:**

```javascript
// Unique environment name per app
{ "app_id": 1, "name": 1 }  // unique: true

// Unique position per app
{ "app_id": 1, "position": 1 }  // unique: true

// Canonical lookup (partial index: only where is_canonical == true)
{ "app_id": 1, "is_canonical": 1 }  // partialFilterExpression: { "is_canonical": true }, unique: true
```

**Example document:**

```json
{
  "_id": ObjectId("664f1a2b3c4d5e6f70809020"),
  "app_id": ObjectId("664f1a2b3c4d5e6f70809010"),
  "name": "Production",
  "position": 3,
  "is_canonical": true,
  "created_at": ISODate("2025-06-01T10:00:00Z"),
  "updated_at": ISODate("2025-06-01T10:00:00Z")
}
```

---

## 5. API Endpoints

### 5.1 `GET /api/apps?page=&limit=`

List apps visible to the authenticated user.

| Attribute | Value |
|-----------|-------|
| Auth | Any authenticated user |
| RBAC | Returns only apps where user has a membership |
| Query params | `page` (default 1), `limit` (default 20, max 100) |

**Response 200:**

```json
{
  "data": [
    {
      "id": "664f1a2b3c4d5e6f70809010",
      "name": "payments-config",
      "repo_path": "conman/payments-config.git",
      "integration_branch": "main",
      "settings": {
        "baseline_mode": "canonical_env_release",
        "canonical_env_id": "664f1a2b3c4d5e6f70809020",
        "commit_mode_default": "submit_commit",
        "blocked_paths": [".git/**", ".gitignore", ".github/**"],
        "file_size_limit_bytes": 5242880
      },
      "created_by": "664f1a2b3c4d5e6f70809001",
      "created_at": "2025-06-01T10:00:00Z",
      "updated_at": "2025-06-01T10:00:00Z"
    }
  ],
  "pagination": { "page": 1, "limit": 20, "total": 1 }
}
```

**Errors:**

| Status | Code | Condition |
|--------|------|-----------|
| 401 | `unauthorized` | Missing or invalid JWT |

---

### 5.2 `POST /api/apps`

Create a new app and register its Git repository.

| Attribute | Value |
|-----------|-------|
| Auth | Any authenticated user (becomes `app_admin` of the new app) |
| RBAC | No pre-existing membership required — caller bootstraps the app |

**Request body:**

```json
{
  "name": "payments-config",
  "repo_path": "conman/payments-config.git"
}
```

**Validation:**

- `name`: required, 1-128 chars, alphanumeric + hyphens + underscores.
- `repo_path`: required, must pass gitaly `RepositoryExists` check (or
  `CreateRepository` if repo does not yet exist).

**Response 201:**

```json
{
  "data": {
    "id": "664f1a2b3c4d5e6f70809010",
    "name": "payments-config",
    "repo_path": "conman/payments-config.git",
    "integration_branch": "main",
    "settings": {
      "baseline_mode": "canonical_env_release",
      "canonical_env_id": null,
      "commit_mode_default": "submit_commit",
      "blocked_paths": [".git/**", ".gitignore", ".github/**"],
      "file_size_limit_bytes": 5242880
    },
    "created_by": "664f1a2b3c4d5e6f70809001",
    "created_at": "2025-06-01T10:00:00Z",
    "updated_at": "2025-06-01T10:00:00Z"
  }
}
```

**Side effects:**

1. Verify repo exists via gitaly `RepositoryExists`. If not, create it via
   `CreateRepository` with `default_branch = app.integration_branch`.
2. Insert `apps` document with default settings.
3. Insert `app_memberships` record: caller as `app_admin`.
4. Insert default environment pipeline: Development (0), QA (1), UAT (2),
   Production (3, `is_canonical: true`).
5. Set `settings.canonical_env_id` to the Production environment id.
6. Emit audit event: `app.created`.

**Errors:**

| Status | Code | Condition |
|--------|------|-----------|
| 400 | `validation_error` | Name or repo_path fails validation |
| 409 | `conflict` | App name or repo_path already registered |
| 502 | `git_error` | Gitaly unreachable or repo creation failed |

---

### 5.3 `GET /api/apps/:appId`

Get a single app by id.

| Attribute | Value |
|-----------|-------|
| Auth | Any authenticated user |
| RBAC | `user` role or above on this app |

**Response 200:**

Same shape as individual item in `GET /api/apps` list.

**Errors:**

| Status | Code | Condition |
|--------|------|-----------|
| 403 | `forbidden` | Caller has no membership on this app |
| 404 | `not_found` | App does not exist |

---

### 5.4 `PATCH /api/apps/:appId/settings`

Update app settings. Partial update — only supplied fields are changed.

| Attribute | Value |
|-----------|-------|
| Auth | Authenticated user |
| RBAC | `app_admin` on this app |

**Request body (all fields optional):**

```json
{
  "baseline_mode": "integration_head",
  "canonical_env_id": "664f1a2b3c4d5e6f70809020",
  "commit_mode_default": "manual_checkpoint",
  "blocked_paths": [".git/**", ".gitignore", ".github/**", "secrets/**"],
  "file_size_limit_bytes": 10485760
}
```

**Validation:**

- `baseline_mode`: must be `"integration_head"` or `"canonical_env_release"`.
- `canonical_env_id`: must reference an existing environment belonging to
  this app.
- `commit_mode_default`: must be `"submit_commit"` or `"manual_checkpoint"`.
- `blocked_paths`: each entry must be a non-empty string.
- `file_size_limit_bytes`: must be > 0 and <= 50 MB (52428800).

**Response 200:**

Full updated app object (same shape as `GET /api/apps/:appId`).

**Side effects:**

1. Emit audit event: `app.settings_updated` with `before`/`after` snapshots.

**Errors:**

| Status | Code | Condition |
|--------|------|-----------|
| 400 | `validation_error` | Invalid field values |
| 403 | `forbidden` | Caller is not `app_admin` |
| 404 | `not_found` | App or referenced environment not found |

---

### 5.5 `GET /api/apps/:appId/environments`

List environments for an app, ordered by position.

| Attribute | Value |
|-----------|-------|
| Auth | Authenticated user |
| RBAC | `user` role or above on this app |

**Response 200:**

```json
{
  "data": [
    {
      "id": "664f1a2b3c4d5e6f70809021",
      "app_id": "664f1a2b3c4d5e6f70809010",
      "name": "Development",
      "position": 0,
      "is_canonical": false,
      "created_at": "2025-06-01T10:00:00Z",
      "updated_at": "2025-06-01T10:00:00Z"
    },
    {
      "id": "664f1a2b3c4d5e6f70809022",
      "app_id": "664f1a2b3c4d5e6f70809010",
      "name": "QA",
      "position": 1,
      "is_canonical": false,
      "created_at": "2025-06-01T10:00:00Z",
      "updated_at": "2025-06-01T10:00:00Z"
    },
    {
      "id": "664f1a2b3c4d5e6f70809023",
      "app_id": "664f1a2b3c4d5e6f70809010",
      "name": "UAT",
      "position": 2,
      "is_canonical": false,
      "created_at": "2025-06-01T10:00:00Z",
      "updated_at": "2025-06-01T10:00:00Z"
    },
    {
      "id": "664f1a2b3c4d5e6f70809020",
      "app_id": "664f1a2b3c4d5e6f70809010",
      "name": "Production",
      "position": 3,
      "is_canonical": true,
      "created_at": "2025-06-01T10:00:00Z",
      "updated_at": "2025-06-01T10:00:00Z"
    }
  ]
}
```

**Errors:**

| Status | Code | Condition |
|--------|------|-----------|
| 403 | `forbidden` | Caller has no membership on this app |
| 404 | `not_found` | App does not exist |

---

### 5.6 `PATCH /api/apps/:appId/environments`

Replace the full environment pipeline. Supports add, rename, reorder, remove,
and canonical flag reassignment in a single atomic operation.

| Attribute | Value |
|-----------|-------|
| Auth | Authenticated user |
| RBAC | `app_admin` on this app |

**Request body:**

```json
{
  "environments": [
    { "id": "664f1a2b3c4d5e6f70809021", "name": "Development", "position": 0, "is_canonical": false },
    { "id": "664f1a2b3c4d5e6f70809022", "name": "QA", "position": 1, "is_canonical": false },
    { "id": "new", "name": "Staging", "position": 2, "is_canonical": false },
    { "id": "664f1a2b3c4d5e6f70809020", "name": "Production", "position": 3, "is_canonical": true }
  ]
}
```

**Validation:**

- At least one environment required.
- Exactly one `is_canonical: true`.
- No duplicate names.
- No duplicate positions.
- Positions must be a contiguous 0-based sequence (0, 1, 2, ..., N-1).
- Existing ids must belong to this app.
- Entries with `id: "new"` are created as new environments.
- Environments not present in the list are deleted (only if they have no
  active deployments — otherwise return 409).

**Response 200:**

Full list of environments after update (same shape as `GET .../environments`).

**Side effects:**

1. If `canonical_env_id` in app settings pointed to a removed environment,
   update it to the new canonical environment's id.
2. Emit audit event: `app.environments_updated` with before/after snapshots.

**Errors:**

| Status | Code | Condition |
|--------|------|-----------|
| 400 | `validation_error` | Duplicate names, positions, or missing canonical flag |
| 403 | `forbidden` | Caller is not `app_admin` |
| 404 | `not_found` | App or referenced environment id not found |
| 409 | `conflict` | Attempting to remove environment with active deployments |

---

### 5.7 `GET /api/apps/:appId/members?page=&limit=`

List members of an app with their roles.

| Attribute | Value |
|-----------|-------|
| Auth | Authenticated user |
| RBAC | `user` role or above on this app |
| Query params | `page` (default 1), `limit` (default 20, max 100) |

**Response 200:**

```json
{
  "data": [
    {
      "user_id": "664f1a2b3c4d5e6f70809001",
      "email": "admin@example.com",
      "role": "app_admin",
      "joined_at": "2025-06-01T10:00:00Z"
    },
    {
      "user_id": "664f1a2b3c4d5e6f70809002",
      "email": "dev@example.com",
      "role": "user",
      "joined_at": "2025-06-02T14:30:00Z"
    }
  ],
  "pagination": { "page": 1, "limit": 20, "total": 2 }
}
```

**Errors:**

| Status | Code | Condition |
|--------|------|-----------|
| 403 | `forbidden` | Caller has no membership on this app |
| 404 | `not_found` | App does not exist |

---

## 6. Business Logic

### 6.1 App creation

```
1. Validate CreateAppRequest (name format, repo_path format).
2. Check name uniqueness against `apps` collection.
3. Check repo_path uniqueness against `apps` collection.
4. Call gitaly RepositoryExists(repo_path):
   a. If exists → proceed.
   b. If not exists → call CreateRepository(repo_path, default_branch: app.integration_branch).
   c. If gitaly unreachable → return Git error.
5. Insert App document with default settings.
6. Insert AppMembership: caller as app_admin.
7. Insert default environments:
   - Development (position: 0)
   - QA (position: 1)
   - UAT (position: 2)
   - Production (position: 3, is_canonical: true)
8. Update App.settings.canonical_env_id to Production environment id.
9. Emit audit event: app.created.
10. Return created App.
```

### 6.2 Settings update

```
1. Validate each supplied field.
2. If canonical_env_id is supplied:
   a. Query environments collection for that id + this app_id.
   b. If not found → return NotFound.
3. If baseline_mode is supplied:
   a. Validate enum variant.
   b. If "canonical_env_release" and no canonical_env_id set (neither in
      request nor existing) → return Validation error.
4. Apply partial update to App document.
5. Emit audit event: app.settings_updated (before/after).
6. Return updated App.
```

### 6.3 Environment reorder

```
1. Parse UpdateEnvironmentsRequest.
2. Validate:
   a. At least one environment.
   b. Exactly one is_canonical == true.
   c. No duplicate names.
   d. Positions form contiguous 0..N-1 sequence.
3. Load existing environments for this app.
4. Partition request entries into: update (known ids), create (id == "new"),
   delete (existing ids not in request).
5. For deletions: check no active deployments reference the environment.
   If any → return 409 Conflict.
6. Execute in a single transaction (or ordered writes):
   a. Delete removed environments.
   b. Update existing environments (name, position, is_canonical).
   c. Insert new environments.
7. If app.settings.canonical_env_id was deleted, update it to the new
   canonical environment's id.
8. Emit audit event: app.environments_updated.
9. Return full environment list.
```

### 6.4 Default blocked paths

```rust
const DEFAULT_BLOCKED_PATHS: &[&str] = &[
    ".git/**",
    ".gitignore",
    ".github/**",
];
```

### 6.5 Default file size limit

```rust
const DEFAULT_FILE_SIZE_LIMIT_BYTES: u64 = 5 * 1024 * 1024; // 5 MB
const MAX_FILE_SIZE_LIMIT_BYTES: u64 = 50 * 1024 * 1024;    // 50 MB upper bound
```

---

## 7. Gitaly-rs Integration

Two RPCs from `RepositoryService` are needed for app creation.

### 7.1 RepositoryService.RepositoryExists

Used during `POST /api/apps` to verify the repo path is valid before
persisting the app record.

**Proto definitions** (from `gitaly/proto/repository.proto` and
`gitaly/proto/shared.proto`):

```protobuf
// shared.proto
message Repository {
  reserved 1;
  reserved "path";
  // storage_name identifies which Gitaly storage the repo lives on.
  string storage_name = 2;
  // relative_path is the path of the repository relative to the storage root.
  string relative_path = 3;
  // git_object_directory sets GIT_OBJECT_DIRECTORY envvar.
  string git_object_directory = 4;
  // git_alternate_object_directories sets GIT_ALTERNATE_OBJECT_DIRECTORIES envvar.
  repeated string git_alternate_object_directories = 5;
  // gl_repository is the identifier used in callbacks to identify the repository.
  string gl_repository = 6;
  reserved 7;
  // gl_project_path is the human-readable project path (e.g. "conman/my-app").
  string gl_project_path = 8;
}
```

```protobuf
// repository.proto
service RepositoryService {
  rpc RepositoryExists(RepositoryExistsRequest) returns (RepositoryExistsResponse) {
    option (op_type) = { op: ACCESSOR };
  }
  // ...
}

// RepositoryExistsRequest checks whether a given repository exists.
message RepositoryExistsRequest {
  // repository is the repo to check. storage_name and relative_path must be provided.
  Repository repository = 1 [(target_repository)=true];
}

// RepositoryExistsResponse is the response for RepositoryExists.
message RepositoryExistsResponse {
  // exists indicates whether the repo exists.
  bool exists = 1;
}
```

**Rust wrapper** (in `conman-git`):

```rust
impl GitalyClient {
    /// Check whether a repository exists on the Gitaly storage.
    pub async fn repository_exists(&self, repo_path: &str) -> Result<bool, ConmanError> {
        let repo = self.build_repository(repo_path);
        let request = RepositoryExistsRequest {
            repository: Some(repo),
        };
        let response = self
            .repository_service()
            .repository_exists(request)
            .await
            .map_err(|e| ConmanError::Git {
                message: format!("RepositoryExists failed: {e}"),
            })?;
        Ok(response.into_inner().exists)
    }
}
```

### 7.2 RepositoryService.CreateRepository

Used during `POST /api/apps` when the repo does not yet exist.

```protobuf
// repository.proto
service RepositoryService {
  rpc CreateRepository(CreateRepositoryRequest) returns (CreateRepositoryResponse) {
    option (op_type) = { op: MUTATOR };
  }
  // ...
}

// CreateRepositoryRequest creates a new repository on the Gitaly storage.
message CreateRepositoryRequest {
  // repository to create. storage_name and relative_path must be provided.
  Repository repository = 1 [(target_repository)=true];
  // default_branch is the branch name to set as default (not a fully qualified ref).
  bytes default_branch = 2;
  // object_format is the object format the repo should use. Experimental.
  ObjectFormat object_format = 3;
}

// CreateRepositoryResponse is the response for CreateRepository.
// An empty response denotes success.
message CreateRepositoryResponse {
}
```

**Rust wrapper** (in `conman-git`):

```rust
impl GitalyClient {
    /// Create a new bare repository on the Gitaly storage.
    pub async fn create_repository(
        &self,
        repo_path: &str,
        default_branch: &str,
    ) -> Result<(), ConmanError> {
        let repo = self.build_repository(repo_path);
        let request = CreateRepositoryRequest {
            repository: Some(repo),
            default_branch: default_branch.as_bytes().to_vec(),
            object_format: 0, // OBJECT_FORMAT_UNSPECIFIED, defaults to SHA1
        };
        self.repository_service()
            .create_repository(request)
            .await
            .map_err(|e| ConmanError::Git {
                message: format!("CreateRepository failed: {e}"),
            })?;
        Ok(())
    }
}
```

**Shared helper:**

```rust
impl GitalyClient {
    /// Build a gitaly Repository message from a Conman repo_path.
    fn build_repository(&self, repo_path: &str) -> Repository {
        Repository {
            storage_name: "default".to_string(),
            relative_path: repo_path.to_string(),
            gl_repository: String::new(),
            gl_project_path: String::new(),
            git_object_directory: String::new(),
            git_alternate_object_directories: vec![],
        }
    }
}
```

---

## 8. Implementation Checklist

### E03-01: App CRUD and repository registration

- [ ] Add `App`, `AppSettings`, `BaselineMode`, `CommitMode` types to `conman-core`
- [ ] Add `AppRepo` to `conman-db` with `ensure_indexes()` (unique name, unique repo_path)
- [ ] Add `repository_exists()` and `create_repository()` to `GitalyClient` in `conman-git`
- [ ] Add `POST /api/apps` handler with gitaly verification, default settings, default environments, and auto app_admin membership
- [ ] Add `GET /api/apps` handler with membership-filtered listing and pagination
- [ ] Add `GET /api/apps/:appId` handler with RBAC check
- [ ] Emit audit events for app creation
- [ ] Write unit tests for name/repo_path validation logic
- [ ] Write integration test: create app with mock gitaly, verify DB state

### E03-02: Settings API

- [ ] Add `PATCH /api/apps/:appId/settings` handler
- [ ] Validate `canonical_env_id` references an existing environment in this app
- [ ] Validate `baseline_mode` + `canonical_env_id` consistency
- [ ] Validate `file_size_limit_bytes` bounds (> 0, <= 50 MB)
- [ ] Partial update: only modify supplied fields
- [ ] Emit audit event with before/after diff
- [ ] Write unit tests for each validation rule
- [ ] Write integration test: update settings, read back, verify

### E03-03: Environment stage CRUD

- [ ] Add `Environment` type to `conman-core`
- [ ] Add `EnvironmentRepo` to `conman-db` with `ensure_indexes()` (unique app_id+name, unique app_id+position, partial unique canonical)
- [ ] Add `GET /api/apps/:appId/environments` handler
- [ ] Add `PATCH /api/apps/:appId/environments` handler (full replacement)
- [ ] Validate contiguous positions, unique names, exactly one canonical
- [ ] Block deletion of environments with active deployments
- [ ] Auto-update `app.settings.canonical_env_id` when canonical environment changes
- [ ] Emit audit events for environment changes
- [ ] Write unit tests for position/name/canonical validation
- [ ] Write integration test: create app with defaults, reorder, add, remove environments

### E03-04: Membership listing and role assignment

- [ ] Add `GET /api/apps/:appId/members` handler with pagination
- [ ] Join `app_memberships` with `users` to return email
- [ ] RBAC: any member can list; only `app_admin` can change roles (role changes handled by E02 invite flow, listing is this epic's scope)
- [ ] Write integration test: create app, add members via invite, list members

---

## 9. Test Cases

### Unit tests (conman-core)

| # | Test | Assertion |
|---|------|-----------|
| 1 | `BaselineMode::default()` | Returns `CanonicalEnvRelease` |
| 2 | `CommitMode::default()` | Returns `SubmitCommit` |
| 3 | `AppSettings::default()` blocked_paths | Contains `.git/**`, `.gitignore`, `.github/**` |
| 4 | `AppSettings::default()` file_size_limit | Equals 5242880 |
| 5 | Validate app name with special chars | Rejects names with spaces, `@`, `/` |
| 6 | Validate app name within length | Accepts 1-128 alphanumeric + hyphen + underscore |
| 7 | Validate file_size_limit_bytes = 0 | Returns Validation error |
| 8 | Validate file_size_limit_bytes > 50 MB | Returns Validation error |
| 9 | Validate baseline_mode unknown string | Returns Validation error |
| 10 | Validate environment list with duplicate names | Returns Validation error |
| 11 | Validate environment list with non-contiguous positions | Returns Validation error |
| 12 | Validate environment list with zero canonical | Returns Validation error |
| 13 | Validate environment list with two canonical | Returns Validation error |

### Integration tests

| # | Test | Setup | Assertion |
|---|------|-------|-----------|
| 14 | Create app — happy path | Mock gitaly returns `exists: true` | App created, 4 default envs, caller is app_admin, canonical_env_id set |
| 15 | Create app — repo does not exist | Mock gitaly returns `exists: false`, then `CreateRepository` succeeds | App created, repo created |
| 16 | Create app — gitaly unreachable | Mock gitaly returns UNAVAILABLE | Returns 502 `git_error` |
| 17 | Create app — duplicate name | App with same name exists | Returns 409 `conflict` |
| 18 | Create app — duplicate repo_path | App with same repo_path exists | Returns 409 `conflict` |
| 19 | Get app — member | User is member | Returns 200 with full app |
| 20 | Get app — non-member | User has no membership | Returns 403 `forbidden` |
| 21 | List apps — filtered by membership | User is member of 1 of 3 apps | Returns 1 app |
| 22 | Update settings — all fields | app_admin sends all fields | All updated, audit emitted |
| 23 | Update settings — partial | app_admin sends only `baseline_mode` | Only baseline_mode changed |
| 24 | Update settings — non-admin | `user` role attempts update | Returns 403 `forbidden` |
| 25 | Update settings — invalid canonical_env_id | References env from other app | Returns 404 `not_found` |
| 26 | List environments | App with 4 envs | Returns ordered list |
| 27 | Reorder environments | Swap positions of QA and UAT | Positions updated, names preserved |
| 28 | Add environment | Add "Staging" at position 2, shift others | 5 envs returned, positions 0-4 |
| 29 | Remove environment — no deployments | Remove UAT | 3 envs, positions re-contiguous |
| 30 | Remove environment — has deployment | Remove env with active deployment | Returns 409 `conflict` |
| 31 | Change canonical flag | Move canonical from Production to UAT | app.settings.canonical_env_id updated |
| 32 | List members | App with 2 members | Returns paginated list with roles |
| 33 | Audit trail | Create app then update settings | 2 audit events with correct entity_type and action |

---

## 10. Acceptance Criteria

- [ ] Any authenticated user can create an app and becomes its `app_admin`.
- [ ] App creation verifies repository existence via gitaly and creates it if needed.
- [ ] New apps receive default settings: `canonical_env_release` baseline mode, `submit_commit` commit mode, standard blocked paths, 5 MB file size limit.
- [ ] New apps receive a default 4-stage environment pipeline (Development, QA, UAT, Production) with Production marked as canonical.
- [ ] `app_admin` can configure baseline mode to either `integration_head` or `canonical_env_release`.
- [ ] `app_admin` can update blocked paths, file size limit, and commit mode default.
- [ ] `app_admin` can add, remove, rename, and reorder environment stages.
- [ ] Exactly one environment per app is marked canonical at all times.
- [ ] Environment removal is blocked when the environment has active deployments.
- [ ] Any app member can list environments and members.
- [ ] Runtime profiles can be created, updated, and revisioned per app.
- [ ] Each environment can be linked to a runtime profile.
- [ ] Canonical env profile approval policy is configurable
  (`same_as_changeset` or `stricter_two_approvals`).
- [ ] Runtime profile env vars are validated as typed values
  (`string|number|boolean|json`) on write.
- [ ] `app_admin` can reveal secret plaintext; other roles receive masked values
  only (length <= 8 => last 4, otherwise first 4 + last 4).
- [ ] Direct app-admin persistent profile edits are allowed, audited, and mark
  deployment drift until revalidation.
- [ ] All mutations emit audit events with before/after snapshots.
- [ ] All endpoints follow the standard response envelope and error format.
- [ ] Pagination works correctly on list endpoints.
