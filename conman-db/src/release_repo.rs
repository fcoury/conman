use chrono::{DateTime, Datelike, Utc};
use conman_core::{ConmanError, ReleaseBatch, ReleaseState};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};

use crate::EnsureIndexes;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReleaseDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    repo_id: ObjectId,
    tag: String,
    state: ReleaseState,
    ordered_changeset_ids: Vec<ObjectId>,
    compose_job_id: Option<ObjectId>,
    published_sha: Option<String>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime_optional")]
    published_at: Option<DateTime<Utc>>,
    published_by: Option<ObjectId>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    updated_at: DateTime<Utc>,
}

impl From<ReleaseDoc> for ReleaseBatch {
    fn from(value: ReleaseDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            repo_id: value.repo_id.to_hex(),
            tag: value.tag,
            state: value.state,
            ordered_changeset_ids: value
                .ordered_changeset_ids
                .into_iter()
                .map(|id| id.to_hex())
                .collect(),
            compose_job_id: value.compose_job_id.map(|id| id.to_hex()),
            published_sha: value.published_sha,
            published_at: value.published_at,
            published_by: value.published_by.map(|id| id.to_hex()),
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Clone)]
pub struct ReleaseRepo {
    collection: Collection<ReleaseDoc>,
}

impl ReleaseRepo {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("release_batches"),
        }
    }

    pub async fn next_tag(&self, repo_id: &str) -> Result<String, ConmanError> {
        let repo_id = ObjectId::parse_str(repo_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid repo_id: {e}"),
        })?;
        let now = Utc::now();
        let day_prefix = format!("r{:04}.{:02}.{:02}.", now.year(), now.month(), now.day());
        let count = self
            .collection
            .count_documents(
                doc! {"repo_id": repo_id, "tag": {"$regex": format!("^{}", day_prefix)}},
            )
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to count releases for next tag: {e}"),
            })?;
        Ok(format!("{day_prefix}{}", count + 1))
    }

    pub async fn create_draft(
        &self,
        repo_id: &str,
        tag: String,
    ) -> Result<ReleaseBatch, ConmanError> {
        let repo_id = ObjectId::parse_str(repo_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid repo_id: {e}"),
        })?;
        let now = Utc::now();
        let row = ReleaseDoc {
            id: ObjectId::new(),
            repo_id,
            tag,
            state: ReleaseState::DraftRelease,
            ordered_changeset_ids: Vec::new(),
            compose_job_id: None,
            published_sha: None,
            published_at: None,
            published_by: None,
            created_at: now,
            updated_at: now,
        };
        self.collection
            .insert_one(row.clone())
            .await
            .map_err(|e| ConmanError::Conflict {
                message: format!("failed to create draft release: {e}"),
            })?;
        Ok(row.into())
    }

    pub async fn find_by_id(&self, release_id: &str) -> Result<Option<ReleaseBatch>, ConmanError> {
        let id = ObjectId::parse_str(release_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid release_id: {e}"),
        })?;
        let row = self
            .collection
            .find_one(doc! {"_id": id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to find release: {e}"),
            })?;
        Ok(row.map(Into::into))
    }

    pub async fn list_by_repo(
        &self,
        repo_id: &str,
        skip: u64,
        limit: u64,
    ) -> Result<(Vec<ReleaseBatch>, u64), ConmanError> {
        let repo_id = ObjectId::parse_str(repo_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid repo_id: {e}"),
        })?;
        let filter = doc! {"repo_id": repo_id};
        let total = self
            .collection
            .count_documents(filter.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to count releases: {e}"),
            })?;
        let mut cursor = self
            .collection
            .find(filter)
            .sort(doc! {"created_at": -1})
            .skip(skip)
            .limit(limit as i64)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to list releases: {e}"),
            })?;
        let mut rows = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("release cursor error: {e}"),
        })? {
            let row: ReleaseDoc =
                cursor
                    .deserialize_current()
                    .map_err(|e| ConmanError::Internal {
                        message: format!("failed to decode release row: {e}"),
                    })?;
            rows.push(row.into());
        }
        Ok((rows, total))
    }

    pub async fn set_changesets(
        &self,
        release_id: &str,
        ordered_changeset_ids: &[String],
    ) -> Result<ReleaseBatch, ConmanError> {
        let release_id = ObjectId::parse_str(release_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid release_id: {e}"),
        })?;
        let ids = ordered_changeset_ids
            .iter()
            .map(ObjectId::parse_str)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ConmanError::Validation {
                message: format!("invalid changeset id in release set: {e}"),
            })?;
        let now = Utc::now();
        let row = self
            .collection
            .find_one_and_update(
                doc! {"_id": release_id},
                doc! {"$set": {"ordered_changeset_ids": ids, "updated_at": now}},
            )
            .return_document(mongodb::options::ReturnDocument::After)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed setting release changesets: {e}"),
            })?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "release",
                id: release_id.to_hex(),
            })?;
        Ok(row.into())
    }

    pub async fn set_state(
        &self,
        release_id: &str,
        state: ReleaseState,
    ) -> Result<ReleaseBatch, ConmanError> {
        let release_id = ObjectId::parse_str(release_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid release_id: {e}"),
        })?;
        let state_bson = mongodb::bson::to_bson(&state).map_err(|e| ConmanError::Internal {
            message: format!("failed to encode release state: {e}"),
        })?;
        let now = Utc::now();
        let row = self
            .collection
            .find_one_and_update(
                doc! {"_id": release_id},
                doc! {"$set": {"state": state_bson, "updated_at": now}},
            )
            .return_document(mongodb::options::ReturnDocument::After)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to update release state: {e}"),
            })?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "release",
                id: release_id.to_hex(),
            })?;
        Ok(row.into())
    }

    pub async fn set_compose_job(
        &self,
        release_id: &str,
        job_id: &str,
    ) -> Result<ReleaseBatch, ConmanError> {
        let release_id = ObjectId::parse_str(release_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid release_id: {e}"),
        })?;
        let job_id = ObjectId::parse_str(job_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid job_id: {e}"),
        })?;
        let now = Utc::now();
        let row = self
            .collection
            .find_one_and_update(
                doc! {"_id": release_id},
                doc! {"$set": {"compose_job_id": job_id, "updated_at": now}},
            )
            .return_document(mongodb::options::ReturnDocument::After)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to set compose job: {e}"),
            })?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "release",
                id: release_id.to_hex(),
            })?;
        Ok(row.into())
    }

    pub async fn publish(
        &self,
        release_id: &str,
        published_sha: String,
        published_by: &str,
    ) -> Result<ReleaseBatch, ConmanError> {
        let release_id = ObjectId::parse_str(release_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid release_id: {e}"),
        })?;
        let published_by =
            ObjectId::parse_str(published_by).map_err(|e| ConmanError::Validation {
                message: format!("invalid published_by: {e}"),
            })?;
        let now = Utc::now();
        let published_state = mongodb::bson::to_bson(&ReleaseState::Published).map_err(|e| {
            ConmanError::Internal {
                message: format!("failed to encode published state: {e}"),
            }
        })?;
        let row = self
            .collection
            .find_one_and_update(
                doc! {"_id": release_id},
                doc! {
                    "$set": {
                        "state": published_state,
                        "published_sha": published_sha,
                        "published_at": now,
                        "published_by": published_by,
                        "updated_at": now
                    }
                },
            )
            .return_document(mongodb::options::ReturnDocument::After)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to publish release: {e}"),
            })?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "release",
                id: release_id.to_hex(),
            })?;
        Ok(row.into())
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for ReleaseRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let by_app = IndexModel::builder()
            .keys(doc! {"repo_id": 1, "created_at": -1})
            .options(
                IndexOptions::builder()
                    .name("release_app_created".to_string())
                    .build(),
            )
            .build();
        let uniq_tag = IndexModel::builder()
            .keys(doc! {"repo_id": 1, "tag": 1})
            .options(
                IndexOptions::builder()
                    .name("release_app_tag_unique".to_string())
                    .unique(true)
                    .build(),
            )
            .build();
        self.collection
            .create_indexes(vec![by_app, uniq_tag])
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure release indexes: {e}"),
            })?;
        Ok(())
    }
}
