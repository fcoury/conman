use std::sync::Arc;

use conman_core::Config;
use conman_git::GitAdapter;
use mongodb::Database;

use crate::rate_limit::FixedWindowRateLimiter;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Database,
    pub git_adapter: Arc<dyn GitAdapter>,
    pub rate_limiter: Arc<FixedWindowRateLimiter>,
}
