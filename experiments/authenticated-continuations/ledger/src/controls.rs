//! Actual-source counterexamples. Each selected original/mutant funds its own reveal.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlsInput {
    controls: Vec<Control>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Control {
    kind: String,
    mode: String,
    original_path: PathBuf,
    mutant_path: PathBuf,
}

fn selected(kind: &str, module: &[u8], mutated: bool) -> Result<Value> {
    let queue = heads();
    let count = if kind == "drop_terminal" { 1 } else { 4 };
    let amount = if kind == "drop_amount" {
        4001
    } else {
        u64::from(count) * PAYOUT
    };
    let recipient = if kind == "drop_opening" {
        [0x55; 32]
    } else {
        RECIPIENTS[0]
    };
    let module_hash = puzzle_hash(module)?.to_bytes();
    let curried = curry(module, &terms(&module_hash, &queue[0], count)?)?;
    let mut sim = Simulator::new();
    let coin = sim.new_coin(puzzle_hash(&curried)?, amount);
    let b = SpendBundle::new(
        vec![coin_spend(
            coin,
            &curried,
            &solution(&recipient, &queue[1])?,
        )?],
        Signature::default(),
    );
    if !mutated {
        let negative = match kind {
            "drop_opening" | "drop_terminal" => Some(runtime_umbrella(&mut sim, b.clone(), kind)?),
            "drop_amount" => Some(exact_rejection(
                &mut sim,
                b.clone(),
                ErrorCode::AssertMyAmountFailed,
                kind,
            )?),
            "wrong_successor" => None,
            _ => anyhow::bail!("unknown source control"),
        };
        if let Some(negative) = negative {
            return Ok(
                json!({"module_hash":hex::encode(module_hash),"coin":coin_json(coin),"negative":negative}),
            );
        }
    }
    let before_height = sim.height();
    let costs = measurement::measure(&b, before_height)?;
    sim.new_transaction(b)
        .map_err(|e| anyhow::anyhow!("source {kind}/{mutated}: {e:?}"))?;
    let children = sim.children(coin.coin_id());
    let payout = Coin::new(coin.coin_id(), recipient.into(), PAYOUT);
    ensure!(
        children
            .iter()
            .any(|s| s.coin == payout && s.spent_height.is_none()),
        "source exact payout"
    );
    let next = if count > 1 {
        let destination = if kind == "wrong_successor" && mutated {
            [0x77; 32].into()
        } else {
            puzzle_hash(&curry(module, &terms(&module_hash, &queue[1], count - 1)?)?)?
        };
        let next = Coin::new(coin.coin_id(), destination, PAYOUT * u64::from(count - 1));
        ensure!(
            children.len() == 2
                && children
                    .iter()
                    .any(|s| s.coin == next && s.spent_height.is_none()),
            "source exact successor"
        );
        Some(next)
    } else {
        ensure!(children.len() == 1, "source terminal has no successor");
        None
    };
    ensure!(
        sim.coin_state(coin.coin_id())
            .context("source spent")?
            .spent_height
            == Some(before_height),
        "source spend mined"
    );
    Ok(
        json!({"module_hash":hex::encode(module_hash),"coin":coin_json(coin),"accepted":true,"payout":coin_json(payout),
        "successor":next.map(coin_json),"fees":amount-u64::from(count)*PAYOUT,"costs":costs}),
    )
}

pub(super) fn run(input: &Path, output: &Path) -> Result<()> {
    ensure!(!output.exists(), "preserve prior control run");
    fs::create_dir_all(output)?;
    let raw = read_bounded(input, 32 * 1024)?;
    ensure!(raw.len() <= 32 * 1024, "bounded control inventory");
    let input: ControlsInput = serde_json::from_slice(&raw)?;
    let expected = [
        "drop_amount",
        "drop_opening",
        "drop_terminal",
        "wrong_successor",
    ];
    let mut keys = input
        .controls
        .iter()
        .map(|r| (r.kind.as_str(), r.mode.as_str()))
        .collect::<Vec<_>>();
    keys.sort_unstable();
    let mut expected_keys = expected
        .into_iter()
        .flat_map(|k| [(k, "off"), (k, "on")])
        .collect::<Vec<_>>();
    expected_keys.sort_unstable();
    ensure!(
        keys == expected_keys,
        "exact four source controls in both Rust modes"
    );
    let mut records = Vec::new();
    for row in input.controls {
        let mut values = Vec::new();
        for (mutated, path) in [(false, row.original_path), (true, row.mutant_path)] {
            let hex = String::from_utf8(read_bounded(&path, 2 * 1024 * 1024)?)?;
            ensure!(hex.len() <= 2 * 1024 * 1024, "bounded source program");
            let code = hex::decode(hex.trim())?;
            values.push(selected(&row.kind, &code, mutated)?);
        }
        records
            .push(json!({"kind":row.kind,"mode":row.mode,"original":values[0],"mutant":values[1]}));
    }
    let report = json!({"schema":1,"scope":"actual-source own-reveal counterexamples, not generic proof, live-network or compiler-preservation evidence",
        "sdk":SIMULATOR_VERSION,"consensus":CONSENSUS_VERSION,"vm":VM_VERSION,"network":NETWORK,
        "controls":records,"counts":{"selected_control_builds":8,"accepted_submissions":10,"typed_rejected_submissions":2,
            "runtime_umbrella_submissions":4,"total_sdk_attempts":16}});
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "four source controls, two modes,16 SDK attempts:10 accepted,2 typed rejected,4 runtime umbrellas"
    );
    Ok(())
}
