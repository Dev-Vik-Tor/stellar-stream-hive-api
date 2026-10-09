//! `GET /stats` -- aggregate figures across every indexed stream.

use axum::{extract::State, Json};

use crate::error::ApiError;
use crate::models::{Stats, StreamStatus};
use crate::state::AppState;

pub async fn get_stats(State(state): State<AppState>) -> Result<Json<Stats>, ApiError> {
    let current_ledger = state.latest_ledger();
    let streams = state.all_streams().await;

    let mut stats = Stats {
        total_streams: streams.len() as u64,
        ..Stats::default()
    };

    for stream in &streams {
        let view = stream
            .to_view(current_ledger)
            .map_err(|e| ApiError::Internal(format!("failed to compute stream view for {}: {e}", stream.id)))?;

        // Use the view's effective status: the stored status is only ever
        // Pending/Cancelled, so counting it directly would never see an active stream.
        if view.stream.status == StreamStatus::Active {
            stats.active_streams += 1;
        }

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
