use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use conman_auth::AuthUser;
use conman_core::{
    Repo, BaseRefType, CommitMode, ConflictStatus, ConmanError, FileAction, FileEntry,
    FileEntryType, GitRepo, GitTreeEntryType, GitUser, Role, Workspace,
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
    let workspace_repo = conman_db::WorkspaceRepo::new(state.db.clone());
    if let Some(existing) = workspace_repo.find_default(&repo.id, &auth.user_id).await? {
        return Ok(existing);
    }

    let branch_name = default_workspace_branch(auth, repo);
    let git_repo = git_repo(repo);
    let git_user = git_user(auth);
    let mut head_sha = repo.integration_branch.clone();

    match state
        .git_adapter
        .create_branch(&git_repo, &git_user, &branch_name, &repo.integration_branch)
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
            title: None,
            is_default: true,
            base_ref_type: BaseRefType::Branch,
            base_ref_value: repo.integration_branch.clone(),
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

    let provided_branch = req.branch_name;
    let is_default = provided_branch.is_none();
    let branch_name = provided_branch.unwrap_or_else(|| default_workspace_branch(&auth, &repo));

    if is_default
        && conman_db::WorkspaceRepo::new(state.db.clone())
            .find_default(&repo_id, &auth.user_id)
            .await?
            .is_some()
    {
        return Err(ConmanError::Conflict {
            message: "default workspace already exists for this repo".to_string(),
        }
        .into());
    }

    let git_repo = git_repo(&repo);
    let git_user = git_user(&auth);
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

    let workspace = conman_db::WorkspaceRepo::new(state.db.clone())
        .create(CreateWorkspaceInput {
            repo_id,
            owner_user_id: auth.user_id.clone(),
            branch_name,
            title: req.title,
            is_default,
            base_ref_type: BaseRefType::Branch,
            base_ref_value: repo.integration_branch,
            head_sha,
        })
        .await?;

    audit_workspace_event(
        &state,
        &auth,
        &workspace.repo_id,
        &workspace.id,
        "created",
        serde_json::to_value(&workspace).ok(),
        Some(&workspace.head_sha),
    )
    .await;

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
            Some(&repo.integration_branch),
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
    let result = state
        .git_adapter
        .commit_files(
            &git_repo,
            &git_user,
            &workspace.branch_name,
            Some(&repo.integration_branch),
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
            conman_db::WorkspaceRepo::new(state.db.clone())
                .update_head(&workspace.id, &head_sha)
                .await?;
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

    conman_db::WorkspaceRepo::new(state.db.clone())
        .update_head(&workspace.id, &head_sha)
        .await?;

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
