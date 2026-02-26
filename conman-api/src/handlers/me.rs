use axum::{Extension, Json, extract::State};
use conman_auth::AuthUser;
use conman_core::NotificationPreference;
use serde::Deserialize;

use crate::{error::ApiConmanError, events::emit_audit, response::ApiResponse, state::AppState};

#[derive(Debug, Deserialize)]
pub struct UpdateNotificationPreferenceRequest {
    pub email_enabled: bool,
}

pub async fn get_notification_preferences(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> Result<Json<ApiResponse<NotificationPreference>>, ApiConmanError> {
    let pref = conman_db::NotificationPreferenceRepo::new(state.db.clone())
        .get_or_create(&auth.user_id)
        .await?;
    Ok(Json(ApiResponse::ok(pref)))
}

pub async fn update_notification_preferences(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(req): Json<UpdateNotificationPreferenceRequest>,
) -> Result<Json<ApiResponse<NotificationPreference>>, ApiConmanError> {
    let pref = conman_db::NotificationPreferenceRepo::new(state.db.clone())
        .set_email_enabled(&auth.user_id, req.email_enabled)
        .await?;
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        None,
        "notification_preference",
        &pref.id,
        "updated",
        None,
        serde_json::to_value(&pref).ok(),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }
    Ok(Json(ApiResponse::ok(pref)))
}
