# E05 Changesets, Review, Comments, Revisions

## 1. Goal

Implement the full changeset lifecycle from creation through approval, including
review workflow, inline/threaded comments with revision history, semantic and raw
diffs, runtime profile overrides, and AI analysis endpoints.

## 2. Dependencies

| Dependency | What it provides |
|------------|-----------------|
| **E02 Auth & RBAC** | `AuthUser` extractor, role checks (`reviewer`, `config_manager`, `admin`) for review actions |
| **E04 Workspaces** | `Workspace` domain type, workspace repository, `head_sha` resolution, branch naming (`ws/<user>/<app>`) |
| **E03 App Setup** | Runtime profiles and environment linkage metadata |
| **E01 Git Adapter** | `GitalyClient` for diff generation, commit resolution, and blob content retrieval |
| **E00 Platform** | Error envelope, pagination, MongoDB bootstrap, audit infrastructure |

## 3. Rust Types

All types live in `conman-core`. API request/response DTOs live in `conman-api`.

### Domain Types

```rust
use chrono::{DateTime, Utc};
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// -- Changeset State Machine --

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangesetState {
    Draft,
    Submitted,
    InReview,
    Approved,
    ChangesRequested,
    Rejected,
    Queued,
    Released,
    Conflicted,
    NeedsRevalidation,
}

impl ChangesetState {
    /// Terminal states cannot transition further.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Released | Self::Rejected)
    }

    /// Open states count toward the one-open-changeset-per-workspace constraint.
    pub fn is_open(&self) -> bool {
        !self.is_terminal()
    }
}

/// Actions that drive changeset state transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangesetAction {
    Submit,
    Resubmit,
    Approve,
    RequestChanges,
    Reject,
    Queue,
    Release,
    MarkConflicted,
    MarkNeedsRevalidation,
    MoveToDraft,
}

/// Validates and executes a state machine transition. Returns the new state
/// or an error when the transition is not allowed.
pub fn transition(
    current: ChangesetState,
    action: ChangesetAction,
) -> Result<ChangesetState, ConmanError> {
    use ChangesetAction::*;
    use ChangesetState::*;

    let next = match (current, action) {
        // Draft can be submitted
        (Draft, Submit) => Submitted,

        // Submitted transitions into review when first reviewer acts
        (Submitted, Approve) => InReview,
        (Submitted, RequestChanges) => InReview,
        (Submitted, Reject) => InReview,

        // Resubmit keeps the changeset in review flow with a new revision
        (Submitted, Resubmit) => Submitted,
        (InReview, Resubmit) => Submitted,
        (ChangesRequested, Resubmit) => Submitted,

        // Review actions from InReview
        (InReview, Approve) => InReview,       // stays in review; threshold check happens in business logic
        (InReview, RequestChanges) => ChangesRequested,
        (InReview, Reject) => Rejected,

        // ChangesRequested can receive further reviews
        (ChangesRequested, Approve) => InReview,
        (ChangesRequested, RequestChanges) => ChangesRequested,
        (ChangesRequested, Reject) => Rejected,

        // ChangesRequested can return to draft
        (ChangesRequested, MoveToDraft) => Draft,

        // Approved changeset is queued by config_manager
        (Approved, Queue) => Queued,

        // Queued changeset outcomes
        (Queued, Release) => Released,
        (Queued, MarkConflicted) => Conflicted,
        (Queued, MarkNeedsRevalidation) => NeedsRevalidation,

        // Recovery paths back to draft
        (Conflicted, MoveToDraft) => Draft,
        (NeedsRevalidation, MoveToDraft) => Draft,

        _ => {
            return Err(ConmanError::InvalidTransition {
                from: format!("{:?}", current),
                to: format!("{:?}", action),
            });
        }
    };

    Ok(next)
}

/// Internal helper: after a review action in InReview state, check whether
/// the approval threshold is met and auto-transition to Approved.
pub fn check_approval_threshold(
    state: ChangesetState,
    approval_count: u32,
    required_approval_count: u32,
) -> ChangesetState {
    if state == ChangesetState::InReview && approval_count >= required_approval_count {
        ChangesetState::Approved
    } else {
        state
    }
}

// -- Changeset --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Changeset {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub app_id: ObjectId,
    pub workspace_id: ObjectId,
    pub author_user_id: ObjectId,
    pub title: String,
    pub description: String,
    pub state: ChangesetState,
    /// The baseline commit this changeset is compared against.
    pub base_sha: String,
    /// The frozen workspace HEAD at time of submit. Updated on resubmit.
    pub head_sha: String,
    /// Monotonically increasing revision counter. Starts at 1 on first submit.
    pub current_revision: u32,
    /// Number of approvals on the current revision. Reset to 0 on resubmit.
    pub approval_count: u32,
    /// Configurable per app; defaults to 1.
    pub required_approval_count: u32,
    /// Result of last revalidation job (for queued changesets).
    pub last_revalidation_status: Option<String>,
    pub last_revalidation_job_id: Option<ObjectId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// -- Changeset Revision --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangesetRevision {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub changeset_id: ObjectId,
    pub revision_number: u32,
    /// The head_sha that was frozen for this revision.
    pub head_sha: String,
    pub created_by: ObjectId,
    pub created_at: DateTime<Utc>,
}

// -- Review --

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Approved,
    ChangesRequested,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangesetReview {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub changeset_id: ObjectId,
    pub reviewer_user_id: ObjectId,
    /// The revision number at the time of review.
    pub revision_number: u32,
    pub decision: ReviewDecision,
    /// Optional comment left with the review.
    pub comment: Option<String>,
    pub created_at: DateTime<Utc>,
}

// -- Comments --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangesetComment {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub changeset_id: ObjectId,
    pub author_user_id: ObjectId,
    pub body: String,
    /// When set, this comment is anchored to a specific file.
    pub file_path: Option<String>,
    /// When set alongside file_path, anchors to a specific line.
    pub line_number: Option<u32>,
    /// When set, this comment is a reply in a thread.
    pub parent_comment_id: Option<ObjectId>,
    pub resolved: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangesetCommentRevision {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub comment_id: ObjectId,
    /// The body text before the edit.
    pub body: String,
    pub edited_at: DateTime<Utc>,
}

// -- Semantic Diff --

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticConfigType {
    Entity,
    Page,
    Queue,
    Provider,
    Workflow,
    Team,
    Menu,
    Asset,
    Script,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticOperation {
    Added,
    Modified,
    Removed,
    Moved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticChange {
    pub id: String,
    pub config_type: SemanticConfigType,
    pub operation: SemanticOperation,
    /// Human-readable target identifier (e.g. entity name, page ID).
    pub target: String,
    /// Human-readable description of what changed.
    pub description: String,
    pub file_path: String,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    /// Arbitrary structured details about the change.
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticDiffSummary {
    pub total_changes: u32,
    pub by_config_type: HashMap<SemanticConfigType, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticDiffResponse {
    pub base_sha: String,
    pub head_sha: String,
    pub summary: SemanticDiffSummary,
    pub changes: Vec<SemanticChange>,
}
```

### API Request/Response Types

```rust
// conman-api: src/handlers/changesets.rs

use serde::{Deserialize, Serialize};

// -- Changeset CRUD --

#[derive(Debug, Deserialize)]
pub struct CreateChangesetRequest {
    pub workspace_id: String,
    pub title: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ChangesetResponse {
    pub id: String,
    pub app_id: String,
    pub workspace_id: String,
    pub author_user_id: String,
    pub title: String,
    pub description: String,
    pub state: String,
    pub base_sha: String,
    pub head_sha: String,
    pub current_revision: u32,
    pub approval_count: u32,
    pub required_approval_count: u32,
    pub last_revalidation_status: Option<String>,
    pub last_revalidation_job_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateChangesetRequest {
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListChangesetsQuery {
    pub page: Option<u64>,
    pub limit: Option<u64>,
    pub state: Option<String>,
}

// -- Submit / Resubmit --

#[derive(Debug, Serialize)]
pub struct SubmitChangesetResponse {
    pub changeset: ChangesetResponse,
    pub revision: ChangesetRevisionResponse,
}

#[derive(Debug, Serialize)]
pub struct ChangesetRevisionResponse {
    pub id: String,
    pub changeset_id: String,
    pub revision_number: u32,
    pub head_sha: String,
    pub created_by: String,
    pub created_at: String,
}

// -- Review --

#[derive(Debug, Deserialize)]
pub struct ReviewRequest {
    pub decision: String,  // "approved" | "changes_requested" | "rejected"
    pub comment: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReviewResponse {
    pub review: ChangesetReviewResponse,
    pub changeset: ChangesetResponse,
}

#[derive(Debug, Serialize)]
pub struct ChangesetReviewResponse {
    pub id: String,
    pub changeset_id: String,
    pub reviewer_user_id: String,
    pub revision_number: u32,
    pub decision: String,
    pub comment: Option<String>,
    pub created_at: String,
}

// -- Diff --

#[derive(Debug, Deserialize)]
pub struct DiffQuery {
    pub mode: String,  // "raw" | "semantic"
}

#[derive(Debug, Serialize)]
pub struct RawDiffResponse {
    pub base_sha: String,
    pub head_sha: String,
    pub patch: String,
    pub stats: Vec<DiffStatEntry>,
}

#[derive(Debug, Serialize)]
pub struct DiffStatEntry {
    pub path: String,
    pub additions: i32,
    pub deletions: i32,
}

// -- Comments --

#[derive(Debug, Deserialize)]
pub struct CreateCommentRequest {
    pub body: String,
    pub file_path: Option<String>,
    pub line_number: Option<u32>,
    pub parent_comment_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCommentRequest {
    pub body: Option<String>,
    pub resolved: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct CommentResponse {
    pub id: String,
    pub changeset_id: String,
    pub author_user_id: String,
    pub body: String,
    pub file_path: Option<String>,
    pub line_number: Option<u32>,
    pub parent_comment_id: Option<String>,
    pub resolved: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct ListCommentsQuery {
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

// -- AI Endpoints --

#[derive(Debug, Deserialize)]
pub struct AnalyzeRequest {
    /// Optional focus areas for analysis.
    pub focus: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct AnalyzeResponse {
    pub summary: String,
    pub suggestions: Vec<AiSuggestion>,
}

#[derive(Debug, Serialize)]
pub struct AiSuggestion {
    pub file_path: String,
    pub description: String,
    pub severity: String,
}

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    /// Optionally scope the chat to specific files.
    pub file_paths: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub reply: String,
    pub referenced_files: Vec<String>,
}
```

## 4. Database

### `changesets` Collection

**Unique constraint:** At most one non-terminal changeset per (app_id, workspace_id).

```javascript
// Indexes
db.changesets.createIndex({ app_id: 1, workspace_id: 1, state: 1 })
db.changesets.createIndex({ app_id: 1, state: 1, updated_at: -1 })
db.changesets.createIndex({ app_id: 1, author_user_id: 1 })

// Unique partial index: enforce one open changeset per workspace
db.changesets.createIndex(
  { app_id: 1, workspace_id: 1 },
  {
    unique: true,
    partialFilterExpression: {
      state: { $nin: ["released", "rejected"] }
    }
  }
)

// Example document
{
  _id: ObjectId("6650a1b2c3d4e5f607080901"),
  app_id: ObjectId("6650a1b2c3d4e5f607080001"),
  workspace_id: ObjectId("6650a1b2c3d4e5f607080801"),
  author_user_id: ObjectId("6650a1b2c3d4e5f607080101"),
  title: "Add payment provider config",
  description: "Configures Stripe provider for EU teams",
  state: "in_review",
  base_sha: "abc123def456abc123def456abc123def456abc1",
  head_sha: "789def012345789def012345789def012345789d",
  current_revision: 2,
  approval_count: 1,
  required_approval_count: 1,
  last_revalidation_status: null,
  last_revalidation_job_id: null,
  created_at: ISODate("2025-06-01T10:00:00Z"),
  updated_at: ISODate("2025-06-02T14:30:00Z")
}
```

### `changeset_revisions` Collection

```javascript
// Indexes
db.changeset_revisions.createIndex({ changeset_id: 1, revision_number: 1 }, { unique: true })
db.changeset_revisions.createIndex({ changeset_id: 1, created_at: -1 })

// Example document
{
  _id: ObjectId("6650a1b2c3d4e5f607080902"),
  changeset_id: ObjectId("6650a1b2c3d4e5f607080901"),
  revision_number: 2,
  head_sha: "789def012345789def012345789def012345789d",
  created_by: ObjectId("6650a1b2c3d4e5f607080101"),
  created_at: ISODate("2025-06-02T14:30:00Z")
}
```

### `changeset_reviews` Collection

```javascript
// Indexes
db.changeset_reviews.createIndex({ changeset_id: 1, created_at: -1 })
db.changeset_reviews.createIndex({ changeset_id: 1, reviewer_user_id: 1 })
db.changeset_reviews.createIndex({ changeset_id: 1, revision_number: 1 })

// Example document
{
  _id: ObjectId("6650a1b2c3d4e5f607080903"),
  changeset_id: ObjectId("6650a1b2c3d4e5f607080901"),
  reviewer_user_id: ObjectId("6650a1b2c3d4e5f607080201"),
  revision_number: 2,
  decision: "approved",
  comment: "LGTM, Stripe config looks correct",
  created_at: ISODate("2025-06-02T16:00:00Z")
}
```

### `changeset_comments` Collection

```javascript
// Indexes
db.changeset_comments.createIndex({ changeset_id: 1, created_at: -1 })
db.changeset_comments.createIndex({ changeset_id: 1, file_path: 1 })
db.changeset_comments.createIndex({ parent_comment_id: 1 })

// Example document
{
  _id: ObjectId("6650a1b2c3d4e5f607080904"),
  changeset_id: ObjectId("6650a1b2c3d4e5f607080901"),
  author_user_id: ObjectId("6650a1b2c3d4e5f607080201"),
  body: "Should we set the retry count higher for EU?",
  file_path: "providers/stripe.json",
  line_number: 42,
  parent_comment_id: null,
  resolved: false,
  created_at: ISODate("2025-06-02T15:30:00Z"),
  updated_at: ISODate("2025-06-02T15:30:00Z")
}
```

### `changeset_comment_revisions` Collection

```javascript
// Indexes
db.changeset_comment_revisions.createIndex({ comment_id: 1, edited_at: -1 })

// Example document
{
  _id: ObjectId("6650a1b2c3d4e5f607080905"),
  comment_id: ObjectId("6650a1b2c3d4e5f607080904"),
  body: "Should we set the retry count higher?",
  edited_at: ISODate("2025-06-02T15:45:00Z")
}
```

## 5. API Endpoints

### `GET /api/repos/:appId/changesets`

List changesets for an app with optional state filter.

- **Auth:** Any app member
- **Query:** `page` (default 1), `limit` (default 20, max 100), `state` (optional, comma-separated)
- **Response 200:**
  ```json
  {
    "data": [ChangesetResponse],
    "pagination": { "page": 1, "limit": 20, "total": 5 }
  }
  ```
- **Errors:** 403 Forbidden (not a member)

### `POST /api/repos/:appId/changesets`

Create a new changeset from a workspace. Enforces one open changeset per
workspace.

- **Auth:** Any app member (must own the workspace or be config_manager+)
- **Body:** `CreateChangesetRequest`
- **Logic:**
  1. Verify workspace belongs to app and caller owns it.
  2. Check no open changeset exists for this workspace (partial unique index enforces this).
  3. Resolve current baseline (`base_sha`) via `resolve_baseline()`.
  4. Read workspace `head_sha`.
  5. Create changeset in `Draft` state with `current_revision: 0`, `approval_count: 0`.
  6. Emit audit event.
- **Response 201:** `{ "data": ChangesetResponse }`
- **Errors:** 409 Conflict (open changeset exists), 404 workspace not found, 403 Forbidden

### `GET /api/repos/:appId/changesets/:changesetId`

Get changeset details.

- **Auth:** Any app member
- **Response 200:** `{ "data": ChangesetResponse }`
- **Errors:** 404 Not Found

### `PATCH /api/repos/:appId/changesets/:changesetId`

Update changeset title/description. Only allowed in `Draft` state.

- **Auth:** Changeset author or config_manager+
- **Body:** `UpdateChangesetRequest`
- **Guard:** State must be `Draft`.
- **Response 200:** `{ "data": ChangesetResponse }`
- **Errors:** 409 (wrong state), 403 Forbidden, 404 Not Found

### `POST /api/repos/:appId/changesets/:changesetId/submit`

Submit a draft changeset for review. Freezes `head_sha` and creates the first
revision record.

- **Auth:** Changeset author
- **Guard:** State must be `Draft`.
- **Logic:**
  1. Resolve workspace `head_sha` via `CommitService.FindCommit`.
  2. Transition `Draft -> Submitted`.
  3. Set `current_revision = 1`, freeze `head_sha`.
  4. Create `ChangesetRevision` record (revision_number=1).
  5. Emit audit event.
- **Behavior:** Profile overrides are auto-included on submit and returned in
  `SubmitChangesetResponse` as `included_profile_overrides`.
- **Response 200:** `{ "data": SubmitChangesetResponse }`
- **Errors:** 409 (wrong state), 404

### `POST /api/repos/:appId/changesets/:changesetId/resubmit`

Resubmit a changeset with new workspace changes. Creates a new revision, resets
approvals to zero.

- **Auth:** Changeset author
- **Guard:** State must be `Submitted`, `InReview`, or `ChangesRequested`.
- **Logic:**
  1. Resolve current workspace `head_sha` via `CommitService.FindCommit`.
  2. Verify `head_sha` differs from current frozen value (otherwise 400).
  3. Increment `current_revision`.
  4. Update changeset: new `head_sha`, `approval_count = 0`.
  5. Transition to `Submitted` (via `ChangesetAction::Resubmit`).
  6. Create new `ChangesetRevision` record.
  7. Emit audit event with before/after state.
- **Response 200:** `{ "data": SubmitChangesetResponse }`
- **Errors:** 409 (wrong state), 400 (no changes)

### `POST /api/repos/:appId/changesets/:changesetId/review`

Submit a review decision on a changeset.

- **Auth:** `reviewer`, `config_manager`, or `admin`
- **Body:** `ReviewRequest`
- **Guard:** State must be `Submitted`, `InReview`, or `ChangesRequested`.
- **Logic:**
  1. Parse and validate `decision`.
  2. Map decision to `ChangesetAction` (`Approve`, `RequestChanges`, `Reject`).
  3. Execute state transition.
  4. If `Approve`: increment `approval_count`, then check threshold.
     If `approval_count >= required_approval_count`, auto-transition to `Approved`.
  5. If `RequestChanges`: reset `approval_count` to 0.
  6. If `Reject`: terminal.
  7. Create `ChangesetReview` record.
  8. Emit audit event.
- **Response 200:** `{ "data": ReviewResponse }`
- **Errors:** 409 (wrong state), 403 (insufficient role)

### `POST /api/repos/:appId/changesets/:changesetId/queue`

Move an approved changeset into the queue. Performed by config_manager+.

- **Auth:** `config_manager` or `admin`
- **Guard:** State must be `Approved`.
- **Logic:**
  1. Transition `Approved -> Queued`.
  2. Emit audit event.
- **Response 200:** `{ "data": ChangesetResponse }`
- **Errors:** 409 (wrong state), 403 Forbidden

### `POST /api/repos/:appId/changesets/:changesetId/move-to-draft`

Return a changeset to draft from `ChangesRequested`, `Conflicted`, or
`NeedsRevalidation`.

- **Auth:** Changeset author (for own), `config_manager`+ (for any)
- **Guard:** State must be `ChangesRequested`, `Conflicted`, or `NeedsRevalidation`.
- **Logic:**
  1. Transition to `Draft` via `MoveToDraft`.
  2. Reset `approval_count` to 0.
  3. Emit audit event.
- **Response 200:** `{ "data": ChangesetResponse }`
- **Errors:** 409 (wrong state), 403 Forbidden

### `GET /api/repos/:appId/changesets/:changesetId/diff`

Retrieve diff between base_sha and head_sha.

- **Auth:** Any app member
- **Query:** `mode` = `raw` | `semantic`
- **Logic (raw):**
  1. Call `DiffService.RawDiff` for unified diff.
  2. Call `DiffService.DiffStats` for per-file stats.
  3. Return `RawDiffResponse`.
- **Logic (semantic):**
  1. Call `DiffService.DiffStats` to identify changed files.
  2. For each changed file, call `BlobService.GetBlobs` to read both base and head versions.
  3. Parse JSON content and classify by config type (entity, page, etc.).
  4. Compare parsed structures and generate `SemanticChange` entries.
  5. Return `SemanticDiffResponse`.
- **Response 200:** `RawDiffResponse` or `SemanticDiffResponse` depending on mode.
- **Errors:** 404, 502 (git error)

### `GET /api/repos/:appId/changesets/:changesetId/comments`

List comments for a changeset, paginated.

- **Auth:** Any app member
- **Query:** `page`, `limit`
- **Response 200:**
  ```json
  {
    "data": [CommentResponse],
    "pagination": { "page": 1, "limit": 20, "total": 8 }
  }
  ```

### `POST /api/repos/:appId/changesets/:changesetId/comments`

Add a comment to a changeset. Supports inline (file+line) and threaded (parent)
comments.

- **Auth:** Any app member
- **Body:** `CreateCommentRequest`
- **Logic:**
  1. Validate `parent_comment_id` exists and belongs to same changeset (if provided).
  2. Validate `file_path` exists in the changeset diff (if provided).
  3. Create `ChangesetComment` record.
  4. Emit audit event.
- **Response 201:** `{ "data": CommentResponse }`
- **Errors:** 404 (changeset or parent comment not found), 400 (validation)

### `PATCH /api/repos/:appId/changesets/:changesetId/comments/:commentId`

Edit a comment body or toggle resolved status. Stores prior body in revision
history.

- **Auth:** Comment author (for body edits), any app member (for resolving)
- **Body:** `UpdateCommentRequest`
- **Logic:**
  1. If `body` changed:
     a. Create `ChangesetCommentRevision` with the *old* body.
     b. Update comment's `body` and `updated_at`.
  2. If `resolved` changed: update `resolved` field.
  3. Emit audit event.
- **Response 200:** `{ "data": CommentResponse }`
- **Errors:** 404, 403 (non-author editing body)

### `POST /api/repos/:appId/changesets/:changesetId/analyze`

AI analysis of changeset diff, scoped to the changeset's files.

- **Auth:** Any app member
- **Body:** `AnalyzeRequest`
- **Logic:**
  1. Fetch semantic diff.
  2. Send to AI service with changeset context.
  3. Return structured analysis.
- **Response 200:** `{ "data": AnalyzeResponse }`
- **Errors:** 502 (AI service error)

### `POST /api/repos/:appId/changesets/:changesetId/chat`

AI chat scoped to the changeset or specific files within it.

- **Auth:** Any app member
- **Body:** `ChatRequest`
- **Logic:**
  1. Build context from changeset diff and optionally scoped files.
  2. Send to AI service.
  3. Return reply.
- **Response 200:** `{ "data": ChatResponse }`
- **Errors:** 502 (AI service error)

## 6. Business Logic

### Changeset State Machine

Complete transition table with guard conditions:

| From | Action | To | Guard |
|------|--------|----|-------|
| `Draft` | `Submit` | `Submitted` | Caller is author; workspace head_sha differs from base_sha |
| `Submitted` | `Approve` | `InReview` | Caller has reviewer+ role; caller is not author |
| `Submitted` | `RequestChanges` | `InReview` | Caller has reviewer+ role; caller is not author |
| `Submitted` | `Reject` | `InReview` | Caller has reviewer+ role; caller is not author |
| `Submitted` | `Resubmit` | `Submitted` | Caller is author; new head_sha differs from current |
| `InReview` | `Approve` | `InReview` or `Approved` | Caller has reviewer+ role; caller is not author. Auto-transitions to `Approved` when `approval_count >= required_approval_count` |
| `InReview` | `RequestChanges` | `ChangesRequested` | Caller has reviewer+ role; caller is not author. Resets `approval_count` to 0 |
| `InReview` | `Reject` | `Rejected` | Caller has reviewer+ role; caller is not author |
| `InReview` | `Resubmit` | `Submitted` | Caller is author; resets `approval_count` to 0 |
| `ChangesRequested` | `Approve` | `InReview` | Caller has reviewer+ role |
| `ChangesRequested` | `RequestChanges` | `ChangesRequested` | Caller has reviewer+ role |
| `ChangesRequested` | `Reject` | `Rejected` | Caller has reviewer+ role |
| `ChangesRequested` | `Resubmit` | `Submitted` | Caller is author; resets `approval_count` to 0 |
| `ChangesRequested` | `MoveToDraft` | `Draft` | Author (own) or config_manager+ (any) |
| `Approved` | `Queue` | `Queued` | Caller is config_manager+ |
| `Queued` | `Release` | `Released` | System only (release publish flow) |
| `Queued` | `MarkConflicted` | `Conflicted` | System only (revalidation job) |
| `Queued` | `MarkNeedsRevalidation` | `NeedsRevalidation` | System only (revalidation job) |
| `Conflicted` | `MoveToDraft` | `Draft` | Author (own) or config_manager+ (any) |
| `NeedsRevalidation` | `MoveToDraft` | `Draft` | Author (own) or config_manager+ (any) |
| `Released` | - | - | Terminal |
| `Rejected` | - | - | Terminal |

### Submit Flow

1. Validate changeset is in `Draft`.
2. Call `CommitService.FindCommit` to resolve the workspace's current `head_sha`.
3. Confirm workspace has diverged from `base_sha` (i.e., there are actual changes).
4. Freeze `head_sha` on changeset.
5. Set `current_revision = 1`.
6. Create `ChangesetRevision { revision_number: 1, head_sha }`.
7. Transition state to `Submitted`.
8. Emit audit event.

### Resubmit Flow

1. Validate changeset is in `Submitted`, `InReview`, or `ChangesRequested`.
2. Resolve workspace's current `head_sha`.
3. Confirm new `head_sha` differs from the currently frozen value.
4. Increment `current_revision`.
5. Update changeset: new `head_sha`, `approval_count = 0`.
6. Create new `ChangesetRevision`.
7. Transition state to `Submitted`.
8. Emit audit event recording the approval reset.

### Review Flow

1. Validate reviewer role.
2. Parse decision.
3. On `Approve`:
   - Increment `approval_count`.
   - Check if `approval_count >= required_approval_count`.
   - If threshold met, auto-transition to `Approved`.
   - Otherwise remain in `InReview`.
4. On `RequestChanges`:
   - Reset `approval_count` to 0.
   - Transition to `ChangesRequested`.
5. On `Reject`:
   - Transition to `Rejected` (terminal).
6. Persist `ChangesetReview` record with current `revision_number`.
7. Emit audit event.

### Approval Threshold

- Default `required_approval_count` is 1 (from app settings, carried to changeset on creation).
- When `approval_count >= required_approval_count` after an `Approve` review, the changeset auto-transitions from `InReview` to `Approved`.
- Resubmit always resets `approval_count` to 0, requiring re-approval from scratch.

### One Open Changeset Per Workspace

Enforced at two levels:

1. **Database:** Partial unique index on `(app_id, workspace_id)` where `state NOT IN [released, rejected]`.
2. **Application:** Pre-check in `POST /api/repos/:appId/changesets` handler queries for existing open changeset before insert.

The partial unique index provides a safety net against race conditions.

### Semantic Diff

The semantic diff pipeline parses DxFlow configuration files and produces
structured change descriptions. Processing steps:

1. **Identify changed files:** Use `DiffService.DiffStats` to get the list of modified paths.
2. **Fetch file content:** For each changed file, use `BlobService.GetBlobs` to read both the base version (at `base_sha`) and the head version (at `head_sha`).
3. **Classify config type:** Determine the `SemanticConfigType` from the file path and/or JSON structure:
   - `entities/**/*.json` -> `Entity`
   - `pages/**/*.json` -> `Page`
   - `queues/**/*.json` -> `Queue`
   - `providers/**/*.json` -> `Provider`
   - `workflows/**/*.json` -> `Workflow`
   - `teams/**/*.json` -> `Team`
   - `menus/**/*.json` -> `Menu`
   - `assets/**/*` -> `Asset`
   - `scripts/**/*` -> `Script`
4. **Parse and compare:** For JSON config types, deserialize both versions into `serde_json::Value` trees and perform structural diff:
   - Top-level key additions -> `Added`
   - Top-level key removals -> `Removed`
   - Value changes at known semantic paths -> `Modified` with field-level detail
   - Path changes (same content, different location) -> `Moved`
5. **Generate descriptions:** Each `SemanticChange` includes a human-readable `description` summarizing what changed (e.g., "Added field `retryCount` to entity `PaymentProvider`").
6. **Aggregate summary:** Count changes per config type for the `summary.by_config_type` map.

For non-JSON files (scripts, assets), the diff falls back to reporting the file
as added/modified/removed without structural detail.

### Comment Edit with Revision History

When a comment's body is updated:

1. Save the *current* body as a `ChangesetCommentRevision` (snapshot-before-edit pattern).
2. Overwrite the comment's `body` with the new text.
3. Update `updated_at`.

This means the `changeset_comment_revisions` collection stores all prior
versions, and the current version lives on the comment document itself.

## 7. Gitaly-rs Integration

All RPCs reference a `Repository` message built from the `App`:

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

### `DiffService.CommitDiff`

Generates per-file patch data between two commits. Used for detailed file-level
diff display.

```protobuf
// RPC signature
rpc CommitDiff(CommitDiffRequest) returns (stream CommitDiffResponse);

// Request
message CommitDiffRequest {
  Repository repository = 1;        // target repo
  string left_commit_id = 2;        // base_sha
  string right_commit_id = 3;       // head_sha
  repeated bytes paths = 5;         // optional: limit to specific paths
  bool collapse_diffs = 6;          // empty patches after safe limits
  bool enforce_limits = 7;          // stop parsing at max limits
  int32 max_files = 8;
  int32 max_lines = 9;
  int32 max_bytes = 10;
  int32 safe_max_files = 11;
  int32 safe_max_lines = 12;
  int32 safe_max_bytes = 13;
  int32 max_patch_bytes = 14;       // per-file patch limit
  DiffMode diff_mode = 15;          // DEFAULT or WORDDIFF
  WhitespaceChanges whitespace_changes = 17;
}

// Response (streamed, chunked per file)
message CommitDiffResponse {
  bytes from_path = 1;              // old path
  bytes to_path = 2;                // new path
  string from_id = 3;               // old blob OID
  string to_id = 4;                 // new blob OID
  int32 old_mode = 5;
  int32 new_mode = 6;
  bool binary = 7;
  bytes raw_patch_data = 9;         // chunked patch bytes
  bool end_of_patch = 10;           // marks last chunk for this file
  bool overflow_marker = 11;
  bool collapsed = 12;
  bool too_large = 13;
  int32 lines_added = 14;
  int32 lines_removed = 15;
}
```

**Conman usage:** For the raw diff mode in
`GET /changesets/:changesetId/diff?mode=raw`, call `CommitDiff` with
`left_commit_id = base_sha` and `right_commit_id = head_sha`. Collect all
streamed `CommitDiffResponse` messages, reassembling chunked `raw_patch_data`
per file (concatenate until `end_of_patch = true`).

### `DiffService.RawDiff`

Returns the complete unified diff as raw bytes. Simpler than `CommitDiff` when
you need the full patch output without per-file parsing.

```protobuf
rpc RawDiff(RawDiffRequest) returns (stream RawDiffResponse);

message RawDiffRequest {
  Repository repository = 1;
  string left_commit_id = 2;        // base_sha
  string right_commit_id = 3;       // head_sha
}

message RawDiffResponse {
  bytes data = 1;                   // chunked raw diff output
}
```

**Conman usage:** Stream all response chunks and concatenate `data` fields to
produce the full unified diff string. Used as the `patch` field in
`RawDiffResponse`.

### `DiffService.DiffStats`

Returns per-file addition/deletion counts without patch data.

```protobuf
rpc DiffStats(DiffStatsRequest) returns (stream DiffStatsResponse);

message DiffStatsRequest {
  Repository repository = 1;
  string left_commit_id = 2;        // base_sha
  string right_commit_id = 3;       // head_sha
}

message DiffStatsResponse {
  repeated DiffStats stats = 1;
}

message DiffStats {
  bytes path = 1;                   // file path
  int32 additions = 2;
  int32 deletions = 3;
  bytes old_path = 4;               // set on rename
}
```

**Conman usage:** Called in both raw and semantic diff modes. For raw mode,
provides the `stats` array. For semantic mode, identifies which files changed
so we know which blobs to fetch.

### `CommitService.FindCommit`

Resolves a commitish to a full `GitCommit` object.

```protobuf
rpc FindCommit(FindCommitRequest) returns (FindCommitResponse);

message FindCommitRequest {
  Repository repository = 1;
  bytes revision = 2;               // commitish (SHA, branch name, tag)
}

message FindCommitResponse {
  GitCommit commit = 1;             // nil if not found
}

message GitCommit {
  string id = 1;                    // full SHA
  bytes subject = 2;
  bytes body = 3;
  CommitAuthor author = 4;
  CommitAuthor committer = 5;
  repeated string parent_ids = 6;
  string tree_id = 9;
}
```

**Conman usage:**
- On **submit/resubmit**: resolve workspace branch to get the current `head_sha`.
  Call with `revision = "refs/heads/ws/<user>/<app>"`.
- On **changeset creation**: validate that the `base_sha` resolves to an existing commit.

### `CommitService.ListCommits`

Lists commits reachable from a set of revisions. Used to enumerate the commit
range between base and head.

```protobuf
rpc ListCommits(ListCommitsRequest) returns (stream ListCommitsResponse);

message ListCommitsRequest {
  Repository repository = 1;
  repeated string revisions = 2;    // e.g. ["base_sha..head_sha"]
  PaginationParameter pagination_params = 3;
  Order order = 4;                  // NONE, TOPO, DATE
}

message ListCommitsResponse {
  repeated GitCommit commits = 1;
  PaginationCursor pagination_cursor = 2;
}
```

**Conman usage:** To list all commits in a changeset's range, call with
`revisions = ["<base_sha>..<head_sha>"]`. This provides the commit log for
changeset detail views and AI analysis context.

### `BlobService.GetBlobs`

Retrieves blob content by revision + path pairs. Used for semantic diff to read
file content at both base and head.

```protobuf
rpc GetBlobs(GetBlobsRequest) returns (stream GetBlobsResponse);

message GetBlobsRequest {
  message RevisionPath {
    string revision = 1;            // commit SHA or ref
    bytes path = 2;                 // file path in tree
  }

  Repository repository = 1;
  repeated RevisionPath revision_paths = 2;
  int64 limit = 3;                  // max bytes per blob (-1 = unlimited)
}

message GetBlobsResponse {
  int64 size = 1;                   // blob size (first message only)
  bytes data = 2;                   // chunked content
  string oid = 3;                   // blob OID (first message only)
  bool is_submodule = 4;
  int32 mode = 5;
  string revision = 6;
  bytes path = 7;
  ObjectType type = 8;
}
```

**Conman usage:** For semantic diff, build two `RevisionPath` entries per
changed file:

```rust
let revision_paths = changed_files.iter().flat_map(|f| vec![
    RevisionPath { revision: base_sha.clone(), path: f.path.clone() },
    RevisionPath { revision: head_sha.clone(), path: f.path.clone() },
]).collect();
```

Collect streamed responses, reassembling chunked `data` per blob (new blob
starts when `oid` is set). Parse the resulting content as JSON for structural
comparison.

## 8. Implementation Checklist

### E05-01: Changeset CRUD

- [ ] Add `Changeset`, `ChangesetState`, `ChangesetAction` types to `conman-core`
- [ ] Implement `transition()` function with all state transitions
- [ ] Implement `check_approval_threshold()` helper
- [ ] Add `ChangesetRepo` to `conman-db` with CRUD operations
- [ ] Create `changesets` collection indexes (including partial unique index)
- [ ] Add `POST /api/repos/:appId/changesets` handler
- [ ] Add `GET /api/repos/:appId/changesets` handler with pagination + state filter
- [ ] Add `GET /api/repos/:appId/changesets/:changesetId` handler
- [ ] Add `PATCH /api/repos/:appId/changesets/:changesetId` handler
- [ ] Add one-open-changeset-per-workspace pre-check
- [ ] Add audit event emission for changeset creation
- [ ] Unit tests for state machine transitions (all valid + all invalid)
- [ ] Unit tests for one-open-changeset constraint

### E05-02: Submit / Resubmit

- [ ] Add `ChangesetRevision` type to `conman-core`
- [ ] Add `ChangesetRevisionRepo` to `conman-db` with insert and list operations
- [ ] Create `changeset_revisions` collection indexes
- [ ] Add `POST /submit` handler (freeze head_sha, create revision, transition)
- [ ] Add `POST /resubmit` handler (new head_sha, increment revision, reset approvals)
- [ ] Integrate `CommitService.FindCommit` to resolve workspace head
- [ ] Validate head_sha differs from base_sha on submit
- [ ] Validate head_sha differs from current head_sha on resubmit
- [ ] Add audit events for submit and resubmit
- [ ] Unit tests for submit flow
- [ ] Unit tests for resubmit flow (approval reset, revision increment)

### E05-03: Approval Workflow

- [ ] Implement approval threshold check (`approval_count >= required_approval_count`)
- [ ] Implement auto-transition from `InReview` to `Approved` on threshold
- [ ] Implement approval reset on resubmit (tested in E05-02)
- [ ] Implement approval reset on `RequestChanges`
- [ ] Integration test: full submit -> approve -> auto-approved flow
- [ ] Integration test: submit -> approve -> resubmit -> approvals reset to 0

### E05-04: Review Actions

- [ ] Add `ChangesetReview`, `ReviewDecision` types to `conman-core`
- [ ] Add `ChangesetReviewRepo` to `conman-db`
- [ ] Create `changeset_reviews` collection indexes
- [ ] Add `POST /review` handler
- [ ] Enforce reviewer+ role check (not author)
- [ ] Handle `Approve`, `RequestChanges`, `Reject` decisions
- [ ] Emit audit events for each review decision
- [ ] Unit tests for each review decision
- [ ] Unit tests for self-review acceptance when caller has reviewer-capable role
- [ ] Unit tests for insufficient role rejection

### E05-05: Diff Endpoints

- [ ] Add `SemanticChange`, `SemanticDiffResponse`, related types to `conman-core`
- [ ] Add `conman-git` methods for `DiffService.RawDiff`, `DiffService.DiffStats`, `DiffService.CommitDiff`
- [ ] Add `conman-git` method for `BlobService.GetBlobs`
- [ ] Add `GET /diff?mode=raw` handler
- [ ] Add `GET /diff?mode=semantic` handler
- [ ] Implement config type classifier (path-based)
- [ ] Implement JSON structural diff comparator
- [ ] Implement `SemanticChange` generation from structural diffs
- [ ] Unit tests for config type classification
- [ ] Unit tests for JSON structural diff
- [ ] Integration test for raw diff end-to-end
- [ ] Integration test for semantic diff end-to-end

### E05-06: Comments

- [ ] Add `ChangesetComment`, `ChangesetCommentRevision` types to `conman-core`
- [ ] Add `ChangesetCommentRepo`, `ChangesetCommentRevisionRepo` to `conman-db`
- [ ] Create `changeset_comments` and `changeset_comment_revisions` indexes
- [ ] Add `GET /comments` handler with pagination
- [ ] Add `POST /comments` handler (file+line anchoring, threading)
- [ ] Add `PATCH /comments/:commentId` handler (body edit with revision history, resolve toggle)
- [ ] Validate parent_comment_id belongs to same changeset
- [ ] Emit audit events for comment create and edit
- [ ] Unit tests for comment creation (top-level, inline, threaded)
- [ ] Unit tests for comment edit with revision history
- [ ] Unit tests for resolve/unresolve toggle

### E05-07: AI Endpoints

- [ ] Add `AnalyzeRequest`, `AnalyzeResponse`, `ChatRequest`, `ChatResponse` types
- [ ] Add `POST /analyze` handler
- [ ] Add `POST /chat` handler
- [ ] Define AI service interface (trait) for testability
- [ ] Build context assembly from changeset diff
- [ ] Integration test with mock AI service
- [ ] Unit test for context assembly logic

## 9. Test Cases

### State Machine Unit Tests

| # | Test | Input | Expected |
|---|------|-------|----------|
| 1 | Draft to Submitted | `transition(Draft, Submit)` | `Ok(Submitted)` |
| 2 | Submitted to InReview on approve | `transition(Submitted, Approve)` | `Ok(InReview)` |
| 3 | Submitted to InReview on request changes | `transition(Submitted, RequestChanges)` | `Ok(InReview)` |
| 4 | Submitted to InReview on reject | `transition(Submitted, Reject)` | `Ok(InReview)` |
| 5 | InReview stays InReview on approve (threshold not met) | `transition(InReview, Approve)` then `check_approval_threshold(InReview, 1, 2)` | `InReview` |
| 6 | InReview to Approved on approve (threshold met) | `transition(InReview, Approve)` then `check_approval_threshold(InReview, 2, 2)` | `Approved` |
| 7 | InReview to ChangesRequested | `transition(InReview, RequestChanges)` | `Ok(ChangesRequested)` |
| 8 | InReview to Rejected | `transition(InReview, Reject)` | `Ok(Rejected)` |
| 9 | ChangesRequested to Submitted on resubmit | `transition(ChangesRequested, Resubmit)` | `Ok(Submitted)` |
| 10 | ChangesRequested to Draft on move | `transition(ChangesRequested, MoveToDraft)` | `Ok(Draft)` |
| 11 | Approved to Queued | `transition(Approved, Queue)` | `Ok(Queued)` |
| 12 | Queued to Released | `transition(Queued, Release)` | `Ok(Released)` |
| 13 | Queued to Conflicted | `transition(Queued, MarkConflicted)` | `Ok(Conflicted)` |
| 14 | Queued to NeedsRevalidation | `transition(Queued, MarkNeedsRevalidation)` | `Ok(NeedsRevalidation)` |
| 15 | Conflicted to Draft | `transition(Conflicted, MoveToDraft)` | `Ok(Draft)` |
| 16 | NeedsRevalidation to Draft | `transition(NeedsRevalidation, MoveToDraft)` | `Ok(Draft)` |
| 17 | Released is terminal | `transition(Released, Submit)` | `Err(InvalidTransition)` |
| 18 | Rejected is terminal | `transition(Rejected, Submit)` | `Err(InvalidTransition)` |
| 19 | Draft cannot be approved | `transition(Draft, Approve)` | `Err(InvalidTransition)` |
| 20 | Submitted cannot be queued | `transition(Submitted, Queue)` | `Err(InvalidTransition)` |
| 21 | InReview resubmit | `transition(InReview, Resubmit)` | `Ok(Submitted)` |
| 22 | Submitted resubmit | `transition(Submitted, Resubmit)` | `Ok(Submitted)` |

### Integration Tests

| # | Test | Setup | Steps | Assertions |
|---|------|-------|-------|------------|
| 23 | Create changeset | App + workspace with commits | `POST /changesets` | 201, state=draft, base_sha set, approval_count=0 |
| 24 | One-open-per-workspace | Existing open changeset | `POST /changesets` with same workspace | 409 Conflict |
| 25 | Second changeset after release | Released changeset on workspace | `POST /changesets` | 201 (partial unique index allows it) |
| 26 | Submit freezes head_sha | Draft changeset + workspace with commits | `POST /submit` | State=submitted, head_sha frozen, revision=1 created |
| 27 | Submit empty changeset | Draft changeset where base_sha == head_sha | `POST /submit` | 400 validation error |
| 28 | Resubmit creates new revision | Submitted changeset + new workspace commit | `POST /resubmit` | New revision created, approval_count=0, new head_sha |
| 29 | Resubmit with no changes | Submitted changeset, same head_sha | `POST /resubmit` | 400 (no changes) |
| 30 | Approve flow | Submitted changeset | `POST /review` (approve) | State transitions through InReview, approval_count=1 |
| 31 | Approve meets threshold | Changeset with required_approval_count=1 | `POST /review` (approve) | Auto-transitions to Approved |
| 32 | Self-review blocked | Changeset authored by user A | `POST /review` as user A | 403 Forbidden |
| 33 | Member role cannot review | Changeset, caller has `member` role | `POST /review` | 403 Forbidden |
| 34 | Request changes resets approvals | InReview changeset with approval_count=1 | `POST /review` (changes_requested) | approval_count=0, state=ChangesRequested |
| 35 | Reject is terminal | Submitted changeset | `POST /review` (reject) | State=Rejected, no further transitions possible |
| 36 | Queue approved changeset | Approved changeset | `POST /queue` as config_manager | State=Queued |
| 37 | Queue by member role blocked | Approved changeset | `POST /queue` as member | 403 Forbidden |
| 38 | Move conflicted to draft | Conflicted changeset (author) | `POST /move-to-draft` | State=Draft, approval_count=0 |
| 39 | Move to draft by non-author non-admin | Conflicted changeset, caller has different member role | `POST /move-to-draft` | 403 Forbidden |
| 40 | Raw diff | Changeset with known changes | `GET /diff?mode=raw` | Returns unified diff and stats |
| 41 | Semantic diff | Changeset with entity JSON changes | `GET /diff?mode=semantic` | Returns classified SemanticChanges |
| 42 | Create top-level comment | Existing changeset | `POST /comments` | 201, comment stored |
| 43 | Create inline comment | Changeset with file changes | `POST /comments` with file_path+line_number | 201, anchored to file |
| 44 | Create threaded reply | Existing comment | `POST /comments` with parent_comment_id | 201, linked to parent |
| 45 | Edit comment stores revision | Existing comment | `PATCH /comments/:id` with new body | Comment body updated, prior body in revisions collection |
| 46 | Resolve comment | Existing unresolved comment | `PATCH /comments/:id` with resolved=true | resolved=true |
| 47 | Update draft changeset | Draft changeset | `PATCH /changesets/:id` with new title | 200, title updated |
| 48 | Update non-draft changeset blocked | Submitted changeset | `PATCH /changesets/:id` | 409 wrong state |
| 49 | Full lifecycle | App + workspace | Create -> submit -> approve -> queue | All transitions succeed, revisions recorded |
| 50 | Resubmit during review | InReview changeset, new commit | Resubmit -> re-approve -> queue | Approval reset verified, new revision recorded |

## 10. Acceptance Criteria

1. **State machine correctness:** All transitions in section 6 are enforced. Invalid transitions return `409 Conflict` with `invalid_transition` error code. Terminal states (`Released`, `Rejected`) reject all actions.

2. **One open changeset per workspace:** Creating a second open changeset for the same workspace returns `409 Conflict`. After a changeset reaches a terminal state, a new one can be created.

3. **Submit freezes head_sha:** After `POST /submit`, the changeset's `head_sha` matches the workspace branch HEAD at the time of submit, and a `ChangesetRevision` record exists with `revision_number = 1`.

4. **Resubmit resets approvals:** After `POST /resubmit`, `approval_count = 0`, `current_revision` is incremented, a new `ChangesetRevision` record exists, and the new `head_sha` is frozen.

5. **Approval threshold auto-transition:** When `approval_count >= required_approval_count` after an `Approve` review, the changeset automatically transitions to `Approved` without additional API calls.

6. **Review role enforcement:** Only users with `reviewer`, `config_manager`, or `admin` role can submit reviews. Self-review is allowed when the author has one of these roles.

7. **Diff modes:** `GET /diff?mode=raw` returns a unified diff with per-file stats. `GET /diff?mode=semantic` returns classified `SemanticChange` entries with config type, operation, target, and description.

8. **Comment revision history:** Editing a comment body creates a `ChangesetCommentRevision` record preserving the prior body text. The comment's `updated_at` is refreshed.

9. **Comment threading:** Comments with `parent_comment_id` form threads. The parent must belong to the same changeset.

10. **Audit trail:** Every mutation (create, submit, resubmit, review, queue, move-to-draft, comment create, comment edit) emits an audit event with `entity_type = "changeset"` or `"changeset_comment"`, including before/after state snapshots.

11. **Queue and recovery:** `config_manager+` can queue approved changesets. `Conflicted` and `NeedsRevalidation` changesets can be moved to draft by their author or by `config_manager+`.

12. **AI endpoints respond:** `POST /analyze` and `POST /chat` return structured responses scoped to the changeset's diff context. Failures in the AI service return `502 Bad Gateway`.

13. **Profile overrides are tracked and auditable:** Changeset-level runtime
    profile overrides are stored in `changeset_profile_overrides`, included in
    release flow, and emit audit events on create/update.

14. **Secret diffs are metadata-only:** Review and diff surfaces show secret
    key operations (added/rotated/deleted) without exposing plaintext values.
