use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentState {
    Pending,
    Running,
    Succeeded,
    Failed,
    Canceled,
}

impl DeploymentState {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Pending | Self::Running)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RollbackMode {
    RevertAndRelease,
    RedeployPriorTag,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deployment {
    pub id: String,
    pub app_id: String,
    pub environment_id: String,
    pub release_id: String,
    pub state: DeploymentState,
    pub is_skip_stage: bool,
    pub is_concurrent_batch: bool,
    pub approvals: Vec<String>,
    pub job_id: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
