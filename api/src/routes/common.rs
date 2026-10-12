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
/// - `sort`: `id` (default), `start_ledger`, `end_ledger`, `total_amount` or
///   `withdrawable_amount`.
/// - `order`: `desc` (default, so the default is newest first) or `asc`.
#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    pub status: Option<StreamStatus>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub sort: Option<SortKey>,
    pub order: Option<SortOrder>,
}

/// Field a list endpoint can be ordered by.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortKey {
    /// Numeric stream id, i.e. creation order.
    #[default]
    Id,
    StartLedger,
    EndLedger,
    TotalAmount,
    WithdrawableAmount,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    Asc,
    #[default]
    Desc,
}

fn numeric_id(view: &StreamView) -> u64 {
    view.stream.id.parse::<u64>().unwrap_or(0)
}

/// Orders by `key`, breaking ties by numeric id (then raw id) so the result is
/// deterministic and pages never overlap or skip items between requests.
fn compare(key: SortKey, a: &StreamView, b: &StreamView) -> std::cmp::Ordering {
    let primary = match key {
        SortKey::Id => numeric_id(a).cmp(&numeric_id(b)),
        SortKey::StartLedger => a.stream.start_ledger.cmp(&b.stream.start_ledger),
        SortKey::EndLedger => a.stream.end_ledger.cmp(&b.stream.end_ledger),
        SortKey::TotalAmount => a.stream.total_amount.cmp(&b.stream.total_amount),
        SortKey::WithdrawableAmount => a.withdrawable_amount.cmp(&b.withdrawable_amount),
    };
    primary
        .then_with(|| numeric_id(a).cmp(&numeric_id(b)))
        .then_with(|| a.stream.id.cmp(&b.stream.id))
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
/// orders (newest first unless `sort`/`order` say otherwise), and applies pagination.
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

    let key = params.sort.unwrap_or_default();
    match params.order.unwrap_or_default() {
        SortOrder::Asc => views.sort_by(|a, b| compare(key, a, b)),
        SortOrder::Desc => views.sort_by(|a, b| compare(key, b, a)),
    }

    let total = views.len();
    let page: Vec<StreamView> = views.into_iter().skip(offset).take(limit).collect();

    let mut headers = HeaderMap::new();
    headers.insert("x-total-count", HeaderValue::from(total));
    Ok((headers, Json(page)))
}
