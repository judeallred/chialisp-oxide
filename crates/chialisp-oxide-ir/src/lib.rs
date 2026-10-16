//! Pure, typed expression IR between checked Rust MIR and Rue's CLVM backend.
mod lower;
mod validate;
mod wire;
pub use lower::compile;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Type {
    Bool,
    Int { bits: u16, signed: bool },
    Bytes { len: usize },
    Tuple { items: Vec<Type> },
    Unit,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub ty: Type,
}
/// How the entry point consumes the untrusted CLVM environment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Environment {
    /// Decode an exact, proper list of typed Rust arguments, including arity zero.
    #[default]
    Strict,
    /// A genuinely nullary function deliberately accepts every CLVM tree.
    Ignored,
}
impl Environment {
    pub fn is_strict(&self) -> bool {
        *self == Self::Strict
    }
}
#[derive(Debug, Clone)]
pub struct Program {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub result: Type,
    pub body: Expr,
    pub environment: Environment,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BinaryOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Bitand,
    Bitor,
    Bitxor,
    Shl,
    Shr,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Expr {
    Unit,
    Bool {
        value: bool,
    },
    Int {
        value: String,
        ty: Type,
    },
    Bytes {
        hex: String,
    },
    Arg {
        index: usize,
    },
    Tuple {
        items: Vec<Expr>,
    },
    Binary {
        kind: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
        ty: Type,
    },
    Not {
        value: Box<Expr>,
    },
    If {
        condition: Box<Expr>,
        then_value: Box<Expr>,
        else_value: Box<Expr>,
    },
    Reject {
        message: String,
    },
    Field {
        value: Box<Expr>,
        index: usize,
    },
    Sequence {
        first: Box<Expr>,
        then_value: Box<Expr>,
    },
}
