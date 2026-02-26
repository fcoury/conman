use std::sync::Arc;

use conman_api::{AppState, build_router};
use conman_core::Config;

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

    conman_db::bootstrap_indexes(&[])
        .await
        .expect("failed to bootstrap MongoDB indexes");

    let gitaly_channel = match tonic::transport::Channel::from_shared(config.gitaly_address.clone())
    {
        Ok(endpoint) => endpoint.connect().await.ok(),
        Err(_) => None,
    };

    if gitaly_channel.is_some() {
        tracing::info!(addr = %config.gitaly_address, "gitaly-rs channel connected");
    } else {
        tracing::warn!(addr = %config.gitaly_address, "gitaly-rs channel not available (will retry on use)");
    }

    let state = AppState {
        config: Arc::new(config.clone()),
        db,
        gitaly_channel,
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
