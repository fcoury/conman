use std::{sync::OnceLock, time::Instant};

use axum::{extract::Request, middleware::Next, response::Response};
use metrics::{counter, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

static METRICS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

pub const HTTP_REQUESTS_TOTAL: &str = "conman_http_requests_total";
pub const HTTP_REQUEST_DURATION_SECONDS: &str = "conman_http_request_duration_seconds";

pub fn init_metrics() -> Result<(), String> {
    if METRICS_HANDLE.get().is_some() {
        return Ok(());
    }
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .map_err(|e| format!("failed to install prometheus recorder: {e}"))?;
    let _ = METRICS_HANDLE.set(handle);
    Ok(())
}

pub fn render_metrics() -> Option<String> {
    METRICS_HANDLE.get().map(PrometheusHandle::render)
}

pub async fn http_metrics_middleware(req: Request, next: Next) -> Response {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let start = Instant::now();
    let response = next.run(req).await;
    let status = response.status().as_u16().to_string();
    let elapsed = start.elapsed().as_secs_f64();

    counter!(
        HTTP_REQUESTS_TOTAL,
        "method" => method.clone(),
        "path" => path.clone(),
        "status" => status
    )
    .increment(1);
    histogram!(
        HTTP_REQUEST_DURATION_SECONDS,
        "method" => method,
        "path" => path
    )
    .record(elapsed);

    response
}
