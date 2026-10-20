//! Preserve legacy formats; runtime uses v4 and typed wire pairs require v5.
use crate::{Environment, Expr, Function, Parameter, Program, Type};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::Error as _,
    ser::{Error as _, SerializeStruct},
};

#[derive(Serialize)]
struct FieldsRef<'a> {
    name: &'a str,
    parameters: &'a [Parameter],
    result: &'a Type,
    body: &'a Expr,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fields {
    name: String,
    parameters: Vec<Parameter>,
    result: Type,
    body: Expr,
}
#[derive(Serialize)]
struct EnvelopeRef<'a> {
    ir_version: u32,
    environment: Environment,
    program: FieldsRef<'a>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    ir_version: u32,
    environment: Environment,
    program: Fields,
}
#[derive(Serialize)]
struct RuntimeFieldsRef<'a> {
    name: &'a str,
    parameters: &'a [Parameter],
    result: &'a Type,
    body: &'a Expr,
    functions: &'a [Function],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeFields {
    name: String,
    parameters: Vec<Parameter>,
    result: Type,
    body: Expr,
    functions: Vec<Function>,
}
#[derive(Serialize)]
struct RuntimeEnvelopeRef<'a> {
    ir_version: u32,
    environment: Environment,
    program: RuntimeFieldsRef<'a>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeEnvelope {
    ir_version: u32,
    environment: Environment,
    program: RuntimeFields,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Wire {
    Runtime(RuntimeEnvelope),
    Versioned(Envelope),
    Strict(Fields),
}
impl Serialize for Program {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.validate().map_err(S::Error::custom)?;
        if self.uses_runtime() {
            return RuntimeEnvelopeRef {
                ir_version: if self.uses_pairs() { 5 } else { 4 },
                environment: self.environment,
                program: RuntimeFieldsRef {
                    name: &self.name,
                    parameters: &self.parameters,
                    result: &self.result,
                    body: &self.body,
                    functions: &self.functions,
                },
            }
            .serialize(serializer);
        }
        let fields = FieldsRef {
            name: &self.name,
            parameters: &self.parameters,
            result: &self.result,
            body: &self.body,
        };
        match self.environment {
            Environment::Strict => fields.serialize(serializer),
            Environment::Ignored | Environment::Raw => EnvelopeRef {
                ir_version: if self.environment == Environment::Raw {
                    3
                } else {
                    2
                },
                environment: self.environment,
                program: fields,
            }
            .serialize(serializer),
        }
    }
}
impl<'de> Deserialize<'de> for Program {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (program, runtime, pair_capability) = match Wire::deserialize(deserializer)? {
            Wire::Runtime(envelope) => {
                if !matches!(envelope.ir_version, 4 | 5) {
                    return Err(D::Error::custom(
                        "function metadata requires IR version 4 or 5",
                    ));
                }
                let pair_capability = envelope.ir_version == 5;
                let fields = envelope.program;
                (
                    Self {
                        name: fields.name,
                        parameters: fields.parameters,
                        result: fields.result,
                        body: fields.body,
                        environment: envelope.environment,
                        functions: fields.functions,
                    },
                    true,
                    pair_capability,
                )
            }
            Wire::Strict(fields) => (
                Self {
                    name: fields.name,
                    parameters: fields.parameters,
                    result: fields.result,
                    body: fields.body,
                    environment: Environment::Strict,
                    functions: vec![],
                },
                false,
                false,
            ),
            Wire::Versioned(envelope) => {
                if !matches!(
                    (envelope.ir_version, envelope.environment),
                    (2, Environment::Ignored) | (3, Environment::Raw)
                ) {
                    return Err(D::Error::custom(
                        "unsupported IR version or environment policy",
                    ));
                }
                let fields = envelope.program;
                (
                    Self {
                        name: fields.name,
                        parameters: fields.parameters,
                        result: fields.result,
                        body: fields.body,
                        environment: envelope.environment,
                        functions: vec![],
                    },
                    false,
                    false,
                )
            }
        };
        if program.uses_pairs() != pair_capability {
            return Err(D::Error::custom(
                "typed pair capability requires IR version 5, and version 5 requires typed pairs",
            ));
        }
        if pair_capability {
            program.validate().map_err(D::Error::custom)?;
        }
        if !runtime && program.uses_runtime() {
            return Err(D::Error::custom("runtime expressions require IR version 4"));
        }
        if runtime && !program.uses_runtime() {
            return Err(D::Error::custom(
                "IR version 4 requires runtime expressions or a nonempty function table",
            ));
        }
        program.validate_environment().map_err(D::Error::custom)?;
        Ok(program)
    }
}

// Keep recursive expression transport in small serializer frames. Deriving
// serialization for the expanded enum overflowed the existing 193-deep
// default-stack test; keep the dispatcher separate from payload serialization.
// These helpers borrow payloads and retain the internally tagged struct shape.
fn node0<S: Serializer>(serializer: S, op: &'static str) -> Result<S::Ok, S::Error> {
    let mut state = serializer.serialize_struct("Expr", 1)?;
    state.serialize_field("op", op)?;
    state.end()
}
fn node1<S: Serializer, A: Serialize + ?Sized>(
    serializer: S,
    op: &'static str,
    a: (&'static str, &A),
) -> Result<S::Ok, S::Error> {
    let mut state = serializer.serialize_struct("Expr", 2)?;
    state.serialize_field("op", op)?;
    state.serialize_field(a.0, a.1)?;
    state.end()
}
fn node2<S: Serializer, A: Serialize + ?Sized, B: Serialize + ?Sized>(
    serializer: S,
    op: &'static str,
    a: (&'static str, &A),
    b: (&'static str, &B),
) -> Result<S::Ok, S::Error> {
    let mut state = serializer.serialize_struct("Expr", 3)?;
    state.serialize_field("op", op)?;
    state.serialize_field(a.0, a.1)?;
    state.serialize_field(b.0, b.1)?;
    state.end()
}
fn node3<S: Serializer, A: Serialize + ?Sized, B: Serialize + ?Sized, C: Serialize + ?Sized>(
    serializer: S,
    op: &'static str,
    a: (&'static str, &A),
    b: (&'static str, &B),
    c: (&'static str, &C),
) -> Result<S::Ok, S::Error> {
    let mut state = serializer.serialize_struct("Expr", 4)?;
    state.serialize_field("op", op)?;
    state.serialize_field(a.0, a.1)?;
    state.serialize_field(b.0, b.1)?;
    state.serialize_field(c.0, c.1)?;
    state.end()
}
fn node4<
    S: Serializer,
    A: Serialize + ?Sized,
    B: Serialize + ?Sized,
    C: Serialize + ?Sized,
    D: Serialize + ?Sized,
>(
    serializer: S,
    op: &'static str,
    a: (&'static str, &A),
    b: (&'static str, &B),
    c: (&'static str, &C),
    d: (&'static str, &D),
) -> Result<S::Ok, S::Error> {
    let mut state = serializer.serialize_struct("Expr", 5)?;
    state.serialize_field("op", op)?;
    state.serialize_field(a.0, a.1)?;
    state.serialize_field(b.0, b.1)?;
    state.serialize_field(c.0, c.1)?;
    state.serialize_field(d.0, d.1)?;
    state.end()
}
impl Serialize for Expr {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Expr::Pair { first, rest } => {
                node2(serializer, "pair", ("first", first), ("rest", rest))
            }
            Expr::PairFirst { value } => node1(serializer, "pair_first", ("value", value)),
            Expr::PairRest { value } => node1(serializer, "pair_rest", ("value", value)),
            Expr::Sha256 { parts } => node1(serializer, "sha256", ("parts", parts)),
            Expr::Apply {
                program,
                environment,
            } => node2(
                serializer,
                "apply",
                ("program", program),
                ("environment", environment),
            ),
            Expr::Call {
                function_id,
                arguments,
            } => node2(
                serializer,
                "call",
                ("function_id", function_id),
                ("arguments", arguments),
            ),
            Expr::Let {
                local_id,
                ty,
                value,
                body,
            } => node4(
                serializer,
                "let",
                ("local_id", local_id),
                ("ty", ty),
                ("value", value),
                ("body", body),
            ),
            Expr::Local { local_id } => node1(serializer, "local", ("local_id", local_id)),
            Expr::Atom { hex } => node1(serializer, "atom", ("hex", hex)),
            Expr::AtomIsEmpty { value } => node1(serializer, "atom_is_empty", ("value", value)),
            Expr::TreeAtom { value } => node1(serializer, "tree_atom", ("value", value)),
            Expr::TreeAtomValue { value } => node1(serializer, "tree_atom_value", ("value", value)),
            Expr::TreePair { first, rest } => {
                node2(serializer, "tree_pair", ("first", first), ("rest", rest))
            }
            Expr::TreeIsPair { value } => node1(serializer, "tree_is_pair", ("value", value)),
            Expr::TreeFirst { value } => node1(serializer, "tree_first", ("value", value)),
            Expr::TreeRest { value } => node1(serializer, "tree_rest", ("value", value)),
            Expr::Unit => node0(serializer, "unit"),
            Expr::Bool { value } => node1(serializer, "bool", ("value", value)),
            Expr::Int { value, ty } => node2(serializer, "int", ("value", value), ("ty", ty)),
            Expr::Bytes { hex } => node1(serializer, "bytes", ("hex", hex)),
            Expr::Arg { index } => node1(serializer, "arg", ("index", index)),
            Expr::Tuple { items } => node1(serializer, "tuple", ("items", items)),
            Expr::Binary {
                kind,
                left,
                right,
                ty,
            } => node4(
                serializer,
                "binary",
                ("kind", kind),
                ("left", left),
                ("right", right),
                ("ty", ty),
            ),
            Expr::Not { value } => node1(serializer, "not", ("value", value)),
            Expr::If {
                condition,
                then_value,
                else_value,
            } => node3(
                serializer,
                "if",
                ("condition", condition),
                ("then_value", then_value),
                ("else_value", else_value),
            ),
            Expr::Reject { message } => node1(serializer, "reject", ("message", message)),
            Expr::Field { value, index } => {
                node2(serializer, "field", ("value", value), ("index", index))
            }
            Expr::Sequence { first, then_value } => node2(
                serializer,
                "sequence",
                ("first", first),
                ("then_value", then_value),
            ),
        }
    }
}
