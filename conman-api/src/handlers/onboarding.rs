use axum::{
    Extension, Json,
    extract::State,
};
use conman_auth::{AuthUser, issue_token};
use conman_core::{ConmanError, Repo, Role, Team};
use serde::{Deserialize, Serialize};

use crate::{
    error::ApiConmanError, events::emit_audit, response::ApiResponse, state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct CreateInstanceRequest {
    pub team_id: Option<String>,
    pub instance_name: String,
    pub instance_slug: String,
    pub integration_branch: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateInstanceResponse {
    pub token: String,
    pub team: Team,
    pub repo: Repo,
    pub instance_slug: String,
}

fn validate_instance_slug(slug: &str) -> Result<(), ApiConmanError> {
    if slug.is_empty() {
        return Err(ConmanError::Validation {
            message: "instance_slug is required".to_string(),
        }
        .into());
    }
    if !slug
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
    {
        return Err(ConmanError::Validation {
            message: "instance_slug must contain only lowercase letters, numbers, and '-'"
                .to_string(),
        }
        .into());
    }
    Ok(())
}

pub async fn create_instance(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(req): Json<CreateInstanceRequest>,
) -> Result<Json<ApiResponse<CreateInstanceResponse>>, ApiConmanError> {
    let instance_name = req.instance_name.trim();
    let instance_slug = req.instance_slug.trim().to_ascii_lowercase();
    if instance_name.is_empty() {
        return Err(ConmanError::Validation {
            message: "instance_name is required".to_string(),
        }
        .into());
    }
    validate_instance_slug(&instance_slug)?;

    let team_membership_repo = conman_db::TeamMembershipRepo::new(state.db.clone());
    let team_id = if let Some(team_id) = req.team_id.as_deref().map(str::trim) {
        if team_id.is_empty() {
            return Err(ConmanError::Validation {
                message: "team_id cannot be empty".to_string(),
            }
            .into());
        }
        team_id.to_string()
    } else {
        team_membership_repo
            .list_team_ids_by_user(&auth.user_id)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| ConmanError::Forbidden {
                message: "current user is not a member of any team".to_string(),
            })?
    };

    let team_role = team_membership_repo
        .role_for_user(&auth.user_id, &team_id)
        .await?
        .ok_or_else(|| ConmanError::Forbidden {
            message: format!("requires team membership on team {team_id}"),
        })?;
    if !team_role.satisfies(Role::Admin) {
        return Err(ConmanError::Forbidden {
            message: format!("requires role admin on team {team_id}"),
        }
        .into());
    }

    let team = conman_db::TeamRepo::new(state.db.clone())
        .find_by_id(&team_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "team",
            id: team_id.clone(),
        })?;

    let integration_branch = req
        .integration_branch
        .unwrap_or_else(|| "main".to_string())
        .trim()
        .to_string();

    let repo_store = conman_db::RepoStore::new(state.db.clone());
    let repo_membership_repo = conman_db::RepoMembershipRepo::new(state.db.clone());
    let repo = match repo_store
        .insert_for_team(
            &team.id,
            instance_name,
            &instance_slug,
            &integration_branch,
            &auth.user_id,
        )
        .await
    {
        Ok(repo) => repo,
        Err(ConmanError::Conflict { message })
            if message.contains("repos_name_unique")
                || message.contains("dup key: { name:") =>
        {
            return Err(ConmanError::Conflict {
                message: "instance_name is already in use".to_string(),
            }
            .into());
        }
        Err(ConmanError::Conflict { message })
            if message.contains("repos_repo_path_unique")
                || message.contains("dup key: { repo_path:") =>
        {
            return Err(ConmanError::Conflict {
                message: "instance_slug is already in use".to_string(),
            }
            .into());
        }
        Err(err) => return Err(err.into()),
    };

    let team_members = team_membership_repo.list_by_team_id(&team.id).await?;
    if team_members.is_empty() {
        repo_membership_repo
            .assign_role(&auth.user_id, &repo.id, Role::Owner)
            .await?;
    } else {
        for member in team_members {
            repo_membership_repo
                .assign_role(&member.user_id, &repo.id, member.role)
                .await?;
        }
    }

    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&repo.id),
        "repo",
        &repo.id,
        "created",
        None,
        serde_json::to_value(&repo).ok(),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }

    let roles = repo_membership_repo.find_roles_by_user_id(&auth.user_id).await?;
    let token = issue_token(
        &auth.user_id,
        &auth.email,
        roles,
        &state.config.jwt_secret,
        state.config.jwt_expiry_hours,
    )?;

    Ok(Json(ApiResponse::ok(CreateInstanceResponse {
        token,
        team,
        repo,
        instance_slug,
    })))
}
