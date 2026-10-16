//! Independent bounded semantics checks, not an implementation of the optimizer.
use chialisp_oxide_ir::{BinaryOp, Environment, Expr, Parameter, Program, Type, compile, optimize};
use chialisp_oxide_vm::classify_failure;
use clvmr::{
    Allocator, ChiaDialect, MEMPOOL_MODE,
    allocator::SExp,
    run_program,
    serde::{node_from_bytes, node_to_bytes},
    treehash::{tree_hash_atom, tree_hash_pair},
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tree {
    Atom(Vec<u8>),
    Pair(Box<Tree>, Box<Tree>),
}
fn atom(bytes: &[u8]) -> Tree {
    Tree::Atom(bytes.to_vec())
}
fn pair(first: Tree, rest: Tree) -> Tree {
    Tree::Pair(Box::new(first), Box::new(rest))
}
fn list(items: &[Tree]) -> Tree {
    items
        .iter()
        .rev()
        .fold(atom(&[]), |rest, first| pair(first.clone(), rest))
}

// Deliberately independent of the project's native codec and Rue assembler.
fn serialize(tree: &Tree) -> Vec<u8> {
    match tree {
        Tree::Pair(first, rest) => {
            let mut result = vec![255];
            result.extend(serialize(first));
            result.extend(serialize(rest));
            result
        }
        Tree::Atom(bytes) => {
            let mut result = match bytes.as_slice() {
                [] => return vec![128],
                [byte] if *byte < 128 => return vec![*byte],
                _ if bytes.len() < 64 => vec![128 | bytes.len() as u8],
                _ if bytes.len() < 8192 => {
                    vec![192 | (bytes.len() >> 8) as u8, bytes.len() as u8]
                }
                _ => panic!("outside this test's bounded atom grammar"),
            };
            result.extend(bytes);
            result
        }
    }
}

#[derive(Clone, Debug)]
enum Value {
    Tree(Tree),
    Bool(bool),
    Int(u8),
    Bytes(Vec<u8>),
    Tuple(Vec<Value>),
}
fn data(value: Value) -> Tree {
    match value {
        Value::Tree(tree) => tree,
        _ => panic!("ill-typed test expression"),
    }
}
fn boolean(value: Value) -> bool {
    match value {
        Value::Bool(value) => value,
        _ => panic!("ill-typed test condition"),
    }
}
fn integer(value: Value) -> u8 {
    match value {
        Value::Int(value) => value,
        _ => panic!("ill-typed test arithmetic"),
    }
}
fn encode(value: Value) -> Tree {
    match value {
        Value::Tree(tree) => tree,
        Value::Bool(value) => atom(if value { &[1] } else { &[] }),
        Value::Int(0) => atom(&[]),
        Value::Int(value) if value < 128 => atom(&[value]),
        Value::Int(value) => atom(&[0, value]),
        Value::Bytes(bytes) => Tree::Atom(bytes),
        Value::Tuple(items) => list(&items.into_iter().map(encode).collect::<Vec<_>>()),
    }
}

// Checked evaluation of the scoped expression grammar. Every pair field and
// Sequence operand evaluates strictly; If evaluates just its selected branch.
fn evaluate(expression: &Expr, arguments: &[Value]) -> Option<Value> {
    let value = match expression {
        Expr::Arg { index } => arguments[*index].clone(),
        Expr::Atom { hex } => Value::Tree(Tree::Atom(hex::decode(hex).unwrap())),
        Expr::Bytes { hex } => Value::Bytes(hex::decode(hex).unwrap()),
        Expr::Int { value, ty } => {
            assert_eq!(*ty, byte_type());
            Value::Int(value.parse().unwrap())
        }
        Expr::TreeAtom { value } => {
            let tree = match evaluate(value, arguments)? {
                Value::Tree(tree) => tree,
                Value::Bytes(bytes) => Tree::Atom(bytes),
                _ => panic!("ill-typed tree atom"),
            };
            assert!(matches!(tree, Tree::Atom(_)));
            Value::Tree(tree)
        }
        Expr::TreePair { first, rest } => Value::Tree(pair(
            data(evaluate(first, arguments)?),
            data(evaluate(rest, arguments)?),
        )),
        Expr::TreeFirst { value } | Expr::TreeRest { value } => {
            let Tree::Pair(first, rest) = data(evaluate(value, arguments)?) else {
                return None;
            };
            Value::Tree(if matches!(expression, Expr::TreeFirst { .. }) {
                *first
            } else {
                *rest
            })
        }
        Expr::TreeAtomValue { value } => {
            let tree = data(evaluate(value, arguments)?);
            if matches!(tree, Tree::Pair(..)) {
                return None;
            }
            Value::Tree(tree)
        }
        Expr::TreeIsPair { value } => {
            Value::Bool(matches!(data(evaluate(value, arguments)?), Tree::Pair(..)))
        }
        Expr::Bool { value } => Value::Bool(*value),
        Expr::Tuple { items } => Value::Tuple(
            items
                .iter()
                .map(|item| evaluate(item, arguments))
                .collect::<Option<_>>()?,
        ),
        Expr::Field { value, index } => match evaluate(value, arguments)? {
            Value::Tuple(items) => items[*index].clone(),
            Value::Bytes(bytes) => Value::Int(bytes[*index]),
            _ => panic!("ill-typed test field"),
        },
        Expr::Binary {
            kind,
            left,
            right,
            ty,
        } => {
            assert_eq!(*kind, BinaryOp::Add);
            assert_eq!(*ty, byte_type());
            Value::Int(
                integer(evaluate(left, arguments)?)
                    .checked_add(integer(evaluate(right, arguments)?))?,
            )
        }
        Expr::Not { value } => Value::Bool(!boolean(evaluate(value, arguments)?)),
        Expr::If {
            condition,
            then_value,
            else_value,
        } => {
            return evaluate(
                if boolean(evaluate(condition, arguments)?) {
                    then_value
                } else {
                    else_value
                },
                arguments,
            );
        }
        Expr::Sequence { first, then_value } => {
            evaluate(first, arguments)?;
            return evaluate(then_value, arguments);
        }
        Expr::Reject { .. } => return None,
        _ => panic!("outside this test's expression grammar: {expression:?}"),
    };
    Some(value)
}
fn interpret(program: &Program, environment: &Tree) -> Option<Vec<u8>> {
    let arguments = match program.environment {
        Environment::Raw => vec![Value::Tree(environment.clone())],
        Environment::Ignored => vec![],
        Environment::Strict => {
            let mut arguments = vec![];
            let mut remaining = environment;
            for parameter in &program.parameters {
                let Tree::Pair(first, rest) = remaining else {
                    return None;
                };
                arguments.push(match &parameter.ty {
                    Type::Tree => Value::Tree((**first).clone()),
                    Type::Int {
                        bits: 8,
                        signed: false,
                    } => {
                        let Tree::Atom(bytes) = &**first else {
                            return None;
                        };
                        // Exactly the canonical signed-CLVM encoding of u8.
                        Value::Int(match bytes.as_slice() {
                            [] => 0,
                            [value @ 1..=127] => *value,
                            [0, value @ 128..=255] => *value,
                            _ => return None,
                        })
                    }
                    Type::Bytes { len } => {
                        let Tree::Atom(bytes) = &**first else {
                            return None;
                        };
                        if bytes.len() != *len {
                            return None;
                        }
                        Value::Bytes(bytes.clone())
                    }
                    _ => panic!("outside this test's argument grammar"),
                });
                remaining = rest;
            }
            if remaining != &atom(&[]) {
                return None;
            }
            arguments
        }
    };
    Some(serialize(&encode(evaluate(&program.body, &arguments)?)))
}
fn execute(program: &[u8], environment: &Tree) -> Option<Vec<u8>> {
    let mut allocator = Allocator::new();
    let program = node_from_bytes(&mut allocator, program).unwrap();
    let environment = node_from_bytes(&mut allocator, &serialize(environment)).unwrap();
    match run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        program,
        environment,
        10_000_000,
    ) {
        Ok(result) => Some(node_to_bytes(&allocator, result.1).unwrap()),
        Err(error) => {
            let failure = classify_failure(&error);
            assert!(
                failure.is_program_rejection(),
                "inconclusive VM failure: {failure:?}: {error:?}"
            );
            None
        }
    }
}
fn program(body: Expr, arguments: usize, raw: bool) -> Program {
    Program {
        name: "independent-tree-optimization-study".into(),
        environment: if raw {
            Environment::Raw
        } else {
            Environment::Strict
        },
        parameters: (0..arguments)
            .map(|index| Parameter {
                name: format!("tree{index}"),
                ty: Type::Tree,
            })
            .collect(),
        result: Type::Tree,
        body,
    }
}
fn arg(index: usize) -> Expr {
    Expr::Arg { index }
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
fn is_pair(value: Expr) -> Expr {
    Expr::TreeIsPair {
        value: Box::new(value),
    }
}
fn tree(hex: &str) -> Expr {
    Expr::TreeAtom {
        value: Box::new(Expr::Atom { hex: hex.into() }),
    }
}
fn reject() -> Expr {
    Expr::Reject {
        message: "required rejection".into(),
    }
}
fn choose(condition: Expr, then_value: Expr, else_value: Expr) -> Expr {
    Expr::If {
        condition: Box::new(condition),
        then_value: Box::new(then_value),
        else_value: Box::new(else_value),
    }
}
fn sequence(first: Expr, then_value: Expr) -> Expr {
    Expr::Sequence {
        first: Box::new(first),
        then_value: Box::new(then_value),
    }
}
fn cons(first: Expr, rest: Expr) -> Expr {
    Expr::TreePair {
        first: Box::new(first),
        rest: Box::new(rest),
    }
}
fn byte_type() -> Type {
    Type::Int {
        bits: 8,
        signed: false,
    }
}
fn number(value: u8) -> Expr {
    Expr::Int {
        value: value.to_string(),
        ty: byte_type(),
    }
}
fn add(left: Expr, right: Expr) -> Expr {
    Expr::Binary {
        kind: BinaryOp::Add,
        left: Box::new(left),
        right: Box::new(right),
        ty: byte_type(),
    }
}
fn field(value: Expr, index: usize) -> Expr {
    Expr::Field {
        value: Box::new(value),
        index,
    }
}

fn compare(program: &Program, environments: &[Tree]) {
    program.validate().unwrap();
    let optimized = optimize(program).unwrap();
    optimized.validate().unwrap();
    let repeated = optimize(&optimized).unwrap();
    repeated.validate().unwrap();
    let builds = [
        compile(program, false).unwrap(),
        compile(program, true).unwrap(),
        compile(&optimized, false).unwrap(),
        compile(&optimized, true).unwrap(),
    ];
    for environment in environments {
        let expected = interpret(program, environment);
        assert_eq!(
            interpret(&optimized, environment),
            expected,
            "IR semantics changed: {program:?}, environment={environment:?}",
        );
        assert_eq!(
            interpret(&repeated, environment),
            expected,
            "repeated optimization changed semantics: {program:?}, environment={environment:?}",
        );
        for (build, bytes) in builds.iter().enumerate() {
            assert_eq!(
                execute(bytes, environment),
                expected,
                "build={build}, program={program:?}, environment={environment:?}",
            );
        }
    }
}

#[test]
fn independent_interpreter_agrees_with_explicit_output_and_rejection_examples() {
    let factory = choose(is_pair(arg(0)), cons(tree("01"), first(arg(0))), reject());
    let cases = [
        (program(factory.clone(), 1, true), atom(&[]), None),
        (
            program(factory, 1, true),
            pair(atom(&[0, 1]), atom(&[7])),
            Some("ff01820001"),
        ),
        (
            program(cons(first(arg(0)), rest(arg(0))), 1, true),
            atom(&[0, 1]),
            None,
        ),
        (
            program(cons(first(arg(0)), rest(arg(0))), 1, true),
            pair(atom(&[0, 1]), atom(&[0])),
            Some("ff82000100"),
        ),
        (
            program(
                choose(
                    is_pair(arg(0)),
                    choose(Expr::Bool { value: true }, arg(0), first(arg(0))),
                    reject(),
                ),
                1,
                true,
            ),
            atom(&[]),
            None,
        ),
        (
            program(choose(is_pair(arg(0)), first(arg(1)), reject()), 2, false),
            list(&[atom(&[1]), pair(atom(&[1]), atom(&[2]))]),
            None,
        ),
        (
            program(
                choose(
                    is_pair(sequence(first(arg(1)), arg(0))),
                    first(arg(0)),
                    reject(),
                ),
                2,
                false,
            ),
            list(&[pair(atom(&[1]), atom(&[2])), atom(&[1])]),
            None,
        ),
        (
            program(choose(is_pair(arg(0)), first(arg(0)), tree("00")), 1, true),
            atom(&[]),
            Some("00"),
        ),
    ];
    for (program, environment, expected) in cases {
        assert_eq!(
            interpret(&program, &environment),
            expected.map(|hex| hex::decode(hex).unwrap()),
        );
        compare(&program, &[environment]);
    }
}

#[test]
fn tuple_fields_and_discarded_scalar_work_keep_their_strict_effects() {
    let selected_tuple_field = || {
        field(
            Expr::Tuple {
                items: vec![arg(0), first(arg(1))],
            },
            0,
        )
    };
    let scalar_effect = |initial| {
        sequence(
            add(sequence(rest(arg(0)), number(initial)), number(1)),
            tree("0001"),
        )
    };
    let byte_field = || {
        field(
            sequence(rest(arg(0)), Expr::Bytes { hex: "8000".into() }),
            0,
        )
    };
    let cases = [
        (
            program(
                choose(is_pair(arg(1)), selected_tuple_field(), reject()),
                2,
                false,
            ),
            list(&[atom(&[0, 1]), atom(&[])]),
            None,
        ),
        (
            program(
                choose(is_pair(arg(1)), selected_tuple_field(), reject()),
                2,
                false,
            ),
            list(&[atom(&[0, 1]), pair(atom(&[]), atom(&[2]))]),
            Some("820001"),
        ),
        (
            program(
                choose(is_pair(arg(0)), selected_tuple_field(), reject()),
                2,
                false,
            ),
            list(&[atom(&[0, 1]), pair(atom(&[]), atom(&[2]))]),
            None,
        ),
        (
            program(
                choose(is_pair(arg(0)), scalar_effect(254), reject()),
                1,
                true,
            ),
            pair(atom(&[]), atom(&[0, 1])),
            Some("820001"),
        ),
        (
            program(
                choose(is_pair(arg(0)), scalar_effect(254), reject()),
                1,
                true,
            ),
            atom(&[]),
            None,
        ),
        (
            program(
                choose(is_pair(arg(0)), scalar_effect(255), reject()),
                1,
                true,
            ),
            pair(atom(&[]), atom(&[0, 1])),
            None,
        ),
        (
            program(
                choose(
                    is_pair(arg(0)),
                    sequence(byte_field(), tree("0001")),
                    reject(),
                ),
                1,
                true,
            ),
            pair(atom(&[]), atom(&[])),
            Some("820001"),
        ),
        (
            program(
                choose(
                    is_pair(arg(0)),
                    sequence(byte_field(), tree("0001")),
                    reject(),
                ),
                1,
                true,
            ),
            atom(&[0, 1]),
            None,
        ),
        (
            program(
                choose(
                    is_pair(arg(0)),
                    sequence(add(byte_field(), number(128)), tree("0001")),
                    reject(),
                ),
                1,
                true,
            ),
            pair(atom(&[]), atom(&[])),
            None, // Byte 0x80 is unsigned 128; adding 128 overflows u8.
        ),
    ];
    for (program, environment, expected) in cases {
        assert_eq!(
            interpret(&program, &environment),
            expected.map(|hex| hex::decode(hex).unwrap()),
        );
        compare(&program, &[environment]);
    }
}

#[test]
fn scalar_abi_canonicality_and_overflow_survive_discarded_binary_results() {
    let mut program = program(
        choose(
            is_pair(arg(0)),
            sequence(add(sequence(rest(arg(0)), arg(1)), number(1)), tree("0001")),
            reject(),
        ),
        2,
        false,
    );
    program.parameters[1].ty = byte_type();
    for (scalar, expected) in [
        (atom(&[]), Some("820001")),
        (atom(&[1]), Some("820001")),
        (atom(&[0, 254]), Some("820001")),
        (atom(&[0, 255]), None),
        (atom(&[0, 1]), None),
        (atom(&[0]), None),
        (atom(&[1, 0]), None),
        (atom(&[255]), None),
        (pair(atom(&[]), atom(&[])), None),
    ] {
        let environment = list(&[pair(atom(&[]), atom(&[0, 1])), scalar]);
        assert_eq!(
            interpret(&program, &environment),
            expected.map(|hex| hex::decode(hex).unwrap()),
        );
        compare(&program, &[environment]);
    }
}

#[test]
fn highest_argument_bit_and_invalid_public_inputs_do_not_shift_outside_the_mask() {
    // Exercise the public IR analysis at its 64-argument boundary independently
    // of backend code-expansion limits; the VM matrix uses smaller programs.
    let program = program(
        choose(is_pair(arg(63)), first(arg(63)), reject()),
        64,
        false,
    );
    let optimized = optimize(&program).unwrap();
    for (last, expected) in [
        (atom(&[0, 1]), None),
        (pair(atom(&[0, 1]), atom(&[255])), Some("820001")),
    ] {
        let mut values = vec![atom(&[]); 64];
        values[0] = pair(atom(&[1]), atom(&[2]));
        values[63] = last;
        let environment = list(&values);
        let expected = expected.map(|hex| hex::decode(hex).unwrap());
        assert_eq!(interpret(&program, &environment), expected);
        assert_eq!(interpret(&optimized, &environment), expected);
        let mut wrong_argument = program.clone();
        wrong_argument.body = choose(is_pair(arg(63)), first(arg(0)), reject());
        let rewritten = optimize(&wrong_argument).unwrap();
        assert_eq!(
            interpret(&rewritten, &environment),
            interpret(&wrong_argument, &environment),
        );
    }
    let mut invalid = program.clone();
    invalid.body = first(arg(64));
    assert!(optimize(&invalid).is_err());
    invalid.body = first(arg(usize::MAX));
    assert!(optimize(&invalid).is_err());
    invalid = program;
    invalid.parameters.push(Parameter {
        name: "too_many".into(),
        ty: Type::Tree,
    });
    assert!(optimize(&invalid).is_err());
}

#[test]
fn canonical_factory_reaches_the_exact_published_module_without_changing_its_contract() {
    let program = program(
        choose(is_pair(arg(0)), cons(tree("01"), first(arg(0))), reject()),
        1,
        true,
    );
    let bytes = compile(&program, true).unwrap();
    assert_eq!(hex::encode(&bytes), "ff04ffff0101ff0280");
    fn hash(allocator: &Allocator, node: clvmr::NodePtr) -> [u8; 32] {
        match allocator.sexp(node) {
            SExp::Atom => tree_hash_atom(allocator.atom(node).as_ref()),
            SExp::Pair(first, rest) => {
                tree_hash_pair(&hash(allocator, first), &hash(allocator, rest))
            }
        }
    }
    let mut allocator = Allocator::new();
    let node = node_from_bytes(&mut allocator, &bytes).unwrap();
    assert_eq!(
        hex::encode(hash(&allocator, node)),
        "1c77d7d5efde60a7a1d2d27db6d746bc8e568aea1ef8586ca967a0d60b83cc36",
    );
    compare(&program, &generated_trees());
}

fn generated_trees() -> Vec<Tree> {
    let atoms = vec![
        atom(&[]),
        atom(&[0]),
        atom(&[1]),
        atom(&[0, 1]),
        atom(&[255]),
        atom(&[0x42; 32]),
        atom(&[0x80; 65]),
    ];
    let mut trees = atoms.clone();
    for first in &atoms {
        for rest in &atoms {
            trees.push(pair(first.clone(), rest.clone()));
        }
    }
    for index in 0..16 {
        trees.push(pair(trees[7 + index].clone(), atoms[index % 7].clone()));
        trees.push(pair(atoms[index % 7].clone(), trees[7 + index].clone()));
    }
    trees
}

#[test]
fn unsafe_guard_elisions_keep_shape_failures_and_lazy_branch_behavior() {
    let cases = [
        // A projection in only one branch is insufficient.
        choose(
            is_pair(arg(0)),
            choose(Expr::Bool { value: true }, arg(0), first(arg(0))),
            reject(),
        ),
        // Checking another argument does not establish this argument's shape.
        choose(is_pair(arg(0)), first(arg(1)), reject()),
        // Extracting atom bytes requires the opposite shape, not a pair.
        choose(
            is_pair(arg(0)),
            Expr::TreeAtom {
                value: Box::new(Expr::TreeAtomValue {
                    value: Box::new(arg(0)),
                }),
            },
            reject(),
        ),
        // The shape test itself can reject or inspect a different value.
        choose(is_pair(first(arg(0))), first(arg(0)), reject()),
        choose(
            is_pair(sequence(first(arg(1)), arg(0))),
            first(arg(0)),
            reject(),
        ),
        // Discarded fields and sequence operands still evaluate strictly.
        choose(
            is_pair(arg(0)),
            sequence(cons(first(arg(0)), first(arg(1))), tree("0001")),
            reject(),
        ),
        // An unselected projection must stay unselected.
        choose(
            is_pair(arg(0)),
            choose(is_pair(arg(1)), first(arg(0)), arg(0)),
            reject(),
        ),
        // Pair reconstruction must keep both partial projections.
        cons(first(arg(0)), rest(arg(0))),
    ];
    let trees = generated_trees();
    let mut environments = vec![atom(&[]), atom(&[1])];
    for first in &trees {
        for second in trees.iter().take(9) {
            environments.push(list(&[first.clone(), second.clone()]));
        }
    }
    environments.extend([
        list(&[atom(&[])]),
        list(&[atom(&[]), atom(&[]), atom(&[])]),
        pair(atom(&[1]), atom(&[2])),
    ]);
    for body in cases {
        compare(&program(body, 2, false), &environments);
    }
}

#[test]
fn generated_expression_and_tree_matrix_agrees_with_independent_checked_evaluation() {
    let values = [
        arg(0),
        tree(""),
        tree("0001"),
        first(arg(0)),
        rest(arg(0)),
        cons(arg(0), arg(0)),
        sequence(first(arg(0)), arg(0)),
    ];
    let conditions = [
        is_pair(arg(0)),
        Expr::Not {
            value: Box::new(is_pair(arg(0))),
        },
        Expr::Bool { value: true },
        Expr::Bool { value: false },
        is_pair(first(arg(0))),
        is_pair(cons(first(arg(0)), arg(0))),
        sequence(first(arg(0)), Expr::Bool { value: true }),
    ];
    let trees = generated_trees();
    let mut expressions = values.to_vec();
    for condition in conditions {
        for (index, value) in values.iter().enumerate() {
            let other = values[(index + 1) % values.len()].clone();
            expressions.extend([
                choose(condition.clone(), value.clone(), reject()),
                choose(condition.clone(), reject(), value.clone()),
                choose(condition.clone(), value.clone(), other),
            ]);
        }
    }
    for expression in expressions {
        compare(&program(expression, 1, true), &trees);
    }
}
