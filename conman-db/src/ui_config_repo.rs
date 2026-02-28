use chrono::{DateTime, Utc};
use conman_core::{ConmanError, UiConfig};
use mongodb::{
    Collection, Database,
    bson::{doc, oid::ObjectId},
};
use serde::{Deserialize, Serialize};

use crate::EnsureIndexes;

const DEFAULT_UI_CONFIG_ID: &str = "default";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UiConfigDoc {
    #[serde(rename = "_id")]
    id: String,
    repo_id: ObjectId,
    configured_by: ObjectId,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    configured_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    updated_at: DateTime<Utc>,
}

impl From<UiConfigDoc> for UiConfig {
    fn from(value: UiConfigDoc) -> Self {
        Self {
            id: value.id,
            repo_id: value.repo_id.to_hex(),
            configured_by: value.configured_by.to_hex(),
            configured_at: value.configured_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Clone)]
pub struct UiConfigRepo {
    collection: Collection<UiConfigDoc>,
}

impl UiConfigRepo {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("ui_config"),
        }
    }

    pub async fn get_default(&self) -> Result<Option<UiConfig>, ConmanError> {
        let row = self
            .collection
            .find_one(doc! {"_id": DEFAULT_UI_CONFIG_ID})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to load ui config: {e}"),
            })?;
        Ok(row.map(Into::into))
    }

    pub async fn set_default(
        &self,
        repo_id: &str,
        configured_by: &str,
    ) -> Result<UiConfig, ConmanError> {
        let repo_id_obj = ObjectId::parse_str(repo_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid repo_id: {e}"),
        })?;
        let configured_by_obj =
            ObjectId::parse_str(configured_by).map_err(|e| ConmanError::Validation {
                message: format!("invalid configured_by: {e}"),
            })?;

        let now = Utc::now();
        let row = self
            .collection
            .find_one_and_update(
                doc! {"_id": DEFAULT_UI_CONFIG_ID},
                doc! {
                    "$set": {
                        "repo_id": repo_id_obj,
                        "configured_by": configured_by_obj,
                        "updated_at": now,
                    },
                    "$setOnInsert": {
                        "configured_at": now,
                    }
                },
            )
            .upsert(true)
            .return_document(mongodb::options::ReturnDocument::After)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to set ui config: {e}"),
            })?
            .ok_or_else(|| ConmanError::Internal {
                message: "failed to read ui config after update".to_string(),
            })?;

        Ok(row.into())
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for UiConfigRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        Ok(())
    }
}

