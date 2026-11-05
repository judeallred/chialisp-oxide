use anyhow::{Context, Result, bail};
use chialisp_oxide_verifier::{
    SolverConfig, Specification, StructuralBooleanVerification, TreeBooleanSpecification,
    TreeSpecification, TreeVerification, Verification, emit, emit_acyclic_scalar,
    emit_structural_boolean, emit_tree, emit_tree_equivalence, replay, replay_structural_boolean,
    replay_tree, verify, verify_acyclic_scalar, verify_structural_boolean, verify_tree,
    verify_tree_equivalence,
};

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [program_path, specification_path, options @ ..] = args.as_slice() else {
        bail!(
            "usage: chialisp-oxide-verifier PROGRAM.json SPECIFICATION.json [--emit]\n       chialisp-oxide-verifier PROGRAM.json --acyclic-scalar SPECIFICATION.json [--emit]\n       chialisp-oxide-verifier PROGRAM.json --replay WITNESS.json\n       chialisp-oxide-verifier PROGRAM.json --optimize-ir\n       chialisp-oxide-verifier PROGRAM.json --tree-equivalent OTHER.json [--emit-json]\n       chialisp-oxide-verifier PROGRAM.json --structural-boolean SPECIFICATION.json [--emit-json]\n       chialisp-oxide-verifier PROGRAM.json --structural-boolean-replay WITNESS.json\n       chialisp-oxide-verifier PROGRAM.json --artifacts"
        )
    };
    let program = serde_json::from_slice(&std::fs::read(program_path).context("read IR program")?)?;
    if specification_path == "--acyclic-scalar" {
        let [path, mode @ ..] = options else {
            bail!("--acyclic-scalar requires an independent scalar specification JSON file")
        };
        let specification: Specification = serde_json::from_slice(
            &std::fs::read(path).context("read acyclic scalar specification")?,
        )?;
        if mode == ["--emit"] {
            let queries = emit_acyclic_scalar(&program, &specification)?;
            println!(
                "; Nonvacuity\n{}\n(reset)\n; Counterexample\n{}",
                queries.nonvacuity, queries.counterexample
            );
        } else {
            anyhow::ensure!(mode.is_empty(), "unknown acyclic scalar option");
            let result = verify_acyclic_scalar(&program, &specification, &SolverConfig::default())?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"model": "acyclic_scalar_calls/v1", "scope": "typed scalar/tuple acceptance and output after ABI; no failure-order, cost/resource, or compiler-preservation theorem", "verification": result})
                )?
            );
            if !matches!(result, Verification::Proven { .. }) {
                std::process::exit(1)
            }
        }
        return Ok(());
    }
    if specification_path == "--structural-boolean-replay" {
        let [path] = options else {
            bail!("--structural-boolean-replay requires exactly one witness JSON file")
        };
        let witness =
            serde_json::from_slice(&std::fs::read(path).context("read environment witness")?)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&replay_structural_boolean(
                &program, &witness, 10_000_000
            )?)?
        );
        return Ok(());
    }
    if specification_path == "--structural-boolean" {
        let [path, mode @ ..] = options else {
            bail!("--structural-boolean requires an independent specification JSON file")
        };
        let specification: TreeBooleanSpecification =
            serde_json::from_slice(&std::fs::read(path).context("read Boolean specification")?)?;
        if mode == ["--emit-json"] {
            println!(
                "{}",
                serde_json::to_string_pretty(&emit_structural_boolean(&program, &specification)?)?
            );
        } else {
            anyhow::ensure!(mode.is_empty(), "unknown structural Boolean option");
            let result =
                verify_structural_boolean(&program, &specification, &SolverConfig::default())?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            if !matches!(result, StructuralBooleanVerification::Proven { .. }) {
                std::process::exit(1)
            }
        }
        return Ok(());
    }
    if specification_path == "--optimize-ir" {
        anyhow::ensure!(
            options.is_empty(),
            "--optimize-ir accepts no additional arguments"
        );
        println!(
            "{}",
            serde_json::to_string_pretty(&chialisp_oxide_ir::optimize(&program)?)?
        );
        return Ok(());
    }
    if specification_path == "--artifacts" {
        anyhow::ensure!(
            options.is_empty(),
            "--artifacts accepts no additional arguments"
        );
        let mut artifacts = Vec::new();
        for optimize in [false, true] {
            let bytes = chialisp_oxide_ir::compile(&program, optimize)?;
            let hash =
                clvmr::serde::tree_hash_from_stream(&mut std::io::Cursor::new(bytes.as_slice()))?;
            artifacts.push(serde_json::json!({
                "optimize": optimize,
                "bytes": bytes.len(),
                "clvm_hex": bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
                "tree_hash": hash.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
            }));
        }
        println!("{}", serde_json::to_string_pretty(&artifacts)?);
        return Ok(());
    }
    if specification_path == "--tree-equivalent" {
        let [path, mode @ ..] = options else {
            bail!("--tree-equivalent requires another structural IR JSON file")
        };
        let other = serde_json::from_slice(&std::fs::read(path).context("read comparison IR")?)?;
        if mode == ["--emit-json"] {
            println!(
                "{}",
                serde_json::to_string_pretty(&emit_tree_equivalence(&program, &other)?)?
            );
        } else {
            anyhow::ensure!(mode.is_empty(), "unknown structural equivalence option");
            let result = verify_tree_equivalence(&program, &other, &SolverConfig::default())?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            if !matches!(result, TreeVerification::Proven { .. }) {
                std::process::exit(1)
            }
        }
        return Ok(());
    }
    if specification_path == "--tree-replay" {
        let [witness_path] = options else {
            bail!("--tree-replay requires exactly one witness JSON file")
        };
        let witness =
            serde_json::from_slice(&std::fs::read(witness_path).context("read tree witness")?)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&replay_tree(&program, &witness, 10_000_000)?)?
        );
        return Ok(());
    }
    if specification_path == "--tree" {
        let [path, mode @ ..] = options else {
            bail!("--tree requires a structural specification JSON file")
        };
        let specification: TreeSpecification =
            serde_json::from_slice(&std::fs::read(path).context("read tree specification")?)?;
        if mode == ["--emit-json"] {
            println!(
                "{}",
                serde_json::to_string_pretty(&emit_tree(&program, &specification)?)?
            );
        } else if mode == ["--emit"] {
            let queries = emit_tree(&program, &specification)?;
            println!(
                "; Atom domain nonvacuity\n{}\n(reset)\n; Atom counterexample\n{}\n(reset)\n; Pair domain nonvacuity\n{}\n(reset)\n; Pair counterexample\n{}",
                queries.atom_nonvacuity,
                queries.atom_counterexample,
                queries.pair_nonvacuity,
                queries.pair_counterexample
            );
        } else {
            anyhow::ensure!(mode.is_empty(), "unknown structural option");
            let result = verify_tree(&program, &specification, &SolverConfig::default())?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            if !matches!(result, TreeVerification::Proven { .. }) {
                std::process::exit(1)
            }
        }
        return Ok(());
    }
    if specification_path == "--replay" {
        let [witness_path] = options else {
            bail!("--replay requires exactly one witness JSON file")
        };
        let witness =
            serde_json::from_slice(&std::fs::read(witness_path).context("read witness")?)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&replay(&program, &witness, 10_000_000)?)?
        );
        return Ok(());
    }
    let specification: Specification =
        serde_json::from_slice(&std::fs::read(specification_path).context("read specification")?)?;
    if options == ["--emit"] {
        let queries = emit(&program, &specification)?;
        println!(
            "; Nonvacuity query\n{}\n(reset)\n; Counterexample query\n{}",
            queries.nonvacuity, queries.counterexample
        );
    } else {
        anyhow::ensure!(options.is_empty(), "unknown option");
        let result = verify(&program, &specification, &SolverConfig::default())?;
        println!("{}", serde_json::to_string_pretty(&result)?);
        if !matches!(result, Verification::Proven { .. }) {
            std::process::exit(1);
        }
    }
    Ok(())
}
