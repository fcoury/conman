# E09 Deploy, Promote, Skip-Stage, Rollback

## 1. Goal

Deliver environment movement and recovery workflows for published releases.
After a release is published (E08), config managers need to deploy it to
environments, promote it across the pipeline (Dev -> QA -> UAT -> Prod), and
recover from bad deployments via two rollback strategies. Skip-stage and
concurrent multi-environment deployments are exceptional flows requiring
two-user approval. Deploy operations are blocked on runtime profile drift until
revalidation passes.

This epic introduces the `Deployment` domain object, environment-scoped locking,
the `deploy_release` async job worker, and the Git-level operations for rollback
mode A (revert on the integration branch + new release).

## 2. Dependencies

| Epic | What it provides |
|------|-----------------|
| E03 | `App`, `Environment` domain types, environment CRUD, pipeline ordering |
| E06 | Async job framework (`jobs` collection, runner, worker trait) |
| E08 | `Release` domain type with `published` state, release tag format `rYYYY.MM.DD.N` |

## 3. Rust Types

### 3.1 DeploymentState (`conman-core/src/deployment.rs`)

State machine governing the lifecycle of a single deployment.

```rust
use serde::{Deserialize, Serialize};

/// State of a deployment progressing through execution.
///
/// ```text
/// Pending -> Running -> Succeeded
///                    -> Failed
///                    -> Canceled
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentState {
    /// Created and waiting for the job runner to pick it up.
    Pending,
    /// Deploy job is actively executing.
    Running,
    /// Deployment completed successfully.
    Succeeded,
    /// Deployment failed (job error or external failure).
    Failed,
    /// Deployment was canceled before completion.
    Canceled,
}

impl DeploymentState {
    /// Return whether this state represents an active (non-terminal) deployment.
    /// Used for environment lock checking -- only one active deployment per env.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Pending | Self::Running)
    }

    /// Validate that a transition from the current state to `to` is legal.
    pub fn can_transition_to(&self, to: DeploymentState) -> bool {
        matches!(
            (self, to),
            (Self::Pending, Self::Running)
                | (Self::Pending, Self::Canceled)
                | (Self::Running, Self::Succeeded)
                | (Self::Running, Self::Failed)
                | (Self::Running, Self::Canceled)
        )
    }
}
```

### 3.2 RollbackMode (`conman-core/src/deployment.rs`)

```rust
/// Strategy used to roll back a problematic deployment.
///
/// - `RevertAndRelease`: create a revert commit on the integration branch for the release's
///   composed changes, then create and publish a new release from that revert.
///   This alters Git history on the integration branch.
/// - `RedeployPriorTag`: redeploy an earlier release tag to the target
///   environment. No Git changes are made; only the deployment record changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RollbackMode {
    /// Revert the release's changes on the integration branch, then create + publish a new release.
    RevertAndRelease,
    /// Redeploy a prior release tag to the environment without Git mutations.
    RedeployPriorTag,
}
```

### 3.3 Deployment (`conman-core/src/deployment.rs`)

```rust
use bson::oid::ObjectId;
use chrono::{DateTime, Utc};

/// A single deployment of a release to an environment.
///
/// Each deployment is backed by an async `deploy_release` job. The
/// environment lock ensures at most one active deployment per environment
/// at any time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deployment {
    /// Unique identifier.
    #[serde(rename = "_id")]
    pub id: ObjectId,

    /// The app this deployment belongs to.
    pub app_id: ObjectId,

    /// Target environment for the deployment.
    pub environment_id: ObjectId,

    /// The release being deployed (immutable artifact).
    pub release_id: ObjectId,

    /// Current lifecycle state.
    pub state: DeploymentState,

    /// Whether this deployment skipped one or more pipeline stages.
    pub skip_stage: bool,

    /// User IDs who approved this deployment. Required (2 distinct users,
    /// at least one privileged) for skip-stage or concurrent deploy flows.
    /// Empty for normal promotions.
    pub approval_user_ids: Vec<ObjectId>,

    /// Reference to structured deployment logs (stored in the job).
    pub logs_ref: Option<String>,

    /// The async job driving this deployment.
    pub job_id: Option<ObjectId>,

    /// When the deployment record was created.
    pub created_at: DateTime<Utc>,

    /// When the deploy job transitioned to Running.
    pub started_at: Option<DateTime<Utc>>,

    /// When the deploy job reached a terminal state.
    pub completed_at: Option<DateTime<Utc>>,

    /// If this is a rollback deployment, the mode used.
    pub rollback_mode: Option<RollbackMode>,

    /// For RedeployPriorTag rollbacks, the original release being redeployed.
    /// For RevertAndRelease rollbacks, the release whose changes were reverted.
    pub rollback_source_release_id: Option<ObjectId>,
}
```

### 3.4 DeployApproval (`conman-core/src/deployment.rs`)

```rust
use conman_core::auth::Role;

/// Represents an approval for an exceptional deployment flow
/// (skip-stage or concurrent multi-environment deploy).
///
/// Validation rules:
/// - Exactly 2 approvals from distinct users.
/// - At least one approver must hold `reviewer`, `config_manager`, or `app_admin`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployApproval {
    /// The user granting approval.
    pub user_id: ObjectId,

    /// The role the user holds on this app at approval time.
    pub role: Role,

    /// When the approval was recorded.
    pub approved_at: DateTime<Utc>,
}

impl DeployApproval {
    /// Validate that a set of approvals satisfies the exceptional deploy rules.
    ///
    /// Returns `Ok(())` if:
    /// - There are at least 2 approvals.
    /// - All user IDs are distinct.
    /// - At least one approver has a privileged role.
    pub fn validate_approvals(approvals: &[DeployApproval]) -> Result<(), ConmanError> {
        // Must have at least 2 approvals.
        if approvals.len() < 2 {
            return Err(ConmanError::Validation {
                message: "skip-stage and concurrent deploys require at least 2 approvals".into(),
            });
        }

        // All approvers must be distinct users.
        let unique_users: HashSet<ObjectId> = approvals.iter().map(|a| a.user_id).collect();
        if unique_users.len() < 2 {
            return Err(ConmanError::Validation {
                message: "approvals must come from distinct users".into(),
            });
        }

        // At least one approver must hold a privileged role.
        let has_privileged = approvals.iter().any(|a| {
            matches!(a.role, Role::Reviewer | Role::ConfigManager | Role::AppAdmin)
        });
        if !has_privileged {
            return Err(ConmanError::Validation {
                message: "at least one approver must be reviewer, config_manager, or app_admin"
                    .into(),
            });
        }

        Ok(())
    }
}
```

### 3.5 API Request / Response Types (`conman-api/src/handlers/deployments.rs`)

```rust
use bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// POST /api/apps/:appId/environments/:envId/deploy
///
/// Deploy a published release to the specified environment.
#[derive(Debug, Deserialize)]
pub struct DeployRequest {
    /// The release to deploy. Must be in `published` (or later) state.
    pub release_id: ObjectId,

    /// If true, this deployment skips one or more pipeline stages.
    /// Requires `approvals` to be populated.
    #[serde(default)]
    pub skip_stage: bool,

    /// Approval records for skip-stage or concurrent deploy.
    /// Ignored for normal sequential promotions.
    #[serde(default)]
    pub approvals: Vec<DeployApprovalInput>,
}

/// Approval input for exceptional deploy flows.
#[derive(Debug, Deserialize)]
pub struct DeployApprovalInput {
    pub user_id: ObjectId,
}

/// POST /api/apps/:appId/environments/:envId/promote
///
/// Promote the currently deployed release to the next environment in
/// the pipeline. The release artifact is immutable; only the target
/// environment changes.
#[derive(Debug, Deserialize)]
pub struct PromoteRequest {
    /// The release to promote. Must already be deployed to a prior stage.
    pub release_id: ObjectId,

    /// If true, this promotion skips one or more stages.
    #[serde(default)]
    pub skip_stage: bool,

    /// Approval records required when skip_stage is true.
    #[serde(default)]
    pub approvals: Vec<DeployApprovalInput>,
}

/// POST /api/apps/:appId/environments/:envId/rollback
///
/// Roll back the environment to a previous state.
#[derive(Debug, Deserialize)]
pub struct RollbackRequest {
    /// Which rollback strategy to use.
    pub mode: RollbackMode,

    /// For `RedeployPriorTag`: the release to redeploy.
    /// For `RevertAndRelease`: the release whose changes to revert on the integration branch.
    pub target_release_id: ObjectId,
}

/// Deployment response returned by all deployment endpoints.
#[derive(Debug, Serialize)]
pub struct DeploymentResponse {
    pub id: String,
    pub app_id: String,
    pub environment_id: String,
    pub release_id: String,
    pub state: DeploymentState,
    pub skip_stage: bool,
    pub approval_user_ids: Vec<String>,
    pub logs_ref: Option<String>,
    pub job_id: Option<String>,
    pub rollback_mode: Option<RollbackMode>,
    pub rollback_source_release_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl From<Deployment> for DeploymentResponse {
    fn from(d: Deployment) -> Self {
        Self {
            id: d.id.to_hex(),
            app_id: d.app_id.to_hex(),
            environment_id: d.environment_id.to_hex(),
            release_id: d.release_id.to_hex(),
            state: d.state,
            skip_stage: d.skip_stage,
            approval_user_ids: d.approval_user_ids.iter().map(|id| id.to_hex()).collect(),
            logs_ref: d.logs_ref,
            job_id: d.job_id.map(|id| id.to_hex()),
            rollback_mode: d.rollback_mode,
            rollback_source_release_id: d.rollback_source_release_id.map(|id| id.to_hex()),
            created_at: d.created_at,
            started_at: d.started_at,
            completed_at: d.completed_at,
        }
    }
}
```

## 4. Database

### 4.1 Collection: `deployments`

Stores every deployment attempt. Documents are never deleted; terminal states
are immutable records.

**Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `_id` | `ObjectId` | Primary key |
| `app_id` | `ObjectId` | Parent app |
| `environment_id` | `ObjectId` | Target environment |
| `release_id` | `ObjectId` | Release being deployed |
| `state` | `string` | One of: `pending`, `running`, `succeeded`, `failed`, `canceled` |
| `skip_stage` | `bool` | Whether this deployment skipped pipeline stages |
| `approval_user_ids` | `ObjectId[]` | Users who approved (for skip-stage/concurrent) |
| `logs_ref` | `string?` | Job log reference |
| `job_id` | `ObjectId?` | Associated async job |
| `rollback_mode` | `string?` | `revert_and_release` or `redeploy_prior_tag` |
| `rollback_source_release_id` | `ObjectId?` | Source release for rollback |
| `created_at` | `DateTime` | Record creation time |
| `started_at` | `DateTime?` | When job began running |
| `completed_at` | `DateTime?` | When job reached terminal state |

**Indexes:**

```rust
async fn ensure_indexes(&self) -> Result<(), ConmanError> {
    let collection = self.collection();

    // Index 1: Environment lock check -- find active deployments per env.
    // Query pattern: { app_id, environment_id, state: { $in: ["pending", "running"] } }
    collection
        .create_index(
            IndexModel::builder()
                .keys(doc! {
                    "app_id": 1,
                    "environment_id": 1,
                    "state": 1,
                })
                .build(),
        )
        .await
        .map_err(|e| ConmanError::Internal {
            message: format!("failed to create deployments env lock index: {e}"),
        })?;

    // Index 2: List deployments by release (e.g. to check which envs a release
    // has been deployed to, or to find the latest deployment of a release).
    collection
        .create_index(
            IndexModel::builder()
                .keys(doc! { "release_id": 1, "created_at": -1 })
                .build(),
        )
        .await
        .map_err(|e| ConmanError::Internal {
            message: format!("failed to create deployments release index: {e}"),
        })?;

    // Index 3: List deployments by app with pagination (newest first).
    collection
        .create_index(
            IndexModel::builder()
                .keys(doc! { "app_id": 1, "created_at": -1 })
                .build(),
        )
        .await
        .map_err(|e| ConmanError::Internal {
            message: format!("failed to create deployments app index: {e}"),
        })?;

    Ok(())
}
```

**Example documents:**

Normal deployment:

```json
{
  "_id": ObjectId("664a1b2c3d4e5f6a7b8c9d0e"),
  "app_id": ObjectId("664a0001000000000000000a"),
  "environment_id": ObjectId("664a0002000000000000000b"),
  "release_id": ObjectId("664a0003000000000000000c"),
  "state": "succeeded",
  "skip_stage": false,
  "approval_user_ids": [],
  "logs_ref": "jobs/664a1b2c3d4e5f6a7b8c9d0f/logs",
  "job_id": ObjectId("664a1b2c3d4e5f6a7b8c9d0f"),
  "rollback_mode": null,
  "rollback_source_release_id": null,
  "created_at": ISODate("2025-06-01T10:00:00Z"),
  "started_at": ISODate("2025-06-01T10:00:05Z"),
  "completed_at": ISODate("2025-06-01T10:02:30Z")
}
```

Skip-stage deployment with approvals:

```json
{
  "_id": ObjectId("664a2b3c4d5e6f7a8b9c0d1e"),
  "app_id": ObjectId("664a0001000000000000000a"),
  "environment_id": ObjectId("664a0004000000000000000d"),
  "release_id": ObjectId("664a0003000000000000000c"),
  "state": "running",
  "skip_stage": true,
  "approval_user_ids": [
    ObjectId("664a0005000000000000000e"),
    ObjectId("664a0006000000000000000f")
  ],
  "logs_ref": null,
  "job_id": ObjectId("664a2b3c4d5e6f7a8b9c0d1f"),
  "rollback_mode": null,
  "rollback_source_release_id": null,
  "created_at": ISODate("2025-06-02T14:00:00Z"),
  "started_at": ISODate("2025-06-02T14:00:03Z"),
  "completed_at": null
}
```

Rollback via redeploy prior tag:

```json
{
  "_id": ObjectId("664a3c4d5e6f7a8b9c0d1e2f"),
  "app_id": ObjectId("664a0001000000000000000a"),
  "environment_id": ObjectId("664a0002000000000000000b"),
  "release_id": ObjectId("664a0007000000000000001a"),
  "state": "succeeded",
  "skip_stage": false,
  "approval_user_ids": [],
  "logs_ref": "jobs/664a3c4d5e6f7a8b9c0d1e30/logs",
  "job_id": ObjectId("664a3c4d5e6f7a8b9c0d1e30"),
  "rollback_mode": "redeploy_prior_tag",
  "rollback_source_release_id": ObjectId("664a0003000000000000000c"),
  "created_at": ISODate("2025-06-03T08:00:00Z"),
  "started_at": ISODate("2025-06-03T08:00:02Z"),
  "completed_at": ISODate("2025-06-03T08:01:15Z")
}
```

## 5. API Endpoints

### 5.1 Deploy Release

```
POST /api/apps/:appId/environments/:envId/deploy
```

**Auth:** `config_manager` or `app_admin` on the app.

**Request body:**

```json
{
  "release_id": "664a0003000000000000000c",
  "skip_stage": false,
  "approvals": []
}
```

**Response 201:**

```json
{
  "data": {
    "id": "664a1b2c3d4e5f6a7b8c9d0e",
    "app_id": "664a0001000000000000000a",
    "environment_id": "664a0002000000000000000b",
    "release_id": "664a0003000000000000000c",
    "state": "pending",
    "skip_stage": false,
    "approval_user_ids": [],
    "logs_ref": null,
    "job_id": "664a1b2c3d4e5f6a7b8c9d0f",
    "rollback_mode": null,
    "rollback_source_release_id": null,
    "created_at": "2025-06-01T10:00:00Z",
    "started_at": null,
    "completed_at": null
  }
}
```

**Error cases:**

| Status | Code | Condition |
|--------|------|-----------|
| 400 | `validation_error` | Release not in deployable state (not `published` or later) |
| 400 | `validation_error` | Skip-stage approvals invalid (< 2, not distinct, no privileged) |
| 403 | `forbidden` | Caller lacks `config_manager`/`app_admin` role |
| 404 | `not_found` | App, environment, or release not found |
| 409 | `conflict` | Active deployment already exists on this environment (lock) |

### 5.2 Promote Release

```
POST /api/apps/:appId/environments/:envId/promote
```

**Auth:** `config_manager` or `app_admin` on the app.

**Request body:**

```json
{
  "release_id": "664a0003000000000000000c",
  "skip_stage": false,
  "approvals": []
}
```

**Response 201:** Same shape as deploy response.

**Business rules:**
- The release must have a `succeeded` deployment in the previous environment in
  the app's pipeline ordering.
- The target `:envId` must be the next stage after that prior deployment, unless
  `skip_stage` is true (requires approvals).
- Normal sequential promotion requires no additional deploy-time approval beyond
  changeset/release approvals.

**Error cases:**

| Status | Code | Condition |
|--------|------|-----------|
| 400 | `validation_error` | Release not deployed to a prior stage |
| 400 | `validation_error` | Target env is not the next stage and `skip_stage` is false |
| 400 | `validation_error` | Skip-stage approvals invalid |
| 409 | `conflict` | Active deployment lock on target environment |

### 5.3 Rollback

```
POST /api/apps/:appId/environments/:envId/rollback
```

**Auth:** `config_manager` or `app_admin` on the app.

**Request body (RedeployPriorTag):**

```json
{
  "mode": "redeploy_prior_tag",
  "target_release_id": "664a0007000000000000001a"
}
```

**Request body (RevertAndRelease):**

```json
{
  "mode": "revert_and_release",
  "target_release_id": "664a0003000000000000000c"
}
```

**Response 201:** Same shape as deploy response, with `rollback_mode` and
`rollback_source_release_id` populated.

**Error cases:**

| Status | Code | Condition |
|--------|------|-----------|
| 400 | `validation_error` | Target release not found or never deployed |
| 400 | `validation_error` | For `revert_and_release`: release tag cannot be resolved in Git |
| 409 | `conflict` | Active deployment lock on target environment |
| 502 | `git_error` | Revert commit conflicts on the integration branch (rollback mode A) |

### 5.4 List Deployments

```
GET /api/apps/:appId/deployments?page=&limit=
```

**Auth:** Any member of the app (read access).

**Response 200:**

```json
{
  "data": [
    { "id": "...", "state": "succeeded", "..." : "..." }
  ],
  "pagination": { "page": 1, "limit": 20, "total": 5 }
}
```

Returns deployments sorted by `created_at` descending (newest first).

## 6. Business Logic

### 6.1 Deploy

1. **Validate release state.** The release must be in `published`,
   `deployed_partial`, or `deployed_full` state. Releases in earlier states
   (`draft_release`, `assembling`, `validated`) cannot be deployed.
2. **Check environment lock.** Query `deployments` for any document matching
   `{ app_id, environment_id, state: { $in: ["pending", "running"] } }`. If one
   exists, return `ConmanError::Conflict` -- only one active deployment per
   environment.
3. **Validate skip-stage approvals (if applicable).** When `skip_stage` is true,
   call `DeployApproval::validate_approvals()` to ensure 2 distinct users with
   at least one privileged role approved.
4. **Create deployment record** in `pending` state.
5. **Enqueue `deploy_release` job** referencing the deployment ID.
6. **Emit audit event:** `deployment.created`.
7. **Return** the deployment record.

### 6.2 Promote

1. **Resolve pipeline ordering.** Load the app's environments sorted by their
   pipeline position (defined in E03 environment metadata).
2. **Verify prior deployment.** The release must have at least one `succeeded`
   deployment in an earlier pipeline stage.
3. **Validate target is next stage.** Unless `skip_stage` is true, the target
   environment must be the immediate successor of the release's latest
   successfully deployed environment.
4. **Delegate to deploy logic.** Promotion creates a deployment record the same
   way as a direct deploy. The release artifact is immutable -- the same tag is
   deployed to the new environment.
5. **Update release state.** After the deployment job succeeds:
   - If the release is now deployed to some but not all environments:
     transition release to `deployed_partial`.
   - If deployed to all environments: transition to `deployed_full`.
6. **Emit audit event:** `deployment.promoted`.

### 6.3 Skip-Stage

Skip-stage is not a separate endpoint; it is a modifier on deploy/promote. When
`skip_stage: true`:

1. The caller must supply `approvals` with at least 2 entries.
2. Each approval `user_id` is verified against `app_memberships` to confirm they
   are current members of the app.
3. `DeployApproval::validate_approvals()` enforces:
   - At least 2 approvals.
   - All user IDs are distinct.
   - At least one approver holds `reviewer`, `config_manager`, or `app_admin`.
4. The requesting user's own approval counts as one of the two (they must be
   `config_manager` or `app_admin` to call deploy/promote).

### 6.4 Concurrent Deploy

Concurrent deploys across _different_ environments are allowed by default --
the lock scope is per-environment. Concurrent deploys to the _same_ environment
are blocked by the environment lock.

If a workflow requires deploying to multiple environments simultaneously (e.g.,
deploying to QA and UAT at the same time without waiting for QA to finish), the
same skip-stage approval rules apply: 2 distinct users, at least one privileged.

### 6.5 Environment Lock

The lock is implemented via a query guard, not a separate lock collection:

```rust
/// Check whether the target environment has an active deployment.
///
/// Returns `Err(ConmanError::Conflict)` if a Pending or Running deployment
/// exists on the environment.
async fn check_environment_lock(
    deployment_repo: &DeploymentRepo,
    app_id: ObjectId,
    environment_id: ObjectId,
) -> Result<(), ConmanError> {
    let active = deployment_repo
        .find_active_deployment(app_id, environment_id)
        .await?;

    if let Some(existing) = active {
        return Err(ConmanError::Conflict {
            message: format!(
                "environment {} has an active deployment {} in state {:?}",
                environment_id.to_hex(),
                existing.id.to_hex(),
                existing.state,
            ),
        });
    }

    Ok(())
}
```

The `find_active_deployment` query uses the compound index on
`(app_id, environment_id, state)` to efficiently check for `pending` or
`running` deployments.

**Race condition mitigation:** Use MongoDB's `findOneAndUpdate` with a filter
that includes `state: { $nin: ["pending", "running"] }` when inserting the new
deployment. If the insert fails to match (because another deployment was created
concurrently), return `Conflict`. This provides optimistic locking without a
separate mutex.

### 6.6 Rollback Mode A: Revert and Release

1. **Resolve the release tag.** Use `RefService.FindTag` to look up the release
   tag (e.g., `r2025.06.01.1`) and get the tagged commit ID.
2. **Create revert commit.** Use `OperationService.UserRevert` to revert the
   tagged commit on `refs/heads/<integration_branch>`. The revert message follows the format:
   `Revert "release r2025.06.01.1"`.
3. **Handle revert conflicts.** If UserRevert returns a `MergeConflictError`,
   surface it as `ConmanError::Git` -- the config manager must resolve conflicts
   manually (out of scope for automated rollback).
4. **Create a new release.** Use the revert commit SHA as the basis for a new
   release with a fresh tag (e.g., `r2025.06.03.1`). This follows the standard
   E08 release creation + publish flow, but with the revert commit.
5. **Deploy the new release** to the target environment using the standard
   deploy flow.
6. **Update the original release state** to `rolled_back`.
7. **Emit audit events:** `release.rolled_back`, `deployment.rollback_created`.

### 6.7 Rollback Mode B: Redeploy Prior Tag

1. **Resolve the prior release.** Load the release record for
   `target_release_id`. Verify it was previously deployed to this environment
   (has a `succeeded` deployment record).
2. **Resolve the release tag in Git.** Use `RefService.FindTag` to verify the
   tag still exists.
3. **Create a new deployment record** linking the prior release to the target
   environment, with `rollback_mode: redeploy_prior_tag`.
4. **Enqueue the `deploy_release` job.** The job deploys the prior release's
   tag content to the environment.
5. **Update the failed release state** to `rolled_back` (if applicable).
6. **No Git mutations are performed.** The prior tag is reused as-is.
7. **Emit audit events:** `deployment.rollback_created`.

### 6.8 Normal Promotion Approvals

Normal sequential promotion (the release moves to the next environment in
pipeline order) requires **no additional deploy-time approval** beyond the
changeset and release approvals already obtained during the review and publish
flow. Only `config_manager` or `app_admin` authorization is checked.

## 7. Gitaly-rs Integration

### 7.1 `OperationService.UserRevert` -- Revert commit on the integration branch (Rollback Mode A)

Used to create a revert commit on `refs/heads/<integration_branch>` that undoes the changes
introduced by the release's composed commit.

**Proto service definition** (from `operations.proto`):

```protobuf
service OperationService {
  // UserRevert tries to perform a revert of a given commit onto a branch.
  rpc UserRevert(UserRevertRequest) returns (UserRevertResponse) {
    option (op_type) = {
      op: MUTATOR
    };
  }
}
```

**Request message:**

```protobuf
// UserRevertRequest is a request for the UserRevert RPC.
message UserRevertRequest {
  // repository is the repository in which the revert shall be applied.
  Repository repository = 1 [(target_repository)=true];
  // user to execute the action as. Also used to perform authentication and
  // authorization via an external endpoint.
  User user = 2;
  // commit is the commit to revert. Only the `id` field is required.
  GitCommit commit = 3;
  // branch_name is the name of the branch onto which the reverted commit shall
  // be committed.
  bytes branch_name = 4;
  // message is the message to use for the revert commit.
  bytes message = 5;
  // start_branch_name is used in case the branch_name branch does not
  // exist. In that case, it will be created from the start_branch_name.
  bytes start_branch_name = 6;
  // start_repository is used in case the branch_name branch does not exist.
  // In that case, it will be created from start_branch_name in the
  // start_repository.
  Repository start_repository = 7;
  // dry_run will compute the revert, but not update the target branch.
  bool dry_run = 8;
  // timestamp is the optional timestamp to use for the created revert
  // commit's committer date. If it's not set, the current time will be used.
  google.protobuf.Timestamp timestamp = 9;
  // expected_old_oid is the object ID which branch is expected to point to.
  // This is used as a safety guard to avoid races when branch has been
  // updated meanwhile to point to a different object ID.
  string expected_old_oid = 10;
  // sign controls whether the commit must be signed using a signing key
  // configured system-wide.
  bool sign = 11;
}
```

**Response message:**

```protobuf
// UserRevertResponse is a response for the UserRevert RPC.
message UserRevertResponse {
  // CreateTreeError represents an error which happened when computing the revert.
  enum CreateTreeError {
    // NONE denotes that no error occurred.
    NONE = 0;
    // EMPTY denotes that the revert would've resulted in an empty commit,
    // typically because it has already been applied to the target branch.
    EMPTY = 1;
    // CONFLICT denotes that the revert resulted in a conflict.
    CONFLICT = 2;
  }

  // branch_update represents details about the updated branch.
  OperationBranchUpdate branch_update = 1;
  // create_tree_error contains the error message if creation of the tree failed.
  string create_tree_error = 2;
  // commit_error contains the error message if updating the reference failed.
  string commit_error = 3;
  // pre_receive_error contains the error message if the pre-receive hook failed.
  string pre_receive_error = 4;
  // create_tree_error_code contains the error code if creation of the tree failed.
  CreateTreeError create_tree_error_code = 5;
}
```

**Error message:**

```protobuf
// UserRevertError is an error returned by the UserRevert RPC.
message UserRevertError {
  oneof error {
    // merge_conflict is returned if there is a conflict when applying the revert.
    MergeConflictError merge_conflict = 1;
    // changes_already_applied is returned if the result after applying the revert is empty.
    ChangesAlreadyAppliedError changes_already_applied = 2;
    // custom_hook contains the error message if the pre-receive hook failed.
    CustomHookError custom_hook = 3;
    // not_ancestor is returned if the old tip of the target branch is not an
    // ancestor of the new commit.
    NotAncestorError not_ancestor = 4;
  }
}
```

**Supporting type:**

```protobuf
// OperationBranchUpdate contains details about a branch that was updated.
message OperationBranchUpdate {
  // commit_id is set to the OID of the created commit if a branch was created or updated.
  string commit_id = 1;
  // repo_created indicates whether the branch created was the first one in the repository.
  bool repo_created = 2;
  // branch_created indicates whether the branch already existed in the repository
  // and was updated or whether it was created.
  bool branch_created = 3;
}
```

**Conman usage (Rollback Mode A):**

```rust
/// Create a revert commit on the integration branch that undoes the release's changes.
///
/// Returns the new commit OID on success, or a ConmanError on failure.
pub async fn revert_release_on_integration(
    &self,
    app: &App,
    release_commit_id: &str,
    release_tag: &str,
    integration_head_oid: &str,
) -> Result<String, ConmanError> {
    let repo = app_to_gitaly_repo(app);

    let request = UserRevertRequest {
        repository: Some(repo),
        user: Some(system_user()),
        commit: Some(GitCommit {
            id: release_commit_id.to_string(),
            ..Default::default()
        }),
        branch_name: app.integration_branch.as_bytes().to_vec(),
        message: format!("Revert \"release {release_tag}\"").into_bytes(),
        start_branch_name: Vec::new(),
        start_repository: None,
        dry_run: false,
        timestamp: None,
        expected_old_oid: integration_head_oid.to_string(),
        sign: false,
    };

    let response = self
        .operation_service()
        .user_revert(request)
        .await
        .map_err(|status| ConmanError::Git {
            message: format!("UserRevert gRPC failed: {status}"),
        })?
        .into_inner();

    // Check for tree creation errors (conflict, empty revert).
    if response.create_tree_error_code() != CreateTreeError::None {
        return Err(ConmanError::Git {
            message: format!(
                "revert failed: {} ({})",
                response.create_tree_error,
                response.create_tree_error_code().as_str_name(),
            ),
        });
    }

    // Check for commit-level errors.
    if !response.commit_error.is_empty() {
        return Err(ConmanError::Git {
            message: format!("revert commit error: {}", response.commit_error),
        });
    }

    // Extract the new commit ID from the branch update.
    let branch_update = response.branch_update.ok_or_else(|| ConmanError::Git {
        message: "revert succeeded but no branch_update returned".to_string(),
    })?;

    Ok(branch_update.commit_id)
}
```

### 7.2 `RefService.FindTag` -- Resolve release tag for redeployment

Used to verify a release tag exists and to obtain its target commit for both
rollback modes.

**Proto service definition** (from `ref.proto`):

```protobuf
service RefService {
  // FindTag looks up a tag by its name and returns it to the caller if it exists.
  // This RPC supports both lightweight and annotated tags. Note: this RPC
  // returns an `Internal` error if the tag was not found.
  rpc FindTag(FindTagRequest) returns (FindTagResponse) {
    option (op_type) = {
      op: ACCESSOR
    };
  }
}
```

**Request message:**

```protobuf
// FindTagRequest is a request for the FindTag RPC.
message FindTagRequest {
  // repository is the repository to look up the tag in.
  Repository repository = 1 [(target_repository)=true];
  // tag_name is the name of the tag that should be looked up. The caller is
  // supposed to pass in the tag name only, so if e.g. a tag `refs/tags/v1.0.0`
  // exists, then the caller should pass `v1.0.0` as argument.
  bytes tag_name = 2;
}
```

**Response message:**

```protobuf
// FindTagResponse is a response for the FindTag RPC.
message FindTagResponse {
  // tag is the tag that was found.
  Tag tag = 1;
}

// FindTagError is an error that will be returned by the FindTag RPC under
// specific error conditions.
message FindTagError {
  oneof error {
    // tag_not_found indicates that the tag was not found.
    ReferenceNotFoundError tag_not_found = 1;
  }
}
```

**Supporting type:**

```protobuf
// Tag represents a Git tag.
message Tag {
  bytes name = 1;
  string id = 2;
  GitCommit target_commit = 3;
  bytes message = 4;
  int64 message_size = 5;
  CommitAuthor tagger = 6;
  SignatureType signature_type = 7;
}
```

**Conman usage:**

```rust
/// Resolve a release tag by name and return its target commit ID.
///
/// Returns the Tag metadata including target_commit. Fails with NotFound
/// if the tag does not exist.
pub async fn find_release_tag(
    &self,
    app: &App,
    tag_name: &str,
) -> Result<(String, String), ConmanError> {
    let repo = app_to_gitaly_repo(app);

    let request = FindTagRequest {
        repository: Some(repo),
        tag_name: tag_name.as_bytes().to_vec(),
    };

    let response = self
        .ref_service()
        .find_tag(request)
        .await
        .map_err(|status| {
            if status.code() == tonic::Code::Internal {
                ConmanError::NotFound {
                    entity: "tag",
                    id: tag_name.to_string(),
                }
            } else {
                ConmanError::Git {
                    message: format!("FindTag gRPC failed: {status}"),
                }
            }
        })?
        .into_inner();

    let tag = response.tag.ok_or_else(|| ConmanError::NotFound {
        entity: "tag",
        id: tag_name.to_string(),
    })?;

    let tag_oid = tag.id.clone();
    let commit_id = tag
        .target_commit
        .map(|c| c.id)
        .ok_or_else(|| ConmanError::Git {
            message: format!("tag {tag_name} has no target commit"),
        })?;

    Ok((tag_oid, commit_id))
}
```

### 7.3 `CommitService.FindCommit` -- Resolve tag target commit

Used to fetch full commit details for a tag's target, ensuring the commit
exists and is valid before proceeding with deployment or revert operations.

**Proto service definition** (from `commit.proto`):

```protobuf
service CommitService {
  // FindCommit finds a commit for a given commitish. Returns nil if the commit
  // is not found.
  rpc FindCommit(FindCommitRequest) returns (FindCommitResponse) {
    option (op_type) = {
      op: ACCESSOR
    };
  }
}
```

**Request message:**

```protobuf
// FindCommitRequest is the request for the FindCommit RPC.
message FindCommitRequest {
  // repository is the repository in which we want to find the commit.
  Repository repository = 1 [(target_repository)=true];
  // revision is a commitish which is to be resolved to a commit.
  bytes revision = 2;
  // trailers if set, parses and adds the trailing information of the commit.
  bool trailers = 3;
}
```

**Response message:**

```protobuf
// FindCommitResponse is the response for the FindCommit RPC. Returns empty
// response if the commit is not found.
message FindCommitResponse {
  // commit is the requested commit, it is nil when the commit was not found.
  GitCommit commit = 1;
}
```

**Conman usage:**

```rust
/// Resolve a commit by its OID or a commitish ref and return the full
/// GitCommit metadata.
///
/// Used to validate that a release tag's target commit is reachable
/// before creating a deployment or performing a revert.
pub async fn find_commit(
    &self,
    app: &App,
    revision: &str,
) -> Result<GitCommit, ConmanError> {
    let repo = app_to_gitaly_repo(app);

    let request = FindCommitRequest {
        repository: Some(repo),
        revision: revision.as_bytes().to_vec(),
        trailers: false,
    };

    let response = self
        .commit_service()
        .find_commit(request)
        .await
        .map_err(|status| ConmanError::Git {
            message: format!("FindCommit gRPC failed: {status}"),
        })?
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

- [ ] **E09-S01** -- Add deployment domain types to `conman-core`.
  Create `deployment.rs` with `DeploymentState`, `RollbackMode`, `Deployment`,
  and `DeployApproval`. Write unit tests for `DeploymentState::is_active()`,
  `can_transition_to()`, and `DeployApproval::validate_approvals()`.

- [ ] **E09-S02** -- Implement `DeploymentRepo` in `conman-db`.
  Create the repository struct with `ensure_indexes()`, `insert()`,
  `find_by_id()`, `find_active_deployment()`, `update_state()`, and
  `list_by_app()` (paginated). Write integration tests against MongoDB.

- [ ] **E09-S03** -- Add Gitaly `find_release_tag` and `find_commit` wrappers.
  Extend `GitalyClient` in `conman-git` with `find_release_tag()` and
  `find_commit()`. Write tests using a mock gRPC server.

- [ ] **E09-S04** -- Add Gitaly `revert_release_on_integration` wrapper.
  Extend `GitalyClient` with `revert_release_on_integration()`. Write tests for
  success, conflict, and empty-revert cases using a mock gRPC server.

- [ ] **E09-S05** -- Implement environment lock check.
  Add `check_environment_lock()` function. Write unit test verifying that
  `Conflict` is returned when an active deployment exists, and `Ok(())`
  otherwise.

- [ ] **E09-S06** -- Implement deploy handler and worker.
  Add `POST /api/apps/:appId/environments/:envId/deploy` handler. Create the
  `deploy_release` job worker in `conman-jobs`. Write integration tests
  covering: successful deploy, lock conflict, invalid release state.

- [ ] **E09-S07** -- Implement promote handler.
  Add `POST /api/apps/:appId/environments/:envId/promote` handler. Write
  integration tests covering: valid sequential promotion, skip-stage with
  approvals, promotion without prior deployment (error).

- [ ] **E09-S08** -- Implement skip-stage and concurrent deploy approval logic.
  Wire `DeployApproval::validate_approvals()` into deploy/promote handlers.
  Write integration tests: valid 2-user approval, same user twice (error),
  no privileged user (error), < 2 approvals (error).

- [ ] **E09-S09** -- Implement rollback mode B (redeploy prior tag).
  Add `POST /api/apps/:appId/environments/:envId/rollback` handler for
  `redeploy_prior_tag` mode. Write integration tests: valid rollback, tag
  not found (error), no prior deployment (error).

- [ ] **E09-S10** -- Implement rollback mode A (revert and release).
  Add `revert_and_release` mode to the rollback handler. Integrate
  `revert_release_on_integration()` and the E08 release creation flow. Write
  integration tests: successful revert, conflict on revert (error).

- [ ] **E09-S11** -- Implement list deployments endpoint.
  Add `GET /api/apps/:appId/deployments` handler with pagination. Write
  integration tests for filtering and ordering.

- [ ] **E09-S12** -- Add release state transitions for deployment events.
  Update the release state machine: `published -> deployed_partial` and
  `deployed_partial -> deployed_full`. Implement transition logic in the
  deploy job worker on success. Write unit tests for the transitions.

- [ ] **E09-S13** -- Audit events for all deployment mutations.
  Emit `deployment.created`, `deployment.promoted`, `deployment.rollback_created`,
  `deployment.succeeded`, `deployment.failed`, `release.rolled_back` events.
  Write integration tests verifying audit records are created.

## 9. Test Cases

### 9.1 DeploymentState transitions

```rust
#[test]
fn pending_can_transition_to_running() {
    assert!(DeploymentState::Pending.can_transition_to(DeploymentState::Running));
}

#[test]
fn pending_can_transition_to_canceled() {
    assert!(DeploymentState::Pending.can_transition_to(DeploymentState::Canceled));
}

#[test]
fn running_can_transition_to_succeeded() {
    assert!(DeploymentState::Running.can_transition_to(DeploymentState::Succeeded));
}

#[test]
fn running_can_transition_to_failed() {
    assert!(DeploymentState::Running.can_transition_to(DeploymentState::Failed));
}

#[test]
fn running_can_transition_to_canceled() {
    assert!(DeploymentState::Running.can_transition_to(DeploymentState::Canceled));
}

#[test]
fn pending_cannot_transition_to_succeeded() {
    assert!(!DeploymentState::Pending.can_transition_to(DeploymentState::Succeeded));
}

#[test]
fn succeeded_cannot_transition_to_anything() {
    assert!(!DeploymentState::Succeeded.can_transition_to(DeploymentState::Running));
    assert!(!DeploymentState::Succeeded.can_transition_to(DeploymentState::Failed));
    assert!(!DeploymentState::Succeeded.can_transition_to(DeploymentState::Pending));
}

#[test]
fn failed_is_terminal() {
    assert!(!DeploymentState::Failed.can_transition_to(DeploymentState::Running));
    assert!(!DeploymentState::Failed.can_transition_to(DeploymentState::Pending));
}

#[test]
fn active_states_are_pending_and_running() {
    assert!(DeploymentState::Pending.is_active());
    assert!(DeploymentState::Running.is_active());
    assert!(!DeploymentState::Succeeded.is_active());
    assert!(!DeploymentState::Failed.is_active());
    assert!(!DeploymentState::Canceled.is_active());
}
```

### 9.2 DeployApproval validation

```rust
#[test]
fn valid_approvals_pass() {
    let approvals = vec![
        DeployApproval {
            user_id: ObjectId::new(),
            role: Role::ConfigManager,
            approved_at: Utc::now(),
        },
        DeployApproval {
            user_id: ObjectId::new(),
            role: Role::User,
            approved_at: Utc::now(),
        },
    ];

    assert!(DeployApproval::validate_approvals(&approvals).is_ok());
}

#[test]
fn fewer_than_two_approvals_fails() {
    let approvals = vec![DeployApproval {
        user_id: ObjectId::new(),
        role: Role::AppAdmin,
        approved_at: Utc::now(),
    }];

    let err = DeployApproval::validate_approvals(&approvals).unwrap_err();
    assert!(err.to_string().contains("at least 2 approvals"));
}

#[test]
fn same_user_twice_fails() {
    let user_id = ObjectId::new();
    let approvals = vec![
        DeployApproval {
            user_id,
            role: Role::ConfigManager,
            approved_at: Utc::now(),
        },
        DeployApproval {
            user_id,
            role: Role::ConfigManager,
            approved_at: Utc::now(),
        },
    ];

    let err = DeployApproval::validate_approvals(&approvals).unwrap_err();
    assert!(err.to_string().contains("distinct users"));
}

#[test]
fn no_privileged_approver_fails() {
    let approvals = vec![
        DeployApproval {
            user_id: ObjectId::new(),
            role: Role::User,
            approved_at: Utc::now(),
        },
        DeployApproval {
            user_id: ObjectId::new(),
            role: Role::User,
            approved_at: Utc::now(),
        },
    ];

    let err = DeployApproval::validate_approvals(&approvals).unwrap_err();
    assert!(err.to_string().contains("reviewer, config_manager, or app_admin"));
}

#[test]
fn reviewer_counts_as_privileged() {
    let approvals = vec![
        DeployApproval {
            user_id: ObjectId::new(),
            role: Role::Reviewer,
            approved_at: Utc::now(),
        },
        DeployApproval {
            user_id: ObjectId::new(),
            role: Role::User,
            approved_at: Utc::now(),
        },
    ];

    assert!(DeployApproval::validate_approvals(&approvals).is_ok());
}
```

### 9.3 Environment lock prevents concurrent deploys

```rust
#[tokio::test]
async fn deploy_blocked_when_active_deployment_exists() {
    let (app, env, release) = setup_published_release().await;
    let state = test_app_state().await;

    // First deploy succeeds and is in Pending state.
    let resp1 = deploy(&state, &app.id, &env.id, &release.id).await;
    assert_eq!(resp1.status(), StatusCode::CREATED);

    // Second deploy to the same environment is blocked.
    let resp2 = deploy(&state, &app.id, &env.id, &release.id).await;
    assert_eq!(resp2.status(), StatusCode::CONFLICT);

    let body: serde_json::Value = parse_body(resp2).await;
    assert_eq!(body["error"]["code"], "conflict");
    assert!(body["error"]["message"].as_str().unwrap().contains("active deployment"));
}
```

### 9.4 Deploy rejects unpublished release

```rust
#[tokio::test]
async fn deploy_rejects_draft_release() {
    let (app, env) = setup_app_with_env().await;
    let release = create_release_in_state(&app.id, ReleaseState::DraftRelease).await;
    let state = test_app_state().await;

    let resp = deploy(&state, &app.id, &env.id, &release.id).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = parse_body(resp).await;
    assert_eq!(body["error"]["code"], "validation_error");
}
```

### 9.5 Promote requires prior deployment in earlier stage

```rust
#[tokio::test]
async fn promote_fails_without_prior_deployment() {
    let (app, envs, release) = setup_app_with_pipeline().await;
    let state = test_app_state().await;

    // Try to promote to env[1] (QA) without deploying to env[0] (Dev) first.
    let resp = promote(&state, &app.id, &envs[1].id, &release.id).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn promote_succeeds_after_prior_deployment() {
    let (app, envs, release) = setup_app_with_pipeline().await;
    let state = test_app_state().await;

    // Deploy to Dev and complete it.
    deploy_and_complete(&state, &app.id, &envs[0].id, &release.id).await;

    // Promote to QA succeeds.
    let resp = promote(&state, &app.id, &envs[1].id, &release.id).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}
```

### 9.6 Skip-stage requires valid approvals

```rust
#[tokio::test]
async fn skip_stage_deploy_with_valid_approvals_succeeds() {
    let (app, envs, release) = setup_app_with_pipeline().await;
    let state = test_app_state().await;

    // Skip Dev, deploy directly to QA with two valid approvals.
    let resp = deploy_with_skip_stage(
        &state,
        &app.id,
        &envs[1].id,
        &release.id,
        vec![config_manager_user_id(), reviewer_user_id()],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn skip_stage_deploy_without_approvals_fails() {
    let (app, envs, release) = setup_app_with_pipeline().await;
    let state = test_app_state().await;

    let resp = deploy_with_skip_stage(
        &state,
        &app.id,
        &envs[1].id,
        &release.id,
        vec![], // No approvals
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
```

### 9.7 Rollback mode B: redeploy prior tag

```rust
#[tokio::test]
async fn rollback_redeploy_prior_tag_creates_deployment() {
    let (app, env, release_v1, release_v2) = setup_two_deployed_releases().await;
    let state = test_app_state().await;

    // Rollback env to release_v1.
    let resp = rollback(
        &state,
        &app.id,
        &env.id,
        RollbackMode::RedeployPriorTag,
        &release_v1.id,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body: serde_json::Value = parse_body(resp).await;
    assert_eq!(body["data"]["rollback_mode"], "redeploy_prior_tag");
    assert_eq!(body["data"]["release_id"], release_v1.id.to_hex());
}
```

### 9.8 Rollback mode A: revert and release

```rust
#[tokio::test]
async fn rollback_revert_and_release_creates_revert_commit() {
    let (app, env, release) = setup_deployed_release().await;
    let state = test_app_state().await;

    // Mock gitaly UserRevert to return success.
    mock_user_revert_success(&state.gitaly_mock, "new-revert-sha");

    let resp = rollback(
        &state,
        &app.id,
        &env.id,
        RollbackMode::RevertAndRelease,
        &release.id,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body: serde_json::Value = parse_body(resp).await;
    assert_eq!(body["data"]["rollback_mode"], "revert_and_release");
}

#[tokio::test]
async fn rollback_revert_conflict_returns_git_error() {
    let (app, env, release) = setup_deployed_release().await;
    let state = test_app_state().await;

    // Mock gitaly UserRevert to return a conflict.
    mock_user_revert_conflict(&state.gitaly_mock);

    let resp = rollback(
        &state,
        &app.id,
        &env.id,
        RollbackMode::RevertAndRelease,
        &release.id,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);

    let body: serde_json::Value = parse_body(resp).await;
    assert_eq!(body["error"]["code"], "git_error");
}
```

### 9.9 List deployments is paginated and sorted

```rust
#[tokio::test]
async fn list_deployments_returns_newest_first() {
    let (app, env, release) = setup_deployed_release().await;
    let state = test_app_state().await;

    // Create 3 deployments.
    for _ in 0..3 {
        create_test_deployment(&state, &app.id, &env.id, &release.id).await;
    }

    let resp = list_deployments(&state, &app.id, 1, 2).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = parse_body(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
    assert_eq!(body["pagination"]["total"], 3);
    assert_eq!(body["pagination"]["page"], 1);
    assert_eq!(body["pagination"]["limit"], 2);

    // Verify ordering: first item is newer than second.
    let first_created = body["data"][0]["created_at"].as_str().unwrap();
    let second_created = body["data"][1]["created_at"].as_str().unwrap();
    assert!(first_created > second_created);
}
```

### 9.10 Audit events are emitted for deployment mutations

```rust
#[tokio::test]
async fn deploy_emits_audit_event() {
    let (app, env, release) = setup_published_release().await;
    let state = test_app_state().await;

    deploy(&state, &app.id, &env.id, &release.id).await;

    let events = list_audit_events(&state, "deployment", "created").await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].app_id, Some(app.id));
    assert_eq!(events[0].action, "created");
}
```

## 10. Acceptance Criteria

1. **Deploy creates an async job and records the deployment.**
   - `POST /api/apps/:appId/environments/:envId/deploy` with a valid published
     release returns 201 with a deployment in `pending` state.
   - The `deploy_release` job is enqueued and the deployment transitions through
     `pending -> running -> succeeded` (or `failed`).

2. **Promote moves the same immutable release across stages.**
   - A release deployed to Dev can be promoted to QA without re-assembly.
   - The promotion verifies prior deployment in the correct pipeline stage.
   - The release artifact (Git tag) is unchanged between environments.

3. **Skip-stage and concurrent deploy require 2 approvals.**
   - Skip-stage deploy with fewer than 2 approvals returns 400.
   - Skip-stage deploy with 2 approvals from the same user returns 400.
   - Skip-stage deploy with 2 approvals but no privileged role returns 400.
   - Valid skip-stage deploy with 2 distinct users (one privileged) returns 201.

4. **Environment lock prevents overlapping deployments.**
   - A second deploy to an environment with a `pending` or `running` deployment
     returns 409 Conflict.
   - Deploys to different environments on the same app proceed independently.

5. **Normal promotion needs no additional deploy-time approval.**
   - Sequential promotion (Dev -> QA -> UAT -> Prod) succeeds with only the
     `config_manager`/`app_admin` authorization check. No approval records
     are required.

6. **Rollback mode A (revert and release) is available and audited.**
   - Creates a revert commit on the integration branch via `OperationService.UserRevert`.
   - Creates a new release from the revert commit.
   - Deploys the new release to the target environment.
   - The original release transitions to `rolled_back`.
   - Revert conflicts are surfaced as 502 `git_error`.
   - Audit events record the rollback action.

7. **Rollback mode B (redeploy prior tag) is available and audited.**
   - Reuses an earlier release tag without modifying Git.
   - Creates a new deployment record with `rollback_mode: redeploy_prior_tag`.
   - Verifies the prior release was previously deployed to some environment.
   - Audit events record the rollback action.

8. **Both rollback modes are audited.**
   - Every deployment creation, promotion, and rollback emits an audit event
     with `entity_type: "deployment"`, the relevant action, and before/after
     state snapshots.

9. **Runtime profile drift blocks deploys.**
   - Drift across env vars, secrets, URL, DB settings, or migration set
     differences returns conflict and blocks deployment until revalidation
     succeeds.
