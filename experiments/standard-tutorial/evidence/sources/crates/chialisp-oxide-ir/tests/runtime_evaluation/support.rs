use chialisp_oxide_ir::{BinaryOp, Environment, Expr, Function, Parameter, Program, Type, compile};
use chialisp_oxide_vm::{FailureCode, FailureKind, classify_failure};
use clvmr::{
    Allocator, ChiaDialect, MEMPOOL_MODE, run_program,
    serde::{node_from_bytes, node_to_bytes},
};

pub const LIMIT: u64 = 100_000;
pub const FINITE_LIMIT: u64 = 100_000_000;
pub fn u8_type() -> Type {
    Type::Int {
        bits: 8,
        signed: false,
    }
}
pub fn int(value: u16) -> Expr {
    Expr::Int {
        value: value.to_string(),
        ty: u8_type(),
    }
}
pub fn arg(index: usize) -> Expr {
    Expr::Arg { index }
}
pub fn local(local_id: usize) -> Expr {
    Expr::Local { local_id }
}
pub fn call(function_id: usize, arguments: Vec<Expr>) -> Expr {
    Expr::Call {
        function_id,
        arguments,
    }
}
pub fn sequence(first: Expr, then_value: Expr) -> Expr {
    Expr::Sequence {
        first: Box::new(first),
        then_value: Box::new(then_value),
    }
}
pub fn binding(local_id: usize, ty: Type, value: Expr, body: Expr) -> Expr {
    Expr::Let {
        local_id,
        ty,
        value: Box::new(value),
        body: Box::new(body),
    }
}
pub fn reject(ty: &Type) -> Expr {
    let fallback = if *ty == Type::Tree {
        tree_atom("")
    } else {
        int(1)
    };
    sequence(
        Expr::Reject {
            message: "explicit first failure".into(),
        },
        fallback,
    )
}
pub fn branch(condition: Expr, then_value: Expr, else_value: Expr) -> Expr {
    Expr::If {
        condition: Box::new(condition),
        then_value: Box::new(then_value),
        else_value: Box::new(else_value),
    }
}
pub fn add(left: Expr, right: Expr) -> Expr {
    Expr::Binary {
        kind: BinaryOp::Add,
        left: Box::new(left),
        right: Box::new(right),
        ty: u8_type(),
    }
}
pub fn tree_atom(hex: &str) -> Expr {
    Expr::TreeAtom {
        value: Box::new(Expr::Atom { hex: hex.into() }),
    }
}
pub fn first(value: Expr) -> Expr {
    Expr::TreeFirst {
        value: Box::new(value),
    }
}
pub fn rest(value: Expr) -> Expr {
    Expr::TreeRest {
        value: Box::new(value),
    }
}
pub fn is_pair(value: Expr) -> Expr {
    Expr::TreeIsPair {
        value: Box::new(value),
    }
}
pub fn field(value: Expr, index: usize) -> Expr {
    Expr::Field {
        value: Box::new(value),
        index,
    }
}
pub fn parameters(types: Vec<Type>) -> Vec<Parameter> {
    types
        .into_iter()
        .enumerate()
        .map(|(i, ty)| Parameter {
            name: format!("argument_{i}"),
            ty,
        })
        .collect()
}
pub fn function(id: usize, types: Vec<Type>, result: Type, body: Expr) -> Function {
    Function {
        id,
        name: format!("function_{id}"),
        parameters: parameters(types),
        result,
        body,
    }
}
pub fn program(
    environment: Environment,
    types: Vec<Type>,
    result: Type,
    body: Expr,
    functions: Vec<Function>,
) -> Program {
    Program {
        name: "runtime-evaluation".into(),
        environment,
        parameters: parameters(types),
        result,
        body,
        functions,
    }
}
pub fn spin(ty: &Type) -> Function {
    function(0, vec![], ty.clone(), call(0, vec![]))
}
pub fn rightmost(id: usize, next: usize) -> Function {
    function(
        id,
        vec![Type::Tree],
        Type::Tree,
        branch(is_pair(arg(0)), call(next, vec![rest(arg(0))]), arg(0)),
    )
}
pub fn modes(source: &Program) -> [(bool, Vec<u8>); 2] {
    source
        .validate()
        .expect("test fixture must be valid typed IR");
    [false, true].map(|optimize| {
        (
            optimize,
            compile(source, optimize).expect("compile runtime fixture"),
        )
    })
}

// Small independent serialized-tree construction. No project codec, native
// interpreter or VM assembler supplies the expected outputs.
pub fn atom(bytes: &[u8]) -> Vec<u8> {
    match bytes {
        [] => vec![0x80],
        [value] if *value < 0x80 => vec![*value],
        _ => {
            assert!(bytes.len() < 64);
            let mut wire = vec![0x80 | bytes.len() as u8];
            wire.extend(bytes);
            wire
        }
    }
}
pub fn pair(first: &[u8], rest: &[u8]) -> Vec<u8> {
    let mut result = vec![0xff];
    result.extend(first);
    result.extend(rest);
    result
}
pub fn list(items: &[Vec<u8>]) -> Vec<u8> {
    items
        .iter()
        .rev()
        .fold(atom(&[]), |rest, first| pair(first, &rest))
}
pub fn uint(value: u16) -> Vec<u8> {
    if value == 0 {
        atom(&[])
    } else if value < 128 {
        atom(&[value as u8])
    } else if value < 256 {
        atom(&[0, value as u8])
    } else {
        atom(&value.to_be_bytes())
    }
}
pub fn right_spine(depth: usize, leaf: &[u8]) -> Vec<u8> {
    let mut result = leaf.to_vec();
    for _ in 0..depth {
        result = pair(&atom(&[0]), &result);
    }
    result
}
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Accepted { bytes: Vec<u8>, cost: u64 },
    Failed(FailureCode),
}
pub fn run(code: &[u8], environment: &[u8], limit: u64) -> Outcome {
    let mut allocator = Allocator::new();
    let root = node_from_bytes(&mut allocator, code).unwrap();
    let environment = node_from_bytes(&mut allocator, environment).unwrap();
    match run_program(
        &mut allocator,
        &ChiaDialect::new(MEMPOOL_MODE),
        root,
        environment,
        limit,
    ) {
        Ok(result) => Outcome::Accepted {
            bytes: node_to_bytes(&allocator, result.1).unwrap(),
            cost: result.0,
        },
        Err(error) => {
            let failure = classify_failure(&error);
            assert!(
                failure.is_program_rejection() || failure.code() == FailureCode::CostExceeded,
                "unexpected VM failure {failure:?}: {error:?}"
            );
            Outcome::Failed(failure.code())
        }
    }
}
pub fn accepted(outcome: Outcome, expected: &[u8]) -> u64 {
    match outcome {
        Outcome::Accepted { bytes, cost } => {
            assert_eq!(bytes, expected);
            assert!(cost > 0);
            cost
        }
        other => panic!("expected successful exact output, got {other:?}"),
    }
}
pub fn failed(outcome: Outcome, code: FailureCode) {
    assert_eq!(outcome, Outcome::Failed(code));
}

/// External ABI rejection may fail a semantic opcode before an explicit Raise.
/// Resource/infrastructure failures still cannot count as an invalid-input check.
pub fn abi_rejected(outcome: Outcome) {
    match outcome {
        Outcome::Failed(code) => assert_eq!(code.kind(), FailureKind::Program),
        other => panic!("malformed ABI input was not rejected: {other:?}"),
    }
}
