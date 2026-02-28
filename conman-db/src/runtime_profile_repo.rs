use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use conman_auth::{decrypt_secret, encrypt_secret};
use conman_core::{ConmanError, EnvVarValue, RuntimeProfile, RuntimeProfileKind};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};

use crate::EnsureIndexes;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeProfileDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    repo_id: ObjectId,
    name: String,
    kind: RuntimeProfileKind,
    base_url: String,
    #[serde(default)]
    app_endpoints: BTreeMap<String, String>,
    env_vars: BTreeMap<String, EnvVarValue>,
    secrets_encrypted: BTreeMap<String, String>,
    database_engine: String,
    connection_ref: String,
    provisioning_mode: String,
    base_profile_id: Option<ObjectId>,
    migration_paths: Vec<String>,
    migration_command: Option<String>,
    revision: u32,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeProfileRevisionDoc {
    #[serde(rename = "_id")]
    id: ObjectId,
    profile_id: ObjectId,
    repo_id: ObjectId,
    revision: u32,
    snapshot: RuntimeProfileDoc,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    created_at: DateTime<Utc>,
}

impl From<RuntimeProfileDoc> for RuntimeProfile {
    fn from(value: RuntimeProfileDoc) -> Self {
        Self {
            id: value.id.to_hex(),
            repo_id: value.repo_id.to_hex(),
            name: value.name,
            kind: value.kind,
            base_url: value.base_url,
            app_endpoints: value.app_endpoints,
            env_vars: value.env_vars,
            secrets_encrypted: value.secrets_encrypted,
            database_engine: value.database_engine,
            connection_ref: value.connection_ref,
            provisioning_mode: value.provisioning_mode,
            base_profile_id: value.base_profile_id.map(|v| v.to_hex()),
            migration_paths: value.migration_paths,
            migration_command: value.migration_command,
            revision: value.revision,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeProfileInput {
    pub name: String,
    pub kind: RuntimeProfileKind,
    pub base_url: String,
    pub app_endpoints: BTreeMap<String, String>,
    pub env_vars: BTreeMap<String, EnvVarValue>,
    pub secrets_plain: BTreeMap<String, String>,
    pub database_engine: String,
    pub connection_ref: String,
    pub provisioning_mode: String,
    pub base_profile_id: Option<String>,
    pub migration_paths: Vec<String>,
    pub migration_command: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeProfileUpdate {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub app_endpoints: Option<BTreeMap<String, String>>,
    pub env_vars: Option<BTreeMap<String, EnvVarValue>>,
    pub secrets_plain: Option<BTreeMap<String, String>>,
    pub database_engine: Option<String>,
    pub connection_ref: Option<String>,
    pub provisioning_mode: Option<String>,
    pub base_profile_id: Option<Option<String>>,
    pub migration_paths: Option<Vec<String>>,
    pub migration_command: Option<Option<String>>,
}

#[derive(Clone)]
pub struct RuntimeProfileRepo {
    collection: Collection<RuntimeProfileDoc>,
    revisions: Collection<RuntimeProfileRevisionDoc>,
}

impl RuntimeProfileRepo {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection("runtime_profiles"),
            revisions: db.collection("runtime_profile_revisions"),
        }
    }

    pub async fn create(
        &self,
        repo_id: &str,
        input: RuntimeProfileInput,
        master_key: &str,
    ) -> Result<RuntimeProfile, ConmanError> {
        let repo_id = ObjectId::parse_str(repo_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid repo_id: {e}"),
        })?;
        let base_profile_id = input
            .base_profile_id
            .as_deref()
            .map(ObjectId::parse_str)
            .transpose()
            .map_err(|e| ConmanError::Validation {
                message: format!("invalid base_profile_id: {e}"),
            })?;

        let mut secrets_encrypted = BTreeMap::new();
        for (key, value) in input.secrets_plain {
            secrets_encrypted.insert(key, encrypt_secret(master_key, &value)?);
        }

        let now = Utc::now();
        let doc = RuntimeProfileDoc {
            id: ObjectId::new(),
            repo_id,
            name: input.name,
            kind: input.kind,
            base_url: input.base_url,
            app_endpoints: input.app_endpoints,
            env_vars: input.env_vars,
            secrets_encrypted,
            database_engine: input.database_engine,
            connection_ref: input.connection_ref,
            provisioning_mode: input.provisioning_mode,
            base_profile_id,
            migration_paths: input.migration_paths,
            migration_command: input.migration_command,
            revision: 1,
            created_at: now,
            updated_at: now,
        };

        self.collection
            .insert_one(doc.clone())
            .await
            .map_err(|e| ConmanError::Conflict {
                message: format!("failed to create runtime profile: {e}"),
            })?;

        self.insert_revision(doc.clone()).await?;
        Ok(doc.into())
    }

    pub async fn list_by_repo(&self, repo_id: &str) -> Result<Vec<RuntimeProfile>, ConmanError> {
        let repo_id = ObjectId::parse_str(repo_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid repo_id: {e}"),
        })?;

        let mut cursor = self
            .collection
            .find(doc! {"repo_id": repo_id})
            .sort(doc! {"updated_at": -1})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to list runtime profiles: {e}"),
            })?;

        let mut items = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("runtime profile cursor error: {e}"),
        })? {
            let profile: RuntimeProfileDoc =
                cursor
                    .deserialize_current()
                    .map_err(|e| ConmanError::Internal {
                        message: format!("failed to decode runtime profile: {e}"),
                    })?;
            items.push(profile.into());
        }
        Ok(items)
    }

    pub async fn find_by_id(
        &self,
        profile_id: &str,
    ) -> Result<Option<RuntimeProfile>, ConmanError> {
        let profile_id = ObjectId::parse_str(profile_id).map_err(|e| ConmanError::Validation {
            message: format!("invalid profile_id: {e}"),
        })?;

        let profile = self
            .collection
            .find_one(doc! {"_id": profile_id})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to find runtime profile: {e}"),
            })?;

        Ok(profile.map(Into::into))
    }

    pub async fn update(
        &self,
        profile_id: &str,
        patch: RuntimeProfileUpdate,
        master_key: &str,
    ) -> Result<RuntimeProfile, ConmanError> {
        let profile_id_obj =
            ObjectId::parse_str(profile_id).map_err(|e| ConmanError::Validation {
                message: format!("invalid profile_id: {e}"),
            })?;

        let mut profile = self
            .collection
            .find_one(doc! {"_id": profile_id_obj})
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to find runtime profile: {e}"),
            })?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "runtime_profile",
                id: profile_id.to_string(),
            })?;

        if let Some(name) = patch.name {
            profile.name = name;
        }
        if let Some(base_url) = patch.base_url {
            profile.base_url = base_url;
        }
        if let Some(app_endpoints) = patch.app_endpoints {
            profile.app_endpoints = app_endpoints;
        }
        if let Some(env_vars) = patch.env_vars {
            profile.env_vars = env_vars;
        }
        if let Some(secrets_plain) = patch.secrets_plain {
            let mut secrets_encrypted = BTreeMap::new();
            for (key, value) in secrets_plain {
                secrets_encrypted.insert(key, encrypt_secret(master_key, &value)?);
            }
            profile.secrets_encrypted = secrets_encrypted;
        }
        if let Some(database_engine) = patch.database_engine {
            profile.database_engine = database_engine;
        }
        if let Some(connection_ref) = patch.connection_ref {
            profile.connection_ref = connection_ref;
        }
        if let Some(provisioning_mode) = patch.provisioning_mode {
            profile.provisioning_mode = provisioning_mode;
        }
        if let Some(base_profile_id) = patch.base_profile_id {
            profile.base_profile_id = base_profile_id
                .as_deref()
                .map(ObjectId::parse_str)
                .transpose()
                .map_err(|e| ConmanError::Validation {
                    message: format!("invalid base_profile_id: {e}"),
                })?;
        }
        if let Some(migration_paths) = patch.migration_paths {
            profile.migration_paths = migration_paths;
        }
        if let Some(migration_command) = patch.migration_command {
            profile.migration_command = migration_command;
        }

        profile.revision += 1;
        profile.updated_at = Utc::now();

        self.collection
            .replace_one(doc! {"_id": profile.id}, profile.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to update runtime profile: {e}"),
            })?;

        self.insert_revision(profile.clone()).await?;
        Ok(profile.into())
    }

    pub async fn reveal_secret(
        &self,
        profile_id: &str,
        key: &str,
        master_key: &str,
    ) -> Result<String, ConmanError> {
        let profile = self
            .find_by_id(profile_id)
            .await?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "runtime_profile",
                id: profile_id.to_string(),
            })?;
        let encrypted =
            profile
                .secrets_encrypted
                .get(key)
                .ok_or_else(|| ConmanError::NotFound {
                    entity: "secret_key",
                    id: key.to_string(),
                })?;
        decrypt_secret(master_key, encrypted)
    }

    async fn insert_revision(&self, snapshot: RuntimeProfileDoc) -> Result<(), ConmanError> {
        let doc = RuntimeProfileRevisionDoc {
            id: ObjectId::new(),
            profile_id: snapshot.id,
            repo_id: snapshot.repo_id,
            revision: snapshot.revision,
            snapshot,
            created_at: Utc::now(),
        };
        self.revisions
            .insert_one(doc)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to insert runtime profile revision: {e}"),
            })?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for RuntimeProfileRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let by_app_name = IndexModel::builder()
            .keys(doc! {"repo_id": 1, "name": 1})
            .options(
                IndexOptions::builder()
                    .name("runtime_profiles_app_name_unique".to_string())
                    .unique(true)
                    .build(),
            )
            .build();
        let by_app_kind = IndexModel::builder()
            .keys(doc! {"repo_id": 1, "kind": 1})
            .options(
                IndexOptions::builder()
                    .name("runtime_profiles_app_kind".to_string())
                    .build(),
            )
            .build();

        self.collection
            .create_indexes(vec![by_app_name, by_app_kind])
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure runtime profile indexes: {e}"),
            })?;

        let by_profile_revision = IndexModel::builder()
            .keys(doc! {"profile_id": 1, "revision": -1})
            .options(
                IndexOptions::builder()
                    .name("runtime_profile_revisions_profile_rev".to_string())
                    .build(),
            )
            .build();
        self.revisions
            .create_index(by_profile_revision)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to ensure runtime profile revision indexes: {e}"),
            })?;

        Ok(())
    }
}
