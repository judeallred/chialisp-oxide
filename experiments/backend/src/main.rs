use anyhow::Result;
use oxide_backend_experiment::*;
use serde_json::json;

fn main() -> Result<()> {
    let mut reports = Vec::new();
    for (backend, optimize, program) in backends()? {
        let mut cases = Vec::new();
        for solution in [
            "(0)", "(1)", "(1000)", "(-1)", "(1001)", "()", "((1))", "(1 2)", "(0x0001)",
        ] {
            cases.push(json!({ "solution": solution, "execution": execute(&program, solution, MAX_COST)? }));
        }
        reports.push(json!({ "artifact": artifact(backend, optimize, &program)?, "cases": cases }));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "experiment": "capped-payment-backends-v1", "rue_revision": RUE_REV,
            "runtime": "clvmr 0.21.0", "dialect": "MEMPOOL_MODE", "max_cost": MAX_COST,
            "limitations": ["No Rust frontend yet", "Handwritten CLVM reference, not compiled Chialisp", "No authorization or spend-bundle validation", "Not a production puzzle"],
            "backends": reports
        }))?
    );
    Ok(())
}
