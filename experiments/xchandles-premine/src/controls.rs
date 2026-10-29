//! Actual wrong-neighbor bundle submission with separate conclusive VM witness.
use anyhow::{Result, ensure};
use chia_consensus::validation_error::ErrorCode;
use chia_protocol::SpendBundle;
use chia_puzzle_types::singleton::SingletonSolution;
use chia_sdk_driver::SpendContext;
use chia_sdk_test::{Simulator, SimulatorError};
use chia_sdk_types::puzzles::{RawActionLayerSolution, XchandlesRegisterActionSolution};
use clvmr::NodePtr;
use serde_json::{Value, json};

pub fn wrong_neighbors(
    ctx: &mut SpendContext,
    sim: &mut Simulator,
    bundle: &SpendBundle,
) -> Result<Value> {
    let mut changed = bundle.clone();
    let last = changed.coin_spends.last_mut().unwrap();
    let node = ctx.alloc(&last.solution)?;
    let mut outer: SingletonSolution<RawActionLayerSolution<NodePtr, NodePtr, NodePtr>> =
        ctx.extract(node)?;
    let mut action: XchandlesRegisterActionSolution<
        NodePtr,
        NodePtr,
        NodePtr,
        NodePtr,
        chia_protocol::Bytes32,
    > = ctx.extract(outer.inner_solution.solutions[0])?;
    std::mem::swap(
        &mut action.neighbors.left_value,
        &mut action.neighbors.right_value,
    );
    outer.inner_solution.solutions[0] = ctx.alloc(&action)?;
    last.solution = ctx.serialize(&outer)?;
    let mut allocator = clvmr::Allocator::new_limited(64 * 1024 * 1024);
    let puzzle = clvmr::serde::node_from_bytes(&mut allocator, last.puzzle_reveal.as_ref())?;
    let solution = clvmr::serde::node_from_bytes(&mut allocator, last.solution.as_ref())?;
    let result = clvmr::run_program(
        &mut allocator,
        &clvmr::ChiaDialect::new(clvmr::MEMPOOL_MODE),
        puzzle,
        solution,
        500_000_000,
    );
    ensure!(
        matches!(result, Err(clvmr::error::EvalErr::Raise(_))),
        "wrong-neighbor witness must be exact VM Raise, resource/infra is inconclusive"
    );
    let witness = json!({"coin_id":hex::encode(last.coin.coin_id()),"solution_hex":hex::encode(last.solution.as_ref()),"failure":{"protocol":"clvmr-0.16.2/v1","kind":"program","code":"raise"}});
    let before = sim.clone();
    let result = sim.new_transaction(changed);
    ensure!(
        matches!(
            result,
            Err(SimulatorError::Validation(ErrorCode::GeneratorRuntimeError))
        ),
        "wrong-neighbor actual SDK runtime umbrella"
    );
    ensure!(
        *sim == before,
        "full simulator equality after wrong-neighbor rejection"
    );
    Ok(
        json!({"name":"wrong_neighbors","status":"runtime_umbrella","code":"GeneratorRuntimeError","semantic_consensus_rejection_credit":false,"full_simulator_equality":true,"component_vm":[witness]}),
    )
}
