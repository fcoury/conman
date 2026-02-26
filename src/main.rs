use std::sync::Arc;

use conman_api::{AppState, build_router};
use conman_core::Config;
use conman_git::{GitAdapter, GitalyClient, NoopGitAdapter};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "conman=debug,tower_http=debug".into()),
        )
        .json()
        .init();

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
    };

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
