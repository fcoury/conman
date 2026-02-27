use chrono::Utc;
use conman_core::{AuditEvent, AuditRequestContext, ConmanError};

use crate::{request_context::RequestContext, state::AppState};

#[allow(clippy::too_many_arguments)]
pub async fn emit_audit(
    state: &AppState,
    actor_user_id: Option<&str>,
    app_id: Option<&str>,
    entity_type: &str,
    entity_id: &str,
    action: &str,
    before: Option<serde_json::Value>,
    after: Option<serde_json::Value>,
    git_sha: Option<&str>,
) -> Result<(), ConmanError> {
    let ctx = RequestContext::current().unwrap_or_default();
    let event = AuditEvent {
        id: String::new(),
        occurred_at: Utc::now(),
        actor_user_id: actor_user_id.map(ToString::to_string),
        app_id: app_id.map(ToString::to_string),
        entity_type: entity_type.to_string(),
        entity_id: entity_id.to_string(),
        action: action.to_string(),
        before,
        after,
        git_sha: git_sha.map(ToString::to_string),
        context: AuditRequestContext {
            request_id: ctx.request_id,
            ip: ctx.client_ip,
            user_agent: ctx.user_agent,
        },
    };
    conman_db::AuditRepo::new(state.db.clone())
        .emit(event)
        .await
}

pub async fn emit_notification(
    state: &AppState,
    user_id: &str,
    app_id: Option<&str>,
    event_type: &str,
    subject: &str,
    body: &str,
) -> Result<(), ConmanError> {
    let pref = conman_db::NotificationPreferenceRepo::new(state.db.clone())
        .get_or_create(user_id)
        .await?;
    if !pref.email_enabled {
        return Ok(());
    }
    let user = conman_db::UserRepo::new(state.db.clone())
        .find_by_id(user_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "member",
            id: user_id.to_string(),
        })?;
    conman_db::NotificationEventRepo::new(state.db.clone())
        .enqueue(user_id, &user.email, app_id, event_type, subject, body)
        .await?;
    Ok(())
}
