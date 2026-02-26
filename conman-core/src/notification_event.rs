use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationState {
    Queued,
    Sent,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationEvent {
    pub id: String,
    pub user_id: String,
    pub app_id: Option<String>,
    pub event_type: String,
    pub subject: String,
    pub body: String,
    pub state: NotificationState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
