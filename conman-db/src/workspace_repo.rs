use chrono::{DateTime, Utc};
use conman_core::{BaseRefType, ConmanError, Workspace};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};

use crate::EnsureIndexes;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkspaceDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    repo_id: ObjectId,
    owner_user_id: ObjectId,
    branch_name: String,
    title: Option<String>,
    is_default: bool,
    base_ref_type: BaseRefType,
    base_ref_value: String,
    #[serde(default)]
    base_sha: String,
    head_sha: String,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    updated_at: DateTime<Utc>,
}

impl From<WorkspaceDoc> for Workspace {
    fn from(value: WorkspaceDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            repo_id: value.repo_id.to_hex(),
            owner_user_id: value.owner_user_id.to_hex(),
            branch_name: value.branch_name,
            title: value.title,
            is_default: value.is_default,
            base_ref_type: value.base_ref_type,
            base_ref_value: value.base_ref_value,
            base_sha: value.base_sha,
            head_sha: value.head_sha,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Clone)]
pub struct WorkspaceRepo {
    collection: Collection<WorkspaceDoc>,
}

#[derive(Debug, Clone)]
pub struct CreateWorkspaceInput {
    pub repo_id: String,
    pub owner_user_id: String,
    pub branch_name: String,
    pub title: Option<String>,
    pub is_default: bool,
    pub base_ref_type: BaseRefType,
    pub base_ref_value: String,
    pub base_sha: String,
    pub head_sha: String,
}

impl WorkspaceRepo {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("workspaces"),
        }
    }

    pub async fn create(&self, input: CreateWorkspaceInput) -> Result<Workspace, ConmanError> {
        let repo_id = ObjectId::parse_str(&input.repo_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid repo_id: {e}"),
        })?;
        let owner_user_id =
            ObjectId::parse_str(&input.owner_user_id).map_err(|e| ConmanError::Validation {
                message: format!("invalid owner_user_id: {e}"),
            })?;

        let now = Utc::now();
        let doc = WorkspaceDoc {
            id: ObjectId::new(),
            repo_id,
            owner_user_id,
            branch_name: input.branch_name,
            title: input.title,
            is_default: input.is_default,
            base_ref_type: input.base_ref_type,
            base_ref_value: input.base_ref_value,
            base_sha: input.base_sha,
            head_sha: input.head_sha,
            created_at: now,
            updated_at: now,
        };

        self.collection
            .insert_one(doc.clone())
            .await
            .map_err(|e| ConmanError::Conflict {
                message: format!("failed to create workspace: {e}"),
            })?;
        Ok(doc.into())
    }

    pub async fn list_by_repo_owner(
        &self,
        repo_id: &str,
        owner_user_id: &str,
    ) -> Result<Vec<Workspace>, ConmanError> {
        let repo_id = ObjectId::parse_str(repo_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid repo_id: {e}"),
        })?;
        let owner_user_id =
            ObjectId::parse_str(owner_user_id).map_err(|e| ConmanError::Validation {
                message: format!("invalid owner_user_id: {e}"),
            })?;
        let mut cursor = self
            .collection
            .find(doc! {"repo_id": repo_id, "owner_user_id": owner_user_id})
            .sort(doc! {"is_default": -1, "updated_at": -1})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to list workspaces: {e}"),
            })?;

        let mut out = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("workspace cursor error: {e}"),
        })? {
            let row: WorkspaceDoc =
                cursor
                    .deserialize_current()
                    .map_err(|e| ConmanError::Internal {
                        message: format!("failed to decode workspace: {e}"),
                    })?;
            out.push(row.into());
        }
        Ok(out)
    }

    pub async fn find_by_id(&self, workspace_id: &str) -> Result<Option<Workspace>, ConmanError> {
        let workspace_id =
            ObjectId::parse_str(workspace_id).map_err(|e| ConmanError::Validation {
                message: format!("invalid workspace_id: {e}"),
            })?;
        let row = self
            .collection
            .find_one(doc! {"_id": workspace_id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to query workspace: {e}"),
            })?;
        Ok(row.map(Into::into))
    }

    pub async fn find_default(
        &self,
        repo_id: &str,
        owner_user_id: &str,
    ) -> Result<Option<Workspace>, ConmanError> {
        let repo_id = ObjectId::parse_str(repo_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid repo_id: {e}"),
        })?;
        let owner_user_id =
            ObjectId::parse_str(owner_user_id).map_err(|e| ConmanError::Validation {
                message: format!("invalid owner_user_id: {e}"),
            })?;
        let row = self
            .collection
            .find_one(doc! {"repo_id": repo_id, "owner_user_id": owner_user_id, "is_default": true})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to find default workspace: {e}"),
            })?;
        Ok(row.map(Into::into))
    }

    pub async fn update_title(
        &self,
        workspace_id: &str,
        title: Option<String>,
    ) -> Result<Workspace, ConmanError> {
        let workspace_id =
            ObjectId::parse_str(workspace_id).map_err(|e| ConmanError::Validation {
                message: format!("invalid workspace_id: {e}"),
            })?;
        self.collection
            .update_one(
                doc! {"_id": workspace_id},
                doc! {"$set": {"title": title, "updated_at": Utc::now()}},
            )
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to update workspace title: {e}"),
            })?;
        self.find_by_id(&workspace_id.to_hex())
            .await?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "workspace",
                id: workspace_id.to_hex(),
            })
    }

    pub async fn update_head(
        &self,
        workspace_id: &str,
        head_sha: &str,
    ) -> Result<Workspace, ConmanError> {
        let workspace_id =
            ObjectId::parse_str(workspace_id).map_err(|e| ConmanError::Validation {
                message: format!("invalid workspace_id: {e}"),
            })?;
        self.collection
            .update_one(
                doc! {"_id": workspace_id},
                doc! {"$set": {"head_sha": head_sha, "updated_at": Utc::now()}},
            )
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to update workspace head: {e}"),
            })?;
        self.find_by_id(&workspace_id.to_hex())
            .await?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "workspace",
                id: workspace_id.to_hex(),
            })
    }

    pub async fn update_base_sha(
        &self,
        workspace_id: &str,
        base_sha: &str,
    ) -> Result<Workspace, ConmanError> {
        let workspace_id =
            ObjectId::parse_str(workspace_id).map_err(|e| ConmanError::Validation {
                message: format!("invalid workspace_id: {e}"),
            })?;
        self.collection
            .update_one(
                doc! {"_id": workspace_id},
                doc! {"$set": {"base_sha": base_sha, "updated_at": Utc::now()}},
            )
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to update workspace base sha: {e}"),
            })?;
        self.find_by_id(&workspace_id.to_hex())
            .await?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "workspace",
                id: workspace_id.to_hex(),
            })
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for WorkspaceRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let uniq_branch = IndexModel::builder()
            .keys(doc! {"repo_id": 1, "owner_user_id": 1, "branch_name": 1})
            .options(
                IndexOptions::builder()
                    .name("workspace_app_owner_branch_unique".to_string())
                    .unique(true)
                    .build(),
            )
            .build();
        let by_owner = IndexModel::builder()
            .keys(doc! {"repo_id": 1, "owner_user_id": 1, "is_default": -1, "updated_at": -1})
            .options(
                IndexOptions::builder()
                    .name("workspace_app_owner_lookup".to_string())
                    .build(),
            )
            .build();

        self.collection
            .create_indexes(vec![uniq_branch, by_owner])
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure workspace indexes: {e}"),
            })?;
        Ok(())
    }
}
