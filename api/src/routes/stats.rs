//! `GET /stats` -- aggregate figures across every indexed stream.

use axum::{extract::State, Json};

use crate::error::ApiError;
use crate::models::{Stats, StreamStatus};
use crate::state::AppState;

pub async fn get_stats(State(state): State<AppState>) -> Result<Json<Stats>, ApiError> {
    let current_ledger = state.latest_ledger();
    let streams = state.all_streams().await;

    let mut stats = Stats::default();
    stats.total_streams = streams.len() as u64;

    for stream in &streams {
        if matches!(stream.status, StreamStatus::Active) {
            stats.active_streams += 1;
        }

        let view = stream
            .to_view(current_ledger)
            .map_err(|e| ApiError::Internal(format!("failed to compute stream view for {}: {e}", stream.id)))?;

        stats.total_streamed = stats
            .total_streamed
            .checked_add(view.streamed_amount)
            .ok_or_else(|| ApiError::Internal("total_streamed overflowed i128".into()))?;
        stats.total_withdrawn = stats
            .total_withdrawn
            .checked_add(stream.withdrawn)
            .ok_or_else(|| ApiError::Internal("total_withdrawn overflowed i128".into()))?;
    }

    Ok(Json(stats))
}
