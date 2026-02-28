use chrono::{DateTime, Utc};
use conman_core::{
    Approval, Changeset, ChangesetAction, ChangesetState, ConmanError, Role, transition_changeset,
};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};

use crate::EnsureIndexes;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChangesetDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    repo_id: ObjectId,
    workspace_id: ObjectId,
    title: String,
    description: Option<String>,
    state: ChangesetState,
    author_user_id: ObjectId,
    head_sha: String,
    submitted_head_sha: Option<String>,
    revision: u32,
    approvals: Vec<Approval>,
    queue_position: Option<i64>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime_optional")]
    queued_at: Option<DateTime<Utc>>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChangesetRevisionDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    changeset_id: ObjectId,
    revision: u32,
    head_sha: String,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    created_at: DateTime<Utc>,
}

impl From<ChangesetDoc> for Changeset {
    fn from(value: ChangesetDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            repo_id: value.repo_id.to_hex(),
            workspace_id: value.workspace_id.to_hex(),
            title: value.title,
            description: value.description,
            state: value.state,
            author_user_id: value.author_user_id.to_hex(),
            head_sha: value.head_sha,
            submitted_head_sha: value.submitted_head_sha,
            revision: value.revision,
            approvals: value.approvals,
            queue_position: value.queue_position,
            queued_at: value.queued_at,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreateChangesetInput {
    pub repo_id: String,
    pub workspace_id: String,
    pub title: String,
    pub description: Option<String>,
    pub author_user_id: String,
    pub head_sha: String,
}

#[derive(Debug, Clone, Copy)]
pub enum ReviewAction {
    Approve,
    RequestChanges,
    Reject,
}

#[derive(Clone)]
pub struct ChangesetRepo {
    collection: Collection<ChangesetDoc>,
    revisions: Collection<ChangesetRevisionDoc>,
}

impl ChangesetRepo {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("changesets"),
            revisions: db.collection("changeset_revisions"),
        }
    }

    pub async fn create(&self, input: CreateChangesetInput) -> Result<Changeset, ConmanError> {
        let repo_id = ObjectId::parse_str(&input.repo_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid repo_id: {e}"),
        })?;
        let workspace_id =
            ObjectId::parse_str(&input.workspace_id).map_err(|e| ConmanError::Validation {
                message: format!("invalid workspace_id: {e}"),
            })?;
        let author_user_id =
            ObjectId::parse_str(&input.author_user_id).map_err(|e| ConmanError::Validation {
                message: format!("invalid author_user_id: {e}"),
            })?;

        if self
            .find_open_by_workspace(&input.workspace_id)
            .await?
            .is_some()
        {
            return Err(ConmanError::Conflict {
                message: "workspace already has an open changeset".to_string(),
            });
        }

        let now = Utc::now();
        let doc = ChangesetDoc {
            id: ObjectId::new(),
            repo_id,
            workspace_id,
            title: input.title,
            description: input.description,
            state: ChangesetState::Draft,
            author_user_id,
            head_sha: input.head_sha,
            submitted_head_sha: None,
            revision: 0,
            approvals: Vec::new(),
            queue_position: None,
            queued_at: None,
            created_at: now,
            updated_at: now,
        };

        self.collection
            .insert_one(doc.clone())
            .await
            .map_err(|e| ConmanError::Conflict {
                message: format!("failed to create changeset: {e}"),
            })?;
        Ok(doc.into())
    }

    pub async fn list_by_repo(
        &self,
        repo_id: &str,
        skip: u64,
        limit: u64,
    ) -> Result<(Vec<Changeset>, u64), ConmanError> {
        let repo_id = ObjectId::parse_str(repo_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid repo_id: {e}"),
        })?;
        let filter = doc! {"repo_id": repo_id};
        let total = self
            .collection
            .count_documents(filter.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to count changesets: {e}"),
            })?;
        let mut cursor = self
            .collection
            .find(filter)
            .sort(doc! {"updated_at": -1})
            .skip(skip)
            .limit(limit as i64)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to list changesets: {e}"),
            })?;
        let mut rows = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("changeset cursor error: {e}"),
        })? {
            let row: ChangesetDoc =
                cursor
                    .deserialize_current()
                    .map_err(|e| ConmanError::Internal {
                        message: format!("failed to decode changeset: {e}"),
                    })?;
            rows.push(row.into());
        }
        Ok((rows, total))
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<Changeset>, ConmanError> {
        let id = ObjectId::parse_str(id).map_err(|e| ConmanError::Validation {
            message: format!("invalid changeset id: {e}"),
        })?;
        let row = self
            .collection
            .find_one(doc! {"_id": id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to find changeset: {e}"),
            })?;
        Ok(row.map(Into::into))
    }

    pub async fn find_open_by_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Option<Changeset>, ConmanError> {
        let workspace_id =
            ObjectId::parse_str(workspace_id).map_err(|e| ConmanError::Validation {
                message: format!("invalid workspace_id: {e}"),
            })?;

        let row = self
            .collection
            .find_one(
                doc! {
                    "workspace_id": workspace_id,
                    "state": {"$in": [
                        mongodb::bson::to_bson(&ChangesetState::Draft).map_err(|e| ConmanError::Internal { message: format!("failed to encode state: {e}") })?,
                        mongodb::bson::to_bson(&ChangesetState::Submitted).map_err(|e| ConmanError::Internal { message: format!("failed to encode state: {e}") })?,
                        mongodb::bson::to_bson(&ChangesetState::InReview).map_err(|e| ConmanError::Internal { message: format!("failed to encode state: {e}") })?,
                        mongodb::bson::to_bson(&ChangesetState::Approved).map_err(|e| ConmanError::Internal { message: format!("failed to encode state: {e}") })?,
                        mongodb::bson::to_bson(&ChangesetState::ChangesRequested).map_err(|e| ConmanError::Internal { message: format!("failed to encode state: {e}") })?,
                        mongodb::bson::to_bson(&ChangesetState::Queued).map_err(|e| ConmanError::Internal { message: format!("failed to encode state: {e}") })?,
                        mongodb::bson::to_bson(&ChangesetState::Conflicted).map_err(|e| ConmanError::Internal { message: format!("failed to encode state: {e}") })?,
                        mongodb::bson::to_bson(&ChangesetState::NeedsRevalidation).map_err(|e| ConmanError::Internal { message: format!("failed to encode state: {e}") })?,
                    ]}
                },
            )
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to query open changeset by workspace: {e}"),
            })?;
        Ok(row.map(Into::into))
    }

    pub async fn submit_or_resubmit(
        &self,
        id: &str,
        head_sha: &str,
        resubmit: bool,
    ) -> Result<Changeset, ConmanError> {
        let id_obj = ObjectId::parse_str(id).map_err(|e| ConmanError::Validation {
            message: format!("invalid changeset id: {e}"),
        })?;
        let mut row = self
            .collection
            .find_one(doc! {"_id": id_obj})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to load changeset for submit: {e}"),
            })?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "changeset",
                id: id.to_string(),
            })?;
        row.state = transition_changeset(
            row.state,
            if resubmit {
                ChangesetAction::Resubmit
            } else {
                ChangesetAction::Submit
            },
        )?;
        row.head_sha = head_sha.to_string();
        row.submitted_head_sha = Some(head_sha.to_string());
        row.revision += 1;
        row.approvals.clear();
        row.updated_at = Utc::now();

        self.collection
            .replace_one(doc! {"_id": id_obj}, row.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to persist submit/resubmit: {e}"),
            })?;
        self.insert_revision(id_obj, row.revision, head_sha).await?;
        Ok(row.into())
    }

    pub async fn review(
        &self,
        id: &str,
        reviewer_user_id: &str,
        reviewer_role: Role,
        action: ReviewAction,
    ) -> Result<Changeset, ConmanError> {
        let id_obj = ObjectId::parse_str(id).map_err(|e| ConmanError::Validation {
            message: format!("invalid changeset id: {e}"),
        })?;
        let mut row = self
            .collection
            .find_one(doc! {"_id": id_obj})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to load changeset for review: {e}"),
            })?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "changeset",
                id: id.to_string(),
            })?;

        let transition_action = match action {
            ReviewAction::Approve => ChangesetAction::Approve,
            ReviewAction::RequestChanges => ChangesetAction::RequestChanges,
            ReviewAction::Reject => ChangesetAction::Reject,
        };
        row.state = transition_changeset(row.state, transition_action)?;

        if matches!(action, ReviewAction::Approve) {
            row.approvals.retain(|a| a.user_id != reviewer_user_id);
            row.approvals.push(Approval {
                user_id: reviewer_user_id.to_string(),
                role: reviewer_role,
                approved_at: Utc::now(),
            });
        } else {
            row.approvals.clear();
        }

        row.updated_at = Utc::now();
        self.collection
            .replace_one(doc! {"_id": id_obj}, row.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to persist review action: {e}"),
            })?;
        Ok(row.into())
    }

    pub async fn queue(&self, id: &str, queue_position: i64) -> Result<Changeset, ConmanError> {
        let id_obj = ObjectId::parse_str(id).map_err(|e| ConmanError::Validation {
            message: format!("invalid changeset id: {e}"),
        })?;
        let mut row = self
            .collection
            .find_one(doc! {"_id": id_obj})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to load changeset for queueing: {e}"),
            })?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "changeset",
                id: id.to_string(),
            })?;

        row.state = transition_changeset(row.state, ChangesetAction::Queue)?;
        row.queue_position = Some(queue_position);
        row.queued_at = Some(Utc::now());
        row.updated_at = Utc::now();
        self.collection
            .replace_one(doc! {"_id": id_obj}, row.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to persist queue action: {e}"),
            })?;
        Ok(row.into())
    }

    pub async fn next_queue_position(&self, repo_id: &str) -> Result<i64, ConmanError> {
        let repo_id = ObjectId::parse_str(repo_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid repo_id: {e}"),
        })?;
        let queued_state =
            mongodb::bson::to_bson(&ChangesetState::Queued).map_err(|e| ConmanError::Internal {
                message: format!("failed to encode queued state: {e}"),
            })?;
        let top = self
            .collection
            .find_one(doc! {"repo_id": repo_id, "state": queued_state})
            .sort(doc! {"queue_position": -1})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to query queue head: {e}"),
            })?;
        Ok(top.and_then(|r| r.queue_position).unwrap_or(0) + 1)
    }

    pub async fn move_to_draft(&self, id: &str) -> Result<Changeset, ConmanError> {
        let id_obj = ObjectId::parse_str(id).map_err(|e| ConmanError::Validation {
            message: format!("invalid changeset id: {e}"),
        })?;
        let mut row = self
            .collection
            .find_one(doc! {"_id": id_obj})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to load changeset for move_to_draft: {e}"),
            })?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "changeset",
                id: id.to_string(),
            })?;
        row.state = transition_changeset(row.state, ChangesetAction::MoveToDraft)?;
        row.approvals.clear();
        row.queue_position = None;
        row.queued_at = None;
        row.updated_at = Utc::now();
        self.collection
            .replace_one(doc! {"_id": id_obj}, row.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to persist move_to_draft: {e}"),
            })?;
        Ok(row.into())
    }

    pub async fn mark_conflicted(&self, id: &str) -> Result<Changeset, ConmanError> {
        self.apply_queue_revalidation_result(id, ChangesetAction::MarkConflicted)
            .await
    }

    pub async fn mark_needs_revalidation(&self, id: &str) -> Result<Changeset, ConmanError> {
        self.apply_queue_revalidation_result(id, ChangesetAction::MarkNeedsRevalidation)
            .await
    }

    pub async fn list_queued_by_repo(&self, repo_id: &str) -> Result<Vec<Changeset>, ConmanError> {
        let repo_id = ObjectId::parse_str(repo_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid repo_id: {e}"),
        })?;
        let queued =
            mongodb::bson::to_bson(&ChangesetState::Queued).map_err(|e| ConmanError::Internal {
                message: format!("failed to encode queued state: {e}"),
            })?;
        let mut cursor = self
            .collection
            .find(doc! {"repo_id": repo_id, "state": queued})
            .sort(doc! {"queue_position": 1, "queued_at": 1})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to list queued changesets: {e}"),
            })?;
        let mut rows = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("queued changeset cursor error: {e}"),
        })? {
            let row: ChangesetDoc =
                cursor
                    .deserialize_current()
                    .map_err(|e| ConmanError::Internal {
                        message: format!("failed to decode queued changeset: {e}"),
                    })?;
            rows.push(row.into());
        }
        Ok(rows)
    }

    pub async fn mark_released_batch(&self, changeset_ids: &[String]) -> Result<(), ConmanError> {
        if changeset_ids.is_empty() {
            return Ok(());
        }
        let ids = changeset_ids
            .iter()
            .map(ObjectId::parse_str)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ConmanError::Validation {
                message: format!("invalid changeset id in release batch: {e}"),
            })?;
        let released = mongodb::bson::to_bson(&ChangesetState::Released).map_err(|e| {
            ConmanError::Internal {
                message: format!("failed to encode released state: {e}"),
            }
        })?;
        self.collection
            .update_many(
                doc! {"_id": {"$in": ids}},
                doc! {"$set": {"state": released, "updated_at": Utc::now()}},
            )
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to mark released changesets: {e}"),
            })?;
        Ok(())
    }

    pub async fn update_title_and_description(
        &self,
        id: &str,
        title: Option<String>,
        description: Option<String>,
    ) -> Result<Changeset, ConmanError> {
        let id_obj = ObjectId::parse_str(id).map_err(|e| ConmanError::Validation {
            message: format!("invalid changeset id: {e}"),
        })?;
        let mut row = self
            .collection
            .find_one(doc! {"_id": id_obj})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to load changeset for update: {e}"),
            })?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "changeset",
                id: id.to_string(),
            })?;
        if let Some(title) = title {
            row.title = title;
        }
        if description.is_some() {
            row.description = description;
        }
        row.updated_at = Utc::now();
        self.collection
            .replace_one(doc! {"_id": id_obj}, row.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to persist changeset metadata update: {e}"),
            })?;
        Ok(row.into())
    }

    async fn insert_revision(
        &self,
        changeset_id: ObjectId,
        revision: u32,
        head_sha: &str,
    ) -> Result<(), ConmanError> {
        self.revisions
            .insert_one(ChangesetRevisionDoc {
                id: ObjectId::new(),
                changeset_id,
                revision,
                head_sha: head_sha.to_string(),
                created_at: Utc::now(),
            })
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to insert changeset revision: {e}"),
            })?;
        Ok(())
    }

    async fn apply_queue_revalidation_result(
        &self,
        id: &str,
        action: ChangesetAction,
    ) -> Result<Changeset, ConmanError> {
        let id_obj = ObjectId::parse_str(id).map_err(|e| ConmanError::Validation {
            message: format!("invalid changeset id: {e}"),
        })?;
        let mut row = self
            .collection
            .find_one(doc! {"_id": id_obj})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to load changeset for revalidation update: {e}"),
            })?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "changeset",
                id: id.to_string(),
            })?;
        row.state = transition_changeset(row.state, action)?;
        row.queue_position = None;
        row.queued_at = None;
        row.approvals.clear();
        row.updated_at = Utc::now();
        self.collection
            .replace_one(doc! {"_id": id_obj}, row.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to persist changeset revalidation status: {e}"),
            })?;
        Ok(row.into())
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for ChangesetRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let by_app = IndexModel::builder()
            .keys(doc! {"repo_id": 1, "updated_at": -1})
            .options(
                IndexOptions::builder()
                    .name("changeset_app_updated_at".to_string())
                    .build(),
            )
            .build();
        let by_workspace_state = IndexModel::builder()
            .keys(doc! {"workspace_id": 1, "state": 1})
            .options(
                IndexOptions::builder()
                    .name("changeset_workspace_state".to_string())
                    .build(),
            )
            .build();
        let queue_idx = IndexModel::builder()
            .keys(doc! {"repo_id": 1, "queue_position": 1})
            .options(
                IndexOptions::builder()
                    .name("changeset_app_queue".to_string())
                    .build(),
            )
            .build();
        self.collection
            .create_indexes(vec![by_app, by_workspace_state, queue_idx])
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure changeset indexes: {e}"),
            })?;
        self.revisions
            .create_index(
                IndexModel::builder()
                    .keys(doc! {"changeset_id": 1, "revision": -1})
                    .options(
                        IndexOptions::builder()
                            .name("changeset_revisions_changeset_revision".to_string())
                            .build(),
                    )
                    .build(),
            )
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure changeset revision indexes: {e}"),
            })?;
        Ok(())
    }
}
