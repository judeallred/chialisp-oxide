//! Preserve legacy entry formats; runtime constructs require an explicit v4 envelope.
use crate::{Environment, Expr, Function, Parameter, Program, Type};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _, ser::Error as _};

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
        self.validate_environment().map_err(S::Error::custom)?;
        if self.uses_runtime() {
            return RuntimeEnvelopeRef {
                ir_version: 4,
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
        let (program, runtime) = match Wire::deserialize(deserializer)? {
            Wire::Runtime(envelope) => {
                if envelope.ir_version != 4 {
                    return Err(D::Error::custom("function metadata requires IR version 4"));
                }
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
                )
            }
        };
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
