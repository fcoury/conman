use async_trait::async_trait;
use conman_core::{Config, ConmanError};
use mongodb::{Client, Database, bson::doc, options::ClientOptions};

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
