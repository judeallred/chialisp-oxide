use std::{path::Path, process::Command};

use serde_json::{Value, json};

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_chialisp-oxide"))
}

fn batch(program: &str, inputs: Value, extra: &[&str]) -> std::process::Output {
    let directory = tempfile::tempdir().unwrap();
    let code = directory.path().join("retained.hex");
    let solutions = directory.path().join("inputs.json");
    std::fs::write(&code, program).unwrap();
    std::fs::write(&solutions, serde_json::to_vec(&inputs).unwrap()).unwrap();
    cli()
        .arg("run-batch")
        .arg(code)
        .arg("--solutions")
        .arg(solutions)
        .args(extra)
        .output()
        .unwrap()
}

fn ordinary(program: &Path, solution: &str, cost: &str) -> Value {
    let output = cli()
        .arg("run")
        .arg(program)
        .args(["--solution", solution, "--max-cost", cost])
        .output()
        .unwrap();
    assert!(matches!(output.status.code(), Some(0 | 2)));
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn ordered_batch_results_match_individual_execution_including_typed_rejections() {
    // Unquoted path2 selects first; an atom environment produces PathIntoAtom.
    let output = batch("02", json!(["(1 2)", "()", "(0x0001 . 0x00)", "(())"]), &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let values: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let code = directory.path().join("same.hex");
    std::fs::write(&code, "02").unwrap();
    let expected: Vec<_> = ["(1 2)", "()", "(0x0001 . 0x00)", "(())"]
        .iter()
        .map(|solution| ordinary(&code, solution, "1000000"))
        .collect();
    assert_eq!(values, expected);
    assert_eq!(values[1]["failure"]["code"], "path_into_atom");
    assert_eq!(values[2]["outcome"]["output_hex"], "820001");
}

#[test]
fn cost_exhaustion_remains_inconclusive_even_when_transport_succeeds() {
    let output = batch("ff01822a2a", json!(["()"]), &["--max-cost", "1"]);
    assert!(output.status.success());
    let values: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(values[0]["outcome"]["status"], "inconclusive");
    assert_eq!(values[0]["failure"]["kind"], "resource");
    assert_eq!(values[0]["failure"]["code"], "cost_exceeded");
}

#[test]
fn malformed_batches_publish_no_partial_observations() {
    for (input, flags) in [
        (json!([]), vec![]),
        (json!([1]), vec![]),
        (json!(true), vec![]),
        (json!(["()", "(unclosed"]), vec![]),
        (json!(["()"]), vec!["--max-cost", "0"]),
        (json!(["()"]), vec!["--max-cost", "100000001"]),
        (json!(vec!["()"; 4097]), vec![]),
    ] {
        let output = batch("01", input, &flags);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn bounded_input_read_rejects_an_oversized_file_before_publishing() {
    let output = batch("01", json!([" ".repeat(8 * 1024 * 1024)]), &[]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("byte limit"));
}

#[test]
fn solution_count_is_bounded_before_parsing_the_rest_of_a_wide_array() {
    let directory = tempfile::tempdir().unwrap();
    let code = directory.path().join("retained.hex");
    let inputs = directory.path().join("wide.json");
    std::fs::write(&code, "01").unwrap();
    let mut data = serde_json::to_string(&vec!["()"; 4096]).unwrap();
    data.pop();
    data.push_str(",\"()\",THIS_REST_MUST_NOT_BE_PARSED]");
    std::fs::write(&inputs, data).unwrap();
    let output = cli()
        .arg("run-batch")
        .arg(code)
        .arg("--solutions")
        .arg(inputs)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("4096-solution limit"));
}

#[test]
fn oversized_response_publishes_nothing_after_valid_executions() {
    // Quote one 1MiB atom: ff01 is (q . ...), f0100000 encodes its byte length.
    // The program fits the 4MiB hex input cap and each result fits the VM codec.
    let program = format!("ff01f0100000{}", "61".repeat(1024 * 1024));
    let single = batch(&program, json!(["()"]), &[]);
    assert!(
        single.status.success(),
        "{}",
        String::from_utf8_lossy(&single.stderr)
    );
    let values: Vec<Value> = serde_json::from_slice(&single.stdout).unwrap();
    assert_eq!(values[0]["outcome"]["status"], "accepted");
    assert_eq!(
        values[0]["outcome"]["output_hex"].as_str().unwrap().len(),
        2 * (1024 * 1024 + 4)
    );

    // Nine individually valid results exceed the aggregate 16MiB response cap.
    let output = batch(&program, json!(vec!["()"; 9]), &[]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("batch response exceeds"));
}
