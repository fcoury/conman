use chrono::{DateTime, Utc};
use conman_core::{ConmanError, NotificationEvent, NotificationState};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};

use crate::EnsureIndexes;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NotificationDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    user_id: ObjectId,
    recipient_email: String,
    app_id: Option<ObjectId>,
    event_type: String,
    subject: String,
    body: String,
    state: NotificationState,
    error_message: Option<String>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    updated_at: DateTime<Utc>,
}

impl From<NotificationDoc> for NotificationEvent {
    fn from(value: NotificationDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            user_id: value.user_id.to_hex(),
            recipient_email: value.recipient_email,
            app_id: value.app_id.map(|v| v.to_hex()),
            event_type: value.event_type,
            subject: value.subject,
            body: value.body,
            state: value.state,
            error_message: value.error_message,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Clone)]
pub struct NotificationEventRepo {
    collection: Collection<NotificationDoc>,
}

impl NotificationEventRepo {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("notification_events"),
        }
    }

    pub async fn enqueue(
        &self,
        user_id: &str,
        recipient_email: &str,
        app_id: Option<&str>,
        event_type: &str,
        subject: &str,
        body: &str,
    ) -> Result<NotificationEvent, ConmanError> {
        let user_id = ObjectId::parse_str(user_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid user_id: {e}"),
        })?;
        let app_id =
            app_id
                .map(ObjectId::parse_str)
                .transpose()
                .map_err(|e| ConmanError::Validation {
                    message: format!("invalid app_id: {e}"),
                })?;
        let now = Utc::now();
        let row = NotificationDoc {
            id: ObjectId::new(),
            user_id,
            recipient_email: recipient_email.to_string(),
            app_id,
            event_type: event_type.to_string(),
            subject: subject.to_string(),
            body: body.to_string(),
            state: NotificationState::Queued,
            error_message: None,
            created_at: now,
            updated_at: now,
        };
        self.collection
            .insert_one(row.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to enqueue notification event: {e}"),
            })?;
        Ok(row.into())
    }

    pub async fn reserve_next_queued(&self) -> Result<Option<NotificationEvent>, ConmanError> {
        let queued = mongodb::bson::to_bson(&NotificationState::Queued).map_err(|e| {
            ConmanError::Internal {
                message: format!("failed to encode queued notification state: {e}"),
            }
        })?;
        let sending = mongodb::bson::to_bson(&NotificationState::Sending).map_err(|e| {
            ConmanError::Internal {
                message: format!("failed to encode sending notification state: {e}"),
            }
        })?;
        let now = Utc::now();
        let row = self
            .collection
            .find_one_and_update(
                doc! {"state": queued},
                doc! {"$set": {"state": sending, "updated_at": now}},
            )
            .sort(doc! {"created_at": 1})
            .return_document(mongodb::options::ReturnDocument::After)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to reserve queued notification event: {e}"),
            })?;
        Ok(row.map(Into::into))
    }

    pub async fn mark_sent(&self, id: &str) -> Result<(), ConmanError> {
        self.set_state(id, NotificationState::Sent, None).await
    }

    pub async fn mark_failed(&self, id: &str, error_message: &str) -> Result<(), ConmanError> {
        self.set_state(
            id,
            NotificationState::Failed,
            Some(error_message.to_string()),
        )
        .await
    }

    async fn set_state(
        &self,
        id: &str,
        state: NotificationState,
        error_message: Option<String>,
    ) -> Result<(), ConmanError> {
        let id = ObjectId::parse_str(id).map_err(|e| ConmanError::Validation {
            message: format!("invalid notification id: {e}"),
        })?;
        let state_bson = mongodb::bson::to_bson(&state).map_err(|e| ConmanError::Internal {
            message: format!("failed to encode notification state: {e}"),
        })?;
        let now = Utc::now();
        self.collection
            .update_one(
                doc! {"_id": id},
                doc! {"$set": {
                    "state": state_bson,
                    "error_message": error_message,
                    "updated_at": now
                }},
            )
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to set notification state: {e}"),
            })?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for NotificationEventRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let by_user = IndexModel::builder()
            .keys(doc! {"user_id": 1, "created_at": -1})
            .options(
                IndexOptions::builder()
                    .name("notification_events_user_created".to_string())
                    .build(),
            )
            .build();
        let by_state = IndexModel::builder()
            .keys(doc! {"state": 1, "created_at": 1})
            .options(
                IndexOptions::builder()
                    .name("notification_events_state_created".to_string())
                    .build(),
            )
            .build();
        self.collection
            .create_indexes(vec![by_user, by_state])
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure notification event indexes: {e}"),
            })?;
        Ok(())
    }
}
