use serde::{Deserialize, Serialize};
use uuid::Uuid;

tokio::task_local! {
    static REQUEST_CONTEXT: RequestContext;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestContext {
    pub request_id: String,
    pub client_ip: Option<String>,
    pub user_agent: Option<String>,
}

impl RequestContext {
    pub fn new() -> Self {
        Self {
            request_id: Uuid::now_v7().to_string(),
            client_ip: None,
            user_agent: None,
        }
    }

    pub fn with_request_id(request_id: String) -> Self {
        Self {
            request_id,
            client_ip: None,
            user_agent: None,
        }
    }

    pub fn current_request_id() -> String {
        REQUEST_CONTEXT
            .try_with(|ctx| ctx.request_id.clone())
            .unwrap_or_else(|_| "unknown".to_string())
    }

    pub fn current() -> Option<Self> {
        REQUEST_CONTEXT.try_with(|ctx| ctx.clone()).ok()
    }

    pub(crate) async fn scope_request<F, Fut, R>(ctx: RequestContext, f: F) -> R
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = R>,
    {
        REQUEST_CONTEXT.scope(ctx, f()).await
    }
}

impl Default for RequestContext {
    fn default() -> Self {
        Self::new()
    }
}
