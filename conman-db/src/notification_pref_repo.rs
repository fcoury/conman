use chrono::{DateTime, Utc};
use conman_core::{ConmanError, NotificationPreference};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};

use crate::EnsureIndexes;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NotificationPrefDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    user_id: ObjectId,
    email_enabled: bool,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    updated_at: DateTime<Utc>,
}

impl From<NotificationPrefDoc> for NotificationPreference {
    fn from(value: NotificationPrefDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            user_id: value.user_id.to_hex(),
            email_enabled: value.email_enabled,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Clone)]
pub struct NotificationPreferenceRepo {
    collection: Collection<NotificationPrefDoc>,
}

impl NotificationPreferenceRepo {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("notification_preferences"),
        }
    }

    pub async fn get_or_create(
        &self,
        user_id: &str,
    ) -> Result<NotificationPreference, ConmanError> {
        let user_id = ObjectId::parse_str(user_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid user_id: {e}"),
        })?;
        if let Some(row) = self
            .collection
            .find_one(doc! {"user_id": user_id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to fetch notification preference: {e}"),
            })?
        {
            return Ok(row.into());
        }
        let now = Utc::now();
        let row = NotificationPrefDoc {
            id: ObjectId::new(),
            user_id,
            email_enabled: true,
            created_at: now,
            updated_at: now,
        };
        self.collection
            .insert_one(row.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to create notification preference: {e}"),
            })?;
        Ok(row.into())
    }

    pub async fn set_email_enabled(
        &self,
        user_id: &str,
        enabled: bool,
    ) -> Result<NotificationPreference, ConmanError> {
        let user_id = ObjectId::parse_str(user_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid user_id: {e}"),
        })?;
        let now = Utc::now();
        let row = self
            .collection
            .find_one_and_update(
                doc! {"user_id": user_id},
                doc! {"$set": {"email_enabled": enabled, "updated_at": now}},
            )
            .upsert(true)
            .return_document(mongodb::options::ReturnDocument::After)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to update notification preference: {e}"),
            })?
            .ok_or_else(|| ConmanError::Internal {
                message: "failed to upsert notification preference".to_string(),
            })?;
        Ok(row.into())
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for NotificationPreferenceRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        self.collection
            .create_index(
                IndexModel::builder()
                    .keys(doc! {"user_id": 1})
                    .options(
                        IndexOptions::builder()
                            .name("notification_pref_user_unique".to_string())
                            .unique(true)
                            .build(),
                    )
                    .build(),
            )
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure notification preference indexes: {e}"),
            })?;
        Ok(())
    }
}
