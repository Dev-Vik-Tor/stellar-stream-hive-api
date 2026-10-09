//! `GET /recipients/:address` -- streams received by a given Stellar address.

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};

use super::common::{paginated_views, validate_address, ListParams};
use crate::error::ApiError;
use crate::models::StreamView;
use crate::state::AppState;

pub async fn get_by_recipient(
    State(state): State<AppState>,
    Path(address): Path<String>,
    Query(params): Query<ListParams>,
) -> Result<(HeaderMap, Json<Vec<StreamView>>), ApiError> {
    validate_address(&address)?;

    let streams = state.streams_by_recipient(&address).await;
    paginated_views(streams, state.latest_ledger(), &params)
}
