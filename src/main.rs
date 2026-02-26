use std::sync::Arc;

use conman_api::{AppState, build_router};
use conman_core::Config;
use conman_git::{GitAdapter, GitalyClient, NoopGitAdapter};
use conman_jobs::JobRunner;

#[tokio::main]
async fn main() {
    if let Ok(path) = dotenvy::dotenv() {
        eprintln!("loaded environment from {}", path.display());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "conman=debug,tower_http=debug".into()),
        )
        .json()
        .init();

    conman_api::metrics::init_metrics().expect("failed to initialize metrics recorder");

    let config = Config::from_env().expect("failed to load configuration");
    tracing::info!(listen = %config.listen_addr, "configuration loaded");

    let db = conman_db::connect_mongo(&config)
        .await
        .expect("failed to connect to MongoDB");

    let user_repo = conman_db::UserRepo::new(db.clone());
    let membership_repo = conman_db::MembershipRepo::new(db.clone());
    let app_repo = conman_db::AppRepo::new(db.clone());
    let environment_repo = conman_db::EnvironmentRepo::new(db.clone());
    let invite_repo = conman_db::InviteRepo::new(db.clone());
    let runtime_profile_repo = conman_db::RuntimeProfileRepo::new(db.clone());
    let password_reset_repo = conman_db::PasswordResetRepo::new(db.clone());
    let workspace_repo = conman_db::WorkspaceRepo::new(db.clone());
    let changeset_repo = conman_db::ChangesetRepo::new(db.clone());
    let changeset_comment_repo = conman_db::ChangesetCommentRepo::new(db.clone());
    let changeset_profile_override_repo = conman_db::ChangesetProfileOverrideRepo::new(db.clone());
    let job_repo = conman_db::JobRepo::new(db.clone());
    let release_repo = conman_db::ReleaseRepo::new(db.clone());
    let deployment_repo = conman_db::DeploymentRepo::new(db.clone());
    let temp_env_repo = conman_db::TempEnvRepo::new(db.clone());
    let audit_repo = conman_db::AuditRepo::new(db.clone());
    let notification_pref_repo = conman_db::NotificationPreferenceRepo::new(db.clone());
    let notification_event_repo = conman_db::NotificationEventRepo::new(db.clone());
    conman_db::bootstrap_indexes(&[
        &user_repo,
        &membership_repo,
        &app_repo,
        &environment_repo,
        &invite_repo,
        &runtime_profile_repo,
        &password_reset_repo,
        &workspace_repo,
        &changeset_repo,
        &changeset_comment_repo,
        &changeset_profile_override_repo,
        &job_repo,
        &release_repo,
        &deployment_repo,
        &temp_env_repo,
        &audit_repo,
        &notification_pref_repo,
        &notification_event_repo,
    ])
    .await
    .expect("failed to bootstrap MongoDB indexes");

    let git_adapter: Arc<dyn GitAdapter> = match GitalyClient::connect(&config.gitaly_address).await
    {
        Ok(client) => {
            tracing::info!(addr = %config.gitaly_address, "gitaly-rs adapter connected");
            Arc::new(client)
        }
        Err(err) => {
            tracing::warn!(
                addr = %config.gitaly_address,
                error = %err,
                "gitaly-rs adapter unavailable, using noop adapter"
            );
            Arc::new(NoopGitAdapter)
        }
    };

    let state = AppState {
        config: Arc::new(config.clone()),
        db,
        git_adapter,
        rate_limiter: conman_api::rate_limit::new_shared_rate_limiter(
            config.http_rate_limit_per_second,
        ),
    };

    let mut job_runner = JobRunner::new(state.db.clone(), &config);
    match conman_jobs::SmtpNotificationSender::from_config(&config) {
        Ok(Some(sender)) => {
            tracing::info!("smtp notification sender configured");
            job_runner = job_runner.with_notification_sender(Arc::new(sender));
        }
        Ok(None) => {
            tracing::info!("smtp not configured, using logging notification sender");
        }
        Err(err) => {
            tracing::warn!(error = %err, "invalid smtp configuration; using logging sender");
        }
    }
    let _job_runner = job_runner.spawn();

    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(config.listen_addr)
        .await
        .expect("failed to bind TCP listener");

    tracing::info!(addr = %config.listen_addr, "server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

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
