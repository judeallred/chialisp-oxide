use super::*;
use chia_consensus::{
    flags::MEMPOOL_MODE,
    spendbundle_validation::{get_flags_for_height_and_constants, validate_clvm_and_signature},
};
use chia_protocol::{CoinSpend, Program, SpendBundle};
use chia_sdk_types::TESTNET11_CONSTANTS;
use chialisp_oxide_simulator::{ConsensusRejection, Simulator, require_consensus_rejection};
use clvmr::{
    Allocator, ChiaDialect, run_program,
    serde::{node_from_bytes, node_to_bytes},
};

pub(super) fn pair(first: &[u8], rest: &[u8]) -> Vec<u8> {
    [&[0xff], first, rest].concat()
}
pub(super) fn list(items: &[&[u8]]) -> Vec<u8> {
    items
        .iter()
        .rev()
        .fold(vec![0x80], |tail, first| pair(first, &tail))
}
pub(super) fn destination_atom(destination: &[u8; 32]) -> Vec<u8> {
    // Exactly an atom, not the SDK's generic array-of-integers CLVM encoding.
    [&[0xa0], destination.as_slice()].concat()
}
pub(super) fn curry(module: &[u8], destination: &[u8; 32]) -> Result<Vec<u8>> {
    let admitted = inspect(module)?;
    preflight_curry(admitted.stats(), module.len(), &limits())?;
    let argument = destination_atom(destination);
    let reveal = list(&[
        &[2],
        &pair(&[1], module),
        &list(&[&[4], &pair(&[1], &argument), &[1]]),
    ]);
    let inspected = chialisp_oxide_package::inspect::inspect_reveal(&reveal, &limits(), None)?;
    ensure!(
        inspected.arguments().len() == 1
            && inspected.arguments()[0].bytes() == argument
            && inspected.module().context("curry has no module")?.bytes() == module,
        "curry did not commit exactly one destination atom"
    );
    Ok(reveal)
}

fn preflight_curry(
    stats: &chialisp_oxide_package::artifact::ArtifactStats,
    bytes: usize,
    limits: &Limits,
) -> Result<()> {
    let add = |value: u64, extra: u64| value.checked_add(extra).context("curry dimension overflow");
    let within = |value: u64, maximum: usize| -> Result<()> {
        ensure!(
            value <= maximum as u64,
            "curry would exceed bounded artifact profile"
        );
        Ok(())
    };
    // Closed one-atom canonical curry adds8pairs/8atoms/48serializedbytes.
    // Check the complete result and conservatively charge intermediate copies
    // before constructing any quote/list buffers containing the module.
    within(add(u64::try_from(bytes)?, 48)?, limits.max_blob_bytes)?;
    within(add(stats.expanded_bytes, 48)?, limits.max_expanded_bytes)?;
    within(add(stats.nodes, 16)?, limits.max_nodes)?;
    within(add(stats.atom_bytes, 37)?, limits.max_atom_bytes)?;
    within(
        add(stats.allocator_atom_heap_bytes, 37)?,
        limits.max_atom_heap_bytes,
    )?;
    within(add(stats.depth, 3)?.max(7), limits.max_depth)?;
    let copies = u64::try_from(bytes)?
        .checked_mul(8)
        .context("curry copy work overflow")?;
    within(add(add(stats.work, copies)?, 4096)?, limits.max_work)?;
    Ok(())
}
pub(super) fn expected(destination: &[u8; 32]) -> Vec<u8> {
    list(&[
        &list(&[&[73], &[0x82, 3, 0xe8]]),
        &list(&[&[51], &destination_atom(destination), &[0x82, 3, 0xe8]]),
    ])
}

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum VmOutcome {
    Accepted {
        output_hex: String,
    },
    Rejected {
        failure: chialisp_oxide_vm::VmFailure,
    },
    Inconclusive {
        failure: chialisp_oxide_vm::VmFailure,
    },
}
#[derive(Serialize)]
pub struct VmObservation {
    pub lane: String,
    pub case: String,
    pub environment_hex: String,
    pub cost: Option<u64>,
    pub outcome: VmOutcome,
    pub expected_outcome: VmOutcome,
}
pub(super) fn execute(
    module: &[u8],
    env: &[u8],
    lane: &str,
    case: &str,
    destination: &[u8; 32],
) -> Result<VmObservation> {
    let expected_outcome = super::oracle::expected_outcome(lane, case, env, destination)?;
    inspect(module)?;
    inspect(env)?;
    let mut allocator = Allocator::new();
    let program = node_from_bytes(&mut allocator, module)?;
    let environment = node_from_bytes(&mut allocator, env)?;
    let (outcome, cost) = match run_program(
        &mut allocator,
        &ChiaDialect::new(clvmr::chia_dialect::MEMPOOL_MODE),
        program,
        environment,
        MAX_COST,
    ) {
        Ok(reduction) => (
            VmOutcome::Accepted {
                output_hex: hex::encode(node_to_bytes(&allocator, reduction.1)?),
            },
            Some(reduction.0),
        ),
        Err(error) => {
            let failure = chialisp_oxide_vm::classify_failure(&error);
            let outcome = if failure.is_program_rejection() {
                VmOutcome::Rejected { failure }
            } else {
                VmOutcome::Inconclusive { failure }
            };
            (outcome, None)
        }
    };
    Ok(VmObservation {
        lane: lane.into(),
        case: case.into(),
        environment_hex: hex::encode(env),
        cost,
        outcome,
        expected_outcome,
    })
}
pub(super) fn vectors_into(
    module: &[u8],
    destination: &[u8; 32],
    lane: &str,
    observations: &mut Vec<VmObservation>,
) -> Result<()> {
    let atom = destination_atom(destination);
    for (case, environment) in [
        ("destination_atom32", list(&[&atom])),
        ("missing", vec![0x80]),
        ("whole_atom", vec![1]),
        ("improper_tail", pair(&atom, &[1])),
        ("extra_argument", list(&[&atom, &[0x80]])),
        ("pair_destination", list(&[&pair(&[1], &[0x80])])),
        ("short_destination", list(&[&[0x9f; 32]])),
        ("long_destination", list(&[&[0xa1; 34]])),
    ] {
        observations.push(execute(module, &environment, lane, case, destination)?);
    }
    let curried = curry(module, destination)?;
    for (case, environment) in [
        ("curried_nil_solution", vec![0x80]),
        ("curried_extra_solution", vec![0xff, 1, 0x80]),
        ("curried_atom_solution", vec![1]),
    ] {
        observations.push(execute(&curried, &environment, lane, case, destination)?);
    }
    Ok(())
}

#[derive(Serialize)]
pub struct SimulatorObservation {
    pub lane: String,
    pub input_coin_id: String,
    pub child_coin_id: String,
    pub destination_hex: String,
    pub amount: u64,
    pub rejection: String,
    pub full_rejected_state_unchanged: bool,
    pub transaction_attempts: u32,
    pub consensus_cost: u64,
}
pub(super) fn simulate(
    module: &[u8],
    destination: &[u8; 32],
    lane: &str,
    attempts: &mut u32,
) -> Result<SimulatorObservation> {
    let reveal = curry(module, destination)?;
    let artifact = crate::Artifact {
        reveal,
        solution: vec![0x80],
    };
    let hash = artifact.puzzle_hash()?;
    let mut simulator = Simulator::new();
    let coin = simulator.new_coin(hash.into(), 1000);
    let bundle = |coin| {
        SpendBundle::new(
            vec![CoinSpend::new(
                coin,
                Program::from(artifact.reveal.as_slice()),
                Program::from(&[0x80][..]),
            )],
            Default::default(),
        )
    };
    let spend = bundle(coin);
    let flags = get_flags_for_height_and_constants(0, &TESTNET11_CONSTANTS) | MEMPOOL_MODE;
    let (conditions, _) =
        validate_clvm_and_signature(&spend, MAX_COST, &TESTNET11_CONSTANTS, flags).map_err(
            |error| anyhow::anyhow!("{lane}: local SDK validation failed: {:?}", error.1),
        )?;
    ensure!(
        conditions.spends.len() == 1
            && conditions.spends[0].create_coin.len() == 1
            && conditions.spends[0].create_coin[0].0.as_ref() == destination
            && conditions.spends[0].create_coin[0].1 == 1000
            && conditions.removal_amount == 1000
            && conditions.addition_amount == 1000,
        "{lane}: SDK complete outputs differ"
    );
    *attempts += 1;
    simulator.new_transaction(spend)?;
    let child = chia_protocol::Coin::new(coin.coin_id(), (*destination).into(), 1000);
    let children = simulator.children(coin.coin_id());
    ensure!(
        children.len() == 1
            && children[0].coin == child
            && simulator
                .coin_state(coin.coin_id())
                .is_some_and(|state| state.spent_height.is_some()),
        "{lane}: actual SDK children/input differ"
    );
    let bad = simulator.new_coin(hash.into(), 999);
    let before = simulator.clone();
    *attempts += 1;
    require_consensus_rejection(
        simulator.new_transaction(bundle(bad)),
        ConsensusRejection::AssertMyAmountFailed,
    )?;
    ensure!(
        simulator == before,
        "{lane}: typed rejection changed complete SDK state"
    );
    Ok(SimulatorObservation {
        lane: lane.into(),
        input_coin_id: hex::encode(coin.coin_id()),
        child_coin_id: hex::encode(child.coin_id()),
        destination_hex: hex::encode(destination),
        amount: 1000,
        rejection: "AssertMyAmountFailed".into(),
        full_rejected_state_unchanged: true,
        transaction_attempts: 2,
        consensus_cost: conditions.cost,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_reference_exact_atom_curry_environment_and_complete_spend() {
        let reference = chialisp_oxide_reference::compile(REFERENCE).unwrap();
        for destination in [[0; 32], [0x42; 32], [0xff; 32]] {
            let mut observations = vec![];
            vectors_into(
                &reference,
                &destination,
                "chialisp_reference",
                &mut observations,
            )
            .unwrap();
            for observation in &observations {
                super::super::oracle::verify(observation, &destination).unwrap();
            }
            assert_eq!(observations.len(), 11);
            for observation in &observations {
                if let VmOutcome::Rejected { failure } = observation.outcome {
                    assert_eq!(failure.code(), chialisp_oxide_vm::FailureCode::Raise);
                }
            }
            assert_eq!(
                simulate(&reference, &destination, "chialisp_reference", &mut 0)
                    .unwrap()
                    .transaction_attempts,
                2
            );
        }
    }
    #[test]
    fn curry_dimensions_are_preflighted_before_module_copies() {
        let artifact = inspect(&[1]).unwrap();
        let limits = limits();
        preflight_curry(artifact.stats(), 1, &limits).unwrap();
        let mut stats = artifact.stats().clone();
        stats.depth = limits.max_depth as u64 - 2;
        assert!(preflight_curry(&stats, 1, &limits).is_err());
        stats = artifact.stats().clone();
        stats.nodes = limits.max_nodes as u64 - 15;
        assert!(preflight_curry(&stats, 1, &limits).is_err());
        assert!(preflight_curry(artifact.stats(), limits.max_blob_bytes - 47, &limits).is_err());
        stats = artifact.stats().clone();
        stats.work = limits.max_work as u64;
        assert!(preflight_curry(&stats, 1, &limits).is_err());
    }
}
