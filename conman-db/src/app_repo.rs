use chrono::{DateTime, Utc};
use conman_core::{App, AppBranding, ConmanError};
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
    repo_id: ObjectId,
    key: String,
    title: String,
    #[serde(default)]
    domains: Vec<String>,
    #[serde(default)]
    branding: Option<AppBranding>,
    #[serde(default)]
    roles: Vec<String>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    updated_at: DateTime<Utc>,
}

impl From<AppDoc> for App {
    fn from(value: AppDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            repo_id: value.repo_id.to_hex(),
            key: value.key,
            title: value.title,
            domains: value.domains,
            branding: value.branding,
            roles: value.roles,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreateAppInput {
    pub key: String,
    pub title: String,
    pub domains: Vec<String>,
    pub branding: Option<AppBranding>,
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateAppInput {
    pub title: Option<String>,
    pub domains: Option<Vec<String>>,
    pub branding: Option<Option<AppBranding>>,
    pub roles: Option<Vec<String>>,
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

    pub async fn create(
        &self,
        repo_id: &str,
        input: CreateAppInput,
    ) -> Result<App, ConmanError> {
        let repo_id = ObjectId::parse_str(repo_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid repo_id: {e}"),
        })?;
        let now = Utc::now();
        let doc = AppDoc {
            id: ObjectId::new(),
            repo_id,
            key: input.key,
            title: input.title,
            domains: input.domains,
            branding: input.branding,
            roles: input.roles,
            created_at: now,
            updated_at: now,
        };
        self.collection
            .insert_one(doc.clone())
            .await
            .map_err(|e| ConmanError::Conflict {
                message: format!("failed to create app: {e}"),
            })?;
        Ok(doc.into())
    }

    pub async fn list_by_repo(&self, repo_id: &str) -> Result<Vec<App>, ConmanError> {
        let repo_id = ObjectId::parse_str(repo_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid repo_id: {e}"),
        })?;
        let mut cursor = self
            .collection
            .find(doc! {"repo_id": repo_id})
            .sort(doc! {"updated_at": -1})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to list apps: {e}"),
            })?;
        let mut items = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("app cursor error: {e}"),
        })? {
            let doc: AppDoc =
                cursor
                    .deserialize_current()
                    .map_err(|e| ConmanError::Internal {
                        message: format!("failed to decode app: {e}"),
                    })?;
            items.push(doc.into());
        }
        Ok(items)
    }

    pub async fn find_by_id(&self, app_id: &str) -> Result<Option<App>, ConmanError> {
        let id = ObjectId::parse_str(app_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid app_id: {e}"),
        })?;
        let doc = self
            .collection
            .find_one(doc! {"_id": id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to get app: {e}"),
            })?;
        Ok(doc.map(Into::into))
    }

    pub async fn update(
        &self,
        app_id: &str,
        input: UpdateAppInput,
    ) -> Result<App, ConmanError> {
        let id = ObjectId::parse_str(app_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid app_id: {e}"),
        })?;

        let mut set_doc = doc! {};
        if let Some(title) = input.title {
            set_doc.insert("title", title);
        }
        if let Some(domains) = input.domains {
            set_doc.insert(
                "domains",
                mongodb::bson::to_bson(&domains).map_err(|e| ConmanError::Internal {
                    message: format!("failed encoding domains: {e}"),
                })?,
            );
        }
        if let Some(branding) = input.branding {
            set_doc.insert(
                "branding",
                mongodb::bson::to_bson(&branding).map_err(|e| ConmanError::Internal {
                    message: format!("failed encoding branding: {e}"),
                })?,
            );
        }
        if let Some(roles) = input.roles {
            set_doc.insert(
                "roles",
                mongodb::bson::to_bson(&roles).map_err(|e| ConmanError::Internal {
                    message: format!("failed encoding roles: {e}"),
                })?,
            );
        }
        set_doc.insert("updated_at", mongodb::bson::DateTime::now());

        self.collection
            .update_one(doc! {"_id": id}, doc! {"$set": set_doc})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to update app: {e}"),
            })?;

        self.find_by_id(app_id)
            .await?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "app",
                id: app_id.to_string(),
            })
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for AppRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let repo_key_unique = IndexModel::builder()
            .keys(doc! {"repo_id": 1, "key": 1})
            .options(
                IndexOptions::builder()
                    .name("apps_repo_key_unique".to_string())
                    .unique(true)
                    .build(),
            )
            .build();
        let repo_idx = IndexModel::builder()
            .keys(doc! {"repo_id": 1})
            .options(
                IndexOptions::builder()
                    .name("apps_repo_idx".to_string())
                    .build(),
            )
            .build();
        self.collection
            .create_indexes(vec![repo_key_unique, repo_idx])
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure app indexes: {e}"),
            })?;
        Ok(())
    }
}
