use clvmr::{
    Allocator, ChiaDialect, MEMPOOL_MODE, error::EvalErr, run_program, serde::node_to_bytes,
};
use id_arena::Arena;
use rue_lir::{CodegenOptions, Lir};

#[derive(Clone, Copy, Debug)]
enum Operator {
    And,
    Or,
}

#[derive(Debug, PartialEq, Eq)]
enum Observation {
    Accepted(Vec<u8>),
    InvalidOpArg,
    CostExceeded,
}

fn observe(
    operator: Operator,
    optimize: bool,
    nested_first: bool,
    diverging_outer: bool,
    value: &str,
) -> Observation {
    let mut arena = Arena::new();
    let value_path = arena.alloc(Lir::Path(2_u8.into()));
    let one = arena.alloc(Lir::Atom(vec![1]));
    let nested = arena.alloc(match operator {
        Operator::And => Lir::Logand(vec![value_path, one]),
        Operator::Or => Lir::Logior(vec![value_path, one]),
    });
    let outer = if diverging_outer {
        let loop_path = arena.alloc(Lir::Path(3_u8.into()));
        arena.alloc(Lir::Run(loop_path, loop_path))
    } else {
        arena.alloc(Lir::Atom(vec![2]))
    };
    let arguments = if nested_first {
        vec![nested, outer]
    } else {
        vec![outer, nested]
    };
    let mut root = arena.alloc(match operator {
        Operator::And => Lir::Logand(arguments),
        Operator::Or => Lir::Logior(arguments),
    });
    if optimize {
        root = rue_lir::optimize(&mut arena, root);
    }
    let mut allocator = Allocator::new();
    let program = rue_lir::codegen(
        &arena,
        &mut allocator,
        root,
        CodegenOptions {
            optimize_static_pairs: false,
        },
    )
    .unwrap();
    // The tail is a self-applying CLVM program. It is executed only when the
    // outer operand runs, never by the test's host language.
    let environment = rue_clvm::assemble(&mut allocator, &format!("({value} . (a 1 1))")).unwrap();
    match run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        program,
        environment,
        1_000_000,
    ) {
        Ok(result) => Observation::Accepted(node_to_bytes(&allocator, result.1).unwrap()),
        Err(EvalErr::InvalidOpArg(_, _)) => Observation::InvalidOpArg,
        Err(EvalErr::CostExceeded) => Observation::CostExceeded,
        Err(error) => panic!("unexpected VM failure: {error:?}"),
    }
}

fn check_nested_integer_validation(operator: Operator) {
    // CLVM evaluates rightmost operands first. The inner operation must reject
    // these pair values before the outer left operand can diverge.
    for value in ["(1 . 2)", "(() . ())", "(0x0001 . 0x00)"] {
        for optimize in [false, true] {
            assert_eq!(
                observe(operator, optimize, false, true, value),
                Observation::InvalidOpArg,
                "operator={operator:?}, optimize={optimize}, value={value}",
            );
        }
    }
}

#[test]
fn nested_logand_keeps_its_integer_check_before_an_outer_divergence() {
    check_nested_integer_validation(Operator::And);
}

#[test]
fn nested_logior_keeps_its_integer_check_before_an_outer_divergence() {
    check_nested_integer_validation(Operator::Or);
}

#[test]
fn rightmost_divergence_still_precedes_the_inner_integer_check() {
    for operator in [Operator::And, Operator::Or] {
        for value in ["(1 . 2)", "(() . ())", "(0x0001 . 0x00)"] {
            for optimize in [false, true] {
                assert_eq!(
                    observe(operator, optimize, true, true, value),
                    Observation::CostExceeded,
                );
            }
        }
    }
}

#[test]
fn well_typed_atoms_do_not_create_a_spurious_rejection() {
    for operator in [Operator::And, Operator::Or] {
        for value in ["()", "0x00", "0x0001", "0xff", "0x0080", "0x80"] {
            for optimize in [false, true] {
                assert_eq!(
                    observe(operator, optimize, false, true, value),
                    Observation::CostExceeded,
                );
            }
        }
    }
}

#[test]
fn finite_numeric_and_malformed_controls_preserve_exact_results() {
    for operator in [Operator::And, Operator::Or] {
        for (value, or_result) in [
            ("()", vec![3]),
            ("0x00", vec![3]),
            ("0x0001", vec![3]),
            ("0xff", vec![0x81, 0xff]),
            ("0x0080", vec![0x82, 0x00, 0x83]),
            ("0x80", vec![0x81, 0x83]),
        ] {
            let expected = match operator {
                Operator::And => vec![0x80],
                Operator::Or => or_result,
            };
            for nested_first in [false, true] {
                for optimize in [false, true] {
                    assert_eq!(
                        observe(operator, optimize, nested_first, false, value),
                        Observation::Accepted(expected.clone()),
                    );
                }
            }
        }
        for nested_first in [false, true] {
            for optimize in [false, true] {
                assert_eq!(
                    observe(operator, optimize, nested_first, false, "(1 . 2)"),
                    Observation::InvalidOpArg,
                );
            }
        }
    }
}
