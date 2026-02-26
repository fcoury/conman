use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use conman_core::{ConmanError, Job, JobType};
use conman_db::{EnqueueJobInput, JobRepo};
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

#[derive(Clone)]
pub struct JobRunner {
    repo: JobRepo,
    workers: Arc<HashMap<JobType, Arc<dyn JobWorker>>>,
    poll_interval: Duration,
}

impl JobRunner {
    pub fn new(db: mongodb::Database) -> Self {
        let mut workers: HashMap<JobType, Arc<dyn JobWorker>> = HashMap::new();
        for job_type in [
            JobType::MsuiteSubmit,
            JobType::MsuiteMerge,
            JobType::MsuiteDeploy,
            JobType::RevalidateQueuedChangeset,
            JobType::ReleaseAssemble,
            JobType::DeployRelease,
            JobType::RuntimeProfileDriftCheck,
            JobType::TempEnvProvision,
            JobType::TempEnvExpire,
        ] {
            workers.insert(job_type, Arc::new(NoopWorker));
        }

        Self {
            repo: JobRepo::new(db),
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

    pub async fn enqueue(&self, input: EnqueueJobInput) -> Result<conman_core::Job, ConmanError> {
        self.repo.enqueue(input).await
    }

    pub async fn tick(&self) -> Result<(), ConmanError> {
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

        let outcome = tokio::time::timeout(timeout, worker.run(&job)).await;
        match outcome {
            Ok(Ok(result)) => {
                self.repo.complete_success(&job.id, result).await?;
                self.repo
                    .append_log(&job.app_id, &job.id, "info", "job succeeded")
                    .await?;
            }
            Ok(Err(err)) => {
                self.repo.complete_failure(&job.id, err.clone()).await?;
                self.repo
                    .append_log(&job.app_id, &job.id, "error", &format!("job failed: {err}"))
                    .await?;
            }
            Err(_) => {
                let err = "job timed out".to_string();
                self.repo.complete_failure(&job.id, err.clone()).await?;
                self.repo
                    .append_log(&job.app_id, &job.id, "error", &err)
                    .await?;
            }
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
