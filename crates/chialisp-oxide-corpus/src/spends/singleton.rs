//! Launcher-only complete-spend observations. No singleton continuation is spent.
use super::{Scenario, rejection, solution};
use anyhow::{Result, ensure};
use chialisp_oxide_backend_experiment::{MAX_COST, Outcome, execute};
use chialisp_oxide_simulator::{
    Bytes32, Coin, CoinSpend, ConsensusRejection, Signature, Simulator, SpendBundle, coin_spend,
    create_coin,
};
use clvmr::{Allocator, NodePtr, SExp, serde::node_from_bytes};
use sha2::{Digest, Sha256};

const DESTINATION: [u8; 32] = [0x42; 32];
const AMOUNT: u64 = 1_000;

#[derive(Clone)]
struct Fields {
    destination: String,
    amount: String,
    key_values: String,
}

impl Fields {
    fn ordinary() -> Self {
        Self {
            destination: format!("0x{}", hex::encode(DESTINATION)),
            amount: "1000".into(),
            key_values: "((0x6b6579 0x76616c7565))".into(),
        }
    }

    fn environment(&self, tail: &str) -> String {
        format!(
            "({} {} {}{tail})",
            self.destination, self.amount, self.key_values,
        )
    }

    fn message(&self) -> Result<[u8; 32]> {
        // This independently hashes a newly built proper three-element tree.
        // It does not call the source port's sha256tree or the SDK hash kernel.
        let mut allocator = Allocator::new();
        let tree = node_from_bytes(&mut allocator, &solution(&self.environment(""))?)?;
        hash_tree(&allocator, tree, 0, &mut 0)
    }
}

fn hash_tree(
    allocator: &Allocator,
    node: NodePtr,
    depth: usize,
    visited: &mut usize,
) -> Result<[u8; 32]> {
    *visited += 1;
    ensure!(
        depth <= 32 && *visited <= 256,
        "ledger fixture hash exceeds its bound"
    );
    let mut digest = Sha256::new();
    match allocator.sexp(node) {
        SExp::Atom => {
            digest.update([1]);
            digest.update(allocator.atom(node).as_ref());
        }
        SExp::Pair(first, rest) => {
            digest.update([2]);
            digest.update(hash_tree(allocator, first, depth + 1, visited)?);
            digest.update(hash_tree(allocator, rest, depth + 1, visited)?);
        }
    }
    Ok(digest.finalize().into())
}

fn launcher_spend(
    simulator: &mut Simulator,
    code: &[u8],
    fields: &Fields,
    tail: &str,
) -> Result<(Coin, CoinSpend)> {
    let environment = fields.environment(tail);
    let expected = solution(&format!(
        "((51 {} {}) (60 0x{}))",
        fields.destination,
        fields.amount,
        hex::encode(fields.message()?),
    ))?;
    let actual = execute(code, &environment, MAX_COST)?;
    ensure!(
        actual.outcome
            == Outcome::Accepted {
                output_hex: hex::encode(expected)
            },
        "launcher must evaluate successfully to the exact raw conditions before consensus: {actual:?}",
    );
    let coin = create_coin(simulator, code, AMOUNT)?;
    let spend = coin_spend(coin, code, &solution(&environment)?)?;
    Ok((coin, spend))
}

fn companion_spend(
    simulator: &mut Simulator,
    launcher: Coin,
    expected_message: [u8; 32],
) -> Result<(Coin, CoinSpend)> {
    let mut digest = Sha256::new();
    digest.update(launcher.coin_id().as_slice());
    digest.update(expected_message);
    let name = hex::encode(digest.finalize());
    let code = chialisp_oxide_reference::compile(&format!("(mod () (list (list 61 0x{name})))"))?;
    let coin = create_coin(simulator, &code, 1)?;
    Ok((coin, coin_spend(coin, &code, &[0x80])?))
}

fn unsigned(spends: Vec<CoinSpend>) -> SpendBundle {
    // Submit directly: signer/preflight parsing must not confirm rejection of
    // malformed CREATE_COIN parameters. No signature is required by this module.
    SpendBundle::new(spends, Signature::default())
}

fn exact_children(simulator: &Simulator, launcher: Coin, companion: Coin) -> Result<()> {
    let children = simulator.children(launcher.coin_id());
    ensure!(
        children.len() == 1,
        "launcher must create exactly one child"
    );
    ensure!(
        children[0].coin.amount == AMOUNT,
        "launcher child amount changed"
    );
    ensure!(
        children[0].coin.puzzle_hash == Bytes32::new(DESTINATION),
        "launcher child recipient changed",
    );
    ensure!(
        simulator.children(companion.coin_id()).is_empty(),
        "companion must create no child"
    );
    for coin in [launcher, companion] {
        ensure!(
            simulator
                .coin_state(coin.coin_id())
                .is_some_and(|state| state.spent_height.is_some()),
            "successful bundle did not spend both inputs",
        );
    }
    Ok(())
}

fn scenario(name: &str, rejections: Vec<super::Rejection>) -> Scenario {
    Scenario {
        name: name.into(),
        passed: true,
        scope: "ledger".into(),
        rejections,
    }
}

pub(super) fn validate(code: &[u8]) -> Result<Vec<Scenario>> {
    let fields = Fields::ordinary();
    let message = fields.message()?;
    let mut reports = Vec::new();
    let mut valid = Simulator::new();
    let (launcher, spend) = launcher_spend(&mut valid, code, &fields, "")?;
    let (companion, assertion) = companion_spend(&mut valid, launcher, message)?;
    let valid_spends = vec![spend, assertion];
    valid.new_transaction(unsigned(valid_spends.clone()))?;
    exact_children(&valid, launcher, companion)?;
    reports.push(scenario(
        "launcher creates exact child and bound announcement",
        vec![],
    ));

    let mut missing = Simulator::new();
    let (launcher, _) = launcher_spend(&mut missing, code, &fields, "")?;
    let (_, assertion) = companion_spend(&mut missing, launcher, message)?;
    let before = missing.clone();
    let observed = rejection(
        missing.new_transaction(unsigned(vec![assertion])),
        ConsensusRejection::AssertCoinAnnouncementFailed,
    )?;
    ensure!(
        missing == before,
        "missing launcher rejection changed ledger"
    );
    reports.push(scenario(
        "missing launcher announcement rejected with rollback",
        vec![observed],
    ));

    // Three failed submissions form one named grouping, not three scenarios.
    let mut changes = [fields.clone(), fields.clone(), fields.clone()];
    changes[0].destination = format!("0x{}", "43".repeat(32));
    changes[1].amount = "999".into();
    changes[2].key_values = "((0x6b6579 0x6368616e676564))".into();
    let mut observations = Vec::new();
    for changed in changes {
        ensure!(
            changed.message()? != message,
            "selected-field mutation must change commitment"
        );
        let mut ledger = Simulator::new();
        let (launcher, spend) = launcher_spend(&mut ledger, code, &changed, "")?;
        let (_, assertion) = companion_spend(&mut ledger, launcher, message)?;
        let before = ledger.clone();
        observations.push(rejection(
            ledger.new_transaction(unsigned(vec![spend, assertion])),
            ConsensusRejection::AssertCoinAnnouncementFailed,
        )?);
        ensure!(
            ledger == before,
            "selected-field tamper rejection changed ledger"
        );
    }
    reports.push(scenario(
        "launcher announcement binds all three selected fields",
        observations,
    ));

    let mut replayed = Simulator::new();
    let (expected_launcher, _) = launcher_spend(&mut replayed, code, &fields, "")?;
    let (different_launcher, spend) = launcher_spend(&mut replayed, code, &fields, "")?;
    ensure!(
        expected_launcher.coin_id() != different_launcher.coin_id(),
        "coin-ID replay control requires distinct coins",
    );
    let (_, assertion) = companion_spend(&mut replayed, expected_launcher, message)?;
    let before = replayed.clone();
    let observed = rejection(
        replayed.new_transaction(unsigned(vec![spend, assertion])),
        ConsensusRejection::AssertCoinAnnouncementFailed,
    )?;
    ensure!(
        replayed == before,
        "wrong launcher coin-ID rejection changed ledger"
    );
    reports.push(scenario(
        "different launcher coin cannot satisfy announcement",
        vec![observed],
    ));

    // Two successful submissions form one grouping; both tails are ignored.
    for tail in [" 0x69676e6f726564", " . 0x00"] {
        let mut ledger = Simulator::new();
        let (launcher, spend) = launcher_spend(&mut ledger, code, &fields, tail)?;
        let (companion, assertion) = companion_spend(&mut ledger, launcher, message)?;
        ledger.new_transaction(unsigned(vec![spend, assertion]))?;
        exact_children(&ledger, launcher, companion)?;
    }
    reports.push(scenario(
        "ignored launcher tail preserves spend and commitment",
        vec![],
    ));

    let mut excess = fields.clone();
    excess.amount = "1001".into();
    let mut ledger = Simulator::new();
    let (_, spend) = launcher_spend(&mut ledger, code, &excess, "")?;
    let before = ledger.clone();
    let observed = rejection(
        // Do not add the one-mojo companion here: it could fund the overmint.
        ledger.new_transaction(unsigned(vec![spend])),
        ConsensusRejection::MintingCoin,
    )?;
    ensure!(ledger == before, "overmint rejection changed ledger");
    reports.push(scenario(
        "launcher overmint rejected with rollback",
        vec![observed],
    ));

    let mut malformed_destination = fields.clone();
    malformed_destination.destination = "(0x61 . 0x62)".into();
    let mut malformed_amount = fields.clone();
    malformed_amount.amount = "(0x61 . 0x62)".into();
    for (fields, expected, name) in [
        (
            malformed_destination,
            ConsensusRejection::InvalidPuzzleHash,
            "launcher pair destination rejected by consensus",
        ),
        (
            malformed_amount,
            ConsensusRejection::InvalidCoinAmount,
            "launcher pair amount rejected by consensus",
        ),
    ] {
        let mut ledger = Simulator::new();
        let (_, spend) = launcher_spend(&mut ledger, code, &fields, "")?;
        let before = ledger.clone();
        let observed = rejection(ledger.new_transaction(unsigned(vec![spend])), expected)?;
        ensure!(
            ledger == before,
            "malformed CREATE_COIN rejection changed ledger"
        );
        reports.push(scenario(name, vec![observed]));
    }

    let before = valid.clone();
    let observed = rejection(
        valid.new_transaction(unsigned(valid_spends)),
        ConsensusRejection::DoubleSpend,
    )?;
    ensure!(valid == before, "launcher replay rejection changed ledger");
    reports.push(scenario(
        "launcher replay rejected with rollback",
        vec![observed],
    ));
    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_launcher_complete_spends_match_closed_consensus_profile() {
        let root = crate::root();
        let source =
            std::fs::read_to_string(root.join("puzzles/singleton-launcher/reference.clsp"))
                .unwrap();
        let include = std::fs::read_to_string(root.join("corpus/upstream/chia-puzzles/cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8/puzzles/condition_codes.clib")).unwrap();
        let compiled = chialisp_oxide_reference::compile_with_includes(
            &source,
            &[("condition_codes.clib", &include)],
        )
        .unwrap();
        let published = hex::decode(
            std::fs::read_to_string(root.join("puzzles/singleton-launcher/reference.clsp.hex"))
                .unwrap()
                .trim(),
        )
        .unwrap();
        assert_eq!(
            compiled, published,
            "independent compilation must retain canonical module identity"
        );
        assert_eq!(compiled.len(), 175);
        let report = validate(&compiled).unwrap();
        let protocol: serde_json::Value = serde_json::from_str(include_str!(
            "../../../chialisp-oxide-simulator/rejection-protocol.json"
        ))
        .unwrap();
        let expected = &protocol["puzzles"]["singleton-launcher"];
        assert_eq!(report.len(), expected.as_array().unwrap().len());
        for (actual, expected) in report.iter().zip(expected.as_array().unwrap()) {
            assert_eq!(actual.name, expected["name"]);
            assert!(actual.passed);
            let codes = actual.rejections.iter().map(|r| r.code).collect::<Vec<_>>();
            assert_eq!(serde_json::json!(codes), expected["codes"]);
        }
    }
}
