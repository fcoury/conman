use axum::{Extension, Json, extract::State};
use conman_auth::AuthUser;
use conman_core::{App, ConmanError, Repo, Role, Team, UiConfig};
use serde::{Deserialize, Serialize};

use crate::{error::ApiConmanError, events::emit_audit, response::ApiResponse, state::AppState};

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoContextStatus {
    Bound,
    Unbound,
}

#[derive(Debug, Serialize)]
pub struct RepoContextResponse {
    pub status: RepoContextStatus,
    pub binding: Option<UiConfig>,
    pub repo: Option<Repo>,
    pub team: Option<Team>,
    pub apps: Vec<App>,
    pub role: Option<Role>,
    pub can_rebind: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBoundRepoRequest {
    pub repo_id: String,
}

fn unbound_response() -> RepoContextResponse {
    RepoContextResponse {
        status: RepoContextStatus::Unbound,
        binding: None,
        repo: None,
        team: None,
        apps: Vec::new(),
        role: None,
        can_rebind: false,
    }
}

async fn auto_bind_first_accessible_repo(
    state: &AppState,
    auth: &AuthUser,
) -> Result<Option<RepoContextResponse>, ApiConmanError> {
    let first_repo_id = auth.roles.keys().next().cloned();
    let Some(repo_id) = first_repo_id else {
        return Ok(None);
    };

    let ui_repo = conman_db::UiConfigRepo::new(state.db.clone());
    let binding = ui_repo.set_for_user(&repo_id, &auth.user_id).await?;
    Ok(Some(bound_response(state, auth, binding).await?))
}

async fn bound_response(
    state: &AppState,
    auth: &AuthUser,
    binding: UiConfig,
) -> Result<RepoContextResponse, ApiConmanError> {
    let role = auth
        .role_for(&binding.repo_id)
        .ok_or_else(|| ConmanError::Forbidden {
            message: "current user does not have access to bound repo".to_string(),
        })?;

    let repo = conman_db::RepoStore::new(state.db.clone())
        .find_by_id(&binding.repo_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "repo",
            id: binding.repo_id.clone(),
        })?;

    let team = if let Some(team_id) = repo.team_id.as_deref() {
        conman_db::TeamRepo::new(state.db.clone())
            .find_by_id(team_id)
            .await?
    } else {
        None
    };

    let apps = conman_db::AppRepo::new(state.db.clone())
        .list_by_repo(&repo.id)
        .await?;

    Ok(RepoContextResponse {
        status: RepoContextStatus::Bound,
        binding: Some(binding),
        repo: Some(repo),
        team,
        apps,
        role: Some(role),
        can_rebind: role.satisfies(Role::Member),
    })
}

pub async fn get_bound_repo(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> Result<Json<ApiResponse<RepoContextResponse>>, ApiConmanError> {
    let ui_repo = conman_db::UiConfigRepo::new(state.db.clone());
    let binding = ui_repo.get_for_user(&auth.user_id).await?;
    let body = match binding {
        Some(binding) => match bound_response(&state, &auth, binding).await {
            Ok(body) => body,
            Err(ApiConmanError(ConmanError::Forbidden { .. })) => {
                if let Some(rebound) = auto_bind_first_accessible_repo(&state, &auth).await? {
                    rebound
                } else {
                    unbound_response()
                }
            }
            Err(err) => return Err(err),
        },
        None => {
            if let Some(rebound) = auto_bind_first_accessible_repo(&state, &auth).await? {
                rebound
            } else {
                unbound_response()
            }
        }
    };
    Ok(Json(ApiResponse::ok(body)))
}

pub async fn update_bound_repo(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(req): Json<UpdateBoundRepoRequest>,
) -> Result<Json<ApiResponse<RepoContextResponse>>, ApiConmanError> {
    let repo_id = req.repo_id.trim().to_string();
    if repo_id.is_empty() {
        return Err(ConmanError::Validation {
            message: "repo_id is required".to_string(),
        }
        .into());
    }

    auth.require_role(&repo_id, Role::Member)?;

    let repo_store = conman_db::RepoStore::new(state.db.clone());
    if repo_store.find_by_id(&repo_id).await?.is_none() {
        return Err(ConmanError::NotFound {
            entity: "repo",
            id: repo_id,
        }
        .into());
    }

    let ui_repo = conman_db::UiConfigRepo::new(state.db.clone());
    let before = ui_repo.get_for_user(&auth.user_id).await?;
    let binding = ui_repo.set_for_user(&repo_id, &auth.user_id).await?;
    let body = bound_response(&state, &auth, binding.clone()).await?;

    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&binding.repo_id),
        "ui_config",
        &binding.id,
        "bound_repo_updated",
        before.and_then(|value| serde_json::to_value(value).ok()),
        serde_json::to_value(&binding).ok(),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }

    Ok(Json(ApiResponse::ok(body)))
}
