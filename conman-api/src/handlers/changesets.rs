use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use conman_auth::AuthUser;
use conman_core::{Changeset, ChangesetState, ConmanError, EnvVarValue, Job, JobType, Role};
use conman_db::{ChangesetProfileOverride, OverrideInput, ReviewAction};
use serde::{Deserialize, Serialize};

use crate::{
    error::ApiConmanError,
    events::{emit_audit, emit_notification},
    extractors::Pagination,
    response::ApiResponse,
    state::AppState,
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
    role.satisfies(Role::Reviewer)
}

fn overrides_conflict(a: &ChangesetProfileOverride, b: &ChangesetProfileOverride) -> bool {
    a.key == b.key && a.target_profile_id == b.target_profile_id && a.value != b.value
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
    Path(repo_id): Path<String>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<ApiResponse<Vec<Changeset>>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let pagination = pagination.validate()?;
    let (rows, total) = conman_db::ChangesetRepo::new(state.db.clone())
        .list_by_repo(&repo_id, pagination.skip(), pagination.limit)
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
    Path(repo_id): Path<String>,
    Json(req): Json<CreateChangesetRequest>,
) -> Result<Json<ApiResponse<Changeset>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
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
    if workspace.repo_id != repo_id || workspace.owner_user_id != auth.user_id {
        return Err(ConmanError::Forbidden {
            message: "workspace does not belong to current user and app".to_string(),
        }
        .into());
    }

    let changeset = conman_db::ChangesetRepo::new(state.db.clone())
        .create(conman_db::CreateChangesetInput {
            repo_id,
            workspace_id: req.workspace_id,
            title: req.title,
            description: req.description,
            author_user_id: auth.user_id.clone(),
            head_sha: workspace.head_sha,
        })
        .await?;
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&changeset.repo_id),
        "changeset",
        &changeset.id,
        "created",
        None,
        serde_json::to_value(&changeset).ok(),
        Some(&changeset.head_sha),
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }
    Ok(Json(ApiResponse::ok(changeset)))
}

pub async fn get_changeset(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, changeset_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<ChangesetDetailResponse>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let changeset = find_changeset_or_404(&state, &changeset_id).await?;
    if changeset.repo_id != repo_id {
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
    Path((repo_id, changeset_id)): Path<(String, String)>,
    Json(req): Json<UpdateChangesetRequest>,
) -> Result<Json<ApiResponse<Changeset>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let changeset = find_changeset_or_404(&state, &changeset_id).await?;
    if changeset.repo_id != repo_id {
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
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&repo_id),
        "changeset",
        &row.id,
        "metadata_updated",
        serde_json::to_value(&changeset).ok(),
        serde_json::to_value(&row).ok(),
        row.submitted_head_sha.as_deref(),
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }
    Ok(Json(ApiResponse::ok(row)))
}

pub async fn submit_changeset(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, changeset_id)): Path<(String, String)>,
    Json(req): Json<SubmitRequest>,
) -> Result<Json<ApiResponse<SubmitResponse>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let changeset = find_changeset_or_404(&state, &changeset_id).await?;
    if changeset.repo_id != repo_id || changeset.author_user_id != auth.user_id {
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
            .replace_for_changeset(&repo_id, &changeset_id, &input)
            .await?;
    }
    let job = conman_db::JobRepo::new(state.db.clone())
        .enqueue(conman_db::EnqueueJobInput {
            repo_id: repo_id.clone(),
            job_type: JobType::MsuiteSubmit,
            entity_type: "changeset".to_string(),
            entity_id: changeset_id.clone(),
            payload: serde_json::json!({
                "gate": "submit",
                "repo_id": repo_id,
                "changeset_id": changeset_id,
            }),
            max_retries: 1,
            timeout_ms: 10 * 60 * 1000,
            created_by: Some(auth.user_id.clone()),
        })
        .await?;
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&repo_id),
        "changeset",
        &submitted.id,
        "submitted",
        None,
        serde_json::to_value(&submitted).ok(),
        submitted.submitted_head_sha.as_deref(),
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }
    if let Err(err) = emit_notification(
        &state,
        &auth.user_id,
        Some(&repo_id),
        "changeset_submitted",
        "Changeset submitted",
        &format!("Changeset {} was submitted for review.", submitted.title),
    )
    .await
    {
        tracing::warn!(error = %err, "failed to enqueue notification");
    }
    Ok(Json(ApiResponse::ok(SubmitResponse {
        changeset: submitted,
        job,
    })))
}

pub async fn resubmit_changeset(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, changeset_id)): Path<(String, String)>,
    Json(req): Json<SubmitRequest>,
) -> Result<Json<ApiResponse<SubmitResponse>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let changeset = find_changeset_or_404(&state, &changeset_id).await?;
    if changeset.repo_id != repo_id || changeset.author_user_id != auth.user_id {
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
            .replace_for_changeset(&repo_id, &changeset_id, &input)
            .await?;
    }
    let job = conman_db::JobRepo::new(state.db.clone())
        .enqueue(conman_db::EnqueueJobInput {
            repo_id: repo_id.clone(),
            job_type: JobType::MsuiteSubmit,
            entity_type: "changeset".to_string(),
            entity_id: changeset_id.clone(),
            payload: serde_json::json!({
                "gate": "resubmit",
                "repo_id": repo_id,
                "changeset_id": changeset_id,
            }),
            max_retries: 1,
            timeout_ms: 10 * 60 * 1000,
            created_by: Some(auth.user_id.clone()),
        })
        .await?;
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&repo_id),
        "changeset",
        &submitted.id,
        "resubmitted",
        None,
        serde_json::to_value(&submitted).ok(),
        submitted.submitted_head_sha.as_deref(),
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }
    Ok(Json(ApiResponse::ok(SubmitResponse {
        changeset: submitted,
        job,
    })))
}

pub async fn review_changeset(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, changeset_id)): Path<(String, String)>,
    Json(req): Json<ReviewRequest>,
) -> Result<Json<ApiResponse<Changeset>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Reviewer)?;
    let role = auth
        .role_for(&repo_id)
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
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&repo_id),
        "changeset",
        &reviewed.id,
        "reviewed",
        None,
        serde_json::to_value(&reviewed).ok(),
        reviewed.submitted_head_sha.as_deref(),
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }
    Ok(Json(ApiResponse::ok(reviewed)))
}

pub async fn queue_changeset(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, changeset_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Changeset>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::ConfigManager)?;
    let changeset_repo = conman_db::ChangesetRepo::new(state.db.clone());
    let overrides_repo = conman_db::ChangesetProfileOverrideRepo::new(state.db.clone());
    let changeset = find_changeset_or_404(&state, &changeset_id).await?;
    if changeset.state != ChangesetState::Approved {
        return Err(ConmanError::Conflict {
            message: "changeset must be approved before queueing".to_string(),
        }
        .into());
    }
    let submit_job = conman_db::JobRepo::new(state.db.clone())
        .latest_for_entity(&repo_id, "changeset", &changeset_id, JobType::MsuiteSubmit)
        .await?;
    match submit_job {
        Some(job) if job.state == conman_core::JobState::Succeeded => {}
        Some(job) => {
            return Err(ConmanError::Conflict {
                message: format!(
                    "changeset submit gate not satisfied; latest msuite_submit job is {:?}",
                    job.state
                ),
            }
            .into());
        }
        None => {
            return Err(ConmanError::Conflict {
                message: "changeset has not completed submit gate job".to_string(),
            }
            .into());
        }
    }
    let queue_position = changeset_repo.next_queue_position(&repo_id).await?;
    let queued = changeset_repo.queue(&changeset_id, queue_position).await?;
    let queued_overrides = overrides_repo.list_by_changeset(&queued.id).await?;
    let maybe_conflict_with = if queued_overrides.is_empty() {
        None
    } else {
        let mut conflict_with = None;
        for other in changeset_repo.list_queued_by_repo(&repo_id).await? {
            if other.id == queued.id {
                continue;
            }
            let other_overrides = overrides_repo.list_by_changeset(&other.id).await?;
            if queued_overrides.iter().any(|left| {
                other_overrides
                    .iter()
                    .any(|right| overrides_conflict(left, right))
            }) {
                conflict_with = Some(other.id);
                break;
            }
        }
        conflict_with
    };
    let queued = if let Some(conflict_with) = maybe_conflict_with {
        let conflicted = changeset_repo.mark_conflicted(&queued.id).await?;
        if let Err(err) = emit_audit(
            &state,
            Some(&auth.user_id),
            Some(&repo_id),
            "changeset",
            &conflicted.id,
            "auto_conflicted_override_collision",
            None,
            Some(serde_json::json!({
                "changeset_id": conflicted.id,
                "state": conflicted.state,
                "conflict_with_changeset_id": conflict_with,
            })),
            conflicted.submitted_head_sha.as_deref(),
        )
        .await
        {
            tracing::warn!(error = %err, "failed to write audit event");
        }
        conflicted
    } else {
        queued
    };
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&repo_id),
        "changeset",
        &queued.id,
        "queued",
        None,
        serde_json::to_value(&queued).ok(),
        queued.submitted_head_sha.as_deref(),
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }
    Ok(Json(ApiResponse::ok(queued)))
}

pub async fn move_changeset_to_draft(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, changeset_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Changeset>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let changeset = find_changeset_or_404(&state, &changeset_id).await?;
    let role = auth
        .role_for(&repo_id)
        .ok_or_else(|| ConmanError::Forbidden {
            message: "missing app role".to_string(),
        })?;
    let privileged = role.satisfies(Role::ConfigManager);
    if !privileged && changeset.author_user_id != auth.user_id {
        return Err(ConmanError::Forbidden {
            message: "only author or config manager/app admin can move to draft".to_string(),
        }
        .into());
    }
    let draft = conman_db::ChangesetRepo::new(state.db.clone())
        .move_to_draft(&changeset_id)
        .await?;
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&repo_id),
        "changeset",
        &draft.id,
        "moved_to_draft",
        None,
        serde_json::to_value(&draft).ok(),
        draft.submitted_head_sha.as_deref(),
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }
    Ok(Json(ApiResponse::ok(draft)))
}

pub async fn get_changeset_diff(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, changeset_id)): Path<(String, String)>,
    Query(query): Query<DiffQuery>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let app = conman_db::RepoStore::new(state.db.clone())
        .find_by_id(&repo_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "app",
            id: repo_id.clone(),
        })?;
    let changeset = find_changeset_or_404(&state, &changeset_id).await?;

    let git_repo = conman_core::GitRepo {
        storage_name: "default".to_string(),
        relative_path: app.repo_path,
        gl_repository: format!("project-{repo_id}"),
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
    Path((repo_id, changeset_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Vec<conman_core::ChangesetComment>>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let rows = conman_db::ChangesetCommentRepo::new(state.db.clone())
        .list_by_changeset(&changeset_id)
        .await?;
    Ok(Json(ApiResponse::ok(rows)))
}

pub async fn create_changeset_comment(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, changeset_id)): Path<(String, String)>,
    Json(req): Json<CreateCommentRequest>,
) -> Result<Json<ApiResponse<conman_core::ChangesetComment>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    if req.body.trim().is_empty() {
        return Err(ConmanError::Validation {
            message: "comment body is required".to_string(),
        }
        .into());
    }
    let row = conman_db::ChangesetCommentRepo::new(state.db.clone())
        .create(&repo_id, &changeset_id, &auth.user_id, &req.body)
        .await?;
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&repo_id),
        "changeset_comment",
        &row.id,
        "created",
        None,
        serde_json::to_value(&row).ok(),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }
    Ok(Json(ApiResponse::ok(row)))
}
