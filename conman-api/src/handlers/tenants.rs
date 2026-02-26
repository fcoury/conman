use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use conman_auth::AuthUser;
use conman_core::{App, AppSurface, ConmanError, Role, SurfaceBranding, Tenant};
use serde::Deserialize;

use crate::{
    error::ApiConmanError, events::emit_audit, extractors::Pagination, response::ApiResponse,
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct CreateTenantRequest {
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateRepoRequest {
    pub name: String,
    pub repo_path: String,
    pub integration_branch: Option<String>,
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
            message: "surface key is required".to_string(),
        }
        .into());
    }
    if !key
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
    {
        return Err(ConmanError::Validation {
            message: "surface key must contain only lowercase letters, numbers, '-' or '_'"
                .to_string(),
        }
        .into());
    }
    Ok(())
}

pub async fn list_tenants(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<ApiResponse<Vec<Tenant>>>, ApiConmanError> {
    let pagination = pagination.validate()?;
    let repo = conman_db::TenantRepo::new(state.db.clone());
    let (items, total) = repo
        .list(pagination.skip(), pagination.limit)
        .await
        .map_err(ApiConmanError)?;
    Ok(Json(ApiResponse::paginated(
        items,
        pagination.page,
        pagination.limit,
        total,
    )))
}

pub async fn create_tenant(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(req): Json<CreateTenantRequest>,
) -> Result<Json<ApiResponse<Tenant>>, ApiConmanError> {
    if req.name.trim().is_empty() {
        return Err(ConmanError::Validation {
            message: "name is required".to_string(),
        }
        .into());
    }
    let slug = req.slug.trim().to_string();
    validate_slug(&slug)?;

    let tenant = conman_db::TenantRepo::new(state.db.clone())
        .create(req.name.trim(), &slug)
        .await?;

    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        None,
        "tenant",
        &tenant.id,
        "created",
        None,
        serde_json::to_value(&tenant).ok(),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }

    Ok(Json(ApiResponse::ok(tenant)))
}

pub async fn get_tenant(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(tenant_id): Path<String>,
) -> Result<Json<ApiResponse<Tenant>>, ApiConmanError> {
    let tenant = conman_db::TenantRepo::new(state.db.clone())
        .find_by_id(&tenant_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "tenant",
            id: tenant_id.clone(),
        })?;
    Ok(Json(ApiResponse::ok(tenant)))
}

pub async fn create_repo_under_tenant(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(tenant_id): Path<String>,
    Json(req): Json<CreateRepoRequest>,
) -> Result<Json<ApiResponse<App>>, ApiConmanError> {
    if req.name.trim().is_empty() || req.repo_path.trim().is_empty() {
        return Err(ConmanError::Validation {
            message: "name and repo_path are required".to_string(),
        }
        .into());
    }
    conman_db::TenantRepo::new(state.db.clone())
        .find_by_id(&tenant_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "tenant",
            id: tenant_id.clone(),
        })?;

    let integration_branch = req
        .integration_branch
        .unwrap_or_else(|| "main".to_string())
        .trim()
        .to_string();
    let app_repo = conman_db::AppRepo::new(state.db.clone());
    let membership_repo = conman_db::MembershipRepo::new(state.db.clone());
    let app = app_repo
        .insert_for_tenant(
            &tenant_id,
            req.name.trim(),
            req.repo_path.trim(),
            &integration_branch,
            &auth.user_id,
        )
        .await?;
    membership_repo
        .assign_role(&auth.user_id, &app.id, Role::AppAdmin)
        .await?;

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

pub async fn list_repo_surfaces(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(repo_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<AppSurface>>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::User)?;
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
    auth.require_role(&repo_id, Role::AppAdmin)?;
    validate_surface_key(req.key.trim())?;
    if req.title.trim().is_empty() {
        return Err(ConmanError::Validation {
            message: "surface title is required".to_string(),
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
        "app_surface",
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
    auth.require_role(&repo_id, Role::AppAdmin)?;

    let repo = conman_db::AppSurfaceRepo::new(state.db.clone());
    let existing = repo
        .find_by_id(&surface_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "app_surface",
            id: surface_id.clone(),
        })?;
    if existing.repo_id != repo_id {
        return Err(ConmanError::Forbidden {
            message: "surface does not belong to the specified repo".to_string(),
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
        "app_surface",
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
