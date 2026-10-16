use anyhow::{Context, Result, bail};
use chialisp_oxide_verifier::{
    SolverConfig, Specification, TreeSpecification, TreeVerification, Verification, emit,
    emit_tree, replay, replay_tree, verify, verify_tree,
};

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [program_path, specification_path, options @ ..] = args.as_slice() else {
        bail!(
            "usage: chialisp-oxide-verifier PROGRAM.json SPECIFICATION.json [--emit]\n       chialisp-oxide-verifier PROGRAM.json --replay WITNESS.json"
        )
    };
    let program = serde_json::from_slice(&std::fs::read(program_path).context("read IR program")?)?;
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
