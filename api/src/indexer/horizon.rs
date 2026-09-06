//! Minimal Horizon REST client -- just the two endpoints the indexer needs.

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HorizonError {
    #[error("request to Horizon failed: {0}")]
    Request(String),
    #[error("Horizon returned an error status: {0}")]
    Status(String),
    #[error("failed to parse Horizon response: {0}")]
    Parse(String),
}

#[derive(Debug, Deserialize)]
struct RootResponse {
    history_latest_ledger: u32,
}

/// The subset of Horizon's transaction record we actually use.
#[derive(Debug, Deserialize, Clone)]
pub struct TransactionRecord {
    pub hash: String,
    pub successful: bool,
    pub ledger: u32,
    pub paging_token: String,
    pub result_meta_xdr: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddedRecords<T> {
    records: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct HalEnvelope<T> {
    #[serde(rename = "_embedded")]
    embedded: EmbeddedRecords<T>,
}

pub struct TransactionsPage {
    pub transactions: Vec<TransactionRecord>,
    /// Paging token of the last record in this page, to resume from on the next
    /// poll. `None` if the page was empty (nothing new since the last poll).
    pub next_cursor: Option<String>,
}

pub struct HorizonClient {
    http: reqwest::Client,
    base_url: String,
}

impl HorizonClient {
    pub fn new(base_url: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url,
        }
    }

    /// Fetches Horizon's most recently ingested ledger sequence, used as the
    /// "current ledger" input to the streaming-math formula for live reads.
    pub async fn fetch_latest_ledger(&self) -> Result<u32, HorizonError> {
        let resp = self
            .http
            .get(&self.base_url)
            .send()
            .await
            .map_err(|e| HorizonError::Request(e.to_string()))?;
        let resp = resp
            .error_for_status()
            .map_err(|e| HorizonError::Status(e.to_string()))?;
        let root: RootResponse = resp
            .json()
            .await
            .map_err(|e| HorizonError::Parse(e.to_string()))?;
        Ok(root.history_latest_ledger)
    }

    /// Fetches successful transactions after `cursor` (Horizon's opaque paging
    /// token), oldest first, up to `limit`. Pass `None` on the very first call to
    /// start from the current tip.
    pub async fn fetch_transactions_after(
        &self,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<TransactionsPage, HorizonError> {
        let mut url = format!(
            "{}/transactions?order=asc&limit={limit}&include_failed=false",
            self.base_url.trim_end_matches('/')
        );
        if let Some(cursor) = cursor {
            url.push_str("&cursor=");
            url.push_str(cursor);
        }

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| HorizonError::Request(e.to_string()))?;
        let resp = resp
            .error_for_status()
            .map_err(|e| HorizonError::Status(e.to_string()))?;
        let envelope: HalEnvelope<TransactionRecord> = resp
            .json()
            .await
            .map_err(|e| HorizonError::Parse(e.to_string()))?;

        let next_cursor = envelope.embedded.records.last().map(|r| r.paging_token.clone());
        Ok(TransactionsPage {
            transactions: envelope.embedded.records,
            next_cursor,
        })
    }
}
