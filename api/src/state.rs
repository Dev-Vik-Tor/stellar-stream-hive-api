//! Shared, in-memory application state.
//!
//! v1 deliberately has no database: the indexer rebuilds its view of the world by
//! replaying Horizon from genesis-of-interest on every process start, and holds it
//! in memory behind a `RwLock` for the lifetime of the process. This is simple and
//! adequate for an indexer whose source of truth is always the chain itself; a
//! restart just means a brief re-sync. A future version can swap this out for a
//! real store without changing the route handlers, since they only ever go through
//! `AppState`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::config::Config;
use crate::models::Stream;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    /// All known streams, keyed by stream id.
    streams: Arc<RwLock<HashMap<String, Stream>>>,
    /// The most recent ledger sequence observed from Horizon. Used to compute live
    /// `streamed_amount` for active streams on every read. Starts at 0 until the
    /// indexer's first successful poll.
    latest_ledger: Arc<AtomicU32>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(config),
            streams: Arc::new(RwLock::new(HashMap::new())),
            latest_ledger: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn latest_ledger(&self) -> u32 {
        self.latest_ledger.load(Ordering::Relaxed)
    }

    pub fn set_latest_ledger(&self, ledger: u32) {
        self.latest_ledger.store(ledger, Ordering::Relaxed);
    }

    pub async fn all_streams(&self) -> Vec<Stream> {
        self.streams.read().await.values().cloned().collect()
    }

    pub async fn get_stream(&self, id: &str) -> Option<Stream> {
        self.streams.read().await.get(id).cloned()
    }

    pub async fn streams_by_sender(&self, sender: &str) -> Vec<Stream> {
        self.streams
            .read()
            .await
            .values()
            .filter(|s| s.sender == sender)
            .cloned()
            .collect()
    }

    pub async fn streams_by_recipient(&self, recipient: &str) -> Vec<Stream> {
        self.streams
            .read()
            .await
            .values()
            .filter(|s| s.recipient == recipient)
            .cloned()
            .collect()
    }

    pub async fn upsert_stream(&self, stream: Stream) {
        self.streams.write().await.insert(stream.id.clone(), stream);
    }

    /// Applies a withdrawal event: bump `withdrawn` on the matching stream, if we
    /// know about it. Silently ignored if the stream id is unknown (e.g. we started
    /// indexing after that stream was created and haven't backfilled it yet).
    pub async fn record_withdrawal(&self, stream_id: &str, amount: i128) {
        if let Some(stream) = self.streams.write().await.get_mut(stream_id) {
            stream.withdrawn = stream.withdrawn.saturating_add(amount);
        }
    }

    pub async fn record_cancellation(&self, stream_id: &str, streamed_now: i128) {
        if let Some(stream) = self.streams.write().await.get_mut(stream_id) {
            stream.status = crate::models::StreamStatus::Cancelled;
            stream.cancelled_streamed = Some(streamed_now);
        }
    }
}
