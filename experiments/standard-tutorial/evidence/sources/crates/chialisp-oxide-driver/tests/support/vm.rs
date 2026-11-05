//! A negative source-semantics test requires a program rejection, not VM limits.
use chialisp_oxide_backend_experiment::{Execution, Outcome};

pub fn execute(program: &[u8], solution: &str, max_cost: u64) -> Result<Execution, String> {
    let execution = chialisp_oxide_backend_experiment::execute(program, solution, max_cost)
        .map_err(|error| format!("{error:#}"))?;
    if execution.outcome == Outcome::Inconclusive {
        return Err(format!(
            "source-semantics test is inconclusive: {execution:?}"
        ));
    }
    Ok(execution)
}
