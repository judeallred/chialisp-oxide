//! Spuds: an actual multi-round hot-potato game across blocks.
//!
//! The host builds the standard curried reveal of the selected module with
//! `(module_hash key payout round)`, funds a 1,000-mojo round-1 pot, and lets two
//! signed takes grow it before the last holder claims. Each take is funded by a
//! separate identity-puzzle coin in the same bundle. Successor puzzle hashes and
//! intent messages are computed here with the host's own SHA-256 and curry
//! spelling, not the puzzle sources. Every rejected bundle must carry its exact
//! typed consensus code and leave the simulator unchanged. This finite lane does
//! not prove fairness, liveness or resistance to take ordering by block producers.
use super::{Scenario, rejection};
use anyhow::{Result, ensure};
use chialisp_oxide_simulator::{
    Coin, CoinSpend, ConsensusRejection, SecretKey, Signature, Simulator, SpendBundle, coin_spend,
    puzzle_hash,
};
use clvmr::{Allocator, NodePtr, serde::node_to_bytes};

const RAISE: u64 = 1_000;
const TAKE_WINDOW: u32 = 5;
/// The CLVM identity program `1`: its solution is its condition list.
const TOP_UP_PUZZLE: [u8; 1] = [0x01];

struct Player {
    key: SecretKey,
    payout: [u8; 32],
}

impl Player {
    fn new(seed: u8, payout: u8) -> Self {
        Self {
            key: SecretKey::from_seed(&[seed; 32]),
            payout: [payout; 32],
        }
    }

    fn public(&self) -> [u8; 48] {
        self.key.public_key().to_bytes()
    }
}

fn terms(module_hash: &[u8; 32], key: &[u8; 48], payout: &[u8; 32], round: u8) -> Result<Vec<u8>> {
    let mut a = Allocator::new();
    let round_atom = a.new_small_number(u32::from(round))?;
    let mut list = a.new_pair(round_atom, NodePtr::NIL)?;
    for item in [payout.as_slice(), key.as_slice(), module_hash.as_slice()] {
        let atom = a.new_atom(item)?;
        list = a.new_pair(atom, list)?;
    }
    Ok(node_to_bytes(&a, list)?)
}

struct Game {
    module: Vec<u8>,
    module_hash: [u8; 32],
}

impl Game {
    fn reveal(&self, holder: &Player, round: u8) -> Result<Vec<u8>> {
        super::super::curried::standard_form(
            &self.module,
            &terms(&self.module_hash, &holder.public(), &holder.payout, round)?,
        )
    }

    fn pot_hash(&self, holder: &Player, round: u8) -> Result<[u8; 32]> {
        Ok(puzzle_hash(&self.reveal(holder, round)?)?.to_bytes())
    }

    fn spend(
        &self,
        pot: Coin,
        holder: &Player,
        round: u8,
        take: bool,
        next: &Player,
    ) -> Result<CoinSpend> {
        let solution = format!(
            "({} 0x{} 0x{})",
            if take { "1" } else { "()" },
            hex::encode(next.public()),
            hex::encode(next.payout)
        );
        coin_spend(
            pot,
            &self.reveal(holder, round)?,
            &super::solution(&solution)?,
        )
    }
}

fn top_up(simulator: &mut Simulator) -> Result<CoinSpend> {
    let coin = simulator.new_coin(puzzle_hash(&TOP_UP_PUZZLE)?, RAISE);
    coin_spend(coin, &TOP_UP_PUZZLE, &super::solution("()")?)
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
    spends: Vec<CoinSpend>,
    keys: &[SecretKey],
    expected: ConsensusRejection,
) -> Result<super::Rejection> {
    let before = simulator.clone();
    let result = if keys.is_empty() {
        simulator.new_transaction(SpendBundle::new(spends, Signature::default()))
    } else {
        simulator.spend_coins(spends, keys)
    };
    let observed = rejection(result, expected)?;
    ensure!(
        *simulator == before,
        "rejected spuds bundle changed the ledger"
    );
    Ok(observed)
}

fn successor(simulator: &Simulator, pot: Coin, puzzle: [u8; 32], amount: u64) -> Result<Coin> {
    let children: Vec<_> = simulator
        .children(pot.coin_id())
        .into_iter()
        .filter(|child| child.coin.puzzle_hash.to_bytes() == puzzle)
        .collect();
    ensure!(
        children.len() == 1 && children[0].coin.amount == amount,
        "take recreates exactly the committed successor with the raised pot"
    );
    Ok(children[0].coin)
}

pub fn validate(code: &[u8]) -> Result<Vec<Scenario>> {
    let game = Game {
        module: code.to_vec(),
        module_hash: puzzle_hash(code)?.to_bytes(),
    };
    let (alice, bob, carol) = (
        Player::new(0x31, 0x11),
        Player::new(0x32, 0x22),
        Player::new(0x33, 0x33),
    );
    let mut simulator = Simulator::new();
    let pot = simulator.new_coin(game.pot_hash(&alice, 1)?.into(), RAISE);
    // Every path asserts a minimum one-block coin age.
    simulator.create_block();
    let mut reports = vec![];

    let take = game.spend(pot, &alice, 1, true, &bob)?;
    let funding = top_up(&mut simulator)?;
    let unsigned = rejected(
        &mut simulator,
        vec![take.clone(), funding],
        &[],
        ConsensusRejection::BadAggregateSignature,
    )?;
    let unfunded = rejected(
        &mut simulator,
        vec![take.clone()],
        std::slice::from_ref(&bob.key),
        ConsensusRejection::MintingCoin,
    )?;
    reports.push(scenario(
        "takes require the new holder's signature and an external top-up",
        vec![unsigned, unfunded],
    ));

    let funding = top_up(&mut simulator)?;
    simulator.spend_coins(vec![take, funding], std::slice::from_ref(&bob.key))?;
    let pot = successor(&simulator, pot, game.pot_hash(&bob, 2)?, 2 * RAISE)?;
    simulator.create_block();
    let early = rejected(
        &mut simulator,
        vec![game.spend(pot, &bob, 2, false, &bob)?],
        std::slice::from_ref(&bob.key),
        ConsensusRejection::AssertHeightRelativeFailed,
    )?;
    let funding = top_up(&mut simulator)?;
    simulator.spend_coins(
        vec![game.spend(pot, &bob, 2, true, &carol)?, funding],
        std::slice::from_ref(&carol.key),
    )?;
    let pot = successor(&simulator, pot, game.pot_hash(&carol, 3)?, 3 * RAISE)?;
    reports.push(scenario(
        "two signed takes grow the pot across blocks and an early claim is rejected",
        vec![early],
    ));

    for _ in 0..TAKE_WINDOW {
        simulator.create_block();
    }
    let funding = top_up(&mut simulator)?;
    let late = rejected(
        &mut simulator,
        vec![game.spend(pot, &carol, 3, true, &alice)?, funding],
        std::slice::from_ref(&alice.key),
        ConsensusRejection::AssertBeforeHeightRelativeFailed,
    )?;
    let claim = game.spend(pot, &carol, 3, false, &carol)?;
    simulator.spend_coins(vec![claim.clone()], std::slice::from_ref(&carol.key))?;
    let children = simulator.children(pot.coin_id());
    ensure!(
        children.len() == 1
            && children[0].coin.puzzle_hash.to_bytes() == carol.payout
            && children[0].coin.amount == 3 * RAISE,
        "claim pays the whole pot to the last holder's committed payout"
    );
    reports.push(scenario(
        "after the window a take is rejected and the last holder claims the whole pot",
        vec![late],
    ));

    let replay = rejected(
        &mut simulator,
        vec![claim],
        std::slice::from_ref(&carol.key),
        ConsensusRejection::DoubleSpend,
    )?;
    reports.push(scenario(
        "replaying the consumed claim is rejected with rollback",
        vec![replay],
    ));
    Ok(reports)
}
