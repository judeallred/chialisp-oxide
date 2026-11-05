use super::*;
pub(super) fn require_network(identity: &NetworkIdentity) -> Result<Testnet11Network> {
    ensure!(identity.scope.len() <= 1024, "bounded provider scope");
    ensure!(
        identity.origin == network::ORIGIN
            && identity.local_agg_sig_me_domain
                == TESTNET11_CONSTANTS.agg_sig_me_additional_data.to_bytes(),
        "fixed Testnet11 local signing domain"
    );
    // The genesis value is pinned locally. The provider reports only labels;
    // neither this token nor bracketing tips authenticate chain genesis/history.
    ensure!(
        identity.local_genesis_challenge == TESTNET11_CONSTANTS.genesis_challenge.to_bytes(),
        "pinned local Testnet11 genesis constant"
    );
    Ok(Testnet11Network::from_provider_labels(
        &identity.provider_network_name,
        &identity.provider_network_prefix,
    )?)
}

pub(super) fn record(coin: chia_protocol::Coin) -> CoinRecord {
    CoinRecord {
        parent: coin.parent_coin_info.to_bytes(),
        puzzle_hash: coin.puzzle_hash.to_bytes(),
        amount: coin.amount,
    }
}

pub(super) fn same_record(actual: &LiveCoinRecord, expected: &CoinRecord) -> bool {
    actual.parent == expected.parent
        && actual.puzzle_hash == expected.puzzle_hash
        && actual.amount == expected.amount
}

/// Check complete expected effects against exact spend bytes and child point
/// records. This is provider consistency, not an authenticated inclusion proof.
pub(super) fn observe_state(
    transfer: &CatPrepared,
    evidence: &MonitoringEvidence,
) -> Result<LiveState> {
    require_network(&evidence.network)?;
    check_receipts(&evidence.receipts)?;
    ensure!(
        evidence.inputs.len() == transfer.inputs.len()
            && evidence.spends.len() == transfer.inputs.len()
            && evidence.children.len() == transfer.children.len(),
        "complete bounded observation dimensions"
    );
    for (input, spend) in evidence.inputs.iter().zip(&evidence.spends) {
        if let Some(spend) = spend {
            ensure!(
                input.as_ref().is_some_and(|input| input.spent)
                    && spend.puzzle_reveal.len() <= MAX_BUNDLE
                    && spend.solution.len() <= MAX_BUNDLE,
                "bounded spend only for a reported spent input"
            );
        }
    }
    if evidence.before != evidence.after {
        return Ok(LiveState::Inconclusive);
    }
    let mut spent = 0;
    let mut unspent = 0;
    let mut height = None;
    for (expected, actual) in transfer.inputs.iter().zip(&evidence.inputs) {
        let Some(actual) = actual else {
            return Ok(LiveState::Missing);
        };
        if !same_record(actual, expected) || actual.confirmed_height > evidence.after.height {
            return Ok(LiveState::Conflict);
        }
        if actual.spent {
            if actual.spent_height < actual.confirmed_height
                || actual.spent_height > evidence.after.height
                || actual.spent_height == 0
                || height.is_some_and(|h| h != actual.spent_height)
            {
                return Ok(LiveState::Conflict);
            }
            spent += 1;
            height = Some(actual.spent_height);
        } else {
            if actual.spent_height != 0 {
                return Ok(LiveState::Conflict);
            }
            unspent += 1;
        }
    }
    if unspent == transfer.inputs.len() {
        ensure!(
            evidence.spends.iter().all(Option::is_none)
                && evidence.children.iter().all(Option::is_none),
            "unspent inputs cannot carry spent effects"
        );
        return Ok(LiveState::Unspent);
    }
    if spent != transfer.inputs.len() {
        return Ok(LiveState::Inconclusive);
    }
    let bundle = decode_bundle(&decode_hex(&transfer.bundle_hex, MAX_BUNDLE)?)?;
    ensure!(
        bundle.coin_spends.len() == transfer.inputs.len(),
        "complete prepared spend set"
    );
    for (expected, actual) in bundle.coin_spends.iter().zip(&evidence.spends) {
        let Some(actual) = actual else {
            return Ok(LiveState::Inconclusive);
        };
        ensure!(
            actual.puzzle_reveal.len() <= MAX_BUNDLE && actual.solution.len() <= MAX_BUNDLE,
            "bounded observed spend"
        );
        if actual.coin_spend() != *expected {
            return Ok(LiveState::Conflict);
        }
    }
    for (expected, actual) in transfer.children.iter().zip(&evidence.children) {
        let Some(actual) = actual else {
            return Ok(LiveState::Inconclusive);
        };
        if !same_record(actual, expected) || Some(actual.confirmed_height) != height {
            return Ok(LiveState::Conflict);
        }
    }
    Ok(LiveState::Confirmed)
}
