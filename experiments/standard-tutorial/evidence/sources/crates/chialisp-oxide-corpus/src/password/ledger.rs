//! Actual fixed-commitment spend attempts. Runtime umbrellas are observations,
//! never members of the closed semantic consensus-rejection protocol.
use super::{argument, curry, environment};
use anyhow::{Context, Result, ensure};
use chialisp_oxide_backend_experiment::{
    Execution, FailureCode, MAX_COST, Outcome, artifact, execute,
};
use chialisp_oxide_simulator::{
    CONSENSUS_VERSION, Coin, CoinSpend, ErrorCode, NETWORK, SIMULATOR_VERSION, Signature,
    Simulator, SimulatorError, SpendBundle, VM_VERSION, create_coin, program, puzzle_hash,
};
use serde::Serialize;

const AMOUNT: u64 = 1_000;
const RECIPIENT: [u8; 32] = [0x42; 32];
const RETARGET: [u8; 32] = [0x43; 32];

#[derive(Serialize)]
pub struct State {
    pub created_height: Option<u32>,
    pub spent_height: Option<u32>,
    pub children: Vec<Child>,
}
#[derive(Serialize)]
pub struct Child {
    pub coin_id: String,
    pub parent_coin_id: String,
    pub puzzle_hash: String,
    pub amount: u64,
}
#[derive(Serialize)]
pub struct ConsensusObservation {
    pub status: &'static str,
    pub code: Option<String>,
    pub error: Option<String>,
    pub semantic_rejection_credit: bool,
}
#[derive(Serialize)]
pub struct Attempt {
    pub label: &'static str,
    pub argument_hex: String,
    pub coin_id: String,
    pub coin_puzzle_hash: String,
    pub reveal_hex: String,
    pub reveal_tree_hash: String,
    pub solution: String,
    pub solution_hex: String,
    pub vm_021: Execution,
    pub consensus: ConsensusObservation,
    pub before: State,
    pub after: State,
    pub full_state_unchanged: bool,
}
#[derive(Serialize)]
pub struct Comparison {
    pub backend: String,
    pub optimize_lir: bool,
    pub simulator: &'static str,
    pub consensus: &'static str,
    pub vm: &'static str,
    pub network: &'static str,
    pub module_tree_hash: String,
    pub instance_tree_hash: String,
    pub transaction_attempts: usize,
    pub attempts: Vec<Attempt>,
}

fn serialized(solution: &str) -> Result<Vec<u8>> {
    let mut allocator = clvmr::Allocator::new();
    let node = rue_clvm::assemble(&mut allocator, solution)?;
    Ok(clvmr::serde::node_to_bytes(&allocator, node)?)
}

fn state(simulator: &Simulator, coin: Coin) -> Result<State> {
    let input = simulator
        .coin_state(coin.coin_id())
        .context("missing input state")?;
    Ok(State {
        created_height: input.created_height,
        spent_height: input.spent_height,
        children: simulator
            .children(coin.coin_id())
            .iter()
            .map(|state| Child {
                coin_id: hex::encode(state.coin.coin_id()),
                parent_coin_id: hex::encode(state.coin.parent_coin_info),
                puzzle_hash: hex::encode(state.coin.puzzle_hash),
                amount: state.coin.amount,
            })
            .collect(),
    })
}

#[derive(Clone, Copy)]
// This fixed private allowlist belongs only to the password-instance protocol.
// InvalidCondition/WrongPuzzleHash are not added to ConsensusRejection::ALL.
enum Expected {
    Accepted([u8; 32]),
    Consensus(ErrorCode),
    RuntimeUmbrella,
}

struct Input<'a> {
    label: &'static str,
    argument: &'a [u8],
    reveal: &'a [u8],
    solution: String,
    expected: Expected,
}

fn attempt(instance: &[u8], input: Input<'_>) -> Result<Attempt> {
    let mut simulator = Simulator::new();
    let coin = create_coin(&mut simulator, instance, AMOUNT)?;
    let before_simulator = simulator.clone();
    let before = state(&simulator, coin)?;
    let solution_hex = hex::encode(serialized(&input.solution)?);
    // A substituted reveal must reach consensus, so deliberately bypass the
    // coin_spend() host preflight. Its VM behavior is evaluated independently.
    let spend = CoinSpend::new(
        coin,
        program(input.reveal)?,
        program(&hex::decode(&solution_hex)?)?,
    );
    let native = crate::expectation(
        "password-coin",
        &environment(input.argument, &input.solution)?,
    )?;
    let vm_021 = execute(input.reveal, &input.solution, MAX_COST)?;
    ensure!(
        vm_021.outcome != Outcome::Inconclusive,
        "inconclusive password ledger VM observation: {vm_021:?}"
    );
    ensure!(
        vm_021.outcome == native,
        "ledger reveal VM differs from actual native H.S policy invocation"
    );
    match input.expected {
        Expected::RuntimeUmbrella => ensure!(
            vm_021.outcome == Outcome::Rejected
                && vm_021
                    .failure
                    .is_some_and(|failure| failure.code() == FailureCode::Raise),
            "wrong password must first have an exact primary-VM Raise, never a resource failure",
        ),
        _ => ensure!(
            matches!(&vm_021.outcome, Outcome::Accepted { .. }),
            "these malformed conditions/reveal must evaluate successfully before consensus"
        ),
    }
    let result = simulator.new_transaction(SpendBundle::new(vec![spend], Signature::default()));
    let consensus = match (input.expected, result) {
        (Expected::Accepted(destination), Ok(_)) => {
            let children = simulator.children(coin.coin_id());
            ensure!(
                children.len() == 1
                    && children[0].coin.amount == AMOUNT
                    && children[0].coin.puzzle_hash.as_slice() == destination,
                "accepted password spend changed exact child"
            );
            ensure!(
                simulator
                    .coin_state(coin.coin_id())
                    .is_some_and(|state| state.spent_height.is_some()),
                "accepted input was not spent"
            );
            ConsensusObservation {
                status: "accepted",
                code: None,
                error: None,
                semantic_rejection_credit: false,
            }
        }
        (Expected::Consensus(expected), Err(SimulatorError::Validation(actual)))
            if expected == actual =>
        {
            ConsensusObservation {
                status: "consensus_rejected",
                code: Some(format!("{actual:?}")),
                error: Some(format!("Validation({actual:?})")),
                semantic_rejection_credit: true,
            }
        }
        (
            Expected::RuntimeUmbrella,
            Err(SimulatorError::Validation(ErrorCode::GeneratorRuntimeError)),
        ) => ConsensusObservation {
            status: "runtime_umbrella",
            code: Some("GeneratorRuntimeError".into()),
            error: Some("Validation(GeneratorRuntimeError)".into()),
            semantic_rejection_credit: false,
        },
        (_, actual) => anyhow::bail!(
            "password ledger observation {} had unexpected result: {actual:?}",
            input.label
        ),
    };
    let full_state_unchanged = simulator == before_simulator;
    ensure!(
        (consensus.status == "accepted") != full_state_unchanged,
        "password ledger state transition or complete rollback failed"
    );
    Ok(Attempt {
        label: input.label,
        argument_hex: hex::encode(input.argument),
        coin_id: hex::encode(coin.coin_id()),
        coin_puzzle_hash: hex::encode(coin.puzzle_hash),
        reveal_hex: hex::encode(input.reveal),
        reveal_tree_hash: hex::encode(puzzle_hash(input.reveal)?),
        solution: input.solution,
        solution_hex,
        vm_021,
        consensus,
        before,
        after: state(&simulator, coin)?,
        full_state_unchanged,
    })
}

pub(super) fn validate(
    backend: &str,
    module: &[u8],
    instance: &[u8],
    fixed_argument: &[u8],
) -> Result<Comparison> {
    let destination = hex::encode(RECIPIENT);
    let retarget = hex::encode(RETARGET);
    let ordinary = format!("((51 0x{destination} {AMOUNT}))");
    let other_argument = argument(b"xyz")?;
    let other_reveal = curry(module, &other_argument)?;
    ensure!(
        other_reveal != instance,
        "wrong reveal control must change commitment"
    );
    let inputs = [
        Input {
            label: "fixed commitment accepts the public fixture password",
            argument: fixed_argument,
            reveal: instance,
            solution: format!("(0x616263 {ordinary})"),
            expected: Expected::Accepted(RECIPIENT),
        },
        Input {
            label: "disclosed password permits unsigned retargeting",
            argument: fixed_argument,
            reveal: instance,
            solution: format!("(0x616263 ((51 0x{retarget} {AMOUNT})))"),
            expected: Expected::Accepted(RETARGET),
        },
        Input {
            label: "wrong password yields a runtime umbrella with complete rollback",
            argument: fixed_argument,
            reveal: instance,
            solution: format!("(0x78797a {ordinary})"),
            expected: Expected::RuntimeUmbrella,
        },
        Input {
            label: "pair destination reaches exact consensus rejection",
            argument: fixed_argument,
            reveal: instance,
            solution: format!("(0x616263 ((51 (1 . 2) {AMOUNT})))"),
            expected: Expected::Consensus(ErrorCode::InvalidPuzzleHash),
        },
        Input {
            label: "pair amount reaches exact consensus rejection",
            argument: fixed_argument,
            reveal: instance,
            solution: format!("(0x616263 ((51 0x{destination} (1 . 2))))"),
            expected: Expected::Consensus(ErrorCode::InvalidCoinAmount),
        },
        Input {
            label: "arbitrary output atom is not valid ledger conditions",
            argument: fixed_argument,
            reveal: instance,
            solution: "(0x616263 0x00)".into(),
            expected: Expected::Consensus(ErrorCode::InvalidCondition),
        },
        Input {
            label: "different committed hash cannot replace this coin reveal",
            argument: &other_argument,
            reveal: &other_reveal,
            solution: format!("(0x78797a {ordinary})"),
            expected: Expected::Consensus(ErrorCode::WrongPuzzleHash),
        },
    ];
    let attempts = inputs
        .into_iter()
        .map(|input| attempt(instance, input))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        attempts[0].coin_id == attempts[1].coin_id,
        "retargeting must use the identical starting coin, not a different commitment"
    );
    Ok(Comparison {
        backend: backend.into(),
        optimize_lir: true,
        simulator: SIMULATOR_VERSION,
        consensus: CONSENSUS_VERSION,
        vm: VM_VERSION,
        network: NETWORK,
        module_tree_hash: artifact(backend, true, module)?.tree_hash,
        instance_tree_hash: artifact(backend, true, instance)?.tree_hash,
        transaction_attempts: attempts.len(),
        attempts,
    })
}
