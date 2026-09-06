//! `GET /recipients/:address` -- streams received by a given Stellar address.

use axum::{
    extract::{Path, State},
    Json,
};

use crate::error::ApiError;
use crate::models::StreamView;
use crate::state::AppState;

pub async fn get_by_recipient(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<Vec<StreamView>>, ApiError> {
    if address.is_empty() {
        return Err(ApiError::BadRequest("address must not be empty".into()));
    }

    let current_ledger = state.latest_ledger();
    let streams = state.streams_by_recipient(&address).await;

    let views = streams
        .iter()
        .map(|s| {
            s.to_view(current_ledger)
                .map_err(|e| ApiError::Internal(format!("failed to compute stream view for {}: {e}", s.id)))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(views))
}
