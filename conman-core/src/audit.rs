use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRequestContext {
    pub request_id: String,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: String,
    pub occurred_at: DateTime<Utc>,
    pub actor_user_id: Option<String>,
    pub repo_id: Option<String>,
    pub entity_type: String,
    pub entity_id: String,
    pub action: String,
    pub before: Option<serde_json::Value>,
    pub after: Option<serde_json::Value>,
    pub git_sha: Option<String>,
    pub context: AuditRequestContext,
}
