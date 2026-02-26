# E12 Hardening and Launch Readiness

## 1. Goal

Stabilize Conman for production rollout. This epic covers load testing, fault
injection, SLO definition, operational dashboards, runbooks, rate limiting, and a
security checklist. Unlike previous epics, E12 produces mostly tests,
configuration, documentation, and observability infrastructure rather than new
domain features.

After this epic, the team has concrete evidence that the system handles
production-scale traffic, degrades gracefully under failure, and operators have
the runbooks and alerts needed to respond to incidents.

**Issues:**

- E12-01: Load and performance testing against large real repositories.
- E12-02: Fault-injection tests for Git adapter and job worker crashes.
- E12-03: SLO definitions and operational dashboards (queue depth, job latency,
  deployment success rate).
- E12-04: Runbooks for release failure, revalidation storms, temp env cleanup.
- E12-05: Security checklist (password policy, token expiry, RBAC tests, input
  validation).

---

## 2. Dependencies

| Dependency | What it provides |
|------------|------------------|
| E08 Releases | Release assembly, composition, tagging, and publish flows to load test |
| E09 Deployments | Deploy, promote, skip-stage, rollback flows to fault-test |
| E10 Temp Environments | Temp env lifecycle to verify cleanup under failure |
| E11 Notifications & Audit | Audit completeness and notification delivery to verify under load |

All prior epics (E00-E11) must be functionally complete before E12 begins. E12
validates the entire system as an integrated whole.

---

## 3. Rust Types

### 3.1 Metrics Registry (`conman-api/src/metrics.rs`)

Integration with the `metrics` crate for lightweight, Prometheus-compatible
instrumentation. The `metrics` crate provides a facade; the
`metrics-exporter-prometheus` crate provides the Prometheus text format exporter.

```rust
use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

/// Initialize the global metrics recorder and return a handle for the
/// Prometheus scrape endpoint.
///
/// Called once during server startup. Subsequent calls to `counter!`,
/// `gauge!`, and `histogram!` anywhere in the codebase will record into
/// this registry.
pub fn init_metrics() -> PrometheusHandle {
    PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install Prometheus metrics recorder")
}

// ── Metric names (constants prevent typos across crates) ──

/// Total HTTP requests received, labeled by method, path pattern, and status.
pub const HTTP_REQUESTS_TOTAL: &str = "conman_http_requests_total";

/// HTTP request duration in seconds, labeled by method and path pattern.
pub const HTTP_REQUEST_DURATION_SECONDS: &str = "conman_http_request_duration_seconds";

/// Total jobs enqueued, labeled by job type.
pub const JOBS_ENQUEUED_TOTAL: &str = "conman_jobs_enqueued_total";

/// Total jobs completed, labeled by job type and outcome (succeeded/failed).
pub const JOBS_COMPLETED_TOTAL: &str = "conman_jobs_completed_total";

/// Job processing duration in seconds, labeled by job type.
pub const JOB_DURATION_SECONDS: &str = "conman_job_duration_seconds";

/// Current number of jobs in `queued` state, labeled by job type.
pub const JOB_QUEUE_DEPTH: &str = "conman_job_queue_depth";

/// Total deployments attempted, labeled by outcome (succeeded/failed/canceled).
pub const DEPLOYMENTS_TOTAL: &str = "conman_deployments_total";

/// Total gitaly gRPC calls, labeled by method and outcome.
pub const GITALY_CALLS_TOTAL: &str = "conman_gitaly_calls_total";

/// Gitaly gRPC call duration in seconds, labeled by method.
pub const GITALY_CALL_DURATION_SECONDS: &str = "conman_gitaly_call_duration_seconds";

/// Total authentication failures (bad password, expired token, etc.).
pub const AUTH_FAILURES_TOTAL: &str = "conman_auth_failures_total";

/// Total rate-limited requests.
pub const RATE_LIMITED_TOTAL: &str = "conman_rate_limited_total";
```

### 3.2 HTTP Metrics Middleware (`conman-api/src/middleware/metrics.rs`)

Records request count and duration for every HTTP request.

```rust
use axum::{extract::Request, middleware::Next, response::Response};
use metrics::{counter, histogram};
use std::time::Instant;

use crate::metrics::{HTTP_REQUESTS_TOTAL, HTTP_REQUEST_DURATION_SECONDS};

/// Middleware that records HTTP request count and latency.
///
/// Labels: `method`, `path` (matched route pattern, not raw URL), `status`.
/// Must be applied as an outer layer so it captures the full request lifecycle.
pub async fn http_metrics_middleware(req: Request, next: Next) -> Response {
    let method = req.method().to_string();

    // Use the matched path pattern (e.g. "/api/apps/:appId") to avoid
    // high-cardinality labels from path parameters.
    let path = req
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(|mp| mp.as_str().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let start = Instant::now();
    let response = next.run(req).await;
    let duration = start.elapsed().as_secs_f64();

    let status = response.status().as_u16().to_string();

    counter!(HTTP_REQUESTS_TOTAL, "method" => method.clone(), "path" => path.clone(), "status" => status);
    histogram!(HTTP_REQUEST_DURATION_SECONDS, "method" => method, "path" => path).record(duration);

    response
}
```

### 3.3 Enhanced Health Check (`conman-api/src/handlers/health.rs`)

Extends the E00 health endpoint with per-component status and version metadata.

```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use crate::state::AppState;

/// Component-level health status.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentStatus {
    /// Component is reachable and responding within acceptable latency.
    Healthy,
    /// Component is reachable but degraded (e.g. high latency, replica lag).
    Degraded,
    /// Component is unreachable or returning errors.
    Unhealthy,
}

/// Individual component health report.
#[derive(Debug, Clone, Serialize)]
pub struct ComponentHealth {
    pub name: &'static str,
    pub status: ComponentStatus,
    /// Human-readable detail (e.g. "ping: 2ms", "connection refused").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Enhanced health response with component-level breakdown.
#[derive(Debug, Clone, Serialize)]
pub struct HealthResponse {
    /// Overall status: "ok" if all components healthy, "degraded" otherwise.
    pub status: &'static str,
    /// Application version from compile-time env.
    pub version: &'static str,
    /// Individual component checks.
    pub components: Vec<ComponentHealth>,
}

/// GET /api/health
///
/// Returns detailed health status for each dependency. Returns 200 when all
/// components are healthy, 503 when any component is unhealthy. Does not
/// require authentication.
pub async fn health_check(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let mut components = Vec::with_capacity(3);
    let mut all_healthy = true;

    // Check MongoDB connectivity.
    let mongo_health = match check_mongo(&state).await {
        Ok(detail) => ComponentHealth {
            name: "mongodb",
            status: ComponentStatus::Healthy,
            detail: Some(detail),
        },
        Err(detail) => {
            all_healthy = false;
            ComponentHealth {
                name: "mongodb",
                status: ComponentStatus::Unhealthy,
                detail: Some(detail),
            }
        }
    };
    components.push(mongo_health);

    // Check Gitaly gRPC channel.
    let gitaly_health = match check_gitaly(&state).await {
        Ok(detail) => ComponentHealth {
            name: "gitaly",
            status: ComponentStatus::Healthy,
            detail: Some(detail),
        },
        Err(detail) => {
            all_healthy = false;
            ComponentHealth {
                name: "gitaly",
                status: ComponentStatus::Unhealthy,
                detail: Some(detail),
            }
        }
    };
    components.push(gitaly_health);

    // Check job runner liveness.
    let job_runner_health = match check_job_runner(&state).await {
        Ok(detail) => ComponentHealth {
            name: "job_runner",
            status: ComponentStatus::Healthy,
            detail: Some(detail),
        },
        Err(detail) => {
            all_healthy = false;
            ComponentHealth {
                name: "job_runner",
                status: ComponentStatus::Unhealthy,
                detail: Some(detail),
            }
        }
    };
    components.push(job_runner_health);

    let status = if all_healthy { "ok" } else { "degraded" };
    let http_status = if all_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        http_status,
        Json(HealthResponse {
            status,
            version: env!("CARGO_PKG_VERSION"),
            components,
        }),
    )
}

/// Ping MongoDB and return round-trip time.
async fn check_mongo(state: &AppState) -> Result<String, String> {
    let start = std::time::Instant::now();
    conman_db::check_mongo_health(&state.db)
        .await
        .map(|_| format!("ping: {}ms", start.elapsed().as_millis()))
        .map_err(|e| e.to_string())
}

/// Verify the Gitaly gRPC channel is connected.
async fn check_gitaly(state: &AppState) -> Result<String, String> {
    match &state.gitaly_channel {
        Some(_channel) => {
            // Attempt a lightweight ServerInfo or similar RPC.
            // For now, channel existence indicates the connection was established.
            Ok("channel connected".to_string())
        }
        None => Err("channel not available".to_string()),
    }
}

/// Verify the job runner is alive by checking its heartbeat timestamp.
async fn check_job_runner(state: &AppState) -> Result<String, String> {
    // The job runner writes a heartbeat timestamp to a known MongoDB document.
    // If the heartbeat is older than 60 seconds, the runner is considered unhealthy.
    let _ = state;
    // Implementation: query `job_runner_heartbeat` document from MongoDB.
    // Placeholder -- will be wired when E06 job runner is available.
    Ok("heartbeat current".to_string())
}
```

### 3.4 Rate Limiter (`conman-api/src/middleware/rate_limit.rs`)

Per-user rate limiting using a token bucket algorithm backed by an in-memory
store. For single-instance v1 deployment this is sufficient; a Redis-backed
implementation can replace the store later without changing the middleware.

```rust
use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::response::{ApiError, ApiErrorBody};
use crate::request_context::RequestContext;

/// Configuration for the rate limiter.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per window per user.
    pub max_requests: u64,
    /// Window duration.
    pub window: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window: Duration::from_secs(60),
        }
    }
}

/// Per-user token bucket entry.
#[derive(Debug, Clone)]
struct BucketEntry {
    remaining: u64,
    window_start: Instant,
}

/// In-memory rate limit store. One entry per authenticated user.
///
/// Thread-safe via `DashMap`. Entries are lazily evicted when accessed
/// past their window.
#[derive(Debug, Clone)]
pub struct RateLimitStore {
    buckets: Arc<DashMap<String, BucketEntry>>,
    config: RateLimitConfig,
}

impl RateLimitStore {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            buckets: Arc::new(DashMap::new()),
            config,
        }
    }

    /// Attempt to consume one token for the given user.
    /// Returns `Ok(remaining)` if allowed, `Err(())` if rate limited.
    pub fn check(&self, user_id: &str) -> Result<u64, ()> {
        let now = Instant::now();
        let mut entry = self.buckets.entry(user_id.to_string()).or_insert_with(|| {
            BucketEntry {
                remaining: self.config.max_requests,
                window_start: now,
            }
        });

        // Reset window if expired.
        if now.duration_since(entry.window_start) >= self.config.window {
            entry.remaining = self.config.max_requests;
            entry.window_start = now;
        }

        if entry.remaining == 0 {
            return Err(());
        }

        entry.remaining -= 1;
        Ok(entry.remaining)
    }
}

/// Rate limiting middleware.
///
/// Extracts the authenticated user ID from request extensions (set by the
/// auth middleware). Unauthenticated requests are not rate-limited here
/// (they are rejected by the auth middleware first).
///
/// Returns 429 Too Many Requests when the limit is exceeded.
pub async fn rate_limit_middleware(
    req: Request,
    next: Next,
    store: RateLimitStore,
) -> Response {
    // Extract user ID from auth context if present.
    let user_id = req
        .extensions()
        .get::<crate::auth::AuthUser>()
        .map(|u| u.user_id.to_string());

    if let Some(uid) = user_id {
        match store.check(&uid) {
            Ok(remaining) => {
                let mut response = next.run(req).await;
                // Attach rate limit headers for client awareness.
                if let Ok(val) = remaining.to_string().parse() {
                    response.headers_mut().insert("X-RateLimit-Remaining", val);
                }
                response
            }
            Err(()) => {
                metrics::counter!(
                    crate::metrics::RATE_LIMITED_TOTAL,
                    "user_id" => uid
                );

                let body = ApiError {
                    error: ApiErrorBody {
                        code: "rate_limited",
                        message: "Too many requests. Please wait and try again.".to_string(),
                        request_id: RequestContext::current_request_id(),
                    },
                };

                (StatusCode::TOO_MANY_REQUESTS, axum::Json(body)).into_response()
            }
        }
    } else {
        // No authenticated user -- skip rate limiting.
        next.run(req).await
    }
}
```

---

## 4. Database

### 4.1 Index Review

E12 does not introduce new collections. Instead, it audits all existing indexes
across every collection for query performance under load. The following index
review must be completed and verified:

| Collection | Index | Purpose | Type |
|------------|-------|---------|------|
| `apps` | `{ name: 1 }` | App lookup by name | unique |
| `apps` | `{ repo_path: 1 }` | App lookup by repo path | unique |
| `app_memberships` | `{ user_id: 1, app_id: 1 }` | Membership lookup | unique compound |
| `app_memberships` | `{ app_id: 1 }` | List members of an app | standard |
| `workspaces` | `{ app_id: 1, owner_user_id: 1 }` | User's workspaces per app | compound |
| `workspaces` | `{ app_id: 1, branch_name: 1 }` | Branch uniqueness per app | unique compound |
| `changesets` | `{ app_id: 1, state: 1 }` | List changesets by state (queue view) | compound |
| `changesets` | `{ workspace_id: 1, state: 1 }` | One open changeset per workspace | compound |
| `changesets` | `{ app_id: 1, author_user_id: 1 }` | User's changesets per app | compound |
| `changeset_revisions` | `{ changeset_id: 1, revision_number: 1 }` | Revision lookup | unique compound |
| `changeset_reviews` | `{ changeset_id: 1 }` | Reviews for a changeset | standard |
| `changeset_comments` | `{ changeset_id: 1, created_at: 1 }` | Paginated comment listing | compound |
| `release_batches` | `{ app_id: 1, state: 1 }` | Releases by state | compound |
| `release_batches` | `{ app_id: 1, tag: 1 }` | Release lookup by tag | unique compound |
| `release_changesets` | `{ release_id: 1 }` | Changesets in a release | standard |
| `environments` | `{ app_id: 1, name: 1 }` | Env name uniqueness per app | unique compound |
| `environments` | `{ app_id: 1, position: 1 }` | Env position uniqueness per app | unique compound |
| `deployments` | `{ app_id: 1, environment_id: 1, state: 1 }` | Active deployment lock per env | compound |
| `deployments` | `{ release_id: 1 }` | Deployments for a release | standard |
| `temp_environments` | `{ app_id: 1, state: 1 }` | Active temp envs per app | compound |
| `temp_environments` | `{ expires_at: 1 }` | TTL expiry scan (job runner) | standard |
| `jobs` | `{ state: 1, created_at: 1 }` | Job polling (FIFO by state) | compound |
| `jobs` | `{ app_id: 1, type: 1, state: 1 }` | Job lookup by app and type | compound |
| `audit_events` | `{ app_id: 1, occurred_at: -1 }` | Audit timeline per app | compound |
| `audit_events` | `{ entity_type: 1, entity_id: 1 }` | Audit for specific entity | compound |
| `notification_preferences` | `{ user_id: 1 }` | User prefs lookup | unique |
| `invites` | `{ app_id: 1, email: 1 }` | Invite uniqueness per app | unique compound |
| `invites` | `{ token: 1 }` | Invite acceptance lookup | unique |

**Action items:**

- Run `db.collection.getIndexes()` for every collection and compare against the
  table above.
- Run `db.collection.aggregate([{$indexStats:{}}])` to identify unused indexes.
- For collections expected to exceed 1M documents in production (`audit_events`,
  `jobs`, `changeset_comments`), verify that common query patterns use index
  scans, not collection scans. Use `explain("executionStats")` on representative
  queries.

### 4.2 Read Preference and Write Concern

For production MongoDB replica set deployment:

| Operation type | Read preference | Write concern | Rationale |
|----------------|----------------|---------------|-----------|
| Health check ping | `primaryPreferred` | n/a | Tolerate primary failover for health |
| Reads (listings, detail) | `secondaryPreferred` | n/a | Spread read load; accept slight staleness |
| Writes (mutations) | n/a | `{ w: "majority", j: true }` | Durability: acknowledged by majority with journal |
| Job polling | `primary` | `{ w: "majority" }` | Avoid duplicate job pickup during failover |
| Audit event writes | n/a | `{ w: 1, j: false }` | Fire-and-forget; acceptable to lose rare event under crash |

These should be configured per-operation, not globally, using the MongoDB
driver's `ReadPreference` and `WriteConcern` options on individual collection
handles or operation options.

### 4.3 Backup Strategy

- **Frequency:** Automated daily full backup via `mongodump` or cloud provider
  snapshot (Atlas continuous backup if using Atlas).
- **Retention:** 30 days of daily backups, 7 days of oplog for point-in-time
  recovery.
- **Restore testing:** Monthly restore drill to a staging environment. Document
  restore time (target: < 30 minutes for databases under 10 GB).
- **Oplog sizing:** Ensure oplog window covers at least 24 hours of write
  activity so replica resync does not require full initial sync.

---

## 5. API Endpoints

### 5.1 `GET /api/health` (enhanced)

Replaces the E00 basic health check with component-level status.

| Attribute | Value |
|-----------|-------|
| Auth | None (public) |
| Rate limit | Exempt |

**Response 200 (all components healthy):**

```json
{
  "status": "ok",
  "version": "0.1.0",
  "components": [
    { "name": "mongodb", "status": "healthy", "detail": "ping: 2ms" },
    { "name": "gitaly", "status": "healthy", "detail": "channel connected" },
    { "name": "job_runner", "status": "healthy", "detail": "heartbeat current" }
  ]
}
```

**Response 503 (one or more components unhealthy):**

```json
{
  "status": "degraded",
  "version": "0.1.0",
  "components": [
    { "name": "mongodb", "status": "healthy", "detail": "ping: 3ms" },
    { "name": "gitaly", "status": "unhealthy", "detail": "channel not available" },
    { "name": "job_runner", "status": "healthy", "detail": "heartbeat current" }
  ]
}
```

### 5.2 `GET /api/metrics` (Prometheus scrape endpoint)

| Attribute | Value |
|-----------|-------|
| Auth | None (should be network-restricted in production via firewall/ingress rules) |
| Rate limit | Exempt |
| Content-Type | `text/plain; version=0.0.4; charset=utf-8` |

**Response 200:**

```
# HELP conman_http_requests_total Total HTTP requests received.
# TYPE conman_http_requests_total counter
conman_http_requests_total{method="GET",path="/api/apps",status="200"} 1042
conman_http_requests_total{method="POST",path="/api/apps/:appId/changesets",status="201"} 87

# HELP conman_http_request_duration_seconds HTTP request duration.
# TYPE conman_http_request_duration_seconds histogram
conman_http_request_duration_seconds_bucket{method="GET",path="/api/apps",le="0.1"} 980
...

# HELP conman_job_queue_depth Current queued jobs.
# TYPE conman_job_queue_depth gauge
conman_job_queue_depth{type="revalidate_queued_changeset"} 3
conman_job_queue_depth{type="deploy_release"} 0
...
```

Handler implementation:

```rust
use axum::response::IntoResponse;
use metrics_exporter_prometheus::PrometheusHandle;

/// GET /api/metrics
///
/// Returns metrics in Prometheus text exposition format. Not authenticated
/// -- restrict access via network policy in production.
pub async fn metrics_endpoint(
    State(handle): State<PrometheusHandle>,
) -> impl IntoResponse {
    handle.render()
}
```

---

## 6. Business Logic

### 6.1 Load Test Scenarios

All load tests use a dedicated test environment with a populated MongoDB and a
Gitaly instance backed by realistic repository data (not empty repos).

| # | Scenario | Parameters | Target | Tool |
|---|----------|------------|--------|------|
| L1 | Concurrent file edits | 50 users, each editing 5 files in their own workspace | All 250 edits succeed within 2s per request | `k6` or `drill` |
| L2 | Changeset submission storm | 50 concurrent changeset submissions, each triggering `msuite_submit` job | All 50 submissions accepted, jobs enqueued within 1s | `k6` |
| L3 | Queue with 100+ changesets | Seed 150 queued changesets, then publish a release of 10 | Post-publish revalidation of remaining 140 completes within 10 minutes | custom Rust test harness |
| L4 | Rapid release cycle | 5 releases published sequentially with 60s gap, each with 5 changesets | No data corruption, all revalidation loops complete, no orphaned jobs | custom Rust test harness |
| L5 | Large repository operations | Repository with 10,000+ files across 500+ directories. Perform tree listing, file read, diff operations | Tree listing < 3s, single file read < 500ms, diff < 5s | `k6` |
| L6 | Deployment pipeline | 10 concurrent deploy requests across different environments for different apps | Each deployment runs to completion, environment locks enforced, no double-deploys | `k6` |
| L7 | API listing under load | 50 concurrent requests to `GET /api/apps/:appId/changesets?state=queued` with 500 changesets in DB | p99 response time < 500ms, no timeouts | `k6` |

### 6.2 Fault Injection Scenarios

| # | Scenario | Injection method | Expected behavior |
|---|----------|------------------|-------------------|
| F1 | Gitaly connection drop | Kill gitaly process (or iptables drop) mid-request | API returns 502 `git_error`. Retry logic in `GitalyClient` attempts 3 retries with backoff. Non-git endpoints remain operational. Health endpoint reports gitaly unhealthy. |
| F2 | Gitaly slow response | Inject 10s delay on gitaly responses (tc netem or proxy) | Requests with git operations time out at configured deadline (default 30s). Client receives 504 or 502. Non-git endpoints unaffected. |
| F3 | MongoDB primary failover | `rs.stepDown()` on primary | Writes fail briefly during election (typically 2-10s). Health endpoint returns 503 during election. After new primary elected, operations resume automatically. No data loss for majority-acknowledged writes. |
| F4 | MongoDB full outage | Stop all replica set members | All API endpoints return 500/503. Health endpoint returns 503 with mongodb unhealthy. Server does not crash. Recovery is automatic when MongoDB comes back. |
| F5 | Job worker crash mid-execution | `kill -9` the process while a job is in `running` state | Job remains in `running` state with stale `locked_until`. Job runner picks it up after lock expiry (configurable, default 5 minutes). Job is retried. Idempotency ensures no duplicate side effects. |
| F6 | Job worker crash during revalidation storm | Kill worker while 50+ revalidation jobs are in progress | All in-progress jobs are re-picked after lock expiry. Remaining queued jobs are processed normally. No changeset is left in an inconsistent state. |
| F7 | Network partition between API and job runner | Block network between API server and MongoDB for job runner only | API continues serving reads from secondary. Job runner stops picking jobs. Health endpoint shows job_runner degraded. When partition heals, job runner resumes. |

### 6.3 SLO Definitions

These SLOs apply to the production deployment. They are measured over a rolling
30-day window.

| SLO | Metric | Target | Measurement |
|-----|--------|--------|-------------|
| API availability | Successful (non-5xx) responses / total responses | >= 99.9% | Prometheus: `rate(conman_http_requests_total{status!~"5.."}[30d]) / rate(conman_http_requests_total[30d])` |
| API latency (p99) | 99th percentile response time for non-background endpoints | < 500ms | Prometheus: `histogram_quantile(0.99, rate(conman_http_request_duration_seconds_bucket[5m]))` |
| Job processing (p99) | 99th percentile time from job enqueue to completion | < 30s | Custom metric: `conman_job_duration_seconds` |
| Job processing (p99) for deployments | 99th percentile deploy job duration | < 120s | `conman_job_duration_seconds{type="deploy_release"}` |
| Deployment success rate | Succeeded deployments / total non-canceled deployments | >= 99% | Prometheus: `rate(conman_deployments_total{outcome="succeeded"}[30d]) / rate(conman_deployments_total{outcome!="canceled"}[30d])` |
| Revalidation turnaround | Time from release publish to all queued changeset revalidations complete | < 10 minutes for 100 queued changesets | Custom metric with event timestamps |

**Alert thresholds (Prometheus alerting rules):**

| Alert | Condition | Severity | Action |
|-------|-----------|----------|--------|
| `ConmanHighErrorRate` | 5xx rate > 1% over 5 minutes | P1 | Page on-call |
| `ConmanHighLatency` | p99 latency > 1s over 5 minutes | P2 | Notify on-call |
| `ConmanJobQueueBacklog` | `conman_job_queue_depth` > 50 for any type for 10 minutes | P2 | Notify on-call, check job runner health |
| `ConmanJobRunnerDown` | Job runner heartbeat stale > 2 minutes | P1 | Page on-call, restart job runner |
| `ConmanGitalyUnhealthy` | Health check reports gitaly unhealthy for 2 minutes | P1 | Page on-call, check gitaly process |
| `ConmanMongoUnhealthy` | Health check reports mongodb unhealthy for 1 minute | P1 | Page on-call, check replica set |
| `ConmanDeploymentFailure` | Any deployment enters `failed` state | P2 | Notify config manager and on-call |
| `ConmanTempEnvLeaking` | Temp environments in `expired` state with `grace_until` in the past > 1 hour | P3 | Investigate cleanup job |

### 6.4 Rate Limiting

Per-user rate limits applied after authentication middleware:

| Scope | Limit | Window | Notes |
|-------|-------|--------|-------|
| Global per-user | 100 requests | 60 seconds | Applies to all authenticated endpoints |
| Write endpoints (POST/PUT/PATCH/DELETE) | 30 requests | 60 seconds | Prevents mutation storms |
| Auth endpoints (`/api/auth/*`) | 10 requests | 60 seconds | Brute-force protection (per IP, not per user) |

Rate limit response (HTTP 429):

```json
{
  "error": {
    "code": "rate_limited",
    "message": "Too many requests. Please wait and try again.",
    "request_id": "req-uuid"
  }
}
```

Response headers on all authenticated requests:

```
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 73
X-RateLimit-Reset: 1719849600
```

### 6.5 Security Checklist

Every item must be verified with a passing test before launch.

**Authentication:**

| # | Item | Requirement | Verification |
|---|------|-------------|--------------|
| S1 | Password minimum length | >= 12 characters | Unit test in `conman-auth` |
| S2 | Password hashing algorithm | Argon2id with `m_cost=19456`, `t_cost=2`, `p_cost=1` (OWASP recommendation) | Unit test verifying hash format |
| S3 | Password hash timing | Verification takes 100-500ms (prevents timing attacks while maintaining UX) | Benchmark test |
| S4 | JWT expiry | 24 hours (configurable via `CONMAN_JWT_EXPIRY_HOURS`) | Integration test: token issued, wait (or mock time), verify rejection |
| S5 | JWT secret strength | Minimum 32 bytes, validated at startup | Startup validation in `Config::from_env()` |
| S6 | Invite token expiry | 7 days (configurable via `CONMAN_INVITE_EXPIRY_DAYS`) | Integration test: expired invite rejected |
| S7 | Password reset token | Single-use, 1-hour expiry | Integration test: used token rejected on second use |
| S8 | Failed login throttling | After 5 failed attempts for same email, enforce 15-minute cooldown | Integration test |

**Authorization (RBAC):**

| # | Item | Requirement | Verification |
|---|------|-------------|--------------|
| S9 | `user` cannot approve changeset | Returns 403 | Integration test |
| S10 | `user` cannot assemble release | Returns 403 | Integration test |
| S11 | `user` cannot deploy | Returns 403 | Integration test |
| S12 | `reviewer` cannot assemble release | Returns 403 | Integration test |
| S13 | `reviewer` cannot manage settings | Returns 403 | Integration test |
| S14 | Non-member cannot access app | Returns 403 | Integration test |
| S15 | Role escalation via API | Sending `role: "app_admin"` in membership update as non-admin returns 403 | Integration test |
| S16 | Cross-app access | User with role on app A cannot access app B resources | Integration test |

**Input validation:**

| # | Item | Requirement | Verification |
|---|------|-------------|--------------|
| S17 | NoSQL injection in query params | `?name[$gt]=` and similar MongoDB operator injection attempts are rejected or sanitized | Integration test |
| S18 | NoSQL injection in JSON body | `{"name": {"$gt": ""}}` rejected by type-safe deserialization (serde rejects objects where String expected) | Unit test |
| S19 | Path traversal in file operations | `../../etc/passwd` and similar traversal in file path parameter is blocked | Integration test |
| S20 | Path traversal with encoded chars | `..%2F..%2Fetc%2Fpasswd` is blocked | Integration test |
| S21 | Blocked path enforcement | Editing `.git/config` or `.github/workflows/ci.yml` returns 403 | Integration test |
| S22 | File size limit enforcement | Upload exceeding `file_size_limit_bytes` returns 400 | Integration test |
| S23 | Request body size limit | Request bodies > 10 MB rejected at middleware level | Integration test |
| S24 | XSS in changeset comments | HTML/script tags in comment body are stored as-is (no execution context in API-only backend) but validated for max length | Unit test |
| S25 | Branch name injection | Workspace branch names cannot contain `..`, leading `-`, or shell metacharacters | Unit test |

---

## 7. Gitaly-rs Integration

### 7.1 Connection Resilience Testing

The `GitalyClient` retry logic (introduced in E01) must be validated under
adversarial conditions:

| Test | Setup | Expected |
|------|-------|----------|
| Retry on `UNAVAILABLE` | Mock gitaly returns `UNAVAILABLE` twice, then success | Operation succeeds after 3rd attempt |
| Retry on `DEADLINE_EXCEEDED` | Mock gitaly returns `DEADLINE_EXCEEDED` once, then success | Operation succeeds after 2nd attempt |
| No retry on `NOT_FOUND` | Mock gitaly returns `NOT_FOUND` | Operation fails immediately, no retry |
| No retry on `INVALID_ARGUMENT` | Mock gitaly returns `INVALID_ARGUMENT` | Operation fails immediately, no retry |
| Max retries exhausted | Mock gitaly returns `UNAVAILABLE` 4 times | Operation fails after 3 retries |
| Backoff timing | Mock gitaly returns `UNAVAILABLE` 3 times, measure delays | Delays follow exponential backoff: ~100ms, ~200ms, ~400ms (+/- jitter) |
| Channel reconnect after restart | Stop gitaly, wait 5s, restart gitaly, make request | Request succeeds (Tonic channel reconnects automatically) |

### 7.2 Timeout Configuration

| Operation | Recommended timeout | Rationale |
|-----------|-------------------|-----------|
| `RepositoryExists` / `CreateRepository` | 5s | Lightweight metadata operations |
| `TreeEntry` / `GetBlob` | 10s | File reads scale with file size |
| `CommitDiff` | 30s | Diffs on large changesets can be expensive |
| `UserCommitFiles` | 30s | Streaming writes for multi-file commits |
| `MergeToRef` / `UserMergeBranch` | 60s | Merge operations on large repos may be slow |

These timeouts should be configurable via environment variables:

```
CONMAN_GITALY_TIMEOUT_DEFAULT=10s
CONMAN_GITALY_TIMEOUT_DIFF=30s
CONMAN_GITALY_TIMEOUT_MERGE=60s
```

---

## 8. Implementation Checklist

This epic is test-heavy and configuration-focused. Steps are organized by
sub-issue rather than sequential commits.

### E12-01: Load and Performance Testing

- [ ] Set up load testing infrastructure: `k6` scripts in `tests/load/` directory
- [ ] Create test data seeding script: 10 apps, 50 users, 100 workspaces, 500
  changesets across various states, 1 repo with 10,000+ files
- [ ] Write L1: 50 concurrent users editing files
- [ ] Write L2: 50 concurrent changeset submissions
- [ ] Write L3: queue with 150 changesets, release 10, revalidate 140
- [ ] Write L4: rapid release cycle (5 releases in 5 minutes)
- [ ] Write L5: large repo tree listing, file read, diff
- [ ] Write L6: 10 concurrent deployments
- [ ] Write L7: listing endpoint under load (500 changesets, 50 concurrent readers)
- [ ] Run all load tests, record baseline results, identify bottlenecks
- [ ] Add slow query logging: log any MongoDB operation taking > 100ms
- [ ] Verify all listing queries use index scans via `explain()`

### E12-02: Fault Injection Testing

- [ ] Write F1: gitaly connection drop test
- [ ] Write F2: gitaly slow response test
- [ ] Write F3: MongoDB primary failover test
- [ ] Write F4: MongoDB full outage test
- [ ] Write F5: job worker crash mid-execution test
- [ ] Write F6: job worker crash during revalidation storm test
- [ ] Write F7: network partition test
- [ ] Verify graceful degradation: non-affected endpoints remain operational
  during each fault
- [ ] Verify health endpoint accurately reflects component status during each
  fault
- [ ] Document recovery time for each fault scenario

### E12-03: SLOs and Operational Dashboards

- [ ] Add `metrics` and `metrics-exporter-prometheus` to workspace dependencies
- [ ] Implement `init_metrics()` in `conman-api/src/metrics.rs`
- [ ] Implement HTTP metrics middleware (request count, duration)
- [ ] Add `GET /api/metrics` Prometheus scrape endpoint
- [ ] Instrument `conman-jobs`: job enqueue, completion, and duration metrics
- [ ] Instrument `conman-git`: gitaly call count and duration metrics
- [ ] Instrument `conman-auth`: auth failure counter
- [ ] Add `conman_job_queue_depth` gauge (updated by job runner poll loop)
- [ ] Add `conman_deployments_total` counter in deployment handlers
- [ ] Enhance `GET /api/health` with component-level status (mongo, gitaly, job_runner)
- [ ] Create Grafana dashboard JSON (or Prometheus recording rules) for:
  - Request rate and error rate
  - p50/p95/p99 latency
  - Job queue depth over time
  - Job processing duration by type
  - Deployment success/failure rate
  - Gitaly call latency
- [ ] Create Prometheus alerting rules for all alerts in section 6.3
- [ ] Write test verifying `/api/metrics` returns valid Prometheus text format

### E12-04: Runbooks

All runbooks written as markdown in `docs/runbooks/`. Each follows the template:
**Trigger**, **Impact**, **Diagnosis**, **Resolution**, **Prevention**.

- [ ] Runbook: Release assembly failure
  - Trigger: `release_assemble` job fails
  - Steps: identify conflicting changeset, mark conflicted, retry without it
- [ ] Runbook: Revalidation storm
  - Trigger: `ConmanJobQueueBacklog` alert fires after a release publish
  - Steps: check job runner health, scale if needed, monitor queue drain rate,
    pause revalidation if queue > 500 to prevent cascading load
- [ ] Runbook: Temp environment cleanup failure
  - Trigger: `ConmanTempEnvLeaking` alert fires
  - Steps: check `temp_env_expire` jobs, manually expire stuck envs, verify
    grace period logic
- [ ] Runbook: Gitaly outage
  - Trigger: `ConmanGitalyUnhealthy` alert fires
  - Steps: check gitaly process, check disk space, check gRPC connectivity,
    restart if needed, verify API recovery
- [ ] Runbook: MongoDB failover
  - Trigger: `ConmanMongoUnhealthy` alert fires
  - Steps: check replica set status, verify new primary elected, check for
    write concern errors in logs, verify application recovery
- [ ] Runbook: Deployment failure
  - Trigger: `ConmanDeploymentFailure` alert fires
  - Steps: check deployment logs, identify root cause, decide between retry and
    rollback, execute remediation
- [ ] Runbook: Authentication issues
  - Trigger: spike in `conman_auth_failures_total`
  - Steps: check for brute-force attempts, verify JWT secret unchanged, check
    token expiry clock skew
- [ ] Write go-live checklist (see section 10)

### E12-05: Security Hardening

- [ ] Implement rate limiting middleware in `conman-api`
- [ ] Add `RateLimitStore` and `RateLimitConfig` types
- [ ] Apply global rate limit (100/min per user) to authenticated routes
- [ ] Apply write rate limit (30/min per user) to mutation routes
- [ ] Apply auth rate limit (10/min per IP) to `/api/auth/*` routes
- [ ] Add request body size limit middleware (10 MB max)
- [ ] Verify password minimum length (>= 12 chars) in `conman-auth`
- [ ] Verify Argon2id parameters match OWASP recommendation
- [ ] Verify JWT minimum secret length at startup (>= 32 bytes)
- [ ] Write S1-S8 authentication security tests
- [ ] Write S9-S16 RBAC security tests
- [ ] Write S17-S25 input validation security tests
- [ ] Run dependency audit: `cargo audit` with no critical vulnerabilities
- [ ] Review all `.unwrap()` calls in non-test code -- replace with proper
  error handling
- [ ] Verify no secrets logged in tracing output (grep for password, token,
  secret in log format strings)

---

## 9. Test Cases

### 9.1 Load test: 50 concurrent users editing files

```javascript
// k6 script: tests/load/concurrent_edits.js
import http from 'k6/http';
import { check, sleep } from 'k6';

export const options = {
    vus: 50,
    duration: '2m',
    thresholds: {
        http_req_duration: ['p(99)<2000'],  // p99 < 2s
        http_req_failed: ['rate<0.01'],      // < 1% failure rate
    },
};

export default function () {
    const userId = __VU;
    const workspaceId = `ws-${userId}`;
    const fileName = `config/service-${userId}/settings.json`;

    const res = http.put(
        `${__ENV.BASE_URL}/api/apps/${__ENV.APP_ID}/workspaces/${workspaceId}/files`,
        JSON.stringify({ path: fileName, content: `{"vu": ${userId}, "iter": ${__ITER}}` }),
        { headers: { 'Authorization': `Bearer ${__ENV.TOKEN}`, 'Content-Type': 'application/json' } }
    );

    check(res, {
        'status is 200': (r) => r.status === 200,
        'response time < 2s': (r) => r.timings.duration < 2000,
    });

    sleep(1);
}
```

### 9.2 Fault test: gitaly goes down, API returns 503 gracefully

```rust
#[tokio::test]
async fn gitaly_down_returns_502_for_git_operations() {
    // Start mock gitaly that immediately drops connections.
    let mock_gitaly = MockGitalyServer::start_refusing_connections().await;
    let app = test_app_with_gitaly(mock_gitaly.address()).await;

    // Non-git endpoint should still work.
    let health_res = app
        .oneshot(Request::builder().uri("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    // Health endpoint reports gitaly as unhealthy but responds 503 overall.
    assert_eq!(health_res.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: serde_json::Value = parse_body(health_res).await;
    assert_eq!(body["status"], "degraded");

    let gitaly_component = body["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "gitaly")
        .unwrap();
    assert_eq!(gitaly_component["status"], "unhealthy");

    // Git operation should return 502.
    let file_res = app_clone
        .oneshot(
            Request::builder()
                .uri("/api/apps/test-app/workspaces/ws-1/files?path=config.json")
                .header("Authorization", "Bearer valid-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(file_res.status(), StatusCode::BAD_GATEWAY);

    let err_body: serde_json::Value = parse_body(file_res).await;
    assert_eq!(err_body["error"]["code"], "git_error");
}
```

### 9.3 Fault test: MongoDB primary failover, operations resume

```rust
#[tokio::test]
async fn mongo_failover_recovers_automatically() {
    // Requires a 3-node replica set (testcontainers or local).
    let rs = MongoReplicaSet::start(3).await;
    let app = test_app_with_mongo(rs.connection_string()).await;

    // Verify initial connectivity.
    let res = app.clone().oneshot(health_request()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Force primary step-down.
    rs.step_down_primary().await;

    // Requests may fail briefly during election.
    tokio::time::sleep(Duration::from_secs(2)).await;

    // After election completes, operations should resume.
    // Retry for up to 15 seconds.
    let mut recovered = false;
    for _ in 0..15 {
        let res = app.clone().oneshot(health_request()).await.unwrap();
        if res.status() == StatusCode::OK {
            recovered = true;
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(recovered, "Server did not recover after MongoDB failover within 15s");
}
```

### 9.4 Fault test: job worker crashes, job re-picked on restart

```rust
#[tokio::test]
async fn crashed_job_is_retried_after_lock_expiry() {
    let db = test_mongo_db().await;
    let job_repo = JobRepo::new(db.clone());

    // Insert a job in "running" state with a stale lock (expired 1 minute ago).
    let stale_job = Job {
        id: ObjectId::new().to_hex(),
        job_type: "msuite_submit".to_string(),
        state: "running".to_string(),
        locked_until: Some(Utc::now() - Duration::from_secs(60)),
        attempts: 1,
        max_attempts: 3,
        ..test_job()
    };
    job_repo.insert(&stale_job).await.unwrap();

    // Start a new job runner instance.
    let runner = JobRunner::new(db.clone(), mock_workers());
    let picked = runner.poll_next_job().await.unwrap();

    // The stale job should be picked up for retry.
    assert!(picked.is_some());
    assert_eq!(picked.unwrap().id, stale_job.id);
    assert_eq!(picked.unwrap().attempts, 2);
}
```

### 9.5 Security test: NoSQL injection attempts blocked

```rust
#[tokio::test]
async fn nosql_injection_in_query_param_rejected() {
    let app = test_app().await;

    // Attempt MongoDB operator injection via query parameter.
    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/apps?name[$gt]=")
                .header("Authorization", "Bearer valid-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should not return all apps -- either 400 (bad param) or empty results.
    // Must NOT return documents where name > "".
    assert!(
        res.status() == StatusCode::BAD_REQUEST || {
            let body: serde_json::Value = parse_body(res).await;
            body["data"].as_array().map_or(true, |arr| arr.is_empty())
        }
    );
}

#[tokio::test]
async fn nosql_injection_in_json_body_rejected() {
    let app = test_app().await;

    // Attempt operator injection in JSON body.
    // Serde's typed deserialization rejects objects where a String is expected.
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/apps")
                .header("Authorization", "Bearer admin-token")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"name": {"$gt": ""}, "repo_path": "test.git"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    // Serde deserialization fails: name expects a string, not an object.
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
```

### 9.6 Security test: path traversal in file operations blocked

```rust
#[tokio::test]
async fn path_traversal_blocked() {
    let app = test_app_with_workspace().await;

    let traversal_paths = vec![
        "../../etc/passwd",
        "..%2F..%2Fetc%2Fpasswd",
        "config/../../../etc/shadow",
        "/etc/passwd",
        "config/../../.git/config",
    ];

    for path in traversal_paths {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&format!(
                        "/api/apps/test-app/workspaces/ws-1/files?path={}",
                        path
                    ))
                    .header("Authorization", "Bearer valid-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(
            res.status() == StatusCode::BAD_REQUEST || res.status() == StatusCode::FORBIDDEN,
            "Path traversal not blocked for: {path}"
        );
    }
}
```

### 9.7 Security test: expired JWT rejected

```rust
#[tokio::test]
async fn expired_jwt_rejected() {
    let app = test_app().await;

    // Generate a JWT that expired 1 hour ago.
    let expired_token = issue_test_jwt(
        "user@example.com",
        Utc::now() - chrono::Duration::hours(1),
    );

    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/apps")
                .header("Authorization", format!("Bearer {expired_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = parse_body(res).await;
    assert_eq!(body["error"]["code"], "unauthorized");
}
```

### 9.8 Security test: role escalation attempts blocked

```rust
#[tokio::test]
async fn user_cannot_escalate_own_role() {
    let app = test_app_with_membership("user@test.com", Role::User).await;

    // Attempt to update own role to app_admin via the membership API.
    let res = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/apps/test-app/members/self")
                .header("Authorization", "Bearer user-token")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"role": "app_admin"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn reviewer_cannot_manage_settings() {
    let app = test_app_with_membership("reviewer@test.com", Role::Reviewer).await;

    let res = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/apps/test-app/settings")
                .header("Authorization", "Bearer reviewer-token")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"baseline_mode": "integration_head"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}
```

---

## 10. Acceptance Criteria

### Go-Live Checklist

Every item must be verified and signed off before production deployment.

**Performance:**

- [ ] All load test scenarios (L1-L7) pass their defined thresholds.
- [ ] API p99 latency is < 500ms under 50 concurrent users.
- [ ] Job processing p99 is < 30s for standard jobs, < 120s for deployments.
- [ ] All MongoDB listing queries confirmed to use index scans (no collection
  scans) via `explain()`.
- [ ] No MongoDB query takes > 100ms under normal load (verified by slow query
  log analysis).

**Resilience:**

- [ ] All fault injection scenarios (F1-F7) pass: system degrades gracefully
  and recovers automatically.
- [ ] Health endpoint accurately reports component status during each fault.
- [ ] Job runner correctly re-picks stale jobs after worker crash (F5, F6).
- [ ] No data loss or corruption observed during any fault injection test.
- [ ] MongoDB write concern set to `majority` for all critical writes.

**Observability:**

- [ ] `GET /api/metrics` returns valid Prometheus text format with all defined
  metrics.
- [ ] `GET /api/health` returns component-level status for mongodb, gitaly, and
  job_runner.
- [ ] Grafana dashboard (or equivalent) shows request rate, error rate, latency
  percentiles, job queue depth, and deployment success rate.
- [ ] All alerting rules from section 6.3 are configured and tested (fire
  expected alerts during fault injection).

**Operational:**

- [ ] Runbooks exist for: release failure, revalidation storm, temp env cleanup,
  gitaly outage, MongoDB failover, deployment failure, authentication issues.
- [ ] Each runbook has been walked through at least once during fault injection
  testing.
- [ ] Backup strategy documented and tested: restore completes within 30
  minutes.
- [ ] Log output is structured JSON with request_id correlation.
- [ ] No sensitive data (passwords, tokens, secrets) appears in log output
  (verified by grep across log samples).

**Security:**

- [ ] All S1-S25 security test cases pass.
- [ ] `cargo audit` reports no critical or high vulnerabilities.
- [ ] Password policy enforces minimum 12 characters.
- [ ] Argon2id parameters match OWASP recommendation.
- [ ] JWT expiry enforced; expired tokens rejected.
- [ ] Rate limiting active on all authenticated endpoints and auth endpoints.
- [ ] Request body size limit enforced (10 MB).
- [ ] Path traversal blocked in all file operation endpoints.
- [ ] NoSQL injection attempts blocked by typed deserialization.
- [ ] RBAC enforced for all permission-gated operations (full matrix tested).
- [ ] No `.unwrap()` calls in non-test production code (or each is documented
  as intentionally infallible).

**No P0 blockers remaining in the issue tracker.**
