//! Private host-side standard curry evidence, separate from the raw template.
//! This constructs CLVM data; it does not implement or replace the policy.
mod ledger;
use super::{CaseReport, ResourceControls, Run, expectation};
use anyhow::{Result, ensure};
use chialisp_oxide_backend_experiment::{
    Artifact, Execution, FailureCode, MAX_COST, Outcome, artifact, execute,
};
use clvmr::{
    Allocator, NodePtr, SExp,
    serde::{node_from_bytes, node_to_bytes},
};
use serde::Serialize;
use sha2::{Digest, Sha256};

const PUBLIC_PASSWORD: &[u8] = b"abc";
const CURRY_SOURCE: &str = "https://github.com/Chia-Network/chia_rs/blob/56f6b676a96de8659fe05d7e4f705df553f37fa5/crates/clvm-utils/src/curried_program.rs";

#[derive(Serialize)]
pub struct InstanceArtifact {
    pub module_tree_hash: String,
    pub artifact: Artifact,
}
#[derive(Serialize)]
pub struct VaryingRun {
    pub module_tree_hash: String,
    pub artifact: Artifact,
    pub execution: Execution,
}
#[derive(Serialize)]
pub struct VaryingArgument {
    pub argument_hex: String,
    pub password_hex: String,
    pub solution: String,
    pub environment: String,
    pub native: Outcome,
    pub runs: Vec<VaryingRun>,
}
#[derive(Serialize)]
pub struct CurriedInstances {
    pub protocol: &'static str,
    pub constructor: &'static str,
    pub source_role: &'static str,
    pub scheme: &'static str,
    pub source: &'static str,
    pub revision: &'static str,
    pub fixed_argument: String,
    pub artifacts: Vec<InstanceArtifact>,
    pub cases: Vec<CaseReport>,
    pub varying_arguments: Vec<VaryingArgument>,
    pub ledger: Vec<ledger::Comparison>,
}

fn list(allocator: &mut Allocator, values: &[NodePtr]) -> Result<NodePtr> {
    let mut tail = NodePtr::NIL;
    for value in values.iter().rev() {
        tail = allocator.new_pair(*value, tail)?;
    }
    Ok(tail)
}

/// Materialize exactly (a (q . M) (c (q . H) 1)). The argument is a serialized
/// tree rather than a hash-shaped byte parameter. Pinned independent checks
/// reconstruct both these bytes and their tree hash without this implementation.
fn curry(module: &[u8], argument: &[u8]) -> Result<Vec<u8>> {
    chialisp_oxide_simulator::program(module)?;
    chialisp_oxide_simulator::program(argument)?;
    let mut a = Allocator::new();
    let module = node_from_bytes(&mut a, module)?;
    let argument = node_from_bytes(&mut a, argument)?;
    let one = a.new_atom(&[1])?;
    let two = a.new_atom(&[2])?;
    let four = a.new_atom(&[4])?;
    let quoted_module = a.new_pair(one, module)?;
    let quoted_argument = a.new_pair(one, argument)?;
    let environment = list(&mut a, &[four, quoted_argument, one])?;
    let result = list(&mut a, &[two, quoted_module, environment])?;
    let bytes = node_to_bytes(&a, result)?;
    // The SDK's owned constructor and hash arithmetic must reproduce this
    // private construction exactly; the recorded constructor stays private.
    let arena = bumpalo::Bump::new();
    let sdk = {
        let tree = chialisp_oxide::curry::curry(
            super::curried::owned(&arena, &a, module),
            super::curried::owned(&arena, &a, argument),
        );
        let mut c = Allocator::new();
        let node = super::curried::encode(&mut c, &tree)?;
        node_to_bytes(&c, node)?
    };
    ensure!(
        sdk == bytes,
        "SDK owned curry differs from the private standard-tree constructor"
    );
    ensure!(
        chialisp_oxide::curry::curried_puzzle_hash(
            &super::curried::tree_hash(&a, module),
            &super::curried::tree_hash(&a, argument),
        ) == super::curried::tree_hash(&a, result),
        "SDK curried puzzle hash differs from the private instance tree hash"
    );
    Ok(bytes)
}

fn argument(password: &[u8]) -> Result<Vec<u8>> {
    let mut a = Allocator::new();
    let hash = a.new_atom(&Sha256::digest(password))?;
    Ok(node_to_bytes(&a, hash)?)
}

fn environment(argument: &[u8], solution: &str) -> Result<String> {
    let mut a = Allocator::new();
    let argument = node_from_bytes(&mut a, argument)?;
    let solution = rue_clvm::assemble(&mut a, solution)?;
    let environment = a.new_pair(argument, solution)?;
    Ok(rue_clvm::disassemble_value(&a, environment))
}

fn checked_run(code: &[u8], solution: &str, expected: &Outcome) -> Result<Execution> {
    let execution = execute(code, solution, MAX_COST)?;
    ensure!(
        execution.outcome != Outcome::Inconclusive,
        "inconclusive curried execution: {execution:?}"
    );
    ensure!(
        &execution.outcome == expected,
        "curried output differs from actual native H.S invocation: {execution:?}"
    );
    Ok(execution)
}

/// Separate finite resource observations. Different compiled programs may
/// perform different amounts of cryptographic work at this small budget; only
/// conclusive exact outputs count as ordinary policy evidence elsewhere.
pub(super) fn resource_controls(
    cases: &[CaseReport],
    programs: &[(&str, bool, Vec<u8>)],
) -> Result<ResourceControls> {
    const LOW_BUDGET: u64 = 100_000;
    let mut reports = vec![];
    for label in [
        "large_missing_conditions_match",
        "large_pair_hash",
        "large_password_success",
    ] {
        let mut matches = cases.iter().filter(|case| case.label == label);
        let case = matches
            .next()
            .ok_or_else(|| anyhow::anyhow!("missing password resource input {label}"))?;
        ensure!(
            matches.next().is_none(),
            "duplicate password resource input"
        );
        let native = expectation("password-coin", &case.solution)?;
        let mut runs = vec![];
        for (backend, optimize, code) in programs {
            let execution = execute(code, &case.solution, LOW_BUDGET)?;
            let exhausted = execution.outcome == Outcome::Inconclusive
                && execution
                    .failure
                    .is_some_and(|failure| failure.code() == FailureCode::CostExceeded);
            ensure!(
                execution.outcome == native || exhausted,
                "unexpected password resource observation {label} {backend}: {execution:?}"
            );
            runs.push(Run {
                backend: (*backend).into(),
                optimize_lir: *optimize,
                execution,
            });
        }
        reports.push(CaseReport {
            label: case.label.clone(),
            solution: case.solution.clone(),
            native,
            runs,
            declared_divergence: None,
        });
    }
    Ok(ResourceControls {
        max_cost: LOW_BUDGET,
        cases: reports,
    })
}

pub(super) fn validate(
    cases: &[CaseReport],
    programs: &[(&str, bool, Vec<u8>)],
) -> Result<CurriedInstances> {
    let fixed_argument = argument(PUBLIC_PASSWORD)?;
    let mut artifacts = vec![];
    let mut codes = vec![];
    for (backend, optimize, module) in programs {
        let code = curry(module, &fixed_argument)?;
        artifacts.push(InstanceArtifact {
            module_tree_hash: artifact(backend, *optimize, module)?.tree_hash,
            artifact: artifact(backend, *optimize, &code)?,
        });
        codes.push((*backend, *optimize, code));
    }
    let mut reports = vec![];
    for case in cases {
        let mut a = Allocator::new();
        let raw = rue_clvm::assemble(&mut a, &case.solution)?;
        // All cases deliberately use ONE fixed commitment. A malformed bare
        // input remains a malformed S; the raw template's varying H is dropped.
        let tail = match a.sexp(raw) {
            SExp::Pair(_, rest) => rest,
            SExp::Atom => raw,
        };
        let solution = rue_clvm::disassemble_value(&a, tail);
        let native = expectation("password-coin", &environment(&fixed_argument, &solution)?)?;
        let mut runs = vec![];
        for (backend, optimize, code) in &codes {
            runs.push(Run {
                backend: (*backend).into(),
                optimize_lir: *optimize,
                execution: checked_run(code, &solution, &native)?,
            });
        }
        reports.push(CaseReport {
            label: format!("derived tail: {}", case.label),
            solution,
            native,
            runs,
            declared_divergence: None,
        });
    }
    let mut varying_arguments = vec![];
    for password in [b"".as_slice(), PUBLIC_PASSWORD, &[0, 0]] {
        let argument = argument(password)?;
        let atom = if password.is_empty() {
            "()".into()
        } else {
            format!("0x{}", hex::encode(password))
        };
        // A program-shaped improper subtree is returned, never evaluated.
        let solution = format!("({atom} (8 . 0x0000) . 0x00)");
        let environment = environment(&argument, &solution)?;
        let native = expectation("password-coin", &environment)?;
        ensure!(
            matches!(&native, Outcome::Accepted { .. }),
            "varying commitment control must accept"
        );
        let mut runs = vec![];
        for (backend, optimize, module) in programs {
            let code = curry(module, &argument)?;
            runs.push(VaryingRun {
                module_tree_hash: artifact(backend, *optimize, module)?.tree_hash,
                artifact: artifact(backend, *optimize, &code)?,
                execution: checked_run(&code, &solution, &native)?,
            });
        }
        varying_arguments.push(VaryingArgument {
            argument_hex: hex::encode(argument),
            password_hex: hex::encode(password),
            solution,
            environment,
            native,
            runs,
        });
    }
    let mut ledger = vec![];
    for ((backend, optimize, module), (_, _, code)) in programs.iter().zip(&codes) {
        if *optimize {
            ledger.push(ledger::validate(backend, module, code, &fixed_argument)?);
        }
    }
    Ok(CurriedInstances {
        protocol: "chialisp-oxide-password-instances/v1",
        constructor: "private standard-tree constructor using clvmr 0.21.0; no clvm-utils call",
        source_role: "algorithm-reference provenance, not an executed library dependency",
        scheme: "(a (q . M) (c (q . H) 1))",
        source: CURRY_SOURCE,
        revision: "56f6b676a96de8659fe05d7e4f705df553f37fa5",
        fixed_argument: hex::encode(fixed_argument),
        artifacts,
        cases: reports,
        varying_arguments,
        ledger,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_curry_has_expected_literal_structure_and_does_not_run_data() {
        let module = [1]; // path 1 returns its entire environment
        for argument in [&[0x80][..], &[0x82, 0, 0][..], &[0xff, 8, 0][..]] {
            let code = curry(&module, argument).unwrap();
            let mut expected = vec![0xff, 2, 0xff, 0xff, 1, 1, 0xff, 0xff, 4, 0xff, 0xff, 1];
            expected.extend_from_slice(argument);
            expected.extend_from_slice(&[0xff, 1, 0x80, 0x80]);
            assert_eq!(code, expected);
            let expected_environment = environment(argument, "(0x0001 . 0x00)").unwrap();
            let mut a = Allocator::new();
            let expected = rue_clvm::assemble(&mut a, &expected_environment).unwrap();
            let expected = Outcome::Accepted {
                output_hex: hex::encode(node_to_bytes(&a, expected).unwrap()),
            };
            checked_run(&code, "(0x0001 . 0x00)", &expected).unwrap();
        }
        assert!(curry(&[1, 1], &[0x80]).is_err());
        assert!(curry(&[1], &[0x80, 1]).is_err());
    }

    #[test]
    fn independently_compiled_original_has_fixed_commitment_ledger_observations() {
        let module = chialisp_oxide_reference::compile(include_str!(
            "../../../puzzles/password-coin/reference.clsp"
        ))
        .unwrap();
        assert_eq!(module.len(), 31);
        assert_eq!(
            artifact("chialisp", true, &module).unwrap().tree_hash,
            "caf9f00763b55cfc71db469957fc5c21a4da350c4d5d36ab0a929de987b8eade"
        );
        let fixed = argument(PUBLIC_PASSWORD).unwrap();
        let code = curry(&module, &fixed).unwrap();
        let result = ledger::validate("chialisp", &module, &code, &fixed).unwrap();
        assert_eq!(result.transaction_attempts, 7);
        assert_eq!(
            result
                .attempts
                .iter()
                .filter(|row| row.consensus.status == "accepted")
                .count(),
            2
        );
        assert_eq!(
            result
                .attempts
                .iter()
                .filter(|row| row.consensus.semantic_rejection_credit)
                .count(),
            4
        );
        let wrong = &result.attempts[2];
        assert_eq!(wrong.consensus.status, "runtime_umbrella");
        assert!(!wrong.consensus.semantic_rejection_credit);
        assert!(wrong.full_state_unchanged);
    }

    #[test]
    fn raw_native_oracle_preserves_returned_bytes_and_host_limits_are_errors() {
        for password in [b"".as_slice(), b"\0", b"\0\0", &[0xff, 0x80]] {
            let hash = argument(password).unwrap();
            let password = if password.is_empty() {
                "()".into()
            } else {
                format!("0x{}", hex::encode(password))
            };
            let input = environment(&hash, &format!("({password} (8 . 0x0000) . 0x00)")).unwrap();
            assert_eq!(
                expectation("password-coin", &input).unwrap(),
                Outcome::Accepted {
                    output_hex: "ff08820000".into()
                }
            );
        }
        let hash = argument(PUBLIC_PASSWORD).unwrap();
        let mut ignored = "0x0001".to_string();
        for _ in 0..257 {
            ignored = format!("(0x00 . {ignored})");
        }
        let input = environment(&hash, &format!("(0x616263 () . {ignored})")).unwrap();
        let error = expectation("password-coin", &input)
            .unwrap_err()
            .to_string();
        assert!(error.contains("input depth exceeds"), "{error}");
        let oversized = format!("0x{}", "00".repeat(1_048_577));
        let input = environment(&hash, &format!("(0x616263 () . {oversized})")).unwrap();
        let error = expectation("password-coin", &input)
            .unwrap_err()
            .to_string();
        assert!(error.contains("atom bytes exceed"), "{error}");
    }
}
