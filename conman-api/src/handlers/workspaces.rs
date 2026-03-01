use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use conman_auth::AuthUser;
use conman_core::{
    BaseRefType, Changeset, CommitMode, ConflictStatus, ConmanError, FileAction, FileEntry,
    FileEntryType, GitRepo, GitTreeEntryType, GitUser, Repo, Role, Workspace,
};
use conman_db::CreateWorkspaceInput;
use serde::{Deserialize, Serialize};

use crate::{error::ApiConmanError, events::emit_audit, response::ApiResponse, state::AppState};

#[derive(Debug, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub title: Option<String>,
    pub branch_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkspaceRequest {
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FilePathQuery {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub recursive: bool,
}

#[derive(Debug, Deserialize)]
pub struct WorkspacePatchQuery {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct WriteFileRequest {
    pub path: String,
    pub content: String,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteFileRequest {
    pub path: String,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCheckpointRequest {
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FileTreeResponse {
    pub path: String,
    pub entries: Vec<FileEntry>,
}

#[derive(Debug, Serialize)]
pub struct FileContentResponse {
    pub path: String,
    pub content: String,
    pub size: i64,
}

#[derive(Debug, Serialize)]
pub struct FileWriteResponse {
    pub commit_sha: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct CheckpointResponse {
    pub commit_sha: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ResetResponse {
    pub head_sha: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct SyncIntegrationResponse {
    pub clean: bool,
    pub head_sha: String,
    pub conflicting_paths: Vec<String>,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceChangesEntry {
    pub path: String,
    pub old_path: Option<String>,
    pub additions: i32,
    pub deletions: i32,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceChangesResponse {
    pub workspace_id: String,
    pub base_sha: String,
    pub head_sha: String,
    pub has_changes: bool,
    pub files_changed: usize,
    pub additions: i32,
    pub deletions: i32,
    pub entries: Vec<WorkspaceChangesEntry>,
}

#[derive(Debug, Serialize)]
pub struct WorkspacePatchResponse {
    pub workspace_id: String,
    pub base_sha: String,
    pub head_sha: String,
    pub path: String,
    pub patch: String,
    pub binary: bool,
    pub lines_added: i32,
    pub lines_removed: i32,
}

#[derive(Debug, Serialize)]
pub struct OpenWorkspaceChangesetResponse {
    pub changeset: Option<Changeset>,
}

fn git_repo(repo: &Repo) -> GitRepo {
    GitRepo {
        storage_name: "default".to_string(),
        relative_path: repo.repo_path.clone(),
        gl_repository: format!("project-{}", repo.id),
    }
}

fn git_user(auth: &AuthUser) -> GitUser {
    GitUser {
        gl_id: format!("user-{}", auth.user_id),
        name: auth.email.clone(),
        email: auth.email.clone(),
        gl_username: auth.email.clone(),
        timezone: "UTC".to_string(),
    }
}

fn default_workspace_branch(auth: &AuthUser, repo: &Repo) -> String {
    let app_slug = repo
        .name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("ws/{}/{app_slug}", auth.user_id)
}

fn normalize_optional_title(title: Option<String>) -> Option<String> {
    title
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn format_first_workspace_title(user_name: &str, app_title: &str) -> String {
    format!("{}'s {} Workspace", user_name.trim(), app_title.trim())
}

async fn resolve_first_workspace_title(
    state: &AppState,
    auth: &AuthUser,
    repo: &Repo,
) -> Result<String, ApiConmanError> {
    let owner_name = conman_db::UserRepo::new(state.db.clone())
        .find_by_id(&auth.user_id)
        .await?
        .map(|user| user.name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| auth.email.clone());

    let app_title = conman_db::AppRepo::new(state.db.clone())
        .list_by_repo(&repo.id)
        .await?
        .into_iter()
        .next()
        .map(|app| app.title.trim().to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| repo.name.clone());

    Ok(format_first_workspace_title(&owner_name, &app_title))
}

fn is_blocked_path(path: &str, blocked: &[String]) -> bool {
    blocked.iter().any(|pattern| {
        if let Some(prefix) = pattern.strip_suffix("/**") {
            path == prefix || path.starts_with(&format!("{prefix}/"))
        } else {
            path == pattern
        }
    })
}

fn normalize_path(path: &str) -> Result<String, ConmanError> {
    let normalized = path.trim().trim_start_matches('/').to_string();
    if normalized.contains("..") {
        return Err(ConmanError::Validation {
            message: "path cannot contain ..".to_string(),
        });
    }
    Ok(normalized)
}

fn normalize_diff_path(path: &str) -> String {
    path.trim_start_matches('/')
        .trim_start_matches("./")
        .trim_start_matches("a/")
        .trim_start_matches("b/")
        .to_string()
}

fn diff_path_matches(candidate: &str, wanted: &str) -> bool {
    normalize_diff_path(candidate) == normalize_diff_path(wanted)
}

fn extract_patch_for_path(raw_diff: &[u8], wanted_path: &str) -> Option<String> {
    let wanted = normalize_diff_path(wanted_path);
    let text = String::from_utf8_lossy(raw_diff);

    let mut current_chunk = String::new();
    let mut current_matches = false;
    let mut result: Option<String> = None;

    for line in text.lines() {
        if line.starts_with("diff --git ") {
            if current_matches && !current_chunk.is_empty() {
                result = Some(std::mem::take(&mut current_chunk));
                break;
            }

            current_chunk = String::new();

            // Expected format: diff --git a/path b/path
            let mut parts = line.split_whitespace();
            let _ = parts.next();
            let _ = parts.next();
            let left = parts.next().unwrap_or_default();
            let right = parts.next().unwrap_or_default();

            let left = left.strip_prefix("a/").unwrap_or(left);
            let right = right.strip_prefix("b/").unwrap_or(right);
            current_matches = normalize_diff_path(left) == wanted || normalize_diff_path(right) == wanted;
        }

        current_chunk.push_str(line);
        current_chunk.push('\n');
    }

    if result.is_none() && current_matches && !current_chunk.is_empty() {
        result = Some(current_chunk);
    }

    result
}

fn is_unimplemented_git(err: &ConmanError) -> bool {
    matches!(err, ConmanError::Git { message } if message.contains("not implemented"))
}

fn is_missing_revision_git(err: &ConmanError) -> bool {
    matches!(
        err,
        ConmanError::Git { message }
            if message.contains("Needed a single revision")
                || message.contains("bad revision")
                || message.contains("Not a valid object name")
    )
}

fn resolve_workspace_base_sha(workspace: &Workspace, repo: &Repo) -> String {
    if !workspace.base_sha.trim().is_empty() {
        return workspace.base_sha.clone();
    }

    if matches!(workspace.base_ref_type, BaseRefType::Commit)
        && !workspace.base_ref_value.trim().is_empty()
    {
        return workspace.base_ref_value.clone();
    }

    if !workspace.base_ref_value.trim().is_empty() {
        return workspace.base_ref_value.clone();
    }

    repo.integration_branch.clone()
}

async fn find_repo(state: &AppState, repo_id: &str) -> Result<Repo, ApiConmanError> {
    conman_db::RepoStore::new(state.db.clone())
        .find_by_id(repo_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "repo",
            id: repo_id.to_string(),
        })
        .map_err(Into::into)
}

async fn find_workspace_for_owner(
    state: &AppState,
    workspace_id: &str,
    owner_user_id: &str,
) -> Result<Workspace, ApiConmanError> {
    let workspace = conman_db::WorkspaceRepo::new(state.db.clone())
        .find_by_id(workspace_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "workspace",
            id: workspace_id.to_string(),
        })?;
    if workspace.owner_user_id != owner_user_id {
        return Err(ConmanError::Forbidden {
            message: "workspace is not owned by current user".to_string(),
        }
        .into());
    }
    Ok(workspace)
}

async fn ensure_workspace_branch_exists(
    state: &AppState,
    repo: &GitRepo,
    user: &GitUser,
    workspace: &Workspace,
    integration_branch: &str,
) -> Result<(), ApiConmanError> {
    let branch = state
        .git_adapter
        .find_branch(repo, &workspace.branch_name)
        .await?;
    if branch.is_none() {
        state
            .git_adapter
            .create_branch(repo, user, &workspace.branch_name, integration_branch)
            .await?;
    }
    Ok(())
}

async fn audit_workspace_event(
    state: &AppState,
    auth: &AuthUser,
    repo_id: &str,
    workspace_id: &str,
    action: &str,
    after: Option<serde_json::Value>,
    commit_sha: Option<&str>,
) {
    if let Err(err) = emit_audit(
        state,
        Some(&auth.user_id),
        Some(repo_id),
        "workspace",
        workspace_id,
        action,
        None,
        after,
        commit_sha,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }
}

async fn ensure_default_workspace(
    state: &AppState,
    auth: &AuthUser,
    repo: &Repo,
) -> Result<Workspace, ApiConmanError> {
    // Auditing is delegated to `create_workspace_entry` via `audit_workspace_event(...)`.
    create_workspace_entry(state, auth, repo, None, None).await
}

async fn create_workspace_entry(
    state: &AppState,
    auth: &AuthUser,
    repo: &Repo,
    requested_branch_name: Option<String>,
    requested_title: Option<String>,
) -> Result<Workspace, ApiConmanError> {
    let workspace_repo = conman_db::WorkspaceRepo::new(state.db.clone());
    let existing_workspaces = workspace_repo
        .list_by_repo_owner(&repo.id, &auth.user_id)
        .await?;
    let is_first_workspace = existing_workspaces.is_empty();

    let is_default = requested_branch_name.is_none();
    if is_default
        && existing_workspaces
            .iter()
            .any(|workspace| workspace.is_default)
    {
        return Err(ConmanError::Conflict {
            message: "default workspace already exists for this repo".to_string(),
        }
        .into());
    }

    let branch_name = requested_branch_name.unwrap_or_else(|| default_workspace_branch(auth, repo));
    let title = if is_first_workspace {
        Some(resolve_first_workspace_title(state, auth, repo).await?)
    } else {
        normalize_optional_title(requested_title)
    };

    let git_repo = git_repo(repo);
    let git_user = git_user(auth);
    let mut head_sha = repo.integration_branch.clone();
    match state
        .git_adapter
        .create_branch(
            &git_repo,
            &git_user,
            &branch_name,
            repo.integration_branch.as_str(),
        )
        .await
    {
        Ok(branch) => {
            if !branch.commit.id.is_empty() {
                head_sha = branch.commit.id;
            }
        }
        Err(err) if is_unimplemented_git(&err) => {}
        Err(err) => return Err(err.into()),
    }

    let workspace = workspace_repo
        .create(CreateWorkspaceInput {
            repo_id: repo.id.clone(),
            owner_user_id: auth.user_id.clone(),
            branch_name,
            title,
            is_default,
            base_ref_type: BaseRefType::Branch,
            base_ref_value: repo.integration_branch.clone(),
            base_sha: head_sha.clone(),
            head_sha,
        })
        .await?;

    audit_workspace_event(
        state,
        auth,
        &workspace.repo_id,
        &workspace.id,
        "created",
        serde_json::to_value(&workspace).ok(),
        Some(&workspace.head_sha),
    )
    .await;

    Ok(workspace)
}

pub async fn list_workspaces(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(repo_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<Workspace>>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let repo = find_repo(&state, &repo_id).await?;
    let workspace_repo = conman_db::WorkspaceRepo::new(state.db.clone());
    let mut workspaces = workspace_repo
        .list_by_repo_owner(&repo_id, &auth.user_id)
        .await?;

    if workspaces.is_empty() {
        workspaces.push(ensure_default_workspace(&state, &auth, &repo).await?);
    }

    Ok(Json(ApiResponse::ok(workspaces)))
}

pub async fn create_workspace(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(repo_id): Path<String>,
    Json(req): Json<CreateWorkspaceRequest>,
) -> Result<Json<ApiResponse<Workspace>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let repo = find_repo(&state, &repo_id).await?;
    // Auditing is delegated to `create_workspace_entry` via `audit_workspace_event(...)`.
    let workspace =
        create_workspace_entry(&state, &auth, &repo, req.branch_name, req.title).await?;

    Ok(Json(ApiResponse::ok(workspace)))
}

pub async fn get_workspace(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, workspace_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Workspace>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let workspace = find_workspace_for_owner(&state, &workspace_id, &auth.user_id).await?;
    if workspace.repo_id != repo_id {
        return Err(ConmanError::Forbidden {
            message: "workspace does not belong to repo".to_string(),
        }
        .into());
    }
    Ok(Json(ApiResponse::ok(workspace)))
}

pub async fn get_workspace_changes(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, workspace_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<WorkspaceChangesResponse>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let repo = find_repo(&state, &repo_id).await?;
    let workspace = find_workspace_for_owner(&state, &workspace_id, &auth.user_id).await?;
    if workspace.repo_id != repo_id {
        return Err(ConmanError::Forbidden {
            message: "workspace does not belong to repo".to_string(),
        }
        .into());
    }

    let git_repo = git_repo(&repo);
    let base_sha = resolve_workspace_base_sha(&workspace, &repo);
    let head_sha = workspace.head_sha.clone();
    let stats = state
        .git_adapter
        .diff_stats(&git_repo, &base_sha, &head_sha)
        .await?;

    let additions = stats.iter().map(|entry| entry.additions).sum::<i32>();
    let deletions = stats.iter().map(|entry| entry.deletions).sum::<i32>();
    let entries = stats
        .into_iter()
        .map(|entry| WorkspaceChangesEntry {
            path: entry.path,
            old_path: entry.old_path,
            additions: entry.additions,
            deletions: entry.deletions,
        })
        .collect::<Vec<_>>();

    let response = WorkspaceChangesResponse {
        workspace_id: workspace.id,
        base_sha,
        head_sha,
        has_changes: !entries.is_empty(),
        files_changed: entries.len(),
        additions,
        deletions,
        entries,
    };

    Ok(Json(ApiResponse::ok(response)))
}

pub async fn get_workspace_change_patch(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, workspace_id)): Path<(String, String)>,
    Query(query): Query<WorkspacePatchQuery>,
) -> Result<Json<ApiResponse<WorkspacePatchResponse>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let repo = find_repo(&state, &repo_id).await?;
    let workspace = find_workspace_for_owner(&state, &workspace_id, &auth.user_id).await?;
    if workspace.repo_id != repo_id {
        return Err(ConmanError::Forbidden {
            message: "workspace does not belong to repo".to_string(),
        }
        .into());
    }

    let path = normalize_path(&query.path)?;
    if path.is_empty() {
        return Err(ConmanError::Validation {
            message: "path is required".to_string(),
        }
        .into());
    }

    let git_repo = git_repo(&repo);
    let base_sha = resolve_workspace_base_sha(&workspace, &repo);
    let head_sha = workspace.head_sha.clone();
    let entries = state
        .git_adapter
        .commit_diff(&git_repo, &base_sha, &head_sha)
        .await?;

    let matched = entries
        .into_iter()
        .find(|entry| diff_path_matches(&entry.to_path, &path) || diff_path_matches(&entry.from_path, &path));

    if let Some(entry) = matched {
        let response = WorkspacePatchResponse {
            workspace_id: workspace.id,
            base_sha,
            head_sha,
            path,
            patch: String::from_utf8_lossy(&entry.patch).to_string(),
            binary: entry.binary,
            lines_added: entry.lines_added,
            lines_removed: entry.lines_removed,
        };

        return Ok(Json(ApiResponse::ok(response)));
    }

    // Fallback for gitaly setups where commit_diff entries don't map paths reliably.
    let raw = state.git_adapter.raw_diff(&git_repo, &base_sha, &head_sha).await?;
    let patch = extract_patch_for_path(&raw, &path).ok_or_else(|| ConmanError::NotFound {
        entity: "workspace change",
        id: path.clone(),
    })?;

    let stats = state
        .git_adapter
        .diff_stats(&git_repo, &base_sha, &head_sha)
        .await?
        .into_iter()
        .find(|entry| {
            diff_path_matches(&entry.path, &path)
                || entry
                    .old_path
                    .as_deref()
                    .map(|value| diff_path_matches(value, &path))
                    .unwrap_or(false)
        });

    let response = WorkspacePatchResponse {
        workspace_id: workspace.id,
        base_sha,
        head_sha,
        path,
        binary: patch.contains("GIT binary patch") || patch.contains("Binary files"),
        patch,
        lines_added: stats.as_ref().map(|value| value.additions).unwrap_or(0),
        lines_removed: stats.as_ref().map(|value| value.deletions).unwrap_or(0),
    };

    Ok(Json(ApiResponse::ok(response)))
}

pub async fn get_workspace_open_changeset(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, workspace_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<OpenWorkspaceChangesetResponse>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let workspace = find_workspace_for_owner(&state, &workspace_id, &auth.user_id).await?;
    if workspace.repo_id != repo_id {
        return Err(ConmanError::Forbidden {
            message: "workspace does not belong to repo".to_string(),
        }
        .into());
    }

    let changeset = conman_db::ChangesetRepo::new(state.db.clone())
        .find_open_by_workspace(&workspace.id)
        .await?;
    if let Some(value) = changeset.as_ref() {
        if value.repo_id != repo_id {
            return Err(ConmanError::Forbidden {
                message: "changeset does not belong to repo".to_string(),
            }
            .into());
        }
    }

    Ok(Json(ApiResponse::ok(OpenWorkspaceChangesetResponse {
        changeset,
    })))
}

pub async fn update_workspace(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, workspace_id)): Path<(String, String)>,
    Json(req): Json<UpdateWorkspaceRequest>,
) -> Result<Json<ApiResponse<Workspace>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let workspace = find_workspace_for_owner(&state, &workspace_id, &auth.user_id).await?;
    if workspace.repo_id != repo_id {
        return Err(ConmanError::Forbidden {
            message: "workspace does not belong to repo".to_string(),
        }
        .into());
    }
    let updated = conman_db::WorkspaceRepo::new(state.db.clone())
        .update_title(&workspace_id, req.title)
        .await?;

    audit_workspace_event(
        &state,
        &auth,
        &repo_id,
        &updated.id,
        "updated",
        serde_json::to_value(&updated).ok(),
        Some(&updated.head_sha),
    )
    .await;

    Ok(Json(ApiResponse::ok(updated)))
}

pub async fn get_workspace_file_or_tree(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, workspace_id)): Path<(String, String)>,
    Query(query): Query<FilePathQuery>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let repo = find_repo(&state, &repo_id).await?;
    let workspace = find_workspace_for_owner(&state, &workspace_id, &auth.user_id).await?;
    if workspace.repo_id != repo_id {
        return Err(ConmanError::Forbidden {
            message: "workspace does not belong to repo".to_string(),
        }
        .into());
    }

    let path = normalize_path(&query.path)?;
    let git_repo = git_repo(&repo);

    if !path.is_empty() {
        match state
            .git_adapter
            .get_blob(&git_repo, &workspace.branch_name, &path)
            .await
        {
            Ok(content) => {
                let file = FileContentResponse {
                    path,
                    size: content.len() as i64,
                    content: STANDARD.encode(content),
                };
                return Ok(Json(ApiResponse::ok(serde_json::to_value(file).map_err(
                    |e| ConmanError::Internal {
                        message: format!("failed to serialize file content response: {e}"),
                    },
                )?)));
            }
            Err(err) if is_unimplemented_git(&err) => {
                return Err(err.into());
            }
            Err(_) => {}
        }
    }

    let entries = state
        .git_adapter
        .get_tree_entries(&git_repo, &workspace.branch_name, &path, query.recursive)
        .await?;

    let mapped = entries
        .into_iter()
        .map(|entry| FileEntry {
            path: entry.path,
            entry_type: if matches!(entry.entry_type, GitTreeEntryType::Tree) {
                FileEntryType::Dir
            } else {
                FileEntryType::File
            },
            size: 0,
            oid: entry.oid,
        })
        .collect::<Vec<_>>();

    let tree = FileTreeResponse {
        path,
        entries: mapped,
    };
    Ok(Json(ApiResponse::ok(serde_json::to_value(tree).map_err(
        |e| ConmanError::Internal {
            message: format!("failed to serialize file tree response: {e}"),
        },
    )?)))
}

pub async fn write_workspace_file(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, workspace_id)): Path<(String, String)>,
    Json(req): Json<WriteFileRequest>,
) -> Result<Json<ApiResponse<FileWriteResponse>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let repo = find_repo(&state, &repo_id).await?;
    let workspace = find_workspace_for_owner(&state, &workspace_id, &auth.user_id).await?;
    if workspace.repo_id != repo_id {
        return Err(ConmanError::Forbidden {
            message: "workspace does not belong to repo".to_string(),
        }
        .into());
    }

    let path = normalize_path(&req.path)?;
    if is_blocked_path(&path, &repo.settings.blocked_paths) {
        return Err(ConmanError::Forbidden {
            message: format!("path is blocked by repo settings: {path}"),
        }
        .into());
    }
    let content = STANDARD
        .decode(req.content.as_bytes())
        .map_err(|e| ConmanError::Validation {
            message: format!("content must be base64: {e}"),
        })?;
    if content.len() as u64 > repo.settings.file_size_limit_bytes {
        return Err(ConmanError::Validation {
            message: format!(
                "file exceeds repo limit of {} bytes",
                repo.settings.file_size_limit_bytes
            ),
        }
        .into());
    }

    let message = req.message.unwrap_or_else(|| format!("update {path}"));
    let git_repo = git_repo(&repo);
    let git_user = git_user(&auth);
    ensure_workspace_branch_exists(&state, &git_repo, &git_user, &workspace, &repo.integration_branch)
        .await?;
    let file_action = match state
        .git_adapter
        .get_blob(&git_repo, &workspace.branch_name, &path)
        .await
    {
        Ok(_) => FileAction::Update {
            path: path.clone(),
            content: content.clone(),
        },
        Err(ConmanError::NotFound { .. }) => FileAction::Create {
            path: path.clone(),
            content: content.clone(),
        },
        Err(err) if is_missing_revision_git(&err) => FileAction::Create {
            path: path.clone(),
            content: content.clone(),
        },
        Err(err) if is_unimplemented_git(&err) => FileAction::Update {
            path: path.clone(),
            content: content.clone(),
        },
        Err(err) => return Err(err.into()),
    };
    let result = state
        .git_adapter
        .commit_files(
            &git_repo,
            &git_user,
            &workspace.branch_name,
            None,
            &message,
            vec![file_action],
        )
        .await?;

    conman_db::WorkspaceRepo::new(state.db.clone())
        .update_head(&workspace.id, &result.commit_id)
        .await?;

    audit_workspace_event(
        &state,
        &auth,
        &repo_id,
        &workspace.id,
        "file_written",
        Some(serde_json::json!({
            "path": path,
            "commit_sha": result.commit_id,
            "workspace_id": workspace.id,
        })),
        Some(&result.commit_id),
    )
    .await;

    Ok(Json(ApiResponse::ok(FileWriteResponse {
        commit_sha: result.commit_id,
        path,
    })))
}

pub async fn delete_workspace_file(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, workspace_id)): Path<(String, String)>,
    Json(req): Json<DeleteFileRequest>,
) -> Result<Json<ApiResponse<FileWriteResponse>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let repo = find_repo(&state, &repo_id).await?;
    let workspace = find_workspace_for_owner(&state, &workspace_id, &auth.user_id).await?;
    if workspace.repo_id != repo_id {
        return Err(ConmanError::Forbidden {
            message: "workspace does not belong to repo".to_string(),
        }
        .into());
    }

    let path = normalize_path(&req.path)?;
    if is_blocked_path(&path, &repo.settings.blocked_paths) {
        return Err(ConmanError::Forbidden {
            message: format!("path is blocked by repo settings: {path}"),
        }
        .into());
    }

    let message = req.message.unwrap_or_else(|| format!("delete {path}"));
    let git_repo = git_repo(&repo);
    let git_user = git_user(&auth);
    ensure_workspace_branch_exists(&state, &git_repo, &git_user, &workspace, &repo.integration_branch)
        .await?;
    let result = state
        .git_adapter
        .commit_files(
            &git_repo,
            &git_user,
            &workspace.branch_name,
            None,
            &message,
            vec![FileAction::Delete { path: path.clone() }],
        )
        .await?;

    conman_db::WorkspaceRepo::new(state.db.clone())
        .update_head(&workspace.id, &result.commit_id)
        .await?;

    audit_workspace_event(
        &state,
        &auth,
        &repo_id,
        &workspace.id,
        "file_deleted",
        Some(serde_json::json!({
            "path": path,
            "commit_sha": result.commit_id,
            "workspace_id": workspace.id,
        })),
        Some(&result.commit_id),
    )
    .await;

    Ok(Json(ApiResponse::ok(FileWriteResponse {
        commit_sha: result.commit_id,
        path,
    })))
}

pub async fn sync_workspace_integration(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, workspace_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<SyncIntegrationResponse>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let repo = find_repo(&state, &repo_id).await?;
    let workspace = find_workspace_for_owner(&state, &workspace_id, &auth.user_id).await?;
    if workspace.repo_id != repo_id {
        return Err(ConmanError::Forbidden {
            message: "workspace does not belong to repo".to_string(),
        }
        .into());
    }

    let git_repo = git_repo(&repo);
    let user = git_user(&auth);
    let status = match state
        .git_adapter
        .rebase_to_ref(
            &git_repo,
            &user,
            &workspace.head_sha,
            &format!("refs/heads/{}", repo.integration_branch),
            &format!("refs/heads/{}", workspace.branch_name),
        )
        .await
    {
        Ok(head_sha) => {
            let workspace_repo = conman_db::WorkspaceRepo::new(state.db.clone());
            workspace_repo.update_head(&workspace.id, &head_sha).await?;
            if workspace.base_sha.trim().is_empty() {
                workspace_repo
                    .update_base_sha(&workspace.id, &head_sha)
                    .await?;
            }
            ConflictStatus {
                clean: true,
                head_sha,
                conflicting_paths: Vec::new(),
                message: "workspace rebased onto integration branch".to_string(),
            }
        }
        Err(ConmanError::Git { message }) if message.contains("conflict") => ConflictStatus {
            clean: false,
            head_sha: workspace.head_sha.clone(),
            conflicting_paths: Vec::new(),
            message,
        },
        Err(err) => return Err(err.into()),
    };

    audit_workspace_event(
        &state,
        &auth,
        &repo_id,
        &workspace.id,
        "synced_integration",
        Some(serde_json::json!({
            "clean": status.clean,
            "head_sha": status.head_sha,
            "conflicting_paths": status.conflicting_paths,
            "message": status.message,
        })),
        Some(&status.head_sha),
    )
    .await;

    Ok(Json(ApiResponse::ok(SyncIntegrationResponse {
        clean: status.clean,
        head_sha: status.head_sha,
        conflicting_paths: status.conflicting_paths,
        message: status.message,
    })))
}

pub async fn reset_workspace(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, workspace_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<ResetResponse>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let repo = find_repo(&state, &repo_id).await?;
    let workspace = find_workspace_for_owner(&state, &workspace_id, &auth.user_id).await?;
    if workspace.repo_id != repo_id {
        return Err(ConmanError::Forbidden {
            message: "workspace does not belong to repo".to_string(),
        }
        .into());
    }

    let repo = git_repo(&repo);
    let user = git_user(&auth);
    let base_ref = workspace.base_ref_value.clone();
    let head_sha = match state
        .git_adapter
        .create_branch(&repo, &user, &workspace.branch_name, &base_ref)
        .await
    {
        Ok(branch) => branch.commit.id,
        Err(err) if is_unimplemented_git(&err) => base_ref.clone(),
        Err(err) => return Err(err.into()),
    };

    let workspace_repo = conman_db::WorkspaceRepo::new(state.db.clone());
    workspace_repo.update_head(&workspace.id, &head_sha).await?;
    if workspace.base_sha.trim().is_empty() {
        workspace_repo
            .update_base_sha(&workspace.id, &head_sha)
            .await?;
    }

    audit_workspace_event(
        &state,
        &auth,
        &repo_id,
        &workspace.id,
        "reset",
        Some(serde_json::json!({
            "head_sha": head_sha,
            "base_ref": base_ref,
        })),
        Some(&head_sha),
    )
    .await;

    Ok(Json(ApiResponse::ok(ResetResponse {
        head_sha,
        message: "workspace reset to baseline reference".to_string(),
    })))
}

pub async fn create_workspace_checkpoint(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, workspace_id)): Path<(String, String)>,
    Json(req): Json<CreateCheckpointRequest>,
) -> Result<Json<ApiResponse<CheckpointResponse>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let workspace = find_workspace_for_owner(&state, &workspace_id, &auth.user_id).await?;
    if workspace.repo_id != repo_id {
        return Err(ConmanError::Forbidden {
            message: "workspace does not belong to repo".to_string(),
        }
        .into());
    }

    let _message = req
        .message
        .unwrap_or_else(|| "workspace checkpoint".to_string());
    let strategy = find_repo(&state, &repo_id)
        .await?
        .settings
        .commit_mode_default;
    let detail = match strategy {
        CommitMode::SubmitCommit => {
            "checkpoint acknowledged; commits are created during submit mode".to_string()
        }
        CommitMode::ManualCheckpoint => {
            "checkpoint acknowledged; manual checkpoint strategy is enabled".to_string()
        }
    };

    audit_workspace_event(
        &state,
        &auth,
        &repo_id,
        &workspace.id,
        "checkpoint_created",
        Some(serde_json::json!({
            "commit_sha": workspace.head_sha,
            "message": detail.clone(),
        })),
        Some(&workspace.head_sha),
    )
    .await;

    Ok(Json(ApiResponse::ok(CheckpointResponse {
        commit_sha: workspace.head_sha,
        message: detail,
    })))
}
