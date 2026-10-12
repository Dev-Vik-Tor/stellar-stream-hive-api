//! Background indexer: polls Soroban RPC on a fixed interval for the Stream
//! contract's events, decodes them, and keeps `AppState` up to date.
//!
//! This task is spawned once from `main` and runs for the life of the process.
//! It never panics and never exits on a transient error -- a failed RPC request
//! just gets logged and retried on the next tick, per the "no panics"
//! constraint on this service.
//!
//! Events are read with `getEvents`, filtered server-side to the Stream
//! contract. The first call starts `backfill_ledgers` back from the tip (so a
//! restart recovers recent activity); every later call resumes from the cursor
//! of the last event seen, so nothing that lands between two polls is missed.

pub mod events;
mod rpc;

use std::time::Duration;

use rpc::{Position, RpcClient};

use crate::state::AppState;

/// Events requested per `getEvents` call.
const PAGE_SIZE: u32 = 100;

/// Upper bound on `getEvents` calls in one tick, so catching up from a long way
/// behind can never monopolise the task; the rest resumes on the next tick.
const MAX_CALLS_PER_TICK: usize = 50;

/// Whether another `getEvents` call is needed this tick.
///
/// `getEvents` scans a bounded window of ledgers per call and returns a cursor
/// for how far it got even when it found nothing, so an empty page is not proof
/// of being caught up: keep going until the cursor reaches the chain tip. Stop
/// if there is no cursor to advance with, or if it did not move (no progress).
fn should_continue(full_page: bool, previous: &Position, cursor: Option<&str>, latest_ledger: u32) -> bool {
    let Some(cursor) = cursor else {
        return false;
    };
    if matches!(previous, Position::After(prev) if prev == cursor) {
        return false;
    }
    full_page || rpc::cursor_ledger(cursor).is_some_and(|scanned| scanned.saturating_add(2) < latest_ledger)
}

pub async fn run(state: AppState) {
    let client = RpcClient::new(state.config.soroban_rpc_url.clone());
    let mut interval = tokio::time::interval(Duration::from_secs(state.config.poll_interval_secs));

    // If `STREAM_CONTRACT_ID` is still the startup placeholder (contracts not
    // yet deployed) there is nothing to filter on, so we only track the latest
    // ledger and skip event decoding on every tick.
    let contract_id: Option<String> = state
        .config
        .has_real_contract_id()
        .then(|| state.config.stream_contract_id.clone());

    let mut position: Option<Position> = None;

    loop {
        interval.tick().await;

        let latest = match client.latest_ledger().await {
            Ok(ledger) => {
                state.set_latest_ledger(ledger);
                ledger
            }
            Err(e) => {
                tracing::warn!(error = %e, "indexer: failed to fetch latest ledger from Soroban RPC");
                continue;
            }
        };

        let Some(contract_id) = contract_id.as_deref() else {
            continue;
        };

        let mut pos = position.clone().unwrap_or_else(|| {
            Position::FromLedger(latest.saturating_sub(state.config.backfill_ledgers))
        });

        // Drain what is available this tick.
        for _ in 0..MAX_CALLS_PER_TICK {
            match client.get_events(contract_id, &pos, PAGE_SIZE).await {
                Ok(page) => {
                    let full_page = page.events.len() as u32 >= PAGE_SIZE;
                    ingest(&state, &page.events).await;

                    let keep_going = should_continue(full_page, &pos, page.cursor.as_deref(), page.latest_ledger);
                    // Always adopt the returned cursor, even for an empty page: it marks how
                    // far the scan got, and re-asking for the same window would loop forever.
                    if let Some(cursor) = page.cursor {
                        pos = Position::After(cursor);
                        position = Some(pos.clone());
                    }
                    if !keep_going {
                        break;
                    }
                }
                Err(e) => {
                    // A start ledger older than the RPC's retention window is rejected;
                    // start from the tip instead rather than retrying a request that
                    // can never succeed.
                    if matches!(pos, Position::FromLedger(start) if start < latest) && e.to_string().contains("range") {
                        tracing::warn!(error = %e, "indexer: backfill start is outside the RPC retention window, starting from the tip");
                        position = Some(Position::FromLedger(latest));
                    } else {
                        tracing::warn!(error = %e, "indexer: failed to fetch events from Soroban RPC");
                    }
                    break;
                }
            }
        }
    }
}

/// Decodes and applies a batch of events in order. Events from a sub-call that
/// was rolled back are ignored; undecodable events are logged and skipped so one
/// bad event can never stall the indexer.
async fn ingest(state: &AppState, events: &[rpc::RpcEvent]) {
    for event in events {
        if !event.in_successful_contract_call {
            continue;
        }
        match events::decode_rpc_event(&event.topic, &event.value) {
            Ok(Some(decoded)) => apply_event(state, decoded).await,
            Ok(None) => {} // not a stream lifecycle event
            Err(e) => tracing::warn!(
                tx_hash = %event.tx_hash,
                ledger = event.ledger,
                error = %e,
                "indexer: failed to decode contract event, skipping it"
            ),
        }
    }
}

async fn apply_event(state: &AppState, event: events::StreamEvent) {
    match event {
        events::StreamEvent::Created(stream) => {
            tracing::info!(stream_id = %stream.id, "indexer: new stream created");
            state.upsert_stream(stream).await;
        }
        events::StreamEvent::Withdrawn { stream_id, amount } => {
            tracing::info!(stream_id = %stream_id, amount, "indexer: withdrawal recorded");
            state.record_withdrawal(&stream_id, amount).await;
        }
        events::StreamEvent::Cancelled { stream_id, streamed_now } => {
            tracing::info!(stream_id = %stream_id, streamed_now, "indexer: stream cancelled");
            state.record_cancellation(&stream_id, streamed_now).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::models::StreamStatus;

    /// Real `getEvents` output from the testnet deployment: stream 1 was created
    /// (100 XLM-stroops = 10 XLM), partly withdrawn (24,333,333), then cancelled
    /// (streamed_now 25,000,000); stream 2 was created afterwards.
    fn fixture() -> rpc::EventsPage {
        serde_json::from_str(include_str!("testdata/testnet_events.json")).unwrap()
    }

    fn test_state() -> AppState {
        AppState::new(Config {
            soroban_rpc_url: "http://localhost".into(),
            stream_contract_id: crate::config::STREAM_CONTRACT_ID_PLACEHOLDER.into(),
            bind_addr: "127.0.0.1:0".into(),
            poll_interval_secs: 10,
            backfill_ledgers: 100,
            cors_allowed_origins: Vec::new(),
        })
    }

    fn after(cursor: &str) -> Position {
        Position::After(cursor.into())
    }

    #[test]
    fn keeps_scanning_when_an_empty_window_ends_far_behind_the_tip() {
        // The bug this guards against: an empty page used to stop the tick, so the
        // 10,000-ledger scan window never advanced to events further along.
        let first_call = Position::FromLedger(5_137_746);
        assert!(should_continue(false, &first_call, Some("0022109400718114815-4294967295"), 5_155_029));
    }

    #[test]
    fn stops_once_the_scan_has_reached_the_tip() {
        let prev = after("0022109400718114815-4294967295");
        // cursor at ledger 5,155,028 with the tip at 5,155,029
        assert!(!should_continue(false, &prev, Some("0022139499848949760-0000000002"), 5_154_754 + 1));
    }

    #[test]
    fn a_full_page_always_means_fetch_again() {
        let prev = after("0022139499848949760-0000000002");
        assert!(should_continue(true, &prev, Some("0022139933640650752-0000000002"), 5_154_855));
    }

    #[test]
    fn stops_when_there_is_no_cursor_or_no_progress() {
        let prev = after("0022139499848949760-0000000002");
        assert!(!should_continue(true, &prev, None, 9_999_999));
        // same cursor back again: nothing moved, don't spin
        assert!(!should_continue(true, &prev, Some("0022139499848949760-0000000002"), 9_999_999));
    }

    #[tokio::test]
    async fn real_testnet_events_reproduce_the_on_chain_state() {
        let state = test_state();
        ingest(&state, &fixture().events).await;

        let s1 = state.get_stream("1").await.expect("stream 1 was indexed");
        assert_eq!(s1.sender, "GBLEJOG7B6ONTCL4F3T7F5G5NVYVKLLEMTV2C6XVZ7BF2ML4TUQVVZHI");
        assert_eq!(s1.recipient, "GAYL5UGDFAIPQTNTGBRVJKWSYVLEAYIFKXEOWB4KMQW3ZXT3A7AMRF2F");
        assert_eq!(s1.token, "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC");
        assert_eq!(s1.total_amount, 100_000_000);
        assert_eq!((s1.start_ledger, s1.end_ledger, s1.cliff_ledger), (5_154_782, 5_155_082, 5_154_782));
        assert!(s1.cancellable);
        assert_eq!(s1.status, StreamStatus::Cancelled);
        assert_eq!(s1.withdrawn, 24_333_333);
        assert_eq!(s1.cancelled_streamed, Some(25_000_000));

        // The contract itself reported withdrawable_amount = 666,667 for this
        // stream after the cancel; the indexer must agree, at any later ledger.
        for ledger in [5_154_900, 5_999_999] {
            let view = s1.to_view(ledger).unwrap();
            assert_eq!(view.streamed_amount, 25_000_000, "ledger {ledger}");
            assert_eq!(view.withdrawable_amount, 666_667, "ledger {ledger}");
        }

        let s2 = state.get_stream("2").await.expect("stream 2 was indexed");
        assert_eq!(s2.total_amount, 20_000_000);
        assert_ne!(s2.status, StreamStatus::Cancelled);
        assert_eq!(state.all_streams().await.len(), 2);
    }

    #[tokio::test]
    async fn events_from_rolled_back_calls_are_ignored() {
        let state = test_state();
        let mut events = fixture().events;
        for e in &mut events {
            e.in_successful_contract_call = false;
        }
        ingest(&state, &events).await;
        assert!(state.all_streams().await.is_empty());
    }

    #[tokio::test]
    async fn a_malformed_event_is_skipped_without_stopping_the_batch() {
        let state = test_state();
        let mut events = fixture().events;
        events[0].value = "not base64 xdr".into(); // corrupts stream 1's create
        ingest(&state, &events).await;
        // stream 2's create (a later event in the same batch) is still applied
        assert!(state.get_stream("2").await.is_some());
        assert!(state.get_stream("1").await.is_none());
    }
}
