use axum::Json;
use axum::extract::Request;
use axum::extract::State;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use conman_auth::{
    AuthUser, PasswordPolicy, hash_password, issue_token, validate_token, verify_password,
};
use conman_core::{ConmanError, Role};
use metrics::counter;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ApiConmanError;
use crate::events::emit_audit;
use crate::response::ApiResponse;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct SignupRequest {
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}

#[derive(Debug, Deserialize)]
pub struct AcceptInviteRequest {
    pub token: String,
    pub name: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct UserSummary {
    pub id: String,
    pub email: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct TeamSummary {
    pub id: String,
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Serialize)]
pub struct RepoSummary {
    pub id: String,
    pub name: String,
    pub repo_path: String,
    pub integration_branch: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserSummary,
}

#[derive(Debug, Serialize)]
pub struct SignupResponse {
    pub token: String,
    pub user: UserSummary,
    pub team: TeamSummary,
    pub repo: RepoSummary,
}

#[derive(Debug, Serialize)]
pub struct ForgotPasswordResponse {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub message: String,
}

const AUTH_FAILURES_TOTAL: &str = "conman_auth_failures_total";

fn first_name(name: &str) -> String {
    name.split_whitespace().next().unwrap_or("New").to_string()
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
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "team".to_string()
    } else {
        trimmed
    }
}

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<ApiResponse<LoginResponse>>, ApiConmanError> {
    if req.email.trim().is_empty() || req.password.is_empty() {
        return Err(ConmanError::Validation {
            message: "email and password are required".to_string(),
        }
        .into());
    }

    let users = conman_db::UserRepo::new(state.db.clone());
    let memberships = conman_db::RepoMembershipRepo::new(state.db.clone());

    let user = users.find_by_email(&req.email).await?.ok_or_else(|| {
        counter!(AUTH_FAILURES_TOTAL, "reason" => "unknown_email").increment(1);
        ConmanError::Unauthorized {
            message: "invalid_credentials".to_string(),
        }
    })?;

    let valid = verify_password(&req.password, &user.password_hash)?;
    if !valid {
        counter!(AUTH_FAILURES_TOTAL, "reason" => "bad_password").increment(1);
        return Err(ConmanError::Unauthorized {
            message: "invalid_credentials".to_string(),
        }
        .into());
    }

    let roles = memberships.find_roles_by_user_id(&user.id).await?;

    let token = issue_token(
        &user.id,
        &user.email,
        roles,
        &state.config.jwt_secret,
        state.config.jwt_expiry_hours,
    )?;

    Ok(Json(ApiResponse::ok(LoginResponse {
        token,
        user: UserSummary {
            id: user.id,
            email: user.email,
            name: user.name,
        },
    })))
}

pub async fn signup(
    State(state): State<AppState>,
    Json(req): Json<SignupRequest>,
) -> Result<Json<ApiResponse<SignupResponse>>, ApiConmanError> {
    if req.name.trim().is_empty() || req.email.trim().is_empty() || req.password.is_empty() {
        return Err(ConmanError::Validation {
            message: "name, email and password are required".to_string(),
        }
        .into());
    }
    PasswordPolicy::validate(&req.password)?;

    let users = conman_db::UserRepo::new(state.db.clone());
    let team_repo = conman_db::TeamRepo::new(state.db.clone());
    let team_memberships = conman_db::TeamMembershipRepo::new(state.db.clone());
    let app_repo = conman_db::RepoStore::new(state.db.clone());
    let memberships = conman_db::RepoMembershipRepo::new(state.db.clone());

    if users.find_by_email(&req.email).await?.is_some() {
        return Err(ConmanError::Conflict {
            message: "email already registered".to_string(),
        }
        .into());
    }

    let password_hash = hash_password(&req.password)?;
    let user = users
        .insert(req.email.trim(), req.name.trim(), &password_hash)
        .await?;

    let first = first_name(req.name.trim());
    let team_name = format!("{}'s Team", first);
    let team_slug = format!(
        "{}-{}",
        slugify(&team_name),
        &Uuid::now_v7().to_string()[..8]
    );
    let team = team_repo.create(&team_name, &team_slug).await?;

    team_memberships
        .assign_role(&user.id, &team.id, Role::Owner)
        .await?;

    let repo_base_name = format!("{}'s Team Configuration", first);
    let repo_base_path = format!(
        "{}-configuration-{}",
        slugify(&first),
        &Uuid::now_v7().to_string()[..8]
    );

    let mut repo_name = repo_base_name.clone();
    let mut repo_path = repo_base_path.clone();
    let repo = loop {
        match app_repo
            .insert_for_team(&team.id, &repo_name, &repo_path, "main", &user.id)
            .await
        {
            Ok(app) => break app,
            Err(ConmanError::Conflict { .. }) => {
                let suffix = &Uuid::now_v7().to_string()[..8];
                repo_name = format!("{} {}", repo_base_name, suffix);
                repo_path = format!("{}-{}", repo_base_path, suffix);
            }
            Err(err) => return Err(err.into()),
        }
    };

    memberships
        .assign_role(&user.id, &repo.id, Role::Owner)
        .await?;

    if let Err(err) = emit_audit(
        &state,
        Some(&user.id),
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
    if let Err(err) = emit_audit(
        &state,
        Some(&user.id),
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

    let roles = memberships.find_roles_by_user_id(&user.id).await?;
    let token = issue_token(
        &user.id,
        &user.email,
        roles,
        &state.config.jwt_secret,
        state.config.jwt_expiry_hours,
    )?;

    Ok(Json(ApiResponse::ok(SignupResponse {
        token,
        user: UserSummary {
            id: user.id,
            email: user.email,
            name: user.name,
        },
        team: TeamSummary {
            id: team.id,
            name: team.name,
            slug: team.slug,
        },
        repo: RepoSummary {
            id: repo.id,
            name: repo.name,
            repo_path: repo.repo_path,
            integration_branch: repo.integration_branch,
        },
    })))
}

pub async fn forgot_password(
    State(state): State<AppState>,
    Json(req): Json<ForgotPasswordRequest>,
) -> Result<Json<ApiResponse<ForgotPasswordResponse>>, ApiConmanError> {
    if req.email.trim().is_empty() {
        return Err(ConmanError::Validation {
            message: "email is required".to_string(),
        }
        .into());
    }

    let users = conman_db::UserRepo::new(state.db.clone());
    let tokens = conman_db::PasswordResetRepo::new(state.db.clone());

    let reset_token = if let Some(user) = users.find_by_email(&req.email).await? {
        let token = tokens.create(&user.id, 60).await?;
        if let Err(err) = emit_audit(
            &state,
            Some(&user.id),
            None,
            "password_reset_token",
            &token.id,
            "requested",
            None,
            Some(serde_json::json!({
                "user_id": user.id,
                "expires_at": token.expires_at,
            })),
            None,
        )
        .await
        {
            tracing::warn!(error = %err, "failed to write audit event");
        }
        Some(token.token)
    } else {
        None
    };

    Ok(Json(ApiResponse::ok(ForgotPasswordResponse {
        message: "if the account exists, a reset email has been queued".to_string(),
        reset_token,
    })))
}

pub async fn reset_password(
    State(state): State<AppState>,
    Json(req): Json<ResetPasswordRequest>,
) -> Result<Json<ApiResponse<MessageResponse>>, ApiConmanError> {
    if req.token.trim().is_empty() || req.new_password.is_empty() {
        return Err(ConmanError::Validation {
            message: "token and new_password are required".to_string(),
        }
        .into());
    }

    PasswordPolicy::validate(&req.new_password)?;

    let reset_repo = conman_db::PasswordResetRepo::new(state.db.clone());
    let users = conman_db::UserRepo::new(state.db.clone());

    let token = reset_repo
        .find_active_by_token(&req.token)
        .await?
        .ok_or_else(|| ConmanError::Forbidden {
            message: "invalid_or_expired_reset_token".to_string(),
        })?;

    let password_hash = hash_password(&req.new_password)?;
    users
        .update_password(&token.user_id, &password_hash)
        .await?;
    reset_repo.mark_used(&token.id).await?;
    if let Err(err) = emit_audit(
        &state,
        Some(&token.user_id),
        None,
        "password",
        &token.user_id,
        "reset",
        None,
        None,
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }

    Ok(Json(ApiResponse::ok(MessageResponse {
        message: "password updated".to_string(),
    })))
}

pub async fn accept_invite(
    State(state): State<AppState>,
    Json(req): Json<AcceptInviteRequest>,
) -> Result<Json<ApiResponse<LoginResponse>>, ApiConmanError> {
    if req.token.trim().is_empty() || req.name.trim().is_empty() || req.password.is_empty() {
        return Err(ConmanError::Validation {
            message: "token, name and password are required".to_string(),
        }
        .into());
    }
    PasswordPolicy::validate(&req.password)?;

    let invites = conman_db::InviteRepo::new(state.db.clone());
    let users = conman_db::UserRepo::new(state.db.clone());
    let team_memberships = conman_db::TeamMembershipRepo::new(state.db.clone());
    let memberships = conman_db::RepoMembershipRepo::new(state.db.clone());
    let app_repo = conman_db::RepoStore::new(state.db.clone());

    let invite = invites
        .find_active_by_token(&req.token)
        .await?
        .ok_or_else(|| ConmanError::Forbidden {
            message: "invalid_or_expired_invite".to_string(),
        })?;

    let user = match users.find_by_email(&invite.email).await? {
        Some(user) => user,
        None => {
            let password_hash = hash_password(&req.password)?;
            users
                .insert(&invite.email, &req.name, &password_hash)
                .await?
        }
    };

    team_memberships
        .assign_role(&user.id, &invite.team_id, invite.role)
        .await?;

    let apps = app_repo.list_by_team_id(&invite.team_id).await?;
    for app in apps {
        memberships
            .assign_role(&user.id, &app.id, invite.role)
            .await?;
    }

    invites.mark_accepted(&invite.id).await?;
    if let Err(err) = emit_audit(
        &state,
        Some(&user.id),
        None,
        "team_invite",
        &invite.id,
        "accepted",
        None,
        serde_json::to_value(&invite).ok(),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }

    let roles = memberships.find_roles_by_user_id(&user.id).await?;
    let token = issue_token(
        &user.id,
        &user.email,
        roles,
        &state.config.jwt_secret,
        state.config.jwt_expiry_hours,
    )?;

    Ok(Json(ApiResponse::ok(LoginResponse {
        token,
        user: UserSummary {
            id: user.id,
            email: user.email,
            name: user.name,
        },
    })))
}

pub async fn logout() -> Json<ApiResponse<MessageResponse>> {
    Json(ApiResponse::ok(MessageResponse {
        message: "logged out".to_string(),
    }))
}

pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path();

    if !is_protected_path(path) {
        return next.run(req).await;
    }

    let result = (|| -> Result<AuthUser, ConmanError> {
        let token = bearer_token(req.headers())?;
        let claims = validate_token(token, &state.config.jwt_secret)?;

        Ok(AuthUser {
            user_id: claims.sub,
            email: claims.email,
            roles: claims.roles,
        })
    })();

    match result {
        Ok(auth_user) => {
            req.extensions_mut().insert(auth_user);
            next.run(req).await
        }
        Err(err) => {
            counter!(AUTH_FAILURES_TOTAL, "reason" => "invalid_bearer").increment(1);
            ApiConmanError(err).into_response()
        }
    }
}

fn is_protected_path(path: &str) -> bool {
    if path == "/api/auth/logout" {
        return true;
    }

    path.starts_with("/api/repo")
        || path.starts_with("/api/repos")
        || path.starts_with("/api/teams")
        || path.starts_with("/api/me")
}

fn bearer_token(headers: &axum::http::HeaderMap) -> Result<&str, ConmanError> {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ConmanError::Forbidden {
            message: "missing bearer token".to_string(),
        })?;

    auth.strip_prefix("Bearer ")
        .or_else(|| auth.strip_prefix("bearer "))
        .ok_or_else(|| ConmanError::Forbidden {
            message: "invalid bearer token".to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_path_logic() {
        assert!(is_protected_path("/api/repos"));
        assert!(is_protected_path("/api/repos/abc"));
        assert!(is_protected_path("/api/repo"));
        assert!(is_protected_path("/api/teams"));
        assert!(is_protected_path("/api/teams/abc"));
        assert!(is_protected_path("/api/me/notification-preferences"));
        assert!(is_protected_path("/api/auth/logout"));

        assert!(!is_protected_path("/api/health"));
        assert!(!is_protected_path("/api/auth/login"));
        assert!(!is_protected_path("/api/auth/forgot-password"));
        assert!(!is_protected_path("/api/nonexistent"));
    }

    #[test]
    fn bearer_parsing() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("authorization", "Bearer token123".parse().expect("hv"));
        assert_eq!(bearer_token(&headers).expect("token"), "token123");
    }
}
