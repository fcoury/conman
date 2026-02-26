use chrono::{DateTime, Utc};
use conman_core::{ConmanError, Deployment, DeploymentState};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};

use crate::EnsureIndexes;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeploymentDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    app_id: ObjectId,
    environment_id: ObjectId,
    release_id: ObjectId,
    state: DeploymentState,
    is_skip_stage: bool,
    is_concurrent_batch: bool,
    approvals: Vec<ObjectId>,
    job_id: Option<ObjectId>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    created_by: ObjectId,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<DeploymentDoc> for Deployment {
    fn from(value: DeploymentDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            app_id: value.app_id.to_hex(),
            environment_id: value.environment_id.to_hex(),
            release_id: value.release_id.to_hex(),
            state: value.state,
            is_skip_stage: value.is_skip_stage,
            is_concurrent_batch: value.is_concurrent_batch,
            approvals: value.approvals.into_iter().map(|id| id.to_hex()).collect(),
            job_id: value.job_id.map(|id| id.to_hex()),
            started_at: value.started_at,
            finished_at: value.finished_at,
            created_by: value.created_by.to_hex(),
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreateDeploymentInput {
    pub app_id: String,
    pub environment_id: String,
    pub release_id: String,
    pub is_skip_stage: bool,
    pub is_concurrent_batch: bool,
    pub approvals: Vec<String>,
    pub created_by: String,
}

#[derive(Clone)]
pub struct DeploymentRepo {
    collection: Collection<DeploymentDoc>,
}

impl DeploymentRepo {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("deployments"),
        }
    }

    pub async fn create(&self, input: CreateDeploymentInput) -> Result<Deployment, ConmanError> {
        let app_id = ObjectId::parse_str(&input.app_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid app_id: {e}"),
        })?;
        let environment_id =
            ObjectId::parse_str(&input.environment_id).map_err(|e| ConmanError::Validation {
                message: format!("invalid environment_id: {e}"),
            })?;
        let release_id =
            ObjectId::parse_str(&input.release_id).map_err(|e| ConmanError::Validation {
                message: format!("invalid release_id: {e}"),
            })?;
        let created_by =
            ObjectId::parse_str(&input.created_by).map_err(|e| ConmanError::Validation {
                message: format!("invalid created_by: {e}"),
            })?;
        let approvals = input
            .approvals
            .iter()
            .map(ObjectId::parse_str)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ConmanError::Validation {
                message: format!("invalid approval user id: {e}"),
            })?;
        let active = self
            .collection
            .count_documents(doc! {
                "app_id": app_id,
                "environment_id": environment_id,
                "state": {"$in": [
                    mongodb::bson::to_bson(&DeploymentState::Pending).map_err(|e| ConmanError::Internal { message: format!("failed to encode pending state: {e}") })?,
                    mongodb::bson::to_bson(&DeploymentState::Running).map_err(|e| ConmanError::Internal { message: format!("failed to encode running state: {e}") })?
                ]}
            })
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed checking active deployment lock: {e}"),
            })?;
        if active > 0 {
            return Err(ConmanError::Conflict {
                message: "an active deployment already exists for this environment".to_string(),
            });
        }

        let now = Utc::now();
        let row = DeploymentDoc {
            id: ObjectId::new(),
            app_id,
            environment_id,
            release_id,
            state: DeploymentState::Pending,
            is_skip_stage: input.is_skip_stage,
            is_concurrent_batch: input.is_concurrent_batch,
            approvals,
            job_id: None,
            started_at: None,
            finished_at: None,
            created_by,
            created_at: now,
            updated_at: now,
        };
        self.collection
            .insert_one(row.clone())
            .await
            .map_err(|e| ConmanError::Conflict {
                message: format!("failed to create deployment: {e}"),
            })?;
        Ok(row.into())
    }

    pub async fn list_by_app(
        &self,
        app_id: &str,
        skip: u64,
        limit: u64,
    ) -> Result<(Vec<Deployment>, u64), ConmanError> {
        let app_id = ObjectId::parse_str(app_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid app_id: {e}"),
        })?;
        let filter = doc! {"app_id": app_id};
        let total = self
            .collection
            .count_documents(filter.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to count deployments: {e}"),
            })?;
        let mut cursor = self
            .collection
            .find(filter)
            .sort(doc! {"created_at": -1})
            .skip(skip)
            .limit(limit as i64)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to list deployments: {e}"),
            })?;
        let mut rows = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("deployment cursor error: {e}"),
        })? {
            let row: DeploymentDoc =
                cursor
                    .deserialize_current()
                    .map_err(|e| ConmanError::Internal {
                        message: format!("failed to decode deployment row: {e}"),
                    })?;
            rows.push(row.into());
        }
        Ok((rows, total))
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for DeploymentRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let by_app = IndexModel::builder()
            .keys(doc! {"app_id": 1, "created_at": -1})
            .options(
                IndexOptions::builder()
                    .name("deployment_app_created".to_string())
                    .build(),
            )
            .build();
        let lock_idx = IndexModel::builder()
            .keys(doc! {"app_id": 1, "environment_id": 1, "state": 1})
            .options(
                IndexOptions::builder()
                    .name("deployment_env_lock_lookup".to_string())
                    .build(),
            )
            .build();
        self.collection
            .create_indexes(vec![by_app, lock_idx])
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure deployment indexes: {e}"),
            })?;
        Ok(())
    }
}
