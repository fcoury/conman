use chrono::{DateTime, Utc};
use conman_core::{AuditEvent, AuditRequestContext, ConmanError};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};

use crate::EnsureIndexes;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuditDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    occurred_at: DateTime<Utc>,
    actor_user_id: Option<ObjectId>,
    app_id: Option<ObjectId>,
    entity_type: String,
    entity_id: String,
    action: String,
    before: Option<serde_json::Value>,
    after: Option<serde_json::Value>,
    git_sha: Option<String>,
    context: AuditRequestContext,
}

impl From<AuditDoc> for AuditEvent {
    fn from(value: AuditDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            occurred_at: value.occurred_at,
            actor_user_id: value.actor_user_id.map(|id| id.to_hex()),
            app_id: value.app_id.map(|id| id.to_hex()),
            entity_type: value.entity_type,
            entity_id: value.entity_id,
            action: value.action,
            before: value.before,
            after: value.after,
            git_sha: value.git_sha,
            context: value.context,
        }
    }
}

#[derive(Clone)]
pub struct AuditRepo {
    collection: Collection<AuditDoc>,
}

impl AuditRepo {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("audit_events"),
        }
    }

    pub async fn emit(&self, event: AuditEvent) -> Result<(), ConmanError> {
        let row = AuditDoc {
            id: ObjectId::new(),
            occurred_at: event.occurred_at,
            actor_user_id: event
                .actor_user_id
                .and_then(|id| ObjectId::parse_str(id).ok()),
            app_id: event.app_id.and_then(|id| ObjectId::parse_str(id).ok()),
            entity_type: event.entity_type,
            entity_id: event.entity_id,
            action: event.action,
            before: event.before,
            after: event.after,
            git_sha: event.git_sha,
            context: event.context,
        };
        self.collection
            .insert_one(row)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to write audit event: {e}"),
            })?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for AuditRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let by_app_time = IndexModel::builder()
            .keys(doc! {"app_id": 1, "occurred_at": -1})
            .options(
                IndexOptions::builder()
                    .name("audit_app_time".to_string())
                    .build(),
            )
            .build();
        let by_entity = IndexModel::builder()
            .keys(doc! {"entity_type": 1, "entity_id": 1, "occurred_at": -1})
            .options(
                IndexOptions::builder()
                    .name("audit_entity_time".to_string())
                    .build(),
            )
            .build();
        self.collection
            .create_indexes(vec![by_app_time, by_entity])
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure audit indexes: {e}"),
            })?;
        Ok(())
    }
}
