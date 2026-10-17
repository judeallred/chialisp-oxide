//! Pure, typed expression IR between checked Rust MIR and Rue's CLVM backend.
mod lower;
mod optimize;
mod validate;
mod wire;
pub use lower::compile;
pub use optimize::optimize;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Type {
    /// An arbitrary immutable CLVM atom; all bytes are significant.
    Atom,
    /// An arbitrary immutable CLVM atom/pair tree.
    Tree,
    Bool,
    Int {
        bits: u16,
        signed: bool,
    },
    Bytes {
        len: usize,
    },
    Tuple {
        items: Vec<Type>,
    },
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
    /// Pass the complete CLVM environment to one tree parameter without decoding.
    Raw,
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
    /// A closed, compiler-owned function table; IDs equal their table indices.
    pub functions: Vec<Function>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Function {
    pub id: usize,
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub result: Type,
    pub body: Expr,
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
    /// SHA256 of ordered atom payloads. Strict operand binding requires v4.
    Sha256 {
        parts: Vec<Expr>,
    },
    Call {
        function_id: usize,
        arguments: Vec<Expr>,
    },
    Let {
        local_id: usize,
        ty: Type,
        value: Box<Expr>,
        body: Box<Expr>,
    },
    Local {
        local_id: usize,
    },
    Atom {
        hex: String,
    },
    /// Test atom byte length, without integer coercion or truthiness folding.
    AtomIsEmpty {
        value: Box<Expr>,
    },
    TreeAtom {
        value: Box<Expr>,
    },
    TreeAtomValue {
        value: Box<Expr>,
    },
    TreePair {
        first: Box<Expr>,
        rest: Box<Expr>,
    },
    TreeIsPair {
        value: Box<Expr>,
    },
    TreeFirst {
        value: Box<Expr>,
    },
    TreeRest {
        value: Box<Expr>,
    },
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
