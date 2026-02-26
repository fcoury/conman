use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use chrono::{Duration, Utc};
use conman_auth::AuthUser;
use conman_core::{ConmanError, Job, JobType, Role, TempEnvKind, TempEnvState, TempEnvironment};
use serde::{Deserialize, Serialize};

use crate::{
    error::ApiConmanError, extractors::Pagination, response::ApiResponse, state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct CreateTempEnvRequest {
    pub kind: String,
    pub source_id: String,
    pub base_profile_id: Option<String>,
    pub runtime_profile_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExtendTempEnvRequest {
    pub seconds: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct TempEnvActionResponse {
    pub temp_env: TempEnvironment,
    pub job: Option<Job>,
}

fn parse_kind(value: &str) -> Result<TempEnvKind, ApiConmanError> {
    match value {
        "workspace" => Ok(TempEnvKind::Workspace),
        "changeset" => Ok(TempEnvKind::Changeset),
        _ => Err(ConmanError::Validation {
            message: "kind must be workspace or changeset".to_string(),
        }
        .into()),
    }
}

pub async fn list_temp_envs(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(app_id): Path<String>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<ApiResponse<Vec<TempEnvironment>>>, ApiConmanError> {
    auth.require_role(&app_id, Role::User)?;
    let pagination = pagination.validate()?;
    let (rows, total) = conman_db::TempEnvRepo::new(state.db.clone())
        .list_by_app(&app_id, pagination.skip(), pagination.limit)
        .await?;
    Ok(Json(ApiResponse::paginated(
        rows,
        pagination.page,
        pagination.limit,
        total,
    )))
}

pub async fn create_temp_env(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(app_id): Path<String>,
    Json(req): Json<CreateTempEnvRequest>,
) -> Result<Json<ApiResponse<TempEnvActionResponse>>, ApiConmanError> {
    auth.require_role(&app_id, Role::User)?;
    let kind = parse_kind(&req.kind)?;
    let temp_env = conman_db::TempEnvRepo::new(state.db.clone())
        .create(conman_db::CreateTempEnvInput {
            app_id: app_id.clone(),
            kind,
            source_id: req.source_id,
            owner_user_id: auth.user_id.clone(),
            base_profile_id: req.base_profile_id,
            runtime_profile_id: req.runtime_profile_id,
            url_domain: state.config.temp_url_domain.clone(),
        })
        .await?;
    let job = conman_db::JobRepo::new(state.db.clone())
        .enqueue(conman_db::EnqueueJobInput {
            app_id: app_id.clone(),
            job_type: JobType::TempEnvProvision,
            entity_type: "temp_environment".to_string(),
            entity_id: temp_env.id.clone(),
            payload: serde_json::json!({"temp_env_id": temp_env.id, "kind": kind}),
            max_retries: 1,
            timeout_ms: 15 * 60 * 1000,
            created_by: Some(auth.user_id),
        })
        .await?;
    Ok(Json(ApiResponse::ok(TempEnvActionResponse {
        temp_env,
        job: Some(job),
    })))
}

pub async fn extend_temp_env(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, temp_env_id)): Path<(String, String)>,
    Json(req): Json<ExtendTempEnvRequest>,
) -> Result<Json<ApiResponse<TempEnvActionResponse>>, ApiConmanError> {
    auth.require_role(&app_id, Role::User)?;
    let seconds = req.seconds.unwrap_or(24 * 3600).max(300);
    let temp_env = conman_db::TempEnvRepo::new(state.db.clone())
        .extend_ttl(&temp_env_id, seconds)
        .await?;
    Ok(Json(ApiResponse::ok(TempEnvActionResponse {
        temp_env,
        job: None,
    })))
}

pub async fn undo_expire_temp_env(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, temp_env_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<TempEnvActionResponse>>, ApiConmanError> {
    auth.require_role(&app_id, Role::User)?;
    let temp_env = conman_db::TempEnvRepo::new(state.db.clone())
        .set_state(&temp_env_id, TempEnvState::Active, None)
        .await?;
    Ok(Json(ApiResponse::ok(TempEnvActionResponse {
        temp_env,
        job: None,
    })))
}

pub async fn delete_temp_env(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, temp_env_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<TempEnvActionResponse>>, ApiConmanError> {
    auth.require_role(&app_id, Role::User)?;
    let grace = Utc::now() + Duration::hours(1);
    let temp_env = conman_db::TempEnvRepo::new(state.db.clone())
        .set_state(&temp_env_id, TempEnvState::Deleted, Some(grace))
        .await?;
    let job = conman_db::JobRepo::new(state.db.clone())
        .enqueue(conman_db::EnqueueJobInput {
            app_id,
            job_type: JobType::TempEnvExpire,
            entity_type: "temp_environment".to_string(),
            entity_id: temp_env_id,
            payload: serde_json::json!({}),
            max_retries: 1,
            timeout_ms: 10 * 60 * 1000,
            created_by: Some(auth.user_id),
        })
        .await?;
    Ok(Json(ApiResponse::ok(TempEnvActionResponse {
        temp_env,
        job: Some(job),
    })))
}
