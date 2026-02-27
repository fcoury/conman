# E00 Platform Foundation

## 1. Goal

Establish the service skeleton, shared primitives, and infrastructure plumbing so
that every subsequent epic builds on a consistent foundation of configuration,
error handling, request tracing, pagination, database connectivity, and secure
runtime-profile secrets configuration.

## 2. Dependencies

None. This is the root epic.

## 3. Rust Types

### 3.1 Config (`conman-core/src/config.rs`)

Loaded from environment variables with the `CONMAN_` prefix. Every field has a
sensible default except `jwt_secret`, which is required.

```rust
use std::net::SocketAddr;

/// Application configuration loaded from environment variables.
///
/// All variables use the `CONMAN_` prefix. Unknown variables are ignored.
/// The only required variable is `CONMAN_JWT_SECRET`.
#[derive(Debug, Clone)]
pub struct Config {
    /// HTTP listen address (host:port).
    /// Env: `CONMAN_HOST` (default `0.0.0.0`) + `CONMAN_PORT` (default `3000`).
    pub listen_addr: SocketAddr,

    /// MongoDB connection string.
    /// Env: `CONMAN_MONGO_URI` (default `mongodb://localhost:27017`).
    pub mongo_uri: String,

    /// MongoDB database name.
    /// Env: `CONMAN_MONGO_DB` (default `conman`).
    pub mongo_db: String,

    /// Gitaly-rs gRPC address.
    /// Env: `CONMAN_GITALY_ADDRESS` (default `http://localhost:8075`).
    pub gitaly_address: String,

    /// JWT signing secret. **Required** -- startup panics if absent.
    /// Env: `CONMAN_JWT_SECRET`.
    pub jwt_secret: String,

    /// JWT token lifetime in hours.
    /// Env: `CONMAN_JWT_EXPIRY_HOURS` (default `24`).
    pub jwt_expiry_hours: u64,

    /// Invite token lifetime in days.
    /// Env: `CONMAN_INVITE_EXPIRY_DAYS` (default `7`).
    pub invite_expiry_days: u64,

    /// Master key for envelope encryption of runtime-profile secrets.
    /// Env: `CONMAN_SECRETS_MASTER_KEY` (required).
    pub secrets_master_key: String,

    /// Domain suffix used for generated temp runtime profile URLs.
    /// Env: `CONMAN_TEMP_URL_DOMAIN` (required).
    pub temp_url_domain: String,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// Panics if `CONMAN_JWT_SECRET` is not set (fail-fast at startup).
    pub fn from_env() -> Result<Self, ConmanError> {
        let host = std::env::var("CONMAN_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port: u16 = std::env::var("CONMAN_PORT")
            .unwrap_or_else(|_| "3000".to_string())
            .parse()
            .map_err(|_| ConmanError::Validation {
                message: "CONMAN_PORT must be a valid u16".to_string(),
            })?;

        let listen_addr = format!("{host}:{port}")
            .parse()
            .map_err(|_| ConmanError::Validation {
                message: "CONMAN_HOST:CONMAN_PORT must form a valid socket address".to_string(),
            })?;

        let jwt_secret = std::env::var("CONMAN_JWT_SECRET").map_err(|_| {
            ConmanError::Validation {
                message: "CONMAN_JWT_SECRET is required".to_string(),
            }
        })?;

        let jwt_expiry_hours: u64 = std::env::var("CONMAN_JWT_EXPIRY_HOURS")
            .unwrap_or_else(|_| "24".to_string())
            .parse()
            .map_err(|_| ConmanError::Validation {
                message: "CONMAN_JWT_EXPIRY_HOURS must be a valid u64".to_string(),
            })?;

        let invite_expiry_days: u64 = std::env::var("CONMAN_INVITE_EXPIRY_DAYS")
            .unwrap_or_else(|_| "7".to_string())
            .parse()
            .map_err(|_| ConmanError::Validation {
                message: "CONMAN_INVITE_EXPIRY_DAYS must be a valid u64".to_string(),
            })?;

        let secrets_master_key = std::env::var("CONMAN_SECRETS_MASTER_KEY")
            .map_err(|_| ConmanError::Validation {
                message: "CONMAN_SECRETS_MASTER_KEY is required".to_string(),
            })?;

        let temp_url_domain = std::env::var("CONMAN_TEMP_URL_DOMAIN")
            .map_err(|_| ConmanError::Validation {
                message: "CONMAN_TEMP_URL_DOMAIN is required".to_string(),
            })?;

        Ok(Self {
            listen_addr,
            mongo_uri: std::env::var("CONMAN_MONGO_URI")
                .unwrap_or_else(|_| "mongodb://localhost:27017".to_string()),
            mongo_db: std::env::var("CONMAN_MONGO_DB")
                .unwrap_or_else(|_| "conman".to_string()),
            gitaly_address: std::env::var("CONMAN_GITALY_ADDRESS")
                .unwrap_or_else(|_| "http://localhost:8075".to_string()),
            jwt_secret,
            jwt_expiry_hours,
            invite_expiry_days,
            secrets_master_key,
            temp_url_domain,
        })
    }
}
```

### 3.2 ConmanError (`conman-core/src/error.rs`)

Central error enum used by every crate. HTTP mapping lives in `conman-api`.

```rust
/// Unified error type for the Conman domain.
///
/// Variants map 1:1 to HTTP status codes in the API layer.
/// Business logic crates return `Result<T, ConmanError>`.
#[derive(Debug, thiserror::Error)]
pub enum ConmanError {
    #[error("not found: {entity} {id}")]
    NotFound { entity: &'static str, id: String },

    #[error("conflict: {message}")]
    Conflict { message: String },

    #[error("forbidden: {message}")]
    Forbidden { message: String },

    #[error("unauthorized: {message}")]
    Unauthorized { message: String },

    #[error("validation: {message}")]
    Validation { message: String },

    #[error("invalid state transition: {from} -> {to}")]
    InvalidTransition { from: String, to: String },

    #[error("git error: {message}")]
    Git { message: String },

    #[error("internal: {message}")]
    Internal { message: String },
}
```

### 3.3 API Response Envelopes (`conman-api/src/response.rs`)

All API responses use one of two shapes: success or error.

```rust
use serde::Serialize;

/// Success envelope returned by all non-error API responses.
///
/// `data` holds the resource or list. `pagination` is present only for
/// list endpoints.
#[derive(Debug, Clone, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub data: T,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination: Option<PaginationMeta>,
}

/// Pagination metadata included in list responses.
#[derive(Debug, Clone, Serialize)]
pub struct PaginationMeta {
    pub page: u64,
    pub limit: u64,
    pub total: u64,
}

/// Error envelope returned by all error API responses.
///
/// Always includes `code`, `message`, and `request_id` for traceability.
#[derive(Debug, Clone, Serialize)]
pub struct ApiError {
    pub error: ApiErrorBody,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiErrorBody {
    /// Machine-readable error code (e.g. `"not_found"`, `"validation_error"`).
    pub code: &'static str,

    /// Human-readable description of what went wrong.
    pub message: String,

    /// The request ID that produced this error, for log correlation.
    pub request_id: String,
}

impl<T: Serialize> ApiResponse<T> {
    /// Wrap a single resource in the success envelope (no pagination).
    pub fn ok(data: T) -> Self {
        Self {
            data,
            pagination: None,
        }
    }

    /// Wrap a list of resources with pagination metadata.
    pub fn paginated(data: T, page: u64, limit: u64, total: u64) -> Self {
        Self {
            data,
            pagination: Some(PaginationMeta { page, limit, total }),
        }
    }
}
```

### 3.4 ConmanError -> HTTP Response (`conman-api/src/error.rs`)

Maps domain errors to status codes and builds the JSON error envelope.

```rust
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use conman_core::ConmanError;
use crate::response::{ApiError, ApiErrorBody};
use crate::request_context::RequestContext;

impl IntoResponse for ConmanError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            ConmanError::NotFound { .. }          => (StatusCode::NOT_FOUND, "not_found"),
            ConmanError::Conflict { .. }          => (StatusCode::CONFLICT, "conflict"),
            ConmanError::Forbidden { .. }         => (StatusCode::FORBIDDEN, "forbidden"),
            ConmanError::Unauthorized { .. }      => (StatusCode::UNAUTHORIZED, "unauthorized"),
            ConmanError::Validation { .. }        => (StatusCode::BAD_REQUEST, "validation_error"),
            ConmanError::InvalidTransition { .. } => (StatusCode::CONFLICT, "invalid_transition"),
            ConmanError::Git { .. }               => (StatusCode::BAD_GATEWAY, "git_error"),
            ConmanError::Internal { .. }          => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };

        // Extract request_id from task-local or default to "unknown".
        let request_id = RequestContext::current_request_id();

        let body = ApiError {
            error: ApiErrorBody {
                code,
                message: self.to_string(),
                request_id,
            },
        };

        (status, Json(body)).into_response()
    }
}
```

### 3.5 Pagination Extractor (`conman-api/src/extractors/pagination.rs`)

Axum `Query` extractor with validation. Clamps limits to the allowed range.

```rust
use serde::Deserialize;
use conman_core::ConmanError;

/// Maximum items per page. Requests above this are clamped.
const MAX_LIMIT: u64 = 100;

/// Default items per page when `limit` is omitted.
const DEFAULT_LIMIT: u64 = 20;

/// Default page number when `page` is omitted.
const DEFAULT_PAGE: u64 = 1;

/// Pagination query parameters extracted from `?page=&limit=`.
///
/// Deserialized via `axum::extract::Query<Pagination>`. Page is 1-based.
/// Limit is clamped to 1..=100.
#[derive(Debug, Clone, Deserialize)]
pub struct Pagination {
    #[serde(default = "default_page")]
    pub page: u64,

    #[serde(default = "default_limit")]
    pub limit: u64,
}

fn default_page() -> u64 {
    DEFAULT_PAGE
}

fn default_limit() -> u64 {
    DEFAULT_LIMIT
}

impl Pagination {
    /// Validate and normalize pagination values.
    ///
    /// Returns `ConmanError::Validation` if `page` is 0.
    /// Clamps `limit` to `1..=MAX_LIMIT`.
    pub fn validate(mut self) -> Result<Self, ConmanError> {
        if self.page == 0 {
            return Err(ConmanError::Validation {
                message: "page must be >= 1".to_string(),
            });
        }

        // Clamp limit to allowed range.
        self.limit = self.limit.clamp(1, MAX_LIMIT);

        Ok(self)
    }

    /// Compute the number of documents to skip for a MongoDB query.
    pub fn skip(&self) -> u64 {
        (self.page - 1) * self.limit
    }
}
```

### 3.6 RequestContext (`conman-api/src/request_context.rs`)

Middleware-populated context carried through request processing.

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Per-request context propagated through middleware, handlers, and into
/// audit events. Stored in Axum extensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestContext {
    /// Unique request identifier. Taken from `X-Request-Id` header if present,
    /// otherwise generated as a UUIDv7.
    pub request_id: String,

    /// Client IP address, extracted from the connection or `X-Forwarded-For`.
    pub client_ip: Option<String>,

    /// User-Agent header value.
    pub user_agent: Option<String>,
}

impl RequestContext {
    /// Create a new context with a generated request ID.
    pub fn new() -> Self {
        Self {
            request_id: Uuid::now_v7().to_string(),
            client_ip: None,
            user_agent: None,
        }
    }

    /// Create a context with a specific request ID (e.g. from an incoming header).
    pub fn with_request_id(request_id: String) -> Self {
        Self {
            request_id,
            client_ip: None,
            user_agent: None,
        }
    }

    /// Retrieve the current request ID from the task-local context.
    ///
    /// Falls back to `"unknown"` if no context is available (e.g. in tests
    /// or outside a request lifecycle).
    pub fn current_request_id() -> String {
        // Implementation will use tokio task-local or Axum extensions.
        // Placeholder for now; wired up in the request_id middleware.
        "unknown".to_string()
    }
}

impl Default for RequestContext {
    fn default() -> Self {
        Self::new()
    }
}
```

### 3.7 AppState (`conman-api/src/state.rs`)

Shared Axum state passed to every handler via `State<AppState>`.

```rust
use std::sync::Arc;
use mongodb::Database;
use conman_core::config::Config;

/// Shared application state injected into every Axum handler.
///
/// Wrapped in `Arc` and passed as `axum::extract::State<AppState>`.
/// Each field is cheaply cloneable (handles, not data).
#[derive(Debug, Clone)]
pub struct AppState {
    /// Application configuration (immutable after startup).
    pub config: Arc<Config>,

    /// MongoDB database handle. The driver manages its own connection pool.
    pub db: Database,

    /// Placeholder for the Tonic gRPC channel to gitaly-rs.
    /// Will be `tonic::transport::Channel` once E01 is implemented.
    /// Using `Option` so the server can boot without a gitaly connection
    /// during early development.
    pub gitaly_channel: Option<tonic::transport::Channel>,
}
```

## 4. Database

### 4.1 MongoDB Connection Setup

Connection is established once at startup. The `mongodb` driver manages an
internal connection pool, so a single `Client` is shared across the application.

```rust
use mongodb::{Client, Database, options::ClientOptions};
use conman_core::config::Config;
use conman_core::ConmanError;

/// Connect to MongoDB and return a handle to the configured database.
///
/// Performs a startup ping to fail fast if the server is unreachable.
pub async fn connect_mongo(config: &Config) -> Result<Database, ConmanError> {
    let opts = ClientOptions::parse(&config.mongo_uri)
        .await
        .map_err(|e| ConmanError::Internal {
            message: format!("failed to parse CONMAN_MONGO_URI: {e}"),
        })?;

    let client = Client::with_options(opts).map_err(|e| ConmanError::Internal {
        message: format!("failed to create MongoDB client: {e}"),
    })?;

    let db = client.database(&config.mongo_db);

    // Startup health check: ping the database to fail fast.
    db.run_command(bson::doc! { "ping": 1 })
        .await
        .map_err(|e| ConmanError::Internal {
            message: format!("MongoDB startup ping failed: {e}"),
        })?;

    tracing::info!(
        db = %config.mongo_db,
        uri = %config.mongo_uri,
        "MongoDB connected"
    );

    Ok(db)
}
```

### 4.2 Health Check Ping

The health endpoint uses the same `ping` command to verify MongoDB is reachable
at request time.

```rust
/// Check MongoDB connectivity by running a `ping` command.
///
/// Returns `Ok(())` if the server responds, or `Err` with the failure reason.
pub async fn check_mongo_health(db: &Database) -> Result<(), ConmanError> {
    db.run_command(bson::doc! { "ping": 1 })
        .await
        .map_err(|e| ConmanError::Internal {
            message: format!("MongoDB health check failed: {e}"),
        })?;
    Ok(())
}
```

### 4.3 Index Bootstrap Pattern

Each repository struct (introduced in later epics) implements an
`ensure_indexes` method called at startup. E00 defines the pattern; no
collections are created yet.

```rust
/// Trait for repository types that require MongoDB indexes.
///
/// Called once at application startup. Implementations should use
/// `create_index` with `IndexModel` definitions. Indexes are idempotent
/// (MongoDB skips creation if the index already exists).
#[async_trait::async_trait]
pub trait EnsureIndexes {
    async fn ensure_indexes(&self) -> Result<(), ConmanError>;
}

/// Bootstrap all indexes by calling `ensure_indexes` on every repository.
///
/// Called during server startup after MongoDB connection is established.
/// Fails fast if any index creation fails.
pub async fn bootstrap_indexes(
    // repos will be added here as epics introduce them:
    // app_repo: &AppRepo,
    // workspace_repo: &WorkspaceRepo,
    // ...
) -> Result<(), ConmanError> {
    // app_repo.ensure_indexes().await?;
    // workspace_repo.ensure_indexes().await?;
    tracing::info!("All MongoDB indexes ensured");
    Ok(())
}
```

## 5. API Endpoints

### 5.1 Health Endpoint

```
GET /api/health
```

**Response 200:**
```json
{
  "status": "ok",
  "mongo": "connected"
}
```

**Response 503 (MongoDB unreachable):**
```json
{
  "status": "degraded",
  "mongo": "disconnected"
}
```

Handler implementation:

```rust
use axum::http::StatusCode;
use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub mongo: &'static str,
}

/// GET /api/health
///
/// Returns server and dependency health. Returns 200 when all dependencies
/// are reachable, 503 otherwise. Does not require authentication.
pub async fn health_check(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    match conman_db::check_mongo_health(&state.db).await {
        Ok(()) => (
            StatusCode::OK,
            Json(HealthResponse {
                status: "ok",
                mongo: "connected",
            }),
        ),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "degraded",
                mongo: "disconnected",
            }),
        ),
    }
}
```

### 5.2 Route Stubs

All resource routes return 501 Not Implemented until their respective epics are
completed. This ensures the router is wired and discoverable from day one.

```rust
use axum::{Router, routing::get};
use crate::state::AppState;

/// Build the complete Axum router with all route groups.
///
/// Health is live immediately. Resource routes return 501 until implemented.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        // Health (live)
        .route("/api/health", get(health_check))

        // Auth stubs (E02)
        .route("/api/auth/login", axum::routing::post(not_implemented))
        .route("/api/auth/logout", axum::routing::post(not_implemented))
        .route("/api/auth/forgot-password", axum::routing::post(not_implemented))
        .route("/api/auth/reset-password", axum::routing::post(not_implemented))
        .route("/api/auth/accept-invite", axum::routing::post(not_implemented))

        // Apps stubs (E03)
        .route("/api/repos", get(not_implemented).post(not_implemented))
        .route("/api/repos/:appId", get(not_implemented))
        .route("/api/repos/:appId/settings", axum::routing::patch(not_implemented))
        .route("/api/repos/:appId/members", get(not_implemented))
        .route("/api/teams/:teamId/invites", axum::routing::post(not_implemented))

        // Workspaces stubs (E04)
        .route("/api/repos/:appId/workspaces", get(not_implemented).post(not_implemented))
        .route("/api/repos/:appId/workspaces/:workspaceId", get(not_implemented).patch(not_implemented))
        .route("/api/repos/:appId/workspaces/:workspaceId/reset", axum::routing::post(not_implemented))
        .route("/api/repos/:appId/workspaces/:workspaceId/sync-integration", axum::routing::post(not_implemented))
        .route("/api/repos/:appId/workspaces/:workspaceId/files", get(not_implemented).put(not_implemented).delete(not_implemented))
        .route("/api/repos/:appId/workspaces/:workspaceId/checkpoints", axum::routing::post(not_implemented))

        // Changesets stubs (E05)
        .route("/api/repos/:appId/changesets", get(not_implemented).post(not_implemented))
        .route("/api/repos/:appId/changesets/:changesetId", get(not_implemented).patch(not_implemented))
        .route("/api/repos/:appId/changesets/:changesetId/submit", axum::routing::post(not_implemented))
        .route("/api/repos/:appId/changesets/:changesetId/resubmit", axum::routing::post(not_implemented))
        .route("/api/repos/:appId/changesets/:changesetId/review", axum::routing::post(not_implemented))
        .route("/api/repos/:appId/changesets/:changesetId/queue", axum::routing::post(not_implemented))
        .route("/api/repos/:appId/changesets/:changesetId/move-to-draft", axum::routing::post(not_implemented))
        .route("/api/repos/:appId/changesets/:changesetId/diff", get(not_implemented))
        .route("/api/repos/:appId/changesets/:changesetId/comments", get(not_implemented).post(not_implemented))

        // Releases stubs (E08)
        .route("/api/repos/:appId/releases", get(not_implemented).post(not_implemented))
        .route("/api/repos/:appId/releases/:releaseId", get(not_implemented))
        .route("/api/repos/:appId/releases/:releaseId/changesets", axum::routing::post(not_implemented))
        .route("/api/repos/:appId/releases/:releaseId/reorder", axum::routing::post(not_implemented))
        .route("/api/repos/:appId/releases/:releaseId/assemble", axum::routing::post(not_implemented))
        .route("/api/repos/:appId/releases/:releaseId/publish", axum::routing::post(not_implemented))

        // Environments + deployments stubs (E09)
        .route("/api/repos/:appId/environments", get(not_implemented).patch(not_implemented))
        .route("/api/repos/:appId/environments/:envId/deploy", axum::routing::post(not_implemented))
        .route("/api/repos/:appId/environments/:envId/promote", axum::routing::post(not_implemented))
        .route("/api/repos/:appId/environments/:envId/rollback", axum::routing::post(not_implemented))
        .route("/api/repos/:appId/deployments", get(not_implemented))

        // Temp environments stubs (E10)
        .route("/api/repos/:appId/temp-envs", get(not_implemented).post(not_implemented))
        .route("/api/repos/:appId/temp-envs/:tempEnvId/extend", axum::routing::post(not_implemented))
        .route("/api/repos/:appId/temp-envs/:tempEnvId/undo-expire", axum::routing::post(not_implemented))
        .route("/api/repos/:appId/temp-envs/:tempEnvId", axum::routing::delete(not_implemented))

        // Jobs stubs (E06)
        .route("/api/repos/:appId/jobs", get(not_implemented))
        .route("/api/repos/:appId/jobs/:jobId", get(not_implemented))

        // Notification preferences stubs (E11)
        .route("/api/me/notification-preferences", get(not_implemented).patch(not_implemented))

        // 404 fallback for unknown routes.
        .fallback(fallback_404)

        // Middleware layers (outermost = first to run).
        .layer(axum::middleware::from_fn(request_id_middleware))

        .with_state(state)
}

/// Handler for routes that exist but are not yet implemented.
/// Returns 501 Not Implemented with the standard error envelope.
async fn not_implemented() -> impl IntoResponse {
    let body = ApiError {
        error: ApiErrorBody {
            code: "not_implemented",
            message: "This endpoint is not yet implemented.".to_string(),
            request_id: RequestContext::current_request_id(),
        },
    };
    (StatusCode::NOT_IMPLEMENTED, Json(body))
}

/// Fallback handler for routes that do not match any defined path.
/// Returns 404 with the standard error envelope.
async fn fallback_404() -> impl IntoResponse {
    let body = ApiError {
        error: ApiErrorBody {
            code: "not_found",
            message: "The requested route does not exist.".to_string(),
            request_id: RequestContext::current_request_id(),
        },
    };
    (StatusCode::NOT_FOUND, Json(body))
}
```

### 5.3 Request ID Middleware

```rust
use axum::{
    extract::Request,
    http::HeaderValue,
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

/// Middleware that extracts or generates a request ID for every request.
///
/// Reads `X-Request-Id` from the incoming request. If absent, generates a
/// UUIDv7. Inserts a `RequestContext` into Axum extensions and echoes the
/// request ID back in the response `X-Request-Id` header.
pub async fn request_id_middleware(mut req: Request, next: Next) -> Response {
    // Read or generate request ID.
    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::now_v7().to_string());

    // Build context from request metadata.
    let client_ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let user_agent = req
        .headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let ctx = RequestContext {
        request_id: request_id.clone(),
        client_ip,
        user_agent,
    };

    // Store in extensions so handlers can access it.
    req.extensions_mut().insert(ctx);

    // Continue processing the request.
    let mut response = next.run(req).await;

    // Echo request ID in the response.
    if let Ok(val) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", val);
    }

    response
}
```

## 6. Business Logic

### 6.1 Config Loading

Config is loaded once in `main.rs` via `Config::from_env()`. Validation errors
cause an immediate panic with a clear message -- there is no point continuing
if required configuration is missing.

### 6.2 Startup Sequence

`main.rs` orchestrates startup in this order:

```rust
#[tokio::main]
async fn main() {
    // 1. Initialize tracing subscriber for structured JSON logging.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "conman=debug,tower_http=debug".into()),
        )
        .json()
        .init();

    // 2. Load configuration from environment.
    let config = Config::from_env().expect("failed to load configuration");
    tracing::info!(listen = %config.listen_addr, "configuration loaded");

    // 3. Connect to MongoDB (fails fast on unreachable server).
    let db = conman_db::connect_mongo(&config)
        .await
        .expect("failed to connect to MongoDB");

    // 4. Bootstrap indexes (idempotent).
    conman_db::bootstrap_indexes()
        .await
        .expect("failed to bootstrap MongoDB indexes");

    // 5. Optionally connect to gitaly-rs (placeholder for E01).
    let gitaly_channel = match tonic::transport::Channel::from_shared(
        config.gitaly_address.clone(),
    ) {
        Ok(endpoint) => endpoint.connect().await.ok(),
        Err(_) => None,
    };

    if gitaly_channel.is_some() {
        tracing::info!(addr = %config.gitaly_address, "gitaly-rs channel connected");
    } else {
        tracing::warn!(addr = %config.gitaly_address, "gitaly-rs channel not available (will retry on use)");
    }

    // 6. Build shared application state.
    let state = AppState {
        config: Arc::new(config.clone()),
        db,
        gitaly_channel,
    };

    // 7. Build router with all routes and middleware.
    let app = build_router(state);

    // 8. Bind and serve with graceful shutdown.
    let listener = tokio::net::TcpListener::bind(&config.listen_addr)
        .await
        .expect("failed to bind TCP listener");

    tracing::info!(addr = %config.listen_addr, "server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

/// Wait for a SIGINT or SIGTERM signal for graceful shutdown.
async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to listen for ctrl+c");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to listen for SIGTERM")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("received SIGINT, shutting down"),
        _ = terminate => tracing::info!("received SIGTERM, shutting down"),
    }
}
```

## 7. Gitaly-rs Integration

This epic creates only the **channel placeholder**. No gRPC calls are made.

- `AppState.gitaly_channel` is `Option<tonic::transport::Channel>`.
- Startup attempts to connect but does **not** fail if gitaly is unreachable.
  A warning is logged and the field is set to `None`.
- Handlers that require Git operations (introduced in E01+) check for `Some`
  and return `ConmanError::Internal` with a descriptive message if the channel
  is unavailable.
- The channel uses HTTP/2 multiplexing (Tonic default). No manual pool is
  needed.

E01 will introduce `GitalyClient` which wraps this channel with typed service
stubs and retry logic.

## 8. Implementation Checklist

Each step is one commit. Follow TDD: write test, run test (fails), implement,
run test (passes), commit.

- [ ] **E00-S01** — Scaffold workspace crates.
  Create `Cargo.toml` workspace with members: `conman`, `conman-core`,
  `conman-api`, `conman-db`, `conman-git`, `conman-auth`, `conman-jobs`.
  Each crate has `src/lib.rs` (or `src/main.rs` for `conman`). Add shared
  dependencies to workspace `[dependencies]`.

- [ ] **E00-S02** — Implement `Config` in `conman-core`.
  Add `config.rs` with `Config::from_env()`. Write unit tests for default
  values, required `jwt_secret`, and invalid port parsing.

- [ ] **E00-S03** — Implement `ConmanError` in `conman-core`.
  Add `error.rs` with the error enum and `thiserror` derives. Write unit
  tests verifying `Display` output for each variant.

- [ ] **E00-S04** — Implement MongoDB connection in `conman-db`.
  Add `connect_mongo()` and `check_mongo_health()`. Write integration test
  that connects to a local MongoDB and pings.

- [ ] **E00-S05** — Implement `EnsureIndexes` trait and `bootstrap_indexes` in `conman-db`.
  Define the trait. Write a no-op bootstrap function. Unit test that it
  returns `Ok(())`.

- [ ] **E00-S06** — Implement response envelopes in `conman-api`.
  Add `ApiResponse`, `PaginationMeta`, `ApiError`, `ApiErrorBody`. Write unit
  tests verifying JSON serialization output for both success and error shapes.

- [ ] **E00-S07** — Implement `ConmanError` -> `IntoResponse` in `conman-api`.
  Map each error variant to its HTTP status code and error code string. Write
  unit tests for each variant.

- [ ] **E00-S08** — Implement `Pagination` extractor in `conman-api`.
  Add deserialization with defaults and `validate()`. Write unit tests for
  defaults, zero page rejection, limit clamping, and skip calculation.

- [ ] **E00-S09** — Implement `RequestContext` and request ID middleware in `conman-api`.
  Add middleware that reads/generates `X-Request-Id`. Write integration test
  verifying the header is echoed and a UUID is generated when absent.

- [ ] **E00-S10** — Implement `AppState` and health endpoint in `conman-api`.
  Wire up `AppState`, add `GET /api/health`. Write integration test using
  `axum::test` (or `tower::ServiceExt`) to verify 200 with connected DB
  and 503 with mock failure.

- [ ] **E00-S11** — Wire route stubs and 404 fallback.
  Register all resource route stubs returning 501. Add fallback returning 404.
  Write integration test hitting a stub (expect 501) and an unknown path
  (expect 404). Verify both return the error envelope format.

- [ ] **E00-S12** — Implement `main.rs` startup and graceful shutdown.
  Wire the full startup sequence in the `conman` binary crate. Manual smoke
  test: `cargo run` boots, `curl /api/health` returns 200, `ctrl-c` shuts
  down cleanly.

## 9. Test Cases

### 9.1 Config loads from env vars with defaults

```rust
#[test]
fn config_loads_defaults() {
    // Set only the required var.
    std::env::set_var("CONMAN_JWT_SECRET", "test-secret");

    let config = Config::from_env().unwrap();

    assert_eq!(config.listen_addr.port(), 3000);
    assert_eq!(config.mongo_uri, "mongodb://localhost:27017");
    assert_eq!(config.mongo_db, "conman");
    assert_eq!(config.gitaly_address, "http://localhost:8075");
    assert_eq!(config.jwt_expiry_hours, 24);
    assert_eq!(config.invite_expiry_days, 7);
}

#[test]
fn config_overrides_from_env() {
    std::env::set_var("CONMAN_JWT_SECRET", "override-secret");
    std::env::set_var("CONMAN_PORT", "8080");
    std::env::set_var("CONMAN_MONGO_URI", "mongodb://custom:27017");
    std::env::set_var("CONMAN_MONGO_DB", "mydb");

    let config = Config::from_env().unwrap();

    assert_eq!(config.listen_addr.port(), 8080);
    assert_eq!(config.mongo_uri, "mongodb://custom:27017");
    assert_eq!(config.mongo_db, "mydb");
}

#[test]
fn config_fails_without_jwt_secret() {
    std::env::remove_var("CONMAN_JWT_SECRET");

    let result = Config::from_env();

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("CONMAN_JWT_SECRET"));
}

#[test]
fn config_fails_on_invalid_port() {
    std::env::set_var("CONMAN_JWT_SECRET", "s");
    std::env::set_var("CONMAN_PORT", "not-a-number");

    let result = Config::from_env();

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("CONMAN_PORT"));
}
```

### 9.2 Health endpoint returns 200 when MongoDB connected

```rust
#[tokio::test]
async fn health_returns_200_when_mongo_connected() {
    let app = test_app_with_real_mongo().await;

    let response = app
        .oneshot(Request::builder().uri("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = parse_body(response).await;
    assert_eq!(body["status"], "ok");
    assert_eq!(body["mongo"], "connected");
}
```

### 9.3 Health endpoint returns 503 when MongoDB disconnected

```rust
#[tokio::test]
async fn health_returns_503_when_mongo_disconnected() {
    // Use a bogus MongoDB URI so the ping fails.
    let app = test_app_with_broken_mongo().await;

    let response = app
        .oneshot(Request::builder().uri("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let body: serde_json::Value = parse_body(response).await;
    assert_eq!(body["status"], "degraded");
    assert_eq!(body["mongo"], "disconnected");
}
```

### 9.4 Error envelope format is correct for each error variant

```rust
#[test]
fn not_found_error_serializes_correctly() {
    let err = ConmanError::NotFound {
        entity: "changeset",
        id: "abc123".to_string(),
    };

    let response = err.into_response();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body: serde_json::Value = parse_response_body(response);
    assert_eq!(body["error"]["code"], "not_found");
    assert_eq!(body["error"]["message"], "not found: changeset abc123");
    assert!(body["error"]["request_id"].is_string());
}

#[test]
fn validation_error_serializes_correctly() {
    let err = ConmanError::Validation {
        message: "name is required".to_string(),
    };

    let response = err.into_response();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = parse_response_body(response);
    assert_eq!(body["error"]["code"], "validation_error");
}

#[test]
fn unauthorized_error_returns_401() {
    let err = ConmanError::Unauthorized {
        message: "token expired".to_string(),
    };

    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn forbidden_error_returns_403() {
    let err = ConmanError::Forbidden {
        message: "insufficient role".to_string(),
    };

    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[test]
fn conflict_error_returns_409() {
    let err = ConmanError::Conflict {
        message: "already exists".to_string(),
    };

    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[test]
fn invalid_transition_returns_409() {
    let err = ConmanError::InvalidTransition {
        from: "draft".to_string(),
        to: "released".to_string(),
    };

    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let body: serde_json::Value = parse_response_body(response);
    assert_eq!(body["error"]["code"], "invalid_transition");
}

#[test]
fn git_error_returns_502() {
    let err = ConmanError::Git {
        message: "ref not found".to_string(),
    };

    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}

#[test]
fn internal_error_returns_500() {
    let err = ConmanError::Internal {
        message: "unexpected".to_string(),
    };

    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
```

### 9.5 Pagination extractor validates limits

```rust
#[test]
fn pagination_defaults() {
    let p = Pagination { page: 1, limit: 20 };
    let p = p.validate().unwrap();
    assert_eq!(p.page, 1);
    assert_eq!(p.limit, 20);
    assert_eq!(p.skip(), 0);
}

#[test]
fn pagination_rejects_zero_page() {
    let p = Pagination { page: 0, limit: 20 };
    assert!(p.validate().is_err());
}

#[test]
fn pagination_clamps_excessive_limit() {
    let p = Pagination { page: 1, limit: 500 };
    let p = p.validate().unwrap();
    assert_eq!(p.limit, 100);
}

#[test]
fn pagination_clamps_zero_limit() {
    let p = Pagination { page: 1, limit: 0 };
    let p = p.validate().unwrap();
    assert_eq!(p.limit, 1);
}

#[test]
fn pagination_skip_calculation() {
    let p = Pagination { page: 3, limit: 25 };
    let p = p.validate().unwrap();
    assert_eq!(p.skip(), 50);
}
```

### 9.6 Request ID is generated and propagated

```rust
#[tokio::test]
async fn request_id_generated_when_absent() {
    let app = test_app().await;

    let response = app
        .oneshot(Request::builder().uri("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let request_id = response.headers().get("x-request-id").unwrap().to_str().unwrap();

    // Verify it is a UUIDv7 generated by middleware.
    let parsed = uuid::Uuid::parse_str(request_id).unwrap();
    assert_eq!(parsed.get_version(), Some(uuid::Version::SortRand));
}

#[tokio::test]
async fn request_id_echoed_when_provided() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .header("x-request-id", "my-custom-id-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let echoed = response.headers().get("x-request-id").unwrap().to_str().unwrap();
    assert_eq!(echoed, "my-custom-id-123");
}
```

### 9.7 Unknown routes return 404 with error envelope

```rust
#[tokio::test]
async fn unknown_route_returns_404_envelope() {
    let app = test_app().await;

    let response = app
        .oneshot(Request::builder().uri("/api/nonexistent").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body: serde_json::Value = parse_body(response).await;
    assert_eq!(body["error"]["code"], "not_found");
    assert!(body["error"]["message"].as_str().unwrap().contains("does not exist"));
    assert!(body["error"]["request_id"].is_string());
}
```

### 9.8 Stub routes return 501 with error envelope

```rust
#[tokio::test]
async fn stub_route_returns_501_envelope() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/repos")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);

    let body: serde_json::Value = parse_body(response).await;
    assert_eq!(body["error"]["code"], "not_implemented");
    assert!(body["error"]["request_id"].is_string());
}
```

## 10. Acceptance Criteria

1. **Server boots with health endpoint and typed route stubs.**
   - `cargo run` starts the server on the configured port without errors.
   - `GET /api/health` returns `200 {"status":"ok","mongo":"connected"}` when
     MongoDB is running.
   - `GET /api/health` returns `503 {"status":"degraded","mongo":"disconnected"}`
     when MongoDB is unreachable.
   - `GET /api/repos` returns `501 {"error":{"code":"not_implemented",...}}`.
   - `POST /api/teams/:teamId/repos` returns `501 {"error":{"code":"not_implemented",...}}`.
   - `GET /api/nonexistent` returns `404 {"error":{"code":"not_found",...}}`.
   - All responses include `X-Request-Id` header.

2. **MongoDB connection resilient with startup validation.**
   - Server fails to start (panics with clear message) if MongoDB is unreachable
     at boot time.
   - After startup, transient MongoDB failures are surfaced through the health
     endpoint (503) and error responses (500), but do not crash the server.

3. **Shared request/response and validation utilities are used by all new routes.**
   - `ApiResponse<T>` wraps all success payloads with optional pagination.
   - `ApiError` wraps all error payloads with `code`, `message`, `request_id`.
   - `Pagination` extractor validates `page >= 1`, clamps `limit` to `1..=100`,
     computes `skip` correctly.
   - `ConmanError` variants map to correct HTTP status codes:
     - `NotFound` -> 404
     - `Conflict` -> 409
     - `Forbidden` -> 403
     - `Unauthorized` -> 401
     - `Validation` -> 400
     - `InvalidTransition` -> 409
     - `Git` -> 502
     - `Internal` -> 500
   - `RequestContext` with `request_id` is available in every handler and
     included in every error response.
