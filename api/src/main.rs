mod config;
mod cors;
mod error;
mod indexer;
mod models;
mod routes;
mod state;
mod streaming_math;

use std::net::SocketAddr;

use tower_http::trace::TraceLayer;

use config::Config;
use state::AppState;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env();

    if !config.has_real_contract_id() {
        tracing::warn!(
            stream_contract_id = %config.stream_contract_id,
            "STREAM_CONTRACT_ID is not set to a real deployed contract address yet; \
             the indexer will track ledgers but will not be able to match or decode \
             stream contract events until this is set. See README.md."
        );
    }

    let bind_addr: SocketAddr = match config.bind_addr.parse() {
        Ok(addr) => addr,
        Err(e) => {
            tracing::error!(bind_addr = %config.bind_addr, error = %e, "invalid BIND_ADDR, exiting");
            std::process::exit(1);
        }
    };

    let cors = cors::layer(&config.cors_allowed_origins);
    let state = AppState::new(config);

    // Background task: poll Soroban RPC on a fixed interval and update `state` with any
    // new stream activity. This runs for the lifetime of the process; if it errors
    // on a given tick (e.g. the RPC is briefly unavailable) it logs and retries on
    // the next tick rather than taking the whole process down.
    tokio::spawn(indexer::run(state.clone()));

    let app = routes::router(state).layer(TraceLayer::new_for_http()).layer(cors);

    tracing::info!(%bind_addr, "starting stellar-stream-hive-api");

    let listener = match tokio::net::TcpListener::bind(bind_addr).await {
        Ok(listener) => listener,
        Err(e) => {
            tracing::error!(%bind_addr, error = %e, "failed to bind TCP listener, exiting");
            std::process::exit(1);
        }
    };

    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        tracing::error!(error = %e, "server exited with error");
        std::process::exit(1);
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %e, "failed to install Ctrl+C handler");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => tracing::error!(error = %e, "failed to install SIGTERM handler"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received, shutting down gracefully");
}
