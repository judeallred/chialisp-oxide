//! Finite backend-IR regressions for dynamic application (CLVM `a`): strict
//! operand bindings, failure propagation, lazy branches, nesting, validation and
//! wire versioning. This is not Rust source admission or a preservation proof.
#[path = "runtime_evaluation/support.rs"]
#[allow(
    dead_code,
    reason = "Shared fixtures also exercise unrelated IR operations."
)]
mod support;

use chialisp_oxide_ir::{Environment, Expr, Program, Type, compile};
use chialisp_oxide_vm::FailureCode;
use support::*;

fn apply(program: Expr, environment: Expr) -> Expr {
    Expr::Apply {
        program: Box::new(program),
        environment: Box::new(environment),
    }
}
fn fixture(types: Vec<Type>, result: Type, body: Expr) -> Program {
    program(Environment::Strict, types, result, body, vec![])
}
/// `(q . 7)` as serialized bytes: a quoted-constant program.
fn quoted_seven() -> Vec<u8> {
    pair(&atom(&[1]), &atom(&[7]))
}
/// `(x)` as serialized bytes: a program that raises.
fn raising() -> Vec<u8> {
    list(&[atom(&[8])])
}
/// `(+ 2 5)`: adds the environment's first two elements.
fn adder() -> Vec<u8> {
    list(&[atom(&[16]), atom(&[2]), atom(&[5])])
}

#[test]
fn applies_untrusted_program_to_untrusted_environment() {
    let two_trees = fixture(
        vec![Type::Tree, Type::Tree],
        Type::Tree,
        apply(arg(0), arg(1)),
    );
    assert!(two_trees.uses_runtime());
    assert!(two_trees.uses_apply());
    assert!(!two_trees.uses_sha256());
    for (optimize, code) in modes(&two_trees) {
        let environment = list(&[quoted_seven(), atom(&[])]);
        accepted(run(&code, &environment, 1_000_000), &atom(&[7]));
        // Program 1 returns its whole environment.
        let environment = list(&[atom(&[1]), list(&[atom(&[7])])]);
        accepted(run(&code, &environment, 1_000_000), &list(&[atom(&[7])]));
        let environment = list(&[adder(), list(&[atom(&[20]), atom(&[22])])]);
        accepted(run(&code, &environment, 1_000_000), &atom(&[42]));
        failed(
            run(&code, &list(&[raising(), atom(&[])]), 1_000_000),
            FailureCode::Raise,
        );
        // Strict ABI: exactly two arguments.
        abi_rejected(run(&code, &list(&[quoted_seven()]), 1_000_000));
        abi_rejected(run(
            &code,
            &list(&[quoted_seven(), atom(&[]), atom(&[])]),
            1_000_000,
        ));
        let _ = optimize;
    }
}

#[test]
fn unused_application_still_evaluates_and_rejects() {
    let unused = fixture(
        vec![Type::Tree, Type::Tree],
        Type::Int {
            bits: 8,
            signed: false,
        },
        binding(0, Type::Tree, apply(arg(0), arg(1)), int(7)),
    );
    for (_, code) in modes(&unused) {
        accepted(
            run(&code, &list(&[quoted_seven(), atom(&[])]), 1_000_000),
            &atom(&[7]),
        );
        failed(
            run(&code, &list(&[raising(), atom(&[])]), 1_000_000),
            FailureCode::Raise,
        );
    }
}

#[test]
fn unchosen_branch_stays_lazy_and_nesting_reapplies_the_result() {
    let lazy = fixture(
        vec![Type::Bool, Type::Tree, Type::Tree],
        Type::Tree,
        branch(arg(0), apply(arg(1), arg(2)), tree_atom("09")),
    );
    for (_, code) in modes(&lazy) {
        accepted(
            run(&code, &list(&[atom(&[]), raising(), atom(&[])]), 1_000_000),
            &atom(&[9]),
        );
        failed(
            run(&code, &list(&[atom(&[1]), raising(), atom(&[])]), 1_000_000),
            FailureCode::Raise,
        );
    }
    let nested = fixture(
        vec![Type::Tree, Type::Tree],
        Type::Tree,
        apply(apply(arg(0), arg(1)), arg(1)),
    );
    for (_, code) in modes(&nested) {
        // (q 1 . 7) produces (1 . 7), which then evaluates to 7.
        let producer = pair(&atom(&[1]), &quoted_seven());
        accepted(
            run(&code, &list(&[producer, atom(&[])]), 1_000_000),
            &atom(&[7]),
        );
    }
}

#[test]
fn operands_bind_in_order_before_the_application() {
    // The environment operand rejects; the program operand is evaluated first
    // but neither order lets the application run on a rejected operand.
    let program = fixture(
        vec![Type::Tree],
        Type::Tree,
        apply(arg(0), reject(&Type::Tree)),
    );
    for (_, code) in modes(&program) {
        failed(
            run(&code, &list(&[quoted_seven()]), 1_000_000),
            FailureCode::Raise,
        );
    }
}

#[test]
fn validation_requires_tree_operands_and_version_four_wire_format() {
    let bad = fixture(
        vec![Type::Int {
            bits: 8,
            signed: false,
        }],
        Type::Tree,
        apply(arg(0), tree_atom("")),
    );
    assert!(
        bad.validate()
            .unwrap_err()
            .to_string()
            .contains("apply operands must be arbitrary trees")
    );
    let good = fixture(
        vec![Type::Tree, Type::Tree],
        Type::Tree,
        apply(arg(0), arg(1)),
    );
    let envelope: serde_json::Value = serde_json::to_value(&good).unwrap();
    assert_eq!(envelope["ir_version"], 4);
    assert_eq!(envelope["program"]["body"]["op"], "apply");
    let mut legacy = envelope.clone();
    legacy["ir_version"] = serde_json::Value::from(3);
    assert!(serde_json::from_value::<Program>(legacy).is_err());
    let roundtrip: Program = serde_json::from_value(envelope).unwrap();
    assert!(roundtrip.uses_apply());
    for optimize in [false, true] {
        assert!(compile(&roundtrip, optimize).is_ok());
    }
}
