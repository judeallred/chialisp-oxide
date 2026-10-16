use chialisp_oxide_backend_experiment::*;
use chialisp_oxide_vm::{FailureCode, FailureKind};
use num_bigint::BigInt;
use proptest::prelude::*;
use std::sync::OnceLock;

fn programs() -> &'static Vec<(&'static str, bool, Vec<u8>)> {
    static PROGRAMS: OnceLock<Vec<(&'static str, bool, Vec<u8>)>> = OnceLock::new();
    PROGRAMS.get_or_init(|| backends().expect("compile all backends"))
}

fn expected(amount_atom: &str) -> Outcome {
    let mut a = clvmr::Allocator::new();
    let output = rue_clvm::assemble(
        &mut a,
        &format!("((51 0x{} {amount_atom}))", "00".repeat(32)),
    )
    .unwrap();
    Outcome::Accepted {
        output_hex: hex::encode(clvmr::serde::node_to_bytes(&a, output).unwrap()),
    }
}

fn check(solution: &str, expected: Outcome) {
    for (backend, optimize, program) in programs() {
        let execution = execute(program, solution, MAX_COST).unwrap();
        assert_eq!(
            execution.outcome, expected,
            "{backend} optimize={optimize}, solution={solution}, error={:?}",
            execution.error
        );
        if matches!(expected, Outcome::Accepted { .. }) {
            assert!(execution.cost.unwrap() < MAX_COST);
            assert!(execution.failure.is_none());
        } else {
            assert!(execution.failure.unwrap().is_program_rejection());
        }
    }
}

#[test]
fn exhaust_small_integer_domain() {
    for amount in -20..=1020 {
        check(
            &format!("({amount})"),
            if native(&BigInt::from(amount)) {
                expected(&amount.to_string())
            } else {
                Outcome::Rejected
            },
        );
    }
}

#[test]
fn malformed_solutions_and_large_integers() {
    for solution in [
        "()",
        "1",
        "(1 2)",
        "(1 . 2)",
        "((1))",
        "((1 . 2))",
        "(1 (2))",
        "(9223372036854775808)",
        "(-9223372036854775809)",
        "(0xffffffffffffffffffffffffffffffff)",
        "(0x00ffffffffffffffffffffffffffffff)",
    ] {
        check(solution, Outcome::Rejected);
    }
}

#[test]
fn numeric_atoms_preserve_noncanonical_encoding() {
    // CLVM accepts padded integer atoms. Native policy is numeric; outputs retain bytes.
    for atom in ["0x00", "0x0000", "0x0001", "0x0003e8"] {
        check(&format!("({atom})"), expected(atom));
    }
}

#[test]
fn cost_exhaustion_cannot_confirm_an_out_of_policy_rejection() {
    assert!(!native(&BigInt::from(1001)));
    for (_, _, program) in programs() {
        let result = execute(program, "(1001)", 1).unwrap();
        assert_eq!(result.outcome, Outcome::Inconclusive);
        let failure = result.failure.unwrap();
        assert_eq!(failure.kind(), FailureKind::Resource);
        assert_eq!(failure.code(), FailureCode::CostExceeded);
        let wire = serde_json::to_value(&result).unwrap();
        assert_eq!(wire["outcome"]["status"], "inconclusive");
        // Relabeling the same real resource failure must not publish a policy
        // rejection, even if a caller manually modifies the public record.
        let mut forged = result;
        forged.outcome = Outcome::Rejected;
        assert!(serde_json::to_value(forged).is_err());
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn random_i64_matches_native(amount in any::<i64>()) {
        check(&format!("({amount})"), if native(&BigInt::from(amount)) { expected(&amount.to_string()) } else { Outcome::Rejected });
    }
    #[test]
    fn random_valid_values_preserve_output(amount in 0i64..=1000) {
        check(&format!("({amount})"), expected(&amount.to_string()));
    }
}

#[derive(Clone, Debug)]
enum Tree {
    Atom(Vec<u8>),
    Pair(Box<Tree>, Box<Tree>),
}
impl Tree {
    fn text(&self) -> String {
        match self {
            Self::Atom(bytes) if bytes.is_empty() => "()".into(),
            Self::Atom(bytes) => format!("0x{}", hex::encode(bytes)),
            Self::Pair(left, right) => format!("({} . {})", left.text(), right.text()),
        }
    }
    fn oracle(&self) -> Outcome {
        if let Self::Pair(first, rest) = self
            && matches!(rest.as_ref(), Self::Atom(bytes) if bytes.is_empty())
            && let Self::Atom(bytes) = first.as_ref()
            && native(&BigInt::from_signed_bytes_be(bytes))
        {
            return expected(&first.text());
        }
        Outcome::Rejected
    }
}
fn trees() -> impl Strategy<Value = Tree> {
    proptest::collection::vec(any::<u8>(), 0..64)
        .prop_map(Tree::Atom)
        .prop_recursive(5, 128, 2, |inner| {
            (inner.clone(), inner)
                .prop_map(|(left, right)| Tree::Pair(Box::new(left), Box::new(right)))
        })
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]
    #[test]
    fn arbitrary_solution_trees_match_native_decoder(tree in trees()) {
        check(&tree.text(), tree.oracle());
    }
}

#[test]
fn weakened_upper_bound_is_detected_in_executable_clvm() {
    let original = std::fs::read_to_string(fixture("reference.clvm")).unwrap();
    assert!(original.contains("(q . 1001)"));
    let mutant = original.replace("(q . 1001)", "(q . 1002)");
    let mut a = clvmr::Allocator::new();
    let ptr = rue_clvm::assemble(&mut a, &mutant).unwrap();
    let program = clvmr::serde::node_to_bytes(&a, ptr).unwrap();
    assert!(!native(&1001.into()));
    assert_eq!(
        execute(&reference().unwrap(), "(1001)", MAX_COST)
            .unwrap()
            .outcome,
        Outcome::Rejected
    );
    assert_eq!(
        execute(&program, "(1001)", MAX_COST).unwrap().outcome,
        expected("1001")
    );
}

#[test]
fn zero_cost_does_not_disable_the_execution_budget() {
    assert!(execute(&reference().unwrap(), "(1)", 0).is_err());
}
