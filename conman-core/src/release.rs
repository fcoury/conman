use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseState {
    DraftRelease,
    Assembling,
    Validated,
    Published,
    DeployedPartial,
    DeployedFull,
    RolledBack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseBatch {
    pub id: String,
    pub repo_id: String,
    pub tag: String,
    pub state: ReleaseState,
    pub ordered_changeset_ids: Vec<String>,
    pub compose_job_id: Option<String>,
    pub published_sha: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub published_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
