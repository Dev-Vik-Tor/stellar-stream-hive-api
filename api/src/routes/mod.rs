pub mod recipients;
pub mod senders;
pub mod stats;
pub mod streams;

use axum::{
    routing::get,
    Router,
};

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/streams", get(streams::list_streams))
        .route("/streams/:id", get(streams::get_stream))
        .route("/senders/:address", get(senders::get_by_sender))
        .route("/recipients/:address", get(recipients::get_by_recipient))
        .route("/stats", get(stats::get_stats))
        .route("/health", get(health))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}
