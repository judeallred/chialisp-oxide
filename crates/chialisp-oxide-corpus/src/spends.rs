use anyhow::{Result, ensure};
use chialisp_oxide_simulator::{SecretKey, Simulator, coin_spend, create_coin};
use serde::Serialize;
#[derive(Serialize)]
pub struct Scenario {
    pub name: String,
    pub passed: bool,
    pub scope: String,
}
fn solution(text: &str) -> Result<Vec<u8>> {
    let mut a = clvmr::Allocator::new();
    let node = rue_clvm::assemble(&mut a, text)?;
    Ok(clvmr::serde::node_to_bytes(&a, node)?)
}
pub fn validate(slug: &str, code: &[u8]) -> Result<Vec<Scenario>> {
    if slug == "bounded-counter" {
        return Ok(vec![]);
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
        ensure!(
            simulator.spend_coins(vec![spend.clone()], &[]).is_err(),
            "missing signature accepted"
        );
        ensure!(simulator == before, "rejection changed ledger");
        reports.push(Scenario {
            name: "missing signature rejected with rollback".into(),
            passed: true,
            scope: "ledger".into(),
        });
    }
    if slug == "two-signature-payment" {
        let before = simulator.clone();
        ensure!(
            simulator
                .spend_coins(vec![spend.clone()], &keys[..1])
                .is_err(),
            "one of two signatures accepted"
        );
        ensure!(simulator == before, "rejection changed ledger");
        reports.push(Scenario {
            name: "single signer rejected".into(),
            passed: true,
            scope: "ledger".into(),
        });
    }
    if slug == "timelocked-payment" {
        let before = simulator.clone();
        ensure!(
            simulator.spend_coins(vec![spend.clone()], &keys).is_err(),
            "early timelock accepted"
        );
        ensure!(simulator == before, "early rejection changed ledger");
        simulator.pass_time(100);
        reports.push(Scenario {
            name: "early spend rejected, time advanced".into(),
            passed: true,
            scope: "ledger".into(),
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
    });
    let before = simulator.clone();
    ensure!(
        simulator.spend_coins(vec![spend], &keys).is_err(),
        "double spend accepted"
    );
    ensure!(simulator == before, "double spend changed ledger");
    reports.push(Scenario {
        name: "double spend rejected with rollback".into(),
        passed: true,
        scope: "ledger".into(),
    });
    let mut wrong = Simulator::new();
    wrong.pass_time(100);
    let coin = create_coin(&mut wrong, code, 999)?;
    let spend = coin_spend(coin, code, &solution(&input)?)?;
    let before = wrong.clone();
    ensure!(
        wrong.spend_coins(vec![spend], &keys).is_err(),
        "wrong amount accepted"
    );
    ensure!(wrong == before, "wrong amount failure changed ledger");
    reports.push(Scenario {
        name: "wrong coin amount rejected with rollback".into(),
        passed: true,
        scope: "ledger".into(),
    });
    if slug == "recovery-payment" {
        let mut s = Simulator::new();
        let coin = create_coin(&mut s, code, 1000)?;
        let spend = coin_spend(coin, code, &solution(&format!("(1 {msg})"))?)?;
        let recovery = SecretKey::from_seed(&[0x18; 32]);
        let before = s.clone();
        ensure!(
            s.spend_coins(vec![spend.clone()], std::slice::from_ref(&recovery))
                .is_err(),
            "early recovery accepted"
        );
        ensure!(s == before, "early recovery changed state");
        s.pass_time(100);
        let before = s.clone();
        ensure!(
            s.spend_coins(vec![spend.clone()], &keys).is_err(),
            "owner key satisfied recovery branch"
        );
        ensure!(s == before, "wrong key changed state");
        s.spend_coins(vec![spend], &[recovery])?;
        reports.push(Scenario {
            name: "recovery requires both delay and recovery key".into(),
            passed: true,
            scope: "ledger".into(),
        });
    }
    Ok(reports)
}
