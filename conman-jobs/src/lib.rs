use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use conman_core::{
    ChangesetState, ConmanError, DeploymentState, Job, JobState, JobType, NotificationEvent,
    TempEnvState,
};
use conman_db::{
    ChangesetProfileOverrideRepo, ChangesetRepo, DeploymentRepo, EnqueueJobInput, JobRepo,
    NotificationEventRepo, ReleaseRepo, TempEnvRepo,
};
use metrics::{counter, gauge, histogram};
use serde_json::json;

#[async_trait]
pub trait JobWorker: Send + Sync {
    async fn run(&self, job: &Job) -> Result<serde_json::Value, String>;
}

#[derive(Default)]
pub struct NoopWorker;

#[async_trait]
impl JobWorker for NoopWorker {
    async fn run(&self, _job: &Job) -> Result<serde_json::Value, String> {
        Ok(json!({"status": "ok", "worker": "noop"}))
    }
}

#[async_trait]
pub trait NotificationSender: Send + Sync {
    async fn send(&self, event: &NotificationEvent) -> Result<(), String>;
}

#[derive(Default)]
pub struct LoggingNotificationSender;

#[async_trait]
impl NotificationSender for LoggingNotificationSender {
    async fn send(&self, event: &NotificationEvent) -> Result<(), String> {
        tracing::info!(
            notification_id = %event.id,
            user_id = %event.user_id,
            event_type = %event.event_type,
            subject = %event.subject,
            "notification queued for delivery"
        );
        Ok(())
    }
}

pub struct ReleaseAssembleWorker {
    releases: ReleaseRepo,
}

#[async_trait]
impl JobWorker for ReleaseAssembleWorker {
    async fn run(&self, job: &Job) -> Result<serde_json::Value, String> {
        let release = self
            .releases
            .set_state(&job.entity_id, conman_core::ReleaseState::Validated)
            .await
            .map_err(|e| e.to_string())?;
        Ok(json!({
            "status": "validated",
            "release_id": release.id,
            "state": release.state,
        }))
    }
}

pub struct DeployReleaseWorker {
    deployments: DeploymentRepo,
}

#[async_trait]
impl JobWorker for DeployReleaseWorker {
    async fn run(&self, job: &Job) -> Result<serde_json::Value, String> {
        self.deployments
            .set_state(&job.entity_id, DeploymentState::Running)
            .await
            .map_err(|e| e.to_string())?;
        let deployment = self
            .deployments
            .set_state(&job.entity_id, DeploymentState::Succeeded)
            .await
            .map_err(|e| e.to_string())?;
        Ok(json!({
            "status": "deployed",
            "deployment_id": deployment.id,
            "state": deployment.state,
        }))
    }
}

pub struct TempEnvProvisionWorker {
    temp_envs: TempEnvRepo,
}

#[async_trait]
impl JobWorker for TempEnvProvisionWorker {
    async fn run(&self, job: &Job) -> Result<serde_json::Value, String> {
        let _temp_env = self
            .temp_envs
            .set_state(&job.entity_id, TempEnvState::Active, None)
            .await
            .map_err(|e| e.to_string())?;
        let temp_env = self
            .temp_envs
            .touch_activity(&job.entity_id)
            .await
            .map_err(|e| e.to_string())?;
        Ok(json!({
            "status": "active",
            "temp_env_id": temp_env.id,
            "url": temp_env.url,
            "state": temp_env.state,
        }))
    }
}

pub struct TempEnvExpireWorker {
    temp_envs: TempEnvRepo,
}

#[async_trait]
impl JobWorker for TempEnvExpireWorker {
    async fn run(&self, job: &Job) -> Result<serde_json::Value, String> {
        let Some(temp_env) = self
            .temp_envs
            .find_by_id(&job.entity_id)
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(json!({"status": "skipped", "reason": "not_found"}));
        };
        let now = Utc::now();
        match temp_env.state {
            TempEnvState::Provisioning | TempEnvState::Active => {
                if temp_env.expires_at <= now {
                    let grace = now + ChronoDuration::seconds(temp_env.grace_ttl_seconds);
                    let row = self
                        .temp_envs
                        .set_state(&temp_env.id, TempEnvState::Expiring, Some(grace))
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok(json!({
                        "status": "expiring",
                        "temp_env_id": row.id,
                        "grace_expires_at": row.grace_expires_at,
                    }))
                } else {
                    Ok(json!({"status": "skipped", "reason": "not_due"}))
                }
            }
            TempEnvState::Expiring | TempEnvState::Deleted | TempEnvState::Expired => {
                if temp_env.grace_expires_at.is_some_and(|grace| grace <= now) {
                    self.temp_envs
                        .hard_delete(&temp_env.id)
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok(json!({
                        "status": "deleted",
                        "temp_env_id": temp_env.id,
                    }))
                } else {
                    Ok(json!({"status": "skipped", "reason": "in_grace"}))
                }
            }
        }
    }
}

pub struct RevalidateQueuedChangesetWorker {
    changesets: ChangesetRepo,
    overrides: ChangesetProfileOverrideRepo,
}

#[async_trait]
impl JobWorker for RevalidateQueuedChangesetWorker {
    async fn run(&self, job: &Job) -> Result<serde_json::Value, String> {
        let Some(changeset) = self
            .changesets
            .find_by_id(&job.entity_id)
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(json!({"status": "skipped", "reason": "not_found"}));
        };
        if changeset.state != ChangesetState::Queued {
            return Ok(json!({"status": "skipped", "reason": "not_queued"}));
        }

        let current_overrides = self
            .overrides
            .list_by_changeset(&changeset.id)
            .await
            .map_err(|e| e.to_string())?;
        let queued = self
            .changesets
            .list_queued_by_app(&changeset.app_id)
            .await
            .map_err(|e| e.to_string())?;
        let current_position = changeset.queue_position.unwrap_or(i64::MAX);

        for other in queued {
            if other.id == changeset.id {
                continue;
            }
            if other.queue_position.unwrap_or(i64::MAX) >= current_position {
                continue;
            }
            let other_overrides = self
                .overrides
                .list_by_changeset(&other.id)
                .await
                .map_err(|e| e.to_string())?;
            for current in &current_overrides {
                let conflict = other_overrides.iter().any(|existing| {
                    current.key == existing.key
                        && current.target_profile_id == existing.target_profile_id
                        && current.value != existing.value
                });
                if conflict {
                    let row = self
                        .changesets
                        .mark_conflicted(&changeset.id)
                        .await
                        .map_err(|e| e.to_string())?;
                    return Ok(json!({
                        "status": "conflicted",
                        "changeset_id": row.id,
                        "conflict_with": other.id,
                    }));
                }
            }
        }

        let force_fail = job
            .payload
            .get("force_fail")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if force_fail {
            let row = self
                .changesets
                .mark_needs_revalidation(&changeset.id)
                .await
                .map_err(|e| e.to_string())?;
            return Ok(json!({
                "status": "needs_revalidation",
                "changeset_id": row.id,
            }));
        }

        Ok(json!({
            "status": "revalidated",
            "changeset_id": changeset.id,
            "state": changeset.state,
        }))
    }
}

#[derive(Clone)]
pub struct JobRunner {
    repo: JobRepo,
    temp_env_repo: TempEnvRepo,
    notification_repo: NotificationEventRepo,
    notification_sender: Arc<dyn NotificationSender>,
    workers: Arc<HashMap<JobType, Arc<dyn JobWorker>>>,
    poll_interval: Duration,
}

const JOBS_ENQUEUED_TOTAL: &str = "conman_jobs_enqueued_total";
const JOBS_COMPLETED_TOTAL: &str = "conman_jobs_completed_total";
const JOB_DURATION_SECONDS: &str = "conman_job_duration_seconds";
const JOB_QUEUE_DEPTH: &str = "conman_job_queue_depth";

fn job_type_label(job_type: JobType) -> &'static str {
    match job_type {
        JobType::MsuiteSubmit => "msuite_submit",
        JobType::MsuiteMerge => "msuite_merge",
        JobType::MsuiteDeploy => "msuite_deploy",
        JobType::RevalidateQueuedChangeset => "revalidate_queued_changeset",
        JobType::ReleaseAssemble => "release_assemble",
        JobType::DeployRelease => "deploy_release",
        JobType::RuntimeProfileDriftCheck => "runtime_profile_drift_check",
        JobType::TempEnvProvision => "temp_env_provision",
        JobType::TempEnvExpire => "temp_env_expire",
    }
}

impl JobRunner {
    pub fn new(db: mongodb::Database) -> Self {
        let mut workers: HashMap<JobType, Arc<dyn JobWorker>> = HashMap::new();
        workers.insert(JobType::MsuiteSubmit, Arc::new(NoopWorker));
        workers.insert(JobType::MsuiteMerge, Arc::new(NoopWorker));
        workers.insert(JobType::MsuiteDeploy, Arc::new(NoopWorker));
        workers.insert(
            JobType::RevalidateQueuedChangeset,
            Arc::new(RevalidateQueuedChangesetWorker {
                changesets: ChangesetRepo::new(db.clone()),
                overrides: ChangesetProfileOverrideRepo::new(db.clone()),
            }),
        );
        workers.insert(
            JobType::ReleaseAssemble,
            Arc::new(ReleaseAssembleWorker {
                releases: ReleaseRepo::new(db.clone()),
            }),
        );
        workers.insert(
            JobType::DeployRelease,
            Arc::new(DeployReleaseWorker {
                deployments: DeploymentRepo::new(db.clone()),
            }),
        );
        workers.insert(JobType::RuntimeProfileDriftCheck, Arc::new(NoopWorker));
        workers.insert(
            JobType::TempEnvProvision,
            Arc::new(TempEnvProvisionWorker {
                temp_envs: TempEnvRepo::new(db.clone()),
            }),
        );
        workers.insert(
            JobType::TempEnvExpire,
            Arc::new(TempEnvExpireWorker {
                temp_envs: TempEnvRepo::new(db.clone()),
            }),
        );

        Self {
            repo: JobRepo::new(db.clone()),
            temp_env_repo: TempEnvRepo::new(db.clone()),
            notification_repo: NotificationEventRepo::new(db),
            notification_sender: Arc::new(LoggingNotificationSender),
            workers: Arc::new(workers),
            poll_interval: Duration::from_secs(1),
        }
    }

    pub fn with_worker(mut self, job_type: JobType, worker: Arc<dyn JobWorker>) -> Self {
        Arc::make_mut(&mut self.workers).insert(job_type, worker);
        self
    }

    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    pub fn with_notification_sender(mut self, sender: Arc<dyn NotificationSender>) -> Self {
        self.notification_sender = sender;
        self
    }

    pub async fn enqueue(&self, input: EnqueueJobInput) -> Result<conman_core::Job, ConmanError> {
        let label = job_type_label(input.job_type);
        let job = self.repo.enqueue(input).await?;
        counter!(JOBS_ENQUEUED_TOTAL, "job_type" => label).increment(1);
        Ok(job)
    }

    pub async fn tick(&self) -> Result<(), ConmanError> {
        self.enqueue_due_temp_env_expiry_jobs().await?;
        self.drain_notification_outbox().await?;
        if let Ok(queued) = self.repo.count_queued().await {
            gauge!(JOB_QUEUE_DEPTH).set(queued as f64);
        }

        let Some(job) = self.repo.reserve_next_queued().await? else {
            return Ok(());
        };

        let worker = self
            .workers
            .get(&job.job_type)
            .cloned()
            .unwrap_or_else(|| Arc::new(NoopWorker));
        let timeout = Duration::from_millis(job.timeout_ms.max(1000));

        self.repo
            .append_log(&job.app_id, &job.id, "info", "job started")
            .await?;
        let started_at = std::time::Instant::now();
        let job_type = job_type_label(job.job_type);

        let outcome = tokio::time::timeout(timeout, worker.run(&job)).await;
        match outcome {
            Ok(Ok(result)) => {
                self.repo.complete_success(&job.id, result).await?;
                self.repo
                    .append_log(&job.app_id, &job.id, "info", "job succeeded")
                    .await?;
                counter!(JOBS_COMPLETED_TOTAL, "job_type" => job_type, "outcome" => "succeeded")
                    .increment(1);
            }
            Ok(Err(err)) => {
                self.repo.complete_failure(&job.id, err.clone()).await?;
                self.repo
                    .append_log(&job.app_id, &job.id, "error", &format!("job failed: {err}"))
                    .await?;
                counter!(JOBS_COMPLETED_TOTAL, "job_type" => job_type, "outcome" => "failed")
                    .increment(1);
            }
            Err(_) => {
                let err = "job timed out".to_string();
                self.repo.complete_failure(&job.id, err.clone()).await?;
                self.repo
                    .append_log(&job.app_id, &job.id, "error", &err)
                    .await?;
                counter!(JOBS_COMPLETED_TOTAL, "job_type" => job_type, "outcome" => "timed_out")
                    .increment(1);
            }
        }
        histogram!(JOB_DURATION_SECONDS, "job_type" => job_type)
            .record(started_at.elapsed().as_secs_f64());

        Ok(())
    }

    async fn drain_notification_outbox(&self) -> Result<(), ConmanError> {
        for _ in 0..20 {
            let Some(event) = self.notification_repo.reserve_next_queued().await? else {
                break;
            };
            match self.notification_sender.send(&event).await {
                Ok(()) => {
                    self.notification_repo.mark_sent(&event.id).await?;
                }
                Err(err) => {
                    self.notification_repo.mark_failed(&event.id, &err).await?;
                }
            }
        }
        Ok(())
    }

    async fn enqueue_due_temp_env_expiry_jobs(&self) -> Result<(), ConmanError> {
        let due = self.temp_env_repo.list_due_for_expiry_scan(100).await?;
        for temp_env in due {
            let existing = self
                .repo
                .latest_for_entity(
                    &temp_env.app_id,
                    "temp_environment",
                    &temp_env.id,
                    JobType::TempEnvExpire,
                )
                .await?;
            let has_inflight = existing
                .map(|job| matches!(job.state, JobState::Queued | JobState::Running))
                .unwrap_or(false);
            if has_inflight {
                continue;
            }
            self.repo
                .enqueue(EnqueueJobInput {
                    app_id: temp_env.app_id.clone(),
                    job_type: JobType::TempEnvExpire,
                    entity_type: "temp_environment".to_string(),
                    entity_id: temp_env.id.clone(),
                    payload: json!({"trigger": "idle_ttl_scan"}),
                    max_retries: 1,
                    timeout_ms: 10 * 60 * 1000,
                    created_by: None,
                })
                .await?;
        }
        Ok(())
    }

    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                if let Err(err) = self.tick().await {
                    tracing::error!(error = %err, "job runner tick failed");
                }
                tokio::time::sleep(self.poll_interval).await;
            }
        })
    }
}
