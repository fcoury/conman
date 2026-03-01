use chrono::{DateTime, Utc};
use conman_core::{ConmanError, Repo, RepoSettings};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};

use crate::EnsureIndexes;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RepoDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    #[serde(default)]
    team_id: Option<ObjectId>,
    name: String,
    repo_path: String,
    integration_branch: String,
    settings: RepoSettings,
    created_by: ObjectId,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    updated_at: DateTime<Utc>,
}

impl From<RepoDoc> for Repo {
    fn from(value: RepoDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            team_id: value.team_id.map(|v| v.to_hex()),
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
pub struct RepoStore {
    collection: Collection<RepoDoc>,
}

impl RepoStore {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("repos"),
        }
    }

    pub async fn insert(
        &self,
        name: &str,
        repo_path: &str,
        integration_branch: &str,
        created_by: &str,
    ) -> Result<Repo, ConmanError> {
        let created_by = ObjectId::parse_str(created_by).map_err(|e| ConmanError::Validation {
            message: format!("invalid created_by: {e}"),
        })?;
        let now = Utc::now();
        let doc = RepoDoc {
            id: ObjectId::new(),
            team_id: None,
            name: name.to_string(),
            repo_path: repo_path.to_string(),
            integration_branch: integration_branch.to_string(),
            settings: RepoSettings::default(),
            created_by,
            created_at: now,
            updated_at: now,
        };

        self.collection
            .insert_one(doc.clone())
            .await
            .map_err(|e| ConmanError::Conflict {
                message: format!("failed to insert repo: {e}"),
            })?;

        Ok(doc.into())
    }

    pub async fn insert_for_team(
        &self,
        team_id: &str,
        name: &str,
        repo_path: &str,
        integration_branch: &str,
        created_by: &str,
    ) -> Result<Repo, ConmanError> {
        let created_by = ObjectId::parse_str(created_by).map_err(|e| ConmanError::Validation {
            message: format!("invalid created_by: {e}"),
        })?;
        let team_id = ObjectId::parse_str(team_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid team_id: {e}"),
        })?;
        let now = Utc::now();
        let doc = RepoDoc {
            id: ObjectId::new(),
            team_id: Some(team_id),
            name: name.to_string(),
            repo_path: repo_path.to_string(),
            integration_branch: integration_branch.to_string(),
            settings: RepoSettings::default(),
            created_by,
            created_at: now,
            updated_at: now,
        };

        self.collection
            .insert_one(doc.clone())
            .await
            .map_err(|e| ConmanError::Conflict {
                message: format!("failed to insert repo: {e}"),
            })?;

        Ok(doc.into())
    }

    pub async fn find_by_id(&self, repo_id: &str) -> Result<Option<Repo>, ConmanError> {
        let repo_id = ObjectId::parse_str(repo_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid repo_id: {e}"),
        })?;

        self.collection
            .find_one(doc! {"_id": repo_id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to find repo: {e}"),
            })
            .map(|doc| doc.map(Into::into))
    }

    pub async fn list_by_ids(
        &self,
        repo_ids: &[String],
        skip: u64,
        limit: u64,
    ) -> Result<(Vec<Repo>, u64), ConmanError> {
        let object_ids: Vec<ObjectId> = repo_ids
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
                message: format!("failed to count repos: {e}"),
            })?;

        let mut cursor = self
            .collection
            .find(filter)
            .sort(doc! {"updated_at": -1})
            .skip(skip)
            .limit(limit as i64)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to list repos: {e}"),
            })?;

        let mut repos = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("failed iterating repos cursor: {e}"),
        })? {
            let repo: RepoDoc =
                cursor
                    .deserialize_current()
                    .map_err(|e| ConmanError::Internal {
                        message: format!("failed to deserialize repo: {e}"),
                    })?;
            repos.push(repo.into());
        }

        Ok((repos, total))
    }

    pub async fn list_by_team_id(&self, team_id: &str) -> Result<Vec<Repo>, ConmanError> {
        let team_id = ObjectId::parse_str(team_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid team_id: {e}"),
        })?;

        let mut cursor = self
            .collection
            .find(doc! {"team_id": team_id})
            .sort(doc! {"updated_at": -1})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to list repos by team: {e}"),
            })?;

        let mut repos = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("failed iterating repos cursor: {e}"),
        })? {
            let repo: RepoDoc =
                cursor
                    .deserialize_current()
                    .map_err(|e| ConmanError::Internal {
                        message: format!("failed to deserialize repo: {e}"),
                    })?;
            repos.push(repo.into());
        }
        Ok(repos)
    }

    pub async fn update_settings(
        &self,
        repo_id: &str,
        settings: &RepoSettings,
    ) -> Result<Repo, ConmanError> {
        let repo_id_obj = ObjectId::parse_str(repo_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid repo_id: {e}"),
        })?;
        let repo_id_hex = repo_id_obj.to_hex();

        let now = Utc::now();
        self.collection
            .update_one(
                doc! {"_id": repo_id_obj},
                doc! {"$set": {"settings": mongodb::bson::to_bson(settings).map_err(|e| ConmanError::Internal { message: format!("failed to encode settings: {e}") })?, "updated_at": now}},
            )
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to update repo settings: {e}"),
            })?;

        self.find_by_id(&repo_id_hex)
            .await?
            .ok_or(ConmanError::NotFound {
                entity: "repo",
                id: repo_id_hex,
            })
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for RepoStore {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let name_idx = IndexModel::builder()
            .keys(doc! {"name": 1})
            .options(
                IndexOptions::builder()
                    .name("repos_name_unique".to_string())
                    .unique(true)
                    .build(),
            )
            .build();
        let repo_idx = IndexModel::builder()
            .keys(doc! {"repo_path": 1})
            .options(
                IndexOptions::builder()
                    .name("repos_repo_path_unique".to_string())
                    .unique(true)
                    .build(),
            )
            .build();
        let team_idx = IndexModel::builder()
            .keys(doc! {"team_id": 1})
            .options(
                IndexOptions::builder()
                    .name("repos_team_id_idx".to_string())
                    .build(),
            )
            .build();

        self.collection
            .create_indexes(vec![name_idx, repo_idx, team_idx])
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure repo indexes: {e}"),
            })?;

        Ok(())
    }
}
