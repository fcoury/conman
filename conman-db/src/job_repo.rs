use chrono::{DateTime, Utc};
use conman_core::{ConmanError, Job, JobLogLine, JobState, JobType};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::{IndexOptions, ReturnDocument},
};
use serde::{Deserialize, Serialize};

use crate::EnsureIndexes;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JobDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    app_id: ObjectId,
    job_type: JobType,
    state: JobState,
    entity_type: String,
    entity_id: String,
    payload: serde_json::Value,
    result: Option<serde_json::Value>,
    error_message: Option<String>,
    retry_count: u32,
    max_retries: u32,
    timeout_ms: u64,
    created_by: Option<ObjectId>,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
}

impl From<JobDoc> for Job {
    fn from(value: JobDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            app_id: value.app_id.to_hex(),
            job_type: value.job_type,
            state: value.state,
            entity_type: value.entity_type,
            entity_id: value.entity_id,
            payload: value.payload,
            result: value.result,
            error_message: value.error_message,
            retry_count: value.retry_count,
            max_retries: value.max_retries,
            timeout_ms: value.timeout_ms,
            created_by: value.created_by.map(|id| id.to_hex()),
            created_at: value.created_at,
            started_at: value.started_at,
            finished_at: value.finished_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JobLogDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    app_id: ObjectId,
    job_id: ObjectId,
    level: String,
    message: String,
    timestamp: DateTime<Utc>,
}

impl From<JobLogDoc> for JobLogLine {
    fn from(value: JobLogDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            app_id: value.app_id.to_hex(),
            job_id: value.job_id.to_hex(),
            level: value.level,
            message: value.message,
            timestamp: value.timestamp,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EnqueueJobInput {
    pub app_id: String,
    pub job_type: JobType,
    pub entity_type: String,
    pub entity_id: String,
    pub payload: serde_json::Value,
    pub max_retries: u32,
    pub timeout_ms: u64,
    pub created_by: Option<String>,
}

#[derive(Clone)]
pub struct JobRepo {
    jobs: Collection<JobDoc>,
    logs: Collection<JobLogDoc>,
}

impl JobRepo {
    pub fn new(db: Database) -> Self {
        Self {
            jobs: db.collection("jobs"),
            logs: db.collection("job_logs"),
        }
    }

    pub async fn enqueue(&self, input: EnqueueJobInput) -> Result<Job, ConmanError> {
        let app_id = ObjectId::parse_str(&input.app_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid app_id: {e}"),
        })?;
        let created_by = input
            .created_by
            .as_deref()
            .map(ObjectId::parse_str)
            .transpose()
            .map_err(|e| ConmanError::Validation {
                message: format!("invalid created_by: {e}"),
            })?;
        let now = Utc::now();
        let row = JobDoc {
            id: ObjectId::new(),
            app_id,
            job_type: input.job_type,
            state: JobState::Queued,
            entity_type: input.entity_type,
            entity_id: input.entity_id,
            payload: input.payload,
            result: None,
            error_message: None,
            retry_count: 0,
            max_retries: input.max_retries,
            timeout_ms: input.timeout_ms,
            created_by,
            created_at: now,
            started_at: None,
            finished_at: None,
            updated_at: now,
        };
        self.jobs
            .insert_one(row.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to enqueue job: {e}"),
            })?;
        Ok(row.into())
    }

    pub async fn reserve_next_queued(&self) -> Result<Option<Job>, ConmanError> {
        let queued =
            mongodb::bson::to_bson(&JobState::Queued).map_err(|e| ConmanError::Internal {
                message: format!("failed to encode queued state: {e}"),
            })?;
        let running =
            mongodb::bson::to_bson(&JobState::Running).map_err(|e| ConmanError::Internal {
                message: format!("failed to encode running state: {e}"),
            })?;
        let now = Utc::now();
        let row = self
            .jobs
            .find_one_and_update(
                doc! {"state": queued},
                doc! {"$set": {"state": running, "started_at": mongodb::bson::DateTime::from_millis(now.timestamp_millis()), "updated_at": mongodb::bson::DateTime::from_millis(now.timestamp_millis())}},
            )
            .sort(doc! {"created_at": 1})
            .return_document(ReturnDocument::After)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to reserve queued job: {e}"),
            })?;
        Ok(row.map(Into::into))
    }

    pub async fn complete_success(
        &self,
        job_id: &str,
        result: serde_json::Value,
    ) -> Result<Job, ConmanError> {
        self.update_terminal(job_id, JobState::Succeeded, Some(result), None)
            .await
    }

    pub async fn complete_failure(
        &self,
        job_id: &str,
        error_message: String,
    ) -> Result<Job, ConmanError> {
        self.update_terminal(job_id, JobState::Failed, None, Some(error_message))
            .await
    }

    async fn update_terminal(
        &self,
        job_id: &str,
        state: JobState,
        result: Option<serde_json::Value>,
        error_message: Option<String>,
    ) -> Result<Job, ConmanError> {
        let job_id = ObjectId::parse_str(job_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid job id: {e}"),
        })?;
        let state_bson = mongodb::bson::to_bson(&state).map_err(|e| ConmanError::Internal {
            message: format!("failed to encode terminal job state: {e}"),
        })?;
        let result_bson = match result {
            Some(value) => mongodb::bson::to_bson(&value).map_err(|e| ConmanError::Internal {
                message: format!("failed to encode job result: {e}"),
            })?,
            None => mongodb::bson::Bson::Null,
        };
        let error_bson = match error_message {
            Some(value) => mongodb::bson::Bson::String(value),
            None => mongodb::bson::Bson::Null,
        };
        let now = Utc::now();
        let row = self
            .jobs
            .find_one_and_update(
                doc! {"_id": job_id},
                doc! {
                    "$set": {
                        "state": state_bson,
                        "result": result_bson,
                        "error_message": error_bson,
                        "finished_at": mongodb::bson::DateTime::from_millis(now.timestamp_millis()),
                        "updated_at": mongodb::bson::DateTime::from_millis(now.timestamp_millis())
                    }
                },
            )
            .return_document(ReturnDocument::After)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to update terminal job state: {e}"),
            })?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "job",
                id: job_id.to_hex(),
            })?;
        Ok(row.into())
    }

    pub async fn get(&self, job_id: &str) -> Result<Option<Job>, ConmanError> {
        let job_id = ObjectId::parse_str(job_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid job id: {e}"),
        })?;
        let row = self
            .jobs
            .find_one(doc! {"_id": job_id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to fetch job: {e}"),
            })?;
        Ok(row.map(Into::into))
    }

    pub async fn latest_for_entity(
        &self,
        app_id: &str,
        entity_type: &str,
        entity_id: &str,
        job_type: JobType,
    ) -> Result<Option<Job>, ConmanError> {
        let app_id = ObjectId::parse_str(app_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid app_id: {e}"),
        })?;
        let job_type_bson =
            mongodb::bson::to_bson(&job_type).map_err(|e| ConmanError::Internal {
                message: format!("failed to encode job_type filter: {e}"),
            })?;
        let row = self
            .jobs
            .find_one(doc! {
                "app_id": app_id,
                "entity_type": entity_type,
                "entity_id": entity_id,
                "job_type": job_type_bson,
            })
            .sort(doc! {"created_at": -1})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to query latest job for entity: {e}"),
            })?;
        Ok(row.map(Into::into))
    }

    pub async fn list_by_app(
        &self,
        app_id: &str,
        skip: u64,
        limit: u64,
    ) -> Result<(Vec<Job>, u64), ConmanError> {
        let app_id = ObjectId::parse_str(app_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid app_id: {e}"),
        })?;
        let filter = doc! {"app_id": app_id};
        let total = self
            .jobs
            .count_documents(filter.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to count jobs: {e}"),
            })?;
        let mut cursor = self
            .jobs
            .find(filter)
            .sort(doc! {"created_at": -1})
            .skip(skip)
            .limit(limit as i64)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to list jobs: {e}"),
            })?;
        let mut rows = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("job cursor error: {e}"),
        })? {
            let row: JobDoc = cursor
                .deserialize_current()
                .map_err(|e| ConmanError::Internal {
                    message: format!("failed to decode job row: {e}"),
                })?;
            rows.push(row.into());
        }
        Ok((rows, total))
    }

    pub async fn append_log(
        &self,
        app_id: &str,
        job_id: &str,
        level: &str,
        message: &str,
    ) -> Result<JobLogLine, ConmanError> {
        let app_id = ObjectId::parse_str(app_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid app_id: {e}"),
        })?;
        let job_id = ObjectId::parse_str(job_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid job_id: {e}"),
        })?;
        let now = Utc::now();
        let row = JobLogDoc {
            id: ObjectId::new(),
            app_id,
            job_id,
            level: level.to_string(),
            message: message.to_string(),
            timestamp: now,
        };
        self.logs
            .insert_one(row.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to append job log: {e}"),
            })?;
        Ok(row.into())
    }

    pub async fn list_logs(&self, job_id: &str) -> Result<Vec<JobLogLine>, ConmanError> {
        let job_id = ObjectId::parse_str(job_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid job_id: {e}"),
        })?;
        let mut cursor = self
            .logs
            .find(doc! {"job_id": job_id})
            .sort(doc! {"timestamp": 1})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to list job logs: {e}"),
            })?;
        let mut rows = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("job log cursor error: {e}"),
        })? {
            let row: JobLogDoc =
                cursor
                    .deserialize_current()
                    .map_err(|e| ConmanError::Internal {
                        message: format!("failed to decode job log row: {e}"),
                    })?;
            rows.push(row.into());
        }
        Ok(rows)
    }

    pub async fn count_queued(&self) -> Result<u64, ConmanError> {
        let queued =
            mongodb::bson::to_bson(&JobState::Queued).map_err(|e| ConmanError::Internal {
                message: format!("failed to encode queued state for count: {e}"),
            })?;
        self.jobs
            .count_documents(doc! {"state": queued})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to count queued jobs: {e}"),
            })
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for JobRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let by_state = IndexModel::builder()
            .keys(doc! {"state": 1, "created_at": 1})
            .options(
                IndexOptions::builder()
                    .name("jobs_state_created_at".to_string())
                    .build(),
            )
            .build();
        let by_app = IndexModel::builder()
            .keys(doc! {"app_id": 1, "created_at": -1})
            .options(
                IndexOptions::builder()
                    .name("jobs_app_created_at".to_string())
                    .build(),
            )
            .build();
        self.jobs
            .create_indexes(vec![by_state, by_app])
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure job indexes: {e}"),
            })?;

        self.logs
            .create_index(
                IndexModel::builder()
                    .keys(doc! {"job_id": 1, "timestamp": 1})
                    .options(
                        IndexOptions::builder()
                            .name("job_logs_job_timestamp".to_string())
                            .build(),
                    )
                    .build(),
            )
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure job log indexes: {e}"),
            })?;
        Ok(())
    }
}
