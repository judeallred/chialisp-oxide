//! Ordered-payout continuation: an actual self-curried chain across blocks.
//!
//! The host builds the standard curried reveal of the selected module with
//! `(module_hash head)`, funds it with exactly one payout per queued recipient,
//! and advances it one mined block per step. Queue digests and successor
//! puzzle hashes are computed here with the host's own SHA-256 and curry
//! spelling, not the puzzle sources. Every rejected bundle must carry its exact
//! typed consensus code and leave the simulator unchanged. Puzzle-level raises
//! (skipped, reordered or injected items) are covered by the VM vectors;
//! this finite ledger lane does not prove liveness, inclusion or atomicity.
use super::{Scenario, rejection};
use anyhow::{Result, ensure};
use chialisp_oxide_simulator::{
    Coin, CoinSpend, ConsensusRejection, Simulator, coin_spend, puzzle_hash,
};
use clvmr::{Allocator, NodePtr, serde::node_to_bytes};
use sha2::{Digest, Sha256};

const PAYOUT: u64 = 1_000;
const DOMAIN: &[u8] = b"chialisp-oxide/ordered-payout/v1/1000:";
const RECIPIENTS: [[u8; 32]; 3] = [[0x11; 32], [0x22; 32], [0x33; 32]];

fn sha(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

/// Every digest from the head to the terminal, in queue order.
fn digests(recipients: &[[u8; 32]]) -> Vec<[u8; 32]> {
    let mut out = vec![sha(&[DOMAIN, b"end"])];
    for recipient in recipients.iter().rev() {
        let next = *out.last().expect("terminal digest");
        out.push(sha(&[DOMAIN, recipient, &next]));
    }
    out.reverse();
    out
}

fn terms(module_hash: &[u8; 32], head: &[u8; 32]) -> Result<Vec<u8>> {
    let mut a = Allocator::new();
    let mut list = NodePtr::NIL;
    for item in [head.as_slice(), module_hash.as_slice()] {
        let atom = a.new_atom(item)?;
        list = a.new_pair(atom, list)?;
    }
    Ok(node_to_bytes(&a, list)?)
}

struct Chain {
    module: Vec<u8>,
    module_hash: [u8; 32],
}

impl Chain {
    fn new(code: &[u8]) -> Result<Self> {
        Ok(Self {
            module: code.to_vec(),
            module_hash: puzzle_hash(code)?.to_bytes(),
        })
    }

    fn reveal(&self, head: &[u8; 32]) -> Result<Vec<u8>> {
        super::super::curried::standard_form(&self.module, &terms(&self.module_hash, head)?)
    }

    fn spend(
        &self,
        coin: Coin,
        head: &[u8; 32],
        recipient: &[u8; 32],
        next: &[u8; 32],
        amount: u64,
    ) -> Result<CoinSpend> {
        let solution = format!(
            "(0x{} 0x{} {amount})",
            hex::encode(recipient),
            hex::encode(next)
        );
        coin_spend(coin, &self.reveal(head)?, &super::solution(&solution)?)
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
        "rejected ordered-payout bundle changed the ledger"
    );
    Ok(observed)
}

pub fn validate(code: &[u8]) -> Result<Vec<Scenario>> {
    let chain = Chain::new(code)?;
    let heads = digests(&RECIPIENTS);
    let mut simulator = Simulator::new();
    let total = PAYOUT * RECIPIENTS.len() as u64;
    let mut coin = simulator.new_coin(puzzle_hash(&chain.reveal(&heads[0])?)?, total);
    let mut reports = vec![];

    // Typed rejections at the funded head, before any step is taken.
    let understated = rejected(
        &mut simulator,
        chain.spend(coin, &heads[0], &RECIPIENTS[0], &heads[1], total - PAYOUT)?,
        ConsensusRejection::AssertMyAmountFailed,
    )?;
    let overstated = rejected(
        &mut simulator,
        chain.spend(coin, &heads[0], &RECIPIENTS[0], &heads[1], total + PAYOUT)?,
        ConsensusRejection::AssertMyAmountFailed,
    )?;
    reports.push(scenario(
        "a misstated coin amount with a valid opening is rejected by consensus with rollback",
        vec![understated, overstated],
    ));

    let mut consumed = vec![];
    for (index, recipient) in RECIPIENTS.iter().enumerate() {
        let remaining = PAYOUT * (RECIPIENTS.len() - index) as u64;
        ensure!(
            coin.amount == remaining,
            "continuation coin holds the committed remainder"
        );
        let spend = chain.spend(coin, &heads[index], recipient, &heads[index + 1], remaining)?;
        simulator.spend_coins(vec![spend.clone()], &[])?;
        consumed.push(spend);
        let children = simulator.children(coin.coin_id());
        let paid: Vec<_> = children
            .iter()
            .filter(|child| child.coin.puzzle_hash.to_bytes() == *recipient)
            .collect();
        ensure!(
            paid.len() == 1 && paid[0].coin.amount == PAYOUT,
            "step pays exactly its committed recipient"
        );
        let last = index + 1 == RECIPIENTS.len();
        ensure!(
            children.len() == if last { 1 } else { 2 },
            "step creates exactly the expected children"
        );
        if !last {
            let expected = puzzle_hash(&chain.reveal(&heads[index + 1])?)?.to_bytes();
            let successor: Vec<_> = children
                .iter()
                .filter(|child| child.coin.puzzle_hash.to_bytes() == expected)
                .collect();
            ensure!(
                successor.len() == 1 && successor[0].coin.amount == remaining - PAYOUT,
                "successor is the same module curried with the next digest and remainder"
            );
            coin = successor[0].coin;
        }
        // One step per block: the chain spans several actual blocks.
        simulator.create_block();
    }
    reports.push(scenario(
        "three-step queue pays every recipient in order across blocks and leaves no successor",
        vec![],
    ));

    // A consumed step cannot be replayed.
    let replay = rejected(
        &mut simulator,
        consumed[0].clone(),
        ConsensusRejection::DoubleSpend,
    )?;
    reports.push(scenario(
        "replaying a consumed step is rejected with rollback",
        vec![replay],
    ));
    Ok(reports)
}
