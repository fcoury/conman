use chrono::{DateTime, Duration, Utc};
use conman_core::{ConmanError, Invite, Role};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::EnsureIndexes;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InviteDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    team_id: ObjectId,
    email: String,
    role: Role,
    token: String,
    invited_by: ObjectId,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    expires_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime_optional")]
    accepted_at: Option<DateTime<Utc>>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    created_at: DateTime<Utc>,
}

impl From<InviteDoc> for Invite {
    fn from(value: InviteDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            team_id: value.team_id.to_hex(),
            email: value.email,
            role: value.role,
            token: value.token,
            invited_by: value.invited_by.to_hex(),
            expires_at: value.expires_at,
            accepted_at: value.accepted_at,
            created_at: value.created_at,
        }
    }
}

#[derive(Clone)]
pub struct InviteRepo {
    collection: Collection<InviteDoc>,
}

impl InviteRepo {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("invites"),
        }
    }

    pub async fn create(
        &self,
        team_id: &str,
        email: &str,
        role: Role,
        invited_by: &str,
        expiry_days: u64,
    ) -> Result<Invite, ConmanError> {
        let team_id = ObjectId::parse_str(team_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid team_id: {e}"),
        })?;
        let invited_by = ObjectId::parse_str(invited_by).map_err(|e| ConmanError::Validation {
            message: format!("invalid invited_by: {e}"),
        })?;

        let now = Utc::now();
        let doc = InviteDoc {
            id: ObjectId::new(),
            team_id,
            email: email.to_lowercase(),
            role,
            token: Uuid::now_v7().to_string(),
            invited_by,
            expires_at: now + Duration::days(expiry_days as i64),
            accepted_at: None,
            created_at: now,
        };

        self.collection
            .insert_one(doc.clone())
            .await
            .map_err(|e| ConmanError::Conflict {
                message: format!("failed to create invite: {e}"),
            })?;

        Ok(doc.into())
    }

    pub async fn find_active_by_token(&self, token: &str) -> Result<Option<Invite>, ConmanError> {
        let now = Utc::now();
        let invite = self
            .collection
            .find_one(doc! {
                "token": token,
                "accepted_at": null,
                "expires_at": {"$gt": now}
            })
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to query invite: {e}"),
            })?;

        Ok(invite.map(Into::into))
    }

    pub async fn find_by_id_for_team(
        &self,
        team_id: &str,
        invite_id: &str,
    ) -> Result<Option<Invite>, ConmanError> {
        let team_id = ObjectId::parse_str(team_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid team_id: {e}"),
        })?;
        let invite_id = ObjectId::parse_str(invite_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid invite_id: {e}"),
        })?;
        let invite = self
            .collection
            .find_one(doc! {"_id": invite_id, "team_id": team_id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to query invite by id: {e}"),
            })?;
        Ok(invite.map(Into::into))
    }

    pub async fn list_active_by_team(
        &self,
        team_id: &str,
        skip: u64,
        limit: u64,
    ) -> Result<(Vec<Invite>, u64), ConmanError> {
        let team_id = ObjectId::parse_str(team_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid team_id: {e}"),
        })?;
        let now = Utc::now();
        let filter = doc! {
            "team_id": team_id,
            "accepted_at": null,
            "expires_at": {"$gt": now},
        };

        let total = self
            .collection
            .count_documents(filter.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to count invites: {e}"),
            })?;

        let mut cursor = self
            .collection
            .find(filter)
            .sort(doc! {"created_at": -1})
            .skip(skip)
            .limit(limit as i64)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to list invites: {e}"),
            })?;

        let mut invites = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("failed iterating invites cursor: {e}"),
        })? {
            let invite: InviteDoc = cursor
                .deserialize_current()
                .map_err(|e| ConmanError::Internal {
                    message: format!("failed to deserialize invite: {e}"),
                })?;
            invites.push(invite.into());
        }

        Ok((invites, total))
    }

    pub async fn resend(
        &self,
        team_id: &str,
        invite_id: &str,
        invited_by: &str,
        expiry_days: u64,
    ) -> Result<Invite, ConmanError> {
        let team_id = ObjectId::parse_str(team_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid team_id: {e}"),
        })?;
        let invite_id = ObjectId::parse_str(invite_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid invite_id: {e}"),
        })?;
        let invited_by = ObjectId::parse_str(invited_by).map_err(|e| ConmanError::Validation {
            message: format!("invalid invited_by: {e}"),
        })?;
        let now = Utc::now();
        let updated = self
            .collection
            .find_one_and_update(
                doc! {"_id": invite_id, "team_id": team_id, "accepted_at": null},
                doc! {"$set": {
                    "token": Uuid::now_v7().to_string(),
                    "invited_by": invited_by,
                    "expires_at": now + Duration::days(expiry_days as i64),
                }},
            )
            .return_document(mongodb::options::ReturnDocument::After)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to resend invite: {e}"),
            })?;

        if let Some(row) = updated {
            return Ok(row.into());
        }

        let existing = self
            .collection
            .find_one(doc! {"_id": invite_id, "team_id": team_id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to load invite after resend miss: {e}"),
            })?;
        if let Some(row) = existing {
            if row.accepted_at.is_some() {
                return Err(ConmanError::Conflict {
                    message: "invite already accepted".to_string(),
                });
            }
        }
        Err(ConmanError::NotFound {
            entity: "invite",
            id: invite_id.to_hex(),
        })
    }

    pub async fn revoke(&self, team_id: &str, invite_id: &str) -> Result<Invite, ConmanError> {
        let team_id = ObjectId::parse_str(team_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid team_id: {e}"),
        })?;
        let invite_id = ObjectId::parse_str(invite_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid invite_id: {e}"),
        })?;

        let deleted = self
            .collection
            .find_one_and_delete(doc! {"_id": invite_id, "team_id": team_id, "accepted_at": null})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to revoke invite: {e}"),
            })?;

        if let Some(row) = deleted {
            return Ok(row.into());
        }

        let existing = self
            .collection
            .find_one(doc! {"_id": invite_id, "team_id": team_id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to load invite after revoke miss: {e}"),
            })?;
        if let Some(row) = existing {
            if row.accepted_at.is_some() {
                return Err(ConmanError::Conflict {
                    message: "invite already accepted".to_string(),
                });
            }
        }
        Err(ConmanError::NotFound {
            entity: "invite",
            id: invite_id.to_hex(),
        })
    }

    pub async fn mark_accepted(&self, invite_id: &str) -> Result<(), ConmanError> {
        let invite_id = ObjectId::parse_str(invite_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid invite_id: {e}"),
        })?;
        self.collection
            .update_one(
                doc! {"_id": invite_id, "accepted_at": null},
                doc! {"$set": {"accepted_at": Utc::now()}},
            )
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to mark invite accepted: {e}"),
            })?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for InviteRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let token_idx = IndexModel::builder()
            .keys(doc! {"token": 1})
            .options(
                IndexOptions::builder()
                    .name("invites_token_unique".to_string())
                    .unique(true)
                    .build(),
            )
            .build();
        let app_email_idx = IndexModel::builder()
            .keys(doc! {"team_id": 1, "email": 1, "accepted_at": 1})
            .options(
                IndexOptions::builder()
                    .name("invites_team_email_lookup".to_string())
                    .build(),
            )
            .build();

        self.collection
            .create_indexes(vec![token_idx, app_email_idx])
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure invite indexes: {e}"),
            })?;
        Ok(())
    }
}
