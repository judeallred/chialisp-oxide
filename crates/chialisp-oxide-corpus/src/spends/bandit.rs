//! One-armed bandit: actual pulls resolved across blocks, and a house that withholds.
//!
//! Each pull coin curries the house payout, a fresh house seed commitment, the
//! player payout and the player's salt. The host fixture picks salts so the three
//! resolved pulls exercise a loss, a pair and a jackpot; a real player would not
//! know the seed. Commitments, outcomes and payouts are computed here with the
//! host's own SHA-256 spelling, not the puzzle sources. Every rejected bundle
//! must carry its exact typed consensus code and leave the simulator unchanged.
use super::{Scenario, rejection};
use anyhow::{Result, ensure};
use chialisp_oxide_simulator::{
    Coin, CoinSpend, ConsensusRejection, Simulator, coin_spend, puzzle_hash,
};
use clvmr::{Allocator, NodePtr, serde::node_to_bytes};
use sha2::{Digest, Sha256};

const BET: u64 = 1_000;
const PULL_TOTAL: u64 = 8_000;
const RESOLVE_WINDOW: u32 = 10;
const SEED_DOMAIN: &[u8] = b"chialisp-oxide/bandit/v1/seed:00";
const SPIN_DOMAIN: &[u8] = b"chialisp-oxide/bandit/v1/spin:00";
const HOUSE: [u8; 32] = [0x48; 32];
const PLAYER: [u8; 32] = [0x50; 32];

fn sha(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

fn payout(seed: &[u8; 32], salt: &[u8; 32]) -> u64 {
    let outcome = sha(&[SPIN_DOMAIN, seed, salt])[31];
    if outcome < 16 {
        7 * BET
    } else if outcome < 64 {
        2 * BET
    } else {
        0
    }
}

/// A fixture salt making `seed` pay exactly `won`.
fn salt_paying(seed: &[u8; 32], won: u64) -> Result<[u8; 32]> {
    for n in 0..u16::MAX {
        let mut salt = [0x5a; 32];
        salt[..2].copy_from_slice(&n.to_be_bytes());
        if payout(seed, &salt) == won {
            return Ok(salt);
        }
    }
    anyhow::bail!("no fixture salt for payout {won}")
}

struct Pull {
    seed: [u8; 32],
    salt: [u8; 32],
    reveal: Vec<u8>,
}

impl Pull {
    fn new(code: &[u8], seed: [u8; 32], salt: [u8; 32]) -> Result<Self> {
        let commitment = sha(&[SEED_DOMAIN, &seed]);
        let mut a = Allocator::new();
        let mut list = NodePtr::NIL;
        for item in [salt.as_slice(), &PLAYER, &commitment, &HOUSE] {
            let atom = a.new_atom(item)?;
            list = a.new_pair(atom, list)?;
        }
        let terms = node_to_bytes(&a, list)?;
        Ok(Self {
            seed,
            salt,
            reveal: super::super::curried::standard_form(code, &terms)?,
        })
    }

    fn fund(&self, simulator: &mut Simulator, amount: u64) -> Result<Coin> {
        Ok(simulator.new_coin(puzzle_hash(&self.reveal)?, amount))
    }

    fn spend(&self, coin: Coin, resolve: bool) -> Result<CoinSpend> {
        let solution = format!(
            "({} 0x{})",
            if resolve { "1" } else { "()" },
            hex::encode(self.seed)
        );
        coin_spend(coin, &self.reveal, &super::solution(&solution)?)
    }
}

fn scenario(name: &str, rejections: Vec<super::Rejection>) -> Scenario {
    Scenario {
        name: name.into(),
        passed: true,
        scope: "ledger".into(),
        rejections,
    }
}

/// Submit an invalid bundle and require both the exact code and full rollback.
fn rejected(
    simulator: &mut Simulator,
    spend: CoinSpend,
    expected: ConsensusRejection,
) -> Result<super::Rejection> {
    let before = simulator.clone();
    let observed = rejection(simulator.spend_coins(vec![spend], &[]), expected)?;
    ensure!(
        *simulator == before,
        "rejected bandit bundle changed the ledger"
    );
    Ok(observed)
}

fn paid(simulator: &Simulator, coin: Coin, destination: &[u8; 32]) -> u64 {
    simulator
        .children(coin.coin_id())
        .iter()
        .filter(|child| child.coin.puzzle_hash.to_bytes() == *destination)
        .map(|child| child.coin.amount)
        .sum()
}

pub fn validate(code: &[u8]) -> Result<Vec<Scenario>> {
    let mut simulator = Simulator::new();
    let mut reports = vec![];

    // Three resolved pulls track the house bankroll across blocks.
    let mut house_net: i64 = 0;
    for (index, won) in [0, 2 * BET, 7 * BET].into_iter().enumerate() {
        let seed = [0x70 + index as u8; 32];
        let pull = Pull::new(code, seed, salt_paying(&seed, won)?)?;
        ensure!(
            payout(&pull.seed, &pull.salt) == won,
            "fixture salt pays the intended line"
        );
        let coin = pull.fund(&mut simulator, PULL_TOTAL)?;
        simulator.create_block();
        simulator.spend_coins(vec![pull.spend(coin, true)?], &[])?;
        ensure!(
            paid(&simulator, coin, &PLAYER) == won
                && paid(&simulator, coin, &HOUSE) == PULL_TOTAL - won,
            "resolution pays exactly the paytable amount and the house the remainder"
        );
        // The house contributed the reserve; it keeps the remainder.
        house_net += (PULL_TOTAL - won) as i64 - (PULL_TOTAL - BET) as i64;
    }
    ensure!(
        house_net == 3 * BET as i64 - 9 * BET as i64,
        "house bankroll after a loss, a pair and a jackpot"
    );
    reports.push(scenario(
        "three pulls pay a loss, a pair and a jackpot by the paytable across blocks",
        vec![],
    ));

    let seed = [0x7f; 32];
    let pull = Pull::new(code, seed, salt_paying(&seed, 0)?)?;
    let coin = pull.fund(&mut simulator, PULL_TOTAL)?;
    let misfunded = pull.fund(&mut simulator, PULL_TOTAL + 1)?;
    simulator.create_block();
    let early = rejected(
        &mut simulator,
        pull.spend(coin, false)?,
        ConsensusRejection::AssertHeightRelativeFailed,
    )?;
    let wrong_amount = rejected(
        &mut simulator,
        pull.spend(misfunded, true)?,
        ConsensusRejection::AssertMyAmountFailed,
    )?;
    for _ in 0..RESOLVE_WINDOW {
        simulator.create_block();
    }
    let late = rejected(
        &mut simulator,
        pull.spend(coin, true)?,
        ConsensusRejection::AssertBeforeHeightRelativeFailed,
    )?;
    reports.push(scenario(
        "an early timeout claim, a misfunded pull and a late resolve are rejected",
        vec![early, wrong_amount, late],
    ));

    let timeout = pull.spend(coin, false)?;
    simulator.spend_coins(vec![timeout.clone()], &[])?;
    ensure!(
        paid(&simulator, coin, &PLAYER) == PULL_TOTAL
            && simulator.children(coin.coin_id()).len() == 1,
        "a withholding house forfeits the whole pull to the player"
    );
    let replay = rejected(&mut simulator, timeout, ConsensusRejection::DoubleSpend)?;
    reports.push(scenario(
        "a withholding house forfeits the whole pull and a replay is rejected",
        vec![replay],
    ));
    Ok(reports)
}
