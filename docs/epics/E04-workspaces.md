# E04 -- Workspace Lifecycle + File Operations

> **Depends on:** E01 (Git Adapter), E03 (App Setup)

## 1. Goal

Deliver editable workspaces with Git-backed file persistence and guardrails.
Users get a mutable branch per app where they can browse the file tree, read
and write files, and synchronize their workspace with the upstream baseline.
All file mutations flow through gitaly-rs and produce real Git commits. The
backend reserves multi-workspace APIs even though the v1 UI surfaces only one
default workspace per user per app.

**From the backlog:**

| ID | Item |
|----|------|
| E04-01 | Create default workspace branch (`ws/<user>/<app>`) on first use |
| E04-02 | Workspace CRUD (reserve multi-workspace APIs, UI can hide extras) |
| E04-03 | File tree/list/read/write/delete endpoints using `path` query/body |
| E04-04 | Guardrails for blocked paths and max file size |
| E04-05 | Workspace reset/sync-integration flow with rebase/merge fallback |
| E04-06 | Conflict detection primitives for later changeset/release flows |

---

## 2. Dependencies

| Dependency | What we need from it |
|------------|----------------------|
| **E01 -- Git Adapter** | `GitalyClient` with a connected tonic `Channel`. Helper `app_to_gitaly_repo()`. |
| **E03 -- App Setup** | `App` document with `repo_path`, `blocked_paths`, `file_size_limit_bytes`, `baseline_mode`, `canonical_env_id`, `commit_mode_default`. `AppRepo` for lookup. |
| **E02 -- Auth** (transitive via E03) | `AuthUser` extractor, `require_role()` guard. Every workspace endpoint requires at least `Role::Member`. |

---

## 3. Rust Types

### 3.1 Domain types (`conman-core`)

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// -- Workspace -----------------------------------------------------------

/// Represents the kind of Git ref the workspace was branched from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BaseRefType {
    /// Branched from a branch (e.g. app `integration_branch`).
    Branch,
    /// Branched from a release tag (e.g. "r2025.06.01.1").
    Tag,
    /// Branched from a specific commit SHA.
    Commit,
}

/// A user-owned mutable branch used to edit app files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    /// MongoDB ObjectId, serialized as hex string in API responses.
    pub id: ObjectId,
    /// The app this workspace belongs to.
    pub app_id: ObjectId,
    /// The user who owns this workspace.
    pub owner_user_id: ObjectId,
    /// Full Git branch name, e.g. "ws/alice/my-app".
    pub branch_name: String,
    /// Optional human-readable title. Reserved for multi-workspace UI.
    pub title: Option<String>,
    /// True for the auto-created default workspace per user per app.
    pub is_default: bool,
    /// What kind of ref this workspace was branched from.
    pub base_ref_type: BaseRefType,
    /// The ref value (branch name, tag name, or commit SHA).
    pub base_ref_value: String,
    /// Current HEAD commit SHA of the workspace branch.
    pub head_sha: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// -- File types ----------------------------------------------------------

/// The kind of entry in a file tree listing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileEntryType {
    File,
    Dir,
}

/// A single entry returned by the file tree listing endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Relative path from repo root, e.g. "config/entities/foo.json".
    pub path: String,
    /// Whether this entry is a file or directory.
    #[serde(rename = "type")]
    pub entry_type: FileEntryType,
    /// Size in bytes. 0 for directories.
    pub size: i64,
    /// Git object ID (blob OID for files, tree OID for directories).
    pub oid: String,
}

/// Full file content returned by the file read endpoint.
#[derive(Debug, Clone)]
pub struct FileContent {
    /// Relative path from repo root.
    pub path: String,
    /// Raw file bytes.
    pub content: Vec<u8>,
    /// Size in bytes.
    pub size: i64,
}

/// An action to perform when committing files via UserCommitFiles.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileAction {
    Create,
    Update,
    Delete,
}

/// Result of a sync-integration or rebase operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictStatus {
    /// True if the sync completed without conflicts.
    pub clean: bool,
    /// New HEAD SHA after successful sync, or the pre-sync HEAD on conflict.
    pub head_sha: String,
    /// List of conflicting file paths, empty when `clean` is true.
    pub conflicting_paths: Vec<String>,
    /// Human-readable summary, e.g. "Rebased 3 commits onto the integration branch".
    pub message: String,
}
```

### 3.2 API types (`conman-api`)

```rust
use serde::{Deserialize, Serialize};

// -- Requests ------------------------------------------------------------

/// POST /api/repos/:appId/workspaces
#[derive(Debug, Deserialize)]
pub struct CreateWorkspaceRequest {
    /// Optional title. Omit for default workspace.
    pub title: Option<String>,
    /// Optional branch name override. Server generates default if omitted.
    pub branch_name: Option<String>,
}

/// PATCH /api/repos/:appId/workspaces/:workspaceId
#[derive(Debug, Deserialize)]
pub struct UpdateWorkspaceRequest {
    pub title: Option<String>,
}

/// PUT /api/repos/:appId/workspaces/:workspaceId/files
#[derive(Debug, Deserialize)]
pub struct WriteFileRequest {
    /// Relative path from repo root.
    pub path: String,
    /// Base64-encoded file content.
    pub content: String,
    /// Optional commit message. Server generates a default if omitted.
    pub message: Option<String>,
}

/// DELETE /api/repos/:appId/workspaces/:workspaceId/files
#[derive(Debug, Deserialize)]
pub struct DeleteFileRequest {
    /// Relative path from repo root.
    pub path: String,
    /// Optional commit message.
    pub message: Option<String>,
}

/// POST /api/repos/:appId/workspaces/:workspaceId/checkpoints
#[derive(Debug, Deserialize)]
pub struct CreateCheckpointRequest {
    /// Commit message for the checkpoint.
    pub message: Option<String>,
}

/// Query params for GET .../files?path=
#[derive(Debug, Deserialize)]
pub struct FilePathQuery {
    /// Path to list or read. Empty or "/" means repo root.
    #[serde(default)]
    pub path: String,
}

// -- Responses -----------------------------------------------------------

/// Single workspace in API responses.
#[derive(Debug, Serialize)]
pub struct WorkspaceResponse {
    pub id: String,
    pub app_id: String,
    pub owner_user_id: String,
    pub branch_name: String,
    pub title: Option<String>,
    pub is_default: bool,
    pub base_ref_type: String,
    pub base_ref_value: String,
    pub head_sha: String,
    pub created_at: String,
    pub updated_at: String,
}

/// GET .../files?path= when path points to a directory.
#[derive(Debug, Serialize)]
pub struct FileTreeResponse {
    pub path: String,
    pub entries: Vec<FileEntry>,
}

/// GET .../files?path= when path points to a file.
#[derive(Debug, Serialize)]
pub struct FileContentResponse {
    pub path: String,
    /// Base64-encoded content.
    pub content: String,
    pub size: i64,
    pub oid: String,
}

/// Response for write/delete file operations.
#[derive(Debug, Serialize)]
pub struct FileWriteResponse {
    pub commit_sha: String,
    pub path: String,
}

/// POST .../sync-integration response.
#[derive(Debug, Serialize)]
pub struct SyncMainResponse {
    pub clean: bool,
    pub head_sha: String,
    pub conflicting_paths: Vec<String>,
    pub message: String,
}

/// POST .../reset response.
#[derive(Debug, Serialize)]
pub struct ResetResponse {
    pub head_sha: String,
    pub message: String,
}

/// POST .../checkpoints response.
#[derive(Debug, Serialize)]
pub struct CheckpointResponse {
    pub commit_sha: String,
    pub message: String,
}
```

---

## 4. Database

### 4.1 Collection: `workspaces`

```json
{
  "_id": ObjectId("..."),
  "app_id": ObjectId("..."),
  "owner_user_id": ObjectId("..."),
  "branch_name": "ws/alice/my-app",
  "title": null,
  "is_default": true,
  "base_ref_type": "branch",
  "base_ref_value": "integration_branch",
  "head_sha": "a1b2c3d4e5f6...",
  "created_at": ISODate("2026-02-25T10:00:00Z"),
  "updated_at": ISODate("2026-02-25T12:30:00Z")
}
```

### 4.2 Indexes

| Index | Fields | Options | Purpose |
|-------|--------|---------|---------|
| `idx_ws_app_branch` | `{ app_id: 1, branch_name: 1 }` | `unique: true` | Prevent duplicate branches per app |
| `idx_ws_app_owner_default` | `{ app_id: 1, owner_user_id: 1, is_default: 1 }` | `unique: true, partialFilterExpression: { is_default: true }` | At most one default workspace per user per app |
| `idx_ws_app_owner` | `{ app_id: 1, owner_user_id: 1 }` | -- | Efficient lookup for listing a user's workspaces in an app |
| `idx_ws_app` | `{ app_id: 1 }` | -- | Paginated listing of all workspaces in an app |

### 4.3 Repository: `WorkspaceRepo`

```rust
pub struct WorkspaceRepo {
    collection: Collection<Workspace>,
}

impl WorkspaceRepo {
    pub async fn ensure_indexes(&self) -> Result<(), ConmanError>;
    pub async fn insert(&self, workspace: &Workspace) -> Result<(), ConmanError>;
    pub async fn find_by_id(&self, id: ObjectId) -> Result<Option<Workspace>, ConmanError>;
    pub async fn find_default(&self, app_id: ObjectId, user_id: ObjectId) -> Result<Option<Workspace>, ConmanError>;
    pub async fn find_by_app_and_branch(&self, app_id: ObjectId, branch: &str) -> Result<Option<Workspace>, ConmanError>;
    pub async fn list_by_app(&self, app_id: ObjectId, page: u64, limit: u64) -> Result<(Vec<Workspace>, u64), ConmanError>;
    pub async fn update_head_sha(&self, id: ObjectId, head_sha: &str) -> Result<(), ConmanError>;
    pub async fn update_title(&self, id: ObjectId, title: Option<&str>) -> Result<(), ConmanError>;
    pub async fn delete(&self, id: ObjectId) -> Result<(), ConmanError>;
}
```

---

## 5. API Endpoints

All endpoints require `Authorization: Bearer <token>` and at minimum `Role::Member` on the target app.

### 5.1 Workspace CRUD

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| `GET` | `/api/repos/:appId/workspaces?page=&limit=` | `list_workspaces` | List workspaces for the app. Paginated. |
| `POST` | `/api/repos/:appId/workspaces` | `create_workspace` | Create a new workspace. Creates Git branch. Returns `201`. |
| `GET` | `/api/repos/:appId/workspaces/:workspaceId` | `get_workspace` | Get single workspace by ID. |
| `PATCH` | `/api/repos/:appId/workspaces/:workspaceId` | `update_workspace` | Update workspace title. |
| `POST` | `/api/repos/:appId/workspaces/:workspaceId/reset` | `reset_workspace` | Reset workspace branch to baseline. |
| `POST` | `/api/repos/:appId/workspaces/:workspaceId/sync-integration` | `sync_workspace` | Rebase/merge workspace onto current integration branch. |

### 5.2 File Operations

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| `GET` | `/api/repos/:appId/workspaces/:workspaceId/files?path=` | `get_files` | If `path` is a directory: list tree entries. If `path` is a file: return content. |
| `PUT` | `/api/repos/:appId/workspaces/:workspaceId/files` | `write_file` | Create or update a file. Body: `{ path, content, message? }`. |
| `DELETE` | `/api/repos/:appId/workspaces/:workspaceId/files` | `delete_file` | Delete a file. Body: `{ path, message? }`. |
| `POST` | `/api/repos/:appId/workspaces/:workspaceId/checkpoints` | `create_checkpoint` | Commit current working state (for `manual_checkpoint` mode). |

### 5.3 Ownership and authorization rules

- Any authenticated user can list and read workspaces in apps they belong to.
- Users can only write to their own workspaces (or `admin` can write to any).
- `reset` and `sync-integration` follow the same ownership rules.
- Default workspace is auto-created on first `POST /workspaces` if none exists for the user.

---

## 6. Business Logic

### 6.1 Default workspace creation

When a user calls `POST /api/repos/:appId/workspaces` without specifying a
`branch_name`, or when any file/changeset operation references a workspace
that does not yet exist:

1. Resolve the app's baseline ref via `resolve_baseline(app)`.
2. Derive the branch name: `ws/<user_email_prefix>/<app_name>`.
   - `user_email_prefix` = everything before `@` in the user's email, lowercased, non-alphanumeric replaced with `-`.
   - `app_name` = `app.name` lowercased, spaces replaced with `-`.
3. Call `OperationService.UserCreateBranch` to create the branch from the baseline commit.
4. Insert `Workspace` document with `is_default: true`.
5. Return `201 Created`.

Branch name sanitization must reject names containing `..`, starting with `-`, or containing whitespace.

### 6.2 File write flow

1. Validate `path` is not in `app.blocked_paths` (glob matching via `globset`).
2. Validate `content` decoded size does not exceed `app.file_size_limit_bytes` (default 5 MB).
3. Determine `FileAction` -- `Create` if blob does not exist at path in HEAD, `Update` otherwise.
4. Call `OperationService.UserCommitFiles` with a single action.
5. Update `workspace.head_sha` in MongoDB to the new commit SHA.
6. Emit audit event: `workspace_file_write`.
7. Return new commit SHA.

### 6.3 File tree listing

1. Call `CommitService.GetTreeEntries` with `revision = workspace.head_sha`, `path = requested_path`.
2. Map `TreeEntry` results to `FileEntry` domain structs.
3. Sort: directories first, then alphabetical.

### 6.4 File read

1. Call `CommitService.TreeEntry` (single entry) to get the blob OID and verify the path exists.
2. Call `BlobService.GetBlobs` with the blob's revision+path to stream the content.
3. Return base64-encoded content to the client.

### 6.5 File delete

1. Validate `path` is not in `app.blocked_paths`.
2. Call `OperationService.UserCommitFiles` with `ActionType::DELETE`.
3. Update `workspace.head_sha`.
4. Emit audit event: `workspace_file_delete`.

### 6.6 Workspace reset

1. Resolve the app's current baseline ref.
2. Call `OperationService.UserCommitFiles` or `RefService.UpdateReferences` to force the workspace branch back to the baseline commit.
3. Update `workspace.head_sha` and `base_ref_value` in MongoDB.
4. Emit audit event: `workspace_reset`.

### 6.7 Sync-integration (rebase onto the integration branch)

1. Find the workspace's current `head_sha` and the current `integration_branch` HEAD.
2. Check if workspace is already up to date via `CommitService.CommitIsAncestor` (if integration branch HEAD is ancestor of workspace HEAD, it is already up to date).
3. Call `OperationService.UserRebaseToRef` with:
   - `source_sha` = workspace `head_sha`
   - `first_parent_ref` = `refs/heads/<integration_branch>`
   - `target_ref` = workspace branch ref
4. If rebase succeeds: update `workspace.head_sha`, return `ConflictStatus { clean: true }`.
5. If rebase fails with conflict: run `DiffService.FindChangedPaths` between the integration branch and workspace to identify conflicting paths. Return `ConflictStatus { clean: false, conflicting_paths }`.
6. Emit audit event: `workspace_sync`.

### 6.8 Checkpoint (manual_checkpoint mode)

1. Check that `app.commit_mode_default == "manual_checkpoint"` (or user override).
2. Call `OperationService.UserCommitFiles` with an empty action list and the provided message to create a commit on the workspace branch. (Alternatively, if there are staged changes in the working state, they get committed here.)
3. Update `workspace.head_sha`.
4. Emit audit event: `workspace_checkpoint`.

### 6.9 Blocked path validation

```rust
/// Returns Err(ConmanError::Validation) if the path matches any blocked pattern.
pub fn validate_path_not_blocked(path: &str, blocked_paths: &[String]) -> Result<(), ConmanError> {
    let glob_set = globset::GlobSetBuilder::new();
    for pattern in blocked_paths {
        glob_set.add(globset::Glob::new(pattern)?);
    }
    let set = glob_set.build()?;
    if set.is_match(path) {
        return Err(ConmanError::Validation {
            message: format!("path '{}' is blocked by app guardrails", path),
        });
    }
    Ok(())
}
```

Default blocked paths: `.git/**`, `.gitignore`, `.github/**`.

### 6.10 File size validation

```rust
pub fn validate_file_size(content_len: usize, limit_bytes: u64) -> Result<(), ConmanError> {
    if content_len as u64 > limit_bytes {
        return Err(ConmanError::Validation {
            message: format!(
                "file size {} exceeds limit of {} bytes",
                content_len, limit_bytes
            ),
        });
    }
    Ok(())
}
```

Default limit: 5,242,880 bytes (5 MB).

---

## 7. Gitaly-rs Integration

All proto definitions below are from the gitaly-rs repository at
`/Volumes/External/code-external/gitaly/proto/`. Included verbatim so this
epic is self-contained.

### 7.1 Shared types (`shared.proto`)

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
  CommitStatInfo short_stats = 11;
  repeated bytes referenced_by = 12;
  string encoding = 13;
}

message CommitAuthor {
  bytes name = 1;
  bytes email = 2;
  google.protobuf.Timestamp date = 3;
  bytes timezone = 4;
}

message PaginationParameter {
  string page_token = 1;
  int32 limit = 2;
}

message PaginationCursor {
  string next_cursor = 1;
}

enum ObjectType {
  UNKNOWN = 0;
  COMMIT = 1;
  BLOB = 2;
  TREE = 3;
  TAG = 4;
}
```

### 7.2 `OperationService.UserCreateBranch` -- Create workspace branch

**Used in:** E04-01 (default workspace creation)

```protobuf
// operations.proto

rpc UserCreateBranch(UserCreateBranchRequest) returns (UserCreateBranchResponse);

message UserCreateBranchRequest {
  // Repository in which the branch should be created.
  Repository repository = 1;
  // Name of the branch to create (e.g. "ws/alice/my-app").
  bytes branch_name = 2;
  // User to execute the action as.
  User user = 3;
  // Git revision to start the branch at (e.g. "refs/heads/<integration_branch>" or a SHA).
  bytes start_point = 4;
}

message UserCreateBranchResponse {
  // The created branch, including target_commit.
  Branch branch = 1;
}

message UserCreateBranchError {
  oneof error {
    CustomHookError custom_hook = 1;
  }
}
```

**Conman usage:**

```rust
async fn create_workspace_branch(
    client: &GitalyClient,
    repo: &gitaly::Repository,
    user: &gitaly::User,
    branch_name: &str,
    start_point: &str,
) -> Result<String, ConmanError> {
    let resp = client
        .operation_service()
        .user_create_branch(UserCreateBranchRequest {
            repository: Some(repo.clone()),
            branch_name: branch_name.as_bytes().to_vec(),
            user: Some(user.clone()),
            start_point: start_point.as_bytes().to_vec(),
        })
        .await?;
    let branch = resp.into_inner().branch.ok_or(ConmanError::Git {
        message: "branch creation returned empty response".into(),
    })?;
    let commit = branch.target_commit.ok_or(ConmanError::Git {
        message: "branch has no target commit".into(),
    })?;
    Ok(commit.id)
}
```

### 7.3 `OperationService.UserCommitFiles` -- Write/delete files

**Used in:** E04-03 (file write/delete), E04-08 (checkpoint)

```protobuf
// operations.proto

rpc UserCommitFiles(stream UserCommitFilesRequest) returns (UserCommitFilesResponse);

message UserCommitFilesRequestHeader {
  Repository repository = 1;
  User user = 2;
  // Branch to commit to (e.g. "ws/alice/my-app").
  bytes branch_name = 3;
  bytes commit_message = 4;
  bytes commit_author_name = 5;
  bytes commit_author_email = 6;
  // Optional: parent branch. Takes priority over branch_name for parent lookup.
  bytes start_branch_name = 7;
  Repository start_repository = 8;
  bool force = 9;
  // Optional: explicit parent SHA. Takes priority over start_branch_name.
  string start_sha = 10;
  google.protobuf.Timestamp timestamp = 11;
  // Expected current OID of branch_name for optimistic concurrency.
  string expected_old_oid = 12;
  bool sign = 13;
}

message UserCommitFilesActionHeader {
  enum ActionType {
    CREATE = 0;
    CREATE_DIR = 1;
    UPDATE = 2;
    MOVE = 3;
    DELETE = 4;
    CHMOD = 5;
  }

  ActionType action = 1;
  bytes file_path = 2;
  bytes previous_path = 3;
  bool base64_content = 4;
  bool execute_filemode = 5;
  bool infer_content = 6;
}

message UserCommitFilesAction {
  oneof user_commit_files_action_payload {
    UserCommitFilesActionHeader header = 1;
    bytes content = 2;
  }
}

message UserCommitFilesRequest {
  oneof user_commit_files_request_payload {
    UserCommitFilesRequestHeader header = 1;
    UserCommitFilesAction action = 2;
  }
}

message UserCommitFilesResponse {
  OperationBranchUpdate branch_update = 1;
  string index_error = 2;
  string pre_receive_error = 3;
}

message OperationBranchUpdate {
  string commit_id = 1;
  bool repo_created = 2;
  bool branch_created = 3;
}

message UserCommitFilesError {
  oneof error {
    AccessCheckError access_check = 1;
    IndexError index_update = 2;
    CustomHookError custom_hook = 3;
  }
}
```

**Conman usage (write file):**

```rust
async fn commit_file(
    client: &GitalyClient,
    repo: &gitaly::Repository,
    user: &gitaly::User,
    branch_name: &str,
    file_path: &str,
    content: &[u8],
    action: FileAction,
    message: &str,
    expected_old_oid: &str,
) -> Result<String, ConmanError> {
    let action_type = match action {
        FileAction::Create => ActionType::Create as i32,
        FileAction::Update => ActionType::Update as i32,
        FileAction::Delete => ActionType::Delete as i32,
    };

    // Stream: header first, then action header, then content chunks
    let header_msg = UserCommitFilesRequest {
        user_commit_files_request_payload: Some(Header(UserCommitFilesRequestHeader {
            repository: Some(repo.clone()),
            user: Some(user.clone()),
            branch_name: branch_name.as_bytes().to_vec(),
            commit_message: message.as_bytes().to_vec(),
            expected_old_oid: expected_old_oid.into(),
            ..Default::default()
        })),
    };

    let action_header_msg = UserCommitFilesRequest {
        user_commit_files_request_payload: Some(Action(UserCommitFilesAction {
            user_commit_files_action_payload: Some(ActionHeader(UserCommitFilesActionHeader {
                action: action_type,
                file_path: file_path.as_bytes().to_vec(),
                base64_content: false,
                ..Default::default()
            })),
        })),
    };

    let content_msg = UserCommitFilesRequest {
        user_commit_files_request_payload: Some(Action(UserCommitFilesAction {
            user_commit_files_action_payload: Some(Content(content.to_vec())),
        })),
    };

    let stream = tokio_stream::iter(vec![header_msg, action_header_msg, content_msg]);
    let resp = client
        .operation_service()
        .user_commit_files(stream)
        .await?;

    let inner = resp.into_inner();
    if !inner.index_error.is_empty() {
        return Err(ConmanError::Git { message: inner.index_error });
    }
    let commit_id = inner
        .branch_update
        .ok_or(ConmanError::Git { message: "no branch update".into() })?
        .commit_id;
    Ok(commit_id)
}
```

### 7.4 `CommitService.GetTreeEntries` -- List file tree

**Used in:** E04-03 (file tree listing)

```protobuf
// commit.proto

rpc GetTreeEntries(GetTreeEntriesRequest) returns (stream GetTreeEntriesResponse);

message GetTreeEntriesRequest {
  enum SortBy {
    DEFAULT = 0;
    TREES_FIRST = 1;
    FILESYSTEM = 2;
  }

  Repository repository = 1;
  // The commitish to read the tree from (workspace HEAD SHA or branch name).
  bytes revision = 2;
  // Path relative to repo root. Empty or "." for root.
  bytes path = 3;
  // Set true to recursively list all entries under path.
  bool recursive = 4;
  SortBy sort = 5;
  PaginationParameter pagination_params = 6;
  bool skip_flat_paths = 7;
}

message GetTreeEntriesResponse {
  repeated TreeEntry entries = 1;
  PaginationCursor pagination_cursor = 2;
}

message TreeEntry {
  enum EntryType {
    BLOB = 0;
    TREE = 1;
    COMMIT = 3;
  }

  string oid = 1;
  bytes path = 3;
  EntryType type = 4;
  int32 mode = 5;
  string commit_oid = 6;
  bytes flat_path = 7;
}

message GetTreeEntriesError {
  oneof error {
    ResolveRevisionError resolve_tree = 1;
    PathError path = 2;
  }
}
```

**Conman usage:**

```rust
async fn list_tree_entries(
    client: &GitalyClient,
    repo: &gitaly::Repository,
    revision: &str,
    path: &str,
) -> Result<Vec<FileEntry>, ConmanError> {
    let mut stream = client
        .commit_service()
        .get_tree_entries(GetTreeEntriesRequest {
            repository: Some(repo.clone()),
            revision: revision.as_bytes().to_vec(),
            path: path.as_bytes().to_vec(),
            recursive: false,
            sort: SortBy::TreesFirst as i32,
            skip_flat_paths: true,
            ..Default::default()
        })
        .await?
        .into_inner();

    let mut entries = Vec::new();
    while let Some(resp) = stream.message().await? {
        for te in resp.entries {
            entries.push(FileEntry {
                path: String::from_utf8_lossy(&te.path).to_string(),
                entry_type: match te.r#type() {
                    TreeEntryType::Tree => FileEntryType::Dir,
                    _ => FileEntryType::File,
                },
                size: 0, // size not included in tree entries; use GetBlob if needed
                oid: te.oid,
            });
        }
    }
    Ok(entries)
}
```

### 7.5 `BlobService.GetBlobs` -- Read file content

**Used in:** E04-03 (file read)

```protobuf
// blob.proto

rpc GetBlobs(GetBlobsRequest) returns (stream GetBlobsResponse);

message GetBlobsRequest {
  message RevisionPath {
    // Revision that identifies the tree-ish (e.g. workspace HEAD SHA).
    string revision = 1;
    // Path relative to the tree-ish (e.g. "config/entities/foo.json").
    bytes path = 2;
  }

  Repository repository = 1;
  repeated RevisionPath revision_paths = 2;
  // Max bytes per blob. -1 for unlimited.
  int64 limit = 3;
}

message GetBlobsResponse {
  int64 size = 1;
  bytes data = 2;
  string oid = 3;
  bool is_submodule = 4;
  int32 mode = 5;
  string revision = 6;
  bytes path = 7;
  ObjectType type = 8;
}
```

**Conman usage:**

```rust
async fn read_file(
    client: &GitalyClient,
    repo: &gitaly::Repository,
    revision: &str,
    path: &str,
) -> Result<FileContent, ConmanError> {
    let mut stream = client
        .blob_service()
        .get_blobs(GetBlobsRequest {
            repository: Some(repo.clone()),
            revision_paths: vec![RevisionPath {
                revision: revision.to_string(),
                path: path.as_bytes().to_vec(),
            }],
            limit: -1, // full content
        })
        .await?
        .into_inner();

    let mut content = Vec::new();
    let mut size = 0i64;
    let mut found = false;

    while let Some(resp) = stream.message().await? {
        if !resp.oid.is_empty() {
            found = true;
            size = resp.size;
        }
        if !resp.data.is_empty() {
            content.extend_from_slice(&resp.data);
        }
    }

    if !found {
        return Err(ConmanError::NotFound {
            entity: "file",
            id: path.to_string(),
        });
    }

    Ok(FileContent {
        path: path.to_string(),
        content,
        size,
    })
}
```

### 7.6 `RefService.FindBranch` -- Check branch exists, get HEAD

**Used in:** E04-01 (branch existence check), E04-05 (reset, sync-integration)

```protobuf
// ref.proto

rpc FindBranch(FindBranchRequest) returns (FindBranchResponse);

message FindBranchRequest {
  Repository repository = 1;
  // Branch name without "refs/heads/" prefix.
  bytes name = 2;
}

message FindBranchResponse {
  // The found branch. Nil if not found.
  Branch branch = 1;
}
```

**Conman usage:**

```rust
async fn find_branch_head(
    client: &GitalyClient,
    repo: &gitaly::Repository,
    branch_name: &str,
) -> Result<Option<String>, ConmanError> {
    let resp = client
        .ref_service()
        .find_branch(FindBranchRequest {
            repository: Some(repo.clone()),
            name: branch_name.as_bytes().to_vec(),
        })
        .await?
        .into_inner();

    Ok(resp.branch.and_then(|b| b.target_commit.map(|c| c.id)))
}
```

### 7.7 `OperationService.UserRebaseToRef` -- Sync workspace with integration branch

**Used in:** E04-05 (sync-integration)

```protobuf
// operations.proto

rpc UserRebaseToRef(UserRebaseToRefRequest) returns (UserRebaseToRefResponse);

message UserRebaseToRefRequest {
  Repository repository = 1;
  User user = 2;
  // Object ID of the commit to be rebased (workspace HEAD).
  string source_sha = 3;
  // Fully qualified ref to overwrite with the rebased result
  // (e.g. "refs/heads/ws/alice/my-app").
  bytes target_ref = 5;
  // Ref on top of which source_sha is rebased
  // (e.g. "refs/heads/<integration_branch>").
  bytes first_parent_ref = 7;
  google.protobuf.Timestamp timestamp = 9;
  // Expected current OID of target_ref for race safety.
  string expected_old_oid = 10;
}

message UserRebaseToRefResponse {
  // Object ID of the HEAD of the rebased ref.
  string commit_id = 1;
}
```

**Conman usage:**

```rust
async fn rebase_workspace_onto_integration(
    client: &GitalyClient,
    repo: &gitaly::Repository,
    user: &gitaly::User,
    workspace: &Workspace,
) -> Result<ConflictStatus, ConmanError> {
    let target_ref = format!("refs/heads/{}", workspace.branch_name);

    match client
        .operation_service()
        .user_rebase_to_ref(UserRebaseToRefRequest {
            repository: Some(repo.clone()),
            user: Some(user.clone()),
            source_sha: workspace.head_sha.clone(),
            target_ref: target_ref.as_bytes().to_vec(),
            first_parent_ref: b"refs/heads/<integration_branch>".to_vec(),
            expected_old_oid: workspace.head_sha.clone(),
            ..Default::default()
        })
        .await
    {
        Ok(resp) => Ok(ConflictStatus {
            clean: true,
            head_sha: resp.into_inner().commit_id,
            conflicting_paths: vec![],
            message: "Successfully rebased onto the integration branch".into(),
        }),
        Err(status) if is_conflict_error(&status) => {
            let conflicts = detect_conflicting_paths(client, repo, workspace).await?;
            Ok(ConflictStatus {
                clean: false,
                head_sha: workspace.head_sha.clone(),
                conflicting_paths: conflicts,
                message: "Rebase failed due to conflicts".into(),
            })
        }
        Err(e) => Err(ConmanError::Git {
            message: format!("rebase failed: {}", e),
        }),
    }
}
```

### 7.8 `DiffService.CommitDiff` -- Detect conflicts

**Used in:** E04-06 (conflict detection primitives)

```protobuf
// diff.proto

rpc CommitDiff(CommitDiffRequest) returns (stream CommitDiffResponse);

message CommitDiffRequest {
  enum DiffMode {
    DEFAULT = 0;
    WORDDIFF = 1;
  }

  enum WhitespaceChanges {
    WHITESPACE_CHANGES_UNSPECIFIED = 0;
    WHITESPACE_CHANGES_IGNORE = 1;
    WHITESPACE_CHANGES_IGNORE_ALL = 2;
  }

  Repository repository = 1;
  // Left commit (e.g. merge base SHA).
  string left_commit_id = 2;
  // Right commit (e.g. workspace HEAD SHA).
  string right_commit_id = 3;
  repeated bytes paths = 5;
  bool collapse_diffs = 6;
  bool enforce_limits = 7;
  int32 max_files = 8;
  int32 max_lines = 9;
  int32 max_bytes = 10;
  int32 safe_max_files = 11;
  int32 safe_max_lines = 12;
  int32 safe_max_bytes = 13;
  int32 max_patch_bytes = 14;
  DiffMode diff_mode = 15;
  map<string, int32> max_patch_bytes_for_file_extension = 16;
  WhitespaceChanges whitespace_changes = 17;
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

Also used: `FindChangedPaths` for identifying which paths diverge between
integration branch and workspace:

```protobuf
rpc FindChangedPaths(FindChangedPathsRequest) returns (stream FindChangedPathsResponse);

message FindChangedPathsRequest {
  message Request {
    message TreeRequest {
      string left_tree_revision = 1;
      string right_tree_revision = 2;
    }
    message CommitRequest {
      string commit_revision = 1;
      repeated string parent_commit_revisions = 2;
    }
    oneof type {
      TreeRequest tree_request = 1;
      CommitRequest commit_request = 2;
    }
  }

  Repository repository = 1;
  repeated Request requests = 3;
  // ...
}

message FindChangedPathsResponse {
  repeated ChangedPaths paths = 1;
}

message ChangedPaths {
  enum Status {
    ADDED = 0;
    MODIFIED = 1;
    DELETED = 2;
    TYPE_CHANGE = 3;
    COPIED = 4;
    RENAMED = 5;
  }

  bytes path = 1;
  Status status = 2;
  int32 old_mode = 3;
  int32 new_mode = 4;
  string old_blob_id = 5;
  string new_blob_id = 6;
  bytes old_path = 7;
  int32 score = 8;
  string commit_id = 9;
}
```

### 7.9 `CommitService.FindCommit` -- Resolve HEAD

**Used in:** E04-01 (resolve baseline), E04-05 (get current HEAD)

```protobuf
// commit.proto

rpc FindCommit(FindCommitRequest) returns (FindCommitResponse);

message FindCommitRequest {
  Repository repository = 1;
  // Any commitish: SHA, branch name, tag, "refs/heads/<integration_branch>", etc.
  bytes revision = 2;
  bool trailers = 3;
}

message FindCommitResponse {
  // Nil if not found.
  GitCommit commit = 1;
}
```

**Conman usage:**

```rust
async fn resolve_commit(
    client: &GitalyClient,
    repo: &gitaly::Repository,
    revision: &str,
) -> Result<String, ConmanError> {
    let resp = client
        .commit_service()
        .find_commit(FindCommitRequest {
            repository: Some(repo.clone()),
            revision: revision.as_bytes().to_vec(),
            trailers: false,
        })
        .await?
        .into_inner();

    resp.commit
        .map(|c| c.id)
        .ok_or(ConmanError::NotFound {
            entity: "commit",
            id: revision.to_string(),
        })
}
```

### 7.10 `CommitService.CommitIsAncestor` -- Up-to-date check

**Used in:** E04-05 (sync-integration short-circuit)

```protobuf
// commit.proto

rpc CommitIsAncestor(CommitIsAncestorRequest) returns (CommitIsAncestorResponse);

message CommitIsAncestorRequest {
  Repository repository = 1;
  string ancestor_id = 2;
  string child_id = 3;
}

message CommitIsAncestorResponse {
  bool value = 1;
}
```

---

## 8. Implementation Checklist

Ordered for incremental implementation. Each step should result in a passing
test suite before moving to the next.

- [ ] **8.1** Add `Workspace`, `BaseRefType`, `FileEntry`, `FileEntryType`, `FileContent`, `FileAction`, `ConflictStatus` to `conman-core/src/types.rs`.
- [ ] **8.2** Add `validate_path_not_blocked()` and `validate_file_size()` to `conman-core/src/validation.rs`. Unit tests for blocked-path glob matching and size limits.
- [ ] **8.3** Add `WorkspaceRepo` to `conman-db` with all methods and `ensure_indexes()`.
- [ ] **8.4** Add workspace API request/response types to `conman-api/src/types/workspace.rs`.
- [ ] **8.5** Implement `create_workspace_branch()` in `conman-git` using `UserCreateBranch`. Integration test with mock gitaly.
- [ ] **8.6** Implement `commit_file()` and `delete_file_commit()` in `conman-git` using `UserCommitFiles`. Integration test.
- [ ] **8.7** Implement `list_tree_entries()` in `conman-git` using `GetTreeEntries`. Integration test.
- [ ] **8.8** Implement `read_file()` in `conman-git` using `GetBlobs`. Integration test.
- [ ] **8.9** Implement `find_branch_head()` in `conman-git` using `FindBranch`. Integration test.
- [ ] **8.10** Implement `resolve_commit()` in `conman-git` using `FindCommit`. Integration test.
- [ ] **8.11** Implement `rebase_workspace_onto_integration()` in `conman-git` using `UserRebaseToRef`. Integration test.
- [ ] **8.12** Implement `detect_conflicting_paths()` in `conman-git` using `FindChangedPaths` / `CommitDiff`. Integration test.
- [ ] **8.13** Wire up `POST /api/repos/:appId/workspaces` handler with default workspace creation logic.
- [ ] **8.14** Wire up `GET /api/repos/:appId/workspaces` and `GET .../workspaces/:workspaceId` handlers.
- [ ] **8.15** Wire up `PATCH /api/repos/:appId/workspaces/:workspaceId` handler.
- [ ] **8.16** Wire up `GET .../workspaces/:workspaceId/files?path=` handler (tree listing + file read).
- [ ] **8.17** Wire up `PUT .../workspaces/:workspaceId/files` handler with blocked-path and size guardrails.
- [ ] **8.18** Wire up `DELETE .../workspaces/:workspaceId/files` handler with blocked-path guardrail.
- [ ] **8.19** Wire up `POST .../workspaces/:workspaceId/reset` handler.
- [ ] **8.20** Wire up `POST .../workspaces/:workspaceId/sync-integration` handler.
- [ ] **8.21** Wire up `POST .../workspaces/:workspaceId/checkpoints` handler.
- [ ] **8.22** Add audit event emission to all mutation handlers.
- [ ] **8.23** End-to-end integration test: create workspace, write file, read file, list tree, sync-integration, reset.

---

## 9. Test Cases

### 9.1 Unit tests (`conman-core`)

| # | Test | Assertion |
|---|------|-----------|
| U1 | `validate_path_not_blocked(".git/config", DEFAULT_BLOCKED)` | Returns `Err(Validation)` |
| U2 | `validate_path_not_blocked(".github/workflows/ci.yml", DEFAULT_BLOCKED)` | Returns `Err(Validation)` |
| U3 | `validate_path_not_blocked(".gitignore", DEFAULT_BLOCKED)` | Returns `Err(Validation)` |
| U4 | `validate_path_not_blocked("config/app.json", DEFAULT_BLOCKED)` | Returns `Ok(())` |
| U5 | `validate_path_not_blocked("src/.gitkeep", DEFAULT_BLOCKED)` | Returns `Ok(())` (only exact `.gitignore` blocked, not nested) |
| U6 | `validate_file_size(5_242_881, 5_242_880)` | Returns `Err(Validation)` |
| U7 | `validate_file_size(5_242_880, 5_242_880)` | Returns `Ok(())` |
| U8 | `validate_file_size(0, 5_242_880)` | Returns `Ok(())` |
| U9 | Branch name derivation: `"Alice@example.com"` + `"My App"` -> `"ws/alice/my-app"` | Correct |
| U10 | Branch name derivation rejects `"../evil"` in email prefix | Returns `Err(Validation)` |

### 9.2 Database tests (`conman-db`)

| # | Test | Assertion |
|---|------|-----------|
| D1 | Insert workspace, find by ID | Found with matching fields |
| D2 | Insert two workspaces with same `app_id + branch_name` | Second insert fails (unique index) |
| D3 | Insert two default workspaces for same user + app | Second insert fails (partial unique index) |
| D4 | Insert non-default workspace for same user + app | Succeeds |
| D5 | `list_by_app` with pagination | Returns correct page and total count |
| D6 | `update_head_sha` | Subsequent `find_by_id` returns new SHA |
| D7 | `find_default(app_id, user_id)` | Returns the `is_default: true` workspace |

### 9.3 Git integration tests (`conman-git`)

| # | Test | Assertion |
|---|------|-----------|
| G1 | `create_workspace_branch` from integration branch HEAD | Returns valid commit SHA, branch exists |
| G2 | `create_workspace_branch` with non-existent start point | Returns `ConmanError::Git` |
| G3 | `commit_file` CREATE action | New file appears in tree |
| G4 | `commit_file` UPDATE action | File content updated |
| G5 | `commit_file` DELETE action | File removed from tree |
| G6 | `list_tree_entries` on root | Returns expected files/dirs |
| G7 | `list_tree_entries` on subdirectory | Returns entries of that directory only |
| G8 | `read_file` for existing path | Returns correct bytes |
| G9 | `read_file` for non-existent path | Returns `ConmanError::NotFound` |
| G10 | `rebase_workspace_onto_integration` with no divergence | Returns `clean: true`, same SHA |
| G11 | `rebase_workspace_onto_integration` with clean rebase | Returns `clean: true`, new SHA |
| G12 | `rebase_workspace_onto_integration` with conflict | Returns `clean: false`, conflicting paths non-empty |

### 9.4 API integration tests (`conman-api`)

| # | Test | Assertion |
|---|------|-----------|
| A1 | `POST /workspaces` without body | Creates default workspace, returns `201` with branch `ws/<user>/<app>` |
| A2 | `POST /workspaces` twice for same user | Second call returns `409 Conflict` (default already exists) |
| A3 | `POST /workspaces` with custom `branch_name` | Creates non-default workspace, returns `201` |
| A4 | `GET /workspaces` | Returns paginated list |
| A5 | `GET /workspaces/:id` | Returns single workspace |
| A6 | `PATCH /workspaces/:id` with `{ "title": "My WS" }` | Title updated |
| A7 | `GET /workspaces/:id/files?path=` (root) | Returns directory listing |
| A8 | `GET /workspaces/:id/files?path=config/app.json` | Returns file content (base64) |
| A9 | `PUT /workspaces/:id/files` with valid path | Returns `200` with new commit SHA |
| A10 | `PUT /workspaces/:id/files` with blocked path `.git/config` | Returns `400 Validation` |
| A11 | `PUT /workspaces/:id/files` with oversized content | Returns `400 Validation` |
| A12 | `DELETE /workspaces/:id/files` with valid path | Returns `200` with new commit SHA |
| A13 | `DELETE /workspaces/:id/files` with blocked path | Returns `400 Validation` |
| A14 | `POST /workspaces/:id/reset` | HEAD matches baseline, returns `200` |
| A15 | `POST /workspaces/:id/sync-integration` (clean) | Returns `200` with `clean: true` |
| A16 | `POST /workspaces/:id/sync-integration` (conflict) | Returns `200` with `clean: false` and conflicting paths |
| A17 | `POST /workspaces/:id/checkpoints` | Returns `200` with commit SHA |
| A18 | Write to another user's workspace as `Role::Member` | Returns `403 Forbidden` |
| A19 | Write to another user's workspace as `Role::Admin` | Returns `200` (admin override) |
| A20 | Unauthenticated request to any endpoint | Returns `401` |

---

## 10. Acceptance Criteria

1. **Default workspace auto-creation**: A user's first interaction with `POST /workspaces` for an app creates a branch `ws/<email_prefix>/<app_name>` from the app baseline. The MongoDB document records `is_default: true` and the correct `head_sha`.

2. **Full file editing except blocked paths**: Users can create, update, and delete files at any path not matched by the app's `blocked_paths` globs. Attempts to write to blocked paths return `400`.

3. **File size guardrail**: Files exceeding `app.file_size_limit_bytes` (default 5 MB) are rejected with `400`.

4. **File tree browsing**: `GET .../files?path=` returns directory listings (with type, OID) when path is a directory, and base64 content when path is a file.

5. **Workspace reset**: `POST .../reset` points the workspace branch back to the app baseline commit. The `head_sha` in MongoDB matches the baseline.

6. **Deterministic sync-integration**: `POST .../sync-integration` rebases the workspace onto current `integration_branch`. On success, returns `clean: true` and updated `head_sha`. On conflict, returns `clean: false` with the list of conflicting file paths. The workspace branch is not corrupted on conflict.

7. **Multi-workspace API readiness**: `POST /workspaces` accepts an optional `branch_name` to create additional workspaces. `GET /workspaces` paginates across all workspaces in an app.

8. **Checkpoint support**: `POST .../checkpoints` creates a commit on the workspace branch for apps using `manual_checkpoint` mode.

9. **Audit trail**: Every mutation (create workspace, write file, delete file, reset, sync, checkpoint) emits an audit event with actor, entity, before/after state, and git SHA.

10. **Ownership enforcement**: Non-admin users cannot mutate another user's workspace. `admin` can operate on any workspace in their app.
