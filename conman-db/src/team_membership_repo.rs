use std::collections::HashSet;

use chrono::{DateTime, Utc};
use conman_core::{ConmanError, Role, TeamMembership};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};

use crate::EnsureIndexes;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TeamMembershipDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    user_id: ObjectId,
    team_id: ObjectId,
    role: Role,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    created_at: DateTime<Utc>,
}

impl From<TeamMembershipDoc> for TeamMembership {
    fn from(value: TeamMembershipDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            user_id: value.user_id.to_hex(),
            team_id: value.team_id.to_hex(),
            role: value.role,
            created_at: value.created_at,
        }
    }
}

#[derive(Clone)]
pub struct TeamMembershipRepo {
    collection: Collection<TeamMembershipDoc>,
}

impl TeamMembershipRepo {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("team_memberships"),
        }
    }

    pub async fn insert(
        &self,
        user_id: &str,
        team_id: &str,
        role: Role,
    ) -> Result<TeamMembership, ConmanError> {
        let user_id = ObjectId::parse_str(user_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid user_id: {e}"),
        })?;
        let team_id = ObjectId::parse_str(team_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid team_id: {e}"),
        })?;

        let doc = TeamMembershipDoc {
            id: ObjectId::new(),
            user_id,
            team_id,
            role,
            created_at: Utc::now(),
        };

        self.collection
            .insert_one(doc.clone())
            .await
            .map_err(|e| ConmanError::Conflict {
                message: format!("failed to insert team membership: {e}"),
            })?;

        Ok(doc.into())
    }

    pub async fn assign_role(
        &self,
        user_id: &str,
        team_id: &str,
        role: Role,
    ) -> Result<TeamMembership, ConmanError> {
        let user_id_obj = ObjectId::parse_str(user_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid user_id: {e}"),
        })?;
        let team_id_obj = ObjectId::parse_str(team_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid team_id: {e}"),
        })?;

        let filter = doc! {"user_id": user_id_obj, "team_id": team_id_obj};
        let existing = self
            .collection
            .find_one(filter.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to query team membership for assign_role: {e}"),
            })?;

        if let Some(existing) = existing {
            let role_bson = mongodb::bson::to_bson(&role).map_err(|e| ConmanError::Internal {
                message: format!("failed to encode role: {e}"),
            })?;
            self.collection
                .update_one(filter, doc! {"$set": {"role": role_bson}})
                .await
                .map_err(|e| ConmanError::Internal {
                    message: format!("failed to update team membership role: {e}"),
                })?;

            return Ok(TeamMembership {
                id: existing.id.to_hex(),
                user_id: user_id_obj.to_hex(),
                team_id: team_id_obj.to_hex(),
                role,
                created_at: existing.created_at,
            });
        }

        self.insert(user_id, team_id, role).await
    }

    pub async fn role_for_user(
        &self,
        user_id: &str,
        team_id: &str,
    ) -> Result<Option<Role>, ConmanError> {
        let user_id = ObjectId::parse_str(user_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid user_id: {e}"),
        })?;
        let team_id = ObjectId::parse_str(team_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid team_id: {e}"),
        })?;

        let doc = self
            .collection
            .find_one(doc! {"user_id": user_id, "team_id": team_id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to query team membership role: {e}"),
            })?;
        Ok(doc.map(|d| d.role))
    }

    pub async fn list_team_ids_by_user(&self, user_id: &str) -> Result<Vec<String>, ConmanError> {
        let user_id = ObjectId::parse_str(user_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid user_id: {e}"),
        })?;

        let mut cursor = self
            .collection
            .find(doc! {"user_id": user_id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to query team memberships by user: {e}"),
            })?;

        let mut ids = HashSet::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("team membership cursor error: {e}"),
        })? {
            let doc: TeamMembershipDoc =
                cursor
                    .deserialize_current()
                    .map_err(|e| ConmanError::Internal {
                        message: format!("failed to deserialize team membership: {e}"),
                    })?;
            ids.insert(doc.team_id.to_hex());
        }

        Ok(ids.into_iter().collect())
    }

    pub async fn list_by_team_id(&self, team_id: &str) -> Result<Vec<TeamMembership>, ConmanError> {
        let team_id = ObjectId::parse_str(team_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid team_id: {e}"),
        })?;

        let mut cursor = self
            .collection
            .find(doc! {"team_id": team_id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to query team memberships: {e}"),
            })?;

        let mut rows = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("team membership cursor error: {e}"),
        })? {
            let doc: TeamMembershipDoc =
                cursor
                    .deserialize_current()
                    .map_err(|e| ConmanError::Internal {
                        message: format!("failed to deserialize team membership: {e}"),
                    })?;
            rows.push(doc.into());
        }
        Ok(rows)
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for TeamMembershipRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let unique = IndexModel::builder()
            .keys(doc! {"user_id": 1, "team_id": 1})
            .options(
                IndexOptions::builder()
                    .unique(true)
                    .name("team_membership_user_team_unique".to_string())
                    .build(),
            )
            .build();
        let by_user = IndexModel::builder()
            .keys(doc! {"user_id": 1})
            .options(
                IndexOptions::builder()
                    .name("team_membership_user".to_string())
                    .build(),
            )
            .build();
        let by_team = IndexModel::builder()
            .keys(doc! {"team_id": 1})
            .options(
                IndexOptions::builder()
                    .name("team_membership_team".to_string())
                    .build(),
            )
            .build();

        self.collection
            .create_indexes(vec![unique, by_user, by_team])
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure team membership indexes: {e}"),
            })?;

        Ok(())
    }
}
