use anyhow::{Result, ensure};
use chialisp_oxide_simulator::{Coin, ErrorCode, Simulator, SimulatorError, SpendBundle};
use clvmr::{
    Allocator, NodePtr,
    serde::{node_from_bytes, node_to_bytes},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub fn sha(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}
pub fn list(a: &mut Allocator, x: &[NodePtr]) -> Result<NodePtr> {
    let mut t = NodePtr::NIL;
    for p in x.iter().rev() {
        t = a.new_pair(*p, t)?;
    }
    Ok(t)
}
pub fn encode(f: impl FnOnce(&mut Allocator) -> Result<NodePtr>) -> Result<Vec<u8>> {
    let mut a = Allocator::new();
    let p = f(&mut a)?;
    Ok(node_to_bytes(&a, p)?)
}
pub fn uint(a: &mut Allocator, n: u64) -> Result<NodePtr> {
    let bytes = n.to_be_bytes();
    let i = bytes.iter().position(|b| *b != 0).unwrap_or(8);
    let mut b = bytes[i..].to_vec();
    if b.first().is_some_and(|v| v & 128 != 0) {
        b.insert(0, 0);
    }
    Ok(a.new_atom(&b)?)
}
pub fn curry(module: &[u8], arg: &[u8]) -> Result<Vec<u8>> {
    encode(|a| {
        let m = node_from_bytes(a, module)?;
        let p = node_from_bytes(a, arg)?;
        let one = uint(a, 1)?;
        let two = uint(a, 2)?;
        let four = uint(a, 4)?;
        let qm = a.new_pair(one, m)?;
        let qp = a.new_pair(one, p)?;
        let env = list(a, &[four, qp, one])?;
        list(a, &[two, qm, env])
    })
}
pub fn quoted(conditions: &[(u64, Vec<Vec<u8>>)]) -> Result<Vec<u8>> {
    encode(|a| {
        let mut cs = vec![];
        for (op, args) in conditions {
            let mut fields = vec![uint(a, *op)?];
            for x in args {
                fields.push(a.new_atom(x)?);
            }
            cs.push(list(a, &fields)?);
        }
        let c = list(a, &cs)?;
        let one = uint(a, 1)?;
        Ok(a.new_pair(one, c)?)
    })
}
pub fn amount(n: u64) -> Vec<u8> {
    let b = n.to_be_bytes();
    let i = b.iter().position(|v| *v != 0).unwrap_or(8);
    let mut v = b[i..].to_vec();
    if v.first().is_some_and(|v| v & 128 != 0) {
        v.insert(0, 0);
    }
    v
}
pub fn coin_json(c: Coin) -> Value {
    json!({"coin_id":hex::encode(c.coin_id()),"parent":hex::encode(c.parent_coin_info),"puzzle_hash":hex::encode(c.puzzle_hash),"amount":c.amount})
}
pub fn reject(sim: &mut Simulator, b: SpendBundle, code: ErrorCode, label: &str) -> Result<Value> {
    let before = sim.clone();
    let r = sim.new_transaction(b);
    ensure!(
        matches!(r,Err(SimulatorError::Validation(c)) if c==code),
        "{label}: expected {code:?}, got {r:?}"
    );
    ensure!(*sim == before, "{label}: full simulator changed");
    Ok(
        json!({"label":label,"code":format!("{code:?}"),"full_simulator_equal":true,"policy_rejection_credit":code!=ErrorCode::GeneratorRuntimeError,"classification":if code==ErrorCode::GeneratorRuntimeError{"inconclusive_runtime_umbrella"}else{"typed_consensus_rejection"}}),
    )
}
pub fn accept(sim: &mut Simulator, b: SpendBundle, expected: &[Coin]) -> Result<Value> {
    let height = sim.height();
    let metrics = crate::measurement::measure(&b, height)?;
    let removals = b.coin_spends.iter().map(|s| s.coin).collect::<Vec<_>>();
    sim.new_transaction(b)
        .map_err(|e| anyhow::anyhow!("accept: {e:?}"))?;
    ensure!(sim.height() == height + 1, "transaction height");
    let mut actual = vec![];
    for c in &removals {
        ensure!(
            sim.coin_state(c.coin_id()).unwrap().spent_height == Some(height),
            "spent height"
        );
        actual.extend(sim.children(c.coin_id()).into_iter().map(|s| s.coin));
    }
    actual.sort_by_key(|c| c.coin_id());
    let mut exp = expected.to_vec();
    exp.sort_by_key(|c| c.coin_id());
    ensure!(
        actual == exp,
        "exact children mismatch: {actual:?} vs {exp:?}"
    );
    for c in &exp {
        ensure!(
            sim.coin_state(c.coin_id()).unwrap().created_height == Some(height),
            "birth height"
        );
    }
    Ok(
        json!({"height":height,"removals":removals.into_iter().map(coin_json).collect::<Vec<_>>(),"outputs":exp.into_iter().map(coin_json).collect::<Vec<_>>(),"measurement":metrics}),
    )
}
pub fn totals(steps: &[Value]) -> Value {
    let mut out = serde_json::Map::new();
    for enc in ["uncompressed", "backrefs"] {
        let mut sums = serde_json::Map::new();
        for field in [
            "charged_bytes",
            "execution_cost",
            "condition_cost",
            "byte_cost",
            "total_cost",
        ] {
            let sum: u64 = steps
                .iter()
                .map(|s| {
                    s["measurement"]["generators"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|g| g["encoding"] == enc)
                        .unwrap()["metrics"][field]
                        .as_u64()
                        .unwrap()
                })
                .sum();
            sums.insert(field.into(), json!(sum));
        }
        out.insert(enc.into(), Value::Object(sums));
    }
    Value::Object(out)
}
/// VM condition bytes are checked separately from the subsequent consensus submission.
pub fn exact_conditions(
    reveal: &[u8],
    solution: &[u8],
    conditions: &[(u64, Vec<Vec<u8>>)],
) -> Result<()> {
    use clvmr::{ChiaDialect, chia_dialect::MEMPOOL_MODE, run_program};
    let mut a = Allocator::new();
    let p = node_from_bytes(&mut a, reveal)?;
    let s = node_from_bytes(&mut a, solution)?;
    let result = run_program(&mut a, &ChiaDialect::new(MEMPOOL_MODE), p, s, 100_000_000)
        .map_err(|e| anyhow::anyhow!("VM output check: {e:?}"))?;
    let quote = quoted(conditions)?;
    let mut expected_a = Allocator::new();
    let q = node_from_bytes(&mut expected_a, &quote)?;
    let clvmr::SExp::Pair(_, expected) = expected_a.sexp(q) else {
        unreachable!()
    };
    ensure!(
        node_to_bytes(&a, result.1)? == node_to_bytes(&expected_a, expected)?,
        "exact complete VM condition list mismatch"
    );
    Ok(())
}
