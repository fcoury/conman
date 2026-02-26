use chrono::{DateTime, Utc};
use conman_core::{ConmanError, Tenant};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};

use crate::EnsureIndexes;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TenantDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    name: String,
    slug: String,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    updated_at: DateTime<Utc>,
}

impl From<TenantDoc> for Tenant {
    fn from(value: TenantDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            name: value.name,
            slug: value.slug,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Clone)]
pub struct TenantRepo {
    collection: Collection<TenantDoc>,
}

impl TenantRepo {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("tenants"),
        }
    }

    pub async fn create(&self, name: &str, slug: &str) -> Result<Tenant, ConmanError> {
        let now = Utc::now();
        let doc = TenantDoc {
            id: ObjectId::new(),
            name: name.to_string(),
            slug: slug.to_string(),
            created_at: now,
            updated_at: now,
        };
        self.collection
            .insert_one(doc.clone())
            .await
            .map_err(|e| ConmanError::Conflict {
                message: format!("failed to create tenant: {e}"),
            })?;
        Ok(doc.into())
    }

    pub async fn find_by_id(&self, tenant_id: &str) -> Result<Option<Tenant>, ConmanError> {
        let id = ObjectId::parse_str(tenant_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid tenant_id: {e}"),
        })?;
        let doc = self
            .collection
            .find_one(doc! {"_id": id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to get tenant: {e}"),
            })?;
        Ok(doc.map(Into::into))
    }

    pub async fn list(&self, skip: u64, limit: u64) -> Result<(Vec<Tenant>, u64), ConmanError> {
        let total = self
            .collection
            .count_documents(doc! {})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to count tenants: {e}"),
            })?;

        let mut cursor = self
            .collection
            .find(doc! {})
            .sort(doc! {"updated_at": -1})
            .skip(skip)
            .limit(limit as i64)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to list tenants: {e}"),
            })?;

        let mut items = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("tenant cursor error: {e}"),
        })? {
            let doc: TenantDoc = cursor
                .deserialize_current()
                .map_err(|e| ConmanError::Internal {
                    message: format!("failed to decode tenant: {e}"),
                })?;
            items.push(doc.into());
        }

        Ok((items, total))
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for TenantRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let slug_idx = IndexModel::builder()
            .keys(doc! {"slug": 1})
            .options(
                IndexOptions::builder()
                    .name("tenants_slug_unique".to_string())
                    .unique(true)
                    .build(),
            )
            .build();
        self.collection
            .create_index(slug_idx)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure tenant indexes: {e}"),
            })?;
        Ok(())
    }
}

