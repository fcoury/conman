use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{Json, http::StatusCode};
use axum::{
    extract::Request,
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{
    request_context::RequestContext,
    response::{ApiError, ApiErrorBody},
    state::AppState,
};

#[derive(Debug)]
struct WindowState {
    started_at: Instant,
    count: u64,
}

#[derive(Debug)]
pub struct FixedWindowRateLimiter {
    limit_per_second: u64,
    window: Mutex<WindowState>,
}

impl FixedWindowRateLimiter {
    pub fn new(limit_per_second: u64) -> Self {
        Self {
            limit_per_second: limit_per_second.max(1),
            window: Mutex::new(WindowState {
                started_at: Instant::now(),
                count: 0,
            }),
        }
    }

    pub fn allow(&self) -> bool {
        let mut state = self.window.lock().expect("rate limiter lock poisoned");
        if state.started_at.elapsed() >= Duration::from_secs(1) {
            state.started_at = Instant::now();
            state.count = 0;
        }
        if state.count >= self.limit_per_second {
            return false;
        }
        state.count += 1;
        true
    }
}

pub async fn rate_limit_middleware(
    axum::extract::State(state): axum::extract::State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    if state.rate_limiter.allow() {
        return next.run(req).await;
    }

    let body = ApiError {
        error: ApiErrorBody {
            code: "rate_limited",
            message: "Too many requests".to_string(),
            request_id: RequestContext::current_request_id(),
        },
    };
    (StatusCode::TOO_MANY_REQUESTS, Json(body)).into_response()
}

pub fn new_shared_rate_limiter(limit_per_second: u64) -> Arc<FixedWindowRateLimiter> {
    Arc::new(FixedWindowRateLimiter::new(limit_per_second))
}
