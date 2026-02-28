use chrono::{DateTime, Duration, Utc};
use conman_core::{ConmanError, TempEnvKind, TempEnvState, TempEnvironment};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::EnsureIndexes;

const SLUG_WORDS: &[&str] = &[
    "amber", "atlas", "banyan", "birch", "brisk", "cedar", "cobalt", "coral", "crisp", "dawn",
    "delta", "ember", "fable", "fjord", "flint", "forest", "glade", "golden", "harbor", "hazel",
    "helium", "indigo", "jaguar", "juniper", "kepler", "lagoon", "linen", "lumen", "maple",
    "meadow", "merlin", "mint", "nebula", "noble", "nova", "onyx", "opal", "orchid", "orbit",
    "otter", "pearl", "pine", "pluto", "prairie", "quartz", "rain", "raven", "ridge", "river",
    "sable", "sage", "scarlet", "sequoia", "solstice", "spruce", "summit", "sunny", "tango",
    "topaz", "valley", "velvet", "violet", "willow", "zephyr",
];

fn readable_slug(uuid: Uuid) -> String {
    let bytes = uuid.into_bytes();
    let first = SLUG_WORDS[usize::from(bytes[0]) % SLUG_WORDS.len()];
    let second = SLUG_WORDS[usize::from(bytes[1]) % SLUG_WORDS.len()];
    let suffix = format!("{:02x}{:02x}", bytes[14], bytes[15]);
    format!("{first}-{second}-{suffix}")
}

fn app_slug_prefix(repo_id: &str) -> String {
    repo_id.chars().take(8).collect::<String>()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TempEnvDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    repo_id: ObjectId,
    kind: TempEnvKind,
    source_id: String,
    owner_user_id: ObjectId,
    state: TempEnvState,
    base_profile_id: Option<ObjectId>,
    runtime_profile_id: Option<ObjectId>,
    url: String,
    db_name: String,
    idle_ttl_seconds: i64,
    grace_ttl_seconds: i64,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    last_activity_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    expires_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime_optional")]
    grace_expires_at: Option<DateTime<Utc>>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    updated_at: DateTime<Utc>,
}

impl From<TempEnvDoc> for TempEnvironment {
    fn from(value: TempEnvDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            repo_id: value.repo_id.to_hex(),
            kind: value.kind,
            source_id: value.source_id,
            owner_user_id: value.owner_user_id.to_hex(),
            state: value.state,
            base_profile_id: value.base_profile_id.map(|v| v.to_hex()),
            runtime_profile_id: value.runtime_profile_id.map(|v| v.to_hex()),
            url: value.url,
            db_name: value.db_name,
            idle_ttl_seconds: value.idle_ttl_seconds,
            grace_ttl_seconds: value.grace_ttl_seconds,
            last_activity_at: value.last_activity_at,
            expires_at: value.expires_at,
            grace_expires_at: value.grace_expires_at,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreateTempEnvInput {
    pub repo_id: String,
    pub kind: TempEnvKind,
    pub source_id: String,
    pub owner_user_id: String,
    pub base_profile_id: Option<String>,
    pub runtime_profile_id: Option<String>,
    pub url_domain: String,
}

#[derive(Clone)]
pub struct TempEnvRepo {
    collection: Collection<TempEnvDoc>,
}

impl TempEnvRepo {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("temp_environments"),
        }
    }

    pub async fn create(&self, input: CreateTempEnvInput) -> Result<TempEnvironment, ConmanError> {
        let repo_id = ObjectId::parse_str(&input.repo_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid repo_id: {e}"),
        })?;
        let owner_user_id =
            ObjectId::parse_str(&input.owner_user_id).map_err(|e| ConmanError::Validation {
                message: format!("invalid owner_user_id: {e}"),
            })?;
        let base_profile_id = input
            .base_profile_id
            .as_deref()
            .map(ObjectId::parse_str)
            .transpose()
            .map_err(|e| ConmanError::Validation {
                message: format!("invalid base_profile_id: {e}"),
            })?;
        let runtime_profile_id = input
            .runtime_profile_id
            .as_deref()
            .map(ObjectId::parse_str)
            .transpose()
            .map_err(|e| ConmanError::Validation {
                message: format!("invalid runtime_profile_id: {e}"),
            })?;

        let now = Utc::now();
        let idle_ttl_seconds = 24 * 3600;
        let grace_ttl_seconds = 3600;
        let short = readable_slug(Uuid::now_v7());
        let kind_label = match input.kind {
            TempEnvKind::Workspace => "ws",
            TempEnvKind::Changeset => "cs",
        };
        let url = format!(
            "{}-{kind_label}-{short}.{}",
            app_slug_prefix(&input.repo_id),
            input.url_domain
        );
        let db_name = format!("tmp_{}_{}", input.repo_id, short);

        let row = TempEnvDoc {
            id: ObjectId::new(),
            repo_id,
            kind: input.kind,
            source_id: input.source_id,
            owner_user_id,
            state: TempEnvState::Provisioning,
            base_profile_id,
            runtime_profile_id,
            url,
            db_name,
            idle_ttl_seconds,
            grace_ttl_seconds,
            last_activity_at: now,
            expires_at: now + Duration::seconds(idle_ttl_seconds),
            grace_expires_at: None,
            created_at: now,
            updated_at: now,
        };
        self.collection
            .insert_one(row.clone())
            .await
            .map_err(|e| ConmanError::Conflict {
                message: format!("failed to create temp environment: {e}"),
            })?;
        Ok(row.into())
    }

    pub async fn list_by_repo(
        &self,
        repo_id: &str,
        skip: u64,
        limit: u64,
    ) -> Result<(Vec<TempEnvironment>, u64), ConmanError> {
        let repo_id = ObjectId::parse_str(repo_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid repo_id: {e}"),
        })?;
        let filter = doc! {"repo_id": repo_id};
        let total = self
            .collection
            .count_documents(filter.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to count temp environments: {e}"),
            })?;
        let mut cursor = self
            .collection
            .find(filter)
            .sort(doc! {"created_at": -1})
            .skip(skip)
            .limit(limit as i64)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to list temp environments: {e}"),
            })?;
        let mut rows = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("temp env cursor error: {e}"),
        })? {
            let row: TempEnvDoc =
                cursor
                    .deserialize_current()
                    .map_err(|e| ConmanError::Internal {
                        message: format!("failed to decode temp env row: {e}"),
                    })?;
            rows.push(row.into());
        }
        Ok((rows, total))
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<TempEnvironment>, ConmanError> {
        let id = ObjectId::parse_str(id).map_err(|e| ConmanError::Validation {
            message: format!("invalid temp_env_id: {e}"),
        })?;
        let row = self
            .collection
            .find_one(doc! {"_id": id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to find temp environment: {e}"),
            })?;
        Ok(row.map(Into::into))
    }

    pub async fn set_state(
        &self,
        id: &str,
        state: TempEnvState,
        grace_expires_at: Option<DateTime<Utc>>,
    ) -> Result<TempEnvironment, ConmanError> {
        let id = ObjectId::parse_str(id).map_err(|e| ConmanError::Validation {
            message: format!("invalid temp_env_id: {e}"),
        })?;
        let state_bson = mongodb::bson::to_bson(&state).map_err(|e| ConmanError::Internal {
            message: format!("failed to encode temp env state: {e}"),
        })?;
        let now = Utc::now();
        let row = self
            .collection
            .find_one_and_update(
                doc! {"_id": id},
                doc! {"$set": {"state": state_bson, "grace_expires_at": grace_expires_at, "updated_at": now}},
            )
            .return_document(mongodb::options::ReturnDocument::After)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to set temp env state: {e}"),
            })?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "temp_environment",
                id: id.to_hex(),
            })?;
        Ok(row.into())
    }

    pub async fn extend_ttl(&self, id: &str, seconds: i64) -> Result<TempEnvironment, ConmanError> {
        let id = ObjectId::parse_str(id).map_err(|e| ConmanError::Validation {
            message: format!("invalid temp_env_id: {e}"),
        })?;
        let now = Utc::now();
        let row = self
            .collection
            .find_one_and_update(
                doc! {"_id": id},
                doc! {"$set": {"last_activity_at": now, "expires_at": now + Duration::seconds(seconds), "updated_at": now}},
            )
            .return_document(mongodb::options::ReturnDocument::After)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to extend temp env ttl: {e}"),
            })?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "temp_environment",
                id: id.to_hex(),
            })?;
        Ok(row.into())
    }

    pub async fn touch_activity(&self, id: &str) -> Result<TempEnvironment, ConmanError> {
        let id = ObjectId::parse_str(id).map_err(|e| ConmanError::Validation {
            message: format!("invalid temp_env_id: {e}"),
        })?;

        let current = self
            .collection
            .find_one(doc! {"_id": id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to load temp env for touch: {e}"),
            })?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "temp_environment",
                id: id.to_hex(),
            })?;

        let now = Utc::now();
        let row = self
            .collection
            .find_one_and_update(
                doc! {"_id": id},
                doc! {"$set": {"last_activity_at": now, "expires_at": now + Duration::seconds(current.idle_ttl_seconds), "updated_at": now}},
            )
            .return_document(mongodb::options::ReturnDocument::After)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to touch temp env activity: {e}"),
            })?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "temp_environment",
                id: id.to_hex(),
            })?;
        Ok(row.into())
    }

    pub async fn list_due_for_expiry_scan(
        &self,
        limit: i64,
    ) -> Result<Vec<TempEnvironment>, ConmanError> {
        let now = Utc::now();
        let active =
            mongodb::bson::to_bson(&TempEnvState::Active).map_err(|e| ConmanError::Internal {
                message: format!("failed to encode active temp env state: {e}"),
            })?;
        let provisioning = mongodb::bson::to_bson(&TempEnvState::Provisioning).map_err(|e| {
            ConmanError::Internal {
                message: format!("failed to encode provisioning temp env state: {e}"),
            }
        })?;
        let expiring =
            mongodb::bson::to_bson(&TempEnvState::Expiring).map_err(|e| ConmanError::Internal {
                message: format!("failed to encode expiring temp env state: {e}"),
            })?;
        let deleted =
            mongodb::bson::to_bson(&TempEnvState::Deleted).map_err(|e| ConmanError::Internal {
                message: format!("failed to encode deleted temp env state: {e}"),
            })?;
        let expired =
            mongodb::bson::to_bson(&TempEnvState::Expired).map_err(|e| ConmanError::Internal {
                message: format!("failed to encode expired temp env state: {e}"),
            })?;

        let filter = doc! {
            "$or": [
                {
                    "state": {"$in": [active, provisioning]},
                    "expires_at": {"$lte": now}
                },
                {
                    "state": {"$in": [expiring, deleted, expired]},
                    "grace_expires_at": {"$ne": mongodb::bson::Bson::Null, "$lte": now}
                }
            ]
        };
        let mut cursor = self
            .collection
            .find(filter)
            .sort(doc! {"updated_at": 1})
            .limit(limit)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to scan due temp environments: {e}"),
            })?;
        let mut rows = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("temp env due scan cursor error: {e}"),
        })? {
            let row: TempEnvDoc =
                cursor
                    .deserialize_current()
                    .map_err(|e| ConmanError::Internal {
                        message: format!("failed to decode due temp env: {e}"),
                    })?;
            rows.push(row.into());
        }
        Ok(rows)
    }

    pub async fn hard_delete(&self, id: &str) -> Result<(), ConmanError> {
        let id = ObjectId::parse_str(id).map_err(|e| ConmanError::Validation {
            message: format!("invalid temp_env_id: {e}"),
        })?;
        self.collection
            .delete_one(doc! {"_id": id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to hard-delete temp environment: {e}"),
            })?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for TempEnvRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let by_app = IndexModel::builder()
            .keys(doc! {"repo_id": 1, "created_at": -1})
            .options(
                IndexOptions::builder()
                    .name("temp_env_app_created".to_string())
                    .build(),
            )
            .build();
        let by_owner = IndexModel::builder()
            .keys(doc! {"repo_id": 1, "owner_user_id": 1, "state": 1})
            .options(
                IndexOptions::builder()
                    .name("temp_env_owner_state".to_string())
                    .build(),
            )
            .build();
        let by_expiry = IndexModel::builder()
            .keys(doc! {"state": 1, "expires_at": 1, "grace_expires_at": 1})
            .options(
                IndexOptions::builder()
                    .name("temp_env_expiry_scan".to_string())
                    .build(),
            )
            .build();
        self.collection
            .create_indexes(vec![by_app, by_owner, by_expiry])
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure temp env indexes: {e}"),
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readable_slug_is_human_friendly() {
        let slug = readable_slug(Uuid::now_v7());
        assert!(slug.contains('-'));
        assert!(slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    }

    #[test]
    fn app_slug_prefix_limits_length() {
        assert_eq!(app_slug_prefix("1234567890abcdef"), "12345678");
        assert_eq!(app_slug_prefix("abc"), "abc");
    }
}
