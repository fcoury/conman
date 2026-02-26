use chrono::{DateTime, Utc};
use conman_core::{ConmanError, User};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};

use crate::EnsureIndexes;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    email: String,
    password_hash: String,
    name: String,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    updated_at: DateTime<Utc>,
}

impl From<UserDoc> for User {
    fn from(value: UserDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            email: value.email,
            password_hash: value.password_hash,
            name: value.name,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Clone)]
pub struct UserRepo {
    collection: Collection<UserDoc>,
}

impl UserRepo {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("users"),
        }
    }

    pub async fn insert(
        &self,
        email: &str,
        name: &str,
        password_hash: &str,
    ) -> Result<User, ConmanError> {
        let now = Utc::now();
        let doc = UserDoc {
            id: ObjectId::new(),
            email: email.to_lowercase(),
            password_hash: password_hash.to_string(),
            name: name.to_string(),
            created_at: now,
            updated_at: now,
        };

        self.collection
            .insert_one(doc.clone())
            .await
            .map_err(|e| ConmanError::Conflict {
                message: format!("failed to insert user: {e}"),
            })?;

        Ok(doc.into())
    }

    pub async fn find_by_email(&self, email: &str) -> Result<Option<User>, ConmanError> {
        let user = self
            .collection
            .find_one(doc! { "email": email.to_lowercase() })
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to query user by email: {e}"),
            })?;

        Ok(user.map(Into::into))
    }

    pub async fn find_by_id(&self, user_id: &str) -> Result<Option<User>, ConmanError> {
        let id = ObjectId::parse_str(user_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid user_id: {e}"),
        })?;

        let user = self
            .collection
            .find_one(doc! { "_id": id })
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to query user by id: {e}"),
            })?;

        Ok(user.map(Into::into))
    }

    pub async fn update_password(
        &self,
        user_id: &str,
        password_hash: &str,
    ) -> Result<(), ConmanError> {
        let id = ObjectId::parse_str(user_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid user_id: {e}"),
        })?;

        self.collection
            .update_one(
                doc! {"_id": id},
                doc! {"$set": {"password_hash": password_hash, "updated_at": Utc::now()}},
            )
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to update password: {e}"),
            })?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for UserRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let model = IndexModel::builder()
            .keys(doc! {"email": 1})
            .options(
                IndexOptions::builder()
                    .unique(true)
                    .name("users_email_unique".to_string())
                    .build(),
            )
            .build();

        self.collection
            .create_index(model)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure user indexes: {e}"),
            })?;

        Ok(())
    }
}
