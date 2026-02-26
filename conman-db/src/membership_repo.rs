use std::collections::HashMap;

use chrono::{DateTime, Utc};
use conman_core::{AppMembership, ConmanError, Role};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};

use crate::EnsureIndexes;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MembershipDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    user_id: ObjectId,
    app_id: ObjectId,
    role: Role,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    created_at: DateTime<Utc>,
}

impl From<MembershipDoc> for AppMembership {
    fn from(value: MembershipDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            user_id: value.user_id.to_hex(),
            app_id: value.app_id.to_hex(),
            role: value.role,
            created_at: value.created_at,
        }
    }
}

#[derive(Clone)]
pub struct MembershipRepo {
    collection: Collection<MembershipDoc>,
}

impl MembershipRepo {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("app_memberships"),
        }
    }

    pub async fn insert(
        &self,
        user_id: &str,
        app_id: &str,
        role: Role,
    ) -> Result<AppMembership, ConmanError> {
        let user_id = ObjectId::parse_str(user_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid user_id: {e}"),
        })?;
        let app_id = ObjectId::parse_str(app_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid app_id: {e}"),
        })?;

        let doc = MembershipDoc {
            id: ObjectId::new(),
            user_id,
            app_id,
            role,
            created_at: Utc::now(),
        };

        self.collection
            .insert_one(doc.clone())
            .await
            .map_err(|e| ConmanError::Conflict {
                message: format!("failed to insert membership: {e}"),
            })?;

        Ok(doc.into())
    }

    pub async fn find_roles_by_user_id(
        &self,
        user_id: &str,
    ) -> Result<HashMap<String, Role>, ConmanError> {
        let user_id = ObjectId::parse_str(user_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid user_id: {e}"),
        })?;

        let mut cursor = self
            .collection
            .find(doc! {"user_id": user_id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to query memberships: {e}"),
            })?;

        let mut roles = HashMap::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("membership cursor error: {e}"),
        })? {
            let doc: MembershipDoc =
                cursor
                    .deserialize_current()
                    .map_err(|e| ConmanError::Internal {
                        message: format!("failed to deserialize membership: {e}"),
                    })?;
            roles.insert(doc.app_id.to_hex(), doc.role);
        }

        Ok(roles)
    }

    pub async fn list_by_app_id(&self, app_id: &str) -> Result<Vec<AppMembership>, ConmanError> {
        let app_id = ObjectId::parse_str(app_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid app_id: {e}"),
        })?;

        let mut cursor = self
            .collection
            .find(doc! {"app_id": app_id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to query app memberships: {e}"),
            })?;

        let mut rows = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("membership cursor error: {e}"),
        })? {
            let doc: MembershipDoc =
                cursor
                    .deserialize_current()
                    .map_err(|e| ConmanError::Internal {
                        message: format!("failed to deserialize membership: {e}"),
                    })?;
            rows.push(doc.into());
        }
        Ok(rows)
    }

    pub async fn assign_role(
        &self,
        user_id: &str,
        app_id: &str,
        role: Role,
    ) -> Result<AppMembership, ConmanError> {
        let user_id = ObjectId::parse_str(user_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid user_id: {e}"),
        })?;
        let app_id = ObjectId::parse_str(app_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid app_id: {e}"),
        })?;

        let filter = doc! {"user_id": user_id, "app_id": app_id};
        let existing = self
            .collection
            .find_one(filter.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to query membership for assign_role: {e}"),
            })?;

        if let Some(existing) = existing {
            let role_bson = mongodb::bson::to_bson(&role).map_err(|e| ConmanError::Internal {
                message: format!("failed to encode role: {e}"),
            })?;
            self.collection
                .update_one(filter, doc! {"$set": {"role": role_bson}})
                .await
                .map_err(|e| ConmanError::Internal {
                    message: format!("failed to update membership role: {e}"),
                })?;

            return Ok(AppMembership {
                id: existing.id.to_hex(),
                user_id: user_id.to_hex(),
                app_id: app_id.to_hex(),
                role,
                created_at: existing.created_at,
            });
        }

        self.insert(&user_id.to_hex(), &app_id.to_hex(), role).await
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for MembershipRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let unique = IndexModel::builder()
            .keys(doc! {"user_id": 1, "app_id": 1})
            .options(
                IndexOptions::builder()
                    .unique(true)
                    .name("membership_user_app_unique".to_string())
                    .build(),
            )
            .build();
        let by_user = IndexModel::builder()
            .keys(doc! {"user_id": 1})
            .options(
                IndexOptions::builder()
                    .name("membership_user".to_string())
                    .build(),
            )
            .build();
        let by_app = IndexModel::builder()
            .keys(doc! {"app_id": 1})
            .options(
                IndexOptions::builder()
                    .name("membership_app".to_string())
                    .build(),
            )
            .build();

        self.collection
            .create_indexes(vec![unique, by_user, by_app])
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure membership indexes: {e}"),
            })?;

        Ok(())
    }
}
