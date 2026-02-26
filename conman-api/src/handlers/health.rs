use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Serialize;
use tonic::transport::Endpoint;

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub components: Vec<HealthComponent>,
}

#[derive(Debug, Serialize)]
pub struct HealthComponent {
    pub name: &'static str,
    pub status: &'static str,
}

pub async fn health_check(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let mongo_ok = conman_db::check_mongo_health(&state.db).await.is_ok();
    let gitaly_ok = if let Ok(endpoint) = Endpoint::from_shared(state.config.gitaly_address.clone())
    {
        match tokio::time::timeout(
            std::time::Duration::from_secs(3),
            endpoint
                .connect_timeout(std::time::Duration::from_secs(2))
                .connect(),
        )
        .await
        {
            Ok(Ok(_)) => true,
            _ => false,
        }
    } else {
        false
    };

    let components = vec![
        HealthComponent {
            name: "mongo",
            status: if mongo_ok { "healthy" } else { "unhealthy" },
        },
        HealthComponent {
            name: "gitaly",
            status: if gitaly_ok { "healthy" } else { "unhealthy" },
        },
        HealthComponent {
            name: "job_runner",
            status: "healthy",
        },
    ];

    if mongo_ok && gitaly_ok {
        (
            StatusCode::OK,
            Json(HealthResponse {
                status: "ok",
                components,
            }),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "degraded",
                components,
            }),
        )
    }
}
