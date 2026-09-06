//! The linear-accrual math shared by the Stream contract and this indexer.
//!
//! This is a Rust re-implementation of the exact formula documented in
//! `docs/app/docs/concepts/streaming-math/page.mdx`. Keeping the derivation and the
//! worked example in one place (the docs) and the implementation here means the two
//! are easy to diff against each other; if you change one, check the other.
//!
//! Formula:
//! ```text
//! elapsed         = min(current_ledger, end_ledger) - start_ledger   (clamped to >= 0)
//! total_duration  = end_ledger - start_ledger
//! streamed_amount = (total_amount * elapsed) / total_duration        (multiply before divide)
//! withdrawable    = streamed_amount - withdrawn
//! ```
//!
//! All arithmetic uses checked i128 operations. Multiplication happens strictly
//! before division so that fractional stroops are only lost once, at the very end
//! of the computation -- see the "why multiply-before-divide" section of the docs
//! for what goes wrong if you divide first.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MathError {
    #[error("total_amount * elapsed overflowed i128")]
    Overflow,
    #[error("end_ledger must be greater than start_ledger")]
    InvalidDuration,
}

/// Computes the total amount streamed so far, given the stream's schedule and the
/// current ledger sequence.
///
/// Edge cases (see the streaming-math doc for the full table):
/// - `current_ledger < cliff_ledger`: returns 0 -- nothing has vested yet.
/// - `current_ledger < start_ledger` (and no cliff, or cliff <= start): returns 0.
/// - `current_ledger >= end_ledger`: returns `total_amount` exactly (elapsed is
///   clamped to `total_duration`, so streamed == total_amount, never more).
/// - `start_ledger == end_ledger` (zero duration): the stream is considered to vest
///   entirely the instant it starts, so this returns `total_amount` once
///   `current_ledger >= start_ledger`, and 0 before that. Contracts should reject
///   zero-duration streams at creation time (see `create_stream` in the contract
///   reference); this branch exists purely so the math never divides by zero.
pub fn streamed_amount(
    total_amount: i128,
    start_ledger: u32,
    end_ledger: u32,
    cliff_ledger: u32,
    current_ledger: u32,
) -> Result<i128, MathError> {
    if current_ledger < cliff_ledger {
        return Ok(0);
    }
    if current_ledger < start_ledger {
        return Ok(0);
    }
    if end_ledger <= start_ledger {
        // Zero (or negative, which shouldn't happen) duration: fully vested at start.
        return Ok(total_amount);
    }

    let capped_ledger = current_ledger.min(end_ledger);
    let elapsed = i128::from(capped_ledger - start_ledger);
    let total_duration = i128::from(end_ledger - start_ledger);

    total_amount
        .checked_mul(elapsed)
        .ok_or(MathError::Overflow)?
        .checked_div(total_duration)
        .ok_or(MathError::InvalidDuration) // unreachable: total_duration > 0 here, kept for exhaustiveness
}

/// `streamed_amount - withdrawn`, i.e. the amount the recipient could withdraw right now.
pub fn withdrawable_amount(streamed: i128, withdrawn: i128) -> i128 {
    // Saturate rather than go negative: withdrawn should never exceed streamed in
    // practice (the contract enforces this on every withdrawal), but a saturating
    // subtraction keeps the API resilient to a stale/mid-reorg read instead of
    // returning a nonsensical negative "withdrawable" figure.
    (streamed - withdrawn).max(0)
}

/// Convenience: 0-100 integer progress used by `StreamView::progress_percent`.
pub fn progress_percent(streamed: i128, total_amount: i128) -> u8 {
    if total_amount <= 0 {
        return 0;
    }
    let pct = streamed.saturating_mul(100) / total_amount;
    pct.clamp(0, 100) as u8
}

/// On cancellation, funds split exactly at the moment of cancellation:
/// - the recipient keeps everything streamed so far (whether or not it was
///   already withdrawn -- if not yet withdrawn, it becomes immediately withdrawable),
/// - the sender is refunded the remainder of `total_amount`.
///
/// Returns `(recipient_entitlement, sender_refund)`. `recipient_entitlement` is the
/// recipient's *total* claim (streamed_amount at cancellation time), not just the
/// unwithdrawn portion -- callers that need "how much is left to withdraw" should
/// subtract `withdrawn` themselves, e.g. via `withdrawable_amount`.
///
/// Not yet wired into the indexer's `Cancelled` event handler (see
/// `indexer::apply_event`), which currently just flips `status` to `Cancelled` and
/// leaves `withdrawn` as-is -- once the Stream contract's `Cancelled` event carries
/// the on-chain refund amount (see the wire format documented in
/// `indexer::events`), this is the function to reconcile it against.
#[allow(dead_code)]
pub fn cancel_split(total_amount: i128, streamed_at_cancel: i128) -> (i128, i128) {
    let recipient_entitlement = streamed_at_cancel;
    let sender_refund = (total_amount - streamed_at_cancel).max(0);
    (recipient_entitlement, sender_refund)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The worked example from the docs: a 30-day, 3,000 XLM payroll stream.
    /// 1 ledger ~= 5s, so 30 days = 2,592,000s = 518,400 ledgers.
    const TOTAL: i128 = 3_000_0000000; // 3000 XLM at 7 decimal stroops
    const START: u32 = 1_000_000;
    const END: u32 = START + 518_400;

    #[test]
    fn before_start_is_zero() {
        assert_eq!(streamed_amount(TOTAL, START, END, START, START - 1).unwrap(), 0);
    }

    #[test]
    fn at_start_is_zero() {
        assert_eq!(streamed_amount(TOTAL, START, END, START, START).unwrap(), 0);
    }

    #[test]
    fn at_quarter_duration_is_quarter_amount() {
        let current = START + 518_400 / 4;
        let streamed = streamed_amount(TOTAL, START, END, START, current).unwrap();
        assert_eq!(streamed, TOTAL / 4);
    }

    #[test]
    fn at_half_duration_is_half_amount() {
        let current = START + 518_400 / 2;
        let streamed = streamed_amount(TOTAL, START, END, START, current).unwrap();
        assert_eq!(streamed, TOTAL / 2);
    }

    #[test]
    fn at_end_is_full_amount() {
        assert_eq!(streamed_amount(TOTAL, START, END, START, END).unwrap(), TOTAL);
    }

    #[test]
    fn after_end_is_capped_at_full_amount() {
        assert_eq!(streamed_amount(TOTAL, START, END, START, END + 100_000).unwrap(), TOTAL);
    }

    #[test]
    fn before_cliff_is_zero_even_if_after_start() {
        let cliff = START + 1000;
        let current = START + 500; // after start, before cliff
        assert_eq!(streamed_amount(TOTAL, START, END, cliff, current).unwrap(), 0);
    }

    #[test]
    fn at_cliff_reflects_full_elapsed_time_not_just_post_cliff_time() {
        // Crossing the cliff releases everything accrued since `start_ledger`,
        // not just what accrued after the cliff -- the cliff only gates
        // *withdrawal*, it does not reset the accrual clock.
        let cliff = START + 1000;
        let streamed = streamed_amount(TOTAL, START, END, cliff, cliff).unwrap();
        let expected = (TOTAL * 1000) / 518_400;
        assert_eq!(streamed, expected);
    }

    #[test]
    fn zero_duration_stream_is_fully_vested_at_start() {
        assert_eq!(streamed_amount(TOTAL, START, START, START, START).unwrap(), TOTAL);
        assert_eq!(streamed_amount(TOTAL, START, START, START, START - 1).unwrap(), 0);
    }

    #[test]
    fn multiply_before_divide_matches_expected_precision() {
        // 3000 XLM over 518,400 ledgers: after exactly 1 ledger, divide-first would
        // truncate (3000 * 1e7) / 518400 = 57 (as an integer amount-per-ledger) and
        // then multiply by 1, losing precision compared to multiplying first.
        let per_ledger_if_divided_first = TOTAL / 518_400; // truncates
        let one_ledger_in = streamed_amount(TOTAL, START, END, START, START + 1).unwrap();
        assert_eq!(one_ledger_in, (TOTAL * 1) / 518_400);
        // The multiply-first result is >= the naive divide-first-then-multiply result.
        assert!(one_ledger_in >= per_ledger_if_divided_first);
    }

    #[test]
    fn withdrawable_never_negative() {
        assert_eq!(withdrawable_amount(100, 150), 0);
        assert_eq!(withdrawable_amount(150, 100), 50);
    }

    #[test]
    fn cancel_split_sums_to_total() {
        let (recipient, sender) = cancel_split(TOTAL, TOTAL / 3);
        assert_eq!(recipient + sender, TOTAL);
        assert_eq!(recipient, TOTAL / 3);
    }

    #[test]
    fn progress_percent_bounds() {
        assert_eq!(progress_percent(0, TOTAL), 0);
        assert_eq!(progress_percent(TOTAL, TOTAL), 100);
        assert_eq!(progress_percent(TOTAL / 2, TOTAL), 50);
        assert_eq!(progress_percent(0, 0), 0);
    }
}
