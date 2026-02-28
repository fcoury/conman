# E10 Temp Environments + TTL Lifecycle

## 1. Goal

Enable on-demand, isolated validation environments scoped to a workspace or
changeset. Each temp environment provisions its own database and (optionally) a
dedicated Git branch so that users can run tests, preview deployments, and
validate configuration changes without affecting shared environments. Each temp
environment is derived from a base runtime profile and gets a generated,
readable URL.

Temp environments are ephemeral by design. A 24-hour idle TTL keeps resource
usage bounded, with soft expiry and a 1-hour grace period giving users time to
recover before permanent teardown. Manual TTL extension allows intentional
prolongation of active work sessions.

Every lifecycle transition generates audit events and email notifications so
that both users and operators have full visibility into environment creation,
expiry warnings, and cleanup.

## 2. Dependencies

| Epic | What it provides |
|------|-----------------|
| E03 App Setup | `App`, `Environment` domain types, app settings, `AppRepo` |
| E06 Async Jobs | Job framework, `JobRepo`, worker registration, job state machine |

Optional runtime dependency on E01 (Git Adapter) for creating temp env
branches via gitaly-rs. The temp env provisioning worker calls
`OperationService.UserCreateBranch` when the temp env kind is `Workspace` and
needs a snapshot branch.

## 3. Rust Types

### 3.1 TempEnvKind (`conman-core/src/temp_env.rs`)

Discriminates whether the temp environment was created from a workspace or a
changeset. Determines which source state is cloned into the isolated
environment.

```rust
use serde::{Deserialize, Serialize};

/// The source kind that a temporary environment was created from.
///
/// `Workspace` envs snapshot the current workspace branch HEAD.
/// `Changeset` envs snapshot the changeset's head_sha at creation time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TempEnvKind {
    /// Created from a workspace branch snapshot.
    Workspace,
    /// Created from a changeset head_sha snapshot.
    Changeset,
}

impl std::fmt::Display for TempEnvKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Workspace => write!(f, "workspace"),
            Self::Changeset => write!(f, "changeset"),
        }
    }
}
```

### 3.2 TempEnvState (`conman-core/src/temp_env.rs`)

State machine governing the temp environment lifecycle from initial
provisioning through expiry and final deletion.

```rust
/// Lifecycle state of a temporary environment.
///
/// State transitions:
///   Provisioning -> Active          (provision job succeeds)
///   Provisioning -> Deleted         (provision job fails, nothing to clean up)
///   Active       -> Expiring        (TTL exceeded, expiry worker sets grace window)
///   Expiring     -> Active          (user calls undo-expire during grace)
///   Expiring     -> Expired         (grace period ends without undo)
///   Active       -> Deleted         (user explicitly deletes)
///   Expired      -> Deleted         (cleanup worker tears down resources)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TempEnvState {
    /// Provisioning job is running (DB creation, branch snapshot).
    Provisioning,
    /// Environment is live and usable.
    Active,
    /// TTL exceeded; grace period is running. User can undo-expire.
    Expiring,
    /// Grace period ended. Awaiting cleanup worker teardown.
    Expired,
    /// All resources torn down. Terminal state.
    Deleted,
}

impl std::fmt::Display for TempEnvState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Provisioning => write!(f, "provisioning"),
            Self::Active => write!(f, "active"),
            Self::Expiring => write!(f, "expiring"),
            Self::Expired => write!(f, "expired"),
            Self::Deleted => write!(f, "deleted"),
        }
    }
}

impl TempEnvState {
    /// Validate that a state transition is allowed.
    ///
    /// Returns `Ok(())` if the transition is valid, or
    /// `Err(ConmanError::InvalidTransition)` otherwise.
    pub fn validate_transition(&self, to: TempEnvState) -> Result<(), ConmanError> {
        let valid = matches!(
            (self, to),
            (Self::Provisioning, Self::Active)
                | (Self::Provisioning, Self::Deleted)
                | (Self::Active, Self::Expiring)
                | (Self::Active, Self::Deleted)
                | (Self::Expiring, Self::Active)
                | (Self::Expiring, Self::Expired)
                | (Self::Expired, Self::Deleted)
        );

        if valid {
            Ok(())
        } else {
            Err(ConmanError::InvalidTransition {
                from: self.to_string(),
                to: to.to_string(),
            })
        }
    }
}
```

### 3.3 TempEnvironment (`conman-core/src/temp_env.rs`)

The domain struct stored in MongoDB representing a single temp environment
instance and its full lifecycle metadata.

```rust
use bson::oid::ObjectId;
use chrono::{DateTime, Utc};

/// A temporary, isolated environment created on-demand for validating
/// workspace or changeset state.
///
/// Each temp environment gets its own database (named `db_name`) and
/// optionally a snapshot branch in the Git repository. The environment
/// is subject to a 24h idle TTL measured from `last_activity_at`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempEnvironment {
    /// Unique identifier (MongoDB ObjectId).
    #[serde(rename = "_id")]
    pub id: ObjectId,

    /// The app this temp environment belongs to.
    pub repo_id: ObjectId,

    /// Whether this environment was created from a workspace or changeset.
    pub kind: TempEnvKind,

    /// Source workspace ID. Set when `kind == Workspace`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<ObjectId>,

    /// Source changeset ID. Set when `kind == Changeset`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changeset_id: Option<ObjectId>,

    /// Base runtime profile used for derivation.
    pub base_runtime_profile_id: ObjectId,

    /// Effective runtime profile for this temp environment.
    pub runtime_profile_id: ObjectId,

    /// Name of the isolated database provisioned for this environment.
    /// Format: `conman_temp_{app_id_short}_{id_short}`.
    pub db_name: String,

    /// Generated shareable URL for this temp environment.
    /// Format: `{app}-{kind}-{word}.<domain>`.
    pub base_url: String,

    /// Current lifecycle state.
    pub state: TempEnvState,

    /// Timestamp of the most recent activity (API call, test run, or
    /// deployment) that touched this environment. TTL is measured from
    /// this timestamp.
    pub last_activity_at: DateTime<Utc>,

    /// Absolute expiry time. Computed as `last_activity_at + 24h`.
    /// Updated whenever `last_activity_at` changes or a manual extension
    /// is requested.
    pub expires_at: DateTime<Utc>,

    /// End of the grace period after soft expiry. Set to `now + 1h` when
    /// the environment transitions to `Expiring`. `None` when not in
    /// grace period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grace_until: Option<DateTime<Utc>>,

    /// User who created this temp environment.
    pub created_by: ObjectId,

    /// When this record was created.
    pub created_at: DateTime<Utc>,

    /// When this record was last modified.
    pub updated_at: DateTime<Utc>,
}
```

### 3.4 API Request/Response Types (`conman-api/src/handlers/temp_envs.rs`)

Request and response DTOs for the temp environment endpoints. These are
API-facing types, distinct from the domain `TempEnvironment` struct.

```rust
use serde::{Deserialize, Serialize};

/// Request body for `POST /api/repos/:repoId/temp-envs`.
///
/// Exactly one of `workspace_id` or `changeset_id` must be provided.
/// The `kind` field is inferred from which ID is present.
#[derive(Debug, Deserialize)]
pub struct CreateTempEnvRequest {
    /// Source workspace to snapshot. Mutually exclusive with `changeset_id`.
    #[serde(default)]
    pub workspace_id: Option<String>,

    /// Source changeset to snapshot. Mutually exclusive with `workspace_id`.
    #[serde(default)]
    pub changeset_id: Option<String>,

    /// Optional base runtime profile. If omitted, app default is used
    /// (Development or app-defined special base profile).
    #[serde(default)]
    pub base_runtime_profile_id: Option<String>,

    /// Optional typed env var overrides applied on top of derived runtime profile.
    /// Uses the same `EnvVarValue` enum defined in E03.
    #[serde(default)]
    pub env_var_overrides: std::collections::BTreeMap<String, EnvVarValue>,
}

/// Response body for temp environment endpoints.
///
/// Serialized inside the standard `ApiResponse<TempEnvResponse>` envelope.
#[derive(Debug, Serialize)]
pub struct TempEnvResponse {
    pub id: String,
    pub repo_id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changeset_id: Option<String>,
    pub base_runtime_profile_id: String,
    pub runtime_profile_id: String,
    pub db_name: String,
    pub base_url: String,
    pub state: String,
    pub last_activity_at: String,
    pub expires_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grace_until: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Request body for `POST /api/repos/:repoId/temp-envs/:tempEnvId/extend`.
///
/// Allows the user to add additional hours to the current TTL.
/// The extension is capped at 72h total from the original creation time
/// to prevent environments from living indefinitely.
#[derive(Debug, Deserialize)]
pub struct ExtendTtlRequest {
    /// Number of hours to add to the current `expires_at`.
    /// Must be between 1 and 48. Defaults to 24 if omitted.
    #[serde(default = "default_extend_hours")]
    pub hours: u64,
}

fn default_extend_hours() -> u64 {
    24
}
```

### 3.5 Conversion (`conman-api/src/handlers/temp_envs.rs`)

```rust
impl From<TempEnvironment> for TempEnvResponse {
    fn from(env: TempEnvironment) -> Self {
        Self {
            id: env.id.to_hex(),
            repo_id: env.repo_id.to_hex(),
            kind: env.kind.to_string(),
            workspace_id: env.workspace_id.map(|id| id.to_hex()),
            changeset_id: env.changeset_id.map(|id| id.to_hex()),
            base_runtime_profile_id: env.base_runtime_profile_id.to_hex(),
            runtime_profile_id: env.runtime_profile_id.to_hex(),
            db_name: env.db_name,
            base_url: env.base_url,
            state: env.state.to_string(),
            last_activity_at: env.last_activity_at.to_rfc3339(),
            expires_at: env.expires_at.to_rfc3339(),
            grace_until: env.grace_until.map(|dt| dt.to_rfc3339()),
            created_by: env.created_by.to_hex(),
            created_at: env.created_at.to_rfc3339(),
            updated_at: env.updated_at.to_rfc3339(),
        }
    }
}
```

## 4. Database

### 4.1 Collection: `temp_environments`

Stores one document per temp environment instance.

| Field | BSON Type | Description |
|-------|-----------|-------------|
| `_id` | ObjectId | Primary key |
| `repo_id` | ObjectId | Parent app |
| `kind` | String | `"workspace"` or `"changeset"` |
| `workspace_id` | ObjectId / null | Source workspace (when kind = workspace) |
| `changeset_id` | ObjectId / null | Source changeset (when kind = changeset) |
| `base_runtime_profile_id` | ObjectId | Base runtime profile for derivation |
| `runtime_profile_id` | ObjectId | Effective temp runtime profile |
| `db_name` | String | Isolated database name for this environment |
| `base_url` | String | Generated URL (`{app}-{kind}-{word}.<domain>`) |
| `state` | String | One of: `provisioning`, `active`, `expiring`, `expired`, `deleted` |
| `last_activity_at` | DateTime | Last activity timestamp (TTL anchor) |
| `expires_at` | DateTime | Absolute expiry time |
| `grace_until` | DateTime / null | Grace period end (set during `expiring` state) |
| `created_by` | ObjectId | User who requested creation |
| `created_at` | DateTime | Document creation time |
| `updated_at` | DateTime | Last modification time |

### 4.2 Indexes

```rust
use mongodb::{bson::doc, IndexModel, options::IndexOptions};

impl TempEnvRepo {
    pub async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let indexes = vec![
            // Expiry worker query: find environments whose TTL has passed.
            // Worker queries: { state: "active", expires_at: { $lte: now } }
            IndexModel::builder()
                .keys(doc! { "state": 1, "expires_at": 1 })
                .options(
                    IndexOptions::builder()
                        .name("idx_state_expires_at".to_string())
                        .build(),
                )
                .build(),

            // Grace period worker: find expiring environments past grace window.
            // Worker queries: { state: "expiring", grace_until: { $lte: now } }
            IndexModel::builder()
                .keys(doc! { "state": 1, "grace_until": 1 })
                .options(
                    IndexOptions::builder()
                        .name("idx_state_grace_until".to_string())
                        .build(),
                )
                .build(),

            // List by app with pagination.
            IndexModel::builder()
                .keys(doc! { "repo_id": 1, "created_at": -1 })
                .options(
                    IndexOptions::builder()
                        .name("idx_app_id_created_at".to_string())
                        .build(),
                )
                .build(),

            // Lookup by source workspace (uniqueness check: one active temp
            // env per workspace).
            IndexModel::builder()
                .keys(doc! { "workspace_id": 1, "state": 1 })
                .options(
                    IndexOptions::builder()
                        .name("idx_workspace_id_state".to_string())
                        .build(),
                )
                .build(),

            // Lookup by source changeset (uniqueness check: one active temp
            // env per changeset).
            IndexModel::builder()
                .keys(doc! { "changeset_id": 1, "state": 1 })
                .options(
                    IndexOptions::builder()
                        .name("idx_changeset_id_state".to_string())
                        .build(),
                )
                .build(),

            // Cleanup worker: find expired environments awaiting teardown.
            // Worker queries: { state: "expired" }
            IndexModel::builder()
                .keys(doc! { "state": 1 })
                .options(
                    IndexOptions::builder()
                        .name("idx_state".to_string())
                        .build(),
                )
                .build(),
        ];

        self.collection.create_indexes(indexes).await.map_err(|e| {
            ConmanError::Internal {
                message: format!("failed to create temp_environments indexes: {e}"),
            }
        })?;

        Ok(())
    }
}
```

### 4.3 Example Documents

**Active workspace temp environment:**

```json
{
  "_id": { "$oid": "665a1b2c3d4e5f6a7b8c9d0e" },
  "repo_id": { "$oid": "664f0a1b2c3d4e5f6a7b8c9d" },
  "kind": "workspace",
  "workspace_id": { "$oid": "665012ab3c4d5e6f7a8b9c0d" },
  "changeset_id": null,
  "db_name": "conman_temp_664f0a_665a1b",
  "state": "active",
  "last_activity_at": { "$date": "2026-02-25T14:30:00Z" },
  "expires_at": { "$date": "2026-02-26T14:30:00Z" },
  "grace_until": null,
  "created_by": { "$oid": "664e0f1a2b3c4d5e6f7a8b9c" },
  "created_at": { "$date": "2026-02-25T10:00:00Z" },
  "updated_at": { "$date": "2026-02-25T14:30:00Z" }
}
```

**Expiring changeset temp environment (in grace period):**

```json
{
  "_id": { "$oid": "665b2c3d4e5f6a7b8c9d0e1f" },
  "repo_id": { "$oid": "664f0a1b2c3d4e5f6a7b8c9d" },
  "kind": "changeset",
  "workspace_id": null,
  "changeset_id": { "$oid": "665123bc4d5e6f7a8b9c0d1e" },
  "db_name": "conman_temp_664f0a_665b2c",
  "state": "expiring",
  "last_activity_at": { "$date": "2026-02-24T08:00:00Z" },
  "expires_at": { "$date": "2026-02-25T08:00:00Z" },
  "grace_until": { "$date": "2026-02-25T09:00:00Z" },
  "created_by": { "$oid": "664e0f1a2b3c4d5e6f7a8b9c" },
  "created_at": { "$date": "2026-02-24T06:00:00Z" },
  "updated_at": { "$date": "2026-02-25T08:00:00Z" }
}
```

## 5. API Endpoints

### 5.1 Create Temp Environment

```
POST /api/repos/:repoId/temp-envs
```

**Auth:** Any role (`user`, `reviewer`, `config_manager`, `admin`).

**Request body:**

```json
{
  "workspace_id": "665012ab3c4d5e6f7a8b9c0d"
}
```

or:

```json
{
  "changeset_id": "665123bc4d5e6f7a8b9c0d1e"
}
```

**Validation:**
- Exactly one of `workspace_id` or `changeset_id` must be set.
- The referenced workspace/changeset must exist and belong to the given app.
- No active (non-deleted, non-expired) temp environment may already exist for
  the same workspace or changeset.

**Response 201:**

```json
{
  "data": {
    "id": "665a1b2c3d4e5f6a7b8c9d0e",
    "repo_id": "664f0a1b2c3d4e5f6a7b8c9d",
    "kind": "workspace",
    "workspace_id": "665012ab3c4d5e6f7a8b9c0d",
    "db_name": "conman_temp_664f0a_665a1b",
    "state": "provisioning",
    "last_activity_at": "2026-02-25T10:00:00Z",
    "expires_at": "2026-02-26T10:00:00Z",
    "created_by": "664e0f1a2b3c4d5e6f7a8b9c",
    "created_at": "2026-02-25T10:00:00Z",
    "updated_at": "2026-02-25T10:00:00Z"
  }
}
```

**Side effects:**
- Inserts document with `state: provisioning`.
- Enqueues a `temp_env_provision` job.
- Emits `temp_env.created` audit event.

**Errors:**
- `400` if both or neither of `workspace_id`/`changeset_id` are provided.
- `404` if the referenced workspace or changeset does not exist.
- `409` if an active temp environment already exists for the source.

### 5.2 List Temp Environments

```
GET /api/repos/:repoId/temp-envs?page=&limit=
```

**Auth:** Any role.

**Query params:** Standard pagination (`page`, `limit`).

**Response 200:**

```json
{
  "data": [
    {
      "id": "665a1b2c3d4e5f6a7b8c9d0e",
      "repo_id": "664f0a1b2c3d4e5f6a7b8c9d",
      "kind": "workspace",
      "workspace_id": "665012ab3c4d5e6f7a8b9c0d",
      "db_name": "conman_temp_664f0a_665a1b",
      "state": "active",
      "last_activity_at": "2026-02-25T14:30:00Z",
      "expires_at": "2026-02-26T14:30:00Z",
      "created_by": "664e0f1a2b3c4d5e6f7a8b9c",
      "created_at": "2026-02-25T10:00:00Z",
      "updated_at": "2026-02-25T14:30:00Z"
    }
  ],
  "pagination": { "page": 1, "limit": 20, "total": 1 }
}
```

**Behavior:**
- Returns all non-deleted temp environments for the app, sorted by
  `created_at` descending.
- Deleted environments are excluded from listing by default.

### 5.3 Extend TTL

```
POST /api/repos/:repoId/temp-envs/:tempEnvId/extend
```

**Auth:** Any role. The user must be the creator of the temp environment, or
hold `config_manager`/`admin` role on the app.

**Request body:**

```json
{
  "hours": 24
}
```

**Validation:**
- `hours` must be between 1 and 48.
- Temp environment must be in `Active` state.
- Total lifetime (from `created_at` to new `expires_at`) must not exceed 72h.

**Response 200:**

```json
{
  "data": {
    "id": "665a1b2c3d4e5f6a7b8c9d0e",
    "state": "active",
    "expires_at": "2026-02-27T14:30:00Z",
    "updated_at": "2026-02-25T15:00:00Z"
  }
}
```

**Side effects:**
- Updates `expires_at` and `updated_at`.
- Emits `temp_env.ttl_extended` audit event.

**Errors:**
- `400` if `hours` is out of range or total lifetime would exceed 72h.
- `409` if temp environment is not in `Active` state.

### 5.4 Undo Expire

```
POST /api/repos/:repoId/temp-envs/:tempEnvId/undo-expire
```

**Auth:** Same as extend (creator, `config_manager`, or `admin`).

**Validation:**
- Temp environment must be in `Expiring` state.
- Current time must be before `grace_until`.

**Response 200:**

```json
{
  "data": {
    "id": "665a1b2c3d4e5f6a7b8c9d0e",
    "state": "active",
    "expires_at": "2026-02-26T15:00:00Z",
    "grace_until": null,
    "updated_at": "2026-02-25T15:00:00Z"
  }
}
```

**Side effects:**
- Transitions state from `Expiring` to `Active`.
- Sets `last_activity_at` to now.
- Recalculates `expires_at` as `now + 24h`.
- Clears `grace_until`.
- Emits `temp_env.undo_expired` audit event.

**Errors:**
- `409` if temp environment is not in `Expiring` state.
- `410` (Gone) if `grace_until` has already passed.

### 5.5 Delete Temp Environment

```
DELETE /api/repos/:repoId/temp-envs/:tempEnvId
```

**Auth:** Creator, `config_manager`, or `admin`.

**Validation:**
- Temp environment must be in `Active` or `Expiring` state. Already `Expired`
  or `Deleted` environments return a `409`.

**Response 204:** No body.

**Side effects:**
- Transitions state directly to `Deleted`.
- Enqueues a `temp_env_cleanup` job to tear down the database.
- Emits `temp_env.deleted` audit event.

**Errors:**
- `409` if temp environment is in `Expired`, `Deleted`, or `Provisioning` state.

## 6. Business Logic

### 6.1 Creation and Provisioning

When a user requests a temp environment, the handler performs synchronous
validation and inserts a document in `Provisioning` state. A
`temp_env_provision` async job is enqueued to perform the heavy lifting:

1. **Generate database name:** `conman_temp_{repo_id[0..6]}_{temp_env_id[0..6]}`.
2. **Create the isolated MongoDB database** by writing an init document (the
   MongoDB driver creates databases lazily on first write).
3. **If workspace kind:** Copy the workspace's current file tree into the
   temp database. Optionally create a snapshot branch via
   `OperationService.UserCreateBranch` at the workspace HEAD so that the
   temp env has a stable Git ref.
4. **If changeset kind:** Copy the changeset's `head_sha` state into the
   temp database.
5. **On success:** Transition state to `Active`, set `last_activity_at` and
   `expires_at`.
6. **On failure:** Transition state to `Deleted`, log the error, and emit a
   `temp_env.provision_failed` audit event.

### 6.2 Activity Tracking

Any API call, test run, or deployment event that touches a temp environment
updates `last_activity_at` and recalculates `expires_at`:

```rust
/// Update the activity timestamp on a temp environment.
///
/// Recalculates `expires_at` as `last_activity_at + TTL_DURATION`.
/// Only updates if the environment is in `Active` state.
pub async fn touch_activity(
    &self,
    temp_env_id: ObjectId,
) -> Result<(), ConmanError> {
    let now = Utc::now();
    let new_expires = now + TTL_DURATION;

    let result = self.collection.update_one(
        doc! {
            "_id": temp_env_id,
            "state": "active",
        },
        doc! {
            "$set": {
                "last_activity_at": now,
                "expires_at": new_expires,
                "updated_at": now,
            }
        },
    ).await?;

    if result.matched_count == 0 {
        return Err(ConmanError::NotFound {
            entity: "temp_environment",
            id: temp_env_id.to_hex(),
        });
    }

    Ok(())
}
```

The following actions trigger an activity touch:
- Any API request scoped to the temp environment.
- A test run (`msuite_deploy` or `msuite_submit` job) targeting the temp env.
- A deployment event targeting the temp env.

### 6.3 TTL and Expiry Worker

The **expiry worker** runs on a periodic schedule (every 5 minutes) and
performs two passes:

**Pass 1 -- Soft expiry:** Find `Active` environments whose `expires_at`
has passed.

```rust
// Find active environments past their TTL.
let filter = doc! {
    "state": "active",
    "expires_at": { "$lte": Utc::now() },
};
```

For each match:
1. Transition state to `Expiring`.
2. Set `grace_until = now + 1h`.
3. Send expiry notification to the creator.
4. Emit `temp_env.expiring` audit event.

**Pass 2 -- Grace expiry:** Find `Expiring` environments whose `grace_until`
has passed.

```rust
// Find expiring environments past their grace window.
let filter = doc! {
    "state": "expiring",
    "grace_until": { "$lte": Utc::now() },
};
```

For each match:
1. Transition state to `Expired`.
2. Enqueue a `temp_env_cleanup` job.
3. Send grace-ended notification to the creator.
4. Emit `temp_env.expired` audit event.

### 6.4 Soft Expiry and Grace Period

When a temp environment enters the `Expiring` state, the user has a 1-hour
window to call the `undo-expire` endpoint. During this window:

- The environment's database and resources remain intact.
- API calls to the environment still work (and each one triggers an
  activity touch, but the touch only applies to `Active` environments, so
  it does not auto-restore from `Expiring`).
- The undo-expire endpoint explicitly transitions back to `Active`,
  recalculates `expires_at`, and clears `grace_until`.

If the grace period expires without an undo, the environment moves to
`Expired` and becomes read-only pending cleanup.

### 6.5 Manual TTL Extension

The extend endpoint allows proactive TTL management before expiry:

```rust
/// Extend the TTL of an active temp environment.
///
/// Adds `hours` to the current `expires_at`, subject to a maximum
/// total lifetime of 72h from `created_at`.
pub fn extend_ttl(
    env: &TempEnvironment,
    hours: u64,
) -> Result<DateTime<Utc>, ConmanError> {
    // Guard: must be active.
    if env.state != TempEnvState::Active {
        return Err(ConmanError::InvalidTransition {
            from: env.state.to_string(),
            to: "active (extended)".to_string(),
        });
    }

    let extension = chrono::Duration::hours(hours as i64);
    let new_expires = env.expires_at + extension;

    // Cap total lifetime at 72h from creation.
    let max_expires = env.created_at + chrono::Duration::hours(MAX_LIFETIME_HOURS);
    if new_expires > max_expires {
        return Err(ConmanError::Validation {
            message: format!(
                "extension would exceed maximum lifetime of {MAX_LIFETIME_HOURS}h; \
                 max expires_at is {}",
                max_expires.to_rfc3339()
            ),
        });
    }

    Ok(new_expires)
}

const TTL_DURATION: chrono::Duration = chrono::Duration::hours(24);
const GRACE_DURATION: chrono::Duration = chrono::Duration::hours(1);
const MAX_LIFETIME_HOURS: i64 = 72;
```

### 6.6 Cleanup Worker

The **cleanup worker** processes `temp_env_cleanup` jobs. For each expired
or explicitly deleted environment:

1. **Drop the isolated MongoDB database** (`db_name`).
2. **Delete the snapshot Git branch** (if one was created during provisioning)
   via `OperationService.UserDeleteBranch`.
3. **Transition state to `Deleted`.**
4. Emit `temp_env.cleaned_up` audit event.

If cleanup fails (e.g., database unreachable), the job is retried with
exponential backoff (3 attempts). After final failure, the job is marked
`failed` and an alert is logged for operator intervention.

### 6.7 Notifications

| Timing | Event | Recipients |
|--------|-------|-----------|
| 1h before `expires_at` | Expiry warning | Creator |
| At soft expiry | Environment expiring (grace started) | Creator |
| At grace end | Environment expired | Creator |
| On explicit delete | Environment deleted | Creator |
| On provision failure | Provisioning failed | Creator |

Notifications follow the per-user toggle defined in E11. If the user has
notifications disabled, the events are still written to the audit log but
no email is sent.

The 1h-before-expiry warning is handled by a third pass in the expiry worker:

```rust
// Find active environments expiring within the next hour that have
// not yet received a warning notification.
let warning_threshold = Utc::now() + chrono::Duration::hours(1);
let filter = doc! {
    "state": "active",
    "expires_at": { "$lte": warning_threshold },
    "expiry_warning_sent": { "$ne": true },
};
```

After sending the warning, the worker sets a `expiry_warning_sent: true`
flag on the document to avoid duplicate notifications.

## 7. Gitaly-rs Integration

### 7.1 OperationService.UserCreateBranch

Used during temp env provisioning to create a snapshot branch from the
workspace HEAD. This gives the temp environment a stable Git ref that
will not move as the user continues editing the workspace.

**Proto definition** (from `gitaly/proto/operations.proto`):

```protobuf
service OperationService {
  rpc UserCreateBranch(UserCreateBranchRequest) returns (UserCreateBranchResponse) {
    option (op_type) = {
      op: MUTATOR
    };
  }
}

message UserCreateBranchRequest {
  // Repository in which the branch should be created.
  Repository repository = 1 [(target_repository)=true];
  // Name of the branch to create.
  bytes branch_name = 2;
  // User to execute the action as.
  User user = 3;
  // Git revision to start the branch at (e.g., a commit SHA).
  bytes start_point = 4;
}

message UserCreateBranchResponse {
  // The branch that was created.
  Branch branch = 1;
}
```

**Conman usage in the provisioning worker:**

```rust
/// Create a snapshot branch for a workspace temp environment.
///
/// Branch naming convention: `temp/<temp_env_id>/<workspace_branch_name>`
/// The start_point is the workspace's current HEAD SHA.
pub async fn create_temp_env_branch(
    &self,
    app: &App,
    temp_env_id: &ObjectId,
    workspace_branch: &str,
    head_sha: &str,
    user: &conman_core::User,
) -> Result<String, ConmanError> {
    let repo = app_to_gitaly_repo(app);
    let branch_name = format!("temp/{}/{}", temp_env_id.to_hex(), workspace_branch);

    let request = UserCreateBranchRequest {
        repository: Some(repo),
        branch_name: branch_name.as_bytes().to_vec(),
        user: Some(domain_user_to_gitaly_user(user)),
        start_point: head_sha.as_bytes().to_vec(),
    };

    let response = self
        .operation_service()
        .user_create_branch(request)
        .await
        .map_err(|e| ConmanError::Git {
            message: format!("failed to create temp env branch: {e}"),
        })?;

    let branch = response
        .into_inner()
        .branch
        .ok_or_else(|| ConmanError::Git {
            message: "UserCreateBranch returned no branch".to_string(),
        })?;

    let commit_id = branch
        .target_commit
        .map(|c| c.id)
        .unwrap_or_default();

    tracing::info!(
        temp_env_id = %temp_env_id,
        branch = %branch_name,
        commit = %commit_id,
        "temp env snapshot branch created"
    );

    Ok(branch_name)
}
```

### 7.2 OperationService.UserDeleteBranch

Used during temp env cleanup to remove the snapshot branch after the
environment is torn down.

**Proto definition** (from `gitaly/proto/operations.proto`):

```protobuf
message UserDeleteBranchRequest {
  // Repository to delete the branch in.
  Repository repository = 1 [(target_repository)=true];
  // Name of the branch to delete (e.g., "temp/665a1b/ws/alice/myapp").
  bytes branch_name = 2;
  // User to execute the action as.
  User user = 3;
  // Optional: expected OID for safe deletion.
  string expected_old_oid = 4;
}

message UserDeleteBranchResponse {
  // Empty on success.
}
```

**Conman usage in the cleanup worker:**

```rust
/// Delete the snapshot branch for a temp environment during cleanup.
///
/// Ignores "branch not found" errors since the branch may have already
/// been cleaned up by a previous attempt.
pub async fn delete_temp_env_branch(
    &self,
    app: &App,
    branch_name: &str,
    user: &conman_core::User,
) -> Result<(), ConmanError> {
    let repo = app_to_gitaly_repo(app);

    let request = UserDeleteBranchRequest {
        repository: Some(repo),
        branch_name: branch_name.as_bytes().to_vec(),
        user: Some(domain_user_to_gitaly_user(user)),
        expected_old_oid: String::new(),
    };

    match self
        .operation_service()
        .user_delete_branch(request)
        .await
    {
        Ok(_) => {
            tracing::info!(branch = %branch_name, "temp env branch deleted");
            Ok(())
        }
        Err(status) if status.code() == tonic::Code::FailedPrecondition => {
            // Branch does not exist -- idempotent, not an error.
            tracing::debug!(branch = %branch_name, "temp env branch already deleted");
            Ok(())
        }
        Err(e) => Err(ConmanError::Git {
            message: format!("failed to delete temp env branch: {e}"),
        }),
    }
}
```

### 7.3 Supporting Proto Types

From `gitaly/proto/shared.proto`:

```protobuf
message Repository {
  string storage_name = 2;
  string relative_path = 3;
  string git_object_directory = 4;
  repeated string git_alternate_object_directories = 5;
  string gl_repository = 6;
  string gl_project_path = 8;
}

message User {
  string gl_id = 1;
  bytes name = 2;
  bytes email = 3;
  string gl_username = 4;
  string timezone = 5;
}

message Branch {
  bytes name = 1;
  GitCommit target_commit = 2;
}

message GitCommit {
  string id = 1;
  bytes subject = 2;
  bytes body = 3;
  CommitAuthor author = 4;
  CommitAuthor committer = 5;
  repeated string parent_ids = 6;
  int64 body_size = 7;
  SignatureType signature_type = 8;
  string tree_id = 9;
  repeated CommitTrailer trailers = 10;
}
```

## 8. Implementation Checklist

Each step is one commit. Follow TDD: write test, run test (fails), implement,
run test (passes), commit.

- [ ] **E10-S01** -- Define domain types in `conman-core`.
  Add `temp_env.rs` with `TempEnvKind`, `TempEnvState`, `TempEnvironment`.
  Write unit tests for `TempEnvState::validate_transition` covering all valid
  and invalid transitions. Write `Display` tests for both enums.

- [ ] **E10-S02** -- Implement `TempEnvRepo` in `conman-db`.
  Add `temp_env_repo.rs` with CRUD methods: `insert`, `find_by_id`,
  `find_by_app_id` (paginated), `update_state`, `touch_activity`,
  `set_grace_until`, `extend_expires_at`, `set_expiry_warning_sent`.
  Implement `ensure_indexes`. Write integration tests against a local MongoDB.

- [ ] **E10-S03** -- Implement API request/response types in `conman-api`.
  Add `CreateTempEnvRequest`, `ExtendTtlRequest`, `TempEnvResponse` and the
  `From<TempEnvironment>` conversion. Write unit tests for serialization and
  validation (e.g., mutually exclusive workspace_id/changeset_id).

- [ ] **E10-S04** -- Implement `POST /api/repos/:repoId/temp-envs` handler.
  Validate input, check for existing active temp env, insert document, enqueue
  `temp_env_provision` job, emit audit event, return 201.
  Write integration tests: happy path (workspace), happy path (changeset),
  missing both IDs (400), duplicate active env (409), nonexistent source (404).

- [ ] **E10-S05** -- Implement `GET /api/repos/:repoId/temp-envs` handler.
  Query with pagination, exclude deleted. Write integration tests: empty list,
  populated list, pagination boundaries.

- [ ] **E10-S06** -- Implement `POST .../extend` handler.
  Validate hours range, check lifetime cap, update `expires_at`, emit audit.
  Write integration tests: happy extension, exceed lifetime cap (400),
  non-active state (409), out-of-range hours (400).

- [ ] **E10-S07** -- Implement `POST .../undo-expire` handler.
  Check `Expiring` state, verify grace window, transition to `Active`,
  recalculate TTL, clear `grace_until`, emit audit.
  Write integration tests: happy undo, not in expiring state (409),
  grace already passed (410).

- [ ] **E10-S08** -- Implement `DELETE .../temp-envs/:tempEnvId` handler.
  Validate state, transition to `Deleted`, enqueue cleanup job, emit audit.
  Write integration tests: delete active (204), delete expiring (204),
  delete already deleted (409).

- [ ] **E10-S09** -- Implement `temp_env_provision` worker in `conman-jobs`.
  Generate `db_name`, create isolated DB, optionally create snapshot branch
  via gitaly, transition state to `Active`. On failure, transition to
  `Deleted`.
  Write integration test with mock gitaly: successful provision, failed
  provision (branch creation error).

- [ ] **E10-S10** -- Implement expiry worker in `conman-jobs`.
  Periodic task (5-min interval) with three passes: warning notifications,
  soft expiry (`Active` -> `Expiring`), grace expiry (`Expiring` -> `Expired`).
  Write integration tests: active env past TTL transitions to expiring,
  expiring env past grace transitions to expired, warning sent flag prevents
  duplicate notifications.

- [ ] **E10-S11** -- Implement `temp_env_cleanup` worker in `conman-jobs`.
  Drop isolated database, delete snapshot branch via gitaly, transition
  state to `Deleted`.
  Write integration test with mock gitaly: successful cleanup, idempotent
  retry when branch already deleted.

- [ ] **E10-S12** -- Wire activity tracking middleware/hooks.
  Add `touch_activity` calls in relevant API handlers and job completions
  that target a temp environment. Write integration test: API call to temp
  env updates `last_activity_at` and `expires_at`.

- [ ] **E10-S13** -- Register routes in the main router.
  Replace the E10 stub routes in `build_router` with the real handlers.
  Verify all five endpoints respond correctly in an end-to-end smoke test.

## 9. Test Cases

### 9.1 State machine transitions

```rust
#[test]
fn valid_transitions_succeed() {
    use TempEnvState::*;

    let valid = vec![
        (Provisioning, Active),
        (Provisioning, Deleted),
        (Active, Expiring),
        (Active, Deleted),
        (Expiring, Active),
        (Expiring, Expired),
        (Expired, Deleted),
    ];

    for (from, to) in valid {
        assert!(
            from.validate_transition(to).is_ok(),
            "expected {from} -> {to} to be valid"
        );
    }
}

#[test]
fn invalid_transitions_fail() {
    use TempEnvState::*;

    let invalid = vec![
        (Provisioning, Expiring),
        (Provisioning, Expired),
        (Active, Provisioning),
        (Active, Expired),
        (Expiring, Provisioning),
        (Expiring, Deleted),
        (Expired, Active),
        (Expired, Expiring),
        (Deleted, Active),
        (Deleted, Provisioning),
    ];

    for (from, to) in invalid {
        assert!(
            from.validate_transition(to).is_err(),
            "expected {from} -> {to} to be invalid"
        );
    }
}
```

### 9.2 Create temp env -- workspace happy path

```rust
#[tokio::test]
async fn create_workspace_temp_env() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/repos/664f0a1b2c3d4e5f6a7b8c9d/temp-envs")
                .header("content-type", "application/json")
                .header("authorization", "Bearer <valid_token>")
                .body(Body::from(r#"{"workspace_id":"665012ab3c4d5e6f7a8b9c0d"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body: serde_json::Value = parse_body(response).await;
    assert_eq!(body["data"]["kind"], "workspace");
    assert_eq!(body["data"]["state"], "provisioning");
    assert!(body["data"]["workspace_id"].is_string());
    assert!(body["data"]["changeset_id"].is_null());
}
```

### 9.3 Create temp env -- rejects duplicate active env

```rust
#[tokio::test]
async fn create_temp_env_rejects_duplicate() {
    let app = test_app_with_active_temp_env().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/repos/664f0a1b2c3d4e5f6a7b8c9d/temp-envs")
                .header("content-type", "application/json")
                .header("authorization", "Bearer <valid_token>")
                .body(Body::from(r#"{"workspace_id":"665012ab3c4d5e6f7a8b9c0d"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);

    let body: serde_json::Value = parse_body(response).await;
    assert_eq!(body["error"]["code"], "conflict");
}
```

### 9.4 Create temp env -- rejects both IDs set

```rust
#[tokio::test]
async fn create_temp_env_rejects_both_ids() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/repos/664f0a1b2c3d4e5f6a7b8c9d/temp-envs")
                .header("content-type", "application/json")
                .header("authorization", "Bearer <valid_token>")
                .body(Body::from(
                    r#"{"workspace_id":"aaa","changeset_id":"bbb"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
```

### 9.5 Extend TTL -- happy path

```rust
#[tokio::test]
async fn extend_ttl_succeeds() {
    let (app, temp_env_id) = test_app_with_active_temp_env().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!(
                    "/api/repos/664f0a1b2c3d4e5f6a7b8c9d/temp-envs/{temp_env_id}/extend"
                ))
                .header("content-type", "application/json")
                .header("authorization", "Bearer <valid_token>")
                .body(Body::from(r#"{"hours": 12}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = parse_body(response).await;
    assert_eq!(body["data"]["state"], "active");
    // expires_at should be extended.
    assert!(body["data"]["expires_at"].is_string());
}
```

### 9.6 Extend TTL -- rejects exceeding max lifetime

```rust
#[tokio::test]
async fn extend_ttl_rejects_over_max_lifetime() {
    // Create a temp env that was created 60h ago with 12h remaining.
    let (app, temp_env_id) = test_app_with_aging_temp_env(60).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!(
                    "/api/repos/664f0a1b2c3d4e5f6a7b8c9d/temp-envs/{temp_env_id}/extend"
                ))
                .header("content-type", "application/json")
                .header("authorization", "Bearer <valid_token>")
                .body(Body::from(r#"{"hours": 24}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = parse_body(response).await;
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("maximum lifetime"));
}
```

### 9.7 Undo expire -- happy path

```rust
#[tokio::test]
async fn undo_expire_reactivates_env() {
    let (app, temp_env_id) = test_app_with_expiring_temp_env().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!(
                    "/api/repos/664f0a1b2c3d4e5f6a7b8c9d/temp-envs/{temp_env_id}/undo-expire"
                ))
                .header("authorization", "Bearer <valid_token>")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = parse_body(response).await;
    assert_eq!(body["data"]["state"], "active");
    assert!(body["data"]["grace_until"].is_null());
}
```

### 9.8 Undo expire -- fails after grace period

```rust
#[tokio::test]
async fn undo_expire_fails_after_grace() {
    // Create a temp env with grace_until in the past.
    let (app, temp_env_id) = test_app_with_expired_grace_temp_env().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!(
                    "/api/repos/664f0a1b2c3d4e5f6a7b8c9d/temp-envs/{temp_env_id}/undo-expire"
                ))
                .header("authorization", "Bearer <valid_token>")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::GONE);
}
```

### 9.9 Delete temp env

```rust
#[tokio::test]
async fn delete_active_temp_env() {
    let (app, temp_env_id) = test_app_with_active_temp_env().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(&format!(
                    "/api/repos/664f0a1b2c3d4e5f6a7b8c9d/temp-envs/{temp_env_id}"
                ))
                .header("authorization", "Bearer <valid_token>")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}
```

### 9.10 Expiry worker -- active past TTL transitions to expiring

```rust
#[tokio::test]
async fn expiry_worker_transitions_active_to_expiring() {
    let db = test_mongo().await;
    let repo = TempEnvRepo::new(&db);

    // Insert an active temp env with expires_at in the past.
    let env = test_temp_env(TempEnvState::Active, Utc::now() - Duration::hours(1));
    repo.insert(&env).await.unwrap();

    // Run the expiry worker.
    run_expiry_worker(&repo).await.unwrap();

    // Verify state transitioned to Expiring with grace_until set.
    let updated = repo.find_by_id(env.id).await.unwrap().unwrap();
    assert_eq!(updated.state, TempEnvState::Expiring);
    assert!(updated.grace_until.is_some());
    assert!(updated.grace_until.unwrap() > Utc::now());
}
```

### 9.11 Expiry worker -- expiring past grace transitions to expired

```rust
#[tokio::test]
async fn expiry_worker_transitions_expiring_to_expired() {
    let db = test_mongo().await;
    let repo = TempEnvRepo::new(&db);

    // Insert an expiring temp env with grace_until in the past.
    let mut env = test_temp_env(TempEnvState::Expiring, Utc::now() - Duration::hours(2));
    env.grace_until = Some(Utc::now() - Duration::minutes(30));
    repo.insert(&env).await.unwrap();

    // Run the expiry worker.
    run_expiry_worker(&repo).await.unwrap();

    // Verify state transitioned to Expired.
    let updated = repo.find_by_id(env.id).await.unwrap().unwrap();
    assert_eq!(updated.state, TempEnvState::Expired);
}
```

### 9.12 Cleanup worker -- tears down database and branch

```rust
#[tokio::test]
async fn cleanup_worker_tears_down_and_deletes() {
    let db = test_mongo().await;
    let repo = TempEnvRepo::new(&db);
    let mock_gitaly = mock_gitaly_server().await;

    // Insert an expired temp env.
    let env = test_temp_env(TempEnvState::Expired, Utc::now() - Duration::hours(3));
    repo.insert(&env).await.unwrap();

    // Run the cleanup worker.
    run_cleanup_worker(&repo, &mock_gitaly).await.unwrap();

    // Verify state transitioned to Deleted.
    let updated = repo.find_by_id(env.id).await.unwrap().unwrap();
    assert_eq!(updated.state, TempEnvState::Deleted);

    // Verify the isolated database was dropped.
    assert!(!database_exists(&db, &env.db_name).await);

    // Verify UserDeleteBranch was called on mock gitaly.
    mock_gitaly.assert_branch_deleted(&format!("temp/{}/ws/alice/myapp", env.id.to_hex()));
}
```

### 9.13 Activity touch updates TTL

```rust
#[tokio::test]
async fn touch_activity_extends_expiry() {
    let db = test_mongo().await;
    let repo = TempEnvRepo::new(&db);

    let env = test_temp_env(TempEnvState::Active, Utc::now() + Duration::hours(12));
    repo.insert(&env).await.unwrap();

    let old_expires = env.expires_at;

    // Simulate a brief delay then touch.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    repo.touch_activity(env.id).await.unwrap();

    let updated = repo.find_by_id(env.id).await.unwrap().unwrap();
    assert!(updated.expires_at > old_expires);
    assert!(updated.last_activity_at > env.last_activity_at);
}
```

## 10. Acceptance Criteria

1. **Temp environments can be created on demand from a workspace or changeset.**
   - `POST /api/repos/:repoId/temp-envs` with `workspace_id` creates a workspace
     temp env in `Provisioning` state and returns 201.
   - `POST /api/repos/:repoId/temp-envs` with `changeset_id` creates a changeset
     temp env in `Provisioning` state and returns 201.
   - The provisioning job creates an isolated database and (for workspace kind)
     a snapshot Git branch, then transitions state to `Active`.
   - Duplicate active temp environments for the same source are rejected (409).

2. **TTL tracking is based on 24h idle and updated on activity.**
   - `expires_at` is set to `last_activity_at + 24h` on creation.
   - Any API call, test run, or deployment targeting the temp env updates
     `last_activity_at` and recalculates `expires_at`.
   - The expiry worker runs periodically and catches environments that have
     exceeded their TTL.

3. **Soft expiry with 1h grace period and undo-expire.**
   - When TTL expires, state transitions to `Expiring` and `grace_until` is set
     to `now + 1h`.
   - `POST .../undo-expire` during the grace window transitions back to `Active`,
     recalculates TTL, and clears `grace_until`.
   - After the grace window, state transitions to `Expired` and a cleanup job
     is enqueued.
   - `POST .../undo-expire` after grace returns 410 Gone.

4. **Manual TTL extension works before expiry.**
   - `POST .../extend` with `hours` adds time to `expires_at`.
   - Extension is capped so total lifetime from `created_at` does not exceed 72h.
   - Extension is only allowed in `Active` state.

5. **Cleanup workers tear down expired environments.**
   - The cleanup worker drops the isolated MongoDB database.
   - The cleanup worker deletes the snapshot Git branch via gitaly.
   - State transitions to `Deleted` after successful cleanup.
   - Cleanup is idempotent: re-running after partial failure completes teardown.

6. **Lifecycle events generate audit and email notifications.**
   - All state transitions emit audit events: `temp_env.created`,
     `temp_env.provisioned`, `temp_env.expiring`, `temp_env.expired`,
     `temp_env.undo_expired`, `temp_env.ttl_extended`, `temp_env.deleted`,
     `temp_env.cleaned_up`, `temp_env.provision_failed`.
   - Email notifications are sent for: expiry warning (1h before), soft expiry,
     grace end, provision failure.
   - Notifications respect the per-user toggle (E11).

7. **Explicit delete tears down immediately.**
   - `DELETE /api/repos/:repoId/temp-envs/:tempEnvId` transitions state to
     `Deleted` and enqueues a cleanup job.
   - Allowed from `Active` or `Expiring` state.
   - Returns 204 on success.

8. **Runtime profile derivation and URL generation.**
   - Temp env runtime profile is derived from selected base environment profile.
   - Base selection priority is app default base profile first, then user
     override at creation time.
   - Generated URL follows readable host pattern:
     `{app}-{kind}-{word}.<domain>`.
   - Each temp-env instance gets a unique generated URL (no workspace-stable
     host reuse).
   - Special reusable base profiles are managed by `admin` only.
