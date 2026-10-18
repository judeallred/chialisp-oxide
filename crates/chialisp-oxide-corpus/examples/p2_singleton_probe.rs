//! Finite raw-domain and resource controls for the declared Cargo puzzle.
use anyhow::{Context, Result, ensure};
use chialisp_oxide_backend_experiment::FailureCode;
use chialisp_oxide_backend_experiment::{Outcome, artifact, execute};
use chialisp_oxide_ir::Program;
use clvmr::{Allocator, NodePtr, serde::node_to_bytes};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[path = "../src/native_hash.rs"]
mod native_hash;
#[path = "../src/native_tree.rs"]
mod native_tree;

const MAX_COST: u64 = 100_000_000;
const REVISION: &str = "cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8";

fn fingerprint(path: &Path) -> Result<String> {
    Ok(hex::encode(Sha256::digest(std::fs::read(path)?)))
}

fn list(allocator: &mut Allocator, items: &[NodePtr]) -> Result<NodePtr> {
    let mut result = NodePtr::NIL;
    for item in items.iter().rev() {
        result = allocator.new_pair(*item, result)?;
    }
    Ok(result)
}

fn condition(allocator: &mut Allocator, opcode: u64, value: NodePtr) -> Result<NodePtr> {
    ensure!(opcode < 128, "condition opcode exceeds its codec");
    let opcode = allocator.new_atom(&[u8::try_from(opcode)?])?;
    list(allocator, &[opcode, value])
}

fn native(solution: &str) -> Result<Outcome> {
    let mut allocator = Allocator::new();
    let environment = rue_clvm::assemble(&mut allocator, solution)?;
    let arena = bumpalo::Bump::new();
    // Bound every branch and ignored tail before source recursion/allocation.
    // Host resource and codec errors cannot become policy rejection.
    let tree = native_hash::decode(&arena, &allocator, environment)?;
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        chialisp_oxide_puzzles::p2_singleton::puzzle(tree)
    }));
    std::panic::set_hook(previous_hook);
    let Ok((assertion, announcement, coin_id)) = result else {
        return Ok(Outcome::Rejected);
    };
    let mut output = Allocator::new();
    let message = output.new_atom(&assertion.1)?;
    let assertion = condition(&mut output, assertion.0, message)?;
    let dollar = output.new_atom(&announcement.1)?;
    let announcement = condition(&mut output, announcement.0, dollar)?;
    let id = native_tree::encode(&mut output, coin_id.1)?;
    let coin_id = condition(&mut output, coin_id.0, id)?;
    let conditions = list(&mut output, &[assertion, announcement, coin_id])?;
    Ok(Outcome::Accepted {
        output_hex: hex::encode(node_to_bytes(&output, conditions)?),
    })
}

fn expected(case: &Value) -> Result<Outcome> {
    Ok(match case["expected"]["status"].as_str() {
        Some("accepted") => Outcome::Accepted {
            output_hex: case["expected"]["output_hex"]
                .as_str()
                .context("expected output")?
                .into(),
        },
        Some("rejected") => Outcome::Rejected,
        other => anyhow::bail!("unknown expected status {other:?}"),
    })
}

fn main() -> Result<()> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(
        arguments.len() == 2,
        "usage: p2_singleton_probe IR.json OUTPUT_DIRECTORY"
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let output = PathBuf::from(&arguments[1]);
    std::fs::create_dir_all(&output)?;
    let source_dir = root.join("puzzles/p2-singleton");
    let upstream = root.join("corpus/upstream/chia-puzzles").join(REVISION);
    let declared: Value =
        serde_json::from_slice(&std::fs::read(root.join("puzzles/authoring.json"))?)?;
    let compiled: Value = serde_json::from_str(include_str!(concat!(
        env!("OUT_DIR"),
        "/native-inputs.json"
    )))?;
    for input in compiled.as_array().context("compiled inputs")? {
        let path = input["path"].as_str().context("input path")?;
        ensure!(
            fingerprint(&root.join(path))? == input["sha256"],
            "stale native input {path}"
        );
    }
    ensure!(
        declared["inputs"]
            .as_array()
            .context("declared inputs")?
            .len()
            == compiled.as_array().unwrap().len(),
        "native input set changed"
    );
    let ir_path = PathBuf::from(&arguments[0]);
    let program: Program = serde_json::from_slice(&std::fs::read(&ir_path)?)?;
    program.validate()?;
    ensure!(
        program.name == "chialisp_oxide_puzzles::p2_singleton::puzzle",
        "wrong Cargo export"
    );
    let source = std::fs::read_to_string(source_dir.join("reference.clsp"))?;
    let codes = std::fs::read_to_string(upstream.join("puzzles/condition_codes.clib"))?;
    let curry = std::fs::read_to_string(upstream.join("puzzles/curry-and-treehash.clib"))?;
    let reference = chialisp_oxide_reference::compile_with_includes(
        &source,
        &[
            ("condition_codes.clib", &codes),
            ("curry-and-treehash.clib", &curry),
        ],
    )?;
    let published = hex::decode(
        std::fs::read_to_string(upstream.join("puzzles/p2_singleton.clsp.hex"))?.trim(),
    )?;
    ensure!(
        reference == published,
        "independent compilation did not reproduce published bytes"
    );
    ensure!(reference.len() == 403, "published module size changed");
    let programs = [
        (
            "rust_mir",
            false,
            chialisp_oxide_ir::compile(&program, false)?,
        ),
        (
            "rust_mir",
            true,
            chialisp_oxide_ir::compile(&program, true)?,
        ),
        ("chialisp", true, reference),
        (
            "rue_source",
            false,
            chialisp_oxide_rue::compile_file(&source_dir.join("puzzle.rue"), false)?,
        ),
        (
            "rue_source",
            true,
            chialisp_oxide_rue::compile_file(&source_dir.join("puzzle.rue"), true)?,
        ),
    ];
    let artifacts = programs
        .iter()
        .map(|(backend, optimized, bytes)| artifact(backend, *optimized, bytes))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        artifacts[2].tree_hash
            == "40f828d8dd55603f4ff9fbf6b73271e904e69406982f4fbefae2c8dcceaf9834",
        "canonical hash changed"
    );
    let vectors: Value = serde_json::from_slice(&std::fs::read(source_dir.join("vectors.json"))?)?;
    ensure!(
        vectors["named"].as_array().unwrap().len() == 50
            && vectors["generated"].as_array().unwrap().len() == 128,
        "independent vector set changed"
    );
    let mut observations = Vec::new();
    for category in ["named", "generated"] {
        for case in vectors[category].as_array().context("vector list")? {
            let solution = case["solution"].as_str().context("solution")?;
            let expected = expected(case)?;
            let actual = native(solution)?;
            ensure!(
                actual == expected,
                "native {}: {actual:?} != {expected:?}",
                case["label"]
            );
            let mut runs = Vec::new();
            for (backend, optimized, bytes) in &programs {
                let run = execute(bytes, solution, MAX_COST)?;
                ensure!(
                    run.outcome == expected,
                    "{backend}/{optimized} {}: {run:?} != {expected:?}",
                    case["label"]
                );
                if expected == Outcome::Rejected {
                    ensure!(
                        run.failure
                            .is_some_and(|failure| failure.is_program_rejection()),
                        "inconclusive rejection"
                    );
                }
                runs.push(json!({"backend":backend,"optimize":optimized,"execution":run}));
            }
            observations.push(json!({"category":category,"label":case["label"],"solution":solution,"expected":expected,"native":actual,"runs":runs}));
        }
    }
    // Complete Cartesian depth-two trees over exact nil/zero/padded atoms.
    // Every one lacks five right-pair projections and independently must fail.
    let mut atoms = vec!["()".to_string(), "0x00".into(), "0x0001".into()];
    let leaves = atoms.clone();
    for first in &leaves {
        for rest in &leaves {
            atoms.push(format!("({first} . {rest})"));
        }
    }
    let shallow = atoms.clone();
    for first in &shallow {
        for rest in &shallow {
            atoms.push(format!("({first} . {rest})"));
        }
    }
    atoms.sort();
    atoms.dedup();
    let mut generic = Vec::new();
    for solution in &atoms {
        ensure!(
            native(solution)? == Outcome::Rejected,
            "native accepted shallow tree"
        );
        let mut runs = Vec::new();
        for (backend, optimized, bytes) in &programs {
            let run = execute(bytes, solution, MAX_COST)?;
            ensure!(
                run.outcome == Outcome::Rejected
                    && run
                        .failure
                        .is_some_and(|failure| failure.is_program_rejection()),
                "nonconclusive shallow rejection: {run:?}"
            );
            runs.push(json!({"backend":backend,"optimize":optimized,"execution":run}));
        }
        generic.push(json!({"solution":solution,"native":"rejected","runs":runs}));
    }
    // Retain low-budget observations separately. Exhaustion cannot establish
    // semantic rejection, and these rows do not change the ordinary suite.
    let mut resource_controls = Vec::new();
    for index in [0, 5, 12, 19, 45, 46] {
        let case = &vectors["named"][index];
        let solution = case["solution"].as_str().unwrap();
        let expected = expected(case)?;
        ensure!(
            native(solution)? == expected,
            "resource control native relation changed"
        );
        let mut runs = Vec::new();
        for (backend, optimized, bytes) in &programs {
            let run = execute(bytes, solution, 100_000)?;
            ensure!(
                run.outcome == expected
                    || (run.outcome == Outcome::Inconclusive
                        && run
                            .failure
                            .is_some_and(|failure| failure.code() == FailureCode::CostExceeded)),
                "resource control changed semantics: {run:?}"
            );
            runs.push(json!({"backend":backend,"optimize":optimized,"execution":run}));
        }
        resource_controls.push(json!({"label":case["label"],"solution":solution,"expected":expected,"max_cost":100_000,"runs":runs}));
    }
    // Native preflight also covers data the policy ignores. This error remains
    // outside the source panic adapter and is never an Outcome::Rejected.
    let mut tail = "0x00".to_string();
    for _ in 0..257 {
        tail = format!("(0x00 . {tail})");
    }
    let host_input = format!("(() () () () () . {tail})");
    let host_error =
        native(&host_input).expect_err("whole-input host bound must include ignored tail");
    ensure!(
        host_error
            .to_string()
            .contains("native hash input depth exceeds"),
        "wrong host resource error: {host_error}"
    );
    let inputs = [
        "puzzles/p2-singleton/puzzle.rs",
        "puzzles/p2-singleton/puzzle.rue",
        "puzzles/p2-singleton/reference.clsp",
        "puzzles/p2-singleton/vectors.json",
        "crates/chialisp-oxide-corpus/examples/p2_singleton_probe.rs",
    ]
    .into_iter()
    .map(|path| Ok(json!({"path":path,"sha256":fingerprint(&root.join(path))?})))
    .collect::<Result<Vec<_>>>()?;
    std::fs::write(
        output.join("artifacts.json"),
        serde_json::to_vec_pretty(&artifacts)?,
    )?;
    let report = json!({"schema_version":1, "protocol":"chialisp-oxide-p2-singleton-raw/v1", "status":"validated", "scope":"Exact raw module domain, finite native/five-build comparisons and independent canonical artifact reproduction. No ledger, cryptographic or compiler-preservation theorem.", "ir":{"path":ir_path,"sha256":fingerprint(&PathBuf::from(&arguments[0]))?}, "native_declared_inputs":compiled,"inputs":inputs,"max_cost":MAX_COST,"artifacts":artifacts,"observations":observations,"generic_malformed":generic,"resource_controls":resource_controls,"native_ignored_tail_resource_error":{"input":host_input,"error":host_error.to_string(),"policy_rejection":false},"counts":{"named":50,"generated":128,"generic_malformed":atoms.len(),"ordinary_VM_executions":(178+atoms.len())*5,"ordinary_native_executions":178+atoms.len(),"diagnostic_resource_VM_executions":30,"resource_native_executions":6,"native_resource_errors":1}});
    std::fs::write(
        output.join("raw-domain.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "p2_singleton: independent exact 403-byte reference; 178 vectors + {} generic trees, native and five builds agree",
        atoms.len()
    );
    Ok(())
}
