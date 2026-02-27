use chrono::{DateTime, Utc};
use conman_core::{ConmanError, Team};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};

use crate::EnsureIndexes;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TeamDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    name: String,
    slug: String,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    updated_at: DateTime<Utc>,
}

impl From<TeamDoc> for Team {
    fn from(value: TeamDoc) -> Self {
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
pub struct TeamRepo {
    collection: Collection<TeamDoc>,
}

impl TeamRepo {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("teams"),
        }
    }

    pub async fn create(&self, name: &str, slug: &str) -> Result<Team, ConmanError> {
        let now = Utc::now();
        let doc = TeamDoc {
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
                message: format!("failed to create team: {e}"),
            })?;
        Ok(doc.into())
    }

    pub async fn find_by_id(&self, team_id: &str) -> Result<Option<Team>, ConmanError> {
        let id = ObjectId::parse_str(team_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid team_id: {e}"),
        })?;
        let doc = self
            .collection
            .find_one(doc! {"_id": id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to get team: {e}"),
            })?;
        Ok(doc.map(Into::into))
    }

    pub async fn list(&self, skip: u64, limit: u64) -> Result<(Vec<Team>, u64), ConmanError> {
        let total = self
            .collection
            .count_documents(doc! {})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to count teams: {e}"),
            })?;

        let mut cursor = self
            .collection
            .find(doc! {})
            .sort(doc! {"updated_at": -1})
            .skip(skip)
            .limit(limit as i64)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to list teams: {e}"),
            })?;

        let mut items = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("team cursor error: {e}"),
        })? {
            let doc: TeamDoc = cursor
                .deserialize_current()
                .map_err(|e| ConmanError::Internal {
                    message: format!("failed to decode team: {e}"),
                })?;
            items.push(doc.into());
        }

        Ok((items, total))
    }

    pub async fn list_by_ids(&self, ids: &[String]) -> Result<Vec<Team>, ConmanError> {
        let object_ids: Vec<ObjectId> = ids
            .iter()
            .filter_map(|id| ObjectId::parse_str(id).ok())
            .collect();

        if object_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut cursor = self
            .collection
            .find(doc! {"_id": {"$in": object_ids}})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to list teams by ids: {e}"),
            })?;

        let mut items = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("team cursor error: {e}"),
        })? {
            let doc: TeamDoc = cursor
                .deserialize_current()
                .map_err(|e| ConmanError::Internal {
                    message: format!("failed to decode team: {e}"),
                })?;
            items.push(doc.into());
        }

        Ok(items)
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for TeamRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let slug_idx = IndexModel::builder()
            .keys(doc! {"slug": 1})
            .options(
                IndexOptions::builder()
                    .name("teams_slug_unique".to_string())
                    .unique(true)
                    .build(),
            )
            .build();
        self.collection
            .create_index(slug_idx)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure team indexes: {e}"),
            })?;
        Ok(())
    }
}
