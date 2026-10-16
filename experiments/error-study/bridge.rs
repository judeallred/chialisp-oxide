//! Execution bridge for trusted study fixtures; never compiles generated policy.
use chialisp_oxide_backend_experiment::{MAX_COST, artifact, execute};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["versions"] {
        println!(
            "{}",
            json!({"reference": chialisp_oxide_reference::COMPILER_VERSION,
            "rue_revision": chialisp_oxide_rue::RUE_REVISION})
        );
        return Ok(());
    }
    let mode = args.first().ok_or("expected chialisp or rue")?;
    let path = std::path::Path::new(args.get(1).ok_or("expected source path")?);
    let settings = if mode == "rue" {
        vec![false, true]
    } else {
        vec![true]
    };
    let mut results = Vec::new();
    for optimize in settings {
        let compilation = match mode.as_str() {
            "chialisp" => chialisp_oxide_reference::compile(&std::fs::read_to_string(path)?),
            "rue" => chialisp_oxide_rue::compile_file(path, optimize),
            _ => return Err("expected chialisp or rue".into()),
        };
        match compilation {
            Err(error) => results.push(json!({
                "optimize_lir": optimize, "status": "compile_rejected", "diagnostic": format!("{error:#}"),
            })),
            Ok(program) => {
                let mut runs = Vec::new();
                for input in &args[2..] {
                    runs.push(json!({"input": input, "execution": execute(&program, input, MAX_COST)?}));
                }
                results.push(json!({"optimize_lir": optimize, "status": "compiled",
                    "artifact": artifact(mode, optimize, &program)?, "runs": runs}));
            }
        }
    }
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}
