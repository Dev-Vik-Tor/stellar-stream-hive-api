//! Minimal Soroban RPC (JSON-RPC) client -- just the two methods the indexer needs.
//!
//! Contract events come from `getEvents`, filtered server-side to the Stream
//! contract. (Horizon no longer returns transaction meta, so contract events
//! cannot be read from its REST API.)
//!
//! `getEvents` is addressed either by `startLedger` (first call) or by the
//! `cursor` of the last event already seen (every later call); the two are
//! mutually exclusive. Unlike Horizon's `cursor=now`, a concrete cursor never
//! skips events that land between two polls.

use serde::Deserialize;
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RpcError {
    #[error("request to Soroban RPC failed: {0}")]
    Request(String),
    #[error("Soroban RPC returned an error: {0}")]
    Rpc(String),
    #[error("failed to parse Soroban RPC response: {0}")]
    Parse(String),
}

/// One contract event as returned by `getEvents`. Topics and value are
/// base64-encoded XDR `ScVal`s.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RpcEvent {
    pub ledger: u32,
    pub topic: Vec<String>,
    pub value: String,
    pub tx_hash: String,
    /// False for events emitted inside a sub-call that was later rolled back.
    pub in_successful_contract_call: bool,
}

/// The `result` object of `getEvents`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventsPage {
    #[serde(default)]
    pub events: Vec<RpcEvent>,
    /// How far this call got. With events it is the last event's cursor; with none
    /// it is where the scan stopped. `getEvents` examines a bounded window of
    /// ledgers per call, so an empty page does NOT mean "caught up" -- keep
    /// following the cursor until it reaches `latest_ledger`.
    pub cursor: Option<String>,
    /// The chain tip as of this response.
    pub latest_ledger: u32,
}

/// The ledger a `getEvents` cursor points at. Cursors look like
/// `"<toid>-<event index>"` where the TOID's high 32 bits are the ledger.
pub fn cursor_ledger(cursor: &str) -> Option<u32> {
    let toid: u64 = cursor.split('-').next()?.parse().ok()?;
    u32::try_from(toid >> 32).ok()
}

/// Where to read events from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Position {
    /// First call: start at (and including) this ledger.
    FromLedger(u32),
    /// Later calls: everything strictly after this event cursor.
    After(String),
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    result: Option<T>,
    error: Option<RpcErrorBody>,
}

#[derive(Debug, Deserialize)]
struct RpcErrorBody {
    code: i64,
    message: String,
}

#[derive(Debug, Deserialize)]
struct LatestLedger {
    sequence: u32,
}

pub struct RpcClient {
    http: reqwest::Client,
    url: String,
}

/// Builds the JSON-RPC request body for `getEvents`.
pub fn get_events_request(contract_id: &str, position: &Position, limit: u32) -> Value {
    let mut pagination = json!({ "limit": limit });
    let mut params = json!({
        "filters": [{ "type": "contract", "contractIds": [contract_id] }],
    });
    match position {
        Position::FromLedger(ledger) => params["startLedger"] = json!(ledger),
        Position::After(cursor) => pagination["cursor"] = json!(cursor),
    }
    params["pagination"] = pagination;
    json!({ "jsonrpc": "2.0", "id": 1, "method": "getEvents", "params": params })
}

impl RpcClient {
    pub fn new(url: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            url,
        }
    }

    async fn call<T: for<'de> Deserialize<'de>>(&self, body: Value) -> Result<T, RpcError> {
        let resp = self
            .http
            .post(&self.url)
            .json(&body)
            .send()
            .await
            .map_err(|e| RpcError::Request(e.to_string()))?
            .error_for_status()
            .map_err(|e| RpcError::Request(e.to_string()))?;
        let envelope: Envelope<T> = resp.json().await.map_err(|e| RpcError::Parse(e.to_string()))?;
        if let Some(err) = envelope.error {
            return Err(RpcError::Rpc(format!("{} (code {})", err.message, err.code)));
        }
        envelope
            .result
            .ok_or_else(|| RpcError::Parse("response had neither result nor error".into()))
    }

    /// The most recently closed ledger sequence.
    pub async fn latest_ledger(&self) -> Result<u32, RpcError> {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "getLatestLedger" });
        Ok(self.call::<LatestLedger>(body).await?.sequence)
    }

    /// Events emitted by `contract_id`, oldest first, at most `limit` per call.
    pub async fn get_events(
        &self,
        contract_id: &str,
        position: &Position,
        limit: u32,
    ) -> Result<EventsPage, RpcError> {
        self.call(get_events_request(contract_id, position, limit)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTRACT: &str = "CCYM2PIMNHOJKWRV677QQNFUMNU2JKEHXTQKK6D3D7JF2UTEB5JNINYY";

    #[test]
    fn first_request_uses_start_ledger_and_no_cursor() {
        let req = get_events_request(CONTRACT, &Position::FromLedger(123), 100);
        assert_eq!(req["method"], "getEvents");
        assert_eq!(req["params"]["startLedger"], 123);
        assert!(req["params"]["pagination"].get("cursor").is_none());
        assert_eq!(req["params"]["pagination"]["limit"], 100);
        assert_eq!(req["params"]["filters"][0]["contractIds"][0], CONTRACT);
        assert_eq!(req["params"]["filters"][0]["type"], "contract");
    }

    #[test]
    fn later_requests_use_the_cursor_and_no_start_ledger() {
        let req = get_events_request(CONTRACT, &Position::After("0022-1".into()), 50);
        assert_eq!(req["params"]["pagination"]["cursor"], "0022-1");
        // startLedger and cursor are mutually exclusive in the RPC
        assert!(req["params"].get("startLedger").is_none());
    }

    #[test]
    fn parses_a_real_get_events_result() {
        let page: EventsPage = serde_json::from_str(include_str!("testdata/testnet_events.json")).unwrap();
        assert_eq!(page.events.len(), 4);
        assert_eq!(page.events[0].topic.len(), 2);
        assert!(page.events.iter().all(|e| e.in_successful_contract_call));
        assert!(page.cursor.is_some());
        assert!(page.latest_ledger > 0);
    }

    #[test]
    fn an_empty_result_has_no_cursor() {
        let page: EventsPage = serde_json::from_str(r#"{"events":[],"latestLedger":42}"#).unwrap();
        assert!(page.events.is_empty());
        assert_eq!(page.cursor, None);
        assert_eq!(page.latest_ledger, 42);
    }

    #[test]
    fn reads_the_ledger_out_of_a_cursor() {
        // an event id observed on testnet for an event emitted in ledger 5154754
        assert_eq!(cursor_ledger("0022139499848949760-0000000002"), Some(5_154_754));
        assert_eq!(cursor_ledger("0022139499848949760"), Some(5_154_754));
        assert_eq!(cursor_ledger("garbage"), None);
        assert_eq!(cursor_ledger(""), None);
    }

    #[test]
    fn surfaces_rpc_errors() {
        let envelope: Envelope<LatestLedger> =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"bad range"}}"#).unwrap();
        assert!(envelope.result.is_none());
        let err = envelope.error.unwrap();
        assert_eq!((err.code, err.message.as_str()), (-32602, "bad range"));
    }
}
