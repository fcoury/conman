use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use conman_auth::AuthUser;
use conman_core::{Changeset, ChangesetState, ConmanError, EnvVarValue, Job, JobType, Role};
use conman_db::{ChangesetProfileOverride, OverrideInput, ReviewAction};
use serde::{Deserialize, Serialize};

use crate::{
    error::ApiConmanError, extractors::Pagination, response::ApiResponse, state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct CreateChangesetRequest {
    pub workspace_id: String,
    pub title: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateChangesetRequest {
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReviewRequest {
    pub action: String,
}

#[derive(Debug, Deserialize)]
pub struct ProfileOverrideRequest {
    pub key: String,
    pub value: EnvVarValue,
    pub target_profile_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitRequest {
    pub profile_overrides: Option<Vec<ProfileOverrideRequest>>,
}

#[derive(Debug, Deserialize)]
pub struct DiffQuery {
    #[serde(default)]
    pub format: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateCommentRequest {
    pub body: String,
}

#[derive(Debug, Serialize)]
pub struct ChangesetDetailResponse {
    pub changeset: Changeset,
    pub profile_overrides: Vec<ChangesetProfileOverride>,
}

#[derive(Debug, Serialize)]
pub struct SubmitResponse {
    pub changeset: Changeset,
    pub job: Job,
}

fn parse_review_action(input: &str) -> Result<ReviewAction, ApiConmanError> {
    match input {
        "approve" => Ok(ReviewAction::Approve),
        "request_changes" => Ok(ReviewAction::RequestChanges),
        "reject" => Ok(ReviewAction::Reject),
        _ => Err(ConmanError::Validation {
            message: "review action must be approve, request_changes, or reject".to_string(),
        }
        .into()),
    }
}

fn is_reviewer(role: Role) -> bool {
    matches!(role, Role::Reviewer | Role::ConfigManager | Role::AppAdmin)
}

async fn find_changeset_or_404(
    state: &AppState,
    changeset_id: &str,
) -> Result<Changeset, ApiConmanError> {
    conman_db::ChangesetRepo::new(state.db.clone())
        .find_by_id(changeset_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "changeset",
            id: changeset_id.to_string(),
        })
        .map_err(Into::into)
}

pub async fn list_changesets(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(app_id): Path<String>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<ApiResponse<Vec<Changeset>>>, ApiConmanError> {
    auth.require_role(&app_id, Role::User)?;
    let pagination = pagination.validate()?;
    let (rows, total) = conman_db::ChangesetRepo::new(state.db.clone())
        .list_by_app(&app_id, pagination.skip(), pagination.limit)
        .await?;
    Ok(Json(ApiResponse::paginated(
        rows,
        pagination.page,
        pagination.limit,
        total,
    )))
}

pub async fn create_changeset(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(app_id): Path<String>,
    Json(req): Json<CreateChangesetRequest>,
) -> Result<Json<ApiResponse<Changeset>>, ApiConmanError> {
    auth.require_role(&app_id, Role::User)?;
    if req.title.trim().is_empty() {
        return Err(ConmanError::Validation {
            message: "title is required".to_string(),
        }
        .into());
    }

    let workspace = conman_db::WorkspaceRepo::new(state.db.clone())
        .find_by_id(&req.workspace_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "workspace",
            id: req.workspace_id.clone(),
        })?;
    if workspace.app_id != app_id || workspace.owner_user_id != auth.user_id {
        return Err(ConmanError::Forbidden {
            message: "workspace does not belong to current user and app".to_string(),
        }
        .into());
    }

    let changeset = conman_db::ChangesetRepo::new(state.db.clone())
        .create(conman_db::CreateChangesetInput {
            app_id,
            workspace_id: req.workspace_id,
            title: req.title,
            description: req.description,
            author_user_id: auth.user_id,
            head_sha: workspace.head_sha,
        })
        .await?;
    Ok(Json(ApiResponse::ok(changeset)))
}

pub async fn get_changeset(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, changeset_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<ChangesetDetailResponse>>, ApiConmanError> {
    auth.require_role(&app_id, Role::User)?;
    let changeset = find_changeset_or_404(&state, &changeset_id).await?;
    if changeset.app_id != app_id {
        return Err(ConmanError::Forbidden {
            message: "changeset does not belong to app".to_string(),
        }
        .into());
    }
    let overrides = conman_db::ChangesetProfileOverrideRepo::new(state.db.clone())
        .list_by_changeset(&changeset_id)
        .await?;
    Ok(Json(ApiResponse::ok(ChangesetDetailResponse {
        changeset,
        profile_overrides: overrides,
    })))
}

pub async fn update_changeset(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, changeset_id)): Path<(String, String)>,
    Json(req): Json<UpdateChangesetRequest>,
) -> Result<Json<ApiResponse<Changeset>>, ApiConmanError> {
    auth.require_role(&app_id, Role::User)?;
    let changeset = find_changeset_or_404(&state, &changeset_id).await?;
    if changeset.app_id != app_id {
        return Err(ConmanError::Forbidden {
            message: "changeset does not belong to app".to_string(),
        }
        .into());
    }
    if changeset.author_user_id != auth.user_id {
        return Err(ConmanError::Forbidden {
            message: "only the author can edit changeset metadata".to_string(),
        }
        .into());
    }
    let row = conman_db::ChangesetRepo::new(state.db.clone())
        .update_title_and_description(&changeset_id, req.title, req.description)
        .await?;
    Ok(Json(ApiResponse::ok(row)))
}

pub async fn submit_changeset(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, changeset_id)): Path<(String, String)>,
    Json(req): Json<SubmitRequest>,
) -> Result<Json<ApiResponse<SubmitResponse>>, ApiConmanError> {
    auth.require_role(&app_id, Role::User)?;
    let changeset = find_changeset_or_404(&state, &changeset_id).await?;
    if changeset.app_id != app_id || changeset.author_user_id != auth.user_id {
        return Err(ConmanError::Forbidden {
            message: "only the author can submit this changeset".to_string(),
        }
        .into());
    }
    let workspace = conman_db::WorkspaceRepo::new(state.db.clone())
        .find_by_id(&changeset.workspace_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "workspace",
            id: changeset.workspace_id.clone(),
        })?;

    let submitted = conman_db::ChangesetRepo::new(state.db.clone())
        .submit_or_resubmit(&changeset_id, &workspace.head_sha, false)
        .await?;
    if let Some(overrides) = req.profile_overrides {
        let input = overrides
            .into_iter()
            .map(|o| OverrideInput {
                key: o.key,
                value: o.value,
                target_profile_id: o.target_profile_id,
            })
            .collect::<Vec<_>>();
        conman_db::ChangesetProfileOverrideRepo::new(state.db.clone())
            .replace_for_changeset(&app_id, &changeset_id, &input)
            .await?;
    }
    let job = conman_db::JobRepo::new(state.db.clone())
        .enqueue(conman_db::EnqueueJobInput {
            app_id: app_id.clone(),
            job_type: JobType::MsuiteSubmit,
            entity_type: "changeset".to_string(),
            entity_id: changeset_id.clone(),
            payload: serde_json::json!({
                "gate": "submit",
                "app_id": app_id,
                "changeset_id": changeset_id,
            }),
            max_retries: 1,
            timeout_ms: 10 * 60 * 1000,
            created_by: Some(auth.user_id.clone()),
        })
        .await?;
    Ok(Json(ApiResponse::ok(SubmitResponse {
        changeset: submitted,
        job,
    })))
}

pub async fn resubmit_changeset(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, changeset_id)): Path<(String, String)>,
    Json(req): Json<SubmitRequest>,
) -> Result<Json<ApiResponse<SubmitResponse>>, ApiConmanError> {
    auth.require_role(&app_id, Role::User)?;
    let changeset = find_changeset_or_404(&state, &changeset_id).await?;
    if changeset.app_id != app_id || changeset.author_user_id != auth.user_id {
        return Err(ConmanError::Forbidden {
            message: "only the author can resubmit this changeset".to_string(),
        }
        .into());
    }
    let workspace = conman_db::WorkspaceRepo::new(state.db.clone())
        .find_by_id(&changeset.workspace_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "workspace",
            id: changeset.workspace_id.clone(),
        })?;
    let submitted = conman_db::ChangesetRepo::new(state.db.clone())
        .submit_or_resubmit(&changeset_id, &workspace.head_sha, true)
        .await?;
    if let Some(overrides) = req.profile_overrides {
        let input = overrides
            .into_iter()
            .map(|o| OverrideInput {
                key: o.key,
                value: o.value,
                target_profile_id: o.target_profile_id,
            })
            .collect::<Vec<_>>();
        conman_db::ChangesetProfileOverrideRepo::new(state.db.clone())
            .replace_for_changeset(&app_id, &changeset_id, &input)
            .await?;
    }
    let job = conman_db::JobRepo::new(state.db.clone())
        .enqueue(conman_db::EnqueueJobInput {
            app_id: app_id.clone(),
            job_type: JobType::MsuiteSubmit,
            entity_type: "changeset".to_string(),
            entity_id: changeset_id.clone(),
            payload: serde_json::json!({
                "gate": "resubmit",
                "app_id": app_id,
                "changeset_id": changeset_id,
            }),
            max_retries: 1,
            timeout_ms: 10 * 60 * 1000,
            created_by: Some(auth.user_id.clone()),
        })
        .await?;
    Ok(Json(ApiResponse::ok(SubmitResponse {
        changeset: submitted,
        job,
    })))
}

pub async fn review_changeset(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, changeset_id)): Path<(String, String)>,
    Json(req): Json<ReviewRequest>,
) -> Result<Json<ApiResponse<Changeset>>, ApiConmanError> {
    auth.require_role(&app_id, Role::Reviewer)?;
    let role = auth
        .role_for(&app_id)
        .ok_or_else(|| ConmanError::Forbidden {
            message: "missing app role".to_string(),
        })?;
    if !is_reviewer(role) {
        return Err(ConmanError::Forbidden {
            message: "review capability required".to_string(),
        }
        .into());
    }
    let action = parse_review_action(&req.action)?;
    let reviewed = conman_db::ChangesetRepo::new(state.db.clone())
        .review(&changeset_id, &auth.user_id, role, action)
        .await?;
    Ok(Json(ApiResponse::ok(reviewed)))
}

pub async fn queue_changeset(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, changeset_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Changeset>>, ApiConmanError> {
    auth.require_role(&app_id, Role::ConfigManager)?;
    let changeset = find_changeset_or_404(&state, &changeset_id).await?;
    if changeset.state != ChangesetState::Approved {
        return Err(ConmanError::Conflict {
            message: "changeset must be approved before queueing".to_string(),
        }
        .into());
    }
    let queue_position = conman_db::ChangesetRepo::new(state.db.clone())
        .next_queue_position(&app_id)
        .await?;
    let queued = conman_db::ChangesetRepo::new(state.db.clone())
        .queue(&changeset_id, queue_position)
        .await?;
    Ok(Json(ApiResponse::ok(queued)))
}

pub async fn move_changeset_to_draft(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, changeset_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Changeset>>, ApiConmanError> {
    auth.require_role(&app_id, Role::User)?;
    let changeset = find_changeset_or_404(&state, &changeset_id).await?;
    let role = auth
        .role_for(&app_id)
        .ok_or_else(|| ConmanError::Forbidden {
            message: "missing app role".to_string(),
        })?;
    let privileged = matches!(role, Role::ConfigManager | Role::AppAdmin);
    if !privileged && changeset.author_user_id != auth.user_id {
        return Err(ConmanError::Forbidden {
            message: "only author or config manager/app admin can move to draft".to_string(),
        }
        .into());
    }
    let draft = conman_db::ChangesetRepo::new(state.db.clone())
        .move_to_draft(&changeset_id)
        .await?;
    Ok(Json(ApiResponse::ok(draft)))
}

pub async fn get_changeset_diff(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, changeset_id)): Path<(String, String)>,
    Query(query): Query<DiffQuery>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiConmanError> {
    auth.require_role(&app_id, Role::User)?;
    let app = conman_db::AppRepo::new(state.db.clone())
        .find_by_id(&app_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "app",
            id: app_id.clone(),
        })?;
    let changeset = find_changeset_or_404(&state, &changeset_id).await?;

    let git_repo = conman_core::GitRepo {
        storage_name: "default".to_string(),
        relative_path: app.repo_path,
        gl_repository: format!("project-{app_id}"),
    };
    let left = app.integration_branch;
    let right = changeset.head_sha;

    let format = if query.format.is_empty() {
        "semantic"
    } else {
        query.format.as_str()
    };
    let value = match format {
        "raw" => {
            let bytes = state.git_adapter.raw_diff(&git_repo, &left, &right).await?;
            serde_json::json!({ "format": "raw", "content": String::from_utf8_lossy(&bytes) })
        }
        "semantic" => {
            let stats = state
                .git_adapter
                .diff_stats(&git_repo, &left, &right)
                .await?;
            serde_json::to_value(stats).map_err(|e| ConmanError::Internal {
                message: format!("failed to serialize semantic diff: {e}"),
            })?
        }
        _ => {
            return Err(ConmanError::Validation {
                message: "format must be raw or semantic".to_string(),
            }
            .into());
        }
    };

    Ok(Json(ApiResponse::ok(value)))
}

pub async fn list_changeset_comments(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, changeset_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Vec<conman_core::ChangesetComment>>>, ApiConmanError> {
    auth.require_role(&app_id, Role::User)?;
    let rows = conman_db::ChangesetCommentRepo::new(state.db.clone())
        .list_by_changeset(&changeset_id)
        .await?;
    Ok(Json(ApiResponse::ok(rows)))
}

pub async fn create_changeset_comment(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, changeset_id)): Path<(String, String)>,
    Json(req): Json<CreateCommentRequest>,
) -> Result<Json<ApiResponse<conman_core::ChangesetComment>>, ApiConmanError> {
    auth.require_role(&app_id, Role::User)?;
    if req.body.trim().is_empty() {
        return Err(ConmanError::Validation {
            message: "comment body is required".to_string(),
        }
        .into());
    }
    let row = conman_db::ChangesetCommentRepo::new(state.db.clone())
        .create(&app_id, &changeset_id, &auth.user_id, &req.body)
        .await?;
    Ok(Json(ApiResponse::ok(row)))
}
