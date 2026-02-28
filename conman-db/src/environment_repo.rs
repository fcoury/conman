use chrono::{DateTime, Utc};
use conman_core::{ConmanError, Environment};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};

use crate::EnsureIndexes;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EnvironmentDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    repo_id: ObjectId,
    name: String,
    position: u32,
    is_canonical: bool,
    runtime_profile_id: Option<ObjectId>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    updated_at: DateTime<Utc>,
}

impl From<EnvironmentDoc> for Environment {
    fn from(value: EnvironmentDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            repo_id: value.repo_id.to_hex(),
            name: value.name,
            position: value.position,
            is_canonical: value.is_canonical,
            runtime_profile_id: value.runtime_profile_id.map(|id| id.to_hex()),
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EnvironmentInput {
    pub name: String,
    pub position: u32,
    pub is_canonical: bool,
    pub runtime_profile_id: Option<String>,
}

#[derive(Clone)]
pub struct EnvironmentRepo {
    collection: Collection<EnvironmentDoc>,
}

impl EnvironmentRepo {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("environments"),
        }
    }

    pub async fn list_by_repo(&self, repo_id: &str) -> Result<Vec<Environment>, ConmanError> {
        let repo_id = ObjectId::parse_str(repo_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid repo_id: {e}"),
        })?;

        let mut cursor = self
            .collection
            .find(doc! {"repo_id": repo_id})
            .sort(doc! {"position": 1})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to list environments: {e}"),
            })?;

        let mut items = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("environment cursor error: {e}"),
        })? {
            let env: EnvironmentDoc =
                cursor
                    .deserialize_current()
                    .map_err(|e| ConmanError::Internal {
                        message: format!("failed to decode environment: {e}"),
                    })?;
            items.push(env.into());
        }

        Ok(items)
    }

    pub async fn replace_all(
        &self,
        repo_id: &str,
        entries: &[EnvironmentInput],
    ) -> Result<Vec<Environment>, ConmanError> {
        let app_id_obj = ObjectId::parse_str(repo_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid repo_id: {e}"),
        })?;

        let canonical_count = entries.iter().filter(|e| e.is_canonical).count();
        if canonical_count > 1 {
            return Err(ConmanError::Validation {
                message: "only one environment can be canonical".to_string(),
            });
        }

        self.collection
            .delete_many(doc! {"repo_id": app_id_obj})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to clear existing environments: {e}"),
            })?;

        let now = Utc::now();
        let docs = entries
            .iter()
            .map(|entry| {
                let runtime_profile_id = entry
                    .runtime_profile_id
                    .as_deref()
                    .and_then(|id| ObjectId::parse_str(id).ok());
                EnvironmentDoc {
                    id: ObjectId::new(),
                    repo_id: app_id_obj,
                    name: entry.name.clone(),
                    position: entry.position,
                    is_canonical: entry.is_canonical,
                    runtime_profile_id,
                    created_at: now,
                    updated_at: now,
                }
            })
            .collect::<Vec<_>>();

        if !docs.is_empty() {
            self.collection
                .insert_many(docs)
                .await
                .map_err(|e| ConmanError::Internal {
                    message: format!("failed to insert environments: {e}"),
                })?;
        }

        self.list_by_repo(repo_id).await
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for EnvironmentRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let by_app = IndexModel::builder()
            .keys(doc! {"repo_id": 1, "position": 1})
            .options(
                IndexOptions::builder()
                    .name("environments_app_position".to_string())
                    .build(),
            )
            .build();
        let uniq_name = IndexModel::builder()
            .keys(doc! {"repo_id": 1, "name": 1})
            .options(
                IndexOptions::builder()
                    .name("environments_app_name_unique".to_string())
                    .unique(true)
                    .build(),
            )
            .build();

        self.collection
            .create_indexes(vec![by_app, uniq_name])
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure environment indexes: {e}"),
            })?;
        Ok(())
    }
}
