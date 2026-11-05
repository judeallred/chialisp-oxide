//! Finite forward-refinement regressions. These are actual VM comparisons, not
//! a general optimizer, termination, resource-cost or compiler-preservation proof.
use chialisp_oxide_ir::{
    BinaryOp, Environment, Expr, Function, Parameter, Program, Type, compile, optimize,
};
use chialisp_oxide_vm::{FailureCode, classify_failure};
use clvmr::{
    Allocator, ChiaDialect, MEMPOOL_MODE, run_program,
    serde::{node_from_bytes, node_to_bytes},
};

const LIMIT: u64 = 100_000;
const FINITE: u64 = 1_000_000;
fn u8_ty() -> Type {
    Type::Int {
        bits: 8,
        signed: false,
    }
}
fn arg(index: usize) -> Expr {
    Expr::Arg { index }
}
fn local(local_id: usize) -> Expr {
    Expr::Local { local_id }
}
fn int(value: u8) -> Expr {
    Expr::Int {
        value: value.to_string(),
        ty: u8_ty(),
    }
}
fn tree_atom(hex: &str) -> Expr {
    Expr::TreeAtom {
        value: Box::new(Expr::Atom { hex: hex.into() }),
    }
}
fn is_pair(value: Expr) -> Expr {
    Expr::TreeIsPair {
        value: Box::new(value),
    }
}
fn first(value: Expr) -> Expr {
    Expr::TreeFirst {
        value: Box::new(value),
    }
}
fn rest(value: Expr) -> Expr {
    Expr::TreeRest {
        value: Box::new(value),
    }
}
fn atom_value(value: Expr) -> Expr {
    Expr::TreeAtomValue {
        value: Box::new(value),
    }
}
fn seq(first: Expr, then_value: Expr) -> Expr {
    Expr::Sequence {
        first: Box::new(first),
        then_value: Box::new(then_value),
    }
}
fn branch(condition: Expr, then_value: Expr, else_value: Expr) -> Expr {
    Expr::If {
        condition: Box::new(condition),
        then_value: Box::new(then_value),
        else_value: Box::new(else_value),
    }
}
fn bind(local_id: usize, ty: Type, value: Expr, body: Expr) -> Expr {
    Expr::Let {
        local_id,
        ty,
        value: Box::new(value),
        body: Box::new(body),
    }
}
fn reject() -> Expr {
    Expr::Reject {
        message: "deliberate failure".into(),
    }
}
fn bottom_tree() -> Expr {
    seq(reject(), tree_atom(""))
}
fn call(function_id: usize, arguments: Vec<Expr>) -> Expr {
    Expr::Call {
        function_id,
        arguments,
    }
}
fn params(types: Vec<Type>) -> Vec<Parameter> {
    types
        .into_iter()
        .enumerate()
        .map(|(index, ty)| Parameter {
            name: format!("argument{index}"),
            ty,
        })
        .collect()
}
fn raw(body: Expr, result: Type, functions: Vec<Function>) -> Program {
    Program {
        name: "execution-facts".into(),
        environment: Environment::Raw,
        parameters: params(vec![Type::Tree]),
        result,
        body,
        functions,
    }
}
fn spin(result: Type) -> Function {
    Function {
        id: 0,
        name: "spin".into(),
        parameters: vec![],
        result,
        body: call(0, vec![]),
    }
}
fn tree_pair(first: Expr, rest: Expr) -> Expr {
    Expr::TreePair {
        first: Box::new(first),
        rest: Box::new(rest),
    }
}
fn field(value: Expr, index: usize) -> Expr {
    Expr::Field {
        value: Box::new(value),
        index,
    }
}

fn atom(bytes: &[u8]) -> Vec<u8> {
    match bytes {
        [] => vec![0x80],
        [byte] if *byte < 0x80 => vec![*byte],
        _ => {
            assert!(bytes.len() < 0x10_0000);
            let mut output = if bytes.len() < 64 {
                vec![0x80 | bytes.len() as u8]
            } else if bytes.len() < 8192 {
                vec![0xc0 | (bytes.len() >> 8) as u8, bytes.len() as u8]
            } else {
                vec![
                    0xe0 | (bytes.len() >> 16) as u8,
                    (bytes.len() >> 8) as u8,
                    bytes.len() as u8,
                ]
            };
            output.extend(bytes);
            output
        }
    }
}
fn pair(first: &[u8], rest: &[u8]) -> Vec<u8> {
    let mut output = vec![0xff];
    output.extend(first);
    output.extend(rest);
    output
}
fn list(values: &[Vec<u8>]) -> Vec<u8> {
    values
        .iter()
        .rev()
        .fold(atom(&[]), |rest, first| pair(first, &rest))
}

#[derive(Debug, PartialEq, Eq)]
enum Observation {
    Accepted(Vec<u8>),
    Failed(FailureCode),
}
fn run(code: &[u8], input: &[u8], limit: u64) -> Observation {
    let mut allocator = Allocator::new();
    let program = node_from_bytes(&mut allocator, code).unwrap();
    let input = node_from_bytes(&mut allocator, input).unwrap();
    match run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        program,
        input,
        limit,
    ) {
        Ok(result) => Observation::Accepted(node_to_bytes(&allocator, result.1).unwrap()),
        Err(error) => {
            let failure = classify_failure(&error);
            assert!(
                failure.is_program_rejection() || failure.code() == FailureCode::CostExceeded,
                "unexpected resource/infrastructure error: {failure:?} {error:?}"
            );
            Observation::Failed(failure.code())
        }
    }
}
fn forms(source: &Program) -> Vec<(&'static str, Vec<u8>)> {
    source.validate().expect("valid source fixture");
    let original_wire = serde_json::to_vec(source).unwrap();
    let rewritten = optimize(source).expect("forward refinement");
    assert_eq!(
        serde_json::to_vec(source).unwrap(),
        original_wire,
        "optimizer mutated its input"
    );
    rewritten.validate().unwrap();
    vec![
        ("source off", compile(source, false).unwrap()),
        ("source on", compile(source, true).unwrap()),
        ("rewritten off", compile(&rewritten, false).unwrap()),
        ("rewritten on", compile(&rewritten, true).unwrap()),
    ]
}
fn expect(forms: &[(&str, Vec<u8>)], input: &[u8], expected: &Observation, limit: u64) {
    for (label, code) in forms {
        assert_eq!(
            &run(code, input, limit),
            expected,
            "{label} input {}",
            hex::encode(input)
        );
    }
}
// Explicit f/r and Rue's fused environment path use different exact semantic
// errors in the pinned VM. Neither arbitrary Program failures nor resources
// satisfy this particular malformed-projection assertion.
fn expect_projection_rejection(forms: &[(&str, Vec<u8>)], input: &[u8]) {
    for (label, code) in forms {
        let actual = run(code, input, FINITE);
        assert!(
            matches!(
                actual,
                Observation::Failed(FailureCode::InvalidOpArg | FailureCode::PathIntoAtom)
            ),
            "{label}: expected checked projection rejection, got {actual:?}"
        );
    }
}
fn tests_count(expression: &Expr) -> usize {
    fn count(value: &serde_json::Value) -> usize {
        match value {
            serde_json::Value::Object(fields) => {
                usize::from(
                    fields.get("op").and_then(|value| value.as_str()) == Some("tree_is_pair"),
                ) + fields.values().map(count).sum::<usize>()
            }
            serde_json::Value::Array(items) => items.iter().map(count).sum(),
            _ => 0,
        }
    }
    count(&serde_json::to_value(expression).unwrap())
}

#[test]
fn executed_projection_refines_aliases_and_preserves_exact_padded_subtrees() {
    let source = raw(
        bind(
            0,
            Type::Tree,
            arg(0),
            seq(
                first(local(0)),
                branch(is_pair(arg(0)), rest(local(0)), reject()),
            ),
        ),
        Type::Tree,
        vec![],
    );
    assert_eq!(
        tests_count(&optimize(&source).unwrap().body),
        0,
        "later proven pair test should disappear"
    );
    let code = forms(&source);
    let reference = chialisp_oxide_reference::compile("(mod ENV (r ENV))").unwrap();
    for bytes in [&[][..], &[0], &[0, 0], &[0, 1], &[0x80], &[0xff]] {
        let tail = atom(bytes);
        let input = pair(&list(&[atom(&[8])]), &tail);
        let expected = Observation::Accepted(tail);
        expect(&code, &input, &expected, FINITE);
        assert_eq!(run(&reference, &input, FINITE), expected);
        expect_projection_rejection(&code, &atom(bytes));
    }
}

#[test]
fn earlier_guard_stays_before_divergence_even_when_projection_is_later() {
    let body = branch(
        is_pair(arg(0)),
        seq(call(0, vec![]), first(arg(0))),
        reject(),
    );
    let guarded = raw(body, Type::Tree, vec![spin(Type::Tree)]);
    assert_eq!(tests_count(&optimize(&guarded).unwrap().body), 1);
    let code = forms(&guarded);
    expect(
        &code,
        &atom(&[0]),
        &Observation::Failed(FailureCode::Raise),
        LIMIT,
    );
    expect(
        &code,
        &pair(&atom(&[]), &atom(&[])),
        &Observation::Failed(FailureCode::CostExceeded),
        LIMIT,
    );
    let unsafe_control = raw(
        seq(call(0, vec![]), first(arg(0))),
        Type::Tree,
        vec![spin(Type::Tree)],
    );
    expect(
        &forms(&unsafe_control),
        &atom(&[0]),
        &Observation::Failed(FailureCode::CostExceeded),
        LIMIT,
    );
}

#[test]
fn branch_facts_do_not_leak_but_explicit_failure_exits_do_not_pollute_continuations() {
    let both_return = raw(
        seq(
            branch(is_pair(arg(0)), seq(first(arg(0)), int(7)), int(8)),
            branch(is_pair(arg(0)), int(1), int(2)),
        ),
        u8_ty(),
        vec![],
    );
    let code = forms(&both_return);
    expect(
        &code,
        &atom(&[0, 0]),
        &Observation::Accepted(atom(&[2])),
        FINITE,
    );
    expect(
        &code,
        &pair(&atom(&[8]), &atom(&[0])),
        &Observation::Accepted(atom(&[1])),
        FINITE,
    );
    let guarded = raw(
        seq(
            branch(is_pair(arg(0)), int(7), reject()),
            branch(is_pair(arg(0)), int(1), reject()),
        ),
        u8_ty(),
        vec![],
    );
    assert!(tests_count(&optimize(&guarded).unwrap().body) < tests_count(&guarded.body));
    let code = forms(&guarded);
    expect(
        &code,
        &atom(&[]),
        &Observation::Failed(FailureCode::Raise),
        FINITE,
    );
    expect(
        &code,
        &pair(&atom(&[]), &atom(&[0])),
        &Observation::Accepted(atom(&[1])),
        FINITE,
    );
}

#[test]
fn argument_identity_local_reuse_and_fresh_callee_scope_remain_distinct() {
    let first_scope = bind(0, Type::Tree, arg(0), seq(first(local(0)), int(0)));
    let second_scope = bind(
        0,
        Type::Tree,
        arg(1),
        branch(is_pair(local(0)), int(1), int(2)),
    );
    let mut source = raw(seq(first_scope, second_scope), u8_ty(), vec![]);
    source.environment = Environment::Strict;
    source.parameters = params(vec![Type::Tree, Type::Tree]);
    let code = forms(&source);
    let tree = pair(&atom(&[]), &atom(&[0]));
    expect(
        &code,
        &list(&[tree.clone(), atom(&[0, 0])]),
        &Observation::Accepted(atom(&[2])),
        FINITE,
    );
    expect(
        &code,
        &list(&[tree.clone(), tree.clone()]),
        &Observation::Accepted(atom(&[1])),
        FINITE,
    );
    expect_projection_rejection(&code, &list(&[atom(&[0]), tree]));
    let helper = Function {
        id: 0,
        name: "fresh_argument0".into(),
        parameters: params(vec![Type::Tree]),
        result: u8_ty(),
        body: branch(is_pair(arg(0)), int(1), int(2)),
    };
    let source = raw(
        seq(first(arg(0)), call(0, vec![tree_atom("00")])),
        u8_ty(),
        vec![helper],
    );
    expect(
        &forms(&source),
        &pair(&atom(&[]), &atom(&[])),
        &Observation::Accepted(atom(&[2])),
        FINITE,
    );
}

#[test]
fn known_constructor_shapes_keep_strict_pair_tuple_and_condition_effects() {
    let rejecting_pair = tree_pair(bottom_tree(), tree_atom(""));
    let rejecting_field = field(
        Expr::Tuple {
            items: vec![tree_pair(tree_atom(""), tree_atom("00")), bottom_tree()],
        },
        0,
    );
    for value in [rejecting_pair, rejecting_field] {
        let source = raw(branch(is_pair(value), int(1), int(2)), u8_ty(), vec![]);
        assert_eq!(tests_count(&optimize(&source).unwrap().body), 0);
        expect(
            &forms(&source),
            &atom(&[]),
            &Observation::Failed(FailureCode::Raise),
            FINITE,
        );
    }
    let spinning_pair = tree_pair(seq(call(0, vec![]), tree_atom("")), tree_atom("00"));
    let source = raw(
        branch(is_pair(spinning_pair), int(1), int(2)),
        u8_ty(),
        vec![spin(Type::Tree)],
    );
    assert_eq!(tests_count(&optimize(&source).unwrap().body), 0);
    expect(
        &forms(&source),
        &atom(&[]),
        &Observation::Failed(FailureCode::CostExceeded),
        LIMIT,
    );
}

#[test]
fn atom_success_keeps_cast_types_sha_work_and_strict_unused_operands() {
    let source = raw(
        seq(
            Expr::Sha256 {
                parts: vec![atom_value(arg(0))],
            },
            branch(is_pair(arg(0)), reject(), int(7)),
        ),
        u8_ty(),
        vec![],
    );
    assert_eq!(tests_count(&optimize(&source).unwrap().body), 0);
    let code = forms(&source);
    for payload in [&[][..], &[0], &[0, 0], &[0, 1], &[0xff, 0x80]] {
        expect(
            &code,
            &atom(payload),
            &Observation::Accepted(atom(&[7])),
            FINITE,
        );
    }
    expect(
        &code,
        &pair(&atom(&[1]), &atom(&[])),
        &Observation::Failed(FailureCode::Raise),
        FINITE,
    );
    // SHA work remains observable even though its digest is unused. Cost
    // observations are separate: every variant exceeds this particular budget.
    expect(
        &code,
        &atom(&vec![0x78; 65_536]),
        &Observation::Failed(FailureCode::CostExceeded),
        LIMIT,
    );
    let bytes = Expr::Bytes {
        hex: "00".repeat(32),
    };
    let roundtrip = atom_value(Expr::TreeAtom {
        value: Box::new(local(0)),
    });
    let source = raw(
        bind(0, Type::Bytes { len: 32 }, bytes, roundtrip),
        Type::Atom,
        vec![],
    );
    expect(
        &forms(&source),
        &atom(&[0]),
        &Observation::Accepted(atom(&[0; 32])),
        FINITE,
    );
}

#[test]
fn boolean_and_integer_discriminant_aliases_refine_only_selected_branches() {
    for condition in [
        bind(
            0,
            Type::Bool,
            is_pair(arg(0)),
            branch(local(0), branch(is_pair(arg(0)), int(1), reject()), int(2)),
        ),
        bind(
            0,
            u8_ty(),
            branch(is_pair(arg(0)), int(1), int(0)),
            branch(
                Expr::Binary {
                    kind: BinaryOp::Eq,
                    left: Box::new(local(0)),
                    right: Box::new(int(1)),
                    ty: u8_ty(),
                },
                branch(is_pair(arg(0)), int(1), reject()),
                int(2),
            ),
        ),
    ] {
        let source = raw(condition, u8_ty(), vec![]);
        assert!(tests_count(&optimize(&source).unwrap().body) < tests_count(&source.body));
        let code = forms(&source);
        expect(
            &code,
            &atom(&[0, 0]),
            &Observation::Accepted(atom(&[2])),
            FINITE,
        );
        expect(
            &code,
            &pair(&atom(&[]), &atom(&[0])),
            &Observation::Accepted(atom(&[1])),
            FINITE,
        );
    }
}

#[test]
fn integer_tag_refinement_uses_values_not_decimal_literal_spellings() {
    // The expected equalities are explicit numeric oracles, not spellings or
    // outputs supplied by the optimizer. Both successful shape branches must
    // survive aliases, either operand order, and Eq/Ne inversion correctly.
    for (pair_tag, atom_tag, compared, signed, pair_equal, atom_equal) in [
        ("01", "2", "1", false, true, false),
        ("+1", "2", "1", false, true, false),
        ("-0", "2", "0", false, true, false),
        ("-01", "2", "-1", true, true, false),
        ("1", "02", "+2", false, false, true),
        ("01", "+1", "1", false, true, true),
    ] {
        let ty = Type::Int { bits: 8, signed };
        let literal = |text: &str| Expr::Int {
            value: text.into(),
            ty: ty.clone(),
        };
        for equal in [false, true] {
            for reverse_operands in [false, true] {
                let (left, right) = if reverse_operands {
                    (literal(compared), local(0))
                } else {
                    (local(0), literal(compared))
                };
                let source = raw(
                    bind(
                        0,
                        ty.clone(),
                        branch(is_pair(arg(0)), literal(pair_tag), literal(atom_tag)),
                        branch(
                            Expr::Binary {
                                kind: if equal { BinaryOp::Eq } else { BinaryOp::Ne },
                                left: Box::new(left),
                                right: Box::new(right),
                                ty: ty.clone(),
                            },
                            branch(is_pair(arg(0)), int(7), int(11)),
                            branch(is_pair(arg(0)), int(13), int(17)),
                        ),
                    ),
                    u8_ty(),
                    vec![],
                );
                let code = forms(&source);
                for (input, matches, yes, no) in [
                    (pair(&atom(&[0]), &atom(&[0, 1])), pair_equal, 7, 13),
                    (atom(&[0, 0]), atom_equal, 11, 17),
                ] {
                    let expected = if matches == equal { yes } else { no };
                    expect(
                        &code,
                        &input,
                        &Observation::Accepted(atom(&[expected])),
                        FINITE,
                    );
                }
            }
        }
    }
}

#[test]
fn projection_paths_and_successful_calls_do_not_forget_existing_facts() {
    let helper = Function {
        id: 0,
        name: "plain_return".into(),
        parameters: vec![],
        result: u8_ty(),
        body: int(3),
    };
    let body = bind(
        0,
        Type::Tree,
        rest(arg(0)),
        seq(
            first(local(0)),
            seq(
                call(0, vec![]),
                branch(is_pair(rest(arg(0))), rest(local(0)), reject()),
            ),
        ),
    );
    let source = raw(body, Type::Tree, vec![helper]);
    assert_eq!(tests_count(&optimize(&source).unwrap().body), 0);
    let expected = atom(&[0, 0, 1]);
    let input = pair(&atom(&[8]), &pair(&atom(&[0]), &expected));
    expect(
        &forms(&source),
        &input,
        &Observation::Accepted(expected),
        FINITE,
    );
}

#[test]
fn bounded_shape_matrix_preserves_lazy_branches_and_padded_data() {
    let source = raw(
        bind(
            0,
            Type::Tree,
            arg(0),
            branch(
                is_pair(local(0)),
                seq(
                    first(local(0)),
                    branch(is_pair(arg(0)), rest(arg(0)), reject()),
                ),
                arg(0),
            ),
        ),
        Type::Tree,
        vec![],
    );
    let code = forms(&source);
    let atoms = [
        atom(&[]),
        atom(&[0]),
        atom(&[0, 0]),
        atom(&[0, 1]),
        atom(&[0xff]),
        atom(&[8]),
    ];
    for atom in &atoms {
        expect(&code, atom, &Observation::Accepted(atom.clone()), FINITE);
    }
    for first in &atoms {
        for rest in &atoms {
            expect(
                &code,
                &pair(first, rest),
                &Observation::Accepted(rest.clone()),
                FINITE,
            );
            let nested = pair(&pair(first, rest), first);
            expect(
                &code,
                &nested,
                &Observation::Accepted(first.clone()),
                FINITE,
            );
        }
    }
}

#[test]
fn invalid_and_unreachable_public_ir_is_still_validated_before_rewriting() {
    let mut invalid = raw(
        branch(Expr::Bool { value: true }, int(7), arg(64)),
        u8_ty(),
        vec![],
    );
    assert!(optimize(&invalid).is_err());
    invalid.body = bind(0, Type::Tree, arg(0), bind(0, Type::Tree, arg(0), local(0)));
    invalid.result = Type::Tree;
    assert!(optimize(&invalid).is_err());
}
