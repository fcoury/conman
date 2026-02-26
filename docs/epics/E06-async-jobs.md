# E06 Async Jobs + msuite Pipeline

## 1. Goal

Provide a generic asynchronous job framework that runs mandatory checks and
long-running operations (msuite validation, release assembly, deployment,
revalidation, runtime-profile drift checks, temp-env provisioning) with state
tracking, structured logs,
retry/timeout policies, and gate hooks that block workflow transitions on job
outcomes. After this epic, every flow that requires an external or long-running
operation uses the jobs subsystem rather than inlining work in the HTTP request
cycle.

---

## 2. Dependencies

| Dependency | Reason |
|------------|--------|
| **E00 Platform Foundation** | MongoDB connection, error types, config, pagination |
| **E05 Changesets** | Submit and queue flows trigger msuite jobs; gate hooks read job results |
| **E03 App Setup** | Runtime profile configuration and validation gate defaults |

Soft consumers (not blocked, but will call into the framework):

- E07 Queue Orchestration (revalidation jobs)
- E08 Releases (release assembly job)
- E09 Deployments (deploy release job)
- E10 Temp Environments (provision/expire jobs)

---

## 3. Rust Types

### conman-core types

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Job Type ────────────────────────────────────────────────────────────

/// Discriminator for the kind of work a job performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    MsuiteSubmit,
    MsuiteMerge,
    MsuiteDeploy,
    RevalidateQueuedChangeset,
    ReleaseAssemble,
    DeployRelease,
    RuntimeProfileDriftCheck,
    TempEnvProvision,
    TempEnvExpire,
}

// ── Job State ───────────────────────────────────────────────────────────

/// Lifecycle states for an async job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Canceled,
}

impl JobState {
    /// Returns `true` for terminal states that will never transition again.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Canceled)
    }
}

// ── Job ─────────────────────────────────────────────────────────────────

/// A unit of asynchronous work tracked in MongoDB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// Unique identifier (MongoDB ObjectId hex).
    pub id: String,

    /// App this job belongs to.
    pub app_id: String,

    /// What kind of work this job performs.
    pub job_type: JobType,

    /// Current lifecycle state.
    pub state: JobState,

    /// The domain entity type this job acts on (e.g. "changeset", "release",
    /// "deployment", "temp_environment").
    pub entity_type: String,

    /// The domain entity id this job acts on.
    pub entity_id: String,

    /// Opaque input payload for the worker (contents vary by job_type).
    pub payload: serde_json::Value,

    /// Terminal result payload set by the worker on success.
    pub result: Option<serde_json::Value>,

    /// Human-readable error message set on failure.
    pub error_message: Option<String>,

    /// How many times this job has been attempted so far.
    pub retry_count: u32,

    /// Maximum retries before the job stays Failed permanently.
    pub max_retries: u32,

    /// Maximum wall-clock time (ms) before the runner cancels the job.
    pub timeout_ms: u64,

    /// When the job was created (enqueued).
    pub created_at: DateTime<Utc>,

    /// When the runner picked up the job (set to Running).
    pub started_at: Option<DateTime<Utc>>,

    /// When the job reached a terminal state.
    pub completed_at: Option<DateTime<Utc>>,
}

// ── Log Level ───────────────────────────────────────────────────────────

/// Severity level for structured job log entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

// ── Job Log ─────────────────────────────────────────────────────────────

/// A single structured log line emitted by a worker during execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobLog {
    /// Unique identifier (MongoDB ObjectId hex).
    pub id: String,

    /// The job this log belongs to.
    pub job_id: String,

    /// Severity level.
    pub level: LogLevel,

    /// Human-readable log message.
    pub message: String,

    /// Optional structured data attached to the log entry.
    pub data: Option<serde_json::Value>,

    /// When the log was written.
    pub timestamp: DateTime<Utc>,
}
```

### conman-jobs types

```rust
use async_trait::async_trait;

use conman_core::{Job, ConmanError};

// ── Job Worker Trait ────────────────────────────────────────────────────

/// Implemented by each job type to perform the actual work.
///
/// Workers receive an immutable reference to the Job (including payload) and
/// return a result payload on success. Workers write structured logs during
/// execution via the `JobLogger` passed at construction time.
#[async_trait]
pub trait JobWorker: Send + Sync {
    /// Execute the job's work. On success, return a JSON result payload that
    /// will be persisted on the Job document. On failure, return a
    /// `ConmanError` whose message becomes the job's `error_message`.
    async fn execute(&self, job: &Job) -> Result<serde_json::Value, ConmanError>;
}

// ── Job Logger ──────────────────────────────────────────────────────────

/// Handle passed to workers so they can emit structured log entries that are
/// persisted to the `job_logs` collection in real time.
#[derive(Clone)]
pub struct JobLogger {
    job_id: String,
    log_repo: JobLogRepo,
}

impl JobLogger {
    pub fn new(job_id: String, log_repo: JobLogRepo) -> Self {
        Self { job_id, log_repo }
    }

    pub async fn info(&self, message: &str, data: Option<serde_json::Value>) {
        self.write(LogLevel::Info, message, data).await;
    }

    pub async fn warn(&self, message: &str, data: Option<serde_json::Value>) {
        self.write(LogLevel::Warn, message, data).await;
    }

    pub async fn error(&self, message: &str, data: Option<serde_json::Value>) {
        self.write(LogLevel::Error, message, data).await;
    }

    /// Fire-and-forget write. Errors are traced but never propagated.
    async fn write(&self, level: LogLevel, message: &str, data: Option<serde_json::Value>) {
        let log = JobLog {
            id: ObjectId::new().to_hex(),
            job_id: self.job_id.clone(),
            level,
            message: message.to_string(),
            data,
            timestamp: Utc::now(),
        };
        if let Err(e) = self.log_repo.insert(&log).await {
            tracing::warn!(job_id = %self.job_id, error = %e, "failed to write job log");
        }
    }
}

// ── Job Runner ──────────────────────────────────────────────────────────

/// The background polling loop that claims Queued jobs and dispatches them to
/// the appropriate `JobWorker` implementation.
pub struct JobRunner {
    /// MongoDB repository for the `jobs` collection.
    job_repo: JobRepo,

    /// MongoDB repository for the `job_logs` collection.
    log_repo: JobLogRepo,

    /// Registry mapping JobType -> Box<dyn JobWorker>.
    workers: HashMap<JobType, Box<dyn JobWorker>>,

    /// How often (ms) the runner polls for new work when idle.
    poll_interval_ms: u64,

    /// Shutdown signal receiver.
    shutdown: tokio::sync::watch::Receiver<bool>,
}

impl JobRunner {
    pub fn new(
        job_repo: JobRepo,
        log_repo: JobLogRepo,
        workers: HashMap<JobType, Box<dyn JobWorker>>,
        poll_interval_ms: u64,
        shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> Self {
        Self { job_repo, log_repo, workers, poll_interval_ms, shutdown }
    }

    /// Main loop: poll for the oldest Queued job, claim it atomically, execute
    /// the matching worker, and transition to Succeeded or Failed.
    pub async fn run(&self) {
        loop {
            // Check for graceful shutdown signal
            if *self.shutdown.borrow() {
                tracing::info!("job runner shutting down");
                break;
            }

            // Attempt to claim and execute one job
            match self.poll_and_execute().await {
                Ok(true) => {
                    // Executed a job — immediately poll for the next one
                    continue;
                }
                Ok(false) => {
                    // No jobs available — sleep before next poll
                }
                Err(e) => {
                    tracing::error!(error = %e, "job runner poll error");
                }
            }

            tokio::time::sleep(
                std::time::Duration::from_millis(self.poll_interval_ms)
            ).await;
        }
    }

    /// Claim the oldest Queued job via atomic findOneAndUpdate, execute the
    /// worker, and transition the job to its terminal state.
    /// Returns `Ok(true)` if a job was processed, `Ok(false)` if none available.
    async fn poll_and_execute(&self) -> Result<bool, ConmanError> {
        // Atomic claim: state=Queued -> Running, set started_at
        let Some(mut job) = self.job_repo.claim_next().await? else {
            return Ok(false);
        };

        let logger = JobLogger::new(job.id.clone(), self.log_repo.clone());

        let Some(worker) = self.workers.get(&job.job_type) else {
            // No registered worker — mark failed
            self.job_repo.mark_failed(
                &job.id,
                &format!("no worker registered for job type {:?}", job.job_type),
            ).await?;
            return Ok(true);
        };

        // Execute with timeout
        let timeout = std::time::Duration::from_millis(job.timeout_ms);
        let result = tokio::time::timeout(timeout, worker.execute(&job)).await;

        match result {
            // Worker completed within timeout
            Ok(Ok(result_payload)) => {
                self.job_repo.mark_succeeded(&job.id, result_payload).await?;
            }
            // Worker returned an error
            Ok(Err(e)) => {
                self.handle_failure(&mut job, &e.to_string()).await?;
            }
            // Timeout elapsed
            Err(_) => {
                self.job_repo.mark_canceled(&job.id, "timeout exceeded").await?;
                logger.error(
                    &format!("job timed out after {}ms", job.timeout_ms),
                    None,
                ).await;
            }
        }

        Ok(true)
    }

    /// On failure: if retries remain, re-queue with incremented count.
    /// Otherwise stay Failed.
    async fn handle_failure(&self, job: &mut Job, error_msg: &str) -> Result<(), ConmanError> {
        if job.retry_count < job.max_retries {
            self.job_repo.requeue_for_retry(&job.id, job.retry_count + 1).await?;
        } else {
            self.job_repo.mark_failed(&job.id, error_msg).await?;
        }
        Ok(())
    }
}
```

### conman-api types

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use conman_core::{JobType, JobState, LogLevel};

// ── API Response DTOs ───────────────────────────────────────────────────

/// GET /api/repos/:appId/jobs/:jobId response body.
#[derive(Debug, Serialize)]
pub struct JobResponse {
    pub id: String,
    pub app_id: String,
    pub job_type: JobType,
    pub state: JobState,
    pub entity_type: String,
    pub entity_id: String,
    pub payload: serde_json::Value,
    pub result: Option<serde_json::Value>,
    pub error_message: Option<String>,
    pub retry_count: u32,
    pub max_retries: u32,
    pub timeout_ms: u64,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Individual log entry within a job response or log listing.
#[derive(Debug, Serialize)]
pub struct JobLogResponse {
    pub id: String,
    pub job_id: String,
    pub level: LogLevel,
    pub message: String,
    pub data: Option<serde_json::Value>,
    pub timestamp: DateTime<Utc>,
}

/// Query parameters for GET /api/repos/:appId/jobs
#[derive(Debug, Deserialize)]
pub struct ListJobsQuery {
    #[serde(default = "default_page")]
    pub page: u64,

    #[serde(default = "default_limit")]
    pub limit: u64,

    /// Filter by job type (optional).
    #[serde(rename = "type")]
    pub job_type: Option<JobType>,

    /// Filter by job state (optional).
    pub state: Option<JobState>,
}

fn default_page() -> u64 { 1 }
fn default_limit() -> u64 { 20 }
```

---

## 4. Database

### `jobs` collection

**Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `_id` | `ObjectId` | Primary key |
| `app_id` | `ObjectId` | Owning app |
| `job_type` | `String` | Enum discriminator (snake_case) |
| `state` | `String` | Lifecycle state (snake_case) |
| `entity_type` | `String` | Domain entity type acted upon |
| `entity_id` | `ObjectId` | Domain entity id acted upon |
| `payload` | `Document` | Opaque input for the worker |
| `result` | `Document?` | Terminal success payload |
| `error_message` | `String?` | Failure reason |
| `retry_count` | `i32` | Current attempt number |
| `max_retries` | `i32` | Retry ceiling |
| `timeout_ms` | `i64` | Wall-clock timeout |
| `created_at` | `DateTime` | Enqueue time |
| `started_at` | `DateTime?` | Claim time |
| `completed_at` | `DateTime?` | Terminal state time |

**Indexes:**

```javascript
// Polling: claim the oldest Queued job efficiently
{ "state": 1, "created_at": 1 }

// Look up jobs for a specific entity (e.g. all jobs for a changeset)
{ "app_id": 1, "entity_type": 1, "entity_id": 1 }

// Gate hook: find the latest job of a given type+state for an entity
{ "app_id": 1, "job_type": 1, "state": 1 }
```

**Example document:**

```json
{
  "_id": ObjectId("665a1b2c3d4e5f6a7b8c9d0e"),
  "app_id": ObjectId("665a0001aabbccddee000001"),
  "job_type": "msuite_submit",
  "state": "queued",
  "entity_type": "changeset",
  "entity_id": ObjectId("665a0002aabbccddee000002"),
  "payload": {
    "changeset_id": "665a0002aabbccddee000002",
    "head_sha": "abc123def456",
    "base_sha": "789012fed345"
  },
  "result": null,
  "error_message": null,
  "retry_count": 0,
  "max_retries": 3,
  "timeout_ms": 300000,
  "created_at": ISODate("2025-06-01T12:00:00Z"),
  "started_at": null,
  "completed_at": null
}
```

### `job_logs` collection

**Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `_id` | `ObjectId` | Primary key |
| `job_id` | `ObjectId` | Parent job |
| `level` | `String` | `info`, `warn`, or `error` |
| `message` | `String` | Human-readable log line |
| `data` | `Document?` | Optional structured data |
| `timestamp` | `DateTime` | When the log was written |

**Indexes:**

```javascript
// Fetch logs for a job in chronological order
{ "job_id": 1, "timestamp": 1 }
```

**Example document:**

```json
{
  "_id": ObjectId("665a1c001122334455667788"),
  "job_id": ObjectId("665a1b2c3d4e5f6a7b8c9d0e"),
  "level": "info",
  "message": "msuite test suite started",
  "data": {
    "suite": "config_validation",
    "test_count": 42
  },
  "timestamp": ISODate("2025-06-01T12:00:05Z")
}
```

---

## 5. API Endpoints

### `GET /api/repos/:appId/jobs/:jobId`

Retrieve a single job by ID, including its current state, result, and error.

**Auth:** Any app member (user, reviewer, config_manager, app_admin).

**Path params:**

| Param | Type | Description |
|-------|------|-------------|
| `appId` | `ObjectId` hex | App scope |
| `jobId` | `ObjectId` hex | Job to retrieve |

**Response:** `200 OK`

```json
{
  "data": {
    "id": "665a1b2c3d4e5f6a7b8c9d0e",
    "app_id": "665a0001aabbccddee000001",
    "job_type": "msuite_submit",
    "state": "succeeded",
    "entity_type": "changeset",
    "entity_id": "665a0002aabbccddee000002",
    "payload": { "changeset_id": "665a0002aabbccddee000002", "head_sha": "abc123" },
    "result": { "passed": true, "test_count": 42, "failures": [] },
    "error_message": null,
    "retry_count": 0,
    "max_retries": 3,
    "timeout_ms": 300000,
    "created_at": "2025-06-01T12:00:00Z",
    "started_at": "2025-06-01T12:00:02Z",
    "completed_at": "2025-06-01T12:01:30Z"
  }
}
```

**Errors:**

| Status | Code | When |
|--------|------|------|
| 404 | `not_found` | Job does not exist or belongs to a different app |
| 403 | `forbidden` | User is not a member of the app |

### `GET /api/repos/:appId/jobs?page=&limit=&type=&state=`

List jobs for an app with optional filters and pagination.

**Auth:** Any app member.

**Query params:**

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `page` | `u64` | `1` | 1-based page number |
| `limit` | `u64` | `20` | Results per page (max 100) |
| `type` | `JobType?` | none | Filter by job type |
| `state` | `JobState?` | none | Filter by job state |

**Response:** `200 OK`

```json
{
  "data": [ /* array of JobResponse */ ],
  "pagination": { "page": 1, "limit": 20, "total": 5 }
}
```

---

## 6. Business Logic

### Job runner polling loop

The `JobRunner` runs as a `tokio::spawn`ed task started in `main.rs`. It
continuously polls for work:

1. Query `jobs` collection for the oldest document with `state = "queued"`,
   ordered by `created_at` ascending.
2. Atomically transition the document from `Queued` to `Running` via
   `findOneAndUpdate` with a filter that includes `state: "queued"`. This is
   the **claim mechanism** -- if two runners race, only one succeeds; the
   loser's update matches zero documents and retries on the next poll.
3. Set `started_at` to `Utc::now()`.
4. Look up the registered `JobWorker` for the job's `job_type`.
5. Execute the worker inside a `tokio::time::timeout` bounded by `timeout_ms`.
6. On success: set `state = "succeeded"`, `result = worker_output`,
   `completed_at = now`.
7. On worker error: invoke retry logic (see below).
8. On timeout: set `state = "canceled"`, `error_message = "timeout exceeded"`,
   `completed_at = now`.

When no jobs are available, the runner sleeps for `poll_interval_ms`
(configurable, default 1000ms) before polling again. A `tokio::sync::watch`
channel carries the shutdown signal so the runner exits cleanly.

### Claim mechanism

The atomic claim prevents double-processing when multiple runner instances
exist (horizontal scaling). The MongoDB update uses:

```javascript
db.jobs.findOneAndUpdate(
  { state: "queued" },
  { $set: { state: "running", started_at: ISODate() } },
  { sort: { created_at: 1 }, returnDocument: "after" }
)
```

If the result is `null`, no job was available. Two concurrent callers will
never both receive the same document because `findOneAndUpdate` is atomic.

### Timeout

If a worker exceeds its `timeout_ms`, the `tokio::time::timeout` wrapper
resolves to `Err`. The runner then:

1. Sets the job to `Canceled` with `error_message = "timeout exceeded"`.
2. Writes an error-level job log entry.
3. Does **not** retry on timeout -- timeouts indicate a systemic issue, not a
   transient failure.

### Retry

When a worker returns an error:

1. If `retry_count < max_retries`, the runner atomically updates the job:
   - `state` back to `Queued`
   - `retry_count` incremented by 1
   - `started_at` cleared to `None`
   The job re-enters the queue and will be picked up on a subsequent poll.
2. If `retry_count >= max_retries`, the job stays `Failed`:
   - `state = "failed"`
   - `error_message` set to the worker's error string
   - `completed_at = now`

Default retry/timeout values per job type:

| Job Type | `max_retries` | `timeout_ms` |
|----------|---------------|--------------|
| `msuite_submit` | 3 | 300000 (5 min) |
| `msuite_merge` | 3 | 300000 (5 min) |
| `msuite_deploy` | 2 | 600000 (10 min) |
| `revalidate_queued_changeset` | 3 | 300000 (5 min) |
| `release_assemble` | 2 | 600000 (10 min) |
| `deploy_release` | 1 | 900000 (15 min) |
| `temp_env_provision` | 2 | 300000 (5 min) |
| `temp_env_expire` | 3 | 60000 (1 min) |

### Gate hooks

Gate hooks are synchronous checks called from request handlers (submit, queue,
release, deploy) to verify that a required asynchronous job completed
successfully before allowing the workflow transition.

```rust
/// Checks whether a successful job of the given type exists for the entity.
/// Returns Ok(()) if the gate passes, or ConmanError::Conflict if the
/// required job has not succeeded.
pub async fn require_job_success(
    job_repo: &JobRepo,
    app_id: &str,
    entity_type: &str,
    entity_id: &str,
    job_type: JobType,
) -> Result<(), ConmanError> {
    let filter = doc! {
        "app_id": ObjectId::parse_str(app_id)?,
        "entity_type": entity_type,
        "entity_id": ObjectId::parse_str(entity_id)?,
        "job_type": job_type.as_str(),
        "state": "succeeded",
    };

    let exists = job_repo.collection.count_documents(filter).await? > 0;

    if !exists {
        return Err(ConmanError::Conflict {
            message: format!(
                "required {} job has not succeeded for {} {}",
                job_type.as_str(), entity_type, entity_id,
            ),
        });
    }

    Ok(())
}
```

Gate hook integration points:

| Flow | Gate check |
|------|-----------|
| Changeset submit | `require_job_success(app_id, "changeset", changeset_id, MsuiteSubmit)` |
| Changeset queue | `require_job_success(app_id, "changeset", changeset_id, MsuiteSubmit)` |
| Release publish | `require_job_success(app_id, "release", release_id, MsuiteMerge)` |
| Deploy release | `require_job_success(app_id, "deployment", deployment_id, MsuiteDeploy)` |

Note: The submit handler first *creates* the `MsuiteSubmit` job. The gate hook
is checked on the *next* transition (e.g. moving from `submitted` to
`in_review` or from `approved` to `queued`), ensuring the msuite job has
completed before the changeset advances.

### Structured logging

Workers receive a `JobLogger` and call `logger.info(msg, data)`,
`logger.warn(msg, data)`, or `logger.error(msg, data)` during execution.
Each call inserts a document into `job_logs` immediately (fire-and-forget --
insertion failures are traced but never block the worker). This gives operators
real-time visibility into job progress.

### msuite workers

The three msuite workers (`MsuiteSubmit`, `MsuiteMerge`, `MsuiteDeploy`) are
placeholder implementations in v1. They simulate the external msuite test
runner with configurable pass/fail behavior:

```rust
pub struct MsuiteSubmitWorker {
    logger: JobLogger,
}

#[async_trait]
impl JobWorker for MsuiteSubmitWorker {
    async fn execute(&self, job: &Job) -> Result<serde_json::Value, ConmanError> {
        self.logger.info("starting msuite submit validation", None).await;

        // Placeholder: simulate test execution
        // Real implementation will call external msuite service
        let changeset_id = job.payload.get("changeset_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ConmanError::Validation {
                message: "missing changeset_id in payload".to_string(),
            })?;

        self.logger.info(
            &format!("running validation for changeset {}", changeset_id),
            Some(serde_json::json!({ "changeset_id": changeset_id })),
        ).await;

        // Simulate success
        let result = serde_json::json!({
            "passed": true,
            "test_count": 0,
            "failures": [],
            "simulated": true,
        });

        self.logger.info("msuite submit validation complete", Some(result.clone())).await;
        Ok(result)
    }
}
```

`MsuiteMergeWorker` and `MsuiteDeployWorker` follow the same pattern with
their respective payload expectations.

---

## 7. Gitaly-rs Integration

N/A for the job framework itself. Individual workers (msuite, release assembly,
deployment) may need gitaly access, but that is handled by those workers
calling into `conman-git` and is specified in their respective epics (E08,
E09).

---

## 8. Implementation Checklist

### E06-01: Generic jobs framework

- [ ] Add `JobType`, `JobState`, `LogLevel` enums to `conman-core`
- [ ] Add `Job`, `JobLog` domain structs to `conman-core`
- [ ] Add `JobState::is_terminal()` helper
- [ ] Implement `Display` / `as_str()` for enums (needed for MongoDB string storage)
- [ ] Add `conman-jobs` crate to workspace `Cargo.toml`
- [ ] Define `JobWorker` trait in `conman-jobs`
- [ ] Define `JobLogger` struct in `conman-jobs`
- [ ] Implement `JobLogger::info`, `warn`, `error`, and internal `write`
- [ ] Unit tests for `JobState::is_terminal()`
- [ ] Unit tests for enum serialization round-trips

### E06-02: Job repositories and runner

- [ ] Implement `JobRepo` in `conman-db` with CRUD operations
- [ ] Implement `JobRepo::claim_next()` using atomic `findOneAndUpdate`
- [ ] Implement `JobRepo::mark_succeeded()`, `mark_failed()`, `mark_canceled()`
- [ ] Implement `JobRepo::requeue_for_retry()`
- [ ] Implement `JobLogRepo` in `conman-db` with insert + query by job_id
- [ ] Create indexes on `jobs` collection in `ensure_indexes()`
- [ ] Create indexes on `job_logs` collection in `ensure_indexes()`
- [ ] Implement `JobRunner` polling loop in `conman-jobs`
- [ ] Wire shutdown signal via `tokio::sync::watch`
- [ ] Add `CONMAN_JOB_POLL_INTERVAL_MS` config (default 1000)
- [ ] Integration test: runner claims and executes a queued job
- [ ] Integration test: runner handles empty queue gracefully

### E06-03: msuite placeholder workers

- [ ] Implement `MsuiteSubmitWorker` with structured logging
- [ ] Implement `MsuiteMergeWorker` with structured logging
- [ ] Implement `MsuiteDeployWorker` with structured logging
- [ ] Register workers in `JobRunner` worker map
- [ ] Wire `JobRunner` startup in `main.rs` with `tokio::spawn`
- [ ] Integration test: msuite_submit worker produces expected result payload
- [ ] Integration test: worker failure is captured as job error

### E06-04: API endpoints

- [ ] Add `JobResponse`, `JobLogResponse`, `ListJobsQuery` to `conman-api`
- [ ] Implement `GET /api/repos/:appId/jobs/:jobId` handler
- [ ] Implement `GET /api/repos/:appId/jobs` handler with filters + pagination
- [ ] Add routes to Axum router
- [ ] Integration test: get job by id returns correct response
- [ ] Integration test: list jobs with type/state filters
- [ ] Integration test: pagination returns correct total and pages

### E06-05: Gate hooks

- [ ] Implement `require_job_success()` in `conman-jobs`
- [ ] Integrate gate hook into changeset submit flow (E05 handler)
- [ ] Integrate gate hook into changeset queue flow (E07 handler, stubbed)
- [ ] Integrate gate hook into release publish flow (E08 handler, stubbed)
- [ ] Integrate gate hook into deploy flow (E09 handler, stubbed)
- [ ] Unit test: gate passes when successful job exists
- [ ] Unit test: gate fails when no successful job exists
- [ ] Unit test: gate fails when only failed/running/queued jobs exist

### E06-06: Retry and timeout policies

- [ ] Implement timeout handling in `JobRunner::poll_and_execute`
- [ ] Implement retry logic in `JobRunner::handle_failure`
- [ ] Add default retry/timeout constants per `JobType`
- [ ] Integration test: failed job with retries remaining is re-queued
- [ ] Integration test: failed job with max retries stays Failed
- [ ] Integration test: timed-out job is Canceled
- [ ] Integration test: re-queued job is picked up and retried

---

## 9. Test Cases

### Unit tests (conman-core, conman-jobs)

1. **Job creation sets state to Queued.** Create a `Job` with `JobState::Queued`
   and verify all default fields are set correctly (`retry_count = 0`,
   `result = None`, `started_at = None`, `completed_at = None`).

2. **JobState::is_terminal returns correct values.** Verify that `Succeeded`,
   `Failed`, and `Canceled` return `true`; `Queued` and `Running` return
   `false`.

3. **Enum serialization round-trips.** Serialize each `JobType`, `JobState`,
   and `LogLevel` variant to JSON and deserialize back, verifying equality.

4. **Gate hook passes when successful job exists.** Given a `JobRepo` containing
   a `Succeeded` job matching `(app_id, entity_type, entity_id, job_type)`,
   `require_job_success` returns `Ok(())`.

5. **Gate hook fails when no successful job exists.** Given an empty collection
   (or only `Failed`/`Running`/`Queued` jobs), `require_job_success` returns
   `Err(ConmanError::Conflict)`.

6. **Gate hook fails with only non-succeeded jobs.** Insert `Failed`, `Running`,
   and `Queued` jobs for the same entity. Verify the gate still returns
   `Err(ConmanError::Conflict)`.

### Integration tests (require MongoDB)

7. **Runner picks up oldest queued job.** Insert two Queued jobs with different
   `created_at` values. Start the runner. Verify the older job is claimed
   first (transitions to `Running` before the newer one).

8. **Successful execution transitions to Succeeded with result.** Create a
   Queued job and register a worker that returns `Ok(json!({"ok": true}))`.
   After runner processes it, verify `state = Succeeded`, `result` matches,
   `completed_at` is set.

9. **Failed execution transitions to Failed with error.** Create a Queued job
   and register a worker that returns `Err(...)`. With `max_retries = 0`,
   verify `state = Failed`, `error_message` is set, `completed_at` is set.

10. **Retry re-queues with incremented count.** Create a Queued job with
    `max_retries = 2`. Register a failing worker. After first execution, verify
    `state = Queued`, `retry_count = 1`, `started_at = None`.

11. **Max retries exceeded stays Failed.** Create a Queued job with
    `max_retries = 1`, `retry_count = 1`. Register a failing worker. After
    execution, verify `state = Failed` (not re-queued).

12. **Timeout cancels running job.** Create a Queued job with `timeout_ms = 50`.
    Register a worker that sleeps for 200ms. After runner processes it, verify
    `state = Canceled`, `error_message` contains "timeout".

13. **Concurrent runners don't double-process (claim mechanism).** Insert one
    Queued job. Spawn two runner instances. Verify that exactly one succeeds
    in claiming (the other gets `None` from `findOneAndUpdate`). Verify the job
    is processed exactly once.

14. **Job logs are queryable by job_id.** Create a job, execute a worker that
    writes 3 log entries via `JobLogger`. Query `job_logs` by `job_id`. Verify
    3 entries returned in chronological order with correct levels and messages.

15. **API: get job returns 404 for wrong app.** Create a job under `app_a`.
    Request `GET /api/repos/:app_b/jobs/:jobId`. Verify 404.

16. **API: list jobs with type filter.** Create 3 jobs (2 MsuiteSubmit, 1
    DeployRelease). Request `GET /api/repos/:appId/jobs?type=msuite_submit`.
    Verify 2 results.

17. **API: list jobs with state filter.** Create 3 jobs (1 Queued, 1 Running, 1
    Succeeded). Request `GET /api/repos/:appId/jobs?state=queued`. Verify 1
    result.

18. **API: list jobs pagination.** Create 25 jobs. Request with
    `page=2&limit=10`. Verify 10 results on page 2, `total = 25`.

---

## 10. Acceptance Criteria

1. **Async execution.** All msuite checks, revalidation, assembly, deployment,
   and temp-env operations run as background jobs -- never inline in HTTP
   handlers.

2. **State tracking.** Every job transitions through `Queued -> Running ->
   Succeeded | Failed | Canceled`. No job gets stuck in an intermediate state
   indefinitely (timeout ensures forward progress).

3. **Gate enforcement.** Submit, queue, release-publish, and deploy handlers
   call `require_job_success` and return `409 Conflict` when the required job
   has not succeeded. Workflow transitions are blocked until the mandatory
   check passes.

4. **Pollable status.** Clients can poll `GET /api/repos/:appId/jobs/:jobId` to
   observe job progress and retrieve the terminal result/error.

5. **Structured logs.** Workers emit real-time log entries to `job_logs`.
   Operators can query logs by `job_id` in chronological order for debugging.

6. **Retry resilience.** Transient failures are retried up to `max_retries`
   without manual intervention. Permanent failures surface a clear
   `error_message`.

7. **Timeout safety.** Jobs that exceed `timeout_ms` are automatically canceled,
   preventing resource exhaustion from hung workers.

8. **No double-processing.** The atomic claim mechanism guarantees that
   concurrent runner instances never execute the same job twice.

9. **Clean shutdown.** The job runner exits its polling loop gracefully on
   receiving the shutdown signal, allowing in-flight jobs to complete before
   the process terminates.

10. **Horizontal scalability.** Multiple `JobRunner` instances can run in
    parallel (e.g. across pods) with correctness guaranteed by the atomic
    claim. No external coordination service is required.

11. **Profile-aware gate defaults.** Gate configuration supports:
    submit -> temp profile only, release publish -> environment profiles only,
    deploy -> target environment profile only.

12. **Migration execution metadata is persisted.** Jobs that execute migration
    commands record applied migration identifiers/status into Conman metadata
    for downstream drift detection and deploy gating.
