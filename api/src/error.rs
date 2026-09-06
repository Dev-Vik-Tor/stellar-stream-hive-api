//! A single typed error enum for the whole API, with an `IntoResponse` impl that
//! maps each variant to the appropriate HTTP status code and a small JSON body
//! `{ "error": "...", "message": "..." }`. Every fallible handler returns
//! `Result<T, ApiError>` so Axum's `?`-based error propagation "just works" and no
//! route needs to reach for `.unwrap()`.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{0} not found")]
    NotFound(String),

    #[error("invalid request: {0}")]
    BadRequest(String),

    // Reserved for a future synchronous "proxy to Horizon" endpoint; the current
    // routes only ever read from in-memory state populated by the background
    // indexer, so nothing constructs this yet.
    #[allow(dead_code)]
    #[error("upstream Horizon request failed: {0}")]
    UpstreamUnavailable(String),

    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
    message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, kind) = match &self {
            ApiError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            ApiError::UpstreamUnavailable(_) => (StatusCode::BAD_GATEWAY, "upstream_unavailable"),
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };

        if matches!(self, ApiError::Internal(_)) {
            tracing::error!(error = %self, "internal error while handling request");
        }

        let body = ErrorBody {
            error: kind,
            message: self.to_string(),
        };
        (status, Json(body)).into_response()
    }
}
