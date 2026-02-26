use std::collections::BTreeMap;

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use conman_auth::{AuthUser, decrypt_secret};
use conman_core::{
    App, BaselineMode, CommitMode, ConmanError, EnvVarValue, Environment, Invite,
    ProfileApprovalPolicy, Role, RuntimeProfile, RuntimeProfileKind, mask_secret,
};
use conman_db::{EnvironmentInput, RuntimeProfileInput, RuntimeProfileUpdate};
use serde::{Deserialize, Serialize};

use crate::{
    error::ApiConmanError, events::emit_audit, extractors::Pagination, response::ApiResponse,
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct CreateAppRequest {
    pub name: String,
    pub repo_path: String,
    pub integration_branch: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAppSettingsRequest {
    pub baseline_mode: Option<String>,
    pub canonical_env_id: Option<String>,
    pub commit_mode_default: Option<String>,
    pub blocked_paths: Option<Vec<String>>,
    pub file_size_limit_bytes: Option<u64>,
    pub profile_approval_policy: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MemberResponse {
    pub user_id: String,
    pub app_id: String,
    pub role: Role,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub email: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AssignMemberRequest {
    pub user_id: String,
    pub role: Role,
}

#[derive(Debug, Deserialize)]
pub struct CreateInviteRequest {
    pub email: String,
    pub role: Role,
}

#[derive(Debug, Deserialize)]
pub struct EnvironmentEntry {
    pub name: String,
    pub position: u32,
    #[serde(default)]
    pub is_canonical: bool,
    pub runtime_profile_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEnvironmentsRequest {
    pub environments: Vec<EnvironmentEntry>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRuntimeProfileRequest {
    pub name: String,
    pub kind: String,
    pub base_url: String,
    #[serde(default)]
    pub surface_endpoints: BTreeMap<String, String>,
    #[serde(default)]
    pub env_vars: BTreeMap<String, EnvVarValue>,
    #[serde(default)]
    pub secrets: BTreeMap<String, String>,
    pub database_engine: String,
    pub connection_ref: String,
    pub provisioning_mode: String,
    pub base_profile_id: Option<String>,
    #[serde(default)]
    pub migration_paths: Vec<String>,
    pub migration_command: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateRuntimeProfileRequest {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub surface_endpoints: Option<BTreeMap<String, String>>,
    pub env_vars: Option<BTreeMap<String, EnvVarValue>>,
    pub secrets: Option<BTreeMap<String, String>>,
    pub database_engine: Option<String>,
    pub connection_ref: Option<String>,
    pub provisioning_mode: Option<String>,
    pub base_profile_id: Option<Option<String>>,
    pub migration_paths: Option<Vec<String>>,
    pub migration_command: Option<Option<String>>,
}

#[derive(Debug, Serialize)]
pub struct RuntimeProfileResponse {
    pub id: String,
    pub app_id: String,
    pub name: String,
    pub kind: RuntimeProfileKind,
    pub base_url: String,
    pub surface_endpoints: BTreeMap<String, String>,
    pub env_vars: BTreeMap<String, EnvVarValue>,
    pub secrets: BTreeMap<String, String>,
    pub database_engine: String,
    pub connection_ref: String,
    pub provisioning_mode: String,
    pub base_profile_id: Option<String>,
    pub migration_paths: Vec<String>,
    pub migration_command: Option<String>,
    pub revision: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct RevealSecretResponse {
    pub key: String,
    pub value: String,
}

fn require_role(auth: &AuthUser, app_id: &str, role: Role) -> Result<(), ApiConmanError> {
    auth.require_role(app_id, role)?;
    Ok(())
}

fn parse_enum<T>(raw: &str, label: &str) -> Result<T, ApiConmanError>
where
    T: std::str::FromStr<Err = String>,
{
    raw.parse().map_err(|err: String| {
        ConmanError::Validation {
            message: format!("{label}: {err}"),
        }
        .into()
    })
}

fn validate_env_keys(env_vars: &BTreeMap<String, EnvVarValue>) -> Result<(), ApiConmanError> {
    for key in env_vars.keys() {
        if key.is_empty() {
            return Err(ConmanError::Validation {
                message: "env var key cannot be empty".to_string(),
            }
            .into());
        }
        if !key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            return Err(ConmanError::Validation {
                message: format!("invalid env var key `{key}`: only [A-Za-z0-9_] is allowed in v1"),
            }
            .into());
        }
    }
    Ok(())
}

fn runtime_profile_response(
    profile: RuntimeProfile,
    master_key: &str,
) -> Result<RuntimeProfileResponse, ApiConmanError> {
    let mut secrets = BTreeMap::new();
    for (key, encrypted) in profile.secrets_encrypted {
        let decrypted = decrypt_secret(master_key, &encrypted)?;
        secrets.insert(key, mask_secret(&decrypted));
    }

    Ok(RuntimeProfileResponse {
        id: profile.id,
        app_id: profile.app_id,
        name: profile.name,
        kind: profile.kind,
        base_url: profile.base_url,
        surface_endpoints: profile.surface_endpoints,
        env_vars: profile.env_vars,
        secrets,
        database_engine: profile.database_engine,
        connection_ref: profile.connection_ref,
        provisioning_mode: profile.provisioning_mode,
        base_profile_id: profile.base_profile_id,
        migration_paths: profile.migration_paths,
        migration_command: profile.migration_command,
        revision: profile.revision,
        created_at: profile.created_at,
        updated_at: profile.updated_at,
    })
}

async fn validate_surface_endpoint_keys(
    state: &AppState,
    app_id: &str,
    surface_endpoints: &BTreeMap<String, String>,
) -> Result<(), ApiConmanError> {
    if surface_endpoints.is_empty() {
        return Ok(());
    }

    let surfaces = conman_db::AppSurfaceRepo::new(state.db.clone())
        .list_by_repo(app_id)
        .await?;
    let keys = surfaces
        .into_iter()
        .map(|s| s.key)
        .collect::<std::collections::BTreeSet<_>>();

    for key in surface_endpoints.keys() {
        if !keys.contains(key) {
            return Err(ConmanError::Validation {
                message: format!("unknown surface endpoint key `{key}` for app {app_id}"),
            }
            .into());
        }
    }

    Ok(())
}

pub async fn list_apps(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<ApiResponse<Vec<App>>>, ApiConmanError> {
    let pagination = pagination.validate()?;
    let app_ids = auth.roles.keys().cloned().collect::<Vec<_>>();
    let apps = conman_db::AppRepo::new(state.db.clone());
    let (items, total) = apps
        .list_by_ids(&app_ids, pagination.skip(), pagination.limit)
        .await?;
    Ok(Json(ApiResponse::paginated(
        items,
        pagination.page,
        pagination.limit,
        total,
    )))
}

pub async fn create_app(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(req): Json<CreateAppRequest>,
) -> Result<Json<ApiResponse<App>>, ApiConmanError> {
    if req.name.trim().is_empty() || req.repo_path.trim().is_empty() {
        return Err(ConmanError::Validation {
            message: "name and repo_path are required".to_string(),
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
    let app = app_repo
        .insert(
            &req.name,
            &req.repo_path,
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
        "app",
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

pub async fn get_app(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(app_id): Path<String>,
) -> Result<Json<ApiResponse<App>>, ApiConmanError> {
    require_role(&auth, &app_id, Role::User)?;
    let app = conman_db::AppRepo::new(state.db.clone())
        .find_by_id(&app_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "app",
            id: app_id.clone(),
        })?;
    Ok(Json(ApiResponse::ok(app)))
}

pub async fn update_app_settings(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(app_id): Path<String>,
    Json(req): Json<UpdateAppSettingsRequest>,
) -> Result<Json<ApiResponse<App>>, ApiConmanError> {
    require_role(&auth, &app_id, Role::AppAdmin)?;

    let app_repo = conman_db::AppRepo::new(state.db.clone());
    let mut app = app_repo
        .find_by_id(&app_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "app",
            id: app_id.clone(),
        })?;

    if let Some(mode) = req.baseline_mode.as_deref() {
        app.settings.baseline_mode = parse_enum::<BaselineMode>(mode, "baseline_mode")?;
    }
    if let Some(mode) = req.commit_mode_default.as_deref() {
        app.settings.commit_mode_default = parse_enum::<CommitMode>(mode, "commit_mode_default")?;
    }
    if let Some(policy) = req.profile_approval_policy.as_deref() {
        app.settings.profile_approval_policy =
            parse_enum::<ProfileApprovalPolicy>(policy, "profile_approval_policy")?;
    }
    if let Some(canonical_env_id) = req.canonical_env_id {
        app.settings.canonical_env_id = Some(canonical_env_id);
    }
    if let Some(paths) = req.blocked_paths {
        app.settings.blocked_paths = paths;
    }
    if let Some(limit) = req.file_size_limit_bytes {
        if limit == 0 {
            return Err(ConmanError::Validation {
                message: "file_size_limit_bytes must be > 0".to_string(),
            }
            .into());
        }
        app.settings.file_size_limit_bytes = limit;
    }

    let app = app_repo.update_settings(&app_id, &app.settings).await?;
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&app_id),
        "app_settings",
        &app.id,
        "updated",
        None,
        serde_json::to_value(&app.settings).ok(),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }
    Ok(Json(ApiResponse::ok(app)))
}

pub async fn list_members(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(app_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<MemberResponse>>>, ApiConmanError> {
    require_role(&auth, &app_id, Role::User)?;
    let memberships = conman_db::MembershipRepo::new(state.db.clone())
        .list_by_app_id(&app_id)
        .await?;
    let users = conman_db::UserRepo::new(state.db.clone());
    let mut out = Vec::with_capacity(memberships.len());
    for membership in memberships {
        let user = users.find_by_id(&membership.user_id).await?;
        out.push(MemberResponse {
            user_id: membership.user_id,
            app_id: membership.app_id,
            role: membership.role,
            created_at: membership.created_at,
            email: user.as_ref().map(|u| u.email.clone()),
            name: user.as_ref().map(|u| u.name.clone()),
        });
    }
    Ok(Json(ApiResponse::ok(out)))
}

pub async fn assign_member(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(app_id): Path<String>,
    Json(req): Json<AssignMemberRequest>,
) -> Result<Json<ApiResponse<MemberResponse>>, ApiConmanError> {
    require_role(&auth, &app_id, Role::AppAdmin)?;
    let membership = conman_db::MembershipRepo::new(state.db.clone())
        .assign_role(&req.user_id, &app_id, req.role)
        .await?;
    let user = conman_db::UserRepo::new(state.db.clone())
        .find_by_id(&req.user_id)
        .await?;
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&app_id),
        "membership",
        &format!("{}:{}", app_id, membership.user_id),
        "assigned",
        None,
        Some(serde_json::json!({
            "user_id": membership.user_id,
            "role": membership.role,
        })),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }
    Ok(Json(ApiResponse::ok(MemberResponse {
        user_id: membership.user_id,
        app_id: membership.app_id,
        role: membership.role,
        created_at: membership.created_at,
        email: user.as_ref().map(|u| u.email.clone()),
        name: user.as_ref().map(|u| u.name.clone()),
    })))
}

pub async fn create_invite(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(app_id): Path<String>,
    Json(req): Json<CreateInviteRequest>,
) -> Result<Json<ApiResponse<Invite>>, ApiConmanError> {
    require_role(&auth, &app_id, Role::AppAdmin)?;
    if req.email.trim().is_empty() {
        return Err(ConmanError::Validation {
            message: "email is required".to_string(),
        }
        .into());
    }

    let invite = conman_db::InviteRepo::new(state.db.clone())
        .create(
            &app_id,
            &req.email,
            req.role,
            &auth.user_id,
            state.config.invite_expiry_days,
        )
        .await?;
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&app_id),
        "invite",
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

pub async fn list_environments(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(app_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<Environment>>>, ApiConmanError> {
    require_role(&auth, &app_id, Role::User)?;
    let environments = conman_db::EnvironmentRepo::new(state.db.clone())
        .list_by_app(&app_id)
        .await?;
    Ok(Json(ApiResponse::ok(environments)))
}

pub async fn replace_environments(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(app_id): Path<String>,
    Json(req): Json<UpdateEnvironmentsRequest>,
) -> Result<Json<ApiResponse<Vec<Environment>>>, ApiConmanError> {
    require_role(&auth, &app_id, Role::AppAdmin)?;
    let input = req
        .environments
        .iter()
        .map(|e| EnvironmentInput {
            name: e.name.clone(),
            position: e.position,
            is_canonical: e.is_canonical,
            runtime_profile_id: e.runtime_profile_id.clone(),
        })
        .collect::<Vec<_>>();
    let environments = conman_db::EnvironmentRepo::new(state.db.clone())
        .replace_all(&app_id, &input)
        .await?;

    if let Some(canonical) = environments.iter().find(|e| e.is_canonical) {
        let app_repo = conman_db::AppRepo::new(state.db.clone());
        if let Some(mut app) = app_repo.find_by_id(&app_id).await? {
            app.settings.canonical_env_id = Some(canonical.id.clone());
            app_repo.update_settings(&app_id, &app.settings).await?;
        }
    }

    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&app_id),
        "environment_set",
        &app_id,
        "replaced",
        None,
        serde_json::to_value(&environments).ok(),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }

    Ok(Json(ApiResponse::ok(environments)))
}

pub async fn list_runtime_profiles(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(app_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<RuntimeProfileResponse>>>, ApiConmanError> {
    require_role(&auth, &app_id, Role::User)?;
    let profiles = conman_db::RuntimeProfileRepo::new(state.db.clone())
        .list_by_app(&app_id)
        .await?;
    let mut out = Vec::with_capacity(profiles.len());
    for profile in profiles {
        out.push(runtime_profile_response(
            profile,
            &state.config.secrets_master_key,
        )?);
    }
    Ok(Json(ApiResponse::ok(out)))
}

pub async fn create_runtime_profile(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(app_id): Path<String>,
    Json(req): Json<CreateRuntimeProfileRequest>,
) -> Result<Json<ApiResponse<RuntimeProfileResponse>>, ApiConmanError> {
    require_role(&auth, &app_id, Role::AppAdmin)?;
    validate_env_keys(&req.env_vars)?;
    validate_surface_endpoint_keys(&state, &app_id, &req.surface_endpoints).await?;
    let kind = parse_enum::<RuntimeProfileKind>(&req.kind, "kind")?;
    let profile = conman_db::RuntimeProfileRepo::new(state.db.clone())
        .create(
            &app_id,
            RuntimeProfileInput {
                name: req.name,
                kind,
                base_url: req.base_url,
                surface_endpoints: req.surface_endpoints,
                env_vars: req.env_vars,
                secrets_plain: req.secrets,
                database_engine: req.database_engine,
                connection_ref: req.connection_ref,
                provisioning_mode: req.provisioning_mode,
                base_profile_id: req.base_profile_id,
                migration_paths: req.migration_paths,
                migration_command: req.migration_command,
            },
            &state.config.secrets_master_key,
        )
        .await?;

    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&app_id),
        "runtime_profile",
        &profile.id,
        "created",
        None,
        serde_json::to_value(&runtime_profile_response(
            profile.clone(),
            &state.config.secrets_master_key,
        )?)
        .ok(),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }

    Ok(Json(ApiResponse::ok(runtime_profile_response(
        profile,
        &state.config.secrets_master_key,
    )?)))
}

pub async fn get_runtime_profile(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, profile_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<RuntimeProfileResponse>>, ApiConmanError> {
    require_role(&auth, &app_id, Role::User)?;
    let profile = conman_db::RuntimeProfileRepo::new(state.db.clone())
        .find_by_id(&profile_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "runtime_profile",
            id: profile_id.clone(),
        })?;
    if profile.app_id != app_id {
        return Err(ConmanError::Forbidden {
            message: "runtime profile does not belong to app".to_string(),
        }
        .into());
    }
    Ok(Json(ApiResponse::ok(runtime_profile_response(
        profile,
        &state.config.secrets_master_key,
    )?)))
}

pub async fn update_runtime_profile(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, profile_id)): Path<(String, String)>,
    Json(req): Json<UpdateRuntimeProfileRequest>,
) -> Result<Json<ApiResponse<RuntimeProfileResponse>>, ApiConmanError> {
    require_role(&auth, &app_id, Role::AppAdmin)?;
    if let Some(env_vars) = req.env_vars.as_ref() {
        validate_env_keys(env_vars)?;
    }
    if let Some(surface_endpoints) = req.surface_endpoints.as_ref() {
        validate_surface_endpoint_keys(&state, &app_id, surface_endpoints).await?;
    }

    let profile = conman_db::RuntimeProfileRepo::new(state.db.clone())
        .update(
            &profile_id,
            RuntimeProfileUpdate {
                name: req.name,
                base_url: req.base_url,
                surface_endpoints: req.surface_endpoints,
                env_vars: req.env_vars,
                secrets_plain: req.secrets,
                database_engine: req.database_engine,
                connection_ref: req.connection_ref,
                provisioning_mode: req.provisioning_mode,
                base_profile_id: req.base_profile_id,
                migration_paths: req.migration_paths,
                migration_command: req.migration_command,
            },
            &state.config.secrets_master_key,
        )
        .await?;
    if profile.app_id != app_id {
        return Err(ConmanError::Forbidden {
            message: "runtime profile does not belong to app".to_string(),
        }
        .into());
    }

    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&app_id),
        "runtime_profile",
        &profile.id,
        "updated",
        None,
        serde_json::to_value(&runtime_profile_response(
            profile.clone(),
            &state.config.secrets_master_key,
        )?)
        .ok(),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }

    Ok(Json(ApiResponse::ok(runtime_profile_response(
        profile,
        &state.config.secrets_master_key,
    )?)))
}

pub async fn reveal_runtime_profile_secret(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((app_id, profile_id, key)): Path<(String, String, String)>,
) -> Result<Json<ApiResponse<RevealSecretResponse>>, ApiConmanError> {
    require_role(&auth, &app_id, Role::AppAdmin)?;
    let value = conman_db::RuntimeProfileRepo::new(state.db.clone())
        .reveal_secret(&profile_id, &key, &state.config.secrets_master_key)
        .await?;
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&app_id),
        "runtime_profile_secret",
        &profile_id,
        "revealed",
        None,
        Some(serde_json::json!({
            "key": key,
            "value_masked": mask_secret(&value),
        })),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }
    Ok(Json(ApiResponse::ok(RevealSecretResponse { key, value })))
}
