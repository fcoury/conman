use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use conman_core::ConmanError;

use crate::request_context::RequestContext;
use crate::response::{ApiError, ApiErrorBody};

#[derive(Debug)]
pub struct ApiConmanError(pub ConmanError);

impl From<ConmanError> for ApiConmanError {
    fn from(value: ConmanError) -> Self {
        Self(value)
    }
}

impl IntoResponse for ApiConmanError {
    fn into_response(self) -> Response {
        let (status, code) = match &self.0 {
            ConmanError::NotFound { .. } => (StatusCode::NOT_FOUND, "not_found"),
            ConmanError::Conflict { .. } => (StatusCode::CONFLICT, "conflict"),
            ConmanError::Forbidden { .. } => (StatusCode::FORBIDDEN, "forbidden"),
            ConmanError::Unauthorized { .. } => (StatusCode::UNAUTHORIZED, "unauthorized"),
            ConmanError::Validation { .. } => (StatusCode::BAD_REQUEST, "validation_error"),
            ConmanError::InvalidTransition { .. } => (StatusCode::CONFLICT, "invalid_transition"),
            ConmanError::Git { .. } => (StatusCode::BAD_GATEWAY, "git_error"),
            ConmanError::Internal { .. } => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };

        let body = ApiError {
            error: ApiErrorBody {
                code,
                message: self.0.to_string(),
                request_id: RequestContext::current_request_id(),
            },
        };

        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use axum::response::IntoResponse;

    use super::*;

    #[tokio::test]
    async fn validation_maps_to_400() {
        let err = ConmanError::Validation {
            message: "bad input".to_string(),
        };

        let response = ApiConmanError(err).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("bytes");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(json["error"]["code"], "validation_error");
        assert!(json["error"]["request_id"].is_string());
    }
}
