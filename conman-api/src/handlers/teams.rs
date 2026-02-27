use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use conman_auth::AuthUser;
use conman_core::{App, AppSurface, ConmanError, Invite, Role, SurfaceBranding, Team};
use serde::Deserialize;

use crate::{
    error::ApiConmanError, events::emit_audit, extractors::Pagination, response::ApiResponse,
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct CreateTeamRequest {
    pub name: String,
    pub slug: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRepoRequest {
    pub name: String,
    pub repo_path: String,
    pub integration_branch: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTeamInviteRequest {
    pub email: String,
    pub role: Role,
}

#[derive(Debug, Deserialize)]
pub struct CreateSurfaceRequest {
    pub key: String,
    pub title: String,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub branding: Option<SurfaceBranding>,
    #[serde(default)]
    pub roles: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateSurfaceRequest {
    pub title: Option<String>,
    pub domains: Option<Vec<String>>,
    pub branding: Option<Option<SurfaceBranding>>,
    pub roles: Option<Vec<String>>,
}

fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in input.chars() {
        let lc = ch.to_ascii_lowercase();
        if lc.is_ascii_alphanumeric() {
            out.push(lc);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn validate_slug(slug: &str) -> Result<(), ApiConmanError> {
    if slug.is_empty() {
        return Err(ConmanError::Validation {
            message: "slug is required".to_string(),
        }
        .into());
    }
    if !slug
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
    {
        return Err(ConmanError::Validation {
            message: "slug must contain only lowercase letters, numbers, '-' or '_'".to_string(),
        }
        .into());
    }
    Ok(())
}

fn validate_surface_key(key: &str) -> Result<(), ApiConmanError> {
    if key.is_empty() {
        return Err(ConmanError::Validation {
            message: "app key is required".to_string(),
        }
        .into());
    }
    if !key
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
    {
        return Err(ConmanError::Validation {
            message: "app key must contain only lowercase letters, numbers, '-' or '_'".to_string(),
        }
        .into());
    }
    Ok(())
}

async fn team_role_for(
    state: &AppState,
    user_id: &str,
    team_id: &str,
) -> Result<Option<Role>, ApiConmanError> {
    Ok(conman_db::TeamMembershipRepo::new(state.db.clone())
        .role_for_user(user_id, team_id)
        .await?)
}

pub async fn list_teams(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<ApiResponse<Vec<Team>>>, ApiConmanError> {
    let pagination = pagination.validate()?;

    let team_ids = conman_db::TeamMembershipRepo::new(state.db.clone())
        .list_team_ids_by_user(&auth.user_id)
        .await?;
    let mut teams = conman_db::TeamRepo::new(state.db.clone())
        .list_by_ids(&team_ids)
        .await?;
    teams.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    let total = teams.len() as u64;
    let start = pagination.skip() as usize;
    let end = (start + pagination.limit as usize).min(teams.len());
    let items = if start >= teams.len() {
        Vec::new()
    } else {
        teams[start..end].to_vec()
    };

    Ok(Json(ApiResponse::paginated(
        items,
        pagination.page,
        pagination.limit,
        total,
    )))
}

pub async fn create_team(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(req): Json<CreateTeamRequest>,
) -> Result<Json<ApiResponse<Team>>, ApiConmanError> {
    if req.name.trim().is_empty() {
        return Err(ConmanError::Validation {
            message: "name is required".to_string(),
        }
        .into());
    }

    let slug_base = req
        .slug
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| slugify(req.name.trim()));
    validate_slug(&slug_base)?;

    let team_repo = conman_db::TeamRepo::new(state.db.clone());
    let team_membership_repo = conman_db::TeamMembershipRepo::new(state.db.clone());

    let mut suffix = 0u32;
    let team = loop {
        let candidate = if suffix == 0 {
            slug_base.clone()
        } else {
            format!("{}-{}", slug_base, suffix)
        };
        match team_repo.create(req.name.trim(), &candidate).await {
            Ok(team) => break team,
            Err(ConmanError::Conflict { .. }) if suffix < 1000 => {
                suffix += 1;
            }
            Err(err) => return Err(err.into()),
        }
    };

    team_membership_repo
        .assign_role(&auth.user_id, &team.id, Role::Owner)
        .await?;

    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        None,
        "team",
        &team.id,
        "created",
        None,
        serde_json::to_value(&team).ok(),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }

    Ok(Json(ApiResponse::ok(team)))
}

pub async fn get_team(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(team_id): Path<String>,
) -> Result<Json<ApiResponse<Team>>, ApiConmanError> {
    if team_role_for(&state, &auth.user_id, &team_id)
        .await?
        .is_none()
    {
        return Err(ConmanError::Forbidden {
            message: format!("requires team membership on team {team_id}"),
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
    Ok(Json(ApiResponse::ok(team)))
}

pub async fn create_repo_under_team(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(team_id): Path<String>,
    Json(req): Json<CreateRepoRequest>,
) -> Result<Json<ApiResponse<App>>, ApiConmanError> {
    if req.name.trim().is_empty() || req.repo_path.trim().is_empty() {
        return Err(ConmanError::Validation {
            message: "name and repo_path are required".to_string(),
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

    let role = team_role_for(&state, &auth.user_id, &team_id)
        .await?
        .ok_or_else(|| ConmanError::Forbidden {
            message: format!("requires team membership on team {}", team.id),
        })?;
    if !role.satisfies(Role::Admin) {
        return Err(ConmanError::Forbidden {
            message: format!("requires role admin on team {}", team.id),
        }
        .into());
    }

    let integration_branch = req
        .integration_branch
        .unwrap_or_else(|| "main".to_string())
        .trim()
        .to_string();

    let app_repo = conman_db::AppRepo::new(state.db.clone());
    let membership_repo = conman_db::MembershipRepo::new(state.db.clone());
    let team_membership_repo = conman_db::TeamMembershipRepo::new(state.db.clone());

    let app = app_repo
        .insert_for_team(
            &team.id,
            req.name.trim(),
            req.repo_path.trim(),
            &integration_branch,
            &auth.user_id,
        )
        .await?;

    let team_members = team_membership_repo.list_by_team_id(&team.id).await?;
    if team_members.is_empty() {
        membership_repo
            .assign_role(&auth.user_id, &app.id, Role::Owner)
            .await?;
    } else {
        for team_member in team_members {
            membership_repo
                .assign_role(&team_member.user_id, &app.id, team_member.role)
                .await?;
        }
    }

    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&app.id),
        "repo",
        &app.id,
        "created",
        None,
        serde_json::to_value(&app).ok(),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }

    Ok(Json(ApiResponse::ok(app)))
}

pub async fn create_team_invite(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(team_id): Path<String>,
    Json(req): Json<CreateTeamInviteRequest>,
) -> Result<Json<ApiResponse<Invite>>, ApiConmanError> {
    if req.email.trim().is_empty() {
        return Err(ConmanError::Validation {
            message: "email is required".to_string(),
        }
        .into());
    }

    let role = team_role_for(&state, &auth.user_id, &team_id)
        .await?
        .ok_or_else(|| ConmanError::Forbidden {
            message: format!("requires team membership on team {team_id}"),
        })?;
    if !role.satisfies(Role::Admin) {
        return Err(ConmanError::Forbidden {
            message: format!("requires role admin on team {team_id}"),
        }
        .into());
    }

    let invite = conman_db::InviteRepo::new(state.db.clone())
        .create(
            &team_id,
            &req.email,
            req.role,
            &auth.user_id,
            state.config.invite_expiry_days,
        )
        .await?;

    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        None,
        "team_invite",
        &invite.id,
        "created",
        None,
        serde_json::to_value(&invite).ok(),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }

    Ok(Json(ApiResponse::ok(invite)))
}

pub async fn list_repo_surfaces(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(repo_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<AppSurface>>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let surfaces = conman_db::AppSurfaceRepo::new(state.db.clone())
        .list_by_repo(&repo_id)
        .await?;
    Ok(Json(ApiResponse::ok(surfaces)))
}

pub async fn create_repo_surface(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(repo_id): Path<String>,
    Json(req): Json<CreateSurfaceRequest>,
) -> Result<Json<ApiResponse<AppSurface>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Admin)?;
    validate_surface_key(req.key.trim())?;
    if req.title.trim().is_empty() {
        return Err(ConmanError::Validation {
            message: "app title is required".to_string(),
        }
        .into());
    }

    let surface = conman_db::AppSurfaceRepo::new(state.db.clone())
        .create(
            &repo_id,
            conman_db::CreateAppSurfaceInput {
                key: req.key.trim().to_string(),
                title: req.title.trim().to_string(),
                domains: req.domains,
                branding: req.branding,
                roles: req.roles,
            },
        )
        .await?;

    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&repo_id),
        "app",
        &surface.id,
        "created",
        None,
        serde_json::to_value(&surface).ok(),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }

    Ok(Json(ApiResponse::ok(surface)))
}

pub async fn update_repo_surface(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, surface_id)): Path<(String, String)>,
    Json(req): Json<UpdateSurfaceRequest>,
) -> Result<Json<ApiResponse<AppSurface>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Admin)?;

    let repo = conman_db::AppSurfaceRepo::new(state.db.clone());
    let existing = repo
        .find_by_id(&surface_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "app",
            id: surface_id.clone(),
        })?;
    if existing.repo_id != repo_id {
        return Err(ConmanError::Forbidden {
            message: "app does not belong to the specified repo".to_string(),
        }
        .into());
    }

    let updated = repo
        .update(
            &surface_id,
            conman_db::UpdateAppSurfaceInput {
                title: req.title.map(|v| v.trim().to_string()),
                domains: req.domains,
                branding: req.branding,
                roles: req.roles,
            },
        )
        .await?;

    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&repo_id),
        "app",
        &updated.id,
        "updated",
        serde_json::to_value(&existing).ok(),
        serde_json::to_value(&updated).ok(),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }

    Ok(Json(ApiResponse::ok(updated)))
}
