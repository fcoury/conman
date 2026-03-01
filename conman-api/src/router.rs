use axum::extract::DefaultBodyLimit;
use axum::extract::Request;
use axum::http::HeaderValue;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{any, delete, get, patch, post};
use axum::{Json, Router};
use uuid::Uuid;

use crate::auth::{
    accept_invite, auth_middleware, forgot_password, login, logout, reset_password, signup,
};
use crate::handlers::changesets::{
    create_changeset, create_changeset_comment, get_changeset, get_changeset_diff,
    list_changeset_comments, list_changesets, move_changeset_to_draft, queue_changeset,
    resubmit_changeset, review_changeset, submit_changeset, update_changeset,
};
use crate::handlers::deployments::{
    deploy_environment, list_deployments, promote_environment, rollback_environment,
};
use crate::handlers::health::health_check;
use crate::handlers::jobs::{get_job, list_jobs};
use crate::handlers::me::{get_notification_preferences, update_notification_preferences};
use crate::handlers::metrics::scrape_metrics;
use crate::handlers::onboarding::create_instance;
use crate::handlers::releases::{
    assemble_release, create_release, get_release, list_releases, publish_release,
    reorder_release_changesets, set_release_changesets,
};
use crate::handlers::repos::{
    assign_member, create_runtime_profile, get_repo, get_runtime_profile, list_environments,
    list_members, list_repos, list_runtime_profiles, replace_environments,
    reveal_runtime_profile_secret, update_repo_settings, update_runtime_profile,
};
use crate::handlers::teams::{
    create_repo_app, create_repo_under_team, create_team, create_team_invite, delete_team_invite,
    get_team, list_repo_apps, list_team_invites, list_teams, resend_team_invite, update_repo_app,
};
use crate::handlers::temp_envs::{
    create_temp_env, delete_temp_env, extend_temp_env, list_temp_envs, undo_expire_temp_env,
};
use crate::handlers::ui::{get_bound_repo, update_bound_repo};
use crate::handlers::web::{serve_app_asset, serve_app_index};
use crate::handlers::workspaces::{
    create_workspace, create_workspace_checkpoint, delete_workspace_file, get_workspace,
    get_workspace_change_patch, get_workspace_changes, get_workspace_file_or_tree,
    get_workspace_open_changeset, list_workspaces, reset_workspace, sync_workspace_integration,
    update_workspace, write_workspace_file,
};
use crate::openapi::{openapi_docs, openapi_json};
use crate::request_context::RequestContext;
use crate::response::{ApiError, ApiErrorBody};
use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health_check))
        .route("/api/metrics", get(scrape_metrics))
        .route("/api/openapi.json", get(openapi_json))
        .route("/api/docs", get(openapi_docs))
        .route("/api/auth/login", post(login))
        .route("/api/auth/signup", post(signup))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/forgot-password", post(forgot_password))
        .route("/api/auth/reset-password", post(reset_password))
        .route("/api/auth/accept-invite", post(accept_invite))
        .route("/api/onboarding/instance", post(create_instance))
        .route("/api/teams", get(list_teams).post(create_team))
        .route("/api/teams/{teamId}", get(get_team))
        .route("/api/teams/{teamId}/repos", post(create_repo_under_team))
        .route("/api/repo", get(get_bound_repo).patch(update_bound_repo))
        .route(
            "/api/repos/{repoId}/apps",
            get(list_repo_apps).post(create_repo_app),
        )
        .route("/api/repos/{repoId}/apps/{appId}", patch(update_repo_app))
        .route("/api/repos", get(list_repos))
        .route("/api/repos/{repoId}", get(get_repo))
        .route("/api/repos/{repoId}/settings", patch(update_repo_settings))
        .route(
            "/api/repos/{repoId}/members",
            get(list_members).post(assign_member),
        )
        .route(
            "/api/teams/{teamId}/invites",
            get(list_team_invites).post(create_team_invite),
        )
        .route(
            "/api/teams/{teamId}/invites/{inviteId}/resend",
            post(resend_team_invite),
        )
        .route(
            "/api/teams/{teamId}/invites/{inviteId}",
            delete(delete_team_invite),
        )
        .route(
            "/api/repos/{repoId}/workspaces",
            get(list_workspaces).post(create_workspace),
        )
        .route(
            "/api/repos/{repoId}/workspaces/{workspaceId}",
            get(get_workspace).patch(update_workspace),
        )
        .route(
            "/api/repos/{repoId}/workspaces/{workspaceId}/reset",
            post(reset_workspace),
        )
        .route(
            "/api/repos/{repoId}/workspaces/{workspaceId}/sync-integration",
            post(sync_workspace_integration),
        )
        .route(
            "/api/repos/{repoId}/workspaces/{workspaceId}/files",
            get(get_workspace_file_or_tree)
                .put(write_workspace_file)
                .delete(delete_workspace_file),
        )
        .route(
            "/api/repos/{repoId}/workspaces/{workspaceId}/changes",
            get(get_workspace_changes),
        )
        .route(
            "/api/repos/{repoId}/workspaces/{workspaceId}/changes/patch",
            get(get_workspace_change_patch),
        )
        .route(
            "/api/repos/{repoId}/workspaces/{workspaceId}/open-changeset",
            get(get_workspace_open_changeset),
        )
        .route(
            "/api/repos/{repoId}/workspaces/{workspaceId}/checkpoints",
            post(create_workspace_checkpoint),
        )
        .route(
            "/api/repos/{repoId}/changesets",
            get(list_changesets).post(create_changeset),
        )
        .route(
            "/api/repos/{repoId}/changesets/{changesetId}",
            get(get_changeset).patch(update_changeset),
        )
        .route(
            "/api/repos/{repoId}/changesets/{changesetId}/submit",
            post(submit_changeset),
        )
        .route(
            "/api/repos/{repoId}/changesets/{changesetId}/resubmit",
            post(resubmit_changeset),
        )
        .route(
            "/api/repos/{repoId}/changesets/{changesetId}/review",
            post(review_changeset),
        )
        .route(
            "/api/repos/{repoId}/changesets/{changesetId}/queue",
            post(queue_changeset),
        )
        .route(
            "/api/repos/{repoId}/changesets/{changesetId}/move-to-draft",
            post(move_changeset_to_draft),
        )
        .route(
            "/api/repos/{repoId}/changesets/{changesetId}/diff",
            get(get_changeset_diff),
        )
        .route(
            "/api/repos/{repoId}/changesets/{changesetId}/comments",
            get(list_changeset_comments).post(create_changeset_comment),
        )
        .route(
            "/api/repos/{repoId}/releases",
            get(list_releases).post(create_release),
        )
        .route("/api/repos/{repoId}/releases/{releaseId}", get(get_release))
        .route(
            "/api/repos/{repoId}/releases/{releaseId}/changesets",
            post(set_release_changesets),
        )
        .route(
            "/api/repos/{repoId}/releases/{releaseId}/reorder",
            post(reorder_release_changesets),
        )
        .route(
            "/api/repos/{repoId}/releases/{releaseId}/assemble",
            post(assemble_release),
        )
        .route(
            "/api/repos/{repoId}/releases/{releaseId}/publish",
            post(publish_release),
        )
        .route(
            "/api/repos/{repoId}/environments",
            get(list_environments).patch(replace_environments),
        )
        .route(
            "/api/repos/{repoId}/runtime-profiles",
            get(list_runtime_profiles).post(create_runtime_profile),
        )
        .route(
            "/api/repos/{repoId}/runtime-profiles/{profileId}",
            get(get_runtime_profile).patch(update_runtime_profile),
        )
        .route(
            "/api/repos/{repoId}/runtime-profiles/{profileId}/secrets/{key}/reveal",
            post(reveal_runtime_profile_secret),
        )
        .route(
            "/api/repos/{repoId}/environments/{envId}/deploy",
            post(deploy_environment),
        )
        .route(
            "/api/repos/{repoId}/environments/{envId}/promote",
            post(promote_environment),
        )
        .route(
            "/api/repos/{repoId}/environments/{envId}/rollback",
            post(rollback_environment),
        )
        .route("/api/repos/{repoId}/deployments", get(list_deployments))
        .route(
            "/api/repos/{repoId}/temp-envs",
            get(list_temp_envs).post(create_temp_env),
        )
        .route(
            "/api/repos/{repoId}/temp-envs/{tempEnvId}/extend",
            post(extend_temp_env),
        )
        .route(
            "/api/repos/{repoId}/temp-envs/{tempEnvId}/undo-expire",
            post(undo_expire_temp_env),
        )
        .route(
            "/api/repos/{repoId}/temp-envs/{tempEnvId}",
            delete(delete_temp_env),
        )
        .route("/api/repos/{repoId}/jobs", get(list_jobs))
        .route("/api/repos/{repoId}/jobs/{jobId}", get(get_job))
        .route(
            "/api/me/notification-preferences",
            get(get_notification_preferences).patch(update_notification_preferences),
        )
        .route("/api", any(fallback_404))
        .route("/api/{*path}", any(fallback_404))
        .route("/", get(serve_app_index))
        .route("/{*path}", get(serve_app_asset))
        .fallback(fallback_404)
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::rate_limit::rate_limit_middleware,
        ))
        .layer(axum::middleware::from_fn(
            crate::metrics::http_metrics_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(axum::middleware::from_fn(request_id_middleware))
        .with_state(state)
}

async fn fallback_404() -> impl IntoResponse {
    let body = ApiError {
        error: ApiErrorBody {
            code: "not_found",
            message: "The requested route does not exist.".to_string(),
            request_id: RequestContext::current_request_id(),
        },
    };
    (StatusCode::NOT_FOUND, Json(body))
}

pub async fn request_id_middleware(mut req: Request, next: Next) -> Response {
    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string)
        .unwrap_or_else(|| Uuid::now_v7().to_string());

    let client_ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);

    let user_agent = req
        .headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);

    let ctx = RequestContext {
        request_id: request_id.clone(),
        client_ip,
        user_agent,
    };

    req.extensions_mut().insert(ctx.clone());

    let mut response =
        RequestContext::scope_request(ctx, || async move { next.run(req).await }).await;

    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", value);
    }

    response
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, sync::Arc};

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use conman_git::NoopGitAdapter;
    use mongodb::Client;
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn request_id_generated_when_absent() {
        let app = Router::new()
            .route("/", get(|| async { StatusCode::OK }))
            .layer(axum::middleware::from_fn(request_id_middleware));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        let request_id = response
            .headers()
            .get("x-request-id")
            .expect("x-request-id")
            .to_str()
            .expect("utf8");

        let parsed = uuid::Uuid::parse_str(request_id).expect("uuid");
        assert_eq!(parsed.get_version(), Some(uuid::Version::SortRand));
    }

    #[tokio::test]
    async fn request_id_echoed_when_provided() {
        let app = Router::new()
            .route("/", get(|| async { StatusCode::OK }))
            .layer(axum::middleware::from_fn(request_id_middleware));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("x-request-id", "my-custom-id-123")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        let echoed = response
            .headers()
            .get("x-request-id")
            .expect("x-request-id")
            .to_str()
            .expect("utf8");
        assert_eq!(echoed, "my-custom-id-123");
    }

    #[tokio::test]
    async fn unknown_route_returns_404_envelope() {
        let client = Client::with_uri_str("mongodb://localhost:27017")
            .await
            .expect("client");

        let state = AppState {
            config: Arc::new(conman_core::Config {
                listen_addr: "127.0.0.1:3000".parse().expect("addr"),
                mongo_uri: "mongodb://localhost:27017".to_string(),
                mongo_db: "conman".to_string(),
                gitaly_address: "http://localhost:8075".to_string(),
                jwt_secret: "secret-secret-secret-secret-1234".to_string(),
                jwt_expiry_hours: 24,
                invite_expiry_days: 7,
                secrets_master_key: "master".to_string(),
                temp_url_domain: "example.test".to_string(),
                http_rate_limit_per_second: 200,
                smtp_host: None,
                smtp_port: 587,
                smtp_username: None,
                smtp_password: None,
                smtp_from_email: None,
                msuite_submit_cmd: "true".to_string(),
                msuite_merge_cmd: "true".to_string(),
                msuite_deploy_cmd: "true".to_string(),
                deploy_release_cmd: "true".to_string(),
                runtime_profile_drift_check_cmd: "true".to_string(),
                temp_env_provision_cmd: None,
                temp_env_expire_cmd: None,
            }),
            db: client.database("conman"),
            git_adapter: Arc::new(NoopGitAdapter),
            rate_limiter: crate::rate_limit::new_shared_rate_limiter(200),
        };

        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/nonexistent")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn metrics_route_returns_scrape_payload() {
        crate::metrics::init_metrics().expect("metrics init");
        let app = Router::new().route(
            "/api/metrics",
            get(crate::handlers::metrics::scrape_metrics),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/metrics")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().contains_key("content-type"));
    }

    fn source_for(relative: &str) -> String {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        fs::read_to_string(root.join(relative)).expect("source file to exist")
    }

    fn function_block<'a>(source: &'a str, fn_name: &str) -> &'a str {
        let markers = [
            format!("pub async fn {fn_name}"),
            format!("async fn {fn_name}"),
        ];
        for marker in markers {
            if let Some(start) = source.find(&marker) {
                let body_start = start + marker.len();
                let tail = &source[body_start..];
                let candidates = [
                    tail.find("\npub async fn "),
                    tail.find("\nasync fn "),
                    tail.find("\n#[cfg(test)]"),
                ];
                let end_rel = candidates
                    .into_iter()
                    .flatten()
                    .filter(|idx| *idx > 0)
                    .min();
                let end = end_rel.map(|idx| body_start + idx).unwrap_or(source.len());
                return &source[start..end];
            }
        }
        panic!("function `{fn_name}` not found");
    }

    #[test]
    fn critical_mutation_handlers_emit_audit_or_delegate() {
        let cases: &[(&str, &str, &str)] = &[
            ("src/auth.rs", "forgot_password", "emit_audit("),
            ("src/auth.rs", "reset_password", "emit_audit("),
            ("src/auth.rs", "accept_invite", "emit_audit("),
            ("src/handlers/repos.rs", "create_repo", "emit_audit("),
            (
                "src/handlers/repos.rs",
                "update_repo_settings",
                "emit_audit(",
            ),
            ("src/handlers/repos.rs", "assign_member", "emit_audit("),
            ("src/handlers/teams.rs", "create_team_invite", "emit_audit("),
            (
                "src/handlers/repos.rs",
                "replace_environments",
                "emit_audit(",
            ),
            (
                "src/handlers/repos.rs",
                "create_runtime_profile",
                "emit_audit(",
            ),
            (
                "src/handlers/repos.rs",
                "update_runtime_profile",
                "emit_audit(",
            ),
            (
                "src/handlers/repos.rs",
                "reveal_runtime_profile_secret",
                "emit_audit(",
            ),
            (
                "src/handlers/workspaces.rs",
                "ensure_default_workspace",
                "audit_workspace_event(",
            ),
            (
                "src/handlers/workspaces.rs",
                "create_workspace",
                "audit_workspace_event(",
            ),
            (
                "src/handlers/workspaces.rs",
                "update_workspace",
                "audit_workspace_event(",
            ),
            (
                "src/handlers/workspaces.rs",
                "write_workspace_file",
                "audit_workspace_event(",
            ),
            (
                "src/handlers/workspaces.rs",
                "delete_workspace_file",
                "audit_workspace_event(",
            ),
            (
                "src/handlers/workspaces.rs",
                "sync_workspace_integration",
                "audit_workspace_event(",
            ),
            (
                "src/handlers/workspaces.rs",
                "reset_workspace",
                "audit_workspace_event(",
            ),
            (
                "src/handlers/workspaces.rs",
                "create_workspace_checkpoint",
                "audit_workspace_event(",
            ),
            (
                "src/handlers/changesets.rs",
                "create_changeset",
                "emit_audit(",
            ),
            (
                "src/handlers/changesets.rs",
                "update_changeset",
                "emit_audit(",
            ),
            (
                "src/handlers/changesets.rs",
                "submit_changeset",
                "emit_audit(",
            ),
            (
                "src/handlers/changesets.rs",
                "resubmit_changeset",
                "emit_audit(",
            ),
            (
                "src/handlers/changesets.rs",
                "review_changeset",
                "emit_audit(",
            ),
            (
                "src/handlers/changesets.rs",
                "queue_changeset",
                "emit_audit(",
            ),
            (
                "src/handlers/changesets.rs",
                "move_changeset_to_draft",
                "emit_audit(",
            ),
            (
                "src/handlers/changesets.rs",
                "create_changeset_comment",
                "emit_audit(",
            ),
            ("src/handlers/releases.rs", "create_release", "emit_audit("),
            (
                "src/handlers/releases.rs",
                "set_release_changesets",
                "emit_audit(",
            ),
            (
                "src/handlers/releases.rs",
                "reorder_release_changesets",
                "set_release_changesets(",
            ),
            (
                "src/handlers/releases.rs",
                "assemble_release",
                "emit_audit(",
            ),
            ("src/handlers/releases.rs", "publish_release", "emit_audit("),
            (
                "src/handlers/deployments.rs",
                "deploy_environment",
                "emit_audit(",
            ),
            (
                "src/handlers/deployments.rs",
                "promote_environment",
                "deploy_environment(",
            ),
            (
                "src/handlers/deployments.rs",
                "rollback_environment",
                "emit_audit(",
            ),
            (
                "src/handlers/temp_envs.rs",
                "create_temp_env",
                "emit_audit(",
            ),
            (
                "src/handlers/temp_envs.rs",
                "extend_temp_env",
                "emit_audit(",
            ),
            (
                "src/handlers/temp_envs.rs",
                "undo_expire_temp_env",
                "emit_audit(",
            ),
            (
                "src/handlers/temp_envs.rs",
                "delete_temp_env",
                "emit_audit(",
            ),
            (
                "src/handlers/me.rs",
                "update_notification_preferences",
                "emit_audit(",
            ),
        ];

        for (file, fn_name, required_fragment) in cases {
            let source = source_for(file);
            let block = function_block(&source, fn_name);
            assert!(
                block.contains(required_fragment),
                "{file}:{fn_name} missing required audit/delegation fragment `{required_fragment}`"
            );
        }
    }
}
