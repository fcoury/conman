use std::collections::HashSet;

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use conman_auth::AuthUser;
use conman_core::{ConmanError, Deployment, Job, JobType, Role, RollbackMode};
use serde::{Deserialize, Serialize};

use crate::{
    error::ApiConmanError, extractors::Pagination, response::ApiResponse, state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct DeployRequest {
    pub release_id: String,
    #[serde(default)]
    pub is_skip_stage: bool,
    #[serde(default)]
    pub is_concurrent_batch: bool,
    #[serde(default)]
    pub approvals: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct RollbackRequest {
    pub release_id: String,
    pub mode: RollbackMode,
    #[serde(default)]
    pub approvals: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DeploymentEnqueueResponse {
    pub deployment: Deployment,
    pub job: Job,
}

fn validate_exceptional_approvals(
    is_skip_stage: bool,
    is_concurrent_batch: bool,
    approvals: &[String],
) -> Result<(), ConmanError> {
    if is_skip_stage || is_concurrent_batch {
        let unique = approvals.iter().cloned().collect::<HashSet<_>>();
        if unique.len() < 2 {
            return Err(ConmanError::Validation {
                message: "skip-stage/concurrent deploy requires 2 distinct approvers".to_string(),
            });
        }
    }
    Ok(())
}

async fn enqueue_deploy_job(
    state: &AppState,
    app_id: &str,
    deployment_id: &str,
    payload: serde_json::Value,
    actor: &str,
) -> Result<Job, ConmanError> {
    conman_db::JobRepo::new(state.db.clone())
        .enqueue(conman_db::EnqueueJobInput {
            app_id: app_id.to_string(),
            job_type: JobType::DeployRelease,
            entity_type: "deployment".to_string(),
            entity_id: deployment_id.to_string(),
            payload,
            max_retries: 1,
            timeout_ms: 30 * 60 * 1000,
            created_by: Some(actor.to_string()),
        })
        .await
}

pub async fn deploy_environment(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, env_id)): Path<(String, String)>,
    Json(req): Json<DeployRequest>,
) -> Result<Json<ApiResponse<DeploymentEnqueueResponse>>, ApiConmanError> {
    auth.require_role(&app_id, Role::ConfigManager)?;
    validate_exceptional_approvals(req.is_skip_stage, req.is_concurrent_batch, &req.approvals)?;

    let deployment = conman_db::DeploymentRepo::new(state.db.clone())
        .create(conman_db::CreateDeploymentInput {
            app_id: app_id.clone(),
            environment_id: env_id.clone(),
            release_id: req.release_id.clone(),
            is_skip_stage: req.is_skip_stage,
            is_concurrent_batch: req.is_concurrent_batch,
            approvals: req.approvals.clone(),
            created_by: auth.user_id.clone(),
        })
        .await?;

    let job = enqueue_deploy_job(
        &state,
        &app_id,
        &deployment.id,
        serde_json::json!({
            "environment_id": env_id,
            "release_id": req.release_id,
            "deployment_id": deployment.id,
            "is_skip_stage": req.is_skip_stage,
            "is_concurrent_batch": req.is_concurrent_batch
        }),
        &auth.user_id,
    )
    .await?;

    Ok(Json(ApiResponse::ok(DeploymentEnqueueResponse {
        deployment,
        job,
    })))
}

pub async fn promote_environment(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, env_id)): Path<(String, String)>,
    Json(req): Json<DeployRequest>,
) -> Result<Json<ApiResponse<DeploymentEnqueueResponse>>, ApiConmanError> {
    deploy_environment(
        State(state),
        Extension(auth),
        Path((app_id, env_id)),
        Json(DeployRequest {
            release_id: req.release_id,
            is_skip_stage: req.is_skip_stage,
            is_concurrent_batch: req.is_concurrent_batch,
            approvals: req.approvals,
        }),
    )
    .await
}

pub async fn rollback_environment(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, env_id)): Path<(String, String)>,
    Json(req): Json<RollbackRequest>,
) -> Result<Json<ApiResponse<DeploymentEnqueueResponse>>, ApiConmanError> {
    auth.require_role(&app_id, Role::ConfigManager)?;
    validate_exceptional_approvals(true, false, &req.approvals)?;
    let deployment = conman_db::DeploymentRepo::new(state.db.clone())
        .create(conman_db::CreateDeploymentInput {
            app_id: app_id.clone(),
            environment_id: env_id.clone(),
            release_id: req.release_id.clone(),
            is_skip_stage: true,
            is_concurrent_batch: false,
            approvals: req.approvals.clone(),
            created_by: auth.user_id.clone(),
        })
        .await?;
    let job = enqueue_deploy_job(
        &state,
        &app_id,
        &deployment.id,
        serde_json::json!({
            "environment_id": env_id,
            "release_id": req.release_id,
            "deployment_id": deployment.id,
            "rollback_mode": req.mode,
        }),
        &auth.user_id,
    )
    .await?;
    Ok(Json(ApiResponse::ok(DeploymentEnqueueResponse {
        deployment,
        job,
    })))
}

pub async fn list_deployments(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(app_id): Path<String>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<ApiResponse<Vec<Deployment>>>, ApiConmanError> {
    auth.require_role(&app_id, Role::User)?;
    let pagination = pagination.validate()?;
    let (rows, total) = conman_db::DeploymentRepo::new(state.db.clone())
        .list_by_app(&app_id, pagination.skip(), pagination.limit)
        .await?;
    Ok(Json(ApiResponse::paginated(
        rows,
        pagination.page,
        pagination.limit,
        total,
    )))
}
