use chrono::{DateTime, Utc};
use conman_core::{ConmanError, EnvVarValue};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};

use crate::EnsureIndexes;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangesetProfileOverride {
    pub id: String,
    pub app_id: String,
    pub changeset_id: String,
    pub key: String,
    pub value: EnvVarValue,
    pub target_profile_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OverrideDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    app_id: ObjectId,
    changeset_id: ObjectId,
    key: String,
    value: EnvVarValue,
    target_profile_id: Option<ObjectId>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<OverrideDoc> for ChangesetProfileOverride {
    fn from(value: OverrideDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            app_id: value.app_id.to_hex(),
            changeset_id: value.changeset_id.to_hex(),
            key: value.key,
            value: value.value,
            target_profile_id: value.target_profile_id.map(|v| v.to_hex()),
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OverrideInput {
    pub key: String,
    pub value: EnvVarValue,
    pub target_profile_id: Option<String>,
}

#[derive(Clone)]
pub struct ChangesetProfileOverrideRepo {
    collection: Collection<OverrideDoc>,
}

impl ChangesetProfileOverrideRepo {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("changeset_profile_overrides"),
        }
    }

    pub async fn replace_for_changeset(
        &self,
        app_id: &str,
        changeset_id: &str,
        overrides: &[OverrideInput],
    ) -> Result<Vec<ChangesetProfileOverride>, ConmanError> {
        let app_id = ObjectId::parse_str(app_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid app_id: {e}"),
        })?;
        let changeset_id =
            ObjectId::parse_str(changeset_id).map_err(|e| ConmanError::Validation {
                message: format!("invalid changeset_id: {e}"),
            })?;

        self.collection
            .delete_many(doc! {"changeset_id": changeset_id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed clearing existing overrides: {e}"),
            })?;

        let now = Utc::now();
        let mut docs = Vec::with_capacity(overrides.len());
        for entry in overrides {
            let target_profile_id = entry
                .target_profile_id
                .as_deref()
                .map(ObjectId::parse_str)
                .transpose()
                .map_err(|e| ConmanError::Validation {
                    message: format!("invalid target_profile_id: {e}"),
                })?;
            docs.push(OverrideDoc {
                id: ObjectId::new(),
                app_id,
                changeset_id,
                key: entry.key.clone(),
                value: entry.value.clone(),
                target_profile_id,
                created_at: now,
                updated_at: now,
            });
        }
        if !docs.is_empty() {
            self.collection
                .insert_many(docs)
                .await
                .map_err(|e| ConmanError::Internal {
                    message: format!("failed inserting overrides: {e}"),
                })?;
        }

        self.list_by_changeset(&changeset_id.to_hex()).await
    }

    pub async fn list_by_changeset(
        &self,
        changeset_id: &str,
    ) -> Result<Vec<ChangesetProfileOverride>, ConmanError> {
        let changeset_id =
            ObjectId::parse_str(changeset_id).map_err(|e| ConmanError::Validation {
                message: format!("invalid changeset_id: {e}"),
            })?;
        let mut cursor = self
            .collection
            .find(doc! {"changeset_id": changeset_id})
            .sort(doc! {"created_at": 1})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to list changeset profile overrides: {e}"),
            })?;
        let mut rows = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("override cursor error: {e}"),
        })? {
            let row: OverrideDoc =
                cursor
                    .deserialize_current()
                    .map_err(|e| ConmanError::Internal {
                        message: format!("failed to decode override row: {e}"),
                    })?;
            rows.push(row.into());
        }
        Ok(rows)
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for ChangesetProfileOverrideRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let unique_key = IndexModel::builder()
            .keys(doc! {"changeset_id": 1, "key": 1, "target_profile_id": 1})
            .options(
                IndexOptions::builder()
                    .name("override_changeset_key_target_unique".to_string())
                    .unique(true)
                    .build(),
            )
            .build();
        let by_changeset = IndexModel::builder()
            .keys(doc! {"changeset_id": 1, "created_at": 1})
            .options(
                IndexOptions::builder()
                    .name("override_changeset_created".to_string())
                    .build(),
            )
            .build();

        self.collection
            .create_indexes(vec![unique_key, by_changeset])
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure override indexes: {e}"),
            })?;
        Ok(())
    }
}
