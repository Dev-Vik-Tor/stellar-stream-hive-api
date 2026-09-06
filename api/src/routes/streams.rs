//! `GET /streams` and `GET /streams/:id`.

use axum::{
    extract::{Path, State},
    Json,
};

use crate::error::ApiError;
use crate::models::StreamView;
use crate::state::AppState;

/// `GET /streams` -- every known stream, with live-computed `streamed_amount`,
/// `withdrawable_amount`, and `progress_percent`.
pub async fn list_streams(State(state): State<AppState>) -> Result<Json<Vec<StreamView>>, ApiError> {
    let current_ledger = state.latest_ledger();
    let streams = state.all_streams().await;

    let views = streams
        .iter()
        .map(|s| {
            s.to_view(current_ledger)
                .map_err(|e| ApiError::Internal(format!("failed to compute stream view for {}: {e}", s.id)))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(views))
}

/// `GET /streams/:id` -- full detail for one stream, 404 if unknown.
pub async fn get_stream(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<StreamView>, ApiError> {
    let stream = state
        .get_stream(&id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("stream '{id}'")))?;

    let view = stream
        .to_view(state.latest_ledger())
        .map_err(|e| ApiError::Internal(format!("failed to compute stream view for {id}: {e}")))?;

    Ok(Json(view))
}
