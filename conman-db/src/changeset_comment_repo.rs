use chrono::{DateTime, Utc};
use conman_core::{ChangesetComment, ChangesetCommentEdit, ConmanError};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};

use crate::EnsureIndexes;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CommentDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    app_id: ObjectId,
    changeset_id: ObjectId,
    author_user_id: ObjectId,
    body: String,
    edits: Vec<ChangesetCommentEdit>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    updated_at: DateTime<Utc>,
}

impl From<CommentDoc> for ChangesetComment {
    fn from(value: CommentDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            app_id: value.app_id.to_hex(),
            changeset_id: value.changeset_id.to_hex(),
            author_user_id: value.author_user_id.to_hex(),
            body: value.body,
            edits: value.edits,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Clone)]
pub struct ChangesetCommentRepo {
    collection: Collection<CommentDoc>,
}

impl ChangesetCommentRepo {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("changeset_comments"),
        }
    }

    pub async fn create(
        &self,
        app_id: &str,
        changeset_id: &str,
        author_user_id: &str,
        body: &str,
    ) -> Result<ChangesetComment, ConmanError> {
        let app_id = ObjectId::parse_str(app_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid app_id: {e}"),
        })?;
        let changeset_id =
            ObjectId::parse_str(changeset_id).map_err(|e| ConmanError::Validation {
                message: format!("invalid changeset_id: {e}"),
            })?;
        let author_user_id =
            ObjectId::parse_str(author_user_id).map_err(|e| ConmanError::Validation {
                message: format!("invalid author_user_id: {e}"),
            })?;
        let now = Utc::now();
        let doc = CommentDoc {
            id: ObjectId::new(),
            app_id,
            changeset_id,
            author_user_id,
            body: body.to_string(),
            edits: Vec::new(),
            created_at: now,
            updated_at: now,
        };
        self.collection
            .insert_one(doc.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to create comment: {e}"),
            })?;
        Ok(doc.into())
    }

    pub async fn list_by_changeset(
        &self,
        changeset_id: &str,
    ) -> Result<Vec<ChangesetComment>, ConmanError> {
        let changeset_id =
            ObjectId::parse_str(changeset_id).map_err(|e| ConmanError::Validation {
                message: format!("invalid changeset_id: {e}"),
            })?;
        let mut cursor = self
            .collection
            .find(doc! {"changeset_id": changeset_id})
            .sort(doc! {"created_at": 1})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to list comments: {e}"),
            })?;
        let mut rows = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("comment cursor error: {e}"),
        })? {
            let row: CommentDoc =
                cursor
                    .deserialize_current()
                    .map_err(|e| ConmanError::Internal {
                        message: format!("failed to decode comment: {e}"),
                    })?;
            rows.push(row.into());
        }
        Ok(rows)
    }

    pub async fn edit(
        &self,
        comment_id: &str,
        new_body: &str,
    ) -> Result<ChangesetComment, ConmanError> {
        let comment_id = ObjectId::parse_str(comment_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid comment_id: {e}"),
        })?;
        let mut row = self
            .collection
            .find_one(doc! {"_id": comment_id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to load comment for edit: {e}"),
            })?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "comment",
                id: comment_id.to_hex(),
            })?;
        row.edits.push(ChangesetCommentEdit {
            previous_body: row.body.clone(),
            edited_at: Utc::now(),
        });
        row.body = new_body.to_string();
        row.updated_at = Utc::now();
        self.collection
            .replace_one(doc! {"_id": comment_id}, row.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to persist comment edit: {e}"),
            })?;
        Ok(row.into())
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for ChangesetCommentRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        self.collection
            .create_index(
                IndexModel::builder()
                    .keys(doc! {"changeset_id": 1, "created_at": 1})
                    .options(
                        IndexOptions::builder()
                            .name("changeset_comments_changeset_created".to_string())
                            .build(),
                    )
                    .build(),
            )
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure comment indexes: {e}"),
            })?;
        Ok(())
    }
}
