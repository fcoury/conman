use axum::extract::Request;
use axum::http::HeaderValue;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use uuid::Uuid;

use crate::auth::{accept_invite, auth_middleware, forgot_password, login, logout, reset_password};
use crate::handlers::apps::{
    assign_member, create_app, create_invite, create_runtime_profile, get_app, get_runtime_profile,
    list_apps, list_environments, list_members, list_runtime_profiles, replace_environments,
    reveal_runtime_profile_secret, update_app_settings, update_runtime_profile,
};
use crate::handlers::health::health_check;
use crate::request_context::RequestContext;
use crate::response::{ApiError, ApiErrorBody};
use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health_check))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/forgot-password", post(forgot_password))
        .route("/api/auth/reset-password", post(reset_password))
        .route("/api/auth/accept-invite", post(accept_invite))
        .route("/api/apps", get(list_apps).post(create_app))
        .route("/api/apps/{appId}", get(get_app))
        .route("/api/apps/{appId}/settings", patch(update_app_settings))
        .route(
            "/api/apps/{appId}/members",
            get(list_members).post(assign_member),
        )
        .route("/api/apps/{appId}/invites", post(create_invite))
        .route(
            "/api/apps/{appId}/workspaces",
            get(not_implemented).post(not_implemented),
        )
        .route(
            "/api/apps/{appId}/workspaces/{workspaceId}",
            get(not_implemented).patch(not_implemented),
        )
        .route(
            "/api/apps/{appId}/workspaces/{workspaceId}/reset",
            post(not_implemented),
        )
        .route(
            "/api/apps/{appId}/workspaces/{workspaceId}/sync-integration",
            post(not_implemented),
        )
        .route(
            "/api/apps/{appId}/workspaces/{workspaceId}/files",
            get(not_implemented)
                .put(not_implemented)
                .delete(not_implemented),
        )
        .route(
            "/api/apps/{appId}/workspaces/{workspaceId}/checkpoints",
            post(not_implemented),
        )
        .route(
            "/api/apps/{appId}/changesets",
            get(not_implemented).post(not_implemented),
        )
        .route(
            "/api/apps/{appId}/changesets/{changesetId}",
            get(not_implemented).patch(not_implemented),
        )
        .route(
            "/api/apps/{appId}/changesets/{changesetId}/submit",
            post(not_implemented),
        )
        .route(
            "/api/apps/{appId}/changesets/{changesetId}/resubmit",
            post(not_implemented),
        )
        .route(
            "/api/apps/{appId}/changesets/{changesetId}/review",
            post(not_implemented),
        )
        .route(
            "/api/apps/{appId}/changesets/{changesetId}/queue",
            post(not_implemented),
        )
        .route(
            "/api/apps/{appId}/changesets/{changesetId}/move-to-draft",
            post(not_implemented),
        )
        .route(
            "/api/apps/{appId}/changesets/{changesetId}/diff",
            get(not_implemented),
        )
        .route(
            "/api/apps/{appId}/changesets/{changesetId}/comments",
            get(not_implemented).post(not_implemented),
        )
        .route(
            "/api/apps/{appId}/releases",
            get(not_implemented).post(not_implemented),
        )
        .route(
            "/api/apps/{appId}/releases/{releaseId}",
            get(not_implemented),
        )
        .route(
            "/api/apps/{appId}/releases/{releaseId}/changesets",
            post(not_implemented),
        )
        .route(
            "/api/apps/{appId}/releases/{releaseId}/reorder",
            post(not_implemented),
        )
        .route(
            "/api/apps/{appId}/releases/{releaseId}/assemble",
            post(not_implemented),
        )
        .route(
            "/api/apps/{appId}/releases/{releaseId}/publish",
            post(not_implemented),
        )
        .route(
            "/api/apps/{appId}/environments",
            get(list_environments).patch(replace_environments),
        )
        .route(
            "/api/apps/{appId}/runtime-profiles",
            get(list_runtime_profiles).post(create_runtime_profile),
        )
        .route(
            "/api/apps/{appId}/runtime-profiles/{profileId}",
            get(get_runtime_profile).patch(update_runtime_profile),
        )
        .route(
            "/api/apps/{appId}/runtime-profiles/{profileId}/secrets/{key}/reveal",
            post(reveal_runtime_profile_secret),
        )
        .route(
            "/api/apps/{appId}/environments/{envId}/deploy",
            post(not_implemented),
        )
        .route(
            "/api/apps/{appId}/environments/{envId}/promote",
            post(not_implemented),
        )
        .route(
            "/api/apps/{appId}/environments/{envId}/rollback",
            post(not_implemented),
        )
        .route("/api/apps/{appId}/deployments", get(not_implemented))
        .route(
            "/api/apps/{appId}/temp-envs",
            get(not_implemented).post(not_implemented),
        )
        .route(
            "/api/apps/{appId}/temp-envs/{tempEnvId}/extend",
            post(not_implemented),
        )
        .route(
            "/api/apps/{appId}/temp-envs/{tempEnvId}/undo-expire",
            post(not_implemented),
        )
        .route(
            "/api/apps/{appId}/temp-envs/{tempEnvId}",
            delete(not_implemented),
        )
        .route("/api/apps/{appId}/jobs", get(not_implemented))
        .route("/api/apps/{appId}/jobs/{jobId}", get(not_implemented))
        .route(
            "/api/me/notification-preferences",
            get(not_implemented).patch(not_implemented),
        )
        .fallback(fallback_404)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(axum::middleware::from_fn(request_id_middleware))
        .with_state(state)
}

async fn not_implemented() -> impl IntoResponse {
    let body = ApiError {
        error: ApiErrorBody {
            code: "not_implemented",
            message: "This endpoint is not yet implemented.".to_string(),
            request_id: RequestContext::current_request_id(),
        },
    };
    (StatusCode::NOT_IMPLEMENTED, Json(body))
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
    use std::sync::Arc;

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
                jwt_secret: "secret".to_string(),
                jwt_expiry_hours: 24,
                invite_expiry_days: 7,
                secrets_master_key: "master".to_string(),
                temp_url_domain: "example.test".to_string(),
            }),
            db: client.database("conman"),
            git_adapter: Arc::new(NoopGitAdapter),
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
}
