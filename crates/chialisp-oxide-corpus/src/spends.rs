use anyhow::{Result, ensure};
use chialisp_oxide_simulator::{
    ConsensusRejection, SecretKey, Signature, Simulator, SimulatorError, SpendBundle, coin_spend,
    create_coin, partial_aggregate_signature, require_consensus_rejection, sign_transaction,
};
use serde::Serialize;
mod precommit;
mod singleton;
#[derive(Serialize)]
pub struct Scenario {
    pub name: String,
    pub passed: bool,
    pub scope: String,
    pub rejections: Vec<Rejection>,
}
#[derive(Serialize)]
pub struct Rejection {
    stage: &'static str,
    code: &'static str,
}
fn rejection<T>(
    result: std::result::Result<T, SimulatorError>,
    expected: ConsensusRejection,
) -> Result<Rejection> {
    let observed = require_consensus_rejection(result, expected)?;
    Ok(Rejection {
        stage: "consensus",
        code: observed.as_str(),
    })
}
fn solution(text: &str) -> Result<Vec<u8>> {
    let mut a = clvmr::Allocator::new();
    let node = rue_clvm::assemble(&mut a, text)?;
    Ok(clvmr::serde::node_to_bytes(&a, node)?)
}
pub fn validate(slug: &str, code: &[u8]) -> Result<Vec<Scenario>> {
    if slug == "singleton-launcher" {
        return singleton::validate(code);
    }
    if matches!(slug, "precommit-claim" | "precommit-registry") {
        return precommit::validate(slug, code);
    }
    let msg = format!("0x{}", "ab".repeat(32));
    let input = match slug {
        "capped-payment" | "split-payment" => "(500)".into(),
        "boolean-gate" => "(1)".into(),
        "signature-payment" | "two-signature-payment" => format!("({msg})"),
        "recovery-payment" => format!("(() {msg})"),
        _ => "()".into(),
    };
    let keys = match slug {
        "signature-payment" | "recovery-payment" => vec![SecretKey::from_seed(&[0x17; 32])],
        "two-signature-payment" => vec![
            SecretKey::from_seed(&[0x17; 32]),
            SecretKey::from_seed(&[0x18; 32]),
        ],
        _ => vec![],
    };
    let mut simulator = Simulator::new();
    let coin = create_coin(&mut simulator, code, 1000)?;
    let spend = coin_spend(coin, code, &solution(&input)?)?;
    let mut reports = vec![];
    if !keys.is_empty() {
        let before = simulator.clone();
        let observed = rejection(
            simulator.new_transaction(SpendBundle::new(vec![spend.clone()], Signature::default())),
            ConsensusRejection::BadAggregateSignature,
        )?;
        ensure!(simulator == before, "rejection changed ledger");
        reports.push(Scenario {
            name: "missing signature rejected with rollback".into(),
            passed: true,
            scope: "ledger".into(),
            rejections: vec![observed],
        });
    }
    if slug == "two-signature-payment" {
        let before = simulator.clone();
        let partial = partial_aggregate_signature(std::slice::from_ref(&spend), &keys[..1])?;
        ensure!(
            partial != Signature::default(),
            "single-signer aggregate is empty"
        );
        let observed = rejection(
            simulator.new_transaction(SpendBundle::new(vec![spend.clone()], partial)),
            ConsensusRejection::BadAggregateSignature,
        )?;
        ensure!(simulator == before, "rejection changed ledger");
        reports.push(Scenario {
            name: "single signer rejected".into(),
            passed: true,
            scope: "ledger".into(),
            rejections: vec![observed],
        });
    }
    if slug == "timelocked-payment" {
        let before = simulator.clone();
        let observed = rejection(
            simulator.spend_coins(vec![spend.clone()], &keys),
            ConsensusRejection::AssertSecondsAbsoluteFailed,
        )?;
        ensure!(simulator == before, "early rejection changed ledger");
        simulator.pass_time(100);
        reports.push(Scenario {
            name: "early spend rejected, time advanced".into(),
            passed: true,
            scope: "ledger".into(),
            rejections: vec![observed],
        });
    }
    simulator.spend_coins(vec![spend.clone()], &keys)?;
    let children = simulator.children(coin.coin_id());
    let expected_sum = if slug == "capped-payment" { 500 } else { 1000 };
    ensure!(
        children.iter().map(|c| c.coin.amount).sum::<u64>() == expected_sum,
        "incorrect payout total"
    );
    ensure!(
        children.len() == if slug == "split-payment" { 2 } else { 1 },
        "incorrect child count"
    );
    let mut destinations = children
        .iter()
        .map(|c| hex::encode(c.coin.puzzle_hash.to_bytes()))
        .collect::<Vec<_>>();
    destinations.sort();
    let expected = if slug == "split-payment" {
        vec!["42".repeat(32), "43".repeat(32)]
    } else {
        vec!["42".repeat(32)]
    };
    ensure!(destinations == expected, "incorrect child puzzle hashes");
    reports.push(Scenario {
        name: "valid spend produces exact recipients and total".into(),
        passed: true,
        scope: "ledger".into(),
        rejections: vec![],
    });
    let before = simulator.clone();
    let observed = rejection(
        simulator.spend_coins(vec![spend], &keys),
        ConsensusRejection::DoubleSpend,
    )?;
    ensure!(simulator == before, "double spend changed ledger");
    reports.push(Scenario {
        name: "double spend rejected with rollback".into(),
        passed: true,
        scope: "ledger".into(),
        rejections: vec![observed],
    });
    let mut wrong = Simulator::new();
    wrong.pass_time(100);
    let coin = create_coin(&mut wrong, code, 999)?;
    let spend = coin_spend(coin, code, &solution(&input)?)?;
    let before = wrong.clone();
    let observed = rejection(
        wrong.spend_coins(vec![spend], &keys),
        if slug == "boolean-gate" {
            // This fixture creates 1,000 mojos without ASSERT_MY_AMOUNT;
            // spending its 999-mojo coin fails value conservation instead.
            ConsensusRejection::MintingCoin
        } else {
            ConsensusRejection::AssertMyAmountFailed
        },
    )?;
    ensure!(wrong == before, "wrong amount failure changed ledger");
    reports.push(Scenario {
        name: if slug == "boolean-gate" {
            "underfunded coin rejected with rollback"
        } else {
            "wrong coin amount rejected with rollback"
        }
        .into(),
        passed: true,
        scope: "ledger".into(),
        rejections: vec![observed],
    });
    if slug == "boolean-gate" {
        let mut surplus = Simulator::new();
        let coin = create_coin(&mut surplus, code, 1_001)?;
        let spend = coin_spend(coin, code, &solution(&input)?)?;
        surplus.spend_coins(vec![spend], &[])?;
        let children = surplus.children(coin.coin_id());
        ensure!(children.len() == 1, "surplus spend must create one child");
        ensure!(
            children[0].coin.amount == 1_000,
            "surplus changed the fixed payout"
        );
        ensure!(
            children[0].coin.puzzle_hash == chialisp_oxide_simulator::Bytes32::new([0x42; 32]),
            "surplus changed the fixed recipient"
        );
        ensure!(
            coin.amount == 1_001 && coin.amount.checked_sub(children[0].coin.amount) == Some(1),
            "surplus spend must leave exactly one mojo as an implicit fee"
        );
        ensure!(
            surplus
                .coin_state(coin.coin_id())
                .is_some_and(|state| state.spent_height.is_some()),
            "surplus input was not actually spent"
        );
        reports.push(Scenario {
            name: "surplus input becomes an implicit fee".into(),
            passed: true,
            scope: "ledger".into(),
            rejections: vec![],
        });
    }
    if slug == "recovery-payment" {
        let mut s = Simulator::new();
        let coin = create_coin(&mut s, code, 1000)?;
        let spend = coin_spend(coin, code, &solution(&format!("(1 {msg})"))?)?;
        let recovery = SecretKey::from_seed(&[0x18; 32]);
        let before = s.clone();
        let early = rejection(
            s.spend_coins(vec![spend.clone()], std::slice::from_ref(&recovery)),
            ConsensusRejection::AssertSecondsAbsoluteFailed,
        )?;
        ensure!(s == before, "early recovery changed state");
        s.pass_time(100);
        let before = s.clone();
        // This is a genuine owner-branch signature from the same coin/reveal,
        // submitted against the recovery solution. The signer is not asked to
        // pretend that an unavailable recovery key belongs to the owner.
        let owner_spend = coin_spend(coin, code, &solution(&format!("(() {msg})"))?)?;
        let owner_signature = sign_transaction(&[owner_spend], &keys)?;
        ensure!(
            owner_signature != Signature::default(),
            "owner signature is empty"
        );
        let wrong_key = rejection(
            s.new_transaction(SpendBundle::new(vec![spend.clone()], owner_signature)),
            ConsensusRejection::BadAggregateSignature,
        )?;
        ensure!(s == before, "wrong key changed state");
        s.spend_coins(vec![spend], &[recovery])?;
        reports.push(Scenario {
            name: "recovery requires both delay and recovery key".into(),
            passed: true,
            scope: "ledger".into(),
            rejections: vec![early, wrong_key],
        });
    }
    Ok(reports)
}
