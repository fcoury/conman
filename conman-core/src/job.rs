use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    MsuiteSubmit,
    MsuiteMerge,
    MsuiteDeploy,
    RevalidateQueuedChangeset,
    ReleaseAssemble,
    DeployRelease,
    RuntimeProfileDriftCheck,
    TempEnvProvision,
    TempEnvExpire,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Canceled,
}

impl JobState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Canceled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub app_id: String,
    pub job_type: JobType,
    pub state: JobState,
    pub entity_type: String,
    pub entity_id: String,
    pub payload: serde_json::Value,
    pub result: Option<serde_json::Value>,
    pub error_message: Option<String>,
    pub retry_count: u32,
    pub max_retries: u32,
    pub timeout_ms: u64,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobLogLine {
    pub id: String,
    pub app_id: String,
    pub job_id: String,
    pub level: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
}
