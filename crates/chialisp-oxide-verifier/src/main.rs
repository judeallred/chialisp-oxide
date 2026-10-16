use anyhow::{Context, Result, bail};
use chialisp_oxide_verifier::{SolverConfig, Specification, Verification, emit, replay, verify};

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [program_path, specification_path, options @ ..] = args.as_slice() else {
        bail!(
            "usage: chialisp-oxide-verifier PROGRAM.json SPECIFICATION.json [--emit]\n       chialisp-oxide-verifier PROGRAM.json --replay WITNESS.json"
        )
    };
    let program = serde_json::from_slice(&std::fs::read(program_path).context("read IR program")?)?;
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
