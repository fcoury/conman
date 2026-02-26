use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use conman_auth::AuthUser;
use conman_core::{ConmanError, Job, JobType, ReleaseBatch, ReleaseState, Role};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::ApiConmanError, extractors::Pagination, response::ApiResponse, state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct SetReleaseChangesetsRequest {
    pub changeset_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct AssembleReleaseResponse {
    pub release: ReleaseBatch,
    pub job: Job,
}

#[derive(Debug, Serialize)]
pub struct PublishReleaseResponse {
    pub release: ReleaseBatch,
    pub released_changesets: Vec<String>,
}

pub async fn list_releases(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(app_id): Path<String>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<ApiResponse<Vec<ReleaseBatch>>>, ApiConmanError> {
    auth.require_role(&app_id, Role::User)?;
    let pagination = pagination.validate()?;
    let (rows, total) = conman_db::ReleaseRepo::new(state.db.clone())
        .list_by_app(&app_id, pagination.skip(), pagination.limit)
        .await?;
    Ok(Json(ApiResponse::paginated(
        rows,
        pagination.page,
        pagination.limit,
        total,
    )))
}

pub async fn create_release(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(app_id): Path<String>,
) -> Result<Json<ApiResponse<ReleaseBatch>>, ApiConmanError> {
    auth.require_role(&app_id, Role::ConfigManager)?;
    let repo = conman_db::ReleaseRepo::new(state.db.clone());
    let tag = repo.next_tag(&app_id).await?;
    let release = repo.create_draft(&app_id, tag).await?;
    Ok(Json(ApiResponse::ok(release)))
}

pub async fn get_release(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, release_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<ReleaseBatch>>, ApiConmanError> {
    auth.require_role(&app_id, Role::User)?;
    let release = conman_db::ReleaseRepo::new(state.db.clone())
        .find_by_id(&release_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "release",
            id: release_id.clone(),
        })?;
    if release.app_id != app_id {
        return Err(ConmanError::Forbidden {
            message: "release does not belong to app".to_string(),
        }
        .into());
    }
    Ok(Json(ApiResponse::ok(release)))
}

pub async fn set_release_changesets(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, release_id)): Path<(String, String)>,
    Json(req): Json<SetReleaseChangesetsRequest>,
) -> Result<Json<ApiResponse<ReleaseBatch>>, ApiConmanError> {
    auth.require_role(&app_id, Role::ConfigManager)?;

    let queued = conman_db::ChangesetRepo::new(state.db.clone())
        .list_queued_by_app(&app_id)
        .await?;
    let queued_ids = queued.into_iter().map(|c| c.id).collect::<Vec<_>>();
    for id in &req.changeset_ids {
        if !queued_ids.contains(id) {
            return Err(ConmanError::Conflict {
                message: format!("changeset {id} is not queued for this app"),
            }
            .into());
        }
    }

    let release = conman_db::ReleaseRepo::new(state.db.clone())
        .set_changesets(&release_id, &req.changeset_ids)
        .await?;
    Ok(Json(ApiResponse::ok(release)))
}

pub async fn reorder_release_changesets(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, release_id)): Path<(String, String)>,
    Json(req): Json<SetReleaseChangesetsRequest>,
) -> Result<Json<ApiResponse<ReleaseBatch>>, ApiConmanError> {
    set_release_changesets(
        State(state),
        Extension(auth),
        Path((app_id, release_id)),
        Json(req),
    )
    .await
}

pub async fn assemble_release(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, release_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<AssembleReleaseResponse>>, ApiConmanError> {
    auth.require_role(&app_id, Role::ConfigManager)?;
    let release_repo = conman_db::ReleaseRepo::new(state.db.clone());
    let release =
        release_repo
            .find_by_id(&release_id)
            .await?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "release",
                id: release_id.clone(),
            })?;
    if release.state != ReleaseState::DraftRelease {
        return Err(ConmanError::Conflict {
            message: "release can only be assembled from draft_release".to_string(),
        }
        .into());
    }
    if release.ordered_changeset_ids.is_empty() {
        return Err(ConmanError::Validation {
            message: "release must include at least one queued changeset".to_string(),
        }
        .into());
    }

    let job = conman_db::JobRepo::new(state.db.clone())
        .enqueue(conman_db::EnqueueJobInput {
            app_id: app_id.clone(),
            job_type: JobType::ReleaseAssemble,
            entity_type: "release".to_string(),
            entity_id: release_id.clone(),
            payload: serde_json::json!({"release_id": release_id, "changeset_ids": release.ordered_changeset_ids}),
            max_retries: 1,
            timeout_ms: 20 * 60 * 1000,
            created_by: Some(auth.user_id),
        })
        .await?;

    release_repo
        .set_state(&release_id, ReleaseState::Assembling)
        .await?;
    let release = release_repo.set_compose_job(&release_id, &job.id).await?;
    let release = release_repo
        .set_state(&release.id, ReleaseState::Validated)
        .await?;

    Ok(Json(ApiResponse::ok(AssembleReleaseResponse {
        release,
        job,
    })))
}

pub async fn publish_release(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, release_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<PublishReleaseResponse>>, ApiConmanError> {
    auth.require_role(&app_id, Role::ConfigManager)?;
    let repo = conman_db::ReleaseRepo::new(state.db.clone());
    let release = repo
        .find_by_id(&release_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "release",
            id: release_id.clone(),
        })?;
    if !matches!(
        release.state,
        ReleaseState::Validated | ReleaseState::DraftRelease
    ) {
        return Err(ConmanError::Conflict {
            message: "release can only be published from validated or draft_release".to_string(),
        }
        .into());
    }
    let published_sha = Uuid::now_v7().to_string();
    let release = repo
        .publish(&release_id, published_sha, &auth.user_id)
        .await?;
    conman_db::ChangesetRepo::new(state.db.clone())
        .mark_released_batch(&release.ordered_changeset_ids)
        .await?;
    Ok(Json(ApiResponse::ok(PublishReleaseResponse {
        released_changesets: release.ordered_changeset_ids.clone(),
        release,
    })))
}
