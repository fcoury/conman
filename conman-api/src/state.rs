use std::sync::Arc;

use conman_core::Config;
use mongodb::Database;

#[derive(Debug, Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Database,
    pub gitaly_channel: Option<tonic::transport::Channel>,
}
