use chrono::{DateTime, Utc};
use conman_core::{App, AppSettings, ConmanError};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};

use crate::EnsureIndexes;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    name: String,
    repo_path: String,
    integration_branch: String,
    settings: AppSettings,
    created_by: ObjectId,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<AppDoc> for App {
    fn from(value: AppDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            name: value.name,
            repo_path: value.repo_path,
            integration_branch: value.integration_branch,
            settings: value.settings,
            created_by: value.created_by.to_hex(),
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Clone)]
pub struct AppRepo {
    collection: Collection<AppDoc>,
}

impl AppRepo {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("apps"),
        }
    }

    pub async fn insert(
        &self,
        name: &str,
        repo_path: &str,
        integration_branch: &str,
        created_by: &str,
    ) -> Result<App, ConmanError> {
        let created_by = ObjectId::parse_str(created_by).map_err(|e| ConmanError::Validation {
            message: format!("invalid created_by: {e}"),
        })?;
        let now = Utc::now();
        let doc = AppDoc {
            id: ObjectId::new(),
            name: name.to_string(),
            repo_path: repo_path.to_string(),
            integration_branch: integration_branch.to_string(),
            settings: AppSettings::default(),
            created_by,
            created_at: now,
            updated_at: now,
        };

        self.collection
            .insert_one(doc.clone())
            .await
            .map_err(|e| ConmanError::Conflict {
                message: format!("failed to insert app: {e}"),
            })?;

        Ok(doc.into())
    }

    pub async fn find_by_id(&self, app_id: &str) -> Result<Option<App>, ConmanError> {
        let app_id = ObjectId::parse_str(app_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid app_id: {e}"),
        })?;

        self.collection
            .find_one(doc! {"_id": app_id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to find app: {e}"),
            })
            .map(|doc| doc.map(Into::into))
    }

    pub async fn list_by_ids(
        &self,
        app_ids: &[String],
        skip: u64,
        limit: u64,
    ) -> Result<(Vec<App>, u64), ConmanError> {
        let object_ids: Vec<ObjectId> = app_ids
            .iter()
            .filter_map(|id| ObjectId::parse_str(id).ok())
            .collect();

        if object_ids.is_empty() {
            return Ok((Vec::new(), 0));
        }

        let filter = doc! {"_id": {"$in": object_ids}};
        let total = self
            .collection
            .count_documents(filter.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to count apps: {e}"),
            })?;

        let mut cursor = self
            .collection
            .find(filter)
            .sort(doc! {"updated_at": -1})
            .skip(skip)
            .limit(limit as i64)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to list apps: {e}"),
            })?;

        let mut apps = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("failed iterating apps cursor: {e}"),
        })? {
            let app: AppDoc = cursor
                .deserialize_current()
                .map_err(|e| ConmanError::Internal {
                    message: format!("failed to deserialize app: {e}"),
                })?;
            apps.push(app.into());
        }

        Ok((apps, total))
    }

    pub async fn update_settings(
        &self,
        app_id: &str,
        settings: &AppSettings,
    ) -> Result<App, ConmanError> {
        let app_id_obj = ObjectId::parse_str(app_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid app_id: {e}"),
        })?;
        let app_id_hex = app_id_obj.to_hex();

        let now = Utc::now();
        self.collection
            .update_one(
                doc! {"_id": app_id_obj},
                doc! {"$set": {"settings": mongodb::bson::to_bson(settings).map_err(|e| ConmanError::Internal { message: format!("failed to encode settings: {e}") })?, "updated_at": mongodb::bson::DateTime::from_millis(now.timestamp_millis())}},
            )
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to update app settings: {e}"),
            })?;

        self.find_by_id(&app_id_hex)
            .await?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "app",
                id: app_id_hex,
            })
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for AppRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let name_idx = IndexModel::builder()
            .keys(doc! {"name": 1})
            .options(
                IndexOptions::builder()
                    .name("apps_name_unique".to_string())
                    .unique(true)
                    .build(),
            )
            .build();
        let repo_idx = IndexModel::builder()
            .keys(doc! {"repo_path": 1})
            .options(
                IndexOptions::builder()
                    .name("apps_repo_path_unique".to_string())
                    .unique(true)
                    .build(),
            )
            .build();

        self.collection
            .create_indexes(vec![name_idx, repo_idx])
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure app indexes: {e}"),
            })?;

        Ok(())
    }
}
