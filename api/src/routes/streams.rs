//! `GET /streams` and `GET /streams/:id`.

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};

use super::common::{paginated_views, ListParams};
use crate::error::ApiError;
use crate::models::StreamView;
use crate::state::AppState;

/// `GET /streams` -- every known stream, newest first, with live-computed
/// `streamed_amount`, `withdrawable_amount`, `progress_percent` and status.
/// Supports `?status=`, `?limit=` and `?offset=`.
pub async fn list_streams(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<(HeaderMap, Json<Vec<StreamView>>), ApiError> {
    let streams = state.all_streams().await;
    paginated_views(streams, state.latest_ledger(), &params)
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
