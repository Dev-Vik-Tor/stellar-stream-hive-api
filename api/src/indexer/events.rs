//! Decodes Soroban contract events (`create`, `withdraw`, `cancel`) out
//! of Soroban RPC `getEvents` results.
//!
//! # Event wire format
//!
//! This is the contract between the Stream contract and this indexer. The
//! contract publishes typed `#[contractevent(data_format = "vec")]` events, so
//! the topic is the snake_case struct name followed by the `#[topic]` fields,
//! and the data is a `Vec` of the remaining fields in declaration order:
//!
//! | Event      | Topics                          | Data (`Vec`)                                                                                          |
//! |------------|---------------------------------|--------------------------------------------------------------------------------------------------------|
//! | `create`   | `[Symbol("create"), U64(id)]`   | `[sender, recipient, token, total_amount: I128, start: U32, end: U32, cliff: U32, cancellable: Bool]` |
//! | `withdraw` | `[Symbol("withdraw"), U64(id)]` | `[recipient, amount: I128]`                                                                           |
//! | `cancel`   | `[Symbol("cancel"), U64(id)]`   | `[sender, streamed_now: I128, remainder: I128]`                                                       |
//!
//! Any transaction meta version we don't recognize, or any contract event that
//! doesn't match this shape, is skipped with a warning rather than treated as a
//! fatal error -- a single malformed or unrelated event must never take down the
//! indexer loop.

use stellar_xdr::curr::{Limits, ReadXdr, ScAddress, ScVal};
#[cfg(test)]
use stellar_xdr::curr::{ContractEvent, ContractEventBody};
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
    Cancelled {
        stream_id: String,
        /// Amount that had streamed at the moment of cancellation; the recipient
        /// keeps (and may still withdraw) this much.
        streamed_now: i128,
    },
}

/// Decodes one event as returned by Soroban RPC `getEvents`: base64 XDR topics
/// and value. Returns `Ok(None)` for events that are not `create` / `withdraw` /
/// `cancel` lifecycle events. The caller filters by contract id (the RPC already
/// does so server-side).
pub fn decode_rpc_event(topics_b64: &[String], value_b64: &str) -> Result<Option<StreamEvent>, DecodeError> {
    let topics = topics_b64
        .iter()
        .map(|t| ScVal::from_xdr_base64(t, Limits::none()).map_err(|e| DecodeError::Xdr(e.to_string())))
        .collect::<Result<Vec<_>, _>>()?;
    let data = ScVal::from_xdr_base64(value_b64, Limits::none()).map_err(|e| DecodeError::Xdr(e.to_string()))?;
    parse_parts(&topics, &data)
}

#[cfg(test)]
fn parse_event(event: &ContractEvent) -> Result<Option<StreamEvent>, DecodeError> {
    let ContractEventBody::V0(body) = &event.body;
    parse_parts(body.topics.as_slice(), &body.data)
}

fn parse_parts(topics: &[ScVal], data: &ScVal) -> Result<Option<StreamEvent>, DecodeError> {
    let Some(ScVal::Symbol(name)) = topics.first() else {
        return Ok(None);
    };
    let name = name.0.to_string();

    // The contract also emits events with no stream id (`pause`, `unpause`,
    // `admin_transfer`), and token-style events may appear too. Only the three
    // lifecycle events carry a stream id, so don't demand one for anything else.
    if !matches!(name.as_str(), "create" | "withdraw" | "cancel") {
        return Ok(None);
    }

    let stream_id = match topics.get(1) {
        Some(v) => sc_val_to_stream_id(v)?,
        None => return Err(DecodeError::Malformed(format!("{name} event missing stream id topic"))),
    };

    match name.as_str() {
        "create" => {
            let fields = sc_val_as_vec(data)?;
            if fields.len() != 8 {
                return Err(DecodeError::Malformed(format!(
                    "create data expected 8 fields, got {}",
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
                cancelled_streamed: None,
            };
            Ok(Some(StreamEvent::Created(stream)))
        }
        "withdraw" => {
            let fields = sc_val_as_vec(data)?;
            if fields.len() != 2 {
                return Err(DecodeError::Malformed(format!(
                    "withdraw data expected 2 fields, got {}",
                    fields.len()
                )));
            }
            Ok(Some(StreamEvent::Withdrawn {
                stream_id,
                amount: sc_val_as_i128(&fields[1])?,
            }))
        }
        "cancel" => {
            let fields = sc_val_as_vec(data)?;
            if fields.len() != 3 {
                return Err(DecodeError::Malformed(format!(
                    "cancel data expected 3 fields, got {}",
                    fields.len()
                )));
            }
            Ok(Some(StreamEvent::Cancelled {
                stream_id,
                streamed_now: sc_val_as_i128(&fields[1])?,
            }))
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::curr::{
        AccountId, ContractEventType, ContractEventV0, ExtensionPoint, Hash, Int128Parts, PublicKey, ScSymbol,
        ScVec, StringM, Uint256, ContractId,
    };

    const CONTRACT: [u8; 32] = [7; 32];

    fn symbol(s: &str) -> ScVal {
        ScVal::Symbol(ScSymbol(StringM::try_from(s).unwrap()))
    }

    fn account(byte: u8) -> ScVal {
        ScVal::Address(ScAddress::Account(AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([byte; 32])))))
    }

    fn i128_val(v: i128) -> ScVal {
        ScVal::I128(Int128Parts { hi: (v >> 64) as i64, lo: v as u64 })
    }

    fn event(contract: Option<[u8; 32]>, topics: Vec<ScVal>, data: ScVal) -> ContractEvent {
        ContractEvent {
            ext: ExtensionPoint::V0,
            contract_id: contract.map(|c| ContractId(Hash(c))),
            type_: ContractEventType::Contract,
            body: ContractEventBody::V0(ContractEventV0 {
                topics: topics.try_into().unwrap(),
                data,
            }),
        }
    }

    fn vec_val(fields: Vec<ScVal>) -> ScVal {
        ScVal::Vec(Some(ScVec(fields.try_into().unwrap())))
    }

    fn created_data(total: i128) -> ScVal {
        let fields: Vec<ScVal> = vec![
            account(1),
            account(2),
            ScVal::Address(ScAddress::Contract(ContractId(Hash([9; 32])))),
            i128_val(total),
            ScVal::U32(100),
            ScVal::U32(200),
            ScVal::U32(120),
            ScVal::Bool(true),
        ];
        ScVal::Vec(Some(ScVec(fields.try_into().unwrap())))
    }

    #[test]
    fn decodes_stream_created() {
        let e = event(Some(CONTRACT), vec![symbol("create"), ScVal::U64(42)], created_data(5_000));
        let Some(StreamEvent::Created(s)) = parse_event(&e).unwrap() else { panic!("expected Created") };
        assert_eq!(s.id, "42");
        assert_eq!(s.total_amount, 5_000);
        assert_eq!((s.start_ledger, s.end_ledger, s.cliff_ledger), (100, 200, 120));
        assert!(s.cancellable);
        assert_eq!(s.withdrawn, 0);
        assert!(s.sender.starts_with('G') && s.recipient.starts_with('G'));
        assert_ne!(s.sender, s.recipient);
        assert!(s.token.starts_with('C'));
    }

    #[test]
    fn decodes_withdrawn_and_cancelled() {
        let w = event(
            Some(CONTRACT),
            vec![symbol("withdraw"), ScVal::U64(3)],
            vec_val(vec![account(2), i128_val(250)]),
        );
        let Some(StreamEvent::Withdrawn { stream_id, amount }) = parse_event(&w).unwrap() else {
            panic!("expected Withdrawn")
        };
        assert_eq!((stream_id.as_str(), amount), ("3", 250));

        let c = event(
            Some(CONTRACT),
            vec![symbol("cancel"), ScVal::U64(3)],
            vec_val(vec![account(1), i128_val(400), i128_val(600)]),
        );
        assert!(matches!(
            parse_event(&c).unwrap(),
            Some(StreamEvent::Cancelled { stream_id, streamed_now: 400 }) if stream_id == "3"
        ));
    }

    #[test]
    fn i128_round_trips_values_above_u64() {
        let big: i128 = 1_i128 << 80;
        assert_eq!(sc_val_as_i128(&i128_val(big)).unwrap(), big);
        assert_eq!(sc_val_as_i128(&i128_val(0)).unwrap(), 0);
    }

    #[test]
    fn unknown_events_are_ignored_and_bad_shapes_rejected() {
        let other = event(Some(CONTRACT), vec![symbol("SomethingElse"), ScVal::U64(1)], ScVal::Void);
        assert!(parse_event(&other).unwrap().is_none());

        let no_topics = event(Some(CONTRACT), vec![], ScVal::Void);
        assert!(parse_event(&no_topics).unwrap().is_none());

        let missing_id = event(
            Some(CONTRACT),
            vec![symbol("withdraw")],
            vec_val(vec![account(2), i128_val(1)]),
        );
        assert!(parse_event(&missing_id).is_err());

        let bad_id = event(
            Some(CONTRACT),
            vec![symbol("withdraw"), ScVal::Bool(true)],
            vec_val(vec![account(2), i128_val(1)]),
        );
        assert!(parse_event(&bad_id).is_err());

        let wrong_arity = event(Some(CONTRACT), vec![symbol("withdraw"), ScVal::U64(1)], i128_val(1));
        assert!(parse_event(&wrong_arity).is_err());
        let wrong_arity = event(Some(CONTRACT), vec![symbol("cancel"), ScVal::U64(1)], vec_val(vec![account(1)]));
        assert!(parse_event(&wrong_arity).is_err());

        let short = event(
            Some(CONTRACT),
            vec![symbol("create"), ScVal::U64(1)],
            ScVal::Vec(Some(ScVec(vec![ScVal::U32(1)].try_into().unwrap()))),
        );
        assert!(parse_event(&short).is_err());
    }

    #[test]
    fn invalid_rpc_xdr_is_an_error_not_a_panic() {
        let good_topic = "AAAADwAAAAZjcmVhdGUAAA==".to_string(); // Symbol("create")
        assert!(matches!(decode_rpc_event(&["not base64!!".into()], "AAAA"), Err(DecodeError::Xdr(_))));
        assert!(matches!(decode_rpc_event(std::slice::from_ref(&good_topic), "AAAA"), Err(DecodeError::Xdr(_))));
        assert!(matches!(decode_rpc_event(&[good_topic], "also not xdr"), Err(DecodeError::Xdr(_))));
    }

    #[test]
    fn rpc_events_without_a_known_name_are_ignored() {
        use stellar_xdr::curr::WriteXdr;
        // a Symbol("transfer") topic: some other contract-style event
        let topic = ScVal::Symbol("transfer".try_into().unwrap()).to_xdr_base64(Limits::none()).unwrap();
        let value = ScVal::U32(1).to_xdr_base64(Limits::none()).unwrap();
        assert!(decode_rpc_event(&[topic], &value).unwrap().is_none());
        assert!(decode_rpc_event(&[], &value).unwrap().is_none());
    }

    #[test]
    fn admin_events_without_a_stream_id_are_ignored_not_errors() {
        use stellar_xdr::curr::WriteXdr;
        // `pause` / `unpause` / `admin_transfer` have a single topic and no stream id.
        for name in ["pause", "unpause", "admin_transfer"] {
            let topic = ScVal::Symbol(name.try_into().unwrap()).to_xdr_base64(Limits::none()).unwrap();
            let value = ScVal::U32(1).to_xdr_base64(Limits::none()).unwrap();
            assert!(decode_rpc_event(&[topic], &value).unwrap().is_none(), "{name}");
        }
    }
}
