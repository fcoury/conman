use std::sync::Arc;

use clap::{Parser, Subcommand};
use conman_api::{AppState, build_router};
use conman_auth::hash_password;
use conman_core::Config;
use conman_db::UserRepo;
use conman_git::{GitAdapter, GitalyClient, NoopGitAdapter};
use conman_jobs::JobRunner;
use mongodb::Client;

#[derive(Debug, Parser)]
#[command(name = "conman")]
#[command(about = "Conman configuration manager service")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start API + job runner (default command).
    Serve,
    /// Create or update an initial admin login user in MongoDB.
    BootstrapAdmin {
        email: String,
        name: String,
        password: String,
        #[arg(long)]
        mongo_uri: Option<String>,
        #[arg(long)]
        mongo_db: Option<String>,
    },
}

#[tokio::main]
async fn main() {
    if let Ok(path) = dotenvy::dotenv() {
        eprintln!("loaded environment from {}", path.display());
    }

    let cli = Cli::parse();
    let command = cli.command.unwrap_or(Command::Serve);
    let result = match command {
        Command::Serve => serve().await,
        Command::BootstrapAdmin {
            email,
            name,
            password,
            mongo_uri,
            mongo_db,
        } => bootstrap_admin(email, name, password, mongo_uri, mongo_db).await,
    };

    if let Err(err) = result {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

async fn serve() -> Result<(), String> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "conman=debug,tower_http=debug".into()),
        )
        .json()
        .init();

    conman_api::metrics::init_metrics().map_err(|err| format!("metrics init failed: {err}"))?;

    let config =
        Config::from_env().map_err(|err| format!("failed to load configuration: {err}"))?;
    tracing::info!(listen = %config.listen_addr, "configuration loaded");

    let db = conman_db::connect_mongo(&config)
        .await
        .map_err(|err| format!("failed to connect to MongoDB: {err}"))?;

    let user_repo = conman_db::UserRepo::new(db.clone());
    let repo_membership_repo = conman_db::RepoMembershipRepo::new(db.clone());
    let team_repo = conman_db::TeamRepo::new(db.clone());
    let team_membership_repo = conman_db::TeamMembershipRepo::new(db.clone());
    let repo_store = conman_db::RepoStore::new(db.clone());
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
        &repo_membership_repo,
        &team_repo,
        &team_membership_repo,
        &repo_store,
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
    .map_err(|err| format!("failed to bootstrap MongoDB indexes: {err}"))?;

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
        .map_err(|err| format!("failed to bind TCP listener: {err}"))?;

    tracing::info!(addr = %config.listen_addr, "server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|err| format!("server error: {err}"))?;

    Ok(())
}

async fn bootstrap_admin(
    email: String,
    name: String,
    password: String,
    mongo_uri: Option<String>,
    mongo_db: Option<String>,
) -> Result<(), String> {
    let email = email.trim().to_lowercase();
    let name = name.trim().to_string();
    if email.is_empty() || name.is_empty() || password.is_empty() {
        return Err("email, name, and password are required".to_string());
    }

    let mongo_uri = mongo_uri
        .or_else(|| std::env::var("CONMAN_MONGO_URI").ok())
        .unwrap_or_else(|| "mongodb://localhost:27017".to_string());
    let mongo_db = mongo_db
        .or_else(|| std::env::var("CONMAN_MONGO_DB").ok())
        .unwrap_or_else(|| "conman".to_string());

    let password_hash = hash_password(&password).map_err(|err| err.to_string())?;
    let client = Client::with_uri_str(&mongo_uri)
        .await
        .map_err(|err| format!("failed to connect mongo at {mongo_uri}: {err}"))?;
    let users = UserRepo::new(client.database(&mongo_db));

    match users
        .find_by_email(&email)
        .await
        .map_err(|err| err.to_string())?
    {
        Some(existing) => {
            users
                .update_password(&existing.id, &password_hash)
                .await
                .map_err(|err| format!("failed to update user password: {err}"))?;
            println!("updated existing user");
            println!("  user_id: {}", existing.id);
            println!("  email:   {}", existing.email);
        }
        None => {
            let created = users
                .insert(&email, &name, &password_hash)
                .await
                .map_err(|err| format!("failed to create user: {err}"))?;
            println!("created new user");
            println!("  user_id: {}", created.id);
            println!("  email:   {}", created.email);
        }
    }
    println!("  mongo:   {mongo_uri}/{mongo_db}");

    Ok(())
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
