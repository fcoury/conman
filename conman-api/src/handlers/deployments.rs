use std::collections::HashSet;

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use conman_auth::AuthUser;
use conman_core::{
    ConmanError, Deployment, Job, JobState, JobType, ReleaseState, Role, RollbackMode,
};
use metrics::counter;
use serde::{Deserialize, Serialize};

use crate::{
    error::ApiConmanError,
    events::{emit_audit, emit_notification},
    extractors::Pagination,
    response::ApiResponse,
    state::AppState,
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

const DEPLOYMENTS_TOTAL: &str = "conman_deployments_total";

fn is_exceptional_deploy(is_skip_stage: bool, is_concurrent_batch: bool) -> bool {
    is_skip_stage || is_concurrent_batch
}

fn role_can_approve_exceptional(role: Role) -> bool {
    role.satisfies(Role::Reviewer)
}

fn role_is_privileged_for_exceptional(role: Role) -> bool {
    role.satisfies(Role::ConfigManager)
}

async fn validate_exceptional_approvals(
    state: &AppState,
    repo_id: &str,
    is_skip_stage: bool,
    is_concurrent_batch: bool,
    approvals: &[String],
) -> Result<(), ConmanError> {
    if !is_exceptional_deploy(is_skip_stage, is_concurrent_batch) {
        return Ok(());
    }

    let unique = approvals.iter().cloned().collect::<HashSet<_>>();
    if unique.len() < 2 {
        return Err(ConmanError::Validation {
            message: "skip-stage/concurrent deploy requires 2 distinct approvers".to_string(),
        });
    }

    let repo_membership_repo = conman_db::RepoMembershipRepo::new(state.db.clone());
    let mut has_privileged_approver = false;
    for approver_user_id in unique {
        let roles = repo_membership_repo
            .find_roles_by_user_id(&approver_user_id)
            .await?;
        let role = roles
            .get(repo_id)
            .copied()
            .ok_or_else(|| ConmanError::Validation {
                message: format!("approver {approver_user_id} is not a member of app {repo_id}"),
            })?;
        if !role_can_approve_exceptional(role) {
            return Err(ConmanError::Validation {
                message: format!(
                    "approver {approver_user_id} must be reviewer/config_manager/admin/owner"
                ),
            });
        }
        has_privileged_approver |= role_is_privileged_for_exceptional(role);
    }
    if !has_privileged_approver {
        return Err(ConmanError::Validation {
            message: "skip-stage/concurrent deploy requires at least one config_manager/admin/owner approver"
                .to_string(),
        });
    }

    Ok(())
}

async fn enqueue_deploy_job(
    state: &AppState,
    repo_id: &str,
    deployment_id: &str,
    payload: serde_json::Value,
    actor: &str,
) -> Result<Job, ConmanError> {
    conman_db::JobRepo::new(state.db.clone())
        .enqueue(conman_db::EnqueueJobInput {
            repo_id: repo_id.to_string(),
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
    Path((repo_id, env_id)): Path<(String, String)>,
    Json(req): Json<DeployRequest>,
) -> Result<Json<ApiResponse<DeploymentEnqueueResponse>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::ConfigManager)?;
    counter!(DEPLOYMENTS_TOTAL, "action" => "deploy").increment(1);
    validate_exceptional_approvals(
        &state,
        &repo_id,
        req.is_skip_stage,
        req.is_concurrent_batch,
        &req.approvals,
    )
    .await?;
    let release = conman_db::ReleaseRepo::new(state.db.clone())
        .find_by_id(&req.release_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "release",
            id: req.release_id.clone(),
        })?;
    if release.repo_id != repo_id || release.state != ReleaseState::Published {
        return Err(ConmanError::Conflict {
            message: "release must be published before deployment".to_string(),
        }
        .into());
    }

    let job_repo = conman_db::JobRepo::new(state.db.clone());
    let drift_gate = job_repo
        .latest_for_entity(
            &repo_id,
            "environment",
            &env_id,
            JobType::RuntimeProfileDriftCheck,
        )
        .await?;
    match drift_gate {
        Some(job) if job.state == JobState::Succeeded => {}
        Some(job) if matches!(job.state, JobState::Queued | JobState::Running) => {
            return Err(ConmanError::Conflict {
                message: format!("drift check in progress for env {env_id} (job {})", job.id),
            }
            .into());
        }
        Some(_) => {
            return Err(ConmanError::Conflict {
                message: "drift check failed for target environment".to_string(),
            }
            .into());
        }
        None => {
            let job = job_repo
                .enqueue(conman_db::EnqueueJobInput {
                    repo_id: repo_id.clone(),
                    job_type: JobType::RuntimeProfileDriftCheck,
                    entity_type: "environment".to_string(),
                    entity_id: env_id.clone(),
                    payload: serde_json::json!({
                        "gate": "deploy_drift_check",
                        "environment_id": env_id,
                        "release_id": req.release_id,
                    }),
                    max_retries: 1,
                    timeout_ms: 10 * 60 * 1000,
                    created_by: Some(auth.user_id.clone()),
                })
                .await?;
            return Err(ConmanError::Conflict {
                message: format!(
                    "drift gate job enqueued ({}); retry deploy once succeeded",
                    job.id
                ),
            }
            .into());
        }
    }

    let deploy_gate = job_repo
        .latest_for_entity(
            &repo_id,
            "environment_release",
            &format!("{env_id}:{}", req.release_id),
            JobType::MsuiteDeploy,
        )
        .await?;
    match deploy_gate {
        Some(job) if job.state == JobState::Succeeded => {}
        Some(job) if matches!(job.state, JobState::Queued | JobState::Running) => {
            return Err(ConmanError::Conflict {
                message: format!("deploy gate in progress (job {})", job.id),
            }
            .into());
        }
        Some(_) => {
            return Err(ConmanError::Conflict {
                message: "deploy gate failed; fix validation before deployment".to_string(),
            }
            .into());
        }
        None => {
            let job = job_repo
                .enqueue(conman_db::EnqueueJobInput {
                    repo_id: repo_id.clone(),
                    job_type: JobType::MsuiteDeploy,
                    entity_type: "environment_release".to_string(),
                    entity_id: format!("{env_id}:{}", req.release_id),
                    payload: serde_json::json!({
                        "gate": "deploy",
                        "environment_id": env_id,
                        "release_id": req.release_id,
                    }),
                    max_retries: 1,
                    timeout_ms: 20 * 60 * 1000,
                    created_by: Some(auth.user_id.clone()),
                })
                .await?;
            return Err(ConmanError::Conflict {
                message: format!(
                    "deploy gate job enqueued ({}); retry deploy once succeeded",
                    job.id
                ),
            }
            .into());
        }
    }

    let deployment = conman_db::DeploymentRepo::new(state.db.clone())
        .create(conman_db::CreateDeploymentInput {
            repo_id: repo_id.clone(),
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
        &repo_id,
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
    let deployment = conman_db::DeploymentRepo::new(state.db.clone())
        .attach_job(&deployment.id, &job.id)
        .await?;
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&repo_id),
        "deployment",
        &deployment.id,
        "created",
        None,
        serde_json::to_value(&deployment).ok(),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }
    if let Err(err) = emit_notification(
        &state,
        &auth.user_id,
        Some(&repo_id),
        "deployment_started",
        "Deployment started",
        &format!(
            "Deployment {} to environment {} was started.",
            deployment.id, env_id
        ),
    )
    .await
    {
        tracing::warn!(error = %err, "failed to enqueue notification");
    }

    Ok(Json(ApiResponse::ok(DeploymentEnqueueResponse {
        deployment,
        job,
    })))
}

pub async fn promote_environment(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, env_id)): Path<(String, String)>,
    Json(req): Json<DeployRequest>,
) -> Result<Json<ApiResponse<DeploymentEnqueueResponse>>, ApiConmanError> {
    deploy_environment(
        State(state),
        Extension(auth),
        Path((repo_id, env_id)),
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
    Path((repo_id, env_id)): Path<(String, String)>,
    Json(req): Json<RollbackRequest>,
) -> Result<Json<ApiResponse<DeploymentEnqueueResponse>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::ConfigManager)?;
    counter!(DEPLOYMENTS_TOTAL, "action" => "rollback").increment(1);
    validate_exceptional_approvals(&state, &repo_id, true, false, &req.approvals).await?;
    let release = conman_db::ReleaseRepo::new(state.db.clone())
        .find_by_id(&req.release_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "release",
            id: req.release_id.clone(),
        })?;
    if release.repo_id != repo_id || release.state != ReleaseState::Published {
        return Err(ConmanError::Conflict {
            message: "rollback requires a published release".to_string(),
        }
        .into());
    }
    let deployment = conman_db::DeploymentRepo::new(state.db.clone())
        .create(conman_db::CreateDeploymentInput {
            repo_id: repo_id.clone(),
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
        &repo_id,
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
    let deployment = conman_db::DeploymentRepo::new(state.db.clone())
        .attach_job(&deployment.id, &job.id)
        .await?;
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&repo_id),
        "deployment",
        &deployment.id,
        "rollback_started",
        None,
        serde_json::to_value(&deployment).ok(),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }
    Ok(Json(ApiResponse::ok(DeploymentEnqueueResponse {
        deployment,
        job,
    })))
}

pub async fn list_deployments(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(repo_id): Path<String>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<ApiResponse<Vec<Deployment>>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let pagination = pagination.validate()?;
    let (rows, total) = conman_db::DeploymentRepo::new(state.db.clone())
        .list_by_repo(&repo_id, pagination.skip(), pagination.limit)
        .await?;
    Ok(Json(ApiResponse::paginated(
        rows,
        pagination.page,
        pagination.limit,
        total,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exceptional_deploy_detection() {
        assert!(!is_exceptional_deploy(false, false));
        assert!(is_exceptional_deploy(true, false));
        assert!(is_exceptional_deploy(false, true));
    }

    #[test]
    fn exceptional_role_rules() {
        assert!(!role_can_approve_exceptional(Role::Member));
        assert!(role_can_approve_exceptional(Role::Reviewer));
        assert!(role_can_approve_exceptional(Role::ConfigManager));
        assert!(role_can_approve_exceptional(Role::Admin));
        assert!(role_can_approve_exceptional(Role::Owner));

        assert!(!role_is_privileged_for_exceptional(Role::Reviewer));
        assert!(role_is_privileged_for_exceptional(Role::ConfigManager));
        assert!(role_is_privileged_for_exceptional(Role::Admin));
        assert!(role_is_privileged_for_exceptional(Role::Owner));
    }
}
