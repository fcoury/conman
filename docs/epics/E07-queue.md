# E07 Queue-First Orchestration + Revalidation

## 1. Goal

Move approved changesets into a managed queue with automatic revalidation after
each published release. The queue is the staging area between review approval and
release assembly: config managers select a subset of queued changesets to include
in each release, while non-selected changesets remain queued and are
automatically revalidated to ensure they stay compatible with the updated integration branch
branch.

Key outcomes:

- Approved changesets transition to `Queued` with deterministic ordering.
- Config managers can manually reorder the queue (audited).
- After every release publish, remaining queued changesets are revalidated:
  conflict detection against updated integration branch, followed by full msuite test run.
- Changesets that fail revalidation transition to `Conflicted` or
  `NeedsRevalidation` and can be returned to `Draft` by the author or a
  privileged user.
- A changeset branch must be up to date with the integration branch before it can enter the queue.
- Queued changesets with conflicting runtime override keys are marked
  `Conflicted` (later entry loses), except when the overlapping key resolves to
  the same typed value.

## 2. Dependencies

| Dependency | What it provides |
|------------|-----------------|
| **E05 Changesets** | Changeset domain types, `ChangesetState` enum, state machine transitions through `Approved` |
| **E06 Async Jobs** | Job framework, `revalidate_queued_changeset` job type, msuite worker infrastructure |

Both E05 and E06 must be complete before E07 implementation begins. E07 also
assumes the gitaly-rs integration layer from E01 is available for conflict
detection and ancestry checks.

## 3. Rust Types

All types live in `conman-core` unless otherwise noted.

### 3.1 Changeset queue fields

Extend the existing `Changeset` struct with queue-specific metadata:

```rust
// conman-core/src/changeset.rs

use chrono::{DateTime, Utc};

/// Extended fields on the Changeset struct for queue orchestration.
/// These are None when the changeset is not in the Queued state.
pub struct Changeset {
    // ... existing fields from E05 ...

    /// Position in the per-app queue. Assigned on Approved -> Queued transition.
    /// Monotonically increasing within an app; gaps are allowed after reordering.
    pub queue_position: Option<i64>,

    /// Timestamp when the changeset entered the queue.
    pub queued_at: Option<DateTime<Utc>>,
}
```

### 3.2 QueueEntry view struct

API-facing DTO returned by the queue listing endpoint. Lives in `conman-api`:

```rust
// conman-api/src/dto/queue.rs

use chrono::{DateTime, Utc};
use serde::Serialize;

/// A single entry in the app queue, combining changeset summary data with its
/// queue position for ordered display.
#[derive(Debug, Clone, Serialize)]
pub struct QueueEntry {
    pub changeset_id: String,
    pub title: String,
    pub author_user_id: String,
    pub author_email: String,
    pub workspace_branch: String,
    pub head_sha: String,
    pub queue_position: i64,
    pub queued_at: DateTime<Utc>,
    pub last_revalidation_status: Option<String>,
    pub last_revalidation_job_id: Option<String>,
}
```

### 3.3 ReorderRequest

```rust
// conman-api/src/dto/queue.rs

use serde::Deserialize;

/// Request body for the queue reorder endpoint. The caller provides the full
/// ordered list of changeset IDs representing the desired queue order. Every
/// currently-queued changeset for the app must appear exactly once.
#[derive(Debug, Clone, Deserialize)]
pub struct ReorderRequest {
    /// Ordered list of changeset IDs (hex ObjectId strings). Position 0 is
    /// highest priority (will be considered first for next release assembly).
    pub ordered_changeset_ids: Vec<String>,
}
```

### 3.4 RevalidationResult

```rust
// conman-core/src/revalidation.rs

/// Outcome of revalidating a single queued changeset against the updated integration branch
/// branch after a release publish.
#[derive(Debug, Clone)]
pub enum RevalidationResult {
    /// Changeset branch merges cleanly with integration branch and msuite tests pass.
    /// The changeset remains in the Queued state.
    Valid,

    /// Changeset branch has merge conflicts with the updated integration branch.
    /// Includes the list of conflicting file paths.
    Conflicted {
        files: Vec<String>,
    },

    /// Changeset branch merges cleanly but the msuite revalidation job failed.
    /// Includes the job ID for log retrieval.
    TestFailed {
        job_id: String,
    },
}
```

### 3.5 State transition extensions

Extend the `ChangesetState` enum and transition logic from E05:

```rust
// conman-core/src/changeset.rs

/// All changeset states. Queue-related states added by E07 are marked below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangesetState {
    Draft,
    Submitted,
    InReview,
    Approved,
    Queued,              // E07: entered via Approved -> Queued
    Conflicted,          // E07: entered via Queued -> Conflicted
    NeedsRevalidation,   // E07: entered via Queued -> NeedsRevalidation
    Released,
    Rejected,
}

impl ChangesetState {
    /// Validate that a state transition is allowed. Returns ConmanError on
    /// invalid transitions.
    pub fn validate_transition(&self, to: ChangesetState) -> Result<(), ConmanError> {
        let valid = matches!(
            (self, to),
            // ... existing transitions from E05 ...
            // E07 transitions:
            (ChangesetState::Approved, ChangesetState::Queued) |
            (ChangesetState::Queued, ChangesetState::Conflicted) |
            (ChangesetState::Queued, ChangesetState::NeedsRevalidation) |
            (ChangesetState::Queued, ChangesetState::Released) |
            (ChangesetState::Conflicted, ChangesetState::Draft) |
            (ChangesetState::NeedsRevalidation, ChangesetState::Draft)
        );

        if !valid {
            return Err(ConmanError::InvalidTransition {
                from: format!("{:?}", self),
                to: format!("{:?}", to),
            });
        }

        Ok(())
    }
}
```

## 4. Database

### 4.1 Collection updates

Update the `changesets` collection schema with new fields:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `queue_position` | `i64` / `null` | `null` | Position in queue. Set on queue entry, updated on reorder, cleared on exit. |
| `queued_at` | `DateTime` / `null` | `null` | Timestamp of Approved -> Queued transition. Cleared on exit from queue. |

These fields are nullable because they only have values while
`state == "queued"`.

### 4.2 Indexes

Add to `ChangesetRepo::ensure_indexes()`:

```rust
// Compound index for ordered queue listing per app.
// Supports: GET /api/repos/:appId/queue (list queued changesets in order).
IndexModel::builder()
    .keys(doc! {
        "app_id": 1,
        "state": 1,
        "queue_position": 1,
    })
    .options(
        IndexOptions::builder()
            .name("idx_changesets_app_queue".to_string())
            .partial_filter_expression(doc! {
                "state": "queued",
                "queue_position": { "$exists": true },
            })
            .build(),
    )
    .build()
```

This partial index only includes documents in the `queued` state, keeping it
compact. Queries filter on `app_id` + `state: "queued"` and sort by
`queue_position: 1`.

## 5. API Endpoints

### 5.1 `POST /api/repos/:appId/changesets/:changesetId/queue`

Move an approved changeset into the queue.

**Auth:** changeset author (any role), config_manager, or admin.

**Guards:**
- Changeset must be in `Approved` state.
- Changeset branch must be up to date with the integration branch (ancestry check).

**Request body:** none

**Response:** `200 OK`

```json
{
  "data": {
    "changeset_id": "abc123",
    "state": "queued",
    "queue_position": 5,
    "queued_at": "2026-02-25T14:30:00Z"
  }
}
```

**Error cases:**
- `409 Conflict` if state is not `Approved`.
- `409 Conflict` if changeset branch is not up to date with the integration branch.
- `404 Not Found` if changeset or app does not exist.

### 5.2 `GET /api/repos/:appId/queue?page=&limit=`

List queued changesets in queue-position order. Convenience endpoint that
filters `state == queued` and sorts by `queue_position ASC`.

**Auth:** any app member.

**Response:** `200 OK`

```json
{
  "data": [
    {
      "changeset_id": "abc123",
      "title": "Add team configuration",
      "author_user_id": "user456",
      "author_email": "alice@example.com",
      "workspace_branch": "ws/alice/myapp",
      "head_sha": "d4e5f6...",
      "queue_position": 1,
      "queued_at": "2026-02-24T10:00:00Z",
      "last_revalidation_status": null,
      "last_revalidation_job_id": null
    }
  ],
  "pagination": { "page": 1, "limit": 20, "total": 3 }
}
```

### 5.3 `POST /api/repos/:appId/queue/reorder`

Reorder the entire queue for an app. The request must include every
currently-queued changeset ID exactly once, in the desired new order.

**Auth:** config_manager or admin.

**Request body:**

```json
{
  "ordered_changeset_ids": ["id3", "id1", "id2"]
}
```

**Response:** `200 OK`

```json
{
  "data": {
    "reordered_count": 3
  }
}
```

**Error cases:**
- `403 Forbidden` if caller lacks config_manager or admin role.
- `400 Validation` if the provided IDs do not match the current queued set
  exactly (missing IDs, extra IDs, or duplicates).
- `409 Conflict` if the queue was modified concurrently (optimistic check).

**Audit:** emits `queue_reordered` audit event with before/after position maps.

### 5.4 `POST /api/repos/:appId/changesets/:changesetId/move-to-draft`

Return a conflicted or needs_revalidation changeset to draft state so the
author can address the issues.

**Auth:**
- Changeset author can move their own.
- config_manager or admin can move any.

**Guards:**
- Changeset must be in `Conflicted` or `NeedsRevalidation` state.

**Request body:** none

**Response:** `200 OK`

```json
{
  "data": {
    "changeset_id": "abc123",
    "state": "draft",
    "queue_position": null,
    "queued_at": null
  }
}
```

**Side effects:**
- Clears `queue_position` and `queued_at`.
- Resets `last_revalidation_status` and `last_revalidation_job_id`.

## 6. Business Logic

### 6.1 Queue transition (Approved -> Queued)

1. Verify changeset is in `Approved` state.
2. **Up-to-date gate:** call `CommitIsAncestor` to confirm that current integration branch
   HEAD is an ancestor of the changeset branch HEAD. If integration branch has advanced past
   the changeset's base, the author must sync their workspace first.
3. Assign `queue_position` as `max(queue_position for app) + 1`. Use a
   find-and-increment pattern to avoid gaps under concurrency. If the queue is
   empty, start at position 1.
4. Set `queued_at` to `Utc::now()`.
5. Transition state to `Queued`.
6. Emit `changeset_queued` audit event and notification.

### 6.2 Queue reorder

1. Verify caller has config_manager or admin role.
2. Load all changesets for the app where `state == "queued"`, sorted by current
   `queue_position`.
3. Validate that `ordered_changeset_ids` is a permutation of the current queued
   set (same IDs, no additions, no removals, no duplicates).
4. Assign new `queue_position` values: position `i * 1000` for index `i` (using
   gaps to allow future insertions without reorder).
5. Write all position updates in a single MongoDB `bulkWrite` with ordered
   updates.
6. Emit `queue_reordered` audit event capturing the before and after position
   maps for full traceability.

### 6.3 Revalidation trigger

After a release is published (by E08), the release publish handler must trigger
revalidation for all remaining queued changesets in the same app. The flow:

1. Query all changesets for the app where `state == "queued"`, ordered by
   `queue_position`.
2. Resolve current integration branch HEAD via `FindCommit` (integration branch has just been updated by the
   release publish).
3. For each queued changeset, enqueue a `revalidate_queued_changeset` job (from
   E06 job framework). Jobs are enqueued in queue-position order so
   higher-priority changesets are validated first.

### 6.4 Revalidation job logic

Each `revalidate_queued_changeset` job executes the following steps:

1. **Conflict detection:** use `DiffService.CommitDiff` to compute the diff
   between the changeset's `head_sha` and updated integration branch HEAD. If the diff
   reveals that both sides modified the same files, check for actual merge
   conflicts by attempting a trial merge via the git adapter. If conflicts
   exist:
   - Transition changeset to `Conflicted`.
   - Record conflicting file paths in `last_revalidation_status`.
   - Emit `changeset_conflicted` notification.
   - Job completes with `RevalidationResult::Conflicted`.

2. **Ancestry check:** use `CommitIsAncestor` to verify the changeset branch is
   still based on (or ahead of) the new integration branch. If not, treat as conflicted.

3. **msuite revalidation:** if no conflicts, trigger a full msuite test run
   (reuse the `msuite_merge` worker from E06). Wait for the job to complete.
   - If msuite passes: changeset remains `Queued`. Record
     `last_revalidation_status = "valid"`. Job completes with
     `RevalidationResult::Valid`.
   - If msuite fails: transition changeset to `NeedsRevalidation`. Record
     `last_revalidation_status = "test_failed"` and
     `last_revalidation_job_id`. Emit `changeset_needs_revalidation`
     notification. Job completes with `RevalidationResult::TestFailed`.

### 6.5 Move-to-draft

1. Verify changeset is in `Conflicted` or `NeedsRevalidation` state.
2. Verify caller is the changeset author, config_manager, or admin.
3. Clear queue metadata: set `queue_position = None`, `queued_at = None`.
4. Clear revalidation metadata: `last_revalidation_status = None`,
   `last_revalidation_job_id = None`.
5. Transition state to `Draft`.
6. Emit `changeset_moved_to_draft` audit event.

The author can then fix their workspace (resolve conflicts, update code), and
re-submit through the normal changeset flow (Draft -> Submitted -> InReview ->
Approved -> Queued).

### 6.6 Up-to-date-with-integration branch gate

Before a changeset can be queued, its branch must contain all commits from integration branch.
This is enforced by checking:

```
CommitIsAncestor(ancestor_id = main_HEAD, child_id = changeset_head_sha)
```

If this returns `false`, the changeset branch is behind integration branch and the queue
transition is rejected with a `409 Conflict` error instructing the user to
sync their workspace with integration branch first (via the workspace sync-integration endpoint
from E04).

## 7. Gitaly-rs Integration

### 7.1 `DiffService.CommitDiff` -- conflict detection

Used to detect which files were modified on both the changeset branch and integration branch
since they diverged. If the same file appears in both diffs, a merge conflict
is likely.

**Service definition** (from `proto/diff.proto`):

```protobuf
service DiffService {
  // CommitDiff returns a diff between two different commits. The patch data is
  // chunked across messages and get streamed back to the client.
  rpc CommitDiff(CommitDiffRequest) returns (stream CommitDiffResponse);
}

message CommitDiffRequest {
  // repository is the one from which to get the diff.
  Repository repository = 1;
  // left_commit_id is the left commit ID in <left commit>..<right commit>.
  string left_commit_id = 2;
  // right_commit_id is the right commit ID in <left commit>..<right commit>.
  string right_commit_id = 3;
  // paths is a list of paths that limits the diff to those specific paths.
  repeated bytes paths = 5;
  // collapse_diffs causes patches to be emptied after safe_max_files,
  // safe_max_lines, or safe_max_lines is reached.
  bool collapse_diffs = 6;
  // enforce_limits causes parsing of diffs to stop if max_files, max_lines,
  // or max_bytes is reached.
  bool enforce_limits = 7;
  int32 max_files = 8;
  int32 max_lines = 9;
  int32 max_bytes = 10;
  int32 safe_max_files = 11;
  int32 safe_max_lines = 12;
  int32 safe_max_bytes = 13;
  int32 max_patch_bytes = 14;
  // collect_all_paths returns all file paths even when limits are hit.
  bool collect_all_paths = 18;
}

message CommitDiffResponse {
  bytes from_path = 1;
  bytes to_path = 2;
  string from_id = 3;
  string to_id = 4;
  int32 old_mode = 5;
  int32 new_mode = 6;
  bool binary = 7;
  bytes raw_patch_data = 9;
  bool end_of_patch = 10;
  bool overflow_marker = 11;
  bool collapsed = 12;
  bool too_large = 13;
  int32 lines_added = 14;
  int32 lines_removed = 15;
}
```

**Conman usage for conflict detection:**

```rust
// conman-git/src/conflict.rs

impl GitalyClient {
    /// Detect files that were modified on both sides since the merge base.
    /// Returns the list of file paths with overlapping changes.
    pub async fn detect_conflicting_files(
        &self,
        repo: &gitaly::Repository,
        integration_head: &str,
        changeset_head: &str,
    ) -> Result<Vec<String>, ConmanError> {
        // Step 1: Get files changed on the integration branch since merge base.
        // Use CommitDiff with left=merge_base, right=integration_head.
        // Step 2: Get files changed on changeset branch since merge base.
        // Use CommitDiff with left=merge_base, right=changeset_head.
        // Step 3: Intersect the two file sets.
        // Files appearing in both diffs are potential conflicts.
        //
        // For revalidation, we set collect_all_paths=true and collapse_diffs=true
        // because we only need the file paths, not the patch content.
    }
}
```

### 7.2 `CommitService.CommitIsAncestor` -- up-to-date check

Used to verify that the changeset branch includes all commits from integration branch HEAD,
which is the prerequisite for entering the queue.

**Service definition** (from `proto/commit.proto`):

```protobuf
service CommitService {
  // CommitIsAncestor checks whether a provided commit is the ancestor of
  // another commit.
  rpc CommitIsAncestor(CommitIsAncestorRequest) returns (CommitIsAncestorResponse);
}

message CommitIsAncestorRequest {
  // repository is the repository for which we need to check the ancestry.
  Repository repository = 1;
  // ancestor_id is the object ID of the commit which needs to be checked as ancestor.
  string ancestor_id = 2;
  // child_id is the object ID of the commit whose ancestor needs to be confirmed.
  string child_id = 3;
}

message CommitIsAncestorResponse {
  // value denotes whether the provided commit is the ancestor or not.
  bool value = 1;
}
```

**Conman usage for up-to-date gate:**

```rust
// conman-git/src/ancestry.rs

impl GitalyClient {
    /// Check if integration branch HEAD is an ancestor of the changeset HEAD. If true, the
    /// changeset branch is up to date with the integration branch and can be queued.
    pub async fn is_up_to_date_with_integration(
        &self,
        repo: &gitaly::Repository,
        integration_head_sha: &str,
        changeset_head_sha: &str,
    ) -> Result<bool, ConmanError> {
        let request = CommitIsAncestorRequest {
            repository: Some(repo.clone()),
            ancestor_id: integration_head_sha.to_string(),
            child_id: changeset_head_sha.to_string(),
        };

        let response = self
            .commit_service()
            .commit_is_ancestor(request)
            .await
            .map_err(|e| ConmanError::Git {
                message: format!("ancestry check failed: {}", e),
            })?;

        Ok(response.into_inner().value)
    }
}
```

### 7.3 `CommitService.FindCommit` -- resolve integration branch HEAD

Used to resolve the current integration branch HEAD SHA after a release publish, so that
revalidation jobs know what to compare against.

**Service definition** (from `proto/commit.proto`):

```protobuf
service CommitService {
  // FindCommit finds a commit for a given commitish. Returns nil if the
  // commit is not found.
  rpc FindCommit(FindCommitRequest) returns (FindCommitResponse);
}

message FindCommitRequest {
  // repository is the repository in which we want to find the commit.
  Repository repository = 1;
  // revision is a commitish which is to be resolved to a commit.
  bytes revision = 2;
  // trailers if set, parses and adds the trailing information of the commit.
  bool trailers = 3;
}

message FindCommitResponse {
  // commit is the requested commit, it is nil when the commit was not found.
  GitCommit commit = 1;
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
  CommitStatInfo short_stats = 11;
  repeated bytes referenced_by = 12;
  string encoding = 13;
}
```

**Conman usage for resolving integration branch HEAD:**

```rust
// conman-git/src/refs.rs

impl GitalyClient {
    /// Resolve the current HEAD commit of the integration branch. Returns the full
    /// GitCommit so callers can use the SHA and metadata.
    pub async fn resolve_integration_head(
        &self,
        repo: &gitaly::Repository,
    ) -> Result<String, ConmanError> {
        let request = FindCommitRequest {
            repository: Some(repo.clone()),
            revision: b"refs/heads/<integration_branch>".to_vec(),
            trailers: false,
        };

        let response = self
            .commit_service()
            .find_commit(request)
            .await
            .map_err(|e| ConmanError::Git {
                message: format!("failed to resolve integration branch HEAD: {}", e),
            })?;

        let commit = response.into_inner().commit.ok_or_else(|| {
            ConmanError::Git {
                message: "integration branch HEAD not found".to_string(),
            }
        })?;

        Ok(commit.id)
    }
}
```

**Shared proto type `Repository`** (from `proto/shared.proto`):

```protobuf
message Repository {
  string storage_name = 2;
  string relative_path = 3;
  string git_object_directory = 4;
  repeated string git_alternate_object_directories = 5;
  string gl_repository = 6;
  string gl_project_path = 8;
}
```

## 8. Implementation Checklist

### E07-01: Queue transition (Approved -> Queued)

- [ ] Add `queue_position: Option<i64>` and `queued_at: Option<DateTime<Utc>>`
      fields to `Changeset` struct in `conman-core`.
- [ ] Add `Queued`, `Conflicted`, `NeedsRevalidation` variants to
      `ChangesetState` enum.
- [ ] Implement `validate_transition` for all E07 state transitions.
- [ ] Implement `is_up_to_date_with_integration` in `conman-git` using
      `CommitIsAncestor`.
- [ ] Implement `resolve_integration_head` in `conman-git` using `FindCommit`.
- [ ] Add queue-position assignment logic in `conman-db` (atomic
      find-max-and-increment).
- [ ] Create handler `POST /api/repos/:appId/changesets/:changesetId/queue` in
      `conman-api`.
- [ ] Emit `changeset_queued` audit event.
- [ ] Add queue index to `ChangesetRepo::ensure_indexes()`.

### E07-02: Queue listing and reorder

- [ ] Create `QueueEntry` DTO in `conman-api`.
- [ ] Create `ReorderRequest` DTO in `conman-api`.
- [ ] Implement handler `GET /api/repos/:appId/queue` with pagination.
- [ ] Implement handler `POST /api/repos/:appId/queue/reorder` with RBAC
      (config_manager+).
- [ ] Validate reorder request: exact permutation of current queue.
- [ ] Atomic bulk position update via MongoDB `bulkWrite`.
- [ ] Emit `queue_reordered` audit event with before/after position maps.

### E07-03: Revalidation trigger

- [ ] Create `RevalidationResult` enum in `conman-core`.
- [ ] Add revalidation trigger hook to release publish flow (called by E08).
- [ ] Implement function to enqueue `revalidate_queued_changeset` jobs for all
      remaining queued changesets after a release.
- [ ] Jobs enqueued in queue-position order.

### E07-04: Revalidation job logic

- [ ] Implement `detect_conflicting_files` in `conman-git` using
      `DiffService.CommitDiff`.
- [ ] Implement `revalidate_queued_changeset` job worker in `conman-jobs`.
- [ ] Step 1: conflict detection via diff + trial merge.
- [ ] Step 2: ancestry check via `CommitIsAncestor`.
- [ ] Step 3: msuite revalidation (reuse `msuite_merge` worker).
- [ ] On conflict: transition to `Conflicted`, record file paths.
- [ ] On test failure: transition to `NeedsRevalidation`, record job ID.
- [ ] On pass: remain `Queued`, update `last_revalidation_status`.
- [ ] Emit appropriate notifications for each outcome.

### E07-05: Move-to-draft

- [ ] Implement handler `POST /api/repos/:appId/changesets/:changesetId/move-to-draft`.
- [ ] RBAC: author can move own; config_manager/admin can move any.
- [ ] Clear queue metadata (`queue_position`, `queued_at`).
- [ ] Clear revalidation metadata.
- [ ] Transition state to `Draft`.
- [ ] Emit `changeset_moved_to_draft` audit event.

## 9. Test Cases

### 9.1 State machine (unit, `conman-core`)

- **Approved -> Queued** succeeds.
- **Draft -> Queued** fails with `InvalidTransition`.
- **Submitted -> Queued** fails with `InvalidTransition`.
- **Queued -> Conflicted** succeeds.
- **Queued -> NeedsRevalidation** succeeds.
- **Queued -> Released** succeeds.
- **Conflicted -> Draft** succeeds.
- **NeedsRevalidation -> Draft** succeeds.
- **Conflicted -> Queued** fails with `InvalidTransition` (must go through
  Draft -> Submitted -> ... -> Approved -> Queued).
- **NeedsRevalidation -> Queued** fails with `InvalidTransition`.

### 9.2 Queue position assignment (unit, `conman-db`)

- First changeset queued in an empty app gets `queue_position = 1`.
- Second changeset gets `queue_position = 2` (monotonic).
- After removing a changeset from the queue, the next queued changeset still
  gets `max + 1` (gaps are fine).

### 9.3 Up-to-date gate (integration, `conman-git`)

- Changeset branch that includes integration branch HEAD passes the ancestry check.
- Changeset branch that is behind integration branch HEAD fails the ancestry check.
- Queue endpoint returns `409` when branch is not up to date.

### 9.4 Reorder (integration, `conman-api`)

- Valid permutation reorder succeeds and positions are updated atomically.
- Reorder with missing changeset ID returns `400`.
- Reorder with extra changeset ID returns `400`.
- Reorder with duplicate changeset ID returns `400`.
- Reorder by user with `user` role returns `403`.
- Reorder by user with `reviewer` role returns `403`.
- Reorder by user with `config_manager` role succeeds.
- Reorder emits audit event with correct before/after maps.

### 9.5 Revalidation (integration, `conman-jobs`)

- After release publish, revalidation jobs are enqueued for all remaining
  queued changesets.
- Changeset with no conflicts and passing msuite remains `Queued` with
  `last_revalidation_status = "valid"`.
- Changeset with file conflicts transitions to `Conflicted` with conflicting
  file paths recorded.
- Changeset with no conflicts but failing msuite transitions to
  `NeedsRevalidation` with job ID recorded.
- Revalidation processes changesets in queue-position order.

### 9.6 Move-to-draft (integration, `conman-api`)

- Author can move their own `Conflicted` changeset to `Draft`.
- Author can move their own `NeedsRevalidation` changeset to `Draft`.
- Author cannot move another user's conflicted changeset (returns `403`).
- config_manager can move any user's conflicted changeset to `Draft`.
- admin can move any user's needs_revalidation changeset to `Draft`.
- Move-to-draft clears `queue_position`, `queued_at`,
  `last_revalidation_status`, and `last_revalidation_job_id`.
- Attempting move-to-draft on a `Queued` changeset returns `409`.
- Attempting move-to-draft on a `Draft` changeset returns `409`.

### 9.7 End-to-end flow (integration)

- Full cycle: create changeset -> submit -> approve -> queue -> release publish
  -> revalidation passes for remaining queued changeset -> changeset stays
  queued.
- Full cycle with conflict: create two changesets modifying the same file ->
  approve both -> queue both -> release first -> revalidation detects conflict
  on second -> second transitions to Conflicted -> move to draft -> fix and
  re-submit.

## 10. Acceptance Criteria

1. **Queue entry:** Approved changesets can transition to Queued only when their
   branch is up to date with the integration branch. Queue position is assigned monotonically.

2. **Queue ordering:** The `GET /api/repos/:appId/queue` endpoint returns
   changesets sorted by `queue_position` in ascending order.

3. **Reorder:** Config managers and app admins can reorder the queue. The
   reorder operation is atomic and audited. Non-privileged users cannot reorder.

4. **Non-selected persistence:** Queued changesets not included in a release
   remain in the `Queued` state with their positions preserved.

5. **Revalidation trigger:** After every release publish, revalidation jobs are
   enqueued for all remaining queued changesets in the app.

6. **Conflict detection:** Revalidation correctly identifies changesets whose
   branches conflict with the updated integration branch and transitions them to `Conflicted`.

7. **msuite revalidation:** Changesets that merge cleanly but fail msuite tests
   transition to `NeedsRevalidation`.

8. **Clean revalidation:** Changesets that pass both conflict detection and
   msuite revalidation remain in `Queued` with updated status.

9. **Move-to-draft:** Authors can return their own conflicted or
   needs_revalidation changesets to draft. Config managers and app admins can
   return any such changeset to draft.

10. **Audit trail:** All queue mutations (queue entry, reorder, revalidation
    outcomes, move-to-draft) are captured in the audit log with actor, timestamp,
    and before/after state.

11. **Override-key conflict handling:** If two queued changesets override the
    same env var key for the same target runtime profile, the later changeset
    transitions to `Conflicted`. If both overrides resolve to the same typed
    value, no conflict is raised.
