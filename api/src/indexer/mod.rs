//! Background indexer: polls Horizon on a fixed interval, decodes Soroban
//! contract events emitted by the Stream contract, and keeps `AppState` up to
//! date.
//!
//! This task is spawned once from `main` and runs for the life of the process.
//! It never panics and never exits on a transient error -- a failed Horizon
//! request just gets logged and retried on the next tick, per the "no panics"
//! constraint on this service.

pub mod events;
mod horizon;

use std::time::Duration;

use horizon::HorizonClient;

use crate::state::AppState;

/// Number of transactions requested per Horizon page. Kept well under Horizon's
/// max page size; if a single 10-second window somehow produces more than this
/// many transactions, the next tick's `cursor` picks up right where this page
/// left off, so nothing is lost -- it just arrives a poll cycle later.
const PAGE_SIZE: u32 = 100;

pub async fn run(state: AppState) {
    let client = HorizonClient::new(state.config.horizon_url.clone());
    let mut interval = tokio::time::interval(Duration::from_secs(state.config.poll_interval_secs));

    // Resolve the Stream contract's raw 32-byte id once, up front. If
    // `STREAM_CONTRACT_ID` is still the startup placeholder (contracts not yet
    // deployed), this is `None` and we simply skip event decoding on every tick
    // below -- the indexer still runs and still tracks `latest_ledger`, it just has
    // nothing to match events against yet.
    let contract_filter: Option<[u8; 32]> = if state.config.has_real_contract_id() {
        match stellar_strkey::Contract::from_string(&state.config.stream_contract_id) {
            Ok(c) => Some(c.0),
            Err(e) => {
                tracing::error!(
                    stream_contract_id = %state.config.stream_contract_id,
                    error = ?e,
                    "STREAM_CONTRACT_ID failed strkey validation despite passing has_real_contract_id(); \
                     this should not happen -- disabling event decoding for this run"
                );
                None
            }
        }
    } else {
        None
    };

    // Start from "now" rather than replaying Horizon's full transaction history on
    // every process start -- v1 is an in-memory indexer with no backfill story, so
    // there is no point paying the cost of walking history we'd discard anyway.
    let mut cursor: Option<String> = Some("now".to_string());

    loop {
        interval.tick().await;

        match client.fetch_latest_ledger().await {
            Ok(ledger) => state.set_latest_ledger(ledger),
            Err(e) => tracing::warn!(error = %e, "indexer: failed to fetch latest ledger from Horizon"),
        }

        let Some(contract_filter) = contract_filter else {
            continue;
        };

        match client.fetch_transactions_after(cursor.as_deref(), PAGE_SIZE).await {
            Ok(page) => {
                for tx in &page.transactions {
                    if !tx.successful {
                        continue;
                    }
                    match events::decode_stream_events(&tx.result_meta_xdr, &contract_filter) {
                        Ok(decoded) => {
                            for event in decoded {
                                apply_event(&state, event).await;
                            }
                        }
                        Err(e) => tracing::warn!(
                            tx_hash = %tx.hash,
                            ledger = tx.ledger,
                            error = %e,
                            "indexer: failed to decode transaction meta, skipping this transaction"
                        ),
                    }
                }
                if page.next_cursor.is_some() {
                    cursor = page.next_cursor;
                }
            }
            Err(e) => tracing::warn!(error = %e, "indexer: failed to fetch transactions from Horizon"),
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
