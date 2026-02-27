# E08 Release Assembly, Publish, and Tagging

## 1. Goal

Compose subset releases from the approved changeset queue and publish immutable
Git-tagged artifacts. A config manager selects which queued changesets to include,
orders them manually, triggers composition (sequential merge onto `integration_branch`), and
publishes the result as a lightweight tag (`rYYYY.MM.DD.N`). Published releases
are immutable and auditable. After publish, remaining queued changesets are
revalidated against the new `integration_branch` HEAD. Publish also enforces
environment-profile validation gates.

## 2. Dependencies

| Epic | What it provides |
|------|-----------------|
| E01 Git Adapter | `GitalyClient` with Tonic channel, retry logic, `app_to_gitaly_repo()` helper |
| E03 App Setup | Runtime profile definitions and environment linkage |
| E06 Async Jobs | Job framework (`jobs` collection, runner, worker trait, job state machine) |
| E07 Queue Orchestration | Queued changeset pool, revalidation trigger interface |

## 3. Rust Types

### 3.1 ReleaseState (`conman-core/src/release.rs`)

Enum representing every state a release can occupy. Serialized to/from
snake_case strings for MongoDB and API responses.

```rust
use serde::{Deserialize, Serialize};

/// State machine for release lifecycle.
///
/// Transitions are enforced by `ReleaseState::transition()` -- the only
/// code path allowed to advance state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseState {
    /// Config manager has created the release and is selecting changesets.
    DraftRelease,
    /// Composition job is running (merging changesets onto the integration branch in order).
    Assembling,
    /// Composition succeeded and all tests passed.
    Validated,
    /// Git tag created, integration branch ref updated. Immutable from here on.
    Published,
    /// Deployed to at least one but not all environments.
    DeployedPartial,
    /// Deployed to every configured environment.
    DeployedFull,
    /// Release was rolled back (revert commit + new release, or prior tag redeployed).
    RolledBack,
}

impl std::fmt::Display for ReleaseState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::DraftRelease => "draft_release",
            Self::Assembling => "assembling",
            Self::Validated => "validated",
            Self::Published => "published",
            Self::DeployedPartial => "deployed_partial",
            Self::DeployedFull => "deployed_full",
            Self::RolledBack => "rolled_back",
        };
        write!(f, "{s}")
    }
}
```

### 3.2 ReleaseBatch (`conman-core/src/release.rs`)

Primary domain struct representing a release. One release per document in the
`release_batches` collection.

```rust
use bson::oid::ObjectId;
use chrono::{DateTime, Utc};

/// A release batch: a curated, ordered subset of queued changesets
/// that will be composed into a single tagged commit on the integration branch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseBatch {
    /// MongoDB document ID.
    #[serde(rename = "_id")]
    pub id: ObjectId,

    /// The app this release belongs to.
    pub app_id: ObjectId,

    /// Release tag in `rYYYY.MM.DD.N` format. Assigned at creation time
    /// (next available sequence number for today). Unique per app.
    pub tag: String,

    /// Current lifecycle state.
    pub state: ReleaseState,

    /// Changeset IDs in composition order. Position is implicit (vec index).
    /// Only changesets in `queued` state may be added.
    pub ordered_changeset_ids: Vec<ObjectId>,

    /// Job ID of the composition/assembly job (set when assembly starts).
    pub compose_job_id: Option<ObjectId>,

    /// SHA of the final composed commit on the integration branch (set after publish).
    pub published_sha: Option<String>,

    /// Timestamp when the release was published.
    pub published_at: Option<DateTime<Utc>>,

    /// User ID of the actor who triggered publish.
    pub published_by: Option<ObjectId>,

    /// When the draft was first created.
    pub created_at: DateTime<Utc>,

    /// Last modification timestamp.
    pub updated_at: DateTime<Utc>,
}
```

### 3.3 ReleaseChangeset (`conman-core/src/release.rs`)

Join record linking a release to an individual changeset, preserving merge
order and tracking the SHA produced when that specific changeset was merged
during composition.

```rust
/// Tracks per-changeset state within a release composition.
///
/// One document per changeset included in a release. The `position` field
/// determines merge order during assembly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseChangeset {
    /// MongoDB document ID.
    #[serde(rename = "_id")]
    pub id: ObjectId,

    /// Parent release batch.
    pub release_id: ObjectId,

    /// The changeset being included.
    pub changeset_id: ObjectId,

    /// Zero-based position in the merge order.
    pub position: u32,

    /// SHA of the merge commit created for this specific changeset during
    /// composition. Set by the assembly worker after a successful merge step.
    pub merge_sha: Option<String>,
}
```

### 3.4 State Machine Transitions (`conman-core/src/release.rs`)

All state changes pass through a single function that validates the
transition and any guard conditions. Returns `ConmanError::InvalidTransition`
for illegal moves.

```rust
use crate::error::ConmanError;

impl ReleaseState {
    /// Attempt to transition from the current state to `target`.
    ///
    /// Guard conditions are checked inline. Returns the new state on
    /// success or `ConmanError::InvalidTransition` on failure.
    pub fn transition(
        self,
        target: ReleaseState,
        guard: &TransitionGuard,
    ) -> Result<ReleaseState, ConmanError> {
        use ReleaseState::*;

        let allowed = match (self, target) {
            // Draft -> Assembling: must have at least one changeset selected.
            (DraftRelease, Assembling) => guard.has_changesets,

            // Assembling -> Validated: compose job succeeded with no conflicts
            // or test failures.
            (Assembling, Validated) => guard.compose_succeeded,

            // Assembling -> DraftRelease: compose failed (conflict or test
            // failure), config manager can revise the selection.
            (Assembling, DraftRelease) => guard.compose_failed,

            // Validated -> Published: tag created and integration branch ref updated.
            (Validated, Published) => guard.tag_created,

            // Published -> DeployedPartial: first deployment to any env succeeded.
            (Published, DeployedPartial) => true,

            // DeployedPartial -> DeployedFull: all environments deployed.
            (DeployedPartial, DeployedFull) => guard.all_envs_deployed,

            // DeployedPartial -> DeployedPartial: another env deployed but not all.
            (DeployedPartial, DeployedPartial) => !guard.all_envs_deployed,

            // Published or DeployedPartial or DeployedFull -> RolledBack.
            (Published, RolledBack)
            | (DeployedPartial, RolledBack)
            | (DeployedFull, RolledBack) => true,

            _ => false,
        };

        if allowed {
            Ok(target)
        } else {
            Err(ConmanError::InvalidTransition {
                from: self.to_string(),
                to: target.to_string(),
            })
        }
    }
}

/// Guard conditions evaluated before a state transition is accepted.
///
/// Populated by the caller (handler or job worker) from current domain state.
#[derive(Debug, Clone, Default)]
pub struct TransitionGuard {
    /// At least one changeset is in `ordered_changeset_ids`.
    pub has_changesets: bool,
    /// Compose job completed without conflicts or test failures.
    pub compose_succeeded: bool,
    /// Compose job failed (conflicts or tests).
    pub compose_failed: bool,
    /// Git tag was created and integration branch ref updated.
    pub tag_created: bool,
    /// All configured environments have a successful deployment for this release.
    pub all_envs_deployed: bool,
}
```

### 3.5 API Request/Response Types (`conman-api/src/dto/release.rs`)

DTOs for the release endpoints. These are API-facing types, distinct from
the domain `ReleaseBatch` struct.

```rust
use bson::oid::ObjectId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// POST /api/repos/:appId/releases
///
/// Creates a draft release. The tag is auto-generated as `rYYYY.MM.DD.N`.
/// Optionally accepts an initial set of changeset IDs to include.
#[derive(Debug, Deserialize)]
pub struct CreateReleaseRequest {
    /// Optional initial changeset IDs to include (must all be in `queued` state).
    #[serde(default)]
    pub changeset_ids: Vec<String>,
}

/// POST /api/repos/:appId/releases/:releaseId/changesets
///
/// Add or remove changesets from a draft release. Only valid when release
/// is in `draft_release` state.
#[derive(Debug, Deserialize)]
pub struct AddChangesetsRequest {
    /// Changeset IDs to add (must be in `queued` state).
    #[serde(default)]
    pub add: Vec<String>,
    /// Changeset IDs to remove from the release.
    #[serde(default)]
    pub remove: Vec<String>,
}

/// POST /api/repos/:appId/releases/:releaseId/reorder
///
/// Set the explicit merge order for changesets in a draft release.
#[derive(Debug, Deserialize)]
pub struct ReorderRequest {
    /// Changeset IDs in the desired merge order. Must be a permutation of
    /// the current `ordered_changeset_ids`.
    pub ordered_changeset_ids: Vec<String>,
}

/// Response DTO returned for release detail and list endpoints.
#[derive(Debug, Serialize)]
pub struct ReleaseResponse {
    pub id: String,
    pub app_id: String,
    pub tag: String,
    pub state: String,
    pub ordered_changeset_ids: Vec<String>,
    pub compose_job_id: Option<String>,
    pub published_sha: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub published_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// List query parameters for GET /api/repos/:appId/releases
#[derive(Debug, Deserialize)]
pub struct ReleaseListQuery {
    #[serde(default = "super::default_page")]
    pub page: u64,
    #[serde(default = "super::default_limit")]
    pub limit: u64,
    /// Optional state filter (e.g. `?state=draft_release`).
    pub state: Option<String>,
}

impl From<&ReleaseBatch> for ReleaseResponse {
    fn from(r: &ReleaseBatch) -> Self {
        Self {
            id: r.id.to_hex(),
            app_id: r.app_id.to_hex(),
            tag: r.tag.clone(),
            state: r.state.to_string(),
            ordered_changeset_ids: r.ordered_changeset_ids.iter().map(|id| id.to_hex()).collect(),
            compose_job_id: r.compose_job_id.map(|id| id.to_hex()),
            published_sha: r.published_sha.clone(),
            published_at: r.published_at,
            published_by: r.published_by.map(|id| id.to_hex()),
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}
```

## 4. Database

### 4.1 `release_batches` Collection

Stores one document per release. Primary workflow collection for the release
lifecycle.

**Fields:**

| Field | BSON type | Description |
|-------|-----------|-------------|
| `_id` | ObjectId | Document ID |
| `app_id` | ObjectId | Parent app |
| `tag` | String | Release tag (`rYYYY.MM.DD.N`) |
| `state` | String | Current `ReleaseState` value |
| `ordered_changeset_ids` | Array\<ObjectId\> | Changeset IDs in merge order |
| `compose_job_id` | ObjectId \| null | Assembly job reference |
| `published_sha` | String \| null | Git SHA of the final composed commit |
| `published_at` | DateTime \| null | Publication timestamp |
| `published_by` | ObjectId \| null | User who published |
| `created_at` | DateTime | Creation timestamp |
| `updated_at` | DateTime | Last modification timestamp |

**Indexes:**

```rust
/// Indexes for the release_batches collection.
async fn ensure_indexes(&self) -> Result<(), ConmanError> {
    let collection = self.db.collection::<ReleaseBatch>("release_batches");

    // Lookup by app + state (list releases filtered by state).
    collection.create_index(
        IndexModel::builder()
            .keys(doc! { "app_id": 1, "state": 1 })
            .build(),
    ).await?;

    // Unique tag per app -- prevents duplicate release tags.
    collection.create_index(
        IndexModel::builder()
            .keys(doc! { "app_id": 1, "tag": 1 })
            .options(IndexOptions::builder().unique(true).build())
            .build(),
    ).await?;

    // Lookup by app sorted by creation time (list recent releases).
    collection.create_index(
        IndexModel::builder()
            .keys(doc! { "app_id": 1, "created_at": -1 })
            .build(),
    ).await?;

    Ok(())
}
```

**Example documents:**

Draft release with two changesets selected:

```json
{
  "_id": ObjectId("664f1a2b3c4d5e6f7a8b9c0e"),
  "app_id": ObjectId("664f1a2b3c4d5e6f7a8b9c01"),
  "tag": "r2026.02.25.1",
  "state": "draft_release",
  "ordered_changeset_ids": [
    ObjectId("664f1a2b3c4d5e6f7a8b9c10"),
    ObjectId("664f1a2b3c4d5e6f7a8b9c11")
  ],
  "compose_job_id": null,
  "published_sha": null,
  "published_at": null,
  "published_by": null,
  "created_at": ISODate("2026-02-25T10:00:00Z"),
  "updated_at": ISODate("2026-02-25T10:05:00Z")
}
```

Published release:

```json
{
  "_id": ObjectId("664f1a2b3c4d5e6f7a8b9c0f"),
  "app_id": ObjectId("664f1a2b3c4d5e6f7a8b9c01"),
  "tag": "r2026.02.24.2",
  "state": "published",
  "ordered_changeset_ids": [
    ObjectId("664f1a2b3c4d5e6f7a8b9c12"),
    ObjectId("664f1a2b3c4d5e6f7a8b9c13"),
    ObjectId("664f1a2b3c4d5e6f7a8b9c14")
  ],
  "compose_job_id": ObjectId("664f1a2b3c4d5e6f7a8b9c20"),
  "published_sha": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
  "published_at": ISODate("2026-02-24T16:42:00Z"),
  "published_by": ObjectId("664f1a2b3c4d5e6f7a8b9c02"),
  "created_at": ISODate("2026-02-24T14:00:00Z"),
  "updated_at": ISODate("2026-02-24T16:42:00Z")
}
```

### 4.2 `release_changesets` Collection

Join collection linking releases to their constituent changesets with
positional ordering and per-changeset merge SHA tracking.

**Fields:**

| Field | BSON type | Description |
|-------|-----------|-------------|
| `_id` | ObjectId | Document ID |
| `release_id` | ObjectId | Parent release batch |
| `changeset_id` | ObjectId | Referenced changeset |
| `position` | Int32 | Zero-based merge order position |
| `merge_sha` | String \| null | SHA of the merge commit for this changeset |

**Indexes:**

```rust
/// Indexes for the release_changesets collection.
async fn ensure_indexes(&self) -> Result<(), ConmanError> {
    let collection = self.db.collection::<ReleaseChangeset>("release_changesets");

    // Ordered lookup of all changesets in a release.
    collection.create_index(
        IndexModel::builder()
            .keys(doc! { "release_id": 1, "position": 1 })
            .build(),
    ).await?;

    // Reverse lookup: which release(s) include a given changeset.
    // A changeset may only appear in one non-draft release, but this
    // index supports the check.
    collection.create_index(
        IndexModel::builder()
            .keys(doc! { "changeset_id": 1 })
            .build(),
    ).await?;

    Ok(())
}
```

**Example document:**

```json
{
  "_id": ObjectId("664f1a2b3c4d5e6f7a8b9c30"),
  "release_id": ObjectId("664f1a2b3c4d5e6f7a8b9c0f"),
  "changeset_id": ObjectId("664f1a2b3c4d5e6f7a8b9c12"),
  "position": 0,
  "merge_sha": "b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3"
}
```

## 5. API Endpoints

All endpoints are scoped under `/api/repos/:appId/releases`. Authentication
is required. Role checks are noted per endpoint.

---

### 5.1 List Releases

```
GET /api/repos/:appId/releases?page=&limit=&state=
```

**Role:** Any app member (read access).

**Query Parameters:**

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `page` | u64 | 1 | Page number (1-based) |
| `limit` | u64 | 20 | Items per page (max 100) |
| `state` | String | (none) | Optional state filter |

**Response 200:**

```json
{
  "data": [
    {
      "id": "664f1a2b3c4d5e6f7a8b9c0e",
      "app_id": "664f1a2b3c4d5e6f7a8b9c01",
      "tag": "r2026.02.25.1",
      "state": "draft_release",
      "ordered_changeset_ids": ["664f1a2b3c4d5e6f7a8b9c10"],
      "compose_job_id": null,
      "published_sha": null,
      "published_at": null,
      "published_by": null,
      "created_at": "2026-02-25T10:00:00Z",
      "updated_at": "2026-02-25T10:05:00Z"
    }
  ],
  "pagination": { "page": 1, "limit": 20, "total": 1 }
}
```

**Handler:**

```rust
/// GET /api/repos/:appId/releases
///
/// List releases for the app, optionally filtered by state.
/// Sorted by created_at descending (most recent first).
pub async fn list_releases(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(app_id): Path<String>,
    Query(params): Query<ReleaseListQuery>,
) -> Result<Json<ApiResponse<Vec<ReleaseResponse>>>, ConmanError> {
    let app_id = parse_object_id(&app_id)?;
    auth_user.require_member(app_id)?;

    let pagination = Pagination { page: params.page, limit: params.limit }.validate()?;
    let state_filter = params.state.as_deref().map(parse_release_state).transpose()?;

    let (releases, total) = release_repo
        .list_by_app(app_id, state_filter, &pagination)
        .await?;

    let data: Vec<ReleaseResponse> = releases.iter().map(ReleaseResponse::from).collect();
    Ok(Json(ApiResponse::paginated(data, pagination.page, pagination.limit, total)))
}
```

---

### 5.2 Create Draft Release

```
POST /api/repos/:appId/releases
```

**Role:** `config_manager` or `admin`.

**Request Body:**

```json
{
  "changeset_ids": ["664f1a2b3c4d5e6f7a8b9c10", "664f1a2b3c4d5e6f7a8b9c11"]
}
```

`changeset_ids` is optional. If provided, each must be in `queued` state.

**Response 201:**

```json
{
  "data": {
    "id": "664f1a2b3c4d5e6f7a8b9c0e",
    "app_id": "664f1a2b3c4d5e6f7a8b9c01",
    "tag": "r2026.02.25.1",
    "state": "draft_release",
    "ordered_changeset_ids": ["664f1a2b3c4d5e6f7a8b9c10", "664f1a2b3c4d5e6f7a8b9c11"],
    "compose_job_id": null,
    "published_sha": null,
    "published_at": null,
    "published_by": null,
    "created_at": "2026-02-25T10:00:00Z",
    "updated_at": "2026-02-25T10:00:00Z"
  }
}
```

**Handler logic:**

1. Verify caller role (`config_manager+`).
2. Validate all provided changeset IDs exist and are in `queued` state.
3. Generate the tag: query `release_batches` for all tags matching
   `rYYYY.MM.DD.*` for today, compute next `N`.
4. Insert `ReleaseBatch` with state `DraftRelease`.
5. Insert `ReleaseChangeset` records for each changeset with sequential positions.
6. Emit audit event (`release.created`).

---

### 5.3 Get Release Detail

```
GET /api/repos/:appId/releases/:releaseId
```

**Role:** Any app member.

**Response 200:** Same shape as `ReleaseResponse`, plus a `changesets` array
with per-changeset detail:

```json
{
  "data": {
    "id": "664f1a2b3c4d5e6f7a8b9c0e",
    "app_id": "664f1a2b3c4d5e6f7a8b9c01",
    "tag": "r2026.02.25.1",
    "state": "draft_release",
    "ordered_changeset_ids": ["664f1a2b3c4d5e6f7a8b9c10"],
    "changesets": [
      {
        "changeset_id": "664f1a2b3c4d5e6f7a8b9c10",
        "position": 0,
        "merge_sha": null
      }
    ],
    "compose_job_id": null,
    "published_sha": null,
    "published_at": null,
    "published_by": null,
    "created_at": "2026-02-25T10:00:00Z",
    "updated_at": "2026-02-25T10:05:00Z"
  }
}
```

---

### 5.4 Add/Remove Changesets

```
POST /api/repos/:appId/releases/:releaseId/changesets
```

**Role:** `config_manager` or `admin`.

**Guard:** Release must be in `draft_release` state.

**Request Body:**

```json
{
  "add": ["664f1a2b3c4d5e6f7a8b9c15"],
  "remove": ["664f1a2b3c4d5e6f7a8b9c11"]
}
```

**Handler logic:**

1. Verify release is in `DraftRelease` state.
2. For `add`: validate each changeset exists, is in `queued` state, and is not
   already included in another non-draft release.
3. For `remove`: delete corresponding `ReleaseChangeset` documents.
4. Append new changesets at the end of the current order.
5. Recompute positions (0-based contiguous).
6. Update `ordered_changeset_ids` and `updated_at` on the release batch.
7. Emit audit event (`release.changesets_modified`).

**Response 200:** Updated `ReleaseResponse`.

---

### 5.5 Reorder Changesets

```
POST /api/repos/:appId/releases/:releaseId/reorder
```

**Role:** `config_manager` or `admin`.

**Guard:** Release must be in `draft_release` state.

**Request Body:**

```json
{
  "ordered_changeset_ids": [
    "664f1a2b3c4d5e6f7a8b9c11",
    "664f1a2b3c4d5e6f7a8b9c10"
  ]
}
```

**Validation:** The provided list must be an exact permutation of the current
`ordered_changeset_ids` (same elements, no additions, no removals).

**Handler logic:**

1. Verify release is in `DraftRelease` state.
2. Validate the provided list is a permutation of current IDs.
3. Update `ordered_changeset_ids` on the release batch.
4. Update `position` on each `ReleaseChangeset` document to match new order.
5. Emit audit event (`release.reordered`).

**Response 200:** Updated `ReleaseResponse`.

---

### 5.6 Assemble Release

```
POST /api/repos/:appId/releases/:releaseId/assemble
```

**Role:** `config_manager` or `admin`.

**Guard:** Release must be in `draft_release` state and have at least one
changeset selected.

**Handler logic:**

1. Transition state: `DraftRelease` -> `Assembling`.
2. Create a `release_assemble` job in the `jobs` collection.
3. Store the job ID as `compose_job_id` on the release batch.
4. Emit audit event (`release.assembly_started`).
5. Return immediately (composition runs asynchronously).

**Response 202:**

```json
{
  "data": {
    "id": "664f1a2b3c4d5e6f7a8b9c0e",
    "state": "assembling",
    "compose_job_id": "664f1a2b3c4d5e6f7a8b9c20",
    "tag": "r2026.02.25.1"
  }
}
```

**Assembly Worker (`conman-jobs`):**

The `release_assemble` worker performs the composition sequentially:

```rust
/// Compose a release by merging each selected changeset onto the integration branch in order.
///
/// For each changeset (in position order):
///   1. Merge the changeset branch into a temp ref using UserMergeToRef.
///   2. If merge conflicts → mark changeset as `conflicted`, fail the job.
///   3. If msuite test fails → mark changeset as `needs_revalidation`, fail the job.
///   4. Record merge_sha on the ReleaseChangeset document.
///
/// On full success:
///   - Transition release to Validated.
///
/// On failure:
///   - Transition release back to DraftRelease.
///   - Mark failing changeset(s) with appropriate state.
async fn execute_release_assemble(job: &Job, ctx: &WorkerContext) -> Result<(), ConmanError> {
    let release = ctx.release_repo.find_by_id(job.entity_id).await?;
    let app = ctx.app_repo.find_by_id(release.app_id).await?;
    let repo = app_to_gitaly_repo(&app);

    // Resolve current integration branch HEAD as the starting point.
    let integration_commit = ctx
        .gitaly
        .find_commit(&repo, "refs/heads/<integration_branch>")
        .await?;
    let mut current_sha = integration_commit.id.clone();

    // Create a temporary composition ref to avoid touching integration branch until publish.
    let compose_ref = format!("refs/conman/compose/{}", release.id.to_hex());

    let release_changesets = ctx.release_changeset_repo
        .find_by_release_ordered(release.id)
        .await?;

    for rc in &release_changesets {
        let changeset = ctx.changeset_repo.find_by_id(rc.changeset_id).await?;

        // Merge changeset head into the running composition ref.
        let merge_result = ctx.gitaly.user_merge_to_ref(
            &repo,
            &ctx.conman_user(),
            &changeset.head_sha,
            compose_ref.as_bytes(),   // target_ref
            current_sha.as_bytes(),    // first_parent_ref resolved from prior step
            format!(
                "Compose changeset {} into release {}",
                changeset.id.to_hex(),
                release.tag
            ).as_bytes(),
        ).await;

        match merge_result {
            Ok(response) => {
                // Record the merge SHA for this changeset.
                ctx.release_changeset_repo
                    .set_merge_sha(rc.id, &response.commit_id)
                    .await?;
                current_sha = response.commit_id;
            }
            Err(e) if e.is_merge_conflict() => {
                // Mark this changeset as conflicted.
                ctx.changeset_repo
                    .transition_state(rc.changeset_id, ChangesetState::Conflicted)
                    .await?;

                // Fail the release back to draft.
                ctx.release_repo
                    .transition_state(release.id, ReleaseState::DraftRelease)
                    .await?;

                return Err(ConmanError::Git {
                    message: format!(
                        "Merge conflict composing changeset {}",
                        changeset.id.to_hex()
                    ),
                });
            }
            Err(e) => return Err(e),
        }
    }

    // All merges succeeded -- run msuite validation on the composed result.
    // (Delegates to msuite_merge job or inline check depending on config.)

    // Transition release to Validated.
    ctx.release_repo
        .transition_state(release.id, ReleaseState::Validated)
        .await?;

    Ok(())
}
```

---

### 5.7 Publish Release

```
POST /api/repos/:appId/releases/:releaseId/publish
```

**Role:** `config_manager` or `admin`.

**Guard:** Release must be in `validated` state.

**Handler logic:**

1. Transition state: `Validated` -> `Published`.
2. Fast-forward `integration_branch` to the composed commit using `UserMergeBranch`.
3. Create a lightweight Git tag (`rYYYY.MM.DD.N`) using `UserCreateTag`.
4. Set `published_sha`, `published_at`, `published_by` on the release batch.
5. Mark all included changesets as `released`.
6. Enqueue `revalidate_queued_changeset` jobs for all remaining queued
   changesets (E07 revalidation trigger).
7. Emit audit event (`release.published`).

**Response 200:**

```json
{
  "data": {
    "id": "664f1a2b3c4d5e6f7a8b9c0e",
    "state": "published",
    "tag": "r2026.02.25.1",
    "published_sha": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
    "published_at": "2026-02-25T16:42:00Z",
    "published_by": "664f1a2b3c4d5e6f7a8b9c02"
  }
}
```

## 6. Business Logic

### 6.1 Release Creation

A config manager (or app admin) creates a draft release by selecting a subset
of queued changesets. The tag is auto-assigned using the format
`rYYYY.MM.DD.N` where `N` is the next available sequence number for that
calendar day within the app.

**Tag generation algorithm:**

```rust
/// Generate the next release tag for today.
///
/// Queries existing tags matching today's date prefix and increments the
/// sequence number. Thread-safe because the unique index on (app_id, tag)
/// rejects duplicates, causing a retry with the next N.
pub async fn next_tag(
    release_repo: &ReleaseRepo,
    app_id: ObjectId,
) -> Result<String, ConmanError> {
    let today = Utc::now().format("%Y.%m.%d").to_string();
    let prefix = format!("r{today}.");

    // Find the highest N for today's tags on this app.
    let max_n = release_repo
        .find_max_tag_sequence(app_id, &prefix)
        .await?;

    let next_n = max_n.map_or(1, |n| n + 1);
    Ok(format!("r{today}.{next_n}"))
}
```

**Validation rules:**

- All selected changeset IDs must resolve to changesets in `queued` state.
- A changeset cannot be included in more than one non-draft release
  simultaneously.
- The creating user must have `config_manager` or `admin` role on the app.

### 6.2 Composition Engine

Composition merges each selected changeset onto `integration_branch` in the specified order.
The process uses a temporary ref (`refs/conman/compose/<releaseId>`) so that
`integration_branch` is not modified until the explicit publish step.

**Composition steps (per changeset, in position order):**

1. Call `UserMergeToRef` with `first_parent_ref` set to the previous merge
   result (or `refs/heads/<integration_branch>` for the first changeset) and `source_sha` set
   to the changeset's `head_sha`.
2. If the merge produces a conflict, mark that specific changeset as
   `conflicted` and abort the composition. The release returns to
   `draft_release` state so the config manager can revise the selection.
3. If the merge succeeds but a subsequent `msuite` test run fails, mark the
   changeset as `needs_revalidation` and abort similarly.
4. Record the resulting `merge_sha` on the `ReleaseChangeset` document.

**Failure handling:**

- On conflict: the offending changeset moves to `conflicted` state, the
  release reverts to `draft_release`, and the compose temp ref is cleaned up.
- On test failure: the offending changeset moves to `needs_revalidation`,
  same cleanup.
- The config manager can then remove or reorder the failing changeset and
  re-trigger assembly.

### 6.3 Tag Format

Tags follow the `rYYYY.MM.DD.N` convention:

- `r` prefix distinguishes release tags from other refs.
- `YYYY.MM.DD` is the UTC date of release creation (not publish).
- `N` is a 1-based daily sequence number, auto-incremented per app.
- Examples: `r2026.02.25.1`, `r2026.02.25.2`, `r2026.03.01.1`.

The tag is generated at draft creation time and reserved via the unique
`(app_id, tag)` index.

### 6.4 Publish Flow

Publish is the atomic step that makes a release immutable and visible:

1. **Verify state** is `Validated` (composition succeeded, tests passed).
2. **Update integration branch ref:** Use `UserMergeBranch` (two-phase streaming RPC) to
   fast-forward `refs/heads/<integration_branch>` to the composed commit SHA. The
   `expected_old_oid` field prevents races with concurrent modifications.
3. **Create tag:** Use `UserCreateTag` to create a lightweight tag pointing
   at the same composed commit SHA.
4. **Persist metadata:** Set `published_sha`, `published_at`, `published_by`
   on the release batch. Transition state to `Published`.
5. **Mark changesets released:** Update every included changeset to the
   `released` terminal state.
6. **Trigger revalidation:** Enqueue `revalidate_queued_changeset` jobs for
   all changesets still in `queued` state for this app (E07 dependency).
   These will check for conflicts and re-run tests against the new `integration_branch`.
7. **Emit audit event** with before/after state, published SHA, and actor.
8. **Clean up compose ref:** Delete `refs/conman/compose/<releaseId>`.

### 6.5 Post-Publish Revalidation

After a release is published, `integration_branch` has moved forward. All remaining queued
changesets must be revalidated:

- **Conflict check:** Attempt a trial merge of each queued changeset against
  the new `integration_branch` HEAD using `UserMergeToRef` to a disposable ref.
- **Test re-run:** Execute `msuite_merge` tests against the trial merge
  result.
- **On conflict:** Transition changeset to `conflicted`.
- **On test failure:** Transition changeset to `needs_revalidation`.
- **On success:** Changeset remains `queued`.

Both `conflicted` and `needs_revalidation` changesets can be moved back to
`draft` by the author or a config manager, where they can be updated and
re-submitted.

### 6.6 Immutability

Once a release reaches `Published` state:

- `ordered_changeset_ids` cannot be modified.
- The release cannot be deleted.
- The Git tag cannot be moved or deleted.
- The only valid state transitions are forward to deployment states
  (`DeployedPartial`, `DeployedFull`) or `RolledBack`.

## 7. Gitaly-rs Integration

All Git operations for release composition and tagging use the following
gRPC RPCs from the gitaly proto definitions.

### 7.1 `OperationService.UserMergeBranch`

Used during the **publish** step to fast-forward `refs/heads/<integration_branch>` to the
composed commit. This is a two-phase streaming RPC: the first request sends
the merge parameters, the response returns the merge commit ID, the second
request confirms with `apply = true`.

**Proto definition** (`operations.proto`):

```protobuf
// Two-phase streaming merge. First request sends parameters, first response
// returns the merge commit ID. Second request sets apply=true to commit the
// ref update. Executes hooks and authorization checks.
rpc UserMergeBranch(stream UserMergeBranchRequest) returns (stream UserMergeBranchResponse);

message UserMergeBranchRequest {
  // Repository where the merge happens.
  Repository repository = 1;
  // User performing the operation (auth + commit author).
  User user = 2;
  // Object ID of the commit to merge into the target branch.
  string commit_id = 3;
  // Target branch name (e.g. app integration branch).
  bytes branch = 4;
  // Merge commit message.
  bytes message = 5;
  // Set to true in the second message to apply the merge.
  bool apply = 6;
  // Optional timestamp for the merge commit.
  google.protobuf.Timestamp timestamp = 7;
  // Expected current OID of the branch (optimistic lock).
  string expected_old_oid = 8;
  // If true, fast-forward the squash commit instead of creating a merge commit.
  bool squash = 9;
  // Whether to sign the commit.
  bool sign = 10;
}

message UserMergeBranchResponse {
  // Merge commit OID (returned in the first response).
  string commit_id = 1;
  // Branch update details (returned in the second response after apply).
  OperationBranchUpdate branch_update = 3;
}

message OperationBranchUpdate {
  // OID of the commit the branch now points to.
  string commit_id = 1;
  // Whether this was the first branch in the repo.
  bool repo_created = 2;
  // Whether the branch was newly created (vs. updated).
  bool branch_created = 3;
}

message UserMergeBranchError {
  oneof error {
    AccessCheckError access_check = 1;
    ReferenceUpdateError reference_update = 2;
    CustomHookError custom_hook = 3;
    MergeConflictError merge_conflict = 4;
  }
}
```

**Conman usage:**

```rust
/// Fast-forward the integration branch to the composed commit using the
/// two-phase merge RPC.
///
/// Sets expected_old_oid to the current integration branch HEAD to prevent races.
/// If another release or process updated integration branch between validation and
/// publish, this will fail with a ReferenceUpdateError, which the
/// handler surfaces as a ConmanError::Conflict.
pub async fn merge_to_integration(
    &self,
    repo: &Repository,
    user: &User,
    integration_branch: &str,
    commit_id: &str,
    expected_integration_oid: &str,
    message: &str,
) -> Result<String, ConmanError> {
    // Phase 1: send merge parameters.
    let req1 = UserMergeBranchRequest {
        repository: Some(repo.clone()),
        user: Some(user.clone()),
        commit_id: commit_id.to_string(),
        branch: integration_branch.as_bytes().to_vec(),
        message: message.as_bytes().to_vec(),
        apply: false,
        expected_old_oid: expected_integration_oid.to_string(),
        ..Default::default()
    };

    // Phase 2: apply the merge.
    let req2 = UserMergeBranchRequest {
        apply: true,
        ..Default::default()
    };

    let responses = self.operation_client
        .user_merge_branch(tokio_stream::iter(vec![req1, req2]))
        .await
        .map_err(|e| self.map_grpc_error("UserMergeBranch", e))?
        .into_inner();

    // Collect both responses from the stream.
    let mut commit_id_out = String::new();
    let mut responses = responses;
    while let Some(resp) = responses.message().await.map_err(|e| {
        self.map_grpc_error("UserMergeBranch stream", e)
    })? {
        if !resp.commit_id.is_empty() {
            commit_id_out = resp.commit_id;
        }
    }

    Ok(commit_id_out)
}
```

### 7.2 `OperationService.UserMergeToRef`

Used during **composition** to merge each changeset branch into the temp
compose ref. Does not execute hooks or authorization (operates on internal
refs). If `target_ref` already exists it is overwritten.

**Proto definition** (`operations.proto`):

```protobuf
// Merge source_sha into first_parent_ref and write result to target_ref.
// Does not execute hooks. Overwrites target_ref if it exists.
rpc UserMergeToRef(UserMergeToRefRequest) returns (UserMergeToRefResponse);

message UserMergeToRefRequest {
  // Repository to perform the merge in.
  Repository repository = 1;
  // User for commit authorship.
  User user = 2;
  // Object ID of the second parent (changeset head SHA).
  string source_sha = 3;
  // Deprecated; use first_parent_ref instead.
  bytes branch = 4 [deprecated = true];
  // Fully-qualified ref to write the merge commit to.
  bytes target_ref = 5;
  // Merge commit message.
  bytes message = 6;
  // Fully-qualified ref or OID used as the first parent (integration branch line).
  bytes first_parent_ref = 7;
  // Deprecated, no longer used.
  bool allow_conflicts = 8 [deprecated = true];
  // Optional timestamp for the merge commit.
  google.protobuf.Timestamp timestamp = 9;
  // Expected OID of target_ref for optimistic locking.
  string expected_old_oid = 10;
  // Whether to sign the commit.
  bool sign = 11;
}

message UserMergeToRefResponse {
  // Object ID of the created merge commit.
  string commit_id = 1;
}
```

**Conman usage:**

```rust
/// Merge a changeset into the composition ref during release assembly.
///
/// first_parent_sha is the OID from the previous composition step
/// (or the current integration branch HEAD for the first changeset).
pub async fn merge_changeset_to_compose_ref(
    &self,
    repo: &Repository,
    user: &User,
    changeset_head_sha: &str,
    first_parent_sha: &str,
    compose_ref: &str,
    message: &str,
) -> Result<UserMergeToRefResponse, ConmanError> {
    let request = UserMergeToRefRequest {
        repository: Some(repo.clone()),
        user: Some(user.clone()),
        source_sha: changeset_head_sha.to_string(),
        target_ref: compose_ref.as_bytes().to_vec(),
        message: message.as_bytes().to_vec(),
        first_parent_ref: first_parent_sha.as_bytes().to_vec(),
        ..Default::default()
    };

    self.operation_client
        .user_merge_to_ref(request)
        .await
        .map(|r| r.into_inner())
        .map_err(|e| self.map_grpc_error("UserMergeToRef", e))
}
```

### 7.3 `OperationService.UserCreateTag`

Used during **publish** to create the lightweight release tag. Pass an
empty `message` to create a lightweight tag (vs. annotated).

**Proto definition** (`operations.proto`):

```protobuf
// Create a lightweight or annotated tag. Lightweight if message is empty.
rpc UserCreateTag(UserCreateTagRequest) returns (UserCreateTagResponse);

message UserCreateTagRequest {
  // Repository to create the tag in.
  Repository repository = 1;
  // Tag name (without refs/tags/ prefix), e.g. "r2026.02.25.1".
  bytes tag_name = 2;
  // User performing the operation.
  User user = 3;
  // Revision the tag should point to (the composed commit SHA).
  bytes target_revision = 4;
  // Tag message. Empty for lightweight tags.
  bytes message = 5;
  // Optional timestamp (only for annotated tags).
  google.protobuf.Timestamp timestamp = 7;
}

message UserCreateTagResponse {
  // The created tag object.
  Tag tag = 1;
}

message UserCreateTagError {
  oneof error {
    AccessCheckError access_check = 1;
    ReferenceUpdateError reference_update = 2;
    CustomHookError custom_hook = 3;
    ReferenceExistsError reference_exists = 4;
  }
}
```

**Conman usage:**

```rust
/// Create a lightweight release tag pointing at the composed commit.
pub async fn create_release_tag(
    &self,
    repo: &Repository,
    user: &User,
    tag_name: &str,
    target_sha: &str,
) -> Result<Tag, ConmanError> {
    let request = UserCreateTagRequest {
        repository: Some(repo.clone()),
        user: Some(user.clone()),
        tag_name: tag_name.as_bytes().to_vec(),
        target_revision: target_sha.as_bytes().to_vec(),
        message: Vec::new(), // lightweight tag
        ..Default::default()
    };

    let response = self.operation_client
        .user_create_tag(request)
        .await
        .map_err(|e| self.map_grpc_error("UserCreateTag", e))?
        .into_inner();

    response.tag.ok_or_else(|| ConmanError::Git {
        message: "UserCreateTag returned no tag".to_string(),
    })
}
```

### 7.4 `RefService.FindTag`

Used to verify tag existence before creating a new one (defensive check
in addition to the unique index).

**Proto definition** (`ref.proto`):

```protobuf
// Look up a single tag by name. Returns Internal error if not found.
rpc FindTag(FindTagRequest) returns (FindTagResponse);

message FindTagRequest {
  // Repository to look up the tag in.
  Repository repository = 1;
  // Tag name without refs/tags/ prefix (e.g. "r2026.02.25.1").
  bytes tag_name = 2;
}

message FindTagResponse {
  // The found tag object.
  Tag tag = 1;
}

message FindTagError {
  oneof error {
    // Set when the tag was not found.
    ReferenceNotFoundError tag_not_found = 1;
  }
}
```

**Conman usage:**

```rust
/// Check whether a tag already exists in Git.
///
/// Returns Ok(Some(tag)) if found, Ok(None) if not found,
/// Err for unexpected gRPC failures.
pub async fn find_tag(
    &self,
    repo: &Repository,
    tag_name: &str,
) -> Result<Option<Tag>, ConmanError> {
    let request = FindTagRequest {
        repository: Some(repo.clone()),
        tag_name: tag_name.as_bytes().to_vec(),
    };

    match self.ref_client.find_tag(request).await {
        Ok(response) => Ok(response.into_inner().tag),
        Err(status) if status.code() == tonic::Code::Internal => {
            // FindTag returns Internal when tag is not found.
            Ok(None)
        }
        Err(e) => Err(self.map_grpc_error("FindTag", e)),
    }
}
```

### 7.5 `RefService.FindAllTags`

Used for tag numbering: list all tags matching the release prefix for today
to determine the next sequence number.

**Proto definition** (`ref.proto`):

```protobuf
// Stream all tags under refs/tags/ for a repository.
rpc FindAllTags(FindAllTagsRequest) returns (stream FindAllTagsResponse);

message FindAllTagsRequest {
  message SortBy {
    enum Key {
      REFNAME = 0;
      CREATORDATE = 1;
      VERSION_REFNAME = 2;
    }
    Key key = 1;
    SortDirection direction = 2;
  }

  // Repository to list tags from.
  Repository repository = 1;
  // Optional sort order.
  SortBy sort_by = 2;
  // Optional pagination.
  PaginationParameter pagination_params = 3;
}

message FindAllTagsResponse {
  // List of tags in this chunk.
  repeated Tag tags = 1;
}
```

**Conman usage:**

```rust
/// List all tags in the repository, collecting them from the response stream.
///
/// Used to find existing release tags for sequence number generation.
/// Filters client-side by the `rYYYY.MM.DD.` prefix for today's date.
pub async fn list_all_tags(
    &self,
    repo: &Repository,
) -> Result<Vec<Tag>, ConmanError> {
    let request = FindAllTagsRequest {
        repository: Some(repo.clone()),
        sort_by: None,
        pagination_params: None,
    };

    let mut stream = self.ref_client
        .find_all_tags(request)
        .await
        .map_err(|e| self.map_grpc_error("FindAllTags", e))?
        .into_inner();

    let mut tags = Vec::new();
    while let Some(response) = stream.message().await.map_err(|e| {
        self.map_grpc_error("FindAllTags stream", e)
    })? {
        tags.extend(response.tags);
    }

    Ok(tags)
}
```

### 7.6 `CommitService.FindCommit`

Used to resolve the current `integration_branch` HEAD SHA before composition begins and
to verify commit existence during publish.

**Proto definition** (`commit.proto`):

```protobuf
// Find a commit by commitish. Returns nil commit if not found.
rpc FindCommit(FindCommitRequest) returns (FindCommitResponse);

message FindCommitRequest {
  // Repository to search in.
  Repository repository = 1;
  // Commitish to resolve (e.g. "refs/heads/<integration_branch>", a SHA, a tag name).
  bytes revision = 2;
  // If true, parse and include Git trailers.
  bool trailers = 3;
}

message FindCommitResponse {
  // The resolved commit, or nil if not found.
  GitCommit commit = 1;
}
```

**Conman usage:**

```rust
/// Resolve a commitish to a full GitCommit object.
///
/// Returns NotFound if the revision does not resolve to a commit.
pub async fn find_commit(
    &self,
    repo: &Repository,
    revision: &str,
) -> Result<GitCommit, ConmanError> {
    let request = FindCommitRequest {
        repository: Some(repo.clone()),
        revision: revision.as_bytes().to_vec(),
        trailers: false,
    };

    let response = self.commit_client
        .find_commit(request)
        .await
        .map_err(|e| self.map_grpc_error("FindCommit", e))?
        .into_inner();

    response.commit.ok_or_else(|| ConmanError::NotFound {
        entity: "commit",
        id: revision.to_string(),
    })
}
```

## 8. Implementation Checklist

Each step is one commit. Follow TDD: write test, run test (fails), implement,
run test (passes), commit.

- [ ] **E08-S01** -- Add `ReleaseState` enum to `conman-core`.
  Add `release.rs` with `ReleaseState` enum, `Display` impl, serde derives.
  Write unit tests for serialization round-trip and `Display` output.

- [ ] **E08-S02** -- Add `ReleaseBatch` and `ReleaseChangeset` structs to `conman-core`.
  Define both domain structs with all fields. Write unit tests for default
  construction and serde round-trip.

- [ ] **E08-S03** -- Implement `ReleaseState::transition()` and `TransitionGuard`.
  Write the state machine function with all valid transitions and guards.
  Write exhaustive tests for every valid transition and every invalid
  transition (negative tests).

- [ ] **E08-S04** -- Add `ReleaseRepo` to `conman-db`.
  Implement repository with `insert`, `find_by_id`, `find_by_app_and_tag`,
  `list_by_app` (with state filter and pagination), `update_state`,
  `set_published_metadata`, `find_max_tag_sequence`. Add `ensure_indexes`.
  Write integration tests against MongoDB.

- [ ] **E08-S05** -- Add `ReleaseChangesetRepo` to `conman-db`.
  Implement repository with `insert_batch`, `find_by_release_ordered`,
  `delete_by_changeset_ids`, `update_positions`, `set_merge_sha`.
  Add `ensure_indexes`. Write integration tests.

- [ ] **E08-S06** -- Add API DTOs for releases to `conman-api`.
  Implement `CreateReleaseRequest`, `AddChangesetsRequest`, `ReorderRequest`,
  `ReleaseResponse`, `ReleaseListQuery`, and the `From<&ReleaseBatch>`
  conversion. Write unit tests for deserialization and serialization.

- [ ] **E08-S07** -- Implement `next_tag()` tag generation logic.
  Add to `conman-core` or a service layer. Write unit tests for first tag
  of the day, incrementing, and date boundary behavior.

- [ ] **E08-S08** -- Implement `POST /api/repos/:appId/releases` handler.
  Create draft release with tag generation, changeset validation, and
  `ReleaseChangeset` insertion. Write integration test: create release with
  two queued changesets, verify 201 response and database state.

- [ ] **E08-S09** -- Implement `GET /api/repos/:appId/releases` handler.
  List with pagination and optional state filter. Write integration tests
  for pagination, state filtering, and empty results.

- [ ] **E08-S10** -- Implement `GET /api/repos/:appId/releases/:releaseId` handler.
  Return release detail with `changesets` array. Write integration test.

- [ ] **E08-S11** -- Implement `POST /api/repos/:appId/releases/:releaseId/changesets` handler.
  Add/remove changesets from draft. Write integration tests for add, remove,
  state guard rejection, and duplicate detection.

- [ ] **E08-S12** -- Implement `POST /api/repos/:appId/releases/:releaseId/reorder` handler.
  Validate permutation and update positions. Write integration tests for
  valid reorder, non-permutation rejection, and state guard.

- [ ] **E08-S13** -- Implement `POST /api/repos/:appId/releases/:releaseId/assemble` handler.
  Transition to `Assembling`, create job, return 202. Write integration test
  verifying state transition and job creation.

- [ ] **E08-S14** -- Implement `release_assemble` job worker in `conman-jobs`.
  Sequential merge via `UserMergeToRef`, conflict handling, test invocation,
  state transitions on success/failure. Write integration tests with mock
  gitaly: success path, conflict path, test failure path.

- [ ] **E08-S15** -- Add gitaly-rs methods for release operations to `conman-git`.
  Implement `merge_to_main`, `merge_changeset_to_compose_ref`,
  `create_release_tag`, `find_tag`, `list_all_tags`, `find_commit`.
  Write unit tests with mock gRPC server for each method.

- [ ] **E08-S16** -- Implement `POST /api/repos/:appId/releases/:releaseId/publish` handler.
  Full publish flow: merge to the integration branch, create tag, persist metadata, mark
  changesets released, trigger revalidation. Write integration tests with
  mock gitaly for success and race condition (expected_old_oid mismatch).

- [ ] **E08-S17** -- Add audit events for all release mutations.
  Emit audit events for `release.created`, `release.changesets_modified`,
  `release.reordered`, `release.assembly_started`, `release.published`.
  Write integration tests verifying audit documents are created.

- [ ] **E08-S18** -- Wire all release routes into the Axum router.
  Replace the 501 stubs from E00 with the implemented handlers. Write
  smoke test hitting each endpoint.

- [ ] **E08-S19** -- Bootstrap `release_batches` and `release_changesets` indexes at startup.
  Register both repos in `bootstrap_indexes()`. Verify indexes are created
  on a fresh database.

## 9. Test Cases

### 9.1 State Machine: Valid Transitions

```rust
#[test]
fn draft_to_assembling_with_changesets() {
    let guard = TransitionGuard { has_changesets: true, ..Default::default() };
    let result = ReleaseState::DraftRelease.transition(ReleaseState::Assembling, &guard);
    assert_eq!(result.unwrap(), ReleaseState::Assembling);
}

#[test]
fn assembling_to_validated_on_success() {
    let guard = TransitionGuard { compose_succeeded: true, ..Default::default() };
    let result = ReleaseState::Assembling.transition(ReleaseState::Validated, &guard);
    assert_eq!(result.unwrap(), ReleaseState::Validated);
}

#[test]
fn assembling_to_draft_on_failure() {
    let guard = TransitionGuard { compose_failed: true, ..Default::default() };
    let result = ReleaseState::Assembling.transition(ReleaseState::DraftRelease, &guard);
    assert_eq!(result.unwrap(), ReleaseState::DraftRelease);
}

#[test]
fn validated_to_published() {
    let guard = TransitionGuard { tag_created: true, ..Default::default() };
    let result = ReleaseState::Validated.transition(ReleaseState::Published, &guard);
    assert_eq!(result.unwrap(), ReleaseState::Published);
}

#[test]
fn published_to_deployed_partial() {
    let guard = TransitionGuard::default();
    let result = ReleaseState::Published.transition(ReleaseState::DeployedPartial, &guard);
    assert_eq!(result.unwrap(), ReleaseState::DeployedPartial);
}

#[test]
fn deployed_partial_to_deployed_full() {
    let guard = TransitionGuard { all_envs_deployed: true, ..Default::default() };
    let result = ReleaseState::DeployedPartial.transition(ReleaseState::DeployedFull, &guard);
    assert_eq!(result.unwrap(), ReleaseState::DeployedFull);
}

#[test]
fn deployed_partial_stays_partial_when_not_all_envs() {
    let guard = TransitionGuard { all_envs_deployed: false, ..Default::default() };
    let result = ReleaseState::DeployedPartial.transition(ReleaseState::DeployedPartial, &guard);
    assert_eq!(result.unwrap(), ReleaseState::DeployedPartial);
}

#[test]
fn published_to_rolled_back() {
    let guard = TransitionGuard::default();
    let result = ReleaseState::Published.transition(ReleaseState::RolledBack, &guard);
    assert_eq!(result.unwrap(), ReleaseState::RolledBack);
}

#[test]
fn deployed_full_to_rolled_back() {
    let guard = TransitionGuard::default();
    let result = ReleaseState::DeployedFull.transition(ReleaseState::RolledBack, &guard);
    assert_eq!(result.unwrap(), ReleaseState::RolledBack);
}
```

### 9.2 State Machine: Invalid Transitions

```rust
#[test]
fn draft_to_assembling_without_changesets_is_rejected() {
    let guard = TransitionGuard { has_changesets: false, ..Default::default() };
    let result = ReleaseState::DraftRelease.transition(ReleaseState::Assembling, &guard);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("invalid state transition"));
}

#[test]
fn draft_to_published_is_rejected() {
    let guard = TransitionGuard::default();
    let result = ReleaseState::DraftRelease.transition(ReleaseState::Published, &guard);
    assert!(result.is_err());
}

#[test]
fn published_to_draft_is_rejected() {
    let guard = TransitionGuard::default();
    let result = ReleaseState::Published.transition(ReleaseState::DraftRelease, &guard);
    assert!(result.is_err());
}

#[test]
fn published_to_assembling_is_rejected() {
    let guard = TransitionGuard::default();
    let result = ReleaseState::Published.transition(ReleaseState::Assembling, &guard);
    assert!(result.is_err());
}

#[test]
fn validated_to_draft_is_rejected() {
    let guard = TransitionGuard::default();
    let result = ReleaseState::Validated.transition(ReleaseState::DraftRelease, &guard);
    assert!(result.is_err());
}

#[test]
fn rolled_back_to_any_is_rejected() {
    let guard = TransitionGuard { tag_created: true, has_changesets: true, ..Default::default() };
    for target in [
        ReleaseState::DraftRelease,
        ReleaseState::Assembling,
        ReleaseState::Validated,
        ReleaseState::Published,
        ReleaseState::DeployedPartial,
        ReleaseState::DeployedFull,
    ] {
        let result = ReleaseState::RolledBack.transition(target, &guard);
        assert!(result.is_err(), "RolledBack -> {target} should be rejected");
    }
}
```

### 9.3 Tag Generation

```rust
#[tokio::test]
async fn first_tag_of_the_day_is_1() {
    let repo = test_release_repo().await;
    let tag = next_tag(&repo, test_app_id()).await.unwrap();
    let today = Utc::now().format("%Y.%m.%d");
    assert_eq!(tag, format!("r{today}.1"));
}

#[tokio::test]
async fn second_tag_of_the_day_is_2() {
    let repo = test_release_repo().await;
    let app_id = test_app_id();

    // Insert a release with today's first tag.
    insert_release(&repo, app_id, &format!("r{}.1", Utc::now().format("%Y.%m.%d"))).await;

    let tag = next_tag(&repo, app_id).await.unwrap();
    let today = Utc::now().format("%Y.%m.%d");
    assert_eq!(tag, format!("r{today}.2"));
}

#[tokio::test]
async fn tags_from_different_days_dont_affect_sequence() {
    let repo = test_release_repo().await;
    let app_id = test_app_id();

    // Insert a release from yesterday.
    insert_release(&repo, app_id, "r2026.02.24.5").await;

    let tag = next_tag(&repo, app_id).await.unwrap();
    let today = Utc::now().format("%Y.%m.%d");
    assert_eq!(tag, format!("r{today}.1"));
}
```

### 9.4 Create Draft Release

```rust
#[tokio::test]
async fn create_release_returns_201_with_tag() {
    let app = test_app_with_real_mongo().await;
    let app_id = seed_app(&app).await;
    let cs1 = seed_queued_changeset(&app, app_id).await;
    let cs2 = seed_queued_changeset(&app, app_id).await;

    let response = app.oneshot(
        Request::builder()
            .method("POST")
            .uri(&format!("/api/repos/{app_id}/releases"))
            .header("content-type", "application/json")
            .header("authorization", config_manager_token())
            .body(Body::from(json!({
                "changeset_ids": [cs1.to_hex(), cs2.to_hex()]
            }).to_string()))
            .unwrap()
    ).await.unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body: serde_json::Value = parse_body(response).await;
    assert_eq!(body["data"]["state"], "draft_release");
    assert!(body["data"]["tag"].as_str().unwrap().starts_with("r"));
    assert_eq!(body["data"]["ordered_changeset_ids"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn create_release_rejects_non_queued_changesets() {
    let app = test_app_with_real_mongo().await;
    let app_id = seed_app(&app).await;
    let draft_cs = seed_draft_changeset(&app, app_id).await;

    let response = app.oneshot(
        Request::builder()
            .method("POST")
            .uri(&format!("/api/repos/{app_id}/releases"))
            .header("content-type", "application/json")
            .header("authorization", config_manager_token())
            .body(Body::from(json!({
                "changeset_ids": [draft_cs.to_hex()]
            }).to_string()))
            .unwrap()
    ).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_release_rejects_user_role() {
    let app = test_app_with_real_mongo().await;
    let app_id = seed_app(&app).await;

    let response = app.oneshot(
        Request::builder()
            .method("POST")
            .uri(&format!("/api/repos/{app_id}/releases"))
            .header("content-type", "application/json")
            .header("authorization", user_token()) // not config_manager
            .body(Body::from(json!({ "changeset_ids": [] }).to_string()))
            .unwrap()
    ).await.unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
```

### 9.5 Add/Remove Changesets

```rust
#[tokio::test]
async fn add_changeset_to_draft_release() {
    let app = test_app_with_real_mongo().await;
    let (app_id, release_id) = seed_draft_release(&app).await;
    let cs = seed_queued_changeset(&app, app_id).await;

    let response = app.oneshot(
        Request::builder()
            .method("POST")
            .uri(&format!("/api/repos/{app_id}/releases/{release_id}/changesets"))
            .header("content-type", "application/json")
            .header("authorization", config_manager_token())
            .body(Body::from(json!({
                "add": [cs.to_hex()],
                "remove": []
            }).to_string()))
            .unwrap()
    ).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = parse_body(response).await;
    assert!(body["data"]["ordered_changeset_ids"]
        .as_array().unwrap()
        .iter()
        .any(|id| id.as_str() == Some(&cs.to_hex())));
}

#[tokio::test]
async fn modify_changesets_rejected_for_non_draft_release() {
    let app = test_app_with_real_mongo().await;
    let (app_id, release_id) = seed_published_release(&app).await;
    let cs = seed_queued_changeset(&app, app_id).await;

    let response = app.oneshot(
        Request::builder()
            .method("POST")
            .uri(&format!("/api/repos/{app_id}/releases/{release_id}/changesets"))
            .header("content-type", "application/json")
            .header("authorization", config_manager_token())
            .body(Body::from(json!({
                "add": [cs.to_hex()],
                "remove": []
            }).to_string()))
            .unwrap()
    ).await.unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}
```

### 9.6 Reorder Changesets

```rust
#[tokio::test]
async fn reorder_changesets_updates_positions() {
    let app = test_app_with_real_mongo().await;
    let (app_id, release_id, cs1, cs2) = seed_draft_release_with_two_changesets(&app).await;

    let response = app.oneshot(
        Request::builder()
            .method("POST")
            .uri(&format!("/api/repos/{app_id}/releases/{release_id}/reorder"))
            .header("content-type", "application/json")
            .header("authorization", config_manager_token())
            .body(Body::from(json!({
                "ordered_changeset_ids": [cs2.to_hex(), cs1.to_hex()]
            }).to_string()))
            .unwrap()
    ).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = parse_body(response).await;
    let ids = body["data"]["ordered_changeset_ids"].as_array().unwrap();
    assert_eq!(ids[0].as_str().unwrap(), cs2.to_hex());
    assert_eq!(ids[1].as_str().unwrap(), cs1.to_hex());
}

#[tokio::test]
async fn reorder_rejects_non_permutation() {
    let app = test_app_with_real_mongo().await;
    let (app_id, release_id, cs1, _cs2) = seed_draft_release_with_two_changesets(&app).await;

    let response = app.oneshot(
        Request::builder()
            .method("POST")
            .uri(&format!("/api/repos/{app_id}/releases/{release_id}/reorder"))
            .header("content-type", "application/json")
            .header("authorization", config_manager_token())
            .body(Body::from(json!({
                "ordered_changeset_ids": [cs1.to_hex()]
            }).to_string()))
            .unwrap()
    ).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
```

### 9.7 Assembly Worker

```rust
#[tokio::test]
async fn assembly_worker_composes_changesets_in_order() {
    let ctx = test_worker_context_with_mock_gitaly().await;

    // Mock gitaly: UserMergeToRef succeeds for both changesets.
    ctx.mock_gitaly.expect_user_merge_to_ref()
        .times(2)
        .returning(|_| Ok(UserMergeToRefResponse { commit_id: "abc123".to_string() }));

    let job = seed_assembly_job(&ctx, 2).await;
    let result = execute_release_assemble(&job, &ctx).await;

    assert!(result.is_ok());

    // Verify release is now Validated.
    let release = ctx.release_repo.find_by_id(job.entity_id).await.unwrap();
    assert_eq!(release.state, ReleaseState::Validated);

    // Verify merge SHAs were recorded.
    let rcs = ctx.release_changeset_repo.find_by_release_ordered(release.id).await.unwrap();
    assert!(rcs.iter().all(|rc| rc.merge_sha.is_some()));
}

#[tokio::test]
async fn assembly_worker_marks_changeset_conflicted_on_merge_failure() {
    let ctx = test_worker_context_with_mock_gitaly().await;

    // First merge succeeds, second conflicts.
    ctx.mock_gitaly.expect_user_merge_to_ref()
        .times(1)
        .returning(|_| Ok(UserMergeToRefResponse { commit_id: "abc123".to_string() }));
    ctx.mock_gitaly.expect_user_merge_to_ref()
        .times(1)
        .returning(|_| Err(merge_conflict_error()));

    let job = seed_assembly_job(&ctx, 2).await;
    let result = execute_release_assemble(&job, &ctx).await;

    assert!(result.is_err());

    // Verify release is back to DraftRelease.
    let release = ctx.release_repo.find_by_id(job.entity_id).await.unwrap();
    assert_eq!(release.state, ReleaseState::DraftRelease);

    // Verify the second changeset is marked conflicted.
    let rcs = ctx.release_changeset_repo.find_by_release_ordered(release.id).await.unwrap();
    let cs = ctx.changeset_repo.find_by_id(rcs[1].changeset_id).await.unwrap();
    assert_eq!(cs.state, ChangesetState::Conflicted);
}
```

### 9.8 Publish Flow

```rust
#[tokio::test]
async fn publish_creates_tag_and_updates_main() {
    let app = test_app_with_mock_gitaly().await;
    let (app_id, release_id) = seed_validated_release(&app).await;

    // Mock: FindCommit returns current integration branch HEAD.
    app.mock_gitaly.expect_find_commit()
        .returning(|_| Ok(test_git_commit("old_main_sha")));

    // Mock: UserMergeBranch succeeds.
    app.mock_gitaly.expect_user_merge_branch()
        .returning(|_| Ok(test_merge_response("new_sha")));

    // Mock: UserCreateTag succeeds.
    app.mock_gitaly.expect_user_create_tag()
        .returning(|_| Ok(test_tag_response()));

    let response = app.oneshot(
        Request::builder()
            .method("POST")
            .uri(&format!("/api/repos/{app_id}/releases/{release_id}/publish"))
            .header("authorization", config_manager_token())
            .body(Body::empty())
            .unwrap()
    ).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = parse_body(response).await;
    assert_eq!(body["data"]["state"], "published");
    assert!(body["data"]["published_sha"].is_string());
    assert!(body["data"]["published_at"].is_string());
}

#[tokio::test]
async fn publish_fails_on_race_condition() {
    let app = test_app_with_mock_gitaly().await;
    let (app_id, release_id) = seed_validated_release(&app).await;

    // Mock: FindCommit returns integration branch HEAD.
    app.mock_gitaly.expect_find_commit()
        .returning(|_| Ok(test_git_commit("old_main_sha")));

    // Mock: UserMergeBranch fails with reference update error (race).
    app.mock_gitaly.expect_user_merge_branch()
        .returning(|_| Err(reference_update_error()));

    let response = app.oneshot(
        Request::builder()
            .method("POST")
            .uri(&format!("/api/repos/{app_id}/releases/{release_id}/publish"))
            .header("authorization", config_manager_token())
            .body(Body::empty())
            .unwrap()
    ).await.unwrap();

    // Race condition surfaces as a conflict error.
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn publish_rejects_non_validated_release() {
    let app = test_app_with_real_mongo().await;
    let (app_id, release_id) = seed_draft_release(&app).await;

    let response = app.oneshot(
        Request::builder()
            .method("POST")
            .uri(&format!("/api/repos/{app_id}/releases/{release_id}/publish"))
            .header("authorization", config_manager_token())
            .body(Body::empty())
            .unwrap()
    ).await.unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}
```

### 9.9 Immutability

```rust
#[tokio::test]
async fn published_release_rejects_changeset_modification() {
    let app = test_app_with_real_mongo().await;
    let (app_id, release_id) = seed_published_release(&app).await;

    // Attempt to add a changeset.
    let response = app.oneshot(
        Request::builder()
            .method("POST")
            .uri(&format!("/api/repos/{app_id}/releases/{release_id}/changesets"))
            .header("content-type", "application/json")
            .header("authorization", config_manager_token())
            .body(Body::from(json!({ "add": ["aabbcc"], "remove": [] }).to_string()))
            .unwrap()
    ).await.unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn published_release_rejects_reorder() {
    let app = test_app_with_real_mongo().await;
    let (app_id, release_id) = seed_published_release(&app).await;

    let response = app.oneshot(
        Request::builder()
            .method("POST")
            .uri(&format!("/api/repos/{app_id}/releases/{release_id}/reorder"))
            .header("content-type", "application/json")
            .header("authorization", config_manager_token())
            .body(Body::from(json!({ "ordered_changeset_ids": [] }).to_string()))
            .unwrap()
    ).await.unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn published_release_rejects_re_assembly() {
    let app = test_app_with_real_mongo().await;
    let (app_id, release_id) = seed_published_release(&app).await;

    let response = app.oneshot(
        Request::builder()
            .method("POST")
            .uri(&format!("/api/repos/{app_id}/releases/{release_id}/assemble"))
            .header("authorization", config_manager_token())
            .body(Body::empty())
            .unwrap()
    ).await.unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}
```

## 10. Acceptance Criteria

1. **Draft release creation with subset selection.**
   - A `config_manager` can create a draft release, optionally selecting
     a subset of queued changesets.
   - Only changesets in `queued` state are accepted.
   - The tag `rYYYY.MM.DD.N` is auto-generated and unique per app.
   - Users with `user` or `reviewer` role receive 403 Forbidden.

2. **Changeset selection and reordering.**
   - Changesets can be added to or removed from a draft release.
   - The merge order can be set via the reorder endpoint.
   - Both operations are rejected for releases not in `draft_release` state.

3. **Composition engine merges in specified order.**
   - The assembly worker merges each changeset sequentially onto `integration_branch`
     via a temp ref using `UserMergeToRef`.
   - On merge conflict, the specific changeset is marked `conflicted` and
     the release returns to `draft_release`.
   - On test failure, the changeset is marked `needs_revalidation`.
   - On full success, the release transitions to `validated`.

4. **Publish creates an immutable tagged artifact.**
   - Publish fast-forwards `refs/heads/<integration_branch>` to the composed commit.
   - A lightweight Git tag (`rYYYY.MM.DD.N`) is created.
   - `published_sha`, `published_at`, and `published_by` are recorded.
   - All included changesets transition to `released`.
   - Race conditions (concurrent integration branch updates) are detected via
     `expected_old_oid` and surfaced as conflict errors.

5. **Post-publish revalidation is triggered.**
   - After publish, `revalidate_queued_changeset` jobs are enqueued for
     all remaining queued changesets in the app.
   - Revalidation checks conflict and test status against the new `integration_branch`.

6. **Immutability is enforced after publish.**
   - Published releases reject changeset modification, reordering, and
     re-assembly attempts.
   - Only forward state transitions (to deployment states or rollback)
     are permitted.

7. **Audit trail is complete.**
   - Every mutation emits an audit event: creation, changeset modification,
     reordering, assembly start, and publish.
   - Audit events include before/after state, actor, and Git SHA where
     applicable.

8. **Env-profile validation gate at publish.**
   - Release publish runs required environment-profile validation jobs.
   - Publish is blocked when the configured env-profile validation fails.
