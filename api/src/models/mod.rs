//! Core domain types shared by the indexer and the HTTP routes.
//!
//! These mirror the on-chain shapes defined by the `stellar-stream-hive-contract`
//! Stream contract (see docs/app/docs/contracts/stream/page.mdx in the docs site
//! for the full field-by-field reference). Keeping the API's model identical to
//! the contract's `Stream` struct means the indexer can populate it directly from
//! decoded `StreamCreated` / `Withdrawn` / `Cancelled` contract events without any
//! lossy translation.

use serde::{Deserialize, Serialize};

/// A Stellar strkey address. This is a `G...` (account) or `C...` (contract)
/// strkey string. We keep it as `String` at the API boundary rather than parsing
/// it into a fixed-size key, since the API only ever echoes it back or uses it as
/// an opaque lookup key -- it never signs or submits transactions.
pub type Address = String;

/// Mirrors the contract's `StreamStatus` enum exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamStatus {
    /// Created but the start ledger has not been reached yet.
    Pending,
    /// Between start ledger (or cliff) and end ledger, funds are actively accruing.
    Active,
    /// The end ledger has passed and the stream ran to completion.
    Completed,
    /// The sender (or an authorized party, if `cancellable`) cancelled the stream
    /// before it completed. See the security-model doc for the exact fund split.
    Cancelled,
}

/// A payment stream, as recorded on-chain by the Stream contract and indexed here.
///
/// Field-for-field, this matches the contract's `Stream` struct:
/// `Stream { id, sender, recipient, token, total_amount, withdrawn, start_ledger,
/// end_ledger, cliff_ledger, cancellable, status, created_at }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stream {
    /// Monotonically increasing stream id assigned by the Registry contract.
    pub id: String,
    /// The account that funded the stream and can cancel it (if cancellable).
    pub sender: Address,
    /// The account that receives streamed funds.
    pub recipient: Address,
    /// The token contract address being streamed (e.g. the XLM SAC or a custom asset).
    pub token: Address,
    /// Total amount deposited into the stream, in the token's smallest unit (stroops
    /// for XLM). Serialized as a decimal string to avoid float/precision loss in
    /// JSON clients -- see `serde_amount`.
    #[serde(with = "serde_amount")]
    pub total_amount: i128,
    /// Amount already withdrawn by the recipient, in the token's smallest unit.
    #[serde(with = "serde_amount")]
    pub withdrawn: i128,
    /// Ledger sequence at which streaming begins.
    pub start_ledger: u32,
    /// Ledger sequence at which streaming completes.
    pub end_ledger: u32,
    /// Ledger sequence before which nothing is withdrawable, even though accrual
    /// may already have started. See the "Cliff behavior" section of the
    /// streaming-math doc.
    pub cliff_ledger: u32,
    /// Whether the sender is permitted to cancel this stream before completion.
    pub cancellable: bool,
    pub status: StreamStatus,
    /// Unix timestamp (seconds) at which the stream was created.
    pub created_at: i64,
}

/// `Stream` plus the fields the API computes on read: how much has streamed so far,
/// how much of that is still withdrawable, and a 0-100 progress percentage. This is
/// the shape returned by `GET /streams` and `GET /streams/:id`.
#[derive(Debug, Clone, Serialize)]
pub struct StreamView {
    #[serde(flatten)]
    pub stream: Stream,
    #[serde(with = "serde_amount")]
    pub streamed_amount: i128,
    #[serde(with = "serde_amount")]
    pub withdrawable_amount: i128,
    /// `streamed_amount / total_amount * 100`, rounded down, 0 when total_amount is 0.
    pub progress_percent: u8,
}

/// Aggregate figures returned by `GET /stats`. Mirrors the Registry contract's
/// `RegistryStats { total_streams, active_streams, total_streamed, total_withdrawn }`.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Stats {
    pub total_streams: u64,
    pub active_streams: u64,
    #[serde(with = "serde_amount")]
    pub total_streamed: i128,
    #[serde(with = "serde_amount")]
    pub total_withdrawn: i128,
}

/// Serializes `i128` amounts as decimal strings instead of JSON numbers.
///
/// JSON numbers are IEEE-754 doubles in every mainstream client (JavaScript
/// included), which only represent integers exactly up to 2^53. Stroop-denominated
/// i128 amounts routinely exceed that, so emitting them as numbers would silently
/// corrupt large values on the client. Encoding as a string sidesteps that entirely;
/// clients parse with `BigInt(value)` (JS/TS) or an equivalent big-integer type.
pub mod serde_amount {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(value: &i128, serializer: S) -> Result<S::Ok, S::Error> {
        value.to_string().serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i128, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse::<i128>().map_err(serde::de::Error::custom)
    }
}

impl StreamStatus {
    /// True for streams the indexer should keep recomputing accrual for on every
    /// read. Part of the public model API for consumers of this crate as a library
    /// (e.g. future batch jobs); not currently called from the HTTP routes, which
    /// recompute accrual unconditionally for simplicity.
    #[allow(dead_code)]
    pub fn is_active(self) -> bool {
        matches!(self, StreamStatus::Active | StreamStatus::Pending)
    }
}

impl Stream {
    /// Builds the read-time `StreamView`, computing `streamed_amount` fresh against
    /// `current_ledger` using the same formula as the contract (see
    /// `crate::streaming_math`). For a `Cancelled` stream, accrual is frozen: we
    /// report `total_amount - withdrawn` as already-settled rather than continuing
    /// to project accrual past the cancellation point, since the contract does not
    /// track the ledger at which cancellation happened separately from `withdrawn`.
    pub fn to_view(&self, current_ledger: u32) -> Result<StreamView, crate::streaming_math::MathError> {
        let streamed = match self.status {
            StreamStatus::Cancelled => self.withdrawn,
            _ => crate::streaming_math::streamed_amount(
                self.total_amount,
                self.start_ledger,
                self.end_ledger,
                self.cliff_ledger,
                current_ledger,
            )?,
        };
        let withdrawable = crate::streaming_math::withdrawable_amount(streamed, self.withdrawn);
        let progress = crate::streaming_math::progress_percent(streamed, self.total_amount);

        Ok(StreamView {
            stream: self.clone(),
            streamed_amount: streamed,
            withdrawable_amount: withdrawable,
            progress_percent: progress,
        })
    }
}
