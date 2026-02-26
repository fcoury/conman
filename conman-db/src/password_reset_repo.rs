use chrono::{DateTime, Duration, Utc};
use conman_core::{ConmanError, PasswordResetToken};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::EnsureIndexes;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PasswordResetDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    user_id: ObjectId,
    token: String,
    expires_at: DateTime<Utc>,
    used_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl From<PasswordResetDoc> for PasswordResetToken {
    fn from(value: PasswordResetDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            user_id: value.user_id.to_hex(),
            token: value.token,
            expires_at: value.expires_at,
            used_at: value.used_at,
            created_at: value.created_at,
        }
    }
}

#[derive(Clone)]
pub struct PasswordResetRepo {
    collection: Collection<PasswordResetDoc>,
}

impl PasswordResetRepo {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("password_reset_tokens"),
        }
    }

    pub async fn create(
        &self,
        user_id: &str,
        expiry_minutes: i64,
    ) -> Result<PasswordResetToken, ConmanError> {
        let user_id = ObjectId::parse_str(user_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid user_id: {e}"),
        })?;
        let now = Utc::now();
        let doc = PasswordResetDoc {
            id: ObjectId::new(),
            user_id,
            token: Uuid::now_v7().to_string(),
            expires_at: now + Duration::minutes(expiry_minutes),
            used_at: None,
            created_at: now,
        };

        self.collection
            .insert_one(doc.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to create password reset token: {e}"),
            })?;
        Ok(doc.into())
    }

    pub async fn find_active_by_token(
        &self,
        token: &str,
    ) -> Result<Option<PasswordResetToken>, ConmanError> {
        let now = Utc::now();
        let row = self
            .collection
            .find_one(doc! {
                "token": token,
                "used_at": null,
                "expires_at": {"$gt": mongodb::bson::DateTime::from_millis(now.timestamp_millis())}
            })
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to query password reset token: {e}"),
            })?;
        Ok(row.map(Into::into))
    }

    pub async fn mark_used(&self, reset_id: &str) -> Result<(), ConmanError> {
        let reset_id = ObjectId::parse_str(reset_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid reset token id: {e}"),
        })?;
        self.collection
            .update_one(
                doc! {"_id": reset_id, "used_at": null},
                doc! {"$set": {"used_at": mongodb::bson::DateTime::from_millis(Utc::now().timestamp_millis())}},
            )
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to mark password reset token used: {e}"),
            })?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for PasswordResetRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let token_idx = IndexModel::builder()
            .keys(doc! {"token": 1})
            .options(
                IndexOptions::builder()
                    .name("password_reset_token_unique".to_string())
                    .unique(true)
                    .build(),
            )
            .build();
        let user_idx = IndexModel::builder()
            .keys(doc! {"user_id": 1, "created_at": -1})
            .options(
                IndexOptions::builder()
                    .name("password_reset_user_created_at".to_string())
                    .build(),
            )
            .build();
        self.collection
            .create_indexes(vec![token_idx, user_idx])
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure password reset indexes: {e}"),
            })?;
        Ok(())
    }
}
