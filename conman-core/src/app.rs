use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BaselineMode {
    IntegrationHead,
    #[default]
    CanonicalEnvRelease,
}

impl std::str::FromStr for BaselineMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "integration_head" => Ok(Self::IntegrationHead),
            "canonical_env_release" => Ok(Self::CanonicalEnvRelease),
            other => Err(format!("invalid baseline_mode: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CommitMode {
    #[default]
    SubmitCommit,
    ManualCheckpoint,
}

impl std::str::FromStr for CommitMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "submit_commit" => Ok(Self::SubmitCommit),
            "manual_checkpoint" => Ok(Self::ManualCheckpoint),
            other => Err(format!("invalid commit_mode_default: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProfileApprovalPolicy {
    SameAsChangeset,
    #[default]
    StricterTwoApprovals,
}

impl std::str::FromStr for ProfileApprovalPolicy {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "same_as_changeset" => Ok(Self::SameAsChangeset),
            "stricter_two_approvals" => Ok(Self::StricterTwoApprovals),
            other => Err(format!("invalid profile_approval_policy: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub baseline_mode: BaselineMode,
    pub canonical_env_id: Option<String>,
    pub commit_mode_default: CommitMode,
    pub blocked_paths: Vec<String>,
    pub file_size_limit_bytes: u64,
    pub profile_approval_policy: ProfileApprovalPolicy,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            baseline_mode: BaselineMode::default(),
            canonical_env_id: None,
            commit_mode_default: CommitMode::default(),
            blocked_paths: vec![
                ".git/**".to_string(),
                ".gitignore".to_string(),
                ".github/**".to_string(),
            ],
            file_size_limit_bytes: 5 * 1024 * 1024,
            profile_approval_policy: ProfileApprovalPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct App {
    pub id: String,
    pub team_id: Option<String>,
    pub name: String,
    pub repo_path: String,
    pub integration_branch: String,
    pub settings: AppSettings,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_scope() {
        let settings = AppSettings::default();
        assert_eq!(settings.baseline_mode, BaselineMode::CanonicalEnvRelease);
        assert_eq!(settings.commit_mode_default, CommitMode::SubmitCommit);
        assert_eq!(
            settings.profile_approval_policy,
            ProfileApprovalPolicy::StricterTwoApprovals
        );
        assert_eq!(settings.file_size_limit_bytes, 5 * 1024 * 1024);
    }
}
