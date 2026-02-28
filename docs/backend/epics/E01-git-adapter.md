# E01: Git Adapter Service (gitaly-rs boundary)

## 1. Goal

Isolate all Git operations behind a Conman adapter interface (`GitAdapter` trait)
implemented as a Tonic gRPC client to gitaly-rs, living entirely in the
`conman-git` crate.

## 2. Dependencies

| Dependency | What it provides |
|------------|-----------------|
| **E00 Platform Foundation** | `ConmanError`, Tokio runtime, config loading (`CONMAN_GITALY_ADDRESS`), tracing setup |

## 3. Gitaly Proto Reference

All proto definitions below are copied verbatim from the gitaly-rs proto files.
An implementer should never need to look at the gitaly repo.

### 3.1 shared.proto -- Core types used across all services

```protobuf
syntax = "proto3";
package gitaly;

import "google/protobuf/timestamp.proto";

// ObjectType ...
enum ObjectType {
  UNKNOWN = 0;
  COMMIT = 1;
  BLOB = 2;
  TREE = 3;
  TAG = 4;
}

// ObjectFormat is the object format that a Git repository can use.
enum ObjectFormat {
  OBJECT_FORMAT_UNSPECIFIED = 0;
  OBJECT_FORMAT_SHA1 = 1;
  OBJECT_FORMAT_SHA256 = 2;
}

// SignatureType ...
enum SignatureType {
  NONE = 0;
  PGP = 1;
  X509 = 2;
  SSH = 3;
}

// Repository ...
message Repository {
  reserved 1;
  reserved "path";
  // storage_name ...
  string storage_name = 2;
  // relative_path ...
  string relative_path = 3;
  // git_object_directory sets the GIT_OBJECT_DIRECTORY envvar on git commands.
  string git_object_directory = 4;
  // git_alternate_object_directories sets the GIT_ALTERNATE_OBJECT_DIRECTORIES envvar.
  repeated string git_alternate_object_directories = 5;
  // gl_repository is used in callbacks to GitLab so that it knows what repository the event is
  // associated with. May be left empty on RPCs that do not perform callbacks.
  string gl_repository = 6;
  reserved 7;
  // gl_project_path is the human-readable GitLab project path (e.g. gitlab-org/gitlab-ce).
  string gl_project_path = 8;
}

// CommitTrailer is a single Git trailer key-value pair.
message CommitTrailer {
  bytes key = 1;
  bytes value = 2;
}

// CommitStatInfo includes the number of changed lines and files in the commit.
message CommitStatInfo {
  int32 additions = 1;
  int32 deletions = 2;
  int32 changed_files = 3;
}

// GitCommit corresponds to Gitlab::Git::Commit
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

// CommitAuthor ...
message CommitAuthor {
  bytes name = 1;
  bytes email = 2;
  google.protobuf.Timestamp date = 3;
  bytes timezone = 4;
}

// ExitStatus ...
message ExitStatus {
  int32 value = 1;
}

// Branch corresponds to Gitlab::Git::Branch
message Branch {
  bytes name = 1;
  GitCommit target_commit = 2;
}

// Tag ...
message Tag {
  bytes name = 1;
  string id = 2;
  GitCommit target_commit = 3;
  bytes message = 4;
  int64 message_size = 5;
  CommitAuthor tagger = 6;
  SignatureType signature_type = 7;
}

// User ...
message User {
  string gl_id = 1;
  bytes name = 2;
  bytes email = 3;
  string gl_username = 4;
  string timezone = 5;
}

// PaginationParameter controls pagination within RPCs.
message PaginationParameter {
  string page_token = 1;
  int32 limit = 2;
}

// PaginationCursor defines the page token clients should use to fetch the next page.
message PaginationCursor {
  string next_cursor = 1;
}

// GlobalOptions are additional git options.
message GlobalOptions {
  bool literal_pathspecs = 1;
}

// SortDirection defines the sort direction.
enum SortDirection {
  ASCENDING = 0;
  DESCENDING = 1;
}
```

### 3.2 errors.proto -- Structured error types

```protobuf
syntax = "proto3";
package gitaly;

import "google/protobuf/duration.proto";

// AccessCheckError is an error returned by GitLab's `/internal/allowed` endpoint.
message AccessCheckError {
  string error_message = 1;
  string protocol = 2;
  string user_id = 3;
  bytes changes = 4;
}

// IndexError is an error returned when an operation fails due to a conflict with
// the repository index.
message IndexError {
  enum ErrorType {
    ERROR_TYPE_UNSPECIFIED = 0;
    ERROR_TYPE_EMPTY_PATH = 1;
    ERROR_TYPE_INVALID_PATH = 2;
    ERROR_TYPE_DIRECTORY_EXISTS = 3;
    ERROR_TYPE_DIRECTORY_TRAVERSAL = 4;
    ERROR_TYPE_FILE_EXISTS = 5;
    ERROR_TYPE_FILE_NOT_FOUND = 6;
  }
  bytes path = 1;
  ErrorType error_type = 2;
}

// InvalidRefFormatError is an error returned when refs have an invalid format.
message InvalidRefFormatError {
  repeated bytes refs = 2;
}

// NotAncestorError is an error returned when parent_revision is not an ancestor
// of the child_revision.
message NotAncestorError {
  bytes parent_revision = 1;
  bytes child_revision = 2;
}

// ChangesAlreadyAppliedError is an error returned when the operation would
// have resulted in no changes because these changes have already been applied.
message ChangesAlreadyAppliedError {
}

// MergeConflictError is an error returned when merging two commits fails due to
// a merge conflict.
message MergeConflictError {
  repeated bytes conflicting_files = 1;
  repeated string conflicting_commit_ids = 2;
}

// ReferencesLockedError is an error returned when ref update fails because
// the references have already been locked by another process.
message ReferencesLockedError {
  repeated bytes refs = 1;
}

// ReferenceExistsError is an error returned when a reference that ought not to
// exist does exist already.
message ReferenceExistsError {
  bytes reference_name = 1;
  string oid = 2;
}

// ReferenceNotFoundError is an error returned when a reference that ought to
// exist does not exist.
message ReferenceNotFoundError {
  bytes reference_name = 1;
}

// ReferenceStateMismatchError is an error returned when updating a reference
// fails because it points to a different object ID than expected.
message ReferenceStateMismatchError {
  bytes reference_name = 1;
  bytes expected_object_id = 2;
  bytes actual_object_id = 3;
}

// ReferenceUpdateError is an error returned when updating a reference has failed.
message ReferenceUpdateError {
  bytes reference_name = 1;
  string old_oid = 2;
  string new_oid = 3;
}

// ResolveRevisionError is an error returned when resolving a specific revision
// has failed.
message ResolveRevisionError {
  bytes revision = 1;
}

// LimitError is an error returned when Gitaly enforces request limits.
message LimitError {
  string error_message = 1;
  google.protobuf.Duration retry_after = 2;
}

// CustomHookError is an error returned when Gitaly executes a custom hook and
// the hook returns a non-zero return code.
message CustomHookError {
  enum HookType {
    HOOK_TYPE_UNSPECIFIED = 0;
    HOOK_TYPE_PRERECEIVE = 1;
    HOOK_TYPE_UPDATE = 2;
    HOOK_TYPE_POSTRECEIVE = 3;
  }
  bytes stdout = 1;
  bytes stderr = 2;
  HookType hook_type = 3;
}

// PathError is an error returned when there is an issue with the path provided.
message PathError {
  enum ErrorType {
    ERROR_TYPE_UNSPECIFIED = 0;
    ERROR_TYPE_EMPTY_PATH = 1;
    ERROR_TYPE_RELATIVE_PATH_ESCAPES_REPOSITORY = 2;
    ERROR_TYPE_ABSOLUTE_PATH = 3;
    ERROR_TYPE_LONG_PATH = 4;
    ERROR_TYPE_INVALID_PATH = 5;
    ERROR_TYPE_PATH_EXISTS = 6;
  }
  bytes path = 1;
  ErrorType error_type = 2;
}

// PathNotFoundError is an error returned when a given path cannot be found.
message PathNotFoundError {
  bytes path = 1;
}

// AmbiguousReferenceError is an error returned when a reference is unknown.
message AmbiguousReferenceError {
  bytes reference = 1;
}

// BadObjectError is an error returned when git cannot find a valid object based
// on its id.
message BadObjectError {
  bytes bad_oid = 1;
}

// InvalidRevisionRange is an error returned when the range given to the git log
// command is invalid.
message InvalidRevisionRange {
  bytes range = 1;
}

// RemoteNotFoundError is an error returned when a repository is not found at
// given remote URL.
message RemoteNotFoundError {
}
```

### 3.3 repository.proto -- RepositoryService (create, exists, remove)

```protobuf
syntax = "proto3";
package gitaly;

// RepositoryService is a service providing RPCs accessing repositories as a whole.
service RepositoryService {
  // RepositoryExists returns whether a given repository exists.
  rpc RepositoryExists(RepositoryExistsRequest) returns (RepositoryExistsResponse) {
    option (op_type) = { op: ACCESSOR };
  }

  // CreateRepository creates a new empty repository.
  rpc CreateRepository(CreateRepositoryRequest) returns (CreateRepositoryResponse) {
    option (op_type) = { op: MUTATOR };
  }

  // RemoveRepository will move the repository to a temp path and eventually remove it.
  rpc RemoveRepository(RemoveRepositoryRequest) returns (RemoveRepositoryResponse) {
    option (op_type) = { op: MUTATOR };
  }
}

// RepositoryExistsRequest is a request for the RepositoryExists RPC.
message RepositoryExistsRequest {
  // repository is the repo to check.
  Repository repository = 1 [(target_repository)=true];
}

// RepositoryExistsResponse is a response for the RepositoryExists RPC.
message RepositoryExistsResponse {
  bool exists = 1;
}

// CreateRepositoryRequest is a request for the CreateRepository RPC.
message CreateRepositoryRequest {
  // repository represents the repo to create.
  Repository repository = 1 [(target_repository)=true];
  // default_branch is the branch name to set as the default branch.
  bytes default_branch = 2;
  // object_format is the object format the repository should be created with.
  ObjectFormat object_format = 3;
}

// CreateRepositoryResponse is a response for the CreateRepository RPC.
message CreateRepositoryResponse {
}

// RemoveRepositoryRequest is a request for the RemoveRepository RPC.
message RemoveRepositoryRequest {
  Repository repository = 1 [(target_repository)=true];
}

// RemoveRepositoryResponse is a response for the RemoveRepository RPC.
message RemoveRepositoryResponse {
}
```

### 3.4 ref.proto -- RefService (branches, tags, references)

```protobuf
syntax = "proto3";
package gitaly;

// RefService is a service that provides RPCs to list and modify Git references.
service RefService {
  // FindDefaultBranchName looks up the default branch reference name.
  rpc FindDefaultBranchName(FindDefaultBranchNameRequest)
      returns (FindDefaultBranchNameResponse) {
    option (op_type) = { op: ACCESSOR };
  }

  // FindLocalBranches finds all local branches under `refs/heads/`.
  rpc FindLocalBranches(FindLocalBranchesRequest)
      returns (stream FindLocalBranchesResponse) {
    option (op_type) = { op: ACCESSOR };
  }

  // FindAllBranches finds all branches under `refs/heads/` and `refs/remotes/`.
  rpc FindAllBranches(FindAllBranchesRequest)
      returns (stream FindAllBranchesResponse) {
    option (op_type) = { op: ACCESSOR };
  }

  // FindAllTags finds all tags under `refs/tags/`.
  rpc FindAllTags(FindAllTagsRequest)
      returns (stream FindAllTagsResponse) {
    option (op_type) = { op: ACCESSOR };
  }

  // FindTag looks up a tag by its name and returns it to the caller if it exists.
  rpc FindTag(FindTagRequest) returns (FindTagResponse) {
    option (op_type) = { op: ACCESSOR };
  }

  // FindBranch finds a branch by its unqualified name (like "master") and
  // returns the commit it currently points to.
  rpc FindBranch(FindBranchRequest) returns (FindBranchResponse) {
    option (op_type) = { op: ACCESSOR };
  }

  // UpdateReferences atomically updates a set of references to a new state.
  rpc UpdateReferences(stream UpdateReferencesRequest)
      returns (UpdateReferencesResponse) {
    option (op_type) = { op: MUTATOR };
  }

  // DeleteRefs deletes the specified references from its repository.
  rpc DeleteRefs(DeleteRefsRequest) returns (DeleteRefsResponse) {
    option (op_type) = { op: MUTATOR };
  }

  // ListRefs returns a stream of all references in the repository.
  rpc ListRefs(ListRefsRequest) returns (stream ListRefsResponse) {
    option (op_type) = { op: ACCESSOR };
  }
}

// --- FindDefaultBranchName ---

message FindDefaultBranchNameRequest {
  Repository repository = 1 [(target_repository)=true];
  // head_only when true will determine the default branch using HEAD only.
  bool head_only = 2;
}

message FindDefaultBranchNameResponse {
  // name is the fully qualified default branch name.
  bytes name = 1;
}

// --- FindLocalBranches ---

message FindLocalBranchesRequest {
  enum SortBy {
    NAME = 0;
    UPDATED_ASC = 1;
    UPDATED_DESC = 2;
  }
  Repository repository = 1 [(target_repository)=true];
  SortBy sort_by = 2;
  PaginationParameter pagination_params = 3;
}

message FindLocalBranchesResponse {
  reserved "branches";
  reserved 1;
  repeated Branch local_branches = 2;
}

// --- FindAllBranches ---

message FindAllBranchesRequest {
  Repository repository = 1 [(target_repository)=true];
  bool merged_only = 2;
  repeated bytes merged_branches = 3;
}

message FindAllBranchesResponse {
  message Branch {
    bytes name = 1;
    GitCommit target = 2;
  }
  repeated Branch branches = 1;
}

// --- FindAllTags ---

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
  Repository repository = 1 [(target_repository)=true];
  SortBy sort_by = 2;
  PaginationParameter pagination_params = 3;
}

message FindAllTagsResponse {
  repeated Tag tags = 1;
}

// --- FindTag ---

message FindTagRequest {
  Repository repository = 1 [(target_repository)=true];
  // tag_name is the name of the tag (without refs/tags/ prefix).
  bytes tag_name = 2;
}

message FindTagResponse {
  Tag tag = 1;
}

message FindTagError {
  oneof error {
    ReferenceNotFoundError tag_not_found = 1;
  }
}

// --- FindBranch ---

message FindBranchRequest {
  Repository repository = 1 [(target_repository)=true];
  // name is the branch name without the "refs/heads/" prefix.
  bytes name = 2;
}

message FindBranchResponse {
  Branch branch = 1;
}

// --- UpdateReferences ---

message UpdateReferencesRequest {
  message Update {
    // reference is the fully-qualified reference name.
    bytes reference = 1;
    // old_object_id is the expected current object ID (optimistic lock).
    // Empty = force-update. All-zeroes = verify doesn't exist.
    bytes old_object_id = 2;
    // new_object_id is the new target. All-zeroes = delete the ref.
    bytes new_object_id = 3;
  }
  Repository repository = 1 [(target_repository)=true];
  repeated Update updates = 2;
}

message UpdateReferencesResponse {
}

message UpdateReferencesError {
  oneof error {
    InvalidRefFormatError invalid_format = 1;
    ReferencesLockedError references_locked = 2;
    ReferenceStateMismatchError reference_state_mismatch = 3;
  }
}

// --- DeleteRefs ---

message DeleteRefsRequest {
  Repository repository = 1 [(target_repository)=true];
  repeated bytes except_with_prefix = 2;
  repeated bytes refs = 3;
}

message DeleteRefsResponse {
  string git_error = 1;
}

message DeleteRefsError {
  oneof error {
    InvalidRefFormatError invalid_format = 1;
    ReferencesLockedError references_locked = 2;
  }
}

// --- ListRefs ---

message ListRefsRequest {
  message SortBy {
    enum Key {
      REFNAME = 0;
      CREATORDATE = 1;
      AUTHORDATE = 2;
      COMMITTERDATE = 3;
    }
    Key key = 1;
    SortDirection direction = 2;
  }
  Repository repository = 1 [(target_repository)=true];
  repeated bytes patterns = 2;
  bool head = 3;
  SortBy sort_by = 4;
  repeated bytes pointing_at_oids = 5;
  bool peel_tags = 6;
  PaginationParameter pagination_params = 7;
}

message ListRefsResponse {
  message Reference {
    bytes name = 1;
    string target = 2;
    string peeled_target = 3;
  }
  repeated Reference references = 1;
  PaginationCursor pagination_cursor = 2;
}
```

### 3.5 operations.proto -- OperationService (mutations)

```protobuf
syntax = "proto3";
package gitaly;

// OperationService provides an interface for performing mutating git
// operations on a repository on behalf of a user.
service OperationService {
  rpc UserCreateBranch(UserCreateBranchRequest) returns (UserCreateBranchResponse) {
    option (op_type) = { op: MUTATOR };
  }
  rpc UserUpdateBranch(UserUpdateBranchRequest) returns (UserUpdateBranchResponse) {
    option (op_type) = { op: MUTATOR };
  }
  rpc UserDeleteBranch(UserDeleteBranchRequest) returns (UserDeleteBranchResponse) {
    option (op_type) = { op: MUTATOR };
  }
  rpc UserCreateTag(UserCreateTagRequest) returns (UserCreateTagResponse) {
    option (op_type) = { op: MUTATOR };
  }
  rpc UserDeleteTag(UserDeleteTagRequest) returns (UserDeleteTagResponse) {
    option (op_type) = { op: MUTATOR };
  }
  rpc UserMergeToRef(UserMergeToRefRequest) returns (UserMergeToRefResponse) {
    option (op_type) = { op: MUTATOR };
  }
  rpc UserRebaseToRef(UserRebaseToRefRequest) returns (UserRebaseToRefResponse) {
    option (op_type) = { op: MUTATOR };
  }
  rpc UserMergeBranch(stream UserMergeBranchRequest)
      returns (stream UserMergeBranchResponse) {
    option (op_type) = { op: MUTATOR };
  }
  rpc UserCherryPick(UserCherryPickRequest) returns (UserCherryPickResponse) {
    option (op_type) = { op: MUTATOR };
  }
  rpc UserCommitFiles(stream UserCommitFilesRequest)
      returns (UserCommitFilesResponse) {
    option (op_type) = { op: MUTATOR };
  }
  rpc UserRevert(UserRevertRequest) returns (UserRevertResponse) {
    option (op_type) = { op: MUTATOR };
  }
}

// --- UserCreateBranch ---

message UserCreateBranchRequest {
  Repository repository = 1 [(target_repository)=true];
  bytes branch_name = 2;
  User user = 3;
  bytes start_point = 4;
}

message UserCreateBranchResponse {
  Branch branch = 1;
  reserved "pre_receive_error";
  reserved 2;
}

message UserCreateBranchError {
  oneof error {
    CustomHookError custom_hook = 1;
  }
}

// --- UserUpdateBranch ---

message UserUpdateBranchRequest {
  Repository repository = 1 [(target_repository)=true];
  bytes branch_name = 2;
  User user = 3;
  bytes newrev = 4;
  bytes oldrev = 5;
}

message UserUpdateBranchResponse {
  string pre_receive_error = 1;
}

// --- UserDeleteBranch ---

message UserDeleteBranchRequest {
  Repository repository = 1 [(target_repository)=true];
  bytes branch_name = 2;
  User user = 3;
  string expected_old_oid = 4;
}

message UserDeleteBranchResponse {
  reserved "pre_receive_error";
  reserved 1;
}

message UserDeleteBranchError {
  oneof error {
    AccessCheckError access_check = 1;
    ReferenceUpdateError reference_update = 2;
    CustomHookError custom_hook = 3;
  }
}

// --- UserCreateTag ---

message UserCreateTagRequest {
  Repository repository = 1 [(target_repository)=true];
  bytes tag_name = 2;
  User user = 3;
  // target_revision is peeled to the commit it points to.
  bytes target_revision = 4;
  // message -- if empty, a lightweight tag is created; otherwise annotated.
  bytes message = 5;
  google.protobuf.Timestamp timestamp = 7;
}

message UserCreateTagResponse {
  Tag tag = 1;
  reserved "exists";
  reserved 2;
  reserved "pre_receive_error";
  reserved 3;
}

message UserCreateTagError {
  oneof error {
    AccessCheckError access_check = 1;
    ReferenceUpdateError reference_update = 2;
    CustomHookError custom_hook = 3;
    ReferenceExistsError reference_exists = 4;
  }
}

// --- UserDeleteTag ---

message UserDeleteTagRequest {
  Repository repository = 1 [(target_repository)=true];
  bytes tag_name = 2;
  User user = 3;
  string expected_old_oid = 4;
}

message UserDeleteTagResponse {
  string pre_receive_error = 1;
}

// --- UserMergeBranch (bidirectional streaming) ---

message UserMergeBranchRequest {
  Repository repository = 1 [(target_repository)=true];
  User user = 2;
  string commit_id = 3;
  bytes branch = 4;
  bytes message = 5;
  bool apply = 6;
  google.protobuf.Timestamp timestamp = 7;
  string expected_old_oid = 8;
  bool squash = 9;
  bool sign = 10;
}

message UserMergeBranchResponse {
  // commit_id is the merge commit (first response).
  string commit_id = 1;
  reserved 2;
  // branch_update is sent as the second response when apply=true.
  OperationBranchUpdate branch_update = 3;
  reserved "pre_receive_error";
  reserved 4;
}

message UserMergeBranchError {
  oneof error {
    AccessCheckError access_check = 1;
    ReferenceUpdateError reference_update = 2;
    CustomHookError custom_hook = 3;
    MergeConflictError merge_conflict = 4;
  }
}

// --- OperationBranchUpdate ---

message OperationBranchUpdate {
  string commit_id = 1;
  bool repo_created = 2;
  bool branch_created = 3;
}

// --- UserMergeToRef ---

message UserMergeToRefRequest {
  Repository repository = 1 [(target_repository)=true];
  User user = 2;
  string source_sha = 3;
  bytes branch = 4 [deprecated = true];
  bytes target_ref = 5;
  bytes message = 6;
  bytes first_parent_ref = 7;
  bool allow_conflicts = 8 [deprecated = true];
  google.protobuf.Timestamp timestamp = 9;
  string expected_old_oid = 10;
  bool sign = 11;
}

message UserMergeToRefResponse {
  string commit_id = 1;
  reserved "pre_receive_error";
  reserved 2;
}

// --- UserRebaseToRef ---

message UserRebaseToRefRequest {
  Repository repository = 1 [(target_repository)=true];
  User user = 2;
  string source_sha = 3;
  bytes target_ref = 5;
  bytes first_parent_ref = 7;
  google.protobuf.Timestamp timestamp = 9;
  string expected_old_oid = 10;
}

message UserRebaseToRefResponse {
  string commit_id = 1;
}

// --- UserCherryPick ---

message UserCherryPickRequest {
  Repository repository = 1 [(target_repository)=true];
  User user = 2;
  GitCommit commit = 3;
  bytes branch_name = 4;
  bytes message = 5;
  bytes start_branch_name = 6;
  Repository start_repository = 7;
  bool dry_run = 8;
  google.protobuf.Timestamp timestamp = 9;
  string expected_old_oid = 10;
  bytes commit_author_name = 11;
  bytes commit_author_email = 12;
  bool sign = 13;
}

message UserCherryPickResponse {
  OperationBranchUpdate branch_update = 1;
  reserved "create_tree_error";
  reserved 2;
  reserved "commit_error";
  reserved 3;
  reserved "pre_receive_error";
  reserved 4;
  reserved "create_tree_error_code";
  reserved 5;
}

message UserCherryPickError {
  oneof error {
    MergeConflictError cherry_pick_conflict = 1;
    NotAncestorError target_branch_diverged = 2;
    ChangesAlreadyAppliedError changes_already_applied = 3;
    AccessCheckError access_check = 4;
  }
}

// --- UserRevert ---

message UserRevertRequest {
  Repository repository = 1 [(target_repository)=true];
  User user = 2;
  GitCommit commit = 3;
  bytes branch_name = 4;
  bytes message = 5;
  bytes start_branch_name = 6;
  Repository start_repository = 7;
  bool dry_run = 8;
  google.protobuf.Timestamp timestamp = 9;
  string expected_old_oid = 10;
  bool sign = 11;
}

message UserRevertResponse {
  enum CreateTreeError {
    NONE = 0;
    EMPTY = 1;
    CONFLICT = 2;
  }
  OperationBranchUpdate branch_update = 1;
  string create_tree_error = 2;
  string commit_error = 3;
  string pre_receive_error = 4;
  CreateTreeError create_tree_error_code = 5;
}

message UserRevertError {
  oneof error {
    MergeConflictError merge_conflict = 1;
    ChangesAlreadyAppliedError changes_already_applied = 2;
    CustomHookError custom_hook = 3;
    NotAncestorError not_ancestor = 4;
  }
}

// --- UserCommitFiles (client streaming) ---

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

message UserCommitFilesRequestHeader {
  Repository repository = 1 [(target_repository)=true];
  User user = 2;
  bytes branch_name = 3;
  bytes commit_message = 4;
  bytes commit_author_name = 5;
  bytes commit_author_email = 6;
  bytes start_branch_name = 7;
  Repository start_repository = 8;
  bool force = 9;
  string start_sha = 10;
  google.protobuf.Timestamp timestamp = 11;
  string expected_old_oid = 12;
  bool sign = 13;
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

message UserCommitFilesError {
  oneof error {
    AccessCheckError access_check = 1;
    IndexError index_update = 2;
    CustomHookError custom_hook = 3;
  }
}
```

### 3.6 commit.proto -- CommitService (read commits, trees, ancestry)

```protobuf
syntax = "proto3";
package gitaly;

// CommitService is a service which provides RPCs that interact with Git commits.
service CommitService {
  // ListCommits lists all commits reachable via a set of references by doing a
  // graph walk.
  rpc ListCommits(ListCommitsRequest) returns (stream ListCommitsResponse) {
    option (op_type) = { op: ACCESSOR };
  }

  // CommitIsAncestor checks whether a provided commit is the ancestor of another.
  rpc CommitIsAncestor(CommitIsAncestorRequest)
      returns (CommitIsAncestorResponse) {
    option (op_type) = { op: ACCESSOR };
  }

  // TreeEntry provides the tree entry for the provided path and revision.
  rpc TreeEntry(TreeEntryRequest) returns (stream TreeEntryResponse) {
    option (op_type) = { op: ACCESSOR };
  }

  // GetTreeEntries provides the tree entries for the provided path and revision,
  // including subtrees with optional recursive fetching.
  rpc GetTreeEntries(GetTreeEntriesRequest)
      returns (stream GetTreeEntriesResponse) {
    option (op_type) = { op: ACCESSOR };
  }

  // FindCommit finds a commit for a given commitish. Returns nil if not found.
  rpc FindCommit(FindCommitRequest) returns (FindCommitResponse) {
    option (op_type) = { op: ACCESSOR };
  }
}

// --- ListCommits ---

message ListCommitsRequest {
  enum Order {
    NONE = 0;
    TOPO = 1;
    DATE = 2;
  }
  Repository repository = 1 [(target_repository)=true];
  // revisions is the set of revisions to walk. Accepts gitrevisions(7) notation
  // as well as pseudo-revisions `--not` and `--all`. Must not be empty.
  repeated string revisions = 2;
  PaginationParameter pagination_params = 3;
  Order order = 4;
  uint32 max_parents = 5;
  bool disable_walk = 6;
  bool first_parent = 7;
  google.protobuf.Timestamp after = 8;
  google.protobuf.Timestamp before = 9;
  bytes author = 10;
  bool reverse = 11;
  bool ignore_case = 12;
  repeated bytes commit_message_patterns = 13;
  uint32 skip = 14;
  repeated bytes paths = 15;
}

message ListCommitsResponse {
  repeated GitCommit commits = 1;
  PaginationCursor pagination_cursor = 2;
}

// --- CommitIsAncestor ---

message CommitIsAncestorRequest {
  Repository repository = 1 [(target_repository)=true];
  string ancestor_id = 2;
  string child_id = 3;
}

message CommitIsAncestorResponse {
  bool value = 1;
}

// --- TreeEntry (single entry, streamed content) ---

message TreeEntryRequest {
  Repository repository = 1 [(target_repository)=true];
  bytes revision = 2;
  bytes path = 3;
  int64 limit = 4;
  int64 max_size = 5;
}

message TreeEntryResponse {
  enum ObjectType {
    COMMIT = 0;
    BLOB = 1;
    TREE = 2;
    TAG = 3;
  }
  ObjectType type = 1;
  string oid = 2;
  int64 size = 3;
  int32 mode = 4;
  bytes data = 5;
}

// --- TreeEntry (message type for GetTreeEntries results) ---

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
  reserved "root_oid";
  reserved 2;
}

// --- GetTreeEntries ---

message GetTreeEntriesRequest {
  enum SortBy {
    DEFAULT = 0;
    TREES_FIRST = 1;
    FILESYSTEM = 2;
  }
  Repository repository = 1 [(target_repository)=true];
  bytes revision = 2;
  bytes path = 3;
  bool recursive = 4;
  SortBy sort = 5;
  PaginationParameter pagination_params = 6;
  bool skip_flat_paths = 7;
}

message GetTreeEntriesResponse {
  repeated TreeEntry entries = 1;
  PaginationCursor pagination_cursor = 2;
}

message GetTreeEntriesError {
  oneof error {
    ResolveRevisionError resolve_tree = 1;
    PathError path = 2;
  }
}

// --- FindCommit ---

message FindCommitRequest {
  Repository repository = 1 [(target_repository)=true];
  bytes revision = 2;
  bool trailers = 3;
}

message FindCommitResponse {
  // commit is nil when the commit was not found.
  GitCommit commit = 1;
}
```

### 3.7 diff.proto -- DiffService (diffs, stats)

```protobuf
syntax = "proto3";
package gitaly;

// DiffService is a service which provides RPCs to inspect differences
// introduced between a set of commits.
service DiffService {
  // CommitDiff returns a diff between two different commits. The patch data is
  // chunked across messages and get streamed back.
  rpc CommitDiff(CommitDiffRequest) returns (stream CommitDiffResponse) {
    option (op_type) = { op: ACCESSOR };
  }

  // RawDiff returns a diff between two commits. The output is the unmodified
  // output from git-diff(1).
  rpc RawDiff(RawDiffRequest) returns (stream RawDiffResponse) {
    option (op_type) = { op: ACCESSOR };
  }

  // DiffStats returns the diff stats between two commits.
  rpc DiffStats(DiffStatsRequest) returns (stream DiffStatsResponse) {
    option (op_type) = { op: ACCESSOR };
  }

  // FindChangedPaths returns a list of files changed along with their status.
  rpc FindChangedPaths(FindChangedPathsRequest)
      returns (stream FindChangedPathsResponse) {
    option (op_type) = { op: ACCESSOR };
  }
}

// --- CommitDiff ---

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
  Repository repository = 1 [(target_repository)=true];
  string left_commit_id = 2;
  string right_commit_id = 3;
  reserved "ignore_whitespace_change";
  reserved 4;
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

// CommitDiffResponse corresponds to a single changed file in a commit.
message CommitDiffResponse {
  reserved 8;
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

// --- RawDiff ---

message RawDiffRequest {
  Repository repository = 1 [(target_repository)=true];
  string left_commit_id = 2;
  string right_commit_id = 3;
}

message RawDiffResponse {
  bytes data = 1;
}

// --- DiffStats ---

message DiffStatsRequest {
  Repository repository = 1 [(target_repository)=true];
  string left_commit_id = 2;
  string right_commit_id = 3;
}

message DiffStats {
  bytes path = 1;
  int32 additions = 2;
  int32 deletions = 3;
  bytes old_path = 4;
}

message DiffStatsResponse {
  repeated DiffStats stats = 1;
}

// --- FindChangedPaths ---

message FindChangedPathsRequest {
  enum MergeCommitDiffMode {
    MERGE_COMMIT_DIFF_MODE_UNSPECIFIED = 0;
    MERGE_COMMIT_DIFF_MODE_INCLUDE_MERGES = 1;
    MERGE_COMMIT_DIFF_MODE_ALL_PARENTS = 2;
  }
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
  Repository repository = 1 [(target_repository)=true];
  repeated string commits = 2 [deprecated=true];
  repeated Request requests = 3;
  MergeCommitDiffMode merge_commit_diff_mode = 4;
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

### 3.8 blob.proto -- BlobService (read file contents)

```protobuf
syntax = "proto3";
package gitaly;

// BlobService is a service which provides RPCs to retrieve Git blobs from a
// specific repository.
service BlobService {
  // GetBlob returns the contents of a blob object referenced by its object ID.
  rpc GetBlob(GetBlobRequest) returns (stream GetBlobResponse) {
    option (op_type) = { op: ACCESSOR };
  }

  // GetBlobs returns blobs identified via a revision and path.
  rpc GetBlobs(GetBlobsRequest) returns (stream GetBlobsResponse) {
    option (op_type) = { op: ACCESSOR };
  }

  // ListBlobs will list all blobs reachable from a given set of revisions.
  rpc ListBlobs(ListBlobsRequest) returns (stream ListBlobsResponse) {
    option (op_type) = { op: ACCESSOR };
  }
}

// --- GetBlob ---

message GetBlobRequest {
  Repository repository = 1 [(target_repository)=true];
  // oid is the object ID of the blob.
  string oid = 2;
  // limit is the maximum number of bytes to receive. Use '-1' for unlimited.
  int64 limit = 3;
}

message GetBlobResponse {
  int64 size = 1;
  bytes data = 2;
  string oid = 3;
}

// --- GetBlobs ---

message GetBlobsRequest {
  message RevisionPath {
    string revision = 1;
    bytes path = 2;
  }
  Repository repository = 1 [(target_repository)=true];
  repeated RevisionPath revision_paths = 2;
  // limit is the maximum number of bytes per blob. Use '-1' for unlimited.
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

// --- ListBlobs ---

message ListBlobsRequest {
  Repository repository = 1 [(target_repository)=true];
  repeated string revisions = 2;
  uint32 limit = 3;
  int64 bytes_limit = 4;
  bool with_paths = 5;
}

message ListBlobsResponse {
  message Blob {
    string oid = 1;
    int64 size = 2;
    bytes data = 3;
    bytes path = 4;
  }
  repeated Blob blobs = 1;
}
```

---

## 4. Rust Types

### 4.1 Domain types (in `conman-core`)

These are the Conman-side types that the adapter maps to/from proto types.
They are intentionally simpler than the proto types -- we only model what
Conman needs.

```rust
use chrono::{DateTime, Utc};

/// A reference to a gitaly repository. Built from an App.
#[derive(Debug, Clone)]
pub struct GitRepo {
    pub storage_name: String,
    pub relative_path: String,
    pub gl_repository: String,
}

/// Identity used for git operations. Built from an AuthUser.
#[derive(Debug, Clone)]
pub struct GitUser {
    pub gl_id: String,
    pub name: String,
    pub email: String,
    pub gl_username: String,
    pub timezone: String,
}

/// A branch with its tip commit.
#[derive(Debug, Clone)]
pub struct GitBranch {
    pub name: String,
    pub commit: GitCommit,
}

/// A Git commit.
#[derive(Debug, Clone)]
pub struct GitCommit {
    pub id: String,
    pub subject: String,
    pub body: String,
    pub author: GitAuthor,
    pub committer: GitAuthor,
    pub parent_ids: Vec<String>,
    pub tree_id: String,
}

/// A commit author/committer.
#[derive(Debug, Clone)]
pub struct GitAuthor {
    pub name: String,
    pub email: String,
    pub date: DateTime<Utc>,
}

/// A Git tag (annotated or lightweight).
#[derive(Debug, Clone)]
pub struct GitTag {
    pub name: String,
    pub id: String,
    pub target_commit: Option<GitCommit>,
    pub message: Option<String>,
    pub tagger: Option<GitAuthor>,
}

/// A tree entry (file or directory).
#[derive(Debug, Clone)]
pub struct GitTreeEntry {
    pub oid: String,
    pub path: String,
    pub entry_type: GitTreeEntryType,
    pub mode: i32,
    pub flat_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitTreeEntryType {
    Blob,
    Tree,
    Commit,
}

/// A single file diff entry.
#[derive(Debug, Clone)]
pub struct GitDiffEntry {
    pub from_path: String,
    pub to_path: String,
    pub from_id: String,
    pub to_id: String,
    pub old_mode: i32,
    pub new_mode: i32,
    pub binary: bool,
    pub patch: Vec<u8>,
    pub lines_added: i32,
    pub lines_removed: i32,
}

/// Diff statistics for a single path.
#[derive(Debug, Clone)]
pub struct GitDiffStat {
    pub path: String,
    pub old_path: Option<String>,
    pub additions: i32,
    pub deletions: i32,
}

/// Result of a commit-files operation.
#[derive(Debug, Clone)]
pub struct CommitResult {
    pub commit_id: String,
    pub branch_created: bool,
}

/// Result of a merge-branch operation.
#[derive(Debug, Clone)]
pub struct MergeResult {
    pub commit_id: String,
}

/// Result of a revert operation.
#[derive(Debug, Clone)]
pub struct RevertResult {
    pub commit_id: String,
}

/// A file action to include in a commit-files call.
#[derive(Debug, Clone)]
pub enum FileAction {
    Create { path: String, content: Vec<u8> },
    CreateDir { path: String },
    Update { path: String, content: Vec<u8> },
    Move { previous_path: String, path: String, content: Option<Vec<u8>> },
    Delete { path: String },
    Chmod { path: String, execute: bool },
}

/// A reference update for atomic batch updates.
#[derive(Debug, Clone)]
pub struct RefUpdate {
    /// Fully qualified reference name (e.g. "refs/heads/main").
    pub reference: String,
    /// Expected old object ID. Empty = force. All-zeroes = must not exist.
    pub old_object_id: String,
    /// New object ID. All-zeroes = delete.
    pub new_object_id: String,
}
```

### 4.2 GitAdapter trait

```rust
use crate::error::ConmanError;
use crate::types::*;

/// GitAdapter is the boundary trait for all Git operations. The production
/// implementation wraps gitaly-rs gRPC calls; tests use MockGitalyClient.
#[async_trait::async_trait]
pub trait GitAdapter: Send + Sync + 'static {
    // -- Repository --

    /// Create a new empty repository at the given storage and path.
    async fn create_repo(
        &self,
        storage: &str,
        path: &str,
    ) -> Result<(), ConmanError>;

    /// Check whether a repository exists.
    async fn repo_exists(
        &self,
        repo: &GitRepo,
    ) -> Result<bool, ConmanError>;

    /// Remove a repository.
    async fn remove_repo(
        &self,
        repo: &GitRepo,
    ) -> Result<(), ConmanError>;

    // -- Branches --

    /// Create a branch from a start point revision.
    async fn create_branch(
        &self,
        repo: &GitRepo,
        user: &GitUser,
        branch_name: &str,
        start_point: &str,
    ) -> Result<GitBranch, ConmanError>;

    /// Delete a branch.
    async fn delete_branch(
        &self,
        repo: &GitRepo,
        user: &GitUser,
        branch_name: &str,
    ) -> Result<(), ConmanError>;

    /// Look up a single branch by unqualified name.
    async fn find_branch(
        &self,
        repo: &GitRepo,
        name: &str,
    ) -> Result<Option<GitBranch>, ConmanError>;

    /// List all local branches.
    async fn list_branches(
        &self,
        repo: &GitRepo,
    ) -> Result<Vec<GitBranch>, ConmanError>;

    // -- Files --

    /// List tree entries at a given path and revision.
    async fn get_tree_entries(
        &self,
        repo: &GitRepo,
        revision: &str,
        path: &str,
        recursive: bool,
    ) -> Result<Vec<GitTreeEntry>, ConmanError>;

    /// Read a single blob by revision:path. Returns the raw bytes.
    async fn get_blob(
        &self,
        repo: &GitRepo,
        revision: &str,
        path: &str,
    ) -> Result<Vec<u8>, ConmanError>;

    /// Commit a batch of file actions to a branch, returning the new commit.
    async fn commit_files(
        &self,
        repo: &GitRepo,
        user: &GitUser,
        branch_name: &str,
        message: &str,
        actions: Vec<FileAction>,
    ) -> Result<CommitResult, ConmanError>;

    // -- Diffs --

    /// Compute a parsed diff between two commit SHAs.
    async fn commit_diff(
        &self,
        repo: &GitRepo,
        left_sha: &str,
        right_sha: &str,
    ) -> Result<Vec<GitDiffEntry>, ConmanError>;

    /// Get raw unified diff output between two commits.
    async fn raw_diff(
        &self,
        repo: &GitRepo,
        left_sha: &str,
        right_sha: &str,
    ) -> Result<Vec<u8>, ConmanError>;

    /// Get per-file diff statistics between two commits.
    async fn diff_stats(
        &self,
        repo: &GitRepo,
        left_sha: &str,
        right_sha: &str,
    ) -> Result<Vec<GitDiffStat>, ConmanError>;

    // -- Commits --

    /// Find a commit by revision (SHA, branch name, tag name, etc.).
    async fn find_commit(
        &self,
        repo: &GitRepo,
        revision: &str,
    ) -> Result<Option<GitCommit>, ConmanError>;

    /// List commits reachable from the given revisions.
    async fn list_commits(
        &self,
        repo: &GitRepo,
        revisions: Vec<String>,
        pagination: Option<(String, i32)>,
    ) -> Result<Vec<GitCommit>, ConmanError>;

    /// Check if ancestor_id is an ancestor of child_id.
    async fn is_ancestor(
        &self,
        repo: &GitRepo,
        ancestor_id: &str,
        child_id: &str,
    ) -> Result<bool, ConmanError>;

    // -- Merge / Rebase --

    /// Create a merge commit and write it to target_ref without updating a branch.
    /// Returns the merge commit SHA.
    async fn merge_to_ref(
        &self,
        repo: &GitRepo,
        user: &GitUser,
        source_sha: &str,
        target_ref: &str,
        first_parent_ref: &str,
        message: &str,
    ) -> Result<String, ConmanError>;

    /// Merge a commit into a branch (two-phase: compute + apply).
    /// Returns the merge commit SHA.
    async fn merge_branch(
        &self,
        repo: &GitRepo,
        user: &GitUser,
        commit_id: &str,
        branch: &str,
        message: &str,
    ) -> Result<MergeResult, ConmanError>;

    /// Rebase source_sha onto first_parent_ref and write result to target_ref.
    /// Returns the rebased commit SHA.
    async fn rebase_to_ref(
        &self,
        repo: &GitRepo,
        user: &GitUser,
        source_sha: &str,
        target_ref: &str,
        first_parent_ref: &str,
    ) -> Result<String, ConmanError>;

    // -- Tags --

    /// Create an annotated tag (or lightweight if message is empty).
    async fn create_tag(
        &self,
        repo: &GitRepo,
        user: &GitUser,
        tag_name: &str,
        target_revision: &str,
        message: &str,
    ) -> Result<GitTag, ConmanError>;

    /// Delete a tag.
    async fn delete_tag(
        &self,
        repo: &GitRepo,
        user: &GitUser,
        tag_name: &str,
    ) -> Result<(), ConmanError>;

    /// Find a tag by name.
    async fn find_tag(
        &self,
        repo: &GitRepo,
        tag_name: &str,
    ) -> Result<Option<GitTag>, ConmanError>;

    /// List all tags.
    async fn list_tags(
        &self,
        repo: &GitRepo,
    ) -> Result<Vec<GitTag>, ConmanError>;

    // -- Revert --

    /// Revert a commit on a branch.
    async fn revert(
        &self,
        repo: &GitRepo,
        user: &GitUser,
        commit_id: &str,
        branch_name: &str,
        message: &str,
    ) -> Result<RevertResult, ConmanError>;

    // -- Refs --

    /// Atomically update a batch of references.
    async fn update_references(
        &self,
        repo: &GitRepo,
        updates: Vec<RefUpdate>,
    ) -> Result<(), ConmanError>;
}
```

### 4.3 GitalyClient (production implementation in `conman-git`)

```rust
use tonic::transport::Channel;

/// GitalyClient holds a shared Tonic channel and creates per-request service
/// stubs. The channel uses HTTP/2 multiplexing, so cloning is cheap.
#[derive(Clone)]
pub struct GitalyClient {
    channel: Channel,
}

impl GitalyClient {
    /// Connect to the gitaly-rs gRPC server.
    pub async fn connect(address: &str) -> Result<Self, ConmanError> {
        let channel = Channel::from_shared(address.to_string())
            .map_err(|e| ConmanError::Git {
                message: format!("invalid gitaly address: {e}"),
            })?
            .connect()
            .await
            .map_err(|e| ConmanError::Git {
                message: format!("failed to connect to gitaly: {e}"),
            })?;
        Ok(Self { channel })
    }

    // Helper: build a gitaly Repository proto from our domain type.
    fn to_proto_repo(repo: &GitRepo) -> gitaly::Repository {
        gitaly::Repository {
            storage_name: repo.storage_name.clone(),
            relative_path: repo.relative_path.clone(),
            gl_repository: repo.gl_repository.clone(),
            ..Default::default()
        }
    }

    // Helper: build a gitaly User proto from our domain type.
    fn to_proto_user(user: &GitUser) -> gitaly::User {
        gitaly::User {
            gl_id: user.gl_id.clone(),
            name: user.name.clone().into_bytes(),
            email: user.email.clone().into_bytes(),
            gl_username: user.gl_username.clone(),
            timezone: user.timezone.clone(),
        }
    }
}

#[async_trait::async_trait]
impl GitAdapter for GitalyClient {
    // Each method creates the appropriate service stub from self.channel.clone()
    // and calls the corresponding gRPC RPC, converting proto types to domain
    // types. All methods are wrapped with the retry helper for transient errors.
    //
    // Example skeleton for create_branch:
    //
    // async fn create_branch(
    //     &self,
    //     repo: &GitRepo,
    //     user: &GitUser,
    //     branch_name: &str,
    //     start_point: &str,
    // ) -> Result<GitBranch, ConmanError> {
    //     retry(|| async {
    //         let mut client = OperationServiceClient::new(self.channel.clone());
    //         let response = client
    //             .user_create_branch(UserCreateBranchRequest {
    //                 repository: Some(Self::to_proto_repo(repo)),
    //                 user: Some(Self::to_proto_user(user)),
    //                 branch_name: branch_name.as_bytes().to_vec(),
    //                 start_point: start_point.as_bytes().to_vec(),
    //             })
    //             .await
    //             .map_err(|s| map_grpc_error(s))?
    //             .into_inner();
    //         let branch = response.branch.ok_or_else(|| ConmanError::Git {
    //             message: "no branch in response".into(),
    //         })?;
    //         Ok(proto_branch_to_domain(branch))
    //     })
    //     .await
    // }
    //
    // ... (all other methods follow the same pattern)
}
```

### 4.4 MockGitalyClient (test implementation)

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// MockGitalyClient records calls and returns pre-configured responses.
/// Used in unit and integration tests that don't require a real gitaly server.
#[derive(Clone, Default)]
pub struct MockGitalyClient {
    inner: Arc<Mutex<MockState>>,
}

#[derive(Default)]
struct MockState {
    repos: HashMap<String, bool>,
    branches: HashMap<String, Vec<GitBranch>>,
    commits: HashMap<String, GitCommit>,
    tags: HashMap<String, Vec<GitTag>>,
    blobs: HashMap<String, Vec<u8>>,
    tree_entries: HashMap<String, Vec<GitTreeEntry>>,
    // Track calls for assertion
    call_log: Vec<MockCall>,
}

#[derive(Debug, Clone)]
pub struct MockCall {
    pub method: String,
    pub args: Vec<String>,
}

impl MockGitalyClient {
    pub fn new() -> Self {
        Self::default()
    }

    // Builder methods for pre-configuring responses

    /// Register a repository as existing.
    pub fn with_repo(self, relative_path: &str) -> Self {
        self.inner.lock().unwrap().repos.insert(relative_path.to_string(), true);
        self
    }

    /// Register branches for a repo.
    pub fn with_branches(self, repo_path: &str, branches: Vec<GitBranch>) -> Self {
        self.inner.lock().unwrap().branches.insert(repo_path.to_string(), branches);
        self
    }

    /// Register a commit.
    pub fn with_commit(self, sha: &str, commit: GitCommit) -> Self {
        self.inner.lock().unwrap().commits.insert(sha.to_string(), commit);
        self
    }

    /// Register blob content for a revision:path key.
    pub fn with_blob(self, key: &str, data: Vec<u8>) -> Self {
        self.inner.lock().unwrap().blobs.insert(key.to_string(), data);
        self
    }

    /// Return all recorded calls for assertions.
    pub fn calls(&self) -> Vec<MockCall> {
        self.inner.lock().unwrap().call_log.clone()
    }
}

#[async_trait::async_trait]
impl GitAdapter for MockGitalyClient {
    // Each method records the call in call_log, then returns the
    // pre-configured response from the corresponding HashMap.
    // Returns ConmanError::NotFound if no response is configured.
    //
    // ...
}
```

### 4.5 Retry wrapper

```rust
use std::time::Duration;
use tonic::Code;

/// Maximum number of retry attempts for transient gRPC failures.
const MAX_RETRIES: u32 = 3;

/// Base delay for exponential backoff.
const BASE_DELAY: Duration = Duration::from_millis(100);

/// Retryable gRPC status codes.
fn is_retryable(code: Code) -> bool {
    matches!(code, Code::Unavailable | Code::DeadlineExceeded)
}

/// Execute an async gRPC call with retry on transient failures.
///
/// Uses exponential backoff: 100ms, 200ms, 400ms.
/// Non-retryable errors propagate immediately.
pub async fn retry<F, Fut, T>(f: F) -> Result<T, ConmanError>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, ConmanError>>,
{
    let mut attempt = 0;
    loop {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                attempt += 1;
                // Only retry if the error is a Git error wrapping a retryable
                // gRPC code, and we haven't exhausted retries.
                if attempt >= MAX_RETRIES || !e.is_retryable_grpc() {
                    return Err(e);
                }
                let delay = BASE_DELAY * 2u32.pow(attempt - 1);
                tokio::time::sleep(delay).await;
            }
        }
    }
}
```

### 4.6 Error mapping

```rust
use tonic::Status;

/// Map a tonic gRPC Status into a ConmanError.
pub fn map_grpc_error(status: Status) -> ConmanError {
    match status.code() {
        Code::NotFound => ConmanError::NotFound {
            entity: "git object",
            id: status.message().to_string(),
        },
        Code::AlreadyExists => ConmanError::Conflict {
            message: status.message().to_string(),
        },
        Code::FailedPrecondition => ConmanError::Conflict {
            message: status.message().to_string(),
        },
        Code::InvalidArgument => ConmanError::Validation {
            message: status.message().to_string(),
        },
        Code::PermissionDenied => ConmanError::Forbidden {
            message: status.message().to_string(),
        },
        Code::Unavailable | Code::DeadlineExceeded => ConmanError::Git {
            message: format!("transient gRPC error ({}): {}", status.code(), status.message()),
        },
        _ => ConmanError::Git {
            message: format!("gRPC error ({}): {}", status.code(), status.message()),
        },
    }
}
```

---

## 5. Database

N/A for this epic. The git adapter is a pure gRPC client with no MongoDB
dependency.

---

## 6. API Endpoints

N/A. `conman-git` is an internal crate consumed by `conman-api` and
`conman-jobs`. It does not expose HTTP endpoints.

---

## 7. Business Logic

### 7.1 Retry policy

- **Max attempts:** 3 (initial + 2 retries)
- **Backoff:** exponential -- 100ms, 200ms, 400ms
- **Retryable codes:** `UNAVAILABLE`, `DEADLINE_EXCEEDED`
- **Non-retryable codes:** all others propagate immediately

### 7.2 Connection management

- Single `tonic::transport::Channel` created at startup from `CONMAN_GITALY_ADDRESS`
- Channel uses HTTP/2 multiplexing; cloned per-request (cheap Arc clone)
- Service stubs (`OperationServiceClient`, `RefServiceClient`, etc.) are
  created from the cloned channel inside each method call

### 7.3 Error mapping

| gRPC Status Code | ConmanError Variant |
|---|---|
| `NOT_FOUND` | `NotFound` |
| `ALREADY_EXISTS` | `Conflict` |
| `FAILED_PRECONDITION` | `Conflict` |
| `INVALID_ARGUMENT` | `Validation` |
| `PERMISSION_DENIED` | `Forbidden` |
| `UNAVAILABLE` | `Git` (retryable) |
| `DEADLINE_EXCEEDED` | `Git` (retryable) |
| All others | `Git` |

### 7.4 Streaming response assembly

Several gRPC calls return streaming responses that must be assembled:

- **CommitDiff:** multiple `CommitDiffResponse` messages per file; accumulate
  `raw_patch_data` until `end_of_patch` is true
- **RawDiff:** concatenate all `data` chunks
- **GetBlob/GetBlobs:** concatenate all `data` chunks per blob
- **GetTreeEntries:** concatenate all `entries` from streamed responses
- **ListCommits:** concatenate all `commits` from streamed responses
- **FindAllBranches/FindAllTags:** concatenate all items from streamed responses
- **DiffStats:** concatenate all `stats` from streamed responses

### 7.5 Repository mapping

Each Conman `App` maps to a gitaly `Repository`:

```rust
fn app_to_git_repo(app: &App) -> GitRepo {
    GitRepo {
        storage_name: "default".to_string(),
        relative_path: app.repo_path.clone(),
        gl_repository: format!("app-{}", app.id.to_hex()),
    }
}
```

### 7.6 User mapping

Each authenticated Conman user maps to a gitaly `User`:

```rust
fn auth_user_to_git_user(user: &AuthUser) -> GitUser {
    GitUser {
        gl_id: format!("user-{}", user.user_id.to_hex()),
        name: user.display_name.clone(),
        email: user.email.clone(),
        gl_username: user.email.clone(),
        timezone: "UTC".to_string(),
    }
}
```

---

## 8. Implementation Checklist

Ordered TDD steps. Each step produces tests first, then implementation.

- [ ] **E01-01** Set up `conman-git` crate in workspace
  - Create `conman-git/Cargo.toml` with deps: `tonic`, `prost`, `async-trait`, `tokio`, `conman-core`
  - Configure `build.rs` with prost-build for gitaly proto compilation
  - Copy required `.proto` files into `conman-git/proto/`
  - Verify proto compilation succeeds

- [ ] **E01-02** Define domain types in `conman-core`
  - Add `GitRepo`, `GitUser`, `GitBranch`, `GitCommit`, `GitAuthor`, `GitTag`
  - Add `GitTreeEntry`, `GitTreeEntryType`, `GitDiffEntry`, `GitDiffStat`
  - Add `CommitResult`, `MergeResult`, `RevertResult`, `FileAction`, `RefUpdate`
  - Add `is_retryable_grpc()` method to `ConmanError`

- [ ] **E01-03** Define `GitAdapter` trait
  - Write the full trait as specified in section 4.2
  - Ensure all methods return `Result<T, ConmanError>`

- [ ] **E01-04** Implement `MockGitalyClient`
  - Builder methods for pre-configuring responses
  - Call recording for test assertions
  - `impl GitAdapter for MockGitalyClient`
  - Unit tests proving mock records calls and returns configured data

- [ ] **E01-05** Implement proto-to-domain type conversions
  - `proto_commit_to_domain(GitCommit) -> crate::GitCommit`
  - `proto_branch_to_domain(Branch) -> GitBranch`
  - `proto_tag_to_domain(Tag) -> GitTag`
  - `proto_tree_entry_to_domain(TreeEntry) -> GitTreeEntry`
  - `proto_diff_to_domain(CommitDiffResponse) -> GitDiffEntry`
  - `proto_diff_stat_to_domain(DiffStats) -> GitDiffStat`
  - Unit tests for each conversion

- [ ] **E01-06** Implement `map_grpc_error` and retry wrapper
  - Error mapping function as specified in section 4.6
  - Retry wrapper as specified in section 4.5
  - Unit tests: retryable codes trigger retry, non-retryable propagate immediately

- [ ] **E01-07** Implement `GitalyClient::connect` and helpers
  - Channel creation with error handling
  - `to_proto_repo()` and `to_proto_user()` helpers

- [ ] **E01-08** Implement `GitAdapter` for `GitalyClient` -- Repository methods
  - `create_repo`: calls `RepositoryServiceClient::create_repository`
  - `repo_exists`: calls `RepositoryServiceClient::repository_exists`
  - `remove_repo`: calls `RepositoryServiceClient::remove_repository`

- [ ] **E01-09** Implement `GitAdapter` for `GitalyClient` -- Branch methods
  - `create_branch`: calls `OperationServiceClient::user_create_branch`
  - `delete_branch`: calls `OperationServiceClient::user_delete_branch`
  - `find_branch`: calls `RefServiceClient::find_branch`
  - `list_branches`: calls `RefServiceClient::find_local_branches` (streaming)

- [ ] **E01-10** Implement `GitAdapter` for `GitalyClient` -- File methods
  - `get_tree_entries`: calls `CommitServiceClient::get_tree_entries` (streaming)
  - `get_blob`: calls `BlobServiceClient::get_blobs` with revision:path (streaming)
  - `commit_files`: calls `OperationServiceClient::user_commit_files` (client streaming)

- [ ] **E01-11** Implement `GitAdapter` for `GitalyClient` -- Diff methods
  - `commit_diff`: calls `DiffServiceClient::commit_diff` (streaming, assemble per-file)
  - `raw_diff`: calls `DiffServiceClient::raw_diff` (streaming, concatenate chunks)
  - `diff_stats`: calls `DiffServiceClient::diff_stats` (streaming)

- [ ] **E01-12** Implement `GitAdapter` for `GitalyClient` -- Commit methods
  - `find_commit`: calls `CommitServiceClient::find_commit`
  - `list_commits`: calls `CommitServiceClient::list_commits` (streaming)
  - `is_ancestor`: calls `CommitServiceClient::commit_is_ancestor`

- [ ] **E01-13** Implement `GitAdapter` for `GitalyClient` -- Merge/Rebase methods
  - `merge_to_ref`: calls `OperationServiceClient::user_merge_to_ref`
  - `merge_branch`: calls `OperationServiceClient::user_merge_branch` (bidi streaming)
  - `rebase_to_ref`: calls `OperationServiceClient::user_rebase_to_ref`

- [ ] **E01-14** Implement `GitAdapter` for `GitalyClient` -- Tag methods
  - `create_tag`: calls `OperationServiceClient::user_create_tag`
  - `delete_tag`: calls `OperationServiceClient::user_delete_tag`
  - `find_tag`: calls `RefServiceClient::find_tag`
  - `list_tags`: calls `RefServiceClient::find_all_tags` (streaming)

- [ ] **E01-15** Implement `GitAdapter` for `GitalyClient` -- Revert and Refs
  - `revert`: calls `OperationServiceClient::user_revert`
  - `update_references`: calls `RefServiceClient::update_references` (client streaming)

- [ ] **E01-16** Integration tests with mock gRPC server
  - Stand up a Tonic mock server implementing the gitaly service RPCs
  - Test each `GitalyClient` method end-to-end against the mock server
  - Verify retry behavior with simulated `UNAVAILABLE` responses
  - Verify streaming response assembly (multi-chunk blobs, diffs, etc.)

---

## 9. Test Cases

### 9.1 Unit tests (in `conman-git`)

| Test | Description |
|------|-------------|
| `test_proto_commit_to_domain` | Converts a proto GitCommit with all fields populated; verify domain fields match |
| `test_proto_commit_empty_body` | Handles GitCommit with empty body; body_size > 0 noted |
| `test_proto_branch_to_domain` | Converts Branch with nested commit |
| `test_proto_tag_annotated` | Converts annotated Tag with message and tagger |
| `test_proto_tag_lightweight` | Converts lightweight Tag (no message, no tagger) |
| `test_proto_tree_entry_blob` | Converts TreeEntry with type BLOB |
| `test_proto_tree_entry_tree` | Converts TreeEntry with type TREE |
| `test_proto_diff_to_domain` | Converts assembled CommitDiffResponse to GitDiffEntry |
| `test_proto_diff_stat_to_domain` | Converts DiffStats with rename (old_path set) |
| `test_map_grpc_not_found` | `map_grpc_error` maps NOT_FOUND to `ConmanError::NotFound` |
| `test_map_grpc_already_exists` | Maps ALREADY_EXISTS to `ConmanError::Conflict` |
| `test_map_grpc_failed_precondition` | Maps FAILED_PRECONDITION to `ConmanError::Conflict` |
| `test_map_grpc_invalid_argument` | Maps INVALID_ARGUMENT to `ConmanError::Validation` |
| `test_map_grpc_permission_denied` | Maps PERMISSION_DENIED to `ConmanError::Forbidden` |
| `test_map_grpc_unavailable` | Maps UNAVAILABLE to `ConmanError::Git` (retryable) |
| `test_map_grpc_internal` | Maps INTERNAL to `ConmanError::Git` (non-retryable) |
| `test_retry_succeeds_on_second_attempt` | Retry wrapper retries on UNAVAILABLE, succeeds on attempt 2 |
| `test_retry_exhausts_attempts` | After 3 failures, error propagates |
| `test_retry_non_retryable_immediate` | Non-retryable error propagates without retry |
| `test_retry_backoff_timing` | Verify exponential backoff delays (100ms, 200ms, 400ms) |
| `test_mock_records_calls` | MockGitalyClient records method name and args |
| `test_mock_returns_configured_branch` | Pre-configured branch is returned from find_branch |
| `test_mock_returns_configured_commit` | Pre-configured commit is returned from find_commit |
| `test_mock_returns_configured_blob` | Pre-configured blob data is returned from get_blob |
| `test_mock_not_found_when_unconfigured` | Returns NotFound for unconfigured items |

### 9.2 Integration tests (with Tonic mock server)

| Test | Description |
|------|-------------|
| `test_create_repo_roundtrip` | Create + exists + remove repository |
| `test_create_and_find_branch` | Create a branch, then find it |
| `test_list_branches_empty` | List branches on empty repo returns empty vec |
| `test_commit_files_and_find_commit` | Commit files, then find the resulting commit |
| `test_commit_diff_multifile` | Diff with multiple files; verify per-file assembly |
| `test_raw_diff_large_payload` | Raw diff with multi-chunk response; verify concatenation |
| `test_diff_stats` | DiffStats returns correct per-file additions/deletions |
| `test_get_tree_entries_recursive` | Recursive tree listing returns nested entries |
| `test_get_blob_content` | GetBlobs returns correct file content |
| `test_merge_to_ref` | MergeToRef creates merge commit at target ref |
| `test_merge_branch_apply` | MergeBranch two-phase: compute + apply |
| `test_merge_branch_conflict` | MergeBranch returns MergeConflictError on conflict |
| `test_rebase_to_ref` | RebaseToRef creates rebased commit at target ref |
| `test_create_annotated_tag` | Create tag with message; verify annotated |
| `test_create_lightweight_tag` | Create tag without message; verify lightweight |
| `test_find_and_list_tags` | Create tags, find one, list all |
| `test_revert_commit` | Revert a commit on a branch |
| `test_update_references_atomic` | Batch update multiple refs atomically |
| `test_is_ancestor_true` | Ancestor check returns true for valid lineage |
| `test_is_ancestor_false` | Ancestor check returns false for diverged commits |
| `test_retry_on_unavailable` | Mock server returns UNAVAILABLE twice, then OK; verify success |
| `test_retry_on_deadline_exceeded` | Mock server returns DEADLINE_EXCEEDED once; verify retry |
| `test_non_retryable_propagates` | Mock server returns INVALID_ARGUMENT; verify no retry |

---

## 10. Acceptance Criteria

- [ ] `conman-git` crate compiles and all protos are generated via prost-build
- [ ] `GitAdapter` trait is defined with all methods listed in section 4.2
- [ ] `GitalyClient` implements `GitAdapter` with real gRPC calls to gitaly-rs
- [ ] `MockGitalyClient` implements `GitAdapter` for test isolation
- [ ] No route handler in `conman-api` calls gitaly-rs directly -- everything
      goes through the `GitAdapter` trait
- [ ] Retry wrapper retries on `UNAVAILABLE` and `DEADLINE_EXCEEDED` with
      exponential backoff (3 attempts max)
- [ ] Non-retryable gRPC errors propagate immediately without retry
- [ ] All proto-to-domain type conversions are tested
- [ ] Streaming responses (blobs, diffs, tree entries, commits, branches, tags)
      are correctly assembled from multiple gRPC messages
- [ ] Integration tests pass against a Tonic mock gRPC server
- [ ] The adapter can be swapped in tests without a networked Git backend
- [ ] `cargo test -p conman-git` passes with all tests green
