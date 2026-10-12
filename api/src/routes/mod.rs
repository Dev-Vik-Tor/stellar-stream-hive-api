mod common;
pub mod recipients;
pub mod senders;
pub mod stats;
pub mod streams;

use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/streams", get(streams::list_streams))
        .route("/streams/:id", get(streams::get_stream))
        .route("/senders/:address", get(senders::get_by_sender))
        .route("/recipients/:address", get(recipients::get_by_recipient))
        .route("/stats", get(stats::get_stats))
        .route("/health", get(health))
        .route("/ready", get(ready))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

#[derive(Serialize)]
struct Readiness {
    /// True once the indexer has observed a ledger from Horizon.
    synced: bool,
    latest_ledger: u32,
    /// True when `STREAM_CONTRACT_ID` is a real contract address, i.e. the
    /// indexer can actually decode stream events.
    contract_configured: bool,
    indexed_streams: usize,
}

/// `GET /ready` -- unlike `/health` (process is up), reports whether the indexer
/// has caught up enough to serve meaningful data. Always `200`; inspect the body.
async fn ready(State(state): State<AppState>) -> Json<Readiness> {
    let latest_ledger = state.latest_ledger();
    Json(Readiness {
        synced: latest_ledger > 0,
        latest_ledger,
        contract_configured: state.config.has_real_contract_id(),
        indexed_streams: state.all_streams().await.len(),
    })
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use super::*;
    use crate::config::Config;
    use crate::models::{test_support::sample_stream, StreamStatus};

    const SENDER: &str = "GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN7";
    const RECIPIENT: &str = "GBRPYHIL2CI3FNQ4BXLFMNDLFJUNPU2HY3ZMFSHONUCEOASW7QC7OX2H";

    fn test_config() -> Config {
        Config {
            horizon_url: "http://localhost".into(),
            stream_contract_id: crate::config::STREAM_CONTRACT_ID_PLACEHOLDER.into(),
            bind_addr: "127.0.0.1:0".into(),
            poll_interval_secs: 10,
            cors_allowed_origins: Vec::new(),
        }
    }

    async fn app_with(ledger: u32, streams: Vec<crate::models::Stream>) -> Router {
        let state = AppState::new(test_config());
        state.set_latest_ledger(ledger);
        for s in streams {
            state.upsert_stream(s).await;
        }
        router(state)
    }

    async fn get_json(app: Router, uri: &str) -> (StatusCode, axum::http::HeaderMap, serde_json::Value) {
        let res = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, headers, json)
    }

    fn streams() -> Vec<crate::models::Stream> {
        let mut a = sample_stream("1");
        a.start_ledger = 100;
        a.end_ledger = 200;
        let mut b = sample_stream("2");
        b.start_ledger = 100;
        b.end_ledger = 1_000;
        let mut c = sample_stream("10");
        c.start_ledger = 100;
        c.end_ledger = 1_000;
        c.status = StreamStatus::Cancelled;
        c.recipient = SENDER.to_string();
        c.sender = RECIPIENT.to_string();
        vec![a, b, c]
    }

    #[tokio::test]
    async fn health_is_ok() {
        let (status, _, _) = get_json(app_with(0, vec![]).await, "/health").await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn ready_reports_sync_state() {
        let (_, _, body) = get_json(app_with(0, vec![]).await, "/ready").await;
        assert_eq!(body["synced"], false);
        assert_eq!(body["contract_configured"], false);

        let (_, _, body) = get_json(app_with(500, streams()).await, "/ready").await;
        assert_eq!(body["synced"], true);
        assert_eq!(body["latest_ledger"], 500);
        assert_eq!(body["indexed_streams"], 3);
    }

    #[tokio::test]
    async fn streams_are_newest_first_with_total_header() {
        let (status, headers, body) = get_json(app_with(150, streams()).await, "/streams").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers["x-total-count"], "3");
        let ids: Vec<_> = body.as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap()).collect();
        // numeric ordering: 10 > 2 > 1 (not lexicographic)
        assert_eq!(ids, ["10", "2", "1"]);
    }

    async fn ids(app: Router, uri: &str) -> Vec<String> {
        let (status, _, body) = get_json(app, uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        body.as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap().to_string()).collect()
    }

    #[tokio::test]
    async fn order_asc_by_id_is_numeric_not_lexicographic() {
        let app = app_with(150, streams()).await;
        assert_eq!(ids(app, "/streams?order=asc").await, ["1", "2", "10"]);
    }

    #[tokio::test]
    async fn sorts_by_ledger_fields_with_id_as_tiebreaker() {
        // end ledgers: 1 -> 200, 2 -> 1000, 10 -> 1000
        let app = app_with(150, streams()).await;
        assert_eq!(ids(app.clone(), "/streams?sort=end_ledger&order=asc").await, ["1", "2", "10"]);
        assert_eq!(ids(app, "/streams?sort=end_ledger&order=desc").await, ["10", "2", "1"]);
    }

    #[tokio::test]
    async fn sorts_by_amounts() {
        let mut a = sample_stream("1");
        a.total_amount = 500;
        let mut b = sample_stream("2");
        b.total_amount = 9_000;
        let mut c = sample_stream("3");
        c.total_amount = 70;
        let app = app_with(150, vec![a, b, c]).await;
        assert_eq!(ids(app.clone(), "/streams?sort=total_amount").await, ["2", "1", "3"]);
        assert_eq!(ids(app, "/streams?sort=total_amount&order=asc").await, ["3", "1", "2"]);
    }

    #[tokio::test]
    async fn sorts_by_withdrawable_amount_to_surface_what_can_be_claimed() {
        // identical totals, different schedules: the one furthest along has accrued most
        let mut early = sample_stream("1");
        early.start_ledger = 100;
        early.end_ledger = 200;
        let mut late = sample_stream("2");
        late.start_ledger = 100;
        late.end_ledger = 10_000;
        let app = app_with(150, vec![early, late]).await;
        assert_eq!(ids(app, "/streams?sort=withdrawable_amount").await, ["1", "2"]);
    }

    #[tokio::test]
    async fn pagination_applies_after_sorting_and_filtering() {
        let app = app_with(150, streams()).await;
        // active streams only (1 and 2), ordered by end ledger descending, second page
        assert_eq!(
            ids(app, "/streams?status=active&sort=end_ledger&order=desc&limit=1&offset=1").await,
            ["1"]
        );
    }

    #[tokio::test]
    async fn sort_applies_to_sender_and_recipient_lists_too() {
        let app = app_with(150, streams()).await;
        let uri = format!("/senders/{SENDER}?order=asc");
        let got = ids(app, &uri).await;
        let mut expected = got.clone();
        expected.sort_by_key(|id| id.parse::<u64>().unwrap());
        assert_eq!(got, expected);
    }

    #[tokio::test]
    async fn unknown_sort_or_order_is_rejected() {
        for uri in ["/streams?sort=nope", "/streams?order=sideways"] {
            let (status, _, _) = get_json(app_with(150, streams()).await, uri).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        }
    }

    #[tokio::test]
    async fn status_filter_uses_effective_status() {
        let (_, headers, body) = get_json(app_with(250, streams()).await, "/streams?status=completed").await;
        // stream 1 ended at 200 -> completed; 2 is still active; 10 is cancelled
        assert_eq!(headers["x-total-count"], "1");
        assert_eq!(body[0]["id"], "1");

        let (_, _, body) = get_json(app_with(250, streams()).await, "/streams?status=active").await;
        assert_eq!(body.as_array().unwrap().len(), 1);
        assert_eq!(body[0]["id"], "2");
    }

    #[tokio::test]
    async fn pagination_limits_and_offsets() {
        let (_, headers, body) = get_json(app_with(150, streams()).await, "/streams?limit=1&offset=1").await;
        assert_eq!(headers["x-total-count"], "3");
        assert_eq!(body.as_array().unwrap().len(), 1);
        assert_eq!(body[0]["id"], "2");
    }

    #[tokio::test]
    async fn invalid_pagination_is_rejected() {
        let (status, _, _) = get_json(app_with(0, vec![]).await, "/streams?limit=0").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _, _) = get_json(app_with(0, vec![]).await, "/streams?limit=9999").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _, _) = get_json(app_with(0, vec![]).await, "/streams?status=bogus").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn stream_detail_and_not_found() {
        let (status, _, body) = get_json(app_with(150, streams()).await, "/streams/2").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "active");

        let (status, _, body) = get_json(app_with(150, streams()).await, "/streams/999").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "not_found");
    }

    #[tokio::test]
    async fn sender_and_recipient_lookups_filter_correctly() {
        let (status, _, body) = get_json(app_with(150, streams()).await, &format!("/senders/{SENDER}")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_array().unwrap().len(), 2);

        let (_, _, body) = get_json(app_with(150, streams()).await, &format!("/recipients/{SENDER}")).await;
        assert_eq!(body.as_array().unwrap().len(), 1);
        assert_eq!(body[0]["id"], "10");
    }

    #[tokio::test]
    async fn malformed_address_is_a_bad_request() {
        let (status, _, body) = get_json(app_with(0, vec![]).await, "/senders/not-an-address").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "bad_request");
    }

    #[tokio::test]
    async fn stats_count_active_streams_by_effective_status() {
        let (status, _, body) = get_json(app_with(150, streams()).await, "/stats").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["total_streams"], 3);
        // streams 1 and 2 are active at ledger 150; 10 is cancelled
        assert_eq!(body["active_streams"], 2);
    }

    #[tokio::test]
    async fn stats_report_value_locked_net_of_withdrawals_and_refunds() {
        let mut live = sample_stream("1");
        live.withdrawn = 300; // 1000 deposited, 300 already paid out -> 700 locked
        let mut cancelled = sample_stream("2");
        cancelled.status = StreamStatus::Cancelled;
        cancelled.cancelled_streamed = Some(400);
        cancelled.withdrawn = 100; // recipient keeps 400, took 100 -> 300 still claimable

        let (_, _, body) = get_json(app_with(150, vec![live, cancelled]).await, "/stats").await;
        assert_eq!(body["total_value_locked"], "1000");
        assert_eq!(body["total_withdrawn"], "400");
    }
}
