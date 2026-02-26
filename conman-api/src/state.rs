use std::sync::Arc;

use conman_core::Config;
use conman_git::GitAdapter;
use mongodb::Database;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Database,
    pub git_adapter: Arc<dyn GitAdapter>,
}
