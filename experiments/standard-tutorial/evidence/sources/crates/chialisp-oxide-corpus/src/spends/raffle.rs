//! Raffle: an actual draw, and an actual refund through an ordered-payout queue.
//!
//! The host curries the selected raffle module with the ordered-payout module
//! hash (from its independently compiled Chialisp reference) and four entrants.
//! Commitments, the draw seed, the winner and every queue digest are computed
//! here with the host's own SHA-256 and curry spelling, not the puzzle sources.
//! Every rejected bundle must carry its exact typed consensus code and leave the
//! simulator unchanged. This finite lane does not prove fairness: the last salt
//! holder can still learn the outcome and force a refund.
use super::{Scenario, rejection};
use anyhow::{Result, ensure};
use chialisp_oxide_simulator::{
    Coin, CoinSpend, ConsensusRejection, Simulator, coin_spend, puzzle_hash,
};
use clvmr::{Allocator, NodePtr, serde::node_to_bytes};
use sha2::{Digest, Sha256};

const TICKET: u64 = 1_000;
const POT: u64 = 4_000;
const REFUND_AGE: u32 = 10;
const COMMIT_DOMAIN: &[u8] = b"chialisp-oxide/raffle/v1/commit";
const DRAW_DOMAIN: &[u8] = b"chialisp-oxide/raffle/v1/draw";
const QUEUE_DOMAIN: &[u8] = b"chialisp-oxide/ordered-payout/v1/1000:";
const PAYOUTS: [[u8; 32]; 4] = [[0x11; 32], [0x22; 32], [0x33; 32], [0x44; 32]];

fn sha(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

fn salts(round: u8) -> [[u8; 32]; 4] {
    std::array::from_fn(|k| [0x60 + 4 * round + k as u8; 32])
}

fn seed(salts: &[[u8; 32]; 4]) -> [u8; 32] {
    sha(&[
        DRAW_DOMAIN,
        &sha(&[&salts[0], &salts[1]]),
        &sha(&[&salts[2], &salts[3]]),
    ])
}

/// Serialize a tree of atoms given as nested proper lists.
enum Node<'a> {
    Atom(&'a [u8]),
    List(Vec<Node<'a>>),
}

fn build(a: &mut Allocator, node: &Node) -> Result<NodePtr> {
    Ok(match node {
        Node::Atom(bytes) => a.new_atom(bytes)?,
        Node::List(items) => {
            let mut list = NodePtr::NIL;
            for item in items.iter().rev() {
                let value = build(a, item)?;
                list = a.new_pair(value, list)?;
            }
            list
        }
    })
}

fn serialize(node: &Node) -> Result<Vec<u8>> {
    let mut a = Allocator::new();
    let root = build(&mut a, node)?;
    Ok(node_to_bytes(&a, root)?)
}

struct Modules {
    raffle: Vec<u8>,
    ordered: Vec<u8>,
    ordered_hash: [u8; 32],
}

impl Modules {
    fn raffle_reveal(&self, commitments: &[[u8; 32]; 4]) -> Result<Vec<u8>> {
        let entries = (0..4)
            .map(|i| Node::List(vec![Node::Atom(&PAYOUTS[i]), Node::Atom(&commitments[i])]))
            .collect::<Vec<_>>();
        let mut terms = vec![Node::Atom(&self.ordered_hash)];
        terms.extend(entries);
        super::super::curried::standard_form(&self.raffle, &serialize(&Node::List(terms))?)
    }

    fn queue_reveal(&self, head: &[u8; 32]) -> Result<Vec<u8>> {
        let terms = Node::List(vec![Node::Atom(&self.ordered_hash), Node::Atom(head)]);
        super::super::curried::standard_form(&self.ordered, &serialize(&terms)?)
    }
}

fn raffle_spend(
    modules: &Modules,
    coin: Coin,
    commitments: &[[u8; 32]; 4],
    draw: bool,
    salts: &[[u8; 32]; 4],
) -> Result<CoinSpend> {
    let solution = format!(
        "({} ({}))",
        if draw { "1" } else { "()" },
        salts
            .iter()
            .map(|s| format!("0x{}", hex::encode(s)))
            .collect::<Vec<_>>()
            .join(" ")
    );
    coin_spend(
        coin,
        &modules.raffle_reveal(commitments)?,
        &super::solution(&solution)?,
    )
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
        "rejected raffle bundle changed the ledger"
    );
    Ok(observed)
}

fn only_child(simulator: &Simulator, coin: Coin, puzzle: &[u8; 32], amount: u64) -> Result<Coin> {
    let children = simulator.children(coin.coin_id());
    ensure!(
        children.len() == 1
            && children[0].coin.puzzle_hash.to_bytes() == *puzzle
            && children[0].coin.amount == amount,
        "spend creates exactly the expected single child"
    );
    Ok(children[0].coin)
}

pub fn validate(code: &[u8]) -> Result<Vec<Scenario>> {
    let ordered = chialisp_oxide_reference::compile(include_str!(
        "../../../../puzzles/ordered-payout/reference.clsp"
    ))?;
    let modules = Modules {
        raffle: code.to_vec(),
        ordered_hash: puzzle_hash(&ordered)?.to_bytes(),
        ordered,
    };
    let mut reports = vec![];
    let mut simulator = Simulator::new();

    // Raffle one: drawn before the refund age.
    let first = salts(0);
    let commitments = first.map(|salt| sha(&[COMMIT_DOMAIN, &salt]));
    let reveal = modules.raffle_reveal(&commitments)?;
    let coin = simulator.new_coin(puzzle_hash(&reveal)?, POT);
    simulator.create_block();
    let early_refund = rejected(
        &mut simulator,
        raffle_spend(&modules, coin, &commitments, false, &first)?,
        ConsensusRejection::AssertHeightRelativeFailed,
    )?;
    let draw = raffle_spend(&modules, coin, &commitments, true, &first)?;
    simulator.spend_coins(vec![draw.clone()], &[])?;
    let winner = PAYOUTS[usize::from(seed(&first)[31] % 4)];
    only_child(&simulator, coin, &winner, POT)?;
    let replay = rejected(&mut simulator, draw, ConsensusRejection::DoubleSpend)?;
    reports.push(scenario(
        "a draw pays the drawn entrant and an early refund or replay is rejected",
        vec![early_refund, replay],
    ));

    // Raffle two: nobody draws in time, so the pot is refunded through a queue.
    let second = salts(1);
    let commitments = second.map(|salt| sha(&[COMMIT_DOMAIN, &salt]));
    let coin = simulator.new_coin(puzzle_hash(&modules.raffle_reveal(&commitments)?)?, POT);
    for _ in 0..REFUND_AGE {
        simulator.create_block();
    }
    let late_draw = rejected(
        &mut simulator,
        raffle_spend(&modules, coin, &commitments, true, &second)?,
        ConsensusRejection::AssertBeforeHeightRelativeFailed,
    )?;
    simulator.spend_coins(
        vec![raffle_spend(&modules, coin, &commitments, false, &second)?],
        &[],
    )?;
    let mut heads = vec![sha(&[QUEUE_DOMAIN, b"end"])];
    for payout in PAYOUTS.iter().rev() {
        let next = *heads.last().expect("terminal digest");
        heads.push(sha(&[QUEUE_DOMAIN, payout, &next]));
    }
    heads.reverse();
    let mut queue = only_child(
        &simulator,
        coin,
        &puzzle_hash(&modules.queue_reveal(&heads[0])?)?.to_bytes(),
        POT,
    )?;
    for (index, payout) in PAYOUTS.iter().enumerate() {
        let remaining = POT - TICKET * index as u64;
        let solution = format!(
            "(0x{} 0x{} {remaining})",
            hex::encode(payout),
            hex::encode(heads[index + 1])
        );
        let spend = coin_spend(
            queue,
            &modules.queue_reveal(&heads[index])?,
            &super::solution(&solution)?,
        )?;
        simulator.spend_coins(vec![spend], &[])?;
        let children = simulator.children(queue.coin_id());
        ensure!(
            children
                .iter()
                .filter(|c| c.coin.puzzle_hash.to_bytes() == *payout && c.coin.amount == TICKET)
                .count()
                == 1,
            "refund queue repays each entrant exactly one ticket"
        );
        if index + 1 < PAYOUTS.len() {
            let next = puzzle_hash(&modules.queue_reveal(&heads[index + 1])?)?.to_bytes();
            queue = children
                .iter()
                .find(|c| c.coin.puzzle_hash.to_bytes() == next)
                .map(|c| c.coin)
                .ok_or_else(|| anyhow::anyhow!("refund queue successor missing"))?;
        } else {
            ensure!(
                children.len() == 1,
                "the final refund step leaves no successor"
            );
        }
        simulator.create_block();
    }
    reports.push(scenario(
        "after the refund age a draw is rejected and the refund queue repays every entrant",
        vec![late_draw],
    ));
    Ok(reports)
}
