use async_trait::async_trait;
use conman_core::{Config, ConmanError};
use mongodb::{Client, Database, bson::doc, options::ClientOptions};

pub mod app_repo;
pub mod audit_repo;
pub mod changeset_comment_repo;
pub mod changeset_profile_override_repo;
pub mod changeset_repo;
pub mod deployment_repo;
pub mod environment_repo;
pub mod invite_repo;
pub mod job_repo;
pub mod membership_repo;
pub mod notification_pref_repo;
pub mod password_reset_repo;
pub mod release_repo;
pub mod runtime_profile_repo;
pub mod temp_env_repo;
pub mod user_repo;
pub mod workspace_repo;

pub use app_repo::AppRepo;
pub use audit_repo::AuditRepo;
pub use changeset_comment_repo::ChangesetCommentRepo;
pub use changeset_profile_override_repo::{
    ChangesetProfileOverride, ChangesetProfileOverrideRepo, OverrideInput,
};
pub use changeset_repo::{ChangesetRepo, CreateChangesetInput, ReviewAction};
pub use deployment_repo::{CreateDeploymentInput, DeploymentRepo};
pub use environment_repo::{EnvironmentInput, EnvironmentRepo};
pub use invite_repo::InviteRepo;
pub use job_repo::{EnqueueJobInput, JobRepo};
pub use membership_repo::MembershipRepo;
pub use notification_pref_repo::NotificationPreferenceRepo;
pub use password_reset_repo::PasswordResetRepo;
pub use release_repo::ReleaseRepo;
pub use runtime_profile_repo::{RuntimeProfileInput, RuntimeProfileRepo, RuntimeProfileUpdate};
pub use temp_env_repo::{CreateTempEnvInput, TempEnvRepo};
pub use user_repo::UserRepo;
pub use workspace_repo::{CreateWorkspaceInput, WorkspaceRepo};

#[async_trait]
pub trait EnsureIndexes: Send + Sync {
    async fn ensure_indexes(&self) -> Result<(), ConmanError>;
}

pub async fn connect_mongo(config: &Config) -> Result<Database, ConmanError> {
    let opts =
        ClientOptions::parse(&config.mongo_uri)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to parse CONMAN_MONGO_URI: {e}"),
            })?;

    let client = Client::with_options(opts).map_err(|e| ConmanError::Internal {
        message: format!("failed to create MongoDB client: {e}"),
    })?;

    let db = client.database(&config.mongo_db);

    db.run_command(doc! {"ping": 1})
        .await
        .map_err(|e| ConmanError::Internal {
            message: format!("MongoDB startup ping failed: {e}"),
        })?;

    tracing::info!(db = %config.mongo_db, "MongoDB connected");

    Ok(db)
}

pub async fn check_mongo_health(db: &Database) -> Result<(), ConmanError> {
    db.run_command(doc! {"ping": 1})
        .await
        .map_err(|e| ConmanError::Internal {
            message: format!("MongoDB health check failed: {e}"),
        })?;

    Ok(())
}

pub async fn bootstrap_indexes(repos: &[&dyn EnsureIndexes]) -> Result<(), ConmanError> {
    for repo in repos {
        repo.ensure_indexes().await?;
    }

    tracing::info!(repo_count = repos.len(), "All MongoDB indexes ensured");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoopRepo;

    #[async_trait]
    impl EnsureIndexes for NoopRepo {
        async fn ensure_indexes(&self) -> Result<(), ConmanError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn bootstrap_indexes_noop_ok() {
        let repo = NoopRepo;
        let result = bootstrap_indexes(&[&repo]).await;
        assert!(result.is_ok());
    }
}
