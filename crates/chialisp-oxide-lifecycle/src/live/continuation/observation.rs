//! Exact complete locally executed effects plus provider point records. This
//! does not claim a complete provider parent/wallet inventory or inclusion proof.
use super::session::CheckedTransaction;
use super::*;
use crate::live::{LiveState, MonitoringEvidence};

pub(super) fn observe(tx: &CheckedTransaction, e: &MonitoringEvidence) -> Result<LiveState> {
    crate::live::workflow::require_network(&e.network)?;
    crate::live::check_receipts(&e.receipts)?;
    ensure!(
        e.inputs.len() == tx.bundle.coin_spends.len()
            && e.spends.len() == e.inputs.len()
            && e.children.len() == tx.children.len(),
        "complete exact point observations"
    );
    // Retained evidence bypasses the provider JSON parser. Check every record's
    // own structure before treating newer snapshots as merely inconclusive.
    // Creation at height zero is valid; a spent record needs a positive spend
    // height no earlier than creation, and unspent records need height zero.
    if e.inputs.iter().chain(&e.children).flatten().any(|coin| {
        if coin.spent {
            coin.spent_height == 0 || coin.spent_height < coin.confirmed_height
        } else {
            coin.spent_height != 0
        }
    }) {
        return Ok(LiveState::Conflict);
    }
    if e.before != e.after {
        return Ok(LiveState::Inconclusive);
    }
    // Concurrent provider snapshots can legitimately be newer than the peak.
    if e.inputs
        .iter()
        .chain(&e.children)
        .flatten()
        .any(|c| c.confirmed_height > e.after.height || c.spent_height > e.after.height)
    {
        return Ok(LiveState::Inconclusive);
    }
    let mut heights = vec![];
    let mut unspent = 0;
    for ((expected, actual), spend) in tx.bundle.coin_spends.iter().zip(&e.inputs).zip(&e.spends) {
        let Some(actual) = actual else {
            return Ok(LiveState::Missing);
        };
        if actual.coin() != expected.coin {
            return Ok(LiveState::Conflict);
        }
        if !actual.spent {
            if actual.spent_height != 0 || spend.is_some() {
                return Ok(LiveState::Conflict);
            }
            unspent += 1;
            continue;
        }
        if actual.spent_height == 0 || actual.spent_height < actual.confirmed_height {
            return Ok(LiveState::Conflict);
        }
        heights.push(actual.spent_height);
        let Some(spend) = spend else {
            return Ok(LiveState::Inconclusive);
        };
        ensure!(
            spend.puzzle_reveal.len() <= MAX_BUNDLE && spend.solution.len() <= MAX_BUNDLE,
            "bounded observed spend"
        );
        if spend.coin_spend() != *expected {
            return Ok(LiveState::Conflict);
        }
    }
    if unspent == e.inputs.len() {
        return Ok(if e.children.iter().all(Option::is_none) {
            LiveState::Unspent
        } else {
            LiveState::Inconclusive
        });
    }
    if unspent > 0 {
        return Ok(LiveState::Inconclusive);
    }
    if !heights.iter().all(|h| *h == heights[0]) {
        return Ok(LiveState::Conflict);
    }
    for (expected, actual) in tx.children.iter().zip(&e.children) {
        let Some(actual) = actual else {
            return Ok(LiveState::Inconclusive);
        };
        if actual.coin() != expected.coin() || actual.confirmed_height != heights[0] {
            return Ok(LiveState::Conflict);
        }
    }
    Ok(LiveState::Confirmed)
}
