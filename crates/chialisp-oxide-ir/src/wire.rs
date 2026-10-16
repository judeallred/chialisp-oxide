//! Preserve the strict v1 wire format; opt-in ABI changes need a versioned envelope.
use crate::{Environment, Expr, Parameter, Program, Type};
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
#[derive(Deserialize)]
#[serde(untagged)]
enum Wire {
    Versioned(Envelope),
    Strict(Fields),
}
impl Serialize for Program {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.validate_environment().map_err(S::Error::custom)?;
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
        let (fields, environment) = match Wire::deserialize(deserializer)? {
            Wire::Strict(fields) => (fields, Environment::Strict),
            Wire::Versioned(envelope) => {
                if !matches!(
                    (envelope.ir_version, envelope.environment),
                    (2, Environment::Ignored) | (3, Environment::Raw)
                ) {
                    return Err(D::Error::custom(
                        "unsupported IR version or environment policy",
                    ));
                }
                (envelope.program, envelope.environment)
            }
        };
        let program = Self {
            name: fields.name,
            parameters: fields.parameters,
            result: fields.result,
            body: fields.body,
            environment,
        };
        program.validate_environment().map_err(D::Error::custom)?;
        Ok(program)
    }
}
