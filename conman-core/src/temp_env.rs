use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TempEnvKind {
    Workspace,
    Changeset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TempEnvState {
    Provisioning,
    Active,
    Expiring,
    Expired,
    Deleted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempEnvironment {
    pub id: String,
    pub app_id: String,
    pub kind: TempEnvKind,
    pub source_id: String,
    pub owner_user_id: String,
    pub state: TempEnvState,
    pub base_profile_id: Option<String>,
    pub runtime_profile_id: Option<String>,
    pub url: String,
    pub db_name: String,
    pub idle_ttl_seconds: i64,
    pub grace_ttl_seconds: i64,
    pub last_activity_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub grace_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
