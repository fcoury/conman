use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub id: String,
    pub repo_id: String,
    pub configured_by: String,
    pub configured_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
