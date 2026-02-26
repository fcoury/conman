use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeProfileKind {
    PersistentEnv,
    TempWorkspace,
    TempChangeset,
}

impl std::str::FromStr for RuntimeProfileKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "persistent_env" => Ok(Self::PersistentEnv),
            "temp_workspace" => Ok(Self::TempWorkspace),
            "temp_changeset" => Ok(Self::TempChangeset),
            other => Err(format!("invalid runtime profile kind: {other}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum EnvVarValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Json(serde_json::Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeProfile {
    pub id: String,
    pub app_id: String,
    pub name: String,
    pub kind: RuntimeProfileKind,
    pub base_url: String,
    pub env_vars: BTreeMap<String, EnvVarValue>,
    pub secrets_encrypted: BTreeMap<String, String>,
    pub database_engine: String,
    pub connection_ref: String,
    pub provisioning_mode: String,
    pub base_profile_id: Option<String>,
    pub migration_paths: Vec<String>,
    pub migration_command: Option<String>,
    pub revision: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub fn mask_secret(raw: &str) -> String {
    let len = raw.chars().count();
    if len <= 4 {
        return "*".repeat(len);
    }

    if len <= 8 {
        let suffix: String = raw
            .chars()
            .rev()
            .take(4)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        return format!("{}{}", "*".repeat(len - 4), suffix);
    }

    let prefix: String = raw.chars().take(4).collect();
    let suffix: String = raw
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{prefix}***{suffix}")
}

#[cfg(test)]
mod tests {
    use super::mask_secret;

    #[test]
    fn secret_masking_is_length_aware() {
        assert_eq!(mask_secret("abcd"), "****");
        assert_eq!(mask_secret("abcdef"), "**cdef");
        assert_eq!(mask_secret("abcdefghijkl"), "abcd***ijkl");
    }
}
