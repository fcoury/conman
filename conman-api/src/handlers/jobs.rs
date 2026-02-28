use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use conman_auth::AuthUser;
use conman_core::{ConmanError, Job, JobLogLine, Role};
use serde::Serialize;

use crate::{
    error::ApiConmanError, extractors::Pagination, response::ApiResponse, state::AppState,
};

#[derive(Debug, Serialize)]
pub struct JobDetailResponse {
    pub job: Job,
    pub logs: Vec<JobLogLine>,
}

pub async fn list_jobs(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(repo_id): Path<String>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<ApiResponse<Vec<Job>>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let pagination = pagination.validate()?;
    let (jobs, total) = conman_db::JobRepo::new(state.db.clone())
        .list_by_repo(&repo_id, pagination.skip(), pagination.limit)
        .await?;
    Ok(Json(ApiResponse::paginated(
        jobs,
        pagination.page,
        pagination.limit,
        total,
    )))
}

pub async fn get_job(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, job_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<JobDetailResponse>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let repo = conman_db::JobRepo::new(state.db.clone());
    let job = repo
        .get(&job_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "job",
            id: job_id.clone(),
        })?;
    if job.repo_id != repo_id {
        return Err(ConmanError::Forbidden {
            message: "job does not belong to app".to_string(),
        }
        .into());
    }
    let logs = repo.list_logs(&job_id).await?;
    Ok(Json(ApiResponse::ok(JobDetailResponse { job, logs })))
}
