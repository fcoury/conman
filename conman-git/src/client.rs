use async_trait::async_trait;
use tonic::transport::Channel;

use conman_core::{ConmanError, GitRepo};

use crate::adapter::GitAdapter;

#[derive(Clone)]
pub struct GitalyClient {
    channel: Channel,
}

impl GitalyClient {
    pub async fn connect(address: &str) -> Result<Self, ConmanError> {
        let channel = Channel::from_shared(address.to_string())
            .map_err(|e| ConmanError::Git {
                message: format!("invalid gitaly address: {e}"),
            })?
            .connect()
            .await
            .map_err(|e| ConmanError::Git {
                message: format!("failed to connect to gitaly: {e}"),
            })?;

        Ok(Self { channel })
    }

    pub fn channel(&self) -> &Channel {
        &self.channel
    }
}

#[async_trait]
impl GitAdapter for GitalyClient {
    async fn repo_exists(&self, _repo: &GitRepo) -> Result<bool, ConmanError> {
        // E01 baseline: connection + adapter boundary are implemented.
        // Typed gRPC calls are wired in the next implementation steps.
        Err(ConmanError::Git {
            message: "repo_exists gRPC call not implemented yet".to_string(),
        })
    }
}
