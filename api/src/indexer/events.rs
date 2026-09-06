//! Decodes Soroban contract events (`StreamCreated`, `Withdrawn`, `Cancelled`) out
//! of a transaction's XDR result meta.
//!
//! # Event wire format
//!
//! This is the contract between the Stream contract and this indexer -- it is
//! reproduced in full in the Stream contract reference
//! (`docs/app/docs/contracts/stream/page.mdx`, "Events emitted"):
//!
//! | Event           | Topics                              | Data                                                                                                    |
//! |-----------------|--------------------------------------|-----------------------------------------------------------------------------------------------------------|
//! | `StreamCreated`  | `[Symbol("StreamCreated"), U64(id)]` | `Vec[Address(sender), Address(recipient), Address(token), I128(total_amount), U32(start_ledger), U32(end_ledger), U32(cliff_ledger), Bool(cancellable)]` |
//! | `Withdrawn`      | `[Symbol("Withdrawn"), U64(id)]`     | `I128(amount_withdrawn_this_call)`                                                                          |
//! | `Cancelled`      | `[Symbol("Cancelled"), U64(id)]`     | `I128(sender_refund_amount)`                                                                                |
//!
//! Any transaction meta version we don't recognize, or any contract event that
//! doesn't match this shape, is skipped with a warning rather than treated as a
//! fatal error -- a single malformed or unrelated event must never take down the
//! indexer loop.

use stellar_xdr::curr::{ContractEvent, ContractEventBody, Limits, ReadXdr, ScAddress, ScVal, TransactionMeta};
use thiserror::Error;

use crate::models::{Stream, StreamStatus};

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("failed to parse transaction result meta XDR: {0}")]
    Xdr(String),
    #[error("malformed contract event: {0}")]
    Malformed(String),
}

/// One decoded stream-lifecycle event, ready to be applied to `AppState`.
#[derive(Debug)]
pub enum StreamEvent {
    Created(Stream),
    Withdrawn { stream_id: String, amount: i128 },
    Cancelled { stream_id: String },
}

/// Decodes every `StreamCreated` / `Withdrawn` / `Cancelled` event emitted by
/// `contract_filter` (the Stream contract's 32-byte contract id) within one
/// transaction's base64 `result_meta_xdr`.
pub fn decode_stream_events(
    meta_xdr_b64: &str,
    contract_filter: &[u8; 32],
) -> Result<Vec<StreamEvent>, DecodeError> {
    let meta = TransactionMeta::from_xdr_base64(meta_xdr_b64, Limits::none())
        .map_err(|e| DecodeError::Xdr(e.to_string()))?;

    let raw_events: Vec<ContractEvent> = match meta {
        // Protocol <= ~21: Soroban events live under `soroban_meta.events`.
        TransactionMeta::V3(v3) => v3
            .soroban_meta
            .map(|sm| sm.events.iter().cloned().collect())
            .unwrap_or_default(),
        // Protocol >= ~22: events moved to a top-level, stage-tagged list.
        TransactionMeta::V4(v4) => v4.events.iter().map(|te| te.event.clone()).collect(),
        // V0/V1/V2 predate Soroban entirely -- no contract events possible.
        TransactionMeta::V0(_) | TransactionMeta::V1(_) | TransactionMeta::V2(_) => Vec::new(),
    };

    let mut out = Vec::new();
    for event in &raw_events {
        let Some(contract_id) = &event.contract_id else {
            continue;
        };
        if contract_id.0 .0 != *contract_filter {
            continue;
        }
        match parse_event(event) {
            Ok(Some(parsed)) => out.push(parsed),
            Ok(None) => { /* not a stream lifecycle event (or a diagnostic-only event) -- ignore */ }
            Err(e) => tracing::warn!(error = %e, "skipping malformed stream contract event"),
        }
    }
    Ok(out)
}

fn parse_event(event: &ContractEvent) -> Result<Option<StreamEvent>, DecodeError> {
    let ContractEventBody::V0(body) = &event.body;

    let Some(ScVal::Symbol(name)) = body.topics.first() else {
        return Ok(None);
    };
    let name = name.0.to_string();

    let stream_id = match body.topics.get(1) {
        Some(v) => sc_val_to_stream_id(v)?,
        None => return Err(DecodeError::Malformed(format!("{name} event missing stream id topic"))),
    };

    match name.as_str() {
        "StreamCreated" => {
            let fields = sc_val_as_vec(&body.data)?;
            if fields.len() != 8 {
                return Err(DecodeError::Malformed(format!(
                    "StreamCreated data expected 8 fields, got {}",
                    fields.len()
                )));
            }
            let stream = Stream {
                id: stream_id,
                sender: sc_val_as_address(&fields[0])?,
                recipient: sc_val_as_address(&fields[1])?,
                token: sc_val_as_address(&fields[2])?,
                total_amount: sc_val_as_i128(&fields[3])?,
                withdrawn: 0,
                start_ledger: sc_val_as_u32(&fields[4])?,
                end_ledger: sc_val_as_u32(&fields[5])?,
                cliff_ledger: sc_val_as_u32(&fields[6])?,
                cancellable: sc_val_as_bool(&fields[7])?,
                status: StreamStatus::Pending,
                created_at: chrono::Utc::now().timestamp(),
            };
            Ok(Some(StreamEvent::Created(stream)))
        }
        "Withdrawn" => Ok(Some(StreamEvent::Withdrawn {
            stream_id,
            amount: sc_val_as_i128(&body.data)?,
        })),
        "Cancelled" => Ok(Some(StreamEvent::Cancelled { stream_id })),
        _ => Ok(None),
    }
}

fn sc_val_to_stream_id(value: &ScVal) -> Result<String, DecodeError> {
    match value {
        ScVal::U64(id) => Ok(id.to_string()),
        other => Err(DecodeError::Malformed(format!("expected U64 stream id topic, got {other:?}"))),
    }
}

fn sc_val_as_vec(value: &ScVal) -> Result<Vec<ScVal>, DecodeError> {
    match value {
        ScVal::Vec(Some(v)) => Ok(v.iter().cloned().collect()),
        other => Err(DecodeError::Malformed(format!("expected Vec, got {other:?}"))),
    }
}

fn sc_val_as_i128(value: &ScVal) -> Result<i128, DecodeError> {
    match value {
        ScVal::I128(parts) => Ok((i128::from(parts.hi) << 64) | i128::from(parts.lo)),
        other => Err(DecodeError::Malformed(format!("expected I128, got {other:?}"))),
    }
}

fn sc_val_as_u32(value: &ScVal) -> Result<u32, DecodeError> {
    match value {
        ScVal::U32(v) => Ok(*v),
        other => Err(DecodeError::Malformed(format!("expected U32, got {other:?}"))),
    }
}

fn sc_val_as_bool(value: &ScVal) -> Result<bool, DecodeError> {
    match value {
        ScVal::Bool(v) => Ok(*v),
        other => Err(DecodeError::Malformed(format!("expected Bool, got {other:?}"))),
    }
}

fn sc_val_as_address(value: &ScVal) -> Result<String, DecodeError> {
    match value {
        ScVal::Address(addr) => sc_address_to_string(addr),
        other => Err(DecodeError::Malformed(format!("expected Address, got {other:?}"))),
    }
}

/// Converts a decoded `ScAddress` into its `G...` / `C...` strkey representation.
fn sc_address_to_string(addr: &ScAddress) -> Result<String, DecodeError> {
    match addr {
        ScAddress::Account(account_id) => {
            let stellar_xdr::curr::PublicKey::PublicKeyTypeEd25519(uint256) = &account_id.0;
            // `stellar_strkey::ed25519::PublicKey` has an inherent `to_string()` that
            // returns a fixed-capacity `heapless::String`, not `std::String` -- go
            // through the `Display` impl explicitly via `format!` to get a real owned
            // `String` for the API/model layer.
            Ok(format!("{}", stellar_strkey::ed25519::PublicKey(uint256.0)))
        }
        ScAddress::Contract(contract_id) => Ok(format!("{}", stellar_strkey::Contract(contract_id.0 .0))),
        other => Err(DecodeError::Malformed(format!("unsupported address type: {other:?}"))),
    }
}
