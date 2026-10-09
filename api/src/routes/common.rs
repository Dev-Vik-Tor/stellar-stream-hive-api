//! Helpers shared by every route that returns a list of streams: query-string
//! parsing, address validation, filtering, ordering, and pagination.

use axum::{
    http::{HeaderMap, HeaderValue},
    Json,
};
use serde::Deserialize;

use crate::error::ApiError;
use crate::models::{Stream, StreamStatus, StreamView};

pub const DEFAULT_LIMIT: usize = 50;
pub const MAX_LIMIT: usize = 200;

/// Query-string parameters accepted by every list endpoint.
///
/// - `status`: only return streams whose *effective* status matches
///   (`pending`, `active`, `completed`, `cancelled`).
/// - `limit`: page size, 1..=200 (default 50).
/// - `offset`: number of streams to skip (default 0).
#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    pub status: Option<StreamStatus>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

impl ListParams {
    fn limit(&self) -> Result<usize, ApiError> {
        match self.limit {
            None => Ok(DEFAULT_LIMIT),
            Some(0) => Err(ApiError::BadRequest("limit must be at least 1".into())),
            Some(n) if n > MAX_LIMIT => Err(ApiError::BadRequest(format!("limit must be at most {MAX_LIMIT}"))),
            Some(n) => Ok(n),
        }
    }
}

/// Validates that `address` is a well-formed Stellar strkey (`G...` account or
/// `C...` contract address), so typos get a clear 400 instead of an empty list.
pub fn validate_address(address: &str) -> Result<(), ApiError> {
    if address.is_empty() {
        return Err(ApiError::BadRequest("address must not be empty".into()));
    }
    match stellar_strkey::Strkey::from_string(address) {
        Ok(stellar_strkey::Strkey::PublicKeyEd25519(_)) | Ok(stellar_strkey::Strkey::Contract(_)) => Ok(()),
        _ => Err(ApiError::BadRequest(format!(
            "'{address}' is not a valid Stellar account (G...) or contract (C...) address"
        ))),
    }
}

/// Builds the response for a list endpoint: computes views, filters by status,
/// orders newest-first (highest numeric id first), and applies pagination.
///
/// The body stays a plain JSON array. The size of the full filtered set is
/// reported in the `X-Total-Count` header so clients can page without a
/// breaking change to the response shape.
pub fn paginated_views(
    streams: Vec<Stream>,
    current_ledger: u32,
    params: &ListParams,
) -> Result<(HeaderMap, Json<Vec<StreamView>>), ApiError> {
    let limit = params.limit()?;
    let offset = params.offset.unwrap_or(0);

    let mut views = streams
        .iter()
        .map(|s| {
            s.to_view(current_ledger)
                .map_err(|e| ApiError::Internal(format!("failed to compute stream view for {}: {e}", s.id)))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if let Some(status) = params.status {
        views.retain(|v| v.stream.status == status);
    }

    views.sort_by(|a, b| {
        let key = |v: &StreamView| v.stream.id.parse::<u64>().unwrap_or(0);
        key(b).cmp(&key(a)).then_with(|| b.stream.id.cmp(&a.stream.id))
    });

    let total = views.len();
    let page: Vec<StreamView> = views.into_iter().skip(offset).take(limit).collect();

    let mut headers = HeaderMap::new();
    headers.insert("x-total-count", HeaderValue::from(total));
    Ok((headers, Json(page)))
}
