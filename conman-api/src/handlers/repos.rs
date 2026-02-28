use std::collections::BTreeMap;

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use conman_auth::{AuthUser, decrypt_secret};
use conman_core::{
    Repo, BaselineMode, CommitMode, ConmanError, EnvVarValue, Environment, ProfileApprovalPolicy,
    Role, RuntimeProfile, RuntimeProfileKind, mask_secret,
};
use conman_db::{EnvironmentInput, RuntimeProfileInput, RuntimeProfileUpdate};
use serde::{Deserialize, Serialize};

use crate::{
    error::ApiConmanError, events::emit_audit, extractors::Pagination, response::ApiResponse,
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct CreateRepoRequest {
    pub name: String,
    pub repo_path: String,
    pub integration_branch: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRepoSettingsRequest {
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
    pub repo_id: String,
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
    pub app_endpoints: BTreeMap<String, String>,
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
    pub app_endpoints: Option<BTreeMap<String, String>>,
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
    pub repo_id: String,
    pub name: String,
    pub kind: RuntimeProfileKind,
    pub base_url: String,
    pub app_endpoints: BTreeMap<String, String>,
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

fn require_role(auth: &AuthUser, repo_id: &str, role: Role) -> Result<(), ApiConmanError> {
    auth.require_role(repo_id, role)?;
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
        repo_id: profile.repo_id,
        name: profile.name,
        kind: profile.kind,
        base_url: profile.base_url,
        app_endpoints: profile.app_endpoints,
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

async fn validate_app_endpoint_keys(
    state: &AppState,
    repo_id: &str,
    app_endpoints: &BTreeMap<String, String>,
) -> Result<(), ApiConmanError> {
    if app_endpoints.is_empty() {
        return Ok(());
    }

    let apps = conman_db::AppRepo::new(state.db.clone())
        .list_by_repo(repo_id)
        .await?;
    let keys = apps
        .into_iter()
        .map(|s| s.key)
        .collect::<std::collections::BTreeSet<_>>();

    for key in app_endpoints.keys() {
        if !keys.contains(key) {
            return Err(ConmanError::Validation {
                message: format!("unknown app endpoint key `{key}` for repo {repo_id}"),
            }
            .into());
        }
    }

    Ok(())
}

pub async fn list_repos(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<ApiResponse<Vec<Repo>>>, ApiConmanError> {
    let pagination = pagination.validate()?;
    let repo_ids = auth.roles.keys().cloned().collect::<Vec<_>>();
    let repos = conman_db::RepoStore::new(state.db.clone());
    let (items, total) = repos
        .list_by_ids(&repo_ids, pagination.skip(), pagination.limit)
        .await?;
    Ok(Json(ApiResponse::paginated(
        items,
        pagination.page,
        pagination.limit,
        total,
    )))
}

pub async fn create_repo(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(req): Json<CreateRepoRequest>,
) -> Result<Json<ApiResponse<Repo>>, ApiConmanError> {
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

    let repo_store = conman_db::RepoStore::new(state.db.clone());
    let repo_membership_repo = conman_db::RepoMembershipRepo::new(state.db.clone());
    let repo = repo_store
        .insert(
            &req.name,
            &req.repo_path,
            &integration_branch,
            &auth.user_id,
        )
        .await?;

    repo_membership_repo
        .assign_role(&auth.user_id, &repo.id, Role::Admin)
        .await?;
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

    Ok(Json(ApiResponse::ok(repo)))
}

pub async fn get_repo(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(repo_id): Path<String>,
) -> Result<Json<ApiResponse<Repo>>, ApiConmanError> {
    require_role(&auth, &repo_id, Role::Member)?;
    let repo = conman_db::RepoStore::new(state.db.clone())
        .find_by_id(&repo_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "repo",
            id: repo_id.clone(),
        })?;
    Ok(Json(ApiResponse::ok(repo)))
}

pub async fn update_repo_settings(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(repo_id): Path<String>,
    Json(req): Json<UpdateRepoSettingsRequest>,
) -> Result<Json<ApiResponse<Repo>>, ApiConmanError> {
    require_role(&auth, &repo_id, Role::Admin)?;

    let repo_store = conman_db::RepoStore::new(state.db.clone());
    let mut repo = repo_store
        .find_by_id(&repo_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "repo",
            id: repo_id.clone(),
        })?;

    if let Some(mode) = req.baseline_mode.as_deref() {
        repo.settings.baseline_mode = parse_enum::<BaselineMode>(mode, "baseline_mode")?;
    }
    if let Some(mode) = req.commit_mode_default.as_deref() {
        repo.settings.commit_mode_default = parse_enum::<CommitMode>(mode, "commit_mode_default")?;
    }
    if let Some(policy) = req.profile_approval_policy.as_deref() {
        repo.settings.profile_approval_policy =
            parse_enum::<ProfileApprovalPolicy>(policy, "profile_approval_policy")?;
    }
    if let Some(canonical_env_id) = req.canonical_env_id {
        repo.settings.canonical_env_id = Some(canonical_env_id);
    }
    if let Some(paths) = req.blocked_paths {
        repo.settings.blocked_paths = paths;
    }
    if let Some(limit) = req.file_size_limit_bytes {
        if limit == 0 {
            return Err(ConmanError::Validation {
                message: "file_size_limit_bytes must be > 0".to_string(),
            }
            .into());
        }
        repo.settings.file_size_limit_bytes = limit;
    }

    let repo = repo_store.update_settings(&repo_id, &repo.settings).await?;
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&repo_id),
        "app_settings",
        &repo.id,
        "updated",
        None,
        serde_json::to_value(&repo.settings).ok(),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }
    Ok(Json(ApiResponse::ok(repo)))
}

pub async fn list_members(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(repo_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<MemberResponse>>>, ApiConmanError> {
    require_role(&auth, &repo_id, Role::Member)?;
    let memberships = conman_db::RepoMembershipRepo::new(state.db.clone())
        .list_by_repo_id(&repo_id)
        .await?;
    let users = conman_db::UserRepo::new(state.db.clone());
    let mut out = Vec::with_capacity(memberships.len());
    for membership in memberships {
        let user = users.find_by_id(&membership.user_id).await?;
        out.push(MemberResponse {
            user_id: membership.user_id,
            repo_id: membership.repo_id,
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
    Path(repo_id): Path<String>,
    Json(req): Json<AssignMemberRequest>,
) -> Result<Json<ApiResponse<MemberResponse>>, ApiConmanError> {
    require_role(&auth, &repo_id, Role::Admin)?;
    let membership = conman_db::RepoMembershipRepo::new(state.db.clone())
        .assign_role(&req.user_id, &repo_id, req.role)
        .await?;
    let user = conman_db::UserRepo::new(state.db.clone())
        .find_by_id(&req.user_id)
        .await?;
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&repo_id),
        "membership",
        &format!("{}:{}", repo_id, membership.user_id),
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
        repo_id: membership.repo_id,
        role: membership.role,
        created_at: membership.created_at,
        email: user.as_ref().map(|u| u.email.clone()),
        name: user.as_ref().map(|u| u.name.clone()),
    })))
}

pub async fn list_environments(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(repo_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<Environment>>>, ApiConmanError> {
    require_role(&auth, &repo_id, Role::Member)?;
    let environments = conman_db::EnvironmentRepo::new(state.db.clone())
        .list_by_repo(&repo_id)
        .await?;
    Ok(Json(ApiResponse::ok(environments)))
}

pub async fn replace_environments(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(repo_id): Path<String>,
    Json(req): Json<UpdateEnvironmentsRequest>,
) -> Result<Json<ApiResponse<Vec<Environment>>>, ApiConmanError> {
    require_role(&auth, &repo_id, Role::Admin)?;
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
        .replace_all(&repo_id, &input)
        .await?;

    if let Some(canonical) = environments.iter().find(|e| e.is_canonical) {
        let repo_store = conman_db::RepoStore::new(state.db.clone());
        if let Some(mut repo) = repo_store.find_by_id(&repo_id).await? {
            repo.settings.canonical_env_id = Some(canonical.id.clone());
            repo_store.update_settings(&repo_id, &repo.settings).await?;
        }
    }

    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&repo_id),
        "environment_set",
        &repo_id,
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
    Path(repo_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<RuntimeProfileResponse>>>, ApiConmanError> {
    require_role(&auth, &repo_id, Role::Member)?;
    let profiles = conman_db::RuntimeProfileRepo::new(state.db.clone())
        .list_by_repo(&repo_id)
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
    Path(repo_id): Path<String>,
    Json(req): Json<CreateRuntimeProfileRequest>,
) -> Result<Json<ApiResponse<RuntimeProfileResponse>>, ApiConmanError> {
    require_role(&auth, &repo_id, Role::Admin)?;
    validate_env_keys(&req.env_vars)?;
    validate_app_endpoint_keys(&state, &repo_id, &req.app_endpoints).await?;
    let kind = parse_enum::<RuntimeProfileKind>(&req.kind, "kind")?;
    let profile = conman_db::RuntimeProfileRepo::new(state.db.clone())
        .create(
            &repo_id,
            RuntimeProfileInput {
                name: req.name,
                kind,
                base_url: req.base_url,
                app_endpoints: req.app_endpoints,
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
        Some(&repo_id),
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
    Path((repo_id, profile_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<RuntimeProfileResponse>>, ApiConmanError> {
    require_role(&auth, &repo_id, Role::Member)?;
    let profile = conman_db::RuntimeProfileRepo::new(state.db.clone())
        .find_by_id(&profile_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "runtime_profile",
            id: profile_id.clone(),
        })?;
    if profile.repo_id != repo_id {
        return Err(ConmanError::Forbidden {
            message: "runtime profile does not belong to repo".to_string(),
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
    Path((repo_id, profile_id)): Path<(String, String)>,
    Json(req): Json<UpdateRuntimeProfileRequest>,
) -> Result<Json<ApiResponse<RuntimeProfileResponse>>, ApiConmanError> {
    require_role(&auth, &repo_id, Role::Admin)?;
    if let Some(env_vars) = req.env_vars.as_ref() {
        validate_env_keys(env_vars)?;
    }
    if let Some(app_endpoints) = req.app_endpoints.as_ref() {
        validate_app_endpoint_keys(&state, &repo_id, app_endpoints).await?;
    }

    let profile = conman_db::RuntimeProfileRepo::new(state.db.clone())
        .update(
            &profile_id,
            RuntimeProfileUpdate {
                name: req.name,
                base_url: req.base_url,
                app_endpoints: req.app_endpoints,
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
    if profile.repo_id != repo_id {
        return Err(ConmanError::Forbidden {
            message: "runtime profile does not belong to repo".to_string(),
        }
        .into());
    }

    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&repo_id),
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
    Path((repo_id, profile_id, key)): Path<(String, String, String)>,
) -> Result<Json<ApiResponse<RevealSecretResponse>>, ApiConmanError> {
    require_role(&auth, &repo_id, Role::Admin)?;
    let value = conman_db::RuntimeProfileRepo::new(state.db.clone())
        .reveal_secret(&profile_id, &key, &state.config.secrets_master_key)
        .await?;
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&repo_id),
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
