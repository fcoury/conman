use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaseRefType {
    Branch,
    Tag,
    Commit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub repo_id: String,
    pub owner_user_id: String,
    pub branch_name: String,
    pub title: Option<String>,
    pub is_default: bool,
    pub base_ref_type: BaseRefType,
    pub base_ref_value: String,
    pub base_sha: String,
    pub head_sha: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileEntryType {
    File,
    Dir,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    #[serde(rename = "type")]
    pub entry_type: FileEntryType,
    pub size: i64,
    pub oid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictStatus {
    pub clean: bool,
    pub head_sha: String,
    pub conflicting_paths: Vec<String>,
    pub message: String,
}
